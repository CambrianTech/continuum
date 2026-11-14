//! Required GGUF facts for the residency consumer, decoded at the artifact boundary.
use candle_core::quantized::gguf_file::Content;
use std::path::Path;

pub struct GgufResidency {
    pub architecture: String,
    pub layer_count: u32,
    pub file_type: u32,
    pub parameter_count: u64,
}

pub fn decode(content: &Content, path: &Path) -> Result<GgufResidency, String> {
    // architecture: required (same posture as backends::read_gguf_metadata).
    // Read through the ONE shared canonical-key reader; this consumer's
    // policy is "refuse if absent".
    let architecture = super::architecture(content).ok_or_else(|| {
        format!(
            "GGUF {} is missing required 'general.architecture' — refuse rather than \
                 guess. Same rule as backends::read_gguf_metadata (2026-04-23).",
            path.display()
        )
    })?;

    // block_count: required. The {arch}.block_count key is the canonical
    // GGUF layer count. Without it, the residency gate's layer-count
    // evidence is missing — refuse rather than fake.
    let layer_count = super::block_count(content, &architecture).ok_or_else(|| {
        format!(
            "GGUF {} (arch={architecture}) is missing required '{architecture}.block_count' \
                 — residency gate cannot report gpu_layer_count without it. Refuse rather \
                 than guess.",
            path.display()
        )
    })?;

    // file_type: required. Maps to bytes_per_parameter. Unknown enum
    // value returns Err — better to refuse than guess wrong quantization
    // (caller would over- or under-estimate VRAM).
    let file_type = content
        .metadata
        .get("general.file_type")
        .and_then(|v| v.to_u32().ok())
        .ok_or_else(|| {
            format!(
                "GGUF {} is missing required 'general.file_type' — bytes-per-param mapping \
                 needs the quantization tag to estimate VRAM.",
                path.display()
            )
        })?;
    Ok(GgufResidency {
        architecture,
        layer_count,
        file_type,
        parameter_count: super::artifact_parameter_count(content)?,
    })
}
