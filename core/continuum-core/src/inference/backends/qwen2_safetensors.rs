//! Qwen2 Safetensors Backend
//!
//! Implements `ModelBackend` for Qwen2 architecture models loaded from
//! safetensors format. Supports both standard and uniformly-pruned models
//! (compacted models just have smaller head counts in config.json).
//!
//! Key difference from Llama: Q and K projections have bias.

use std::path::PathBuf;

use candle_core::{DType, Device, Tensor};
use tokenizers::Tokenizer;

use super::{ModelBackend, ModelFormat};
use crate::inference::vendored::qwen2::Qwen2;
use crate::runtime;

/// Qwen2 safetensors backend.
#[allow(dead_code)]
pub(crate) struct Qwen2SafetensorsBackend {
    model: Qwen2,
    tokenizer: Tokenizer,
    device: Device,
    dtype: DType,
    model_id: String,
    eos_token_ids: Vec<u32>,
    context_length: usize,
    weight_paths: Vec<PathBuf>,
}

impl Qwen2SafetensorsBackend {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        model: Qwen2,
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

impl ModelBackend for Qwen2SafetensorsBackend {
    fn architecture(&self) -> &str {
        "qwen2"
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

    /// Full-batch prefill (BF16 has proper causal masking).
    fn prefill(&mut self, tokens: &[u32]) -> Result<Tensor, String> {
        if tokens.is_empty() {
            return Err("Empty token sequence".to_string());
        }

        let log = runtime::logger("candle");
        log.debug(&format!(
            "Qwen2 prefill: {} tokens full-batch",
            tokens.len()
        ));

        let input = Tensor::new(tokens, &self.device)
            .map_err(|e| format!("Tensor creation: {e}"))?
            .unsqueeze(0)
            .map_err(|e| format!("Unsqueeze: {e}"))?;

        let logits = self
            .model
            .forward(&input, 0)
            .map_err(|e| format!("Qwen2 forward pass: {e}"))?;

        self.device
            .synchronize()
            .map_err(|e| format!("GPU sync after prefill: {e}"))?;

        Ok(logits)
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        self.model.clear_cache();
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

    fn supports_lora(&self) -> bool {
        false // TODO: LoRA support for Qwen2
    }
}
