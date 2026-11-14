//! GGUF architecture decoder. Untrusted metadata is interpreted once here;
//! downstream architecture consumers receive required, typed dimensions.
use candle_core::quantized::gguf_file::Content;
use std::path::Path;

pub struct GgufArchitecture {
    pub num_layers: usize,
    pub hidden_size: usize,
    pub num_attention_heads: usize,
    pub num_kv_heads: usize,
    pub head_dim: usize,
    pub intermediate_size: usize,
    pub vocab_size: usize,
    pub context_length: usize,
}

pub fn decode(content: &Content, path: &Path) -> Result<GgufArchitecture, String> {
    let md = &content.metadata;

    // `general.architecture` via the ONE shared reader; this consumer's
    // policy is "required" so it wraps the shared `Option` in its own
    // refuse-error.
    let arch = super::architecture(&content)
        .ok_or_else(|| format!("GGUF {} missing `general.architecture`", path.display()))?;

    // Required scalar dimension keyed under the model's own architecture.
    let req = |key: &str| -> Result<usize, String> {
        let v = md
            .get(key)
            .ok_or_else(|| format!("GGUF {} missing required key `{key}`", path.display()))?;
        v.to_u32()
            .map(|n| n as usize)
            .map_err(|e| format!("GGUF {} key `{key}` not a u32: {e}", path.display()))
    };

    let hidden_size = req(&format!("{arch}.embedding_length"))?;
    let intermediate_size = req(&format!("{arch}.feed_forward_length"))?;
    let num_attention_heads = req(&format!("{arch}.attention.head_count"))?;
    let num_kv_heads = req(&format!("{arch}.attention.head_count_kv"))?;
    let num_layers = req(&format!("{arch}.block_count"))?;
    // context_length through the shared reader so this path inherits the
    // `{arch}.context_length` → `llama.context_length` fallback the other
    // readers already have (previously ABSENT here — a GGUF carrying only
    // the historical llama.* key failed dimension extraction while its
    // Model row hydrated fine). Required for the KV-cache budget, so the
    // absent case is a refuse-error.
    let context_length = super::context_length(&content, &arch).ok_or_else(|| {
        format!(
            "GGUF {} missing required context_length (tried `{arch}.context_length` \
                         and `llama.context_length`)",
            path.display()
        )
    })? as usize;

    // head_dim: explicit `{arch}.attention.key_length` if present, else the
    // spec-defined derivation hidden_size / num_attention_heads (llama.cpp
    // omits the key precisely when it equals that quotient).
    let head_dim = match md.get(&format!("{arch}.attention.key_length")) {
        Some(v) => v.to_u32().map(|n| n as usize).map_err(|e| {
            format!(
                "GGUF {} key `{arch}.attention.key_length` not a u32: {e}",
                path.display()
            )
        })?,
        None => {
            if num_attention_heads == 0 {
                return Err(format!(
                    "GGUF {} has 0 attention heads — cannot derive head_dim",
                    path.display()
                ));
            }
            hidden_size / num_attention_heads
        }
    };

    // vocab_size: explicit `{arch}.vocab_size` if present, else the length
    // of the tokenizer's token array (the vocab IS that array). One of the
    // two must exist — a GGUF with neither cannot be served.
    let vocab_size = match md.get(&format!("{arch}.vocab_size")) {
        Some(v) => v.to_u32().map(|n| n as usize).map_err(|e| {
            format!(
                "GGUF {} key `{arch}.vocab_size` not a u32: {e}",
                path.display()
            )
        })?,
        None => {
            let tokens = md.get("tokenizer.ggml.tokens").ok_or_else(|| {
                format!(
                    "GGUF {} missing both `{arch}.vocab_size` and `tokenizer.ggml.tokens`",
                    path.display()
                )
            })?;
            tokens
                .to_vec()
                .map_err(|e| {
                    format!(
                        "GGUF {} `tokenizer.ggml.tokens` not an array: {e}",
                        path.display()
                    )
                })?
                .len()
        }
    };

    Ok(GgufArchitecture {
        num_layers,
        hidden_size,
        num_attention_heads,
        num_kv_heads,
        head_dim,
        intermediate_size,
        vocab_size,
        context_length,
    })
}
