mod capture;
mod mcap_writer;
mod processor;
mod uploader;
mod ws_server;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use tokio::sync::mpsc;
use tokio::time;
use tracing::info;

use mcap_writer::RollingWriter;
use processor::{KeyboardProcessor, MousePosition, MouseScroll, RawEvent};
use uploader::S3Uploader;
use ws_server::Channels;

// Single-threaded runtime: 100 % I/O-bound work; no need for extra threads.
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // --- Configuration from environment (set by NixOS module or shell) ------
    let port: u16 = std::env::var("KMMON_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8765);

    let mcap_dir = PathBuf::from(
        std::env::var("KMMON_MCAP_DIR").unwrap_or_else(|_| "/tmp".into()),
    );

    let roll_secs: u64 = std::env::var("KMMON_ROLL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3600);

    let retention_secs: u64 = std::env::var("KMMON_RETENTION_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(7 * 24 * 3600);

    // --- Foxglove MCAP metadata (indexed in place by the data platform) -----
    let foxglove_metadata = build_foxglove_metadata();

    // --- S3 uploader (optional — enabled by KMMON_S3_BUCKET) ----------------
    let uploader: Option<Arc<S3Uploader>> =
        S3Uploader::from_env()?.map(Arc::new);

    // --- Foxglove sinks ------------------------------------------------------
    let ws = ws_server::WsServer::start("0.0.0.0", port).await?;
    info!("WebSocket server listening on ws://0.0.0.0:{port}");

    let mut recorder = RollingWriter::new(
        mcap_dir.clone(),
        Duration::from_secs(roll_secs),
        Duration::from_secs(retention_secs),
        foxglove_metadata,
    )?;

    // --- Channels ------------------------------------------------------------
    let channels = Arc::new(ws_server::create_channels()?);

    // --- Capture -------------------------------------------------------------
    let (tx, rx) = mpsc::channel::<RawEvent>(4096);
    let n = capture::spawn_capture_tasks(tx).await;
    info!("Spawned capture tasks for {n} input device(s)");

    // --- Processing loop -----------------------------------------------------
    let channels_for_loop = channels.clone();
    let loop_handle = tokio::spawn(run_event_loop(rx, channels_for_loop));

    // Roll timer: check once per minute; actual roll fires at roll_secs boundary.
    let mut roll_check = time::interval(Duration::from_secs(60));
    roll_check.tick().await; // consume the immediate first tick

    // Wait for shutdown or roll ticks.
    loop {
        tokio::select! {
            _ = roll_check.tick() => {
                if let Ok(Some(completed)) = recorder.maybe_roll() {
                    // Upload the just-closed file in the background.
                    if let Some(up) = &uploader {
                        up.upload_detached(completed);
                    }
                    // Prune files that have aged past the retention window.
                    if let Err(e) = recorder.cleanup_old_files() {
                        tracing::warn!("Cleanup failed: {e}");
                    }
                }
            }
            _ = wait_for_shutdown() => break,
        }
    }

    info!("Shutting down…");
    loop_handle.abort();

    // Roll one final time so the last segment is a complete MCAP file.
    let final_file = recorder.roll()?;
    if let Some(up) = &uploader {
        // Upload synchronously on the way out so we don't lose data.
        if let Err(e) = up.upload(&final_file).await {
            tracing::warn!("Final S3 upload failed: {e:#}");
        }
    }
    recorder.close()?;

    ws.stop();
    info!("Done. Last recording: {}", final_file.display());

    Ok(())
}

/// Returns when either SIGINT (Ctrl-C) or SIGTERM (systemd stop) is received.
async fn wait_for_shutdown() -> Result<()> {
    use tokio::signal::unix::{signal, SignalKind};
    let mut sigterm = signal(SignalKind::terminate())?;
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = sigterm.recv() => {}
    }
    Ok(())
}

/// Reads raw events from capture tasks and publishes processed messages to
/// foxglove channels.
async fn run_event_loop(mut rx: mpsc::Receiver<RawEvent>, channels: Arc<Channels>) {
    let mut mouse_x: i32 = 0;
    let mut mouse_y: i32 = 0;

    // Lazy mouse deadline: None → no wakeups while the mouse is idle.
    let mut mouse_deadline: Option<time::Instant> = None;

    let kb_proc = Arc::new(Mutex::new(KeyboardProcessor::new()));
    let mut kb_timer = time::interval(Duration::from_secs(1));
    let mut kb_was_active = false;

    loop {
        tokio::select! {
            Some(event) = rx.recv() => {
                match event {
                    RawEvent::MouseRelMove { dx, dy } => {
                        mouse_x += dx;
                        mouse_y += dy;
                        mark_mouse_dirty(&mut mouse_deadline);
                    }
                    RawEvent::MouseAbsX(x) => {
                        mouse_x = x;
                        mark_mouse_dirty(&mut mouse_deadline);
                    }
                    RawEvent::MouseAbsY(y) => {
                        mouse_y = y;
                        mark_mouse_dirty(&mut mouse_deadline);
                    }
                    RawEvent::Scroll { dx, dy } => {
                        publish(&channels.mouse_scroll, &MouseScroll { dx, dy });
                    }
                    RawEvent::Keystroke => {
                        kb_proc.lock().unwrap().record_keystroke();
                    }
                }
            }

            // Mouse position: emitted once, 50 ms after the first move in a
            // burst.  Branch disabled (zero wakeups) when the mouse is still.
            _ = time::sleep_until(mouse_deadline.unwrap_or(time::Instant::now())),
                if mouse_deadline.is_some() =>
            {
                publish(
                    &channels.mouse_position,
                    &MousePosition { x: mouse_x, y: mouse_y },
                );
                mouse_deadline = None;
            }

            _ = kb_timer.tick() => {
                let activity = kb_proc.lock().unwrap().activity();
                // Silent while continuously inactive; emit on state change.
                if activity.active || kb_was_active {
                    publish(&channels.keyboard_activity, &activity);
                }
                kb_was_active = activity.active;
            }
        }
    }
}

#[inline]
fn mark_mouse_dirty(deadline: &mut Option<time::Instant>) {
    if deadline.is_none() {
        *deadline = Some(time::Instant::now() + Duration::from_millis(50));
    }
}

fn publish(channel: &foxglove::RawChannel, msg: &impl serde::Serialize) {
    if let Ok(bytes) = serde_json::to_vec(msg) {
        channel.log(&bytes);
    }
}

/// Builds the `"foxglove"` MCAP Metadata record from environment variables.
/// Returns an empty map when `KMMON_FOXGLOVE_PROJECT_ID` is unset (no record
/// will be written).
fn build_foxglove_metadata() -> BTreeMap<String, String> {
    let project_id = match std::env::var("KMMON_FOXGLOVE_PROJECT_ID") {
        Ok(id) => id,
        Err(_) => return BTreeMap::new(),
    };

    let hostname = gethostname();

    let mut m = BTreeMap::new();
    m.insert("projectId".into(), project_id);
    m.insert(
        "deviceId".into(),
        std::env::var("KMMON_FOXGLOVE_DEVICE_ID").unwrap_or_else(|_| hostname.clone()),
    );
    m.insert(
        "deviceName".into(),
        std::env::var("KMMON_FOXGLOVE_DEVICE_NAME").unwrap_or(hostname),
    );
    m
}

fn gethostname() -> String {
    nix::unistd::gethostname()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".into())
}
