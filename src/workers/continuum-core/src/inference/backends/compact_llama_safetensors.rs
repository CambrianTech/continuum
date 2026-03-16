//! Compact Llama Safetensors Backend
//!
//! Implements `ModelBackend` for plasticity-compacted Llama models loaded from
//! safetensors format with a HeadTopology manifest. Each layer can have a
//! different number of attention heads — standard Llama won't work because it
//! expects uniform head counts across all layers.
//!
//! Auto-detected when `head_topology.json` exists alongside model weights.
//! Uses `CompactLlama` from `vendored/compact_llama.rs` for the actual
//! per-layer variable-dimension inference.

use std::path::PathBuf;
use std::sync::Arc;

use candle_core::{DType, Device, Tensor};
use candle_transformers::models::llama::{Config as LlamaModelConfig, LlamaEosToks};
use tokenizers::Tokenizer;

use super::{GenomeAdapter, GpuMemoryManager, ModelBackend, ModelFormat};
use crate::inference::vendored::compact_llama::CompactLlama;
use crate::modules::plasticity::types::HeadTopology;
use crate::runtime;

/// Compact Llama safetensors backend — per-layer variable head counts.
///
/// Loaded when a `head_topology.json` is detected alongside model weights.
/// Uses CompactLlama instead of standard Llama for the forward pass, reading
/// per-layer attention dimensions from the topology manifest.
pub struct CompactLlamaSafetensorsBackend {
    model: CompactLlama,
    tokenizer: Tokenizer,
    device: Device,
    dtype: DType,
    config: LlamaModelConfig,
    topology: HeadTopology,
    model_id: String,
    eos_token_ids: Vec<u32>,
    context_length: usize,
    /// Original weight paths for VRAM estimation.
    weight_paths: Vec<PathBuf>,
}

impl CompactLlamaSafetensorsBackend {
    /// Create from already-loaded compact model components.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        model: CompactLlama,
        tokenizer: Tokenizer,
        device: Device,
        dtype: DType,
        config: LlamaModelConfig,
        topology: HeadTopology,
        model_id: String,
        eos_token_ids: Vec<u32>,
        weight_paths: Vec<PathBuf>,
    ) -> Self {
        let context_length = config.max_position_embeddings;

        Self {
            model,
            tokenizer,
            device,
            dtype,
            config,
            topology,
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

    /// Access topology for external inspection.
    pub fn topology(&self) -> &HeadTopology {
        &self.topology
    }

    /// Access weight paths.
    pub fn weight_paths(&self) -> &[PathBuf] {
        &self.weight_paths
    }

    /// Access dtype.
    pub fn dtype(&self) -> DType {
        self.dtype
    }

    /// Access config.
    pub fn config(&self) -> &LlamaModelConfig {
        &self.config
    }
}

impl ModelBackend for CompactLlamaSafetensorsBackend {
    fn architecture(&self) -> &str {
        "llama-compact"
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

    /// Full-batch prefill (same as standard safetensors — BF16 causal masking works).
    fn prefill(&mut self, tokens: &[u32]) -> Result<Tensor, String> {
        if tokens.is_empty() {
            return Err("Empty token sequence".to_string());
        }

        let log = runtime::logger("candle");
        log.debug(&format!(
            "Compact prefill: {} tokens full-batch ({}% parameter reduction)",
            tokens.len(),
            (self.topology.parameter_reduction * 100.0) as u32
        ));

        let input = Tensor::new(tokens, &self.device)
            .map_err(|e| format!("Tensor creation: {e}"))?
            .unsqueeze(0)
            .map_err(|e| format!("Unsqueeze: {e}"))?;

        let logits = self
            .model
            .forward(&input, 0)
            .map_err(|e| format!("Compact forward pass: {e}"))?;

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
        // Compacted model is smaller — use actual file sizes
        self.weight_paths
            .iter()
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum()
    }

    // ── LoRA Support ──

    fn supports_lora(&self) -> bool {
        // TODO: Implement topology-aware LoRA rebuild for compacted models.
        // The LoRA adapter dimensions must match the compacted weight dimensions.
        // This requires mapping LoRA layer names through the topology's
        // retained_head_indices to slice the LoRA weights to match.
        false
    }

    fn rebuild_with_lora(
        &mut self,
        _adapters: &[GenomeAdapter],
        _gpu_manager: Option<&Arc<GpuMemoryManager>>,
    ) -> Result<(), String> {
        Err("LoRA rebuild not yet supported for compact models. \
             Compact the base model first, then train new LoRA adapters \
             targeting the compacted dimensions."
            .to_string())
    }
}
