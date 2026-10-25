//! Whether a LoRA can load on a base, as one digest read from a GGUF header (card 3fd98325,
//! S3 of the cross-grid training lease).
//!
//! A LoRA loads on any base with its tokenizer and tensor shapes; the quantization does not
//! matter (the adapter adds f16 deltas to whatever the base dequantizes to). A GGUF LoRA may
//! omit tokenizer metadata, so the trainer reports this digest of its base beside the base's
//! content digest. This is COMPATIBILITY evidence, never identity: a fine-tuned sibling with
//! the same architecture, tokenizer and shapes has the same digest (Codex on #5418), so which
//! base an adapter was trained on is decided by the content digest, not this one.
//!
//! The digest covers, in sorted order: `general.architecture`, every `{arch}.*` hyperparameter,
//! every `tokenizer.ggml.*` value, and every tensor's name and shape. It leaves out tensor types
//! and offsets (the quantization), `general.*` naming, and the chat template (rendering, not
//! tokenization). Only the header is read, never the weights.

use std::collections::BTreeMap;

use candle_core::quantized::gguf_file::Content;
use sha2::{Digest, Sha256};

/// The LoRA-compatibility digest (lowercase hex SHA-256) of an open GGUF, read from its start.
/// The handle is left wherever the header ends, so a caller reading the whole file again seeks
/// back first. Taking a handle, not a path, lets a caller read the header through the same
/// handle it proved to be the file it means (`WeightsStamp::read_verified`).
pub fn lora_base_fingerprint<R: std::io::Read + std::io::Seek>(reader: &mut R) -> Result<String, String> {
    reader.seek(std::io::SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let content = Content::read(reader).map_err(|e| format!("not a readable GGUF header: {e}"))?;
    fingerprint_of(&content)
}

fn fingerprint_of(content: &Content) -> Result<String, String> {
    let architecture = super::gguf_keys::architecture(content).ok_or("no general.architecture: the base is unidentifiable")?;
    let arch_prefix = format!("{architecture}.");
    let mut fields: BTreeMap<&str, String> = BTreeMap::new();
    fields.insert("general.architecture", architecture.clone());
    for (key, value) in &content.metadata {
        if key.starts_with(&arch_prefix) || key.starts_with("tokenizer.ggml.") {
            fields.insert(key.as_str(), format!("{value:?}"));
        }
    }
    let tensors: BTreeMap<&str, &[usize]> = content.tensor_infos.iter().map(|(name, info)| (name.as_str(), info.shape.dims())).collect();
    let mut hasher = Sha256::new();
    for (key, value) in fields {
        hasher.update(format!("{key}={value}\n"));
    }
    for (name, dims) in tensors {
        hasher.update(format!("tensor {name}={dims:?}\n"));
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::quantized::gguf_file::{self, Value};
    use candle_core::quantized::{GgmlDType, QTensor};
    use candle_core::{Device, Tensor};
    use std::io::Cursor;

    fn header(tokens: &[&str], dtype: GgmlDType, name: &str) -> Content {
        let md = vec![
            ("general.architecture", Value::String("qwen2".into())),
            ("general.name", Value::String(name.into())),
            ("qwen2.block_count", Value::U32(2)),
            ("tokenizer.ggml.tokens", Value::Array(tokens.iter().map(|t| Value::String((*t).into())).collect())),
            ("tokenizer.chat_template", Value::String(format!("{name} template"))),
        ];
        let weights = Tensor::zeros((4, 32), candle_core::DType::F32, &Device::Cpu).expect("tensor"); // JUSTIFIED: test fixture
        let q = QTensor::quantize(&weights, dtype).expect("quantize"); // JUSTIFIED: test fixture
        let refs: Vec<(&str, &Value)> = md.iter().map(|(k, v)| (*k, v)).collect();
        let mut buf = Cursor::new(Vec::new());
        gguf_file::write(&mut buf, &refs, &[("blk.0.attn_q.weight", &q)]).expect("write"); // JUSTIFIED: test fixture
        buf.set_position(0);
        Content::read(&mut buf).expect("read") // JUSTIFIED: test fixture
    }

    // what this catches (card 3fd98325): an adapter refused because the origin's copy of the
    // same base is quantized or named differently, or accepted onto a base with another
    // tokenizer. Only the tokenizer, architecture and shapes decide.
    #[test]
    fn the_base_digest_is_its_tokenizer_and_shapes_not_its_quantization() {
        let f32_copy = fingerprint_of(&header(&["a", "b"], GgmlDType::F32, "base")).expect("digest"); // JUSTIFIED: test asserts digests
        let f16_copy = fingerprint_of(&header(&["a", "b"], GgmlDType::F16, "base-f16")).expect("digest"); // JUSTIFIED: test asserts digests
        let other_tokens = fingerprint_of(&header(&["a", "c"], GgmlDType::F32, "base")).expect("digest"); // JUSTIFIED: test asserts digests
        assert_eq!(f32_copy, f16_copy, "quantization, name and chat template do not change the base");
        assert_ne!(f32_copy, other_tokens, "another tokenizer is another base");

        let refs: Vec<(&str, &Value)> = Vec::new();
        let mut buf = Cursor::new(Vec::new());
        gguf_file::write(&mut buf, &refs, &[]).expect("write"); // JUSTIFIED: test fixture
        buf.set_position(0);
        assert!(fingerprint_of(&Content::read(&mut buf).expect("read")).is_err(), "no architecture, no identity"); // JUSTIFIED: test fixture
    }
}
