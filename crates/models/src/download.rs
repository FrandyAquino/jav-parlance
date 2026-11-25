use std::path::{Path, PathBuf};

use futures::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::catalog::{Archive, CatalogEntry};
use crate::store::{INSTALLED_MARKER, ModelStore};

const PROGRESS_STEP: u64 = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("file system error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{id}: expected {expected} bytes, received {actual}")]
    SizeMismatch { id: String, expected: u64, actual: u64 },
    #[error("{id}: checksum mismatch (expected {expected}, got {actual}); the file was discarded")]
    ChecksumMismatch { id: String, expected: String, actual: String },
    #[error("archive extraction failed: {0}")]
    Extract(String),
    #[error("download cancelled")]
    Cancelled,
}

/// Downloads `entry` into `store`, verifying size and SHA-256 before the
/// artifact becomes visible. `progress(downloaded, total)` is called roughly
/// every MiB.
pub async fn download(
    client: &reqwest::Client,
    store: &ModelStore,
    entry: &CatalogEntry,
    cancel: CancellationToken,
    progress: impl Fn(u64, u64),
) -> Result<PathBuf, DownloadError> {
    let staging = store.staging_dir();
    tokio::fs::create_dir_all(&staging).await?;
    let partial = staging.join(format!("{}.part", entry.file_name));

    let result = fetch_verified(client, entry, &partial, &cancel, &progress).await;
    if let Err(error) = result {
        let _ = tokio::fs::remove_file(&partial).await;
        return Err(error);
    }
    let destination = store.path(entry);
    match entry.archive {
        None => {
            tokio::fs::rename(&partial, &destination).await?;
        }
        Some(kind) => {
            let (archive, target) = (partial.clone(), destination.clone());
            tokio::task::spawn_blocking(move || extract(kind, &archive, &target))
                .await
                .map_err(|e| DownloadError::Extract(e.to_string()))??;
            tokio::fs::remove_file(&partial).await?;
        }
    }
    progress(entry.size_bytes, entry.size_bytes);
    Ok(destination)
}

async fn fetch_verified(
    client: &reqwest::Client,
    entry: &CatalogEntry,
    partial: &Path,
    cancel: &CancellationToken,
    progress: &impl Fn(u64, u64),
) -> Result<(), DownloadError> {
    let response = client.get(entry.url).send().await?.error_for_status()?;
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(partial).await?;
    let mut hasher = Sha256::new();
    let (mut received, mut reported) = (0u64, 0u64);

    loop {
        let chunk = tokio::select! {
            _ = cancel.cancelled() => return Err(DownloadError::Cancelled),
            next = stream.next() => match next {
                Some(chunk) => chunk?,
                None => break,
            },
        };
        received += chunk.len() as u64;
        if received > entry.size_bytes {
            return Err(DownloadError::SizeMismatch {
                id: entry.id.into(),
                expected: entry.size_bytes,
                actual: received,
            });
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        if received - reported >= PROGRESS_STEP {
            reported = received;
            progress(received, entry.size_bytes);
        }
    }
    file.flush().await?;
    drop(file);

    if received != entry.size_bytes {
        return Err(DownloadError::SizeMismatch { id: entry.id.into(), expected: entry.size_bytes, actual: received });
    }
    let actual = hex::encode(hasher.finalize());
    if !actual.eq_ignore_ascii_case(entry.sha256) {
        return Err(DownloadError::ChecksumMismatch { id: entry.id.into(), expected: entry.sha256.into(), actual });
    }
    Ok(())
}

/// Extracts into a sibling temp directory and renames it into place, so a
/// crash mid-extraction never leaves a half-installed runtime.
fn extract(kind: Archive, archive: &Path, target: &Path) -> Result<(), DownloadError> {
    let staging = target.with_extension("extracting");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    let file = std::fs::File::open(archive)?;
    match kind {
        Archive::Zip => zip::ZipArchive::new(file)
            .and_then(|mut zip| zip.extract(&staging))
            .map_err(|e| DownloadError::Extract(e.to_string()))?,
        Archive::TarGz => tar::Archive::new(flate2::read::GzDecoder::new(file))
            .unpack(&staging)
            .map_err(|e| DownloadError::Extract(e.to_string()))?,
    }
    std::fs::write(staging.join(INSTALLED_MARKER), b"")?;
    let _ = std::fs::remove_dir_all(target);
    std::fs::rename(&staging, target)?;
    Ok(())
}
