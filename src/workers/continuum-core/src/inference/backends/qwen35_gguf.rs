//! Qwen3.5 GGUF Backend
//!
//! Implements `ModelBackend` for Qwen3.5 hybrid DeltaNet+Attention GGUF models.
//! Uses vendored `quantized_qwen35.rs` for the forward pass.
//!
//! Supports:
//!   - Qwen3.5-0.6B through Qwen3.5-235B (any size with qwen35 architecture)
//!   - Hybrid DeltaNet (24 layers) + full attention (8 layers)
//!   - Partial RoPE (rope_dim < head_dim)
//!   - continuum-ai forged models (qwen3.5-4b-code-forged, etc.)

use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use tokenizers::Tokenizer;

use super::{
    GenomeAdapter, GpuMemoryManager, GpuPriority, GpuSubsystem, ModelBackend, ModelFormat,
};
use crate::inference::vendored::quantized_qwen35::ModelWeights;
use crate::runtime;

pub struct Qwen35GgufBackend {
    model: ModelWeights,
    tokenizer: Tokenizer,
    context_length: usize,
    eos_token_ids: Vec<u32>,
    suppress_token_ids: Vec<u32>,
    model_id: String,
    model_path: PathBuf,
    device: Device,
}

impl Qwen35GgufBackend {
    pub fn from_gguf<R: std::io::Seek + std::io::Read>(
        ct: gguf_file::Content,
        reader: &mut R,
        tokenizer: Tokenizer,
        model_id: &str,
        model_path: &Path,
        device: &Device,
    ) -> Result<Self, String> {
        let eos_token_ids = Self::read_eos_tokens(&ct);
        let suppress_token_ids = Self::read_suppress_tokens(&ct);

        let model = ModelWeights::from_gguf(ct, reader, device)
            .map_err(|e| format!("Qwen3.5 GGUF load failed: {e}"))?;

        let context_length = model.context_length;

        Ok(Self {
            model,
            tokenizer,
            context_length,
            eos_token_ids,
            suppress_token_ids,
            model_id: model_id.to_string(),
            model_path: model_path.to_path_buf(),
            device: device.clone(),
        })
    }

    fn read_eos_tokens(ct: &gguf_file::Content) -> Vec<u32> {
        // Qwen3.5 uses <|im_end|> (151645) as EOS, same as Qwen2.
        let base_eos = ct
            .metadata
            .get("tokenizer.ggml.eos_token_id")
            .and_then(|v| v.to_u32().ok());

        base_eos.map(|e| vec![e]).unwrap_or_else(|| vec![151645])
    }

    fn read_suppress_tokens(ct: &gguf_file::Content) -> Vec<u32> {
        // Suppress <|endoftext|> (151643) and <|im_start|> (151644)
        // Same as Qwen2 — inflated logits in quantized variants.
        vec![151643, 151644]
    }

    fn reload_weights(&mut self) -> Result<(), String> {
        let mut file = std::fs::File::open(&self.model_path)
            .map_err(|e| format!("Failed to open GGUF: {e}"))?;
        let content =
            gguf_file::Content::read(&mut file).map_err(|e| format!("Failed to read GGUF: {e}"))?;

        let mut reader = BufReader::new(
            std::fs::File::open(&self.model_path)
                .map_err(|e| format!("Failed to reopen GGUF: {e}"))?,
        );

        self.model = ModelWeights::from_gguf(content, &mut reader, &self.device)
            .map_err(|e| format!("Qwen3.5 GGUF reload failed: {e}"))?;

        Ok(())
    }
}

impl ModelBackend for Qwen35GgufBackend {
    fn architecture(&self) -> &str {
        "qwen35"
    }

    fn suppress_token_ids(&self) -> &[u32] {
        &self.suppress_token_ids
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
        ModelFormat::Gguf
    }

    fn device(&self) -> &Device {
        &self.device
    }

    fn estimated_vram_bytes(&self) -> u64 {
        std::fs::metadata(&self.model_path)
            .map(|m| m.len())
            .unwrap_or(0)
    }

    fn forward(&mut self, input: &Tensor, index_pos: usize) -> Result<Tensor, candle_core::Error> {
        self.model.forward_from_ids(input, index_pos)
    }

    fn prefill(&mut self, tokens: &[u32]) -> Result<Tensor, String> {
        if tokens.is_empty() {
            return Err("Empty token sequence".to_string());
        }

        let log = runtime::logger("candle");
        log.debug(&format!(
            "Qwen3.5 batch prefilling {} tokens",
            tokens.len()
        ));

        let input = Tensor::new(tokens, &self.device)
            .map_err(|e| format!("Tensor creation: {e}"))?
            .unsqueeze(0)
            .map_err(|e| format!("Unsqueeze: {e}"))?;

        let logits = self
            .model
            .forward_from_ids(&input, 0)
            .map_err(|e| format!("Qwen3.5 prefill forward: {e}"))?;

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

    fn supports_lora(&self) -> bool {
        false // TODO: LoRA support for hybrid DeltaNet+Attention needs tensor name mapping
    }

    fn rebuild_with_lora(
        &mut self,
        _adapters: &[GenomeAdapter],
        _gpu_manager: Option<&Arc<GpuMemoryManager>>,
    ) -> Result<(), String> {
        Err("LoRA not yet supported for Qwen3.5 hybrid architecture".to_string())
    }

    fn reload_base(&mut self) -> Result<(), String> {
        self.reload_weights()
    }
}
