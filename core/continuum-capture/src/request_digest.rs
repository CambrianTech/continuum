//! The one digest of a captured request: canonical (object keys sorted), so the same request
//! digests the same whoever serialized it. Replay checks a capture against it before replaying
//! (`request_replay`), and an adoption from a capture names the capture by it
//! (`persona/hands/adopt_capture`).

use continuum_engine::ai::types::TextGenerationRequest;
use sha2::Digest;

/// The canonical sha256 of `request`, hex.
pub fn request_sha256(request: &TextGenerationRequest) -> Result<String, serde_json::Error> {
    value_sha256(serde_json::to_value(request)?) // boundary: the request as its canonical digest's input
}

/// The canonical sha256 of `value`, hex: object keys sorted before hashing.
pub fn value_sha256(mut value: serde_json::Value) -> Result<String, serde_json::Error> {
    value.sort_all_objects();
    let bytes = serde_json::to_vec(&value)?; // boundary: the canonical bytes that are hashed
    Ok(format!("{:x}", sha2::Sha256::digest(bytes)))
}
