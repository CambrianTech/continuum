//! Qwen3.5 Safetensors Backend
//!
//! Implements `ModelBackend` for Qwen3.5 hybrid architecture (linear attention + full attention).
//! Loads from safetensors format. Supports forged/pruned models with modified head counts.
//!
//! Key difference from Qwen2: hybrid layer_types (linear_attention / full_attention),
//! causal conv1d in linear attention layers, partial rotary embeddings.

use std::path::PathBuf;

use candle_core::{DType, Device, Tensor};
use tokenizers::Tokenizer;

use super::{ModelBackend, ModelFormat};
use crate::inference::vendored::qwen35::Qwen35;
use crate::runtime;

/// Qwen3.5 safetensors backend.
#[allow(dead_code)]
pub struct Qwen35SafetensorsBackend {
    model: Qwen35,
    tokenizer: Tokenizer,
    device: Device,
    dtype: DType,
    model_id: String,
    eos_token_ids: Vec<u32>,
    context_length: usize,
    weight_paths: Vec<PathBuf>,
}

impl Qwen35SafetensorsBackend {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        model: Qwen35,
        tokenizer: Tokenizer,
        device: Device,
        dtype: DType,
        model_id: String,
        eos_token_ids: Vec<u32>,
        weight_paths: Vec<PathBuf>,
    ) -> Self {
        let context_length = model.context_length;
        Self {
            model,
            tokenizer,
            device,
            dtype,
            model_id,
            eos_token_ids,
            context_length,
            weight_paths,
        }
    }
}

impl ModelBackend for Qwen35SafetensorsBackend {
    fn architecture(&self) -> &str {
        "qwen3_5"
    }

    fn context_length(&self) -> usize {
        self.context_length
    }

    fn eos_token_ids(&self) -> &[u32] {
        &self.eos_token_ids
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn format(&self) -> ModelFormat {
        ModelFormat::Safetensors
    }

    fn device(&self) -> &Device {
        &self.device
    }

    fn forward(&mut self, input: &Tensor, index_pos: usize) -> Result<Tensor, candle_core::Error> {
        self.model.forward(input, index_pos)
    }

    fn prefill(&mut self, tokens: &[u32]) -> Result<Tensor, String> {
        if tokens.is_empty() {
            return Err("Empty token sequence".to_string());
        }

        let log = runtime::logger("candle");
        log.debug(&format!("Qwen3.5 prefill: {} tokens full-batch", tokens.len()));

        let input = Tensor::new(tokens, &self.device)
            .map_err(|e| format!("Tensor creation: {e}"))?
            .unsqueeze(0)
            .map_err(|e| format!("Unsqueeze: {e}"))?;

        let logits = self
            .model
            .forward(&input, 0)
            .map_err(|e| format!("Qwen3.5 forward pass: {e}"))?;

        self.device
            .synchronize()
            .map_err(|e| format!("GPU sync after prefill: {e}"))?;

        Ok(logits)
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        // TODO: implement KV cache clearing for Qwen3.5
        Ok(())
    }

    fn tokenize(&self, text: &str) -> Result<Vec<u32>, String> {
        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| format!("Tokenization failed: {e}"))?;
        Ok(encoding.get_ids().to_vec())
    }

    fn decode(&self, tokens: &[u32]) -> Result<String, String> {
        self.tokenizer
            .decode(tokens, true)
            .map_err(|e| format!("Decode failed: {e}"))
    }

    fn estimated_vram_bytes(&self) -> u64 {
        self.weight_paths
            .iter()
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum()
    }
}
