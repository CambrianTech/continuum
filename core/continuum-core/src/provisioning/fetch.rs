//! The fetch executor — turn an `ArtifactSpec` into a placed file on disk.
//!
//! Outlier A of the fetch executor: the **zip/avatar** path (download the VRoid archive,
//! extract the `.vrm`, place it), which needs no repo→quant resolution and so validates
//! the fetch-executor interface before the model outlier B. Composes the `Downloader`
//! (resumable/verified/progress) with a `spawn_blocking` zip extract — Rust-native,
//! retiring the python3-`extractall` in `download-avatar-models.sh`.
//!
//! `Direct`/`HfFile` are a straight download (the download IS the artifact); `Zip`
//! downloads the archive then extracts the member matching `dest`'s extension.

use std::io::Read;
use std::path::{Path, PathBuf};

use super::downloader::{DownloadError, Downloader, ProgressSink};
use super::{ArtifactSpec, SourceKind};

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error(transparent)]
    Download(#[from] DownloadError),
    #[error("zip error for {url}: {msg}")]
    Zip { url: String, msg: String },
    #[error("no .{want} member in archive {url}")]
    NoMember { url: String, want: String },
    #[error("i/o at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Fetch `spec` and place the final artifact at `dest`. Direct/HfFile: the download IS
/// the artifact. Zip: download the archive to a sibling temp, extract the member whose
/// name ends in `dest`'s extension (e.g. `.vrm`), place it at `dest`, drop the archive.
/// Returns the byte count at `dest`. Fails LOUD (typed FetchError) — never a half-placed
/// file.
pub async fn fetch_and_place(
    spec: &ArtifactSpec,
    dest: &Path,
    downloader: &Downloader,
    progress: &dyn ProgressSink,
) -> Result<u64, FetchError> {
    match spec.source_kind {
        SourceKind::Direct | SourceKind::HfFile => Ok(downloader
            .fetch_with_progress(&spec.url, dest, spec.checksum.as_deref(), progress)
            .await?),
        SourceKind::Zip => {
            // Download the archive next to dest (Downloader owns .part/resume/verify).
            let archive = dest.with_extension("srczip");
            downloader
                .fetch_with_progress(&spec.url, &archive, spec.checksum.as_deref(), progress)
                .await?;

            let want_ext = dest
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("vrm")
                .to_string();
            let (archive_for_task, dest_buf, url) =
                (archive.clone(), dest.to_path_buf(), spec.url.clone());
            let bytes = tokio::task::spawn_blocking(move || {
                extract_member_by_ext(&archive_for_task, &want_ext, &dest_buf, &url)
            })
            .await
            .map_err(|e| FetchError::Zip {
                url: spec.url.clone(),
                msg: e.to_string(),
            })??;

            let _ = tokio::fs::remove_file(&archive).await; // archive is scratch — best-effort
            Ok(bytes)
        }
    }
}

/// Extract the first archive member whose name ends in `.{want_ext}` into `dest`
/// (blocking — call under `spawn_blocking`). Fail loud if no member matches.
fn extract_member_by_ext(
    archive: &Path,
    want_ext: &str,
    dest: &Path,
    url: &str,
) -> Result<u64, FetchError> {
    let file = std::fs::File::open(archive).map_err(|source| FetchError::Io {
        path: archive.to_path_buf(),
        source,
    })?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| FetchError::Zip {
        url: url.to_string(),
        msg: e.to_string(),
    })?;
    let suffix = format!(".{}", want_ext.to_lowercase());
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| FetchError::Zip {
            url: url.to_string(),
            msg: e.to_string(),
        })?;
        if entry.is_file() && entry.name().to_lowercase().ends_with(&suffix) {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|source| FetchError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            // Extract to a temp sibling, then atomic-rename — a crash mid-extract never
            // leaves a truncated .vrm masquerading as complete.
            let tmp = dest.with_extension("vrm.part");
            let mut out = std::fs::File::create(&tmp).map_err(|source| FetchError::Io {
                path: tmp.clone(),
                source,
            })?;
            let mut buf = [0u8; 1 << 16];
            let mut written = 0u64;
            loop {
                let n = entry.read(&mut buf).map_err(|source| FetchError::Io {
                    path: tmp.clone(),
                    source,
                })?;
                if n == 0 {
                    break;
                }
                use std::io::Write;
                out.write_all(&buf[..n]).map_err(|source| FetchError::Io {
                    path: tmp.clone(),
                    source,
                })?;
                written += n as u64;
            }
            drop(out);
            std::fs::rename(&tmp, dest).map_err(|source| FetchError::Io {
                path: dest.to_path_buf(),
                source,
            })?;
            return Ok(written);
        }
    }
    Err(FetchError::NoMember {
        url: url.to_string(),
        want: want_ext.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    // what this catches: the zip-extract primitive pulls the .vrm out of an archive
    // (ignoring other members) and writes its exact bytes to dest — the core of
    // Rust-native avatar provisioning that replaces the python3 extractall.
    #[test]
    fn extract_member_by_ext_pulls_the_vrm() {
        let dir = TempDir::new().unwrap();
        let archive = dir.path().join("a.srczip");
        // Build a zip with a decoy + the real .vrm.
        {
            let f = std::fs::File::create(&archive).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            let opts = zip::write::FileOptions::default();
            zw.start_file("readme.txt", opts).unwrap();
            zw.write_all(b"ignore me").unwrap();
            zw.start_file("models/avatar.vrm", opts).unwrap();
            zw.write_all(b"VRM-BYTES-1234").unwrap();
            zw.finish().unwrap();
        }
        let dest = dir.path().join("out/female.vrm");
        let n = extract_member_by_ext(&archive, "vrm", &dest, "test://a").unwrap();
        assert_eq!(n, 14);
        assert_eq!(std::fs::read(&dest).unwrap(), b"VRM-BYTES-1234");
        assert!(
            !dest.with_extension("vrm.part").exists(),
            "temp cleaned via rename"
        );
    }

    // what this catches: an archive with no matching member fails LOUD (NoMember),
    // never silently placing nothing.
    #[test]
    fn extract_fails_loud_when_no_member_matches() {
        let dir = TempDir::new().unwrap();
        let archive = dir.path().join("a.srczip");
        {
            let f = std::fs::File::create(&archive).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            let opts = zip::write::FileOptions::default();
            zw.start_file("readme.txt", opts).unwrap();
            zw.write_all(b"no vrm here").unwrap();
            zw.finish().unwrap();
        }
        let dest = dir.path().join("female.vrm");
        let err = extract_member_by_ext(&archive, "vrm", &dest, "test://a").unwrap_err();
        assert!(matches!(err, FetchError::NoMember { .. }));
        assert!(!dest.exists());
    }
}
