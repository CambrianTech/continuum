//! Llama Safetensors Backend (BF16/FP32)
//!
//! Implements `ModelBackend` for non-quantized Llama models loaded from
//! HuggingFace safetensors format. Used for LoRA training and high-quality inference.
//!
//! Context length comes from `config.json` → `max_position_embeddings`.
//! No hardcoded values — the model file is the single source of truth.

use std::path::PathBuf;

use candle_core::{DType, Device, Tensor};
use candle_transformers::models::llama::{
    Cache, Config as LlamaModelConfig, Llama, LlamaEosToks,
};
use tokenizers::Tokenizer;

use super::{GenomeAdapter, ModelBackend, ModelFormat};
use crate::inference::model::rebuild_with_stacked_lora;
use crate::runtime;

/// BF16 full-batch prefill on Metal creates an O(n²) attention matrix.
/// At 150 tokens → 1s, at 3500 tokens → 55s. Unusable for interactive chat.
/// Cap to this limit so prefill stays under ~15s and 4 serialized personas
/// complete within the 180s timeout.
///
/// This is NOT the model's theoretical limit (which is 131072 for Llama 3.2 3B).
/// It's the practical throughput ceiling for BF16 full-batch attention on Metal.
/// Known at compile time — hardware constraint, not model property.
pub const BF16_PRACTICAL_CONTEXT: usize = 2048;

/// Llama safetensors (BF16/FP32) backend.
///
/// Full-precision model for LoRA training and high-quality inference.
/// Context length from `config.max_position_embeddings`, capped to
/// `BF16_PRACTICAL_CONTEXT` for Metal performance.
/// Full-batch prefill (BF16 has proper causal masking, no Metal SDPA issue).
pub struct LlamaSafetensorsBackend {
    model: Llama,
    cache: Cache,
    tokenizer: Tokenizer,
    device: Device,
    dtype: DType,
    config: LlamaModelConfig,
    model_id: String,
    eos_token_ids: Vec<u32>,
    context_length: usize,
    /// Original weight paths for LoRA rebuild.
    weight_paths: Vec<PathBuf>,
}

impl LlamaSafetensorsBackend {
    /// Create from already-loaded model components.
    ///
    /// This is the construction path from `model::load_model_by_id()`.
    /// Context length is read from `config.max_position_embeddings`.
    pub fn new(
        model: Llama,
        cache: Cache,
        tokenizer: Tokenizer,
        device: Device,
        dtype: DType,
        config: LlamaModelConfig,
        model_id: String,
        eos_token_ids: Vec<u32>,
        weight_paths: Vec<PathBuf>,
    ) -> Self {
        let context_length = config.max_position_embeddings.min(BF16_PRACTICAL_CONTEXT);

        Self {
            model,
            cache,
            tokenizer,
            device,
            dtype,
            config,
            model_id,
            eos_token_ids,
            context_length,
            weight_paths,
        }
    }

    /// Parse EOS token IDs from Llama config.
    pub fn parse_eos_tokens(eos: &Option<LlamaEosToks>) -> Vec<u32> {
        match eos {
            Some(LlamaEosToks::Single(id)) => vec![*id],
            Some(LlamaEosToks::Multiple(ids)) => ids.clone(),
            None => vec![128001, 128009],
        }
    }

    /// Access weight paths (needed by CandleAdapter for LoRA operations).
    pub fn weight_paths(&self) -> &[PathBuf] {
        &self.weight_paths
    }

    /// Access dtype (needed by CandleAdapter for LoRA operations).
    pub fn dtype(&self) -> DType {
        self.dtype
    }

    /// Access config (needed by CandleAdapter for LoRA operations).
    pub fn config(&self) -> &LlamaModelConfig {
        &self.config
    }
}

impl ModelBackend for LlamaSafetensorsBackend {
    fn architecture(&self) -> &str {
        "llama"
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
        let logits = self.model.forward(input, index_pos, &mut self.cache)?;
        // GPU sync after each forward to prevent Metal command buffer accumulation
        self.device.synchronize()?;
        Ok(logits)
    }

    /// Full-batch prefill (BF16 path).
    ///
    /// BF16 Llama has proper causal masking in the attention implementation,
    /// so full-batch processing is both correct and efficient on Metal.
    /// The GPU parallelizes the matrix multiplications across all tokens.
    fn prefill(&mut self, tokens: &[u32]) -> Result<Tensor, String> {
        if tokens.is_empty() {
            return Err("Empty token sequence".to_string());
        }

        let log = runtime::logger("candle");
        log.debug(&format!(
            "Prefilling {} tokens full-batch (BF16 causal masking)",
            tokens.len()
        ));

        let input = Tensor::new(tokens, &self.device)
            .map_err(|e| format!("Tensor creation: {e}"))?
            .unsqueeze(0)
            .map_err(|e| format!("Unsqueeze: {e}"))?;

        let logits = self.model
            .forward(&input, 0, &mut self.cache)
            .map_err(|e| format!("Forward pass: {e}"))?;

        self.device
            .synchronize()
            .map_err(|e| format!("GPU sync after prefill: {e}"))?;

        Ok(logits)
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        self.cache = Cache::new(true, self.dtype, &self.config, &self.device)
            .map_err(|e| format!("Cache creation failed: {e}"))?;
        Ok(())
    }

    fn tokenize(&self, text: &str) -> Result<Vec<u32>, String> {
        let encoding = self.tokenizer
            .encode(text, false)
            .map_err(|e| format!("Tokenization failed: {e}"))?;
        Ok(encoding.get_ids().to_vec())
    }

    fn decode(&self, tokens: &[u32]) -> Result<String, String> {
        self.tokenizer
            .decode(tokens, true)
            .map_err(|e| format!("Decode failed: {e}"))
    }

    // ── LoRA Support ──

    fn supports_lora(&self) -> bool {
        true
    }

    fn rebuild_with_lora(&mut self, adapters: &[GenomeAdapter]) -> Result<(), String> {
        let new_model = rebuild_with_stacked_lora(
            &self.weight_paths,
            &self.device,
            self.dtype,
            &self.config,
            adapters,
        )
        .map_err(|e| format!("LoRA rebuild failed: {e}"))?;

        self.model = new_model;
        self.clear_cache()?;

        runtime::logger("candle").info(&format!(
            "Rebuilt model with {} LoRA adapters",
            adapters.len()
        ));

        Ok(())
    }

    fn reload_base(&mut self) -> Result<(), String> {
        use candle_nn::VarBuilder;

        let log = runtime::logger("candle");
        log.info("Reloading base model (removing LoRA)");

        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&self.weight_paths, self.dtype, &self.device)
                .map_err(|e| format!("Failed to load weights: {e}"))?
        };

        self.model = Llama::load(vb, &self.config)
            .map_err(|e| format!("Failed to rebuild model: {e}"))?;

        self.clear_cache()
    }
}
