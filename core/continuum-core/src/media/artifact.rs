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
    FsStore::new(
        crate::modules::persona_instance_manager::resolve_continuum_root()
            .join("media")
            .join("blobs"),
    )
    .map_err(|e| e.to_string())
}

impl ImageArtifact {
    /// Validate actual image headers, then atomically retain the original bytes.
    pub fn retain(store: &FsStore, bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() as u64 > MAX_ENCODED_BYTES {
            return Err("image exceeds encoded-byte allocation ceiling".into());
        }
        let format = image::guess_format(bytes).map_err(|e| e.to_string())?;
        let reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
        let (width, height) = reader.into_dimensions().map_err(|e| e.to_string())?;
        if width == 0 || height == 0 {
            return Err("image has an empty extent".into());
        }
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
        store
            .get_verified(
                &MediaRef {
                    hash,
                    size_bytes: self.size_bytes,
                    mime: Some(self.mime.clone()),
                },
                MAX_ENCODED_BYTES,
            )
            .map_err(|e| e.to_string())
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

/// Lift only registered capture-result shapes. Arbitrary shell output containing
/// a dataUrl is not a screenshot. Failed captures never become visual evidence.
/// The original field is removed only after durable storage succeeds.
pub fn retain_capture(
    command: &str,
    value: &mut serde_json::Value,
) -> Result<Option<ImageArtifact>, String> {
    retain_capture_with(command, value, store)
}

pub(crate) fn retain_capture_with(
    command: &str,
    value: &mut serde_json::Value,
    open_store: impl FnOnce() -> Result<FsStore, String>,
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
    let artifact = ImageArtifact::from_data_url(&open_store()?, data)?;
    image.remove("dataUrl");
    image.insert(
        "artifact".into(),
        serde_json::to_value(&artifact).map_err(|e| e.to_string())?, // boundary: durable reference replaces pixels in the tool-result transcript wire.
    );
    Ok(Some(artifact))
}
