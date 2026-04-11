use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use foxglove::McapWriter;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Single-file writer
// ---------------------------------------------------------------------------

/// Thin wrapper around `foxglove::McapWriterHandle` that tracks the output path.
///
/// The underlying sink registers itself with the global foxglove context on
/// creation, so all `Channel::log` calls automatically flow into the MCAP file
/// until `close()` is called.
pub struct McapFileWriter {
    handle: foxglove::McapWriterHandle<BufWriter<File>>,
    path: PathBuf,
}

impl McapFileWriter {
    /// Creates a new MCAP file at `path`. Fails if the file already exists.
    pub fn create(path: &Path) -> Result<Self> {
        let handle = McapWriter::new().create_new_buffered_file(path)?;
        Ok(Self {
            handle,
            path: path.to_path_buf(),
        })
    }

    /// Returns the path to the MCAP file being written.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Writes an MCAP Metadata record to the file.
    ///
    /// Call this immediately after `create()` to embed file-level metadata
    /// (e.g. project ID) before any messages are written.
    pub fn write_metadata(
        &self,
        name: &str,
        metadata: std::collections::BTreeMap<String, String>,
    ) -> Result<()> {
        self.handle.write_metadata(name, metadata)?;
        Ok(())
    }

    /// Flushes and closes the MCAP file, finalising all channel index entries.
    pub fn close(self) -> Result<()> {
        self.handle.close()?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Rolling writer
// ---------------------------------------------------------------------------

/// Manages a sequence of fixed-duration MCAP files in a directory.
///
/// When `maybe_roll()` fires it:
///   1. Opens the **new** file first (both old and new sinks briefly active → no gap)
///   2. Closes the old file
///   3. Returns the completed path for upload / bookkeeping
///
/// `cleanup_old_files()` deletes files older than the configured retention window.
pub struct RollingWriter {
    current: McapFileWriter,
    mcap_dir: PathBuf,
    roll_interval: Duration,
    retention: Duration,
    /// When the *current* file was opened.
    opened_at: Instant,
    /// Foxglove metadata written to every new file (empty → no record).
    foxglove_metadata: BTreeMap<String, String>,
}

impl RollingWriter {
    pub fn new(
        mcap_dir: PathBuf,
        roll_interval: Duration,
        retention: Duration,
        foxglove_metadata: BTreeMap<String, String>,
    ) -> Result<Self> {
        let path = timestamped_path(&mcap_dir)?;
        info!("Recording to {}", path.display());
        let current = McapFileWriter::create(&path)?;
        if !foxglove_metadata.is_empty() {
            current.write_metadata("foxglove", foxglove_metadata.clone())?;
        }
        Ok(Self {
            current,
            mcap_dir,
            roll_interval,
            retention,
            opened_at: Instant::now(),
            foxglove_metadata,
        })
    }

    /// Rolls to a new file if the current one has exceeded `roll_interval`.
    /// Returns `Some(completed_path)` when a roll occurred.
    pub fn maybe_roll(&mut self) -> Result<Option<PathBuf>> {
        if self.opened_at.elapsed() < self.roll_interval {
            return Ok(None);
        }
        Ok(Some(self.roll()?))
    }

    /// Forces an immediate roll. Returns the path of the file that was closed.
    pub fn roll(&mut self) -> Result<PathBuf> {
        let completed = self.current.path().to_path_buf();

        // Open the new sink BEFORE closing the old one so there is no window
        // where channel messages have nowhere to go.
        let new_path = timestamped_path(&self.mcap_dir)?;
        info!("Rolling MCAP → {}", new_path.display());
        let new_writer = McapFileWriter::create(&new_path)?;
        if !self.foxglove_metadata.is_empty() {
            new_writer.write_metadata("foxglove", self.foxglove_metadata.clone())?;
        }
        let old_writer = std::mem::replace(&mut self.current, new_writer);
        self.opened_at = Instant::now();

        old_writer.close()?;
        Ok(completed)
    }

    /// Returns the path of the currently-open MCAP file.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn current_path(&self) -> &Path {
        self.current.path()
    }

    /// Deletes MCAP files in `mcap_dir` whose mtime is older than `retention`,
    /// skipping the currently-open file.
    pub fn cleanup_old_files(&self) -> Result<()> {
        let cutoff = SystemTime::now()
            .checked_sub(self.retention)
            .expect("retention duration is too large");

        let current = self.current.path();
        for entry in std::fs::read_dir(&self.mcap_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("mcap") {
                continue;
            }
            if path == current {
                continue;
            }
            match entry.metadata().and_then(|m| m.modified()) {
                Ok(mtime) if mtime < cutoff => {
                    info!("Deleting expired recording: {}", path.display());
                    if let Err(e) = std::fs::remove_file(&path) {
                        warn!("Could not delete {}: {e}", path.display());
                    }
                }
                Err(e) => warn!("Could not stat {}: {e}", path.display()),
                _ => {}
            }
        }
        Ok(())
    }

    /// Closes the current file without rolling. Call on shutdown.
    pub fn close(self) -> Result<()> {
        self.current.close()
    }
}

fn timestamped_path(dir: &Path) -> Result<PathBuf> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs();
    // Happy path: the per-second name is unique (normal for hourly rolls).
    let base = dir.join(format!("kmmon-{ts}.mcap"));
    if !base.exists() {
        return Ok(base);
    }
    // Within-the-same-second collision (only happens in tests / rapid rolls).
    for seq in 1u32..=999 {
        let path = dir.join(format!("kmmon-{ts}-{seq:03}.mcap"));
        if !path.exists() {
            return Ok(path);
        }
    }
    anyhow::bail!("Could not generate a unique MCAP path in {}", dir.display())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_close_produces_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.mcap");

        let writer = McapFileWriter::create(&path).unwrap();
        assert_eq!(writer.path(), path);
        writer.close().unwrap();

        assert!(path.exists(), "MCAP file should exist after close()");
    }

    #[test]
    fn create_fails_if_file_exists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("exists.mcap");
        std::fs::write(&path, b"").unwrap();
        assert!(McapFileWriter::create(&path).is_err());
    }

    #[test]
    fn rolling_writer_roll_produces_two_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut writer = RollingWriter::new(
            dir.path().to_path_buf(),
            Duration::from_secs(3600),
            Duration::from_secs(604_800),
            Default::default(),
        )
        .unwrap();

        let first = writer.current_path().to_path_buf();
        let completed = writer.roll().unwrap();
        let second = writer.current_path().to_path_buf();

        assert_eq!(completed, first);
        assert_ne!(first, second);
        assert!(first.exists());
        assert!(second.exists());

        writer.close().unwrap();
    }

    /// Reads all MCAP metadata records named `name` from the file at `path`.
    fn read_mcap_metadata(
        path: &Path,
        name: &str,
    ) -> Vec<std::collections::BTreeMap<String, String>> {
        let data = std::fs::read(path).unwrap();
        let summary = mcap::Summary::read(&data).unwrap().unwrap();
        let mut out = Vec::new();
        for index in &summary.metadata_indexes {
            if index.name == name {
                let record = mcap::read::metadata(&data, index).unwrap();
                out.push(record.metadata);
            }
        }
        out
    }

    #[test]
    fn rolling_writer_writes_foxglove_metadata_to_initial_file() {
        let dir = tempfile::tempdir().unwrap();
        let metadata = std::collections::BTreeMap::from([
            ("projectId".into(), "prj_test".into()),
            ("deviceId".into(), "host-1".into()),
        ]);

        let writer = RollingWriter::new(
            dir.path().to_path_buf(),
            Duration::from_secs(3600),
            Duration::from_secs(604_800),
            metadata,
        )
        .unwrap();

        let path = writer.current_path().to_path_buf();
        writer.close().unwrap();

        let records = read_mcap_metadata(&path, "foxglove");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["projectId"], "prj_test");
        assert_eq!(records[0]["deviceId"], "host-1");
    }

    #[test]
    fn rolling_writer_writes_foxglove_metadata_after_roll() {
        let dir = tempfile::tempdir().unwrap();
        let metadata = std::collections::BTreeMap::from([
            ("projectId".into(), "prj_roll".into()),
        ]);

        let mut writer = RollingWriter::new(
            dir.path().to_path_buf(),
            Duration::from_secs(3600),
            Duration::from_secs(604_800),
            metadata,
        )
        .unwrap();

        let first = writer.current_path().to_path_buf();
        writer.roll().unwrap();
        let second = writer.current_path().to_path_buf();
        writer.close().unwrap();

        // Both files should contain the metadata record.
        for path in [&first, &second] {
            let records = read_mcap_metadata(path, "foxglove");
            assert_eq!(records.len(), 1, "missing metadata in {}", path.display());
            assert_eq!(records[0]["projectId"], "prj_roll");
        }
    }

    #[test]
    fn rolling_writer_no_metadata_when_empty() {
        let dir = tempfile::tempdir().unwrap();
        let metadata = std::collections::BTreeMap::new();

        let writer = RollingWriter::new(
            dir.path().to_path_buf(),
            Duration::from_secs(3600),
            Duration::from_secs(604_800),
            metadata,
        )
        .unwrap();

        let path = writer.current_path().to_path_buf();
        writer.close().unwrap();

        let records = read_mcap_metadata(&path, "foxglove");
        assert!(records.is_empty(), "no metadata record should be written when map is empty");
    }

    #[test]
    fn cleanup_deletes_old_files() {
        let dir = tempfile::tempdir().unwrap();
        // Create a stale file by touching it and backdating its mtime.
        let stale = dir.path().join("kmmon-0000000000.mcap");
        std::fs::write(&stale, b"stale").unwrap();
        // Set mtime to epoch (definitely older than any retention window).
        let epoch = filetime::FileTime::zero();
        filetime::set_file_mtime(&stale, epoch).unwrap();

        let writer = RollingWriter::new(
            dir.path().to_path_buf(),
            Duration::from_secs(3600),
            Duration::from_secs(604_800),
            Default::default(),
        )
        .unwrap();

        writer.cleanup_old_files().unwrap();

        assert!(!stale.exists(), "stale file should have been deleted");
        writer.close().unwrap();
    }
}
