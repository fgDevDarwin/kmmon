use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use object_store::aws::AmazonS3Builder;
use object_store::path::Path as ObjPath;
use object_store::ObjectStore;
use tracing::{info, warn};

/// Uploads completed MCAP files to an S3-compatible bucket.
///
/// Credentials must be supplied explicitly via the standard AWS environment
/// variables (typically loaded from the systemd `EnvironmentFile`):
///   AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_SESSION_TOKEN (optional)
///   AWS_REGION / AWS_DEFAULT_REGION
///
/// There is intentionally **no fallback to EC2 instance metadata** — this
/// tool is designed for workstations, not cloud instances.
///
/// For S3-compatible storage (Cloudflare R2, MinIO, …) set
///   KMMON_S3_ENDPOINT_URL=https://…
#[derive(Clone)]
pub struct S3Uploader {
    store: Arc<dyn ObjectStore>,
    prefix: String,
}

impl S3Uploader {
    /// Builds the uploader from environment variables.
    ///
    /// Returns `Ok(None)` when `KMMON_S3_BUCKET` is unset (upload disabled).
    /// Returns `Err` when the bucket is set but credentials are missing.
    pub fn from_env() -> Result<Option<Self>> {
        let bucket = match std::env::var("KMMON_S3_BUCKET") {
            Ok(b) => b,
            Err(_) => return Ok(None),
        };

        let prefix = std::env::var("KMMON_S3_PREFIX").unwrap_or_else(|_| "recordings".into());

        // Credentials are required. We do not fall back to the EC2 instance
        // metadata service — if the key isn't in the environment the
        // EnvironmentFile probably didn't load, and we should say so clearly.
        let key = std::env::var("AWS_ACCESS_KEY_ID")
            .context("KMMON_S3_BUCKET is set but AWS_ACCESS_KEY_ID is missing — check credentialsFile")?;
        let secret = std::env::var("AWS_SECRET_ACCESS_KEY")
            .context("KMMON_S3_BUCKET is set but AWS_SECRET_ACCESS_KEY is missing — check credentialsFile")?;

        let mut builder = AmazonS3Builder::new()
            .with_bucket_name(&bucket)
            .with_access_key_id(key)
            .with_secret_access_key(secret);

        if let Ok(token) = std::env::var("AWS_SESSION_TOKEN") {
            builder = builder.with_token(token);
        }

        let region = std::env::var("AWS_REGION")
            .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
            .unwrap_or_else(|_| "us-east-1".into());
        builder = builder.with_region(region);

        if let Ok(endpoint) = std::env::var("KMMON_S3_ENDPOINT_URL") {
            builder = builder.with_endpoint(endpoint).with_allow_http(true);
        }

        let store = builder
            .build()
            .context("Failed to initialise S3 client")?;

        info!("S3 upload enabled: bucket={bucket} prefix={prefix}");

        Ok(Some(Self {
            store: Arc::new(store),
            prefix,
        }))
    }

    /// Reads `local_path` and uploads it to `<prefix>/<filename>` in the bucket.
    /// The local file is **not** deleted here; retention is handled separately.
    pub async fn upload(&self, local_path: &Path) -> Result<()> {
        let filename = local_path
            .file_name()
            .and_then(|n| n.to_str())
            .context("path has no filename")?;

        let prefix = self.prefix.trim_matches('/');
        let object_key = if prefix.is_empty() {
            filename.to_string()
        } else {
            format!("{prefix}/{filename}")
        };

        let obj_path = ObjPath::parse(&object_key).context("invalid S3 key")?;

        let data: Vec<u8> = tokio::fs::read(local_path)
            .await
            .with_context(|| format!("reading {}", local_path.display()))?;
        let bytes_len = data.len();

        self.store
            .put(&obj_path, data.into())
            .await
            .with_context(|| format!("uploading to s3://{object_key}"))?;

        info!(
            "Uploaded {} ({:.1} MB) → s3://{}",
            filename,
            bytes_len as f64 / 1_048_576.0,
            object_key,
        );
        Ok(())
    }

    /// Fire-and-forget wrapper: spawns a tokio task and logs any error.
    pub fn upload_detached(&self, local_path: std::path::PathBuf) {
        let this = self.clone();
        tokio::spawn(async move {
            if let Err(e) = this.upload(&local_path).await {
                warn!("S3 upload failed: {e:#}");
            }
        });
    }
}
