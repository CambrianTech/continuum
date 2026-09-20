//! The `Downloader` — the ONE fetch primitive every artifact uses.
//!
//! Replaces the per-script curl/retry/extract copy-paste with one resumable,
//! checksum-verified, crash-safe download. A 9 GB model survives an interrupt (resume
//! via HTTP Range from the `.part`), a corrupt file is caught (sha256), and a partial
//! never masquerades as complete (atomic `.part` → rename only after verify). Streams
//! to disk + hashes in 1 MB chunks — never buffers the whole weight in RAM.

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("http error fetching {url}: {source}")]
    Http {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("i/o error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("checksum mismatch for {url}: expected {expected}, got {actual}")]
    Checksum {
        url: String,
        expected: String,
        actual: String,
    },
}

/// Receives download progress so a fetch is never a blind wait — the UI / bus / log
/// gets "42% (3.8/9.0 GB)" instead of a frozen spinner. Called periodically (throttled)
/// as bytes land; `total` is None when the server sends no Content-Length.
pub trait ProgressSink: Send + Sync {
    fn on_progress(&self, downloaded: u64, total: Option<u64>);
}

/// The default — no feedback (for callers that don't want it).
pub struct NoopProgress;
impl ProgressSink for NoopProgress {
    fn on_progress(&self, _downloaded: u64, _total: Option<u64>) {}
}

/// One resumable, verified fetch primitive shared by every `ArtifactSource`.
pub struct Downloader {
    client: reqwest::Client,
}

impl Default for Downloader {
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Downloader {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    /// Fetch `url` into `dest`. Idempotent + resumable + verified:
    /// - if `dest` already exists and (when a checksum is given) verifies → cache hit,
    ///   returns its size without touching the network;
    /// - otherwise streams into `dest.part`, resuming from any partial via Range;
    /// - verifies sha256 (if given) before committing;
    /// - atomically renames `.part` → `dest` only on success.
    /// Returns the on-disk byte count. Fails LOUD — never leaves a verified-wrong or
    /// half-written file in `dest`.
    pub async fn fetch(
        &self,
        url: &str,
        dest: &Path,
        expected_sha256: Option<&str>,
    ) -> Result<u64, DownloadError> {
        self.fetch_with_progress(url, dest, expected_sha256, &NoopProgress)
            .await
    }

    /// As `fetch`, but emits periodic progress to `progress` (throttled to ~4 MiB) so a
    /// big download shows feedback instead of a blind wait ([[never-blind-feedback-driven-iteration]]).
    pub async fn fetch_with_progress(
        &self,
        url: &str,
        dest: &Path,
        expected_sha256: Option<&str>,
        progress: &dyn ProgressSink,
    ) -> Result<u64, DownloadError> {
        // Cache hit: present + (if a checksum is pinned) verifies.
        if dest.exists() {
            match expected_sha256 {
                None => return Ok(file_len(dest).await),
                Some(exp)
                    if sha256_file(dest).await.ok().as_deref() == Some(&exp.to_lowercase()) =>
                {
                    return Ok(file_len(dest).await);
                }
                // present but wrong/unknown checksum → re-fetch over it.
                Some(_) => {}
            }
        }

        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|source| DownloadError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
        }

        let part = part_path(dest);
        let resume_from = file_len(&part).await;

        let mut req = self.client.get(url);
        if resume_from > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={resume_from}-"));
        }
        let resp = req
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|source| DownloadError::Http {
                url: url.to_string(),
                source,
            })?;

        // We can only APPEND to the partial if the server honored Range (206). A plain
        // 200 means it sent the whole body from byte 0 → restart clean, or we'd corrupt.
        let append = resume_from > 0 && resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;

        let mut opts = tokio::fs::OpenOptions::new();
        opts.create(true).write(true);
        if append {
            opts.append(true);
        } else {
            opts.truncate(true);
        }
        let mut file = opts.open(&part).await.map_err(|source| DownloadError::Io {
            path: part.clone(),
            source,
        })?;

        // Total for progress: Content-Length is the REMAINING body, so add back the
        // already-downloaded prefix on a resume (206).
        let total = resp.content_length().map(|remaining| {
            if append {
                resume_from + remaining
            } else {
                remaining
            }
        });
        let mut downloaded = if append { resume_from } else { 0 };
        let mut last_emit = downloaded;

        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|source| DownloadError::Http {
                url: url.to_string(),
                source,
            })?;
            file.write_all(&chunk)
                .await
                .map_err(|source| DownloadError::Io {
                    path: part.clone(),
                    source,
                })?;
            downloaded += chunk.len() as u64;
            // Throttle to ~4 MiB so a 9 GB model gives ~2000 updates, not one per packet.
            if downloaded - last_emit >= (4 << 20) {
                progress.on_progress(downloaded, total);
                last_emit = downloaded;
            }
        }
        progress.on_progress(downloaded, total); // final tick (100%)
        file.sync_all().await.map_err(|source| DownloadError::Io {
            path: part.clone(),
            source,
        })?;
        drop(file);

        if let Some(exp) = expected_sha256 {
            let actual = sha256_file(&part)
                .await
                .map_err(|source| DownloadError::Io {
                    path: part.clone(),
                    source,
                })?;
            if actual != exp.to_lowercase() {
                return Err(DownloadError::Checksum {
                    url: url.to_string(),
                    expected: exp.to_string(),
                    actual,
                });
            }
        }

        tokio::fs::rename(&part, dest)
            .await
            .map_err(|source| DownloadError::Io {
                path: part.clone(),
                source,
            })?;
        Ok(file_len(dest).await)
    }
}

/// `<dest>.part` — the in-flight file; renamed to `dest` only after verify.
fn part_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_os_string();
    s.push(".part");
    PathBuf::from(s)
}

/// File length in bytes, or 0 if it doesn't exist.
async fn file_len(path: &Path) -> u64 {
    tokio::fs::metadata(path)
        .await
        .map(|m| m.len())
        .unwrap_or(0)
}

/// Stream-hash a file (1 MB chunks — safe for multi-GB weights). Lowercase hex.
async fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // what this catches: the verify primitive is correct — sha256 of a known input
    // matches the known digest. A wrong checksum in the catalog would otherwise let a
    // corrupt weight through.
    #[tokio::test]
    async fn sha256_file_matches_known_digest() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("f");
        tokio::fs::write(&p, b"hello").await.unwrap();
        // sha256("hello")
        assert_eq!(
            sha256_file(&p).await.unwrap(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    // what this catches: fetch is a cache-hit no-op when the file is already present +
    // verifies — no network touched (a bad URL here would error if it dialed out).
    #[tokio::test]
    async fn fetch_is_cache_hit_when_present_and_verified() {
        let dir = TempDir::new().unwrap();
        let dest = dir.path().join("model.gguf");
        tokio::fs::write(&dest, b"hello").await.unwrap();
        let dl = Downloader::default();
        let bytes = dl
            .fetch(
                "http://0.0.0.0:1/never-dialed",
                &dest,
                Some("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"),
            )
            .await
            .expect("present+verified is a no-op, must not dial the network");
        assert_eq!(bytes, 5);
    }

    // what this catches: part_path derivation is the sibling ".part", not a mangled name.
    #[test]
    fn part_path_is_dest_dot_part() {
        assert_eq!(
            part_path(Path::new("/m/x.gguf")),
            PathBuf::from("/m/x.gguf.part")
        );
    }

    // what this catches: LIVE — a real download emits progress (feedback, not a blind
    // wait) and the final tick equals the total bytes. Network-gated + ignored by
    // default; run explicitly: `cargo test -p continuum-core -- --ignored fetch_emits_progress`.
    #[tokio::test]
    #[ignore]
    async fn fetch_emits_progress_live() {
        use std::sync::atomic::{AtomicU64, Ordering};
        struct Rec {
            calls: AtomicU64,
            last: AtomicU64,
        }
        impl ProgressSink for Rec {
            fn on_progress(&self, downloaded: u64, _total: Option<u64>) {
                self.calls.fetch_add(1, Ordering::Relaxed);
                self.last.store(downloaded, Ordering::Relaxed);
            }
        }
        let dir = TempDir::new().unwrap();
        let dest = dir.path().join("base_female.zip");
        let rec = Rec {
            calls: AtomicU64::new(0),
            last: AtomicU64::new(0),
        };
        let bytes = Downloader::default()
            .fetch_with_progress(
                "https://opengameart.org/sites/default/files/base_female.zip",
                &dest,
                None,
                &rec,
            )
            .await
            .expect("real download");
        assert!(bytes > 1_000_000, "downloaded a real multi-MB file");
        assert!(
            rec.calls.load(Ordering::Relaxed) >= 1,
            "progress was emitted"
        );
        assert_eq!(
            rec.last.load(Ordering::Relaxed),
            bytes,
            "final progress == total bytes"
        );
    }
}
