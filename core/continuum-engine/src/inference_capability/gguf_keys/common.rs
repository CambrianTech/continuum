//! Required metadata for backend construction, decoded at the GGUF boundary.
use candle_core::quantized::gguf_file::Content;
use std::path::Path;

/// GGUF metadata extracted before backend construction.
pub struct GgufMetadata {
    pub architecture: String,
    pub context_length: usize,
    pub model_name: Option<String>,
}

pub fn decode(content: &Content, path: &Path) -> Result<GgufMetadata, String> {
    // general.architecture is REQUIRED — silently falling back to "llama" would
    // route a qwen/mistral/phi/etc. model through the wrong backend and produce
    // garbage output or outright crash. Rule-2 violation (fallbacks are illegal)
    // fixed 2026-04-23. If a GGUF is missing this metadata, that's a broken file,
    // not a thing to paper over. Read via the ONE shared canonical-key reader.
    let architecture = super::architecture(&content).ok_or_else(|| {
        format!(
            "GGUF {} is missing required metadata key 'general.architecture' — cannot \
             determine backend. Silent fallback to 'llama' has been removed; fix the \
             GGUF file or re-export it with proper metadata.",
            path.display()
        )
    })?;

    // context_length via the shared reader: architecture-specific key first,
    // then the historical `llama.context_length` fallback — the ONE place that
    // policy is defined. If neither exists, that's a broken GGUF, not a thing
    // to guess 4096 for.
    let context_length = super::context_length(&content, &architecture)
        .map(|v| v as usize)
        .ok_or_else(|| {
            format!(
                "GGUF {} (architecture={architecture}) is missing context_length metadata \
             (tried '{architecture}.context_length' and 'llama.context_length'). Silent \
             fallback to 4096 has been removed; fix the GGUF file.",
                path.display()
            )
        })?;

    let model_name = super::general_name(&content);

    Ok(GgufMetadata {
        architecture,
        context_length,
        model_name,
    })
}
