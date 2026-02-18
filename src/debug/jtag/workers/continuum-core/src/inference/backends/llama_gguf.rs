//! Llama GGUF Backend
//!
//! Implements `ModelBackend` for Llama-family GGUF quantized models.
//! Uses vendored `quantized_llama.rs` with context_length fix.
//!
//! Supports:
//!   - Llama 2, 3, 3.1, 3.2 (all sizes)
//!   - Mistral (uses Llama architecture in GGUF)
//!   - CodeLlama
//!   - Any GGUF model with `general.architecture = "llama"`

use std::io::BufReader;
use std::path::{Path, PathBuf};

use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use tokenizers::Tokenizer;

use super::{ModelBackend, ModelFormat};
use crate::inference::vendored::quantized_llama::ModelWeights;
use crate::runtime;

/// GPU sync interval during token-by-token prefill.
const PREFILL_SYNC_INTERVAL: usize = 64;

/// Llama-family GGUF quantized backend.
///
/// Context length, EOS tokens — all from GGUF metadata.
/// Token-by-token prefill keeps every forward call at seq_len=1
/// (Metal SDPA fast path).
pub struct LlamaGgufBackend {
    model: ModelWeights,
    tokenizer: Tokenizer,
    context_length: usize,
    eos_token_ids: Vec<u32>,
    model_id: String,
    model_path: PathBuf,
    device: Device,
}

impl LlamaGgufBackend {
    /// Load from GGUF content + reader.
    ///
    /// Reads `llama.context_length` from metadata for RoPE table sizing.
    /// EOS tokens default to Llama 3 `<|eot_id|>` (128009).
    pub fn from_gguf<R: std::io::Seek + std::io::Read>(
        ct: gguf_file::Content,
        reader: &mut R,
        tokenizer: Tokenizer,
        model_id: &str,
        model_path: &Path,
        device: &Device,
    ) -> Result<Self, String> {
        let eos_token_ids = Self::read_eos_tokens(&ct);

        let model = ModelWeights::from_gguf(ct, reader, device)
            .map_err(|e| format!("Llama GGUF load failed: {e}"))?;

        let context_length = model.context_length;

        Ok(Self {
            model,
            tokenizer,
            context_length,
            eos_token_ids,
            model_id: model_id.to_string(),
            model_path: model_path.to_path_buf(),
            device: device.clone(),
        })
    }

    /// Read EOS token IDs from GGUF metadata.
    fn read_eos_tokens(ct: &gguf_file::Content) -> Vec<u32> {
        if let Some(eos) = ct
            .metadata
            .get("tokenizer.ggml.eos_token_id")
            .and_then(|v| v.to_u32().ok())
        {
            if eos == 128001 {
                // Llama 3 has TWO EOS: <|end_of_text|> (128001) + <|eot_id|> (128009)
                vec![128001, 128009]
            } else {
                vec![eos]
            }
        } else {
            vec![128009]
        }
    }

    /// Reload model weights from disk to clear KV cache.
    fn reload_weights(&mut self) -> Result<(), String> {
        let mut file = std::fs::File::open(&self.model_path)
            .map_err(|e| format!("Failed to open GGUF: {e}"))?;
        let content = gguf_file::Content::read(&mut file)
            .map_err(|e| format!("Failed to read GGUF: {e}"))?;

        let mut reader = BufReader::new(
            std::fs::File::open(&self.model_path)
                .map_err(|e| format!("Failed to reopen GGUF: {e}"))?,
        );

        self.model = ModelWeights::from_gguf(content, &mut reader, &self.device)
            .map_err(|e| format!("Llama GGUF reload failed: {e}"))?;

        Ok(())
    }
}

impl ModelBackend for LlamaGgufBackend {
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
        ModelFormat::Gguf
    }

    fn device(&self) -> &Device {
        &self.device
    }

    fn forward(&mut self, input: &Tensor, index_pos: usize) -> Result<Tensor, candle_core::Error> {
        self.model.forward(input, index_pos)
    }

    /// Token-by-token prefill (Metal SDPA safe path).
    ///
    /// Each forward call has seq_len=1, which uses the Metal SDPA kernel
    /// instead of the manual O(n²) attention path that corrupts at >1000 tokens.
    fn prefill(&mut self, tokens: &[u32]) -> Result<Tensor, String> {
        if tokens.is_empty() {
            return Err("Empty token sequence".to_string());
        }

        let log = runtime::logger("candle");
        log.debug(&format!(
            "Prefilling {} tokens one-at-a-time (Metal SDPA safe path)",
            tokens.len()
        ));

        let mut last_logits = None;
        for (pos, &token) in tokens.iter().enumerate() {
            let input = Tensor::new(&[token], &self.device)
                .map_err(|e| format!("Tensor creation at pos {pos}: {e}"))?
                .unsqueeze(0)
                .map_err(|e| format!("Unsqueeze at pos {pos}: {e}"))?;

            let logits = self
                .model
                .forward(&input, pos)
                .map_err(|e| format!("Forward at pos {pos}: {e}"))?;

            // GPU sync periodically to prevent command buffer explosion
            if (pos + 1) % PREFILL_SYNC_INTERVAL == 0 {
                self.device
                    .synchronize()
                    .map_err(|e| format!("GPU sync at pos {pos}: {e}"))?;
            }

            last_logits = Some(logits);
        }

        // Final sync
        self.device
            .synchronize()
            .map_err(|e| format!("GPU sync after prefill: {e}"))?;

        last_logits.ok_or_else(|| "Empty token sequence".to_string())
    }

    /// Clear KV cache by reloading model from disk.
    /// GGUF ModelWeights has internal per-layer kv_cache with no reset API.
    /// The GGUF file should be in OS page cache, making this fast (~2-3s).
    fn clear_cache(&mut self) -> Result<(), String> {
        self.reload_weights()
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
}
