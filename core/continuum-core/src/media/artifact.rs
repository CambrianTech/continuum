//! Durable image receipts shared by browser observations and explicit inspection.
//! Bytes live in AIRC's verified content-addressed store. Transcripts carry only
//! this reference; loading pixels is a separate, bounded projection.

use airc_blobs::{ContentAddressedStore, ContentHash, FsStore, MediaRef};
use base64::Engine;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Decoder allocation ceiling, independent of model context size. This bounds
/// untrusted encoded images before decoding; it is not a visual resolution tier.
pub const MAX_ENCODED_BYTES: u64 = 12 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/media/ImageArtifact.ts"
)]
pub struct ImageArtifact {
    pub hash: String,
    #[ts(type = "number")]
    pub size_bytes: u64,
    pub mime: String,
    pub width: u32,
    pub height: u32,
}

pub fn store() -> Result<FsStore, String> {
    FsStore::new(store_path()).map_err(|e| e.to_string())
}

/// A retained text blob (a submitted patch) read by its reference: the store's own verified
/// read ([`FsStore::get_verified`]: bounded by the reference's size, digest checked), so the
/// reviewer gets exactly the bytes the submitter published or a named reason why not (card
/// e1babb61). The bound is the reference's own size, never a constant: a whole patch, unclipped.
pub(crate) fn read_text_blob(reference: &MediaRef) -> Result<String, String> {
    let bytes = store()?
        .get_verified(reference, reference.size_bytes)
        .map_err(|e| format!("{} ({} bytes) in {}: {e}", reference.hash.to_hex(), reference.size_bytes, store_path().display()))?;
    String::from_utf8(bytes).map_err(|e| format!("{} is not UTF-8 text: {e}", reference.hash.to_hex()))
}

pub(crate) fn store_path() -> std::path::PathBuf {
    crate::paths::continuum_root()
        .join("media")
        .join("blobs")
}

impl ImageArtifact {
    /// Validate actual image headers, then atomically retain the original bytes.
    pub fn retain(store: &FsStore, bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() as u64 > MAX_ENCODED_BYTES {
            return Err("image exceeds encoded-byte allocation ceiling".into());
        }
        let (format, width, height) = image_header(bytes)?;
        let hash = store.put(bytes).map_err(|e| e.to_string())?;
        Ok(Self {
            hash: hash.to_hex(),
            size_bytes: bytes.len() as u64,
            mime: format.to_mime_type().to_string(),
            width,
            height,
        })
    }

    pub fn read(&self, store: &FsStore) -> Result<Vec<u8>, String> {
        let hash = ContentHash::from_hex(&self.hash).ok_or("invalid image content hash")?;
        let bytes = store
            .get_verified(
                &MediaRef {
                    hash,
                    size_bytes: self.size_bytes,
                    mime: Some(self.mime.clone()),
                },
                MAX_ENCODED_BYTES,
            )
            .map_err(|e| e.to_string())?;
        // A digest authenticates the bytes, not the caller's projection metadata.
        // Dimensions feed visual budgeting; MIME feeds the inference wire.
        let (format, width, height) = image_header(&bytes)?;
        if (width, height) != (self.width, self.height)
            || format.to_mime_type() != self.mime
        {
            return Err("image reference metadata does not match stored image header".into());
        }
        Ok(bytes)
    }

    pub fn from_data_url(store: &FsStore, value: &str) -> Result<Self, String> {
        let (header, payload) = value.split_once(',').ok_or("invalid image data URL")?;
        if !header.starts_with("data:image/") || !header.ends_with(";base64") {
            return Err("expected a base64 image data URL".into());
        }
        if payload.len() as u64 > MAX_ENCODED_BYTES.div_ceil(3) * 4 {
            return Err("image data URL exceeds encoded-byte allocation ceiling".into());
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .map_err(|e| e.to_string())?; // boundary: browser image data URL becomes stored binary bytes.
        Self::retain(store, &bytes)
    }
}

fn image_header(bytes: &[u8]) -> Result<(image::ImageFormat, u32, u32), String> {
    let format = image::guess_format(bytes).map_err(|e| e.to_string())?;
    let reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let (width, height) = reader.into_dimensions().map_err(|e| e.to_string())?;
    if width == 0 || height == 0 {
        return Err("image has an empty extent".into());
    }
    Ok((format, width, height))
}

/// A live look can return several participants. Keep every frame through the
/// same durable image boundary used by screenshots, before transcript folding.
pub fn retain_captures(
    command: &str,
    value: &mut serde_json::Value,
) -> Result<Vec<ImageArtifact>, String> {
    let directory = store_path().with_file_name("captures");
    retain_captures_with(command, value, store, &directory)
}

pub(crate) fn retain_captures_with(
    command: &str,
    value: &mut serde_json::Value,
    mut open_store: impl FnMut() -> Result<FsStore, String>,
    directory: &std::path::Path,
) -> Result<Vec<ImageArtifact>, String> {
    if command != "perception/look" {
        return retain_capture_with(command, value, &mut open_store, directory)
            .map(|image| image.into_iter().collect());
    }
    if value.get("success").and_then(|v| v.as_bool()) != Some(true) {
        return Ok(Vec::new());
    }
    let mut images = Vec::new();
    if let Some(views) = value.get_mut("views").and_then(|v| v.as_array_mut()) {
        for view in views {
            if view.get("error").is_some_and(|error| !error.is_null()) {
                continue;
            }
            let Some(image) = view.get_mut("image") else { continue };
            let mut capture = serde_json::json!({"success": true, "image": image.take()});
            let retained = retain_capture_with("perception/observe", &mut capture, &mut open_store, directory);
            *image = capture["image"].take();
            match retained {
                Ok(Some(artifact)) => images.push(artifact),
                Ok(None) => {},
                Err(error) => {
                    // One unavailable participant must not discard the other
                    // participants' verified frames. Preserve the per-view fault.
                    view["error"] = serde_json::Value::String(error);
                }
            }
        }
    }
    Ok(images)
}

pub(crate) fn retain_capture_with(
    command: &str,
    value: &mut serde_json::Value,
    open_store: impl FnOnce() -> Result<FsStore, String>,
    attachment_directory: &std::path::Path,
) -> Result<Option<ImageArtifact>, String> {
    if command == "vision/look" {
        return value
            .get("artifact")
            .filter(|v| !v.is_null())
            .map(|v| serde_json::from_value(v.clone()).map_err(|e| e.to_string())) // boundary: typed vision command's retained reference crosses its result JSON wire.
            .transpose();
    }
    let image = match command {
        "perception/observe" | "perception/hot-edit" | "perception/interact" => {
            if value.get("success").and_then(|v| v.as_bool()) != Some(true) {
                return Ok(None);
            }
            value.get_mut("image")
        }
        "interface/screenshot" => {
            if value.get("success").and_then(|v| v.as_bool()) != Some(true) {
                return Ok(None);
            }
            Some(value)
        }
        _ => return Ok(None),
    };
    let Some(image) = image.and_then(|v| v.as_object_mut()) else {
        return Ok(None);
    };
    let Some(data) = image.get("dataUrl").and_then(|v| v.as_str()) else {
        return Ok(None);
    };
    let store = open_store()?;
    let artifact = ImageArtifact::from_data_url(&store, data)?;
    // The same capture supplies pixels AND a ready-to-use attachment. Storage
    // layout is never a task for the resident. Reuse the declared filepath slot.
    let bytes = artifact.read(&store)?;
    let format = image::guess_format(&bytes).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(attachment_directory).map_err(|e| e.to_string())?;
    let path = attachment_directory.join(format!(
        "{}.{}", artifact.hash, format.extensions_str()[0]
    ));
    if std::fs::read(&path).ok().as_deref() != Some(bytes.as_slice()) {
        let temporary = attachment_directory.join(format!("{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            std::fs::write(&temporary, &bytes)?;
            std::fs::rename(&temporary, &path)
        })();
        if let Err(error) = result {
            let _ = std::fs::remove_file(&temporary);
            // A concurrent identical capture may have published this attachment.
            if std::fs::read(&path).ok().as_deref() != Some(bytes.as_slice()) {
                return Err(format!("screenshot attachment publication failed: {error}"));
            }
        }
    }
    image.insert("filepath".into(), path.to_string_lossy().into_owned().into());
    image.remove("dataUrl");
    image.insert(
        "artifact".into(),
        serde_json::to_value(&artifact).map_err(|e| e.to_string())?, // boundary: durable reference replaces pixels in the tool-result transcript wire.
    );
    Ok(Some(artifact))
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: card e1babb61. A reviewer must read exactly the bytes a submitter
    // retained, by the hash the submission published, and a reference that doesn't match the
    // store (another size, or no bytes at all) must say so, never hand back something else.
    #[test]
    fn a_retained_patch_reads_back_by_its_hash_and_a_mismatch_is_named() {
        use airc_blobs::ContentAddressedStore;
        let home = tempfile::tempdir().expect("test: home");
        crate::test_env::with_test_home(home.path(), || {
            let patch = "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-old\n+new\n";
            let hash = store().expect("test: store").put(patch.as_bytes()).expect("test: put");
            let reference = |size_bytes| MediaRef { hash: hash.clone(), size_bytes, mime: Some("text/x-patch".into()) };
            assert_eq!(read_text_blob(&reference(patch.len() as u64)).as_deref(), Ok(patch), "the retained bytes, verified");
            assert!(read_text_blob(&reference(patch.len() as u64 + 1)).is_err(), "a size the store doesn't hold is named");
            let absent = MediaRef { hash: airc_blobs::ContentHash::from_hex(&("0".repeat(63) + "1")).expect("test: hex"), size_bytes: 1, mime: None };
            assert!(read_text_blob(&absent).is_err(), "no bytes behind a hash is named, not empty text");
        });
    }
}
