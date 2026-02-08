//! Moonshine STT Adapter
//!
//! Fast, lightweight speech-to-text using Moonshine ONNX models (Useful Sensors).
//! Compute scales linearly with audio length (unlike Whisper's fixed 30s window),
//! making it 5–15x faster on short utterances.
//!
//! Models: moonshine-tiny (~26 MB int8, 27M params), moonshine-base (~57 MB int8, 62M params)
//! Input: 16 kHz mono f32 PCM (variable length)
//! Architecture: encoder-decoder transformer with learned audio stem (not mel spectrogram)
//!
//! Model files (4-file sherpa-onnx format):
//!   preprocess.onnx, encode.int8.onnx, uncached_decode.int8.onnx, cached_decode.int8.onnx, tokens.txt
//!
//! Download from: https://huggingface.co/UsefulSensors/moonshine
//! Place in: models/moonshine/tiny/ or models/moonshine/base/

use super::{STTError, SpeechToText, TranscriptResult, TranscriptSegment};
use crate::audio_constants::AUDIO_SAMPLE_RATE;
use async_trait::async_trait;
use ndarray::{Array2, ArrayD, IxDyn};
use once_cell::sync::OnceCell;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};

// Token constants (SentencePiece, same as Llama)
const BOS_TOKEN_ID: i64 = 1;
const EOS_TOKEN_ID: i64 = 2;
const MAX_TOKENS: usize = 194; // max_position_embeddings

/// Global model (loaded once — ONNX sessions are expensive)
static MOONSHINE_MODEL: OnceCell<Arc<MoonshineModel>> = OnceCell::new();

/// Loaded Moonshine ONNX pipeline (4 sessions + vocabulary)
struct MoonshineModel {
    preprocess: Session,
    encoder: Session,
    uncached_decoder: Session,
    cached_decoder: Session,
    vocab: Vec<String>,
}

/// A KV cache tensor extracted as raw data + shape for recreation between steps.
/// ort v2 uses `ValueRef<'_>` from session outputs (not clonable),
/// so we extract → own → recreate each decode step.
struct CacheTensor {
    shape: Vec<usize>,
    data: Vec<f32>,
}

impl CacheTensor {
    /// Convert back to an ort Value for the next decoder step
    fn to_value(&self) -> Result<Value, STTError> {
        let array = ArrayD::from_shape_vec(IxDyn(&self.shape), self.data.clone())
            .map_err(|e| STTError::InferenceFailed(format!("KV cache reshape failed: {e}")))?;
        Value::from_array(array)
            .map(|v| v.into()) // upcast typed Value → DynValue
            .map_err(|e| STTError::InferenceFailed(format!("KV cache to Value failed: {e}")))
    }
}

/// Moonshine STT Adapter — local ONNX inference
pub struct MoonshineStt {
    model_dir: Option<PathBuf>,
}

impl MoonshineStt {
    pub fn new() -> Self {
        Self { model_dir: None }
    }

    pub fn with_model_dir(dir: PathBuf) -> Self {
        Self {
            model_dir: Some(dir),
        }
    }

    /// Model variant preference: tiny first (fast), base second (more accurate)
    const VARIANT_PREFERENCE: &'static [(&'static str, &'static str)] = &[
        ("tiny", "tiny"),
        ("base", "base"),
    ];

    /// Required files per model variant
    const REQUIRED_FILES: &'static [&'static str] = &[
        "preprocess.onnx",
        "encode.int8.onnx",
        "uncached_decode.int8.onnx",
        "cached_decode.int8.onnx",
        "tokens.txt",
    ];

    /// Search directories for model files
    fn model_search_dirs() -> Vec<PathBuf> {
        let mut dirs = vec![PathBuf::from("models/moonshine")];
        if let Some(data_dir) = dirs::data_dir() {
            dirs.push(data_dir.join("moonshine"));
        }
        dirs.push(PathBuf::from("/usr/local/share/moonshine"));
        dirs
    }

    /// Find a complete model directory on disk.
    ///
    /// Priority:
    /// 1. Explicit `model_dir` field (constructor override)
    /// 2. `MOONSHINE_MODEL` env var ("tiny" or "base")
    /// 3. Auto-select best available variant
    fn find_model_dir(&self) -> PathBuf {
        if let Some(ref dir) = self.model_dir {
            return dir.clone();
        }

        let search_dirs = Self::model_search_dirs();

        if let Ok(variant) = std::env::var("MOONSHINE_MODEL") {
            for dir in &search_dirs {
                let candidate = dir.join(&variant);
                if Self::dir_has_all_files(&candidate) {
                    info!(
                        "Moonshine: Using variant from MOONSHINE_MODEL env: {} ({:?})",
                        variant, candidate
                    );
                    return candidate;
                }
            }
            warn!(
                "Moonshine: MOONSHINE_MODEL='{}' set but files not found, falling back",
                variant
            );
        }

        for (name, subdir) in Self::VARIANT_PREFERENCE {
            for dir in &search_dirs {
                let candidate = dir.join(subdir);
                if Self::dir_has_all_files(&candidate) {
                    info!("Moonshine: Auto-selected variant: {} ({:?})", name, candidate);
                    return candidate;
                }
            }
        }

        warn!("Moonshine: No model files found. Download from:");
        warn!("  https://huggingface.co/UsefulSensors/moonshine");
        warn!("  Place onnx/tiny/ contents in: models/moonshine/tiny/");
        PathBuf::from("models/moonshine/tiny")
    }

    fn dir_has_all_files(dir: &Path) -> bool {
        Self::REQUIRED_FILES.iter().all(|f| dir.join(f).exists())
    }

    /// Load vocabulary from tokens.txt (one token per line, line number = token ID)
    fn load_vocab(dir: &Path) -> Result<Vec<String>, STTError> {
        let vocab_path = dir.join("tokens.txt");
        let content = std::fs::read_to_string(&vocab_path).map_err(|e| {
            STTError::ModelNotLoaded(format!("Failed to read tokens.txt: {e}"))
        })?;

        let tokens: Vec<String> = content
            .lines()
            .map(|line| {
                // sherpa-onnx tokens.txt format: "token id" per line — take just the token
                if let Some(space_idx) = line.rfind(' ') {
                    if line[space_idx + 1..].parse::<usize>().is_ok() {
                        return line[..space_idx].to_string();
                    }
                }
                line.to_string()
            })
            .collect();

        info!("Moonshine: Loaded vocabulary with {} tokens", tokens.len());
        Ok(tokens)
    }

    /// Decode token IDs to text using vocabulary
    fn decode_tokens(vocab: &[String], token_ids: &[i64]) -> String {
        let mut text = String::new();

        for &id in token_ids {
            let idx = id as usize;
            if idx >= vocab.len() {
                continue;
            }
            let token = &vocab[idx];

            // Skip special tokens
            if token == "<s>" || token == "</s>" || token == "<unk>" || token == "<pad>" {
                continue;
            }

            // Handle byte-level tokens: <0xHH>
            if token.starts_with("<0x") && token.ends_with('>') && token.len() == 6 {
                if let Ok(byte_val) = u8::from_str_radix(&token[3..5], 16) {
                    text.push(byte_val as char);
                    continue;
                }
            }

            // SentencePiece word boundary: ▁ → space
            if token.starts_with('▁') {
                text.push(' ');
                text.push_str(&token['\u{2581}'.len_utf8()..]);
            } else {
                text.push_str(token);
            }
        }

        text.trim().to_string()
    }

    /// Build an ONNX session with standard settings
    fn build_session(model_path: &Path) -> Result<Session, STTError> {
        let threads = num_cpus::get().min(4);
        Session::builder()
            .map_err(|e| STTError::ModelNotLoaded(format!("Session builder failed: {e}")))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| STTError::ModelNotLoaded(format!("Optimization level failed: {e}")))?
            .with_intra_threads(threads)
            .map_err(|e| STTError::ModelNotLoaded(format!("Thread config failed: {e}")))?
            .commit_from_file(model_path)
            .map_err(|e| {
                STTError::ModelNotLoaded(format!(
                    "Failed to load model {:?}: {e}",
                    model_path.file_name().unwrap_or_default()
                ))
            })
    }

    /// Extract a float tensor from session outputs by index as owned data + shape.
    /// ort v2 returns ValueRef (not clonable), so we copy out the raw data.
    fn extract_f32(
        outputs: &ort::session::SessionOutputs,
        index: usize,
    ) -> Result<CacheTensor, STTError> {
        let (shape, data) = outputs[index]
            .try_extract_raw_tensor::<f32>()
            .map_err(|e| STTError::InferenceFailed(format!("Tensor extraction at [{index}]: {e}")))?;
        Ok(CacheTensor {
            shape: shape.iter().map(|&s| s as usize).collect(),
            data: data.to_vec(),
        })
    }

    /// Convert a CacheTensor to an ndarray for ort::inputs! macro
    fn cache_to_array(ct: &CacheTensor) -> Result<ArrayD<f32>, STTError> {
        ArrayD::from_shape_vec(IxDyn(&ct.shape), ct.data.clone())
            .map_err(|e| STTError::InferenceFailed(format!("Array reshape: {e}")))
    }

    /// Synchronous transcription pipeline (runs on blocking thread)
    fn transcribe_sync(
        model: &MoonshineModel,
        samples: Vec<f32>,
    ) -> Result<TranscriptResult, STTError> {
        if samples.is_empty() {
            return Err(STTError::InvalidAudio("Empty audio samples".into()));
        }

        let num_samples = samples.len();
        let duration_ms = (num_samples as u64 * 1000) / AUDIO_SAMPLE_RATE as u64;

        // ── Step 1: Preprocess ─ raw audio → features ────────────────────
        let audio_input = Array2::from_shape_vec((1, num_samples), samples)
            .map_err(|e| STTError::InferenceFailed(format!("Audio input reshape: {e}")))?;

        let preprocess_out = model
            .preprocess
            .run(ort::inputs![audio_input].map_err(|e| {
                STTError::InferenceFailed(format!("Preprocess inputs: {e}"))
            })?)
            .map_err(|e| STTError::InferenceFailed(format!("Preprocess run: {e}")))?;

        let features = Self::extract_f32(&preprocess_out, 0)?;
        let features_array = Self::cache_to_array(&features)?;

        // ── Step 2: Encode ─ features → hidden states ────────────────────
        let encoder_out = model
            .encoder
            .run(ort::inputs![features_array].map_err(|e| {
                STTError::InferenceFailed(format!("Encoder inputs: {e}"))
            })?)
            .map_err(|e| STTError::InferenceFailed(format!("Encoder run: {e}")))?;

        let encoder_hidden = Self::extract_f32(&encoder_out, 0)?;

        // ── Step 3: Autoregressive decoding ──────────────────────────────
        let mut generated_tokens: Vec<i64> = Vec::with_capacity(MAX_TOKENS);
        let mut current_token = BOS_TOKEN_ID;

        // First decode step (uncached — no KV cache input)
        let token_input = Array2::from_shape_vec((1, 1), vec![current_token]).unwrap();
        let enc_array = Self::cache_to_array(&encoder_hidden)?;

        let uncached_out = model
            .uncached_decoder
            .run(ort::inputs![token_input, enc_array].map_err(|e| {
                STTError::InferenceFailed(format!("Uncached decoder inputs: {e}"))
            })?)
            .map_err(|e| STTError::InferenceFailed(format!("Uncached decoder run: {e}")))?;

        // Logits = output[0], KV cache = output[1..]
        let logits = Self::extract_f32(&uncached_out, 0)?;
        current_token = Self::argmax(&logits.data);

        if current_token == EOS_TOKEN_ID {
            return Ok(TranscriptResult {
                text: String::new(),
                language: "en".to_string(),
                confidence: 0.0,
                segments: vec![],
            });
        }
        generated_tokens.push(current_token);

        // Collect KV cache tensors
        let num_outputs = uncached_out.len();
        let mut kv_cache: Vec<CacheTensor> = (1..num_outputs)
            .map(|i| Self::extract_f32(&uncached_out, i))
            .collect::<Result<Vec<_>, _>>()?;

        // Subsequent decode steps (cached — with KV cache)
        for _step in 1..MAX_TOKENS {
            let token_input = Array2::from_shape_vec((1, 1), vec![current_token]).unwrap();
            let enc_array = Self::cache_to_array(&encoder_hidden)?;

            // Build named inputs: [token, encoder_hidden, kv_0, kv_1, ...]
            // ort v2: Value::from_array returns typed Value — .into() upcasts to DynValue
            let token_val: Value = Value::from_array(token_input)
                .map(|v| v.into())
                .map_err(|e| STTError::InferenceFailed(format!("Token value: {e}")))?;
            let enc_val: Value = Value::from_array(enc_array)
                .map(|v| v.into())
                .map_err(|e| STTError::InferenceFailed(format!("Encoder value: {e}")))?;

            let mut input_values: Vec<Value> = Vec::with_capacity(2 + kv_cache.len());
            input_values.push(token_val);
            input_values.push(enc_val);
            for ct in &kv_cache {
                input_values.push(ct.to_value()?);
            }

            // ort v2: session.run() needs named inputs — pair with model's input names
            let named_inputs: Vec<(String, Value)> = model
                .cached_decoder
                .inputs
                .iter()
                .map(|i| i.name.clone())
                .zip(input_values)
                .collect();

            let cached_out = model
                .cached_decoder
                .run(named_inputs)
                .map_err(|e| STTError::InferenceFailed(format!("Cached decoder run: {e}")))?;

            let logits = Self::extract_f32(&cached_out, 0)?;
            current_token = Self::argmax(&logits.data);

            if current_token == EOS_TOKEN_ID {
                break;
            }
            generated_tokens.push(current_token);

            // Update KV cache from new outputs
            let num_outputs = cached_out.len();
            kv_cache = (1..num_outputs)
                .map(|i| Self::extract_f32(&cached_out, i))
                .collect::<Result<Vec<_>, _>>()?;
        }

        // ── Step 4: Decode tokens → text ─────────────────────────────────
        let text = Self::decode_tokens(&model.vocab, &generated_tokens);

        info!(
            "Moonshine: Transcribed {}ms audio → {} tokens → \"{}\"",
            duration_ms,
            generated_tokens.len(),
            if text.len() > 80 {
                format!("{}...", &text[..80])
            } else {
                text.clone()
            }
        );

        Ok(TranscriptResult {
            text,
            language: "en".to_string(),
            confidence: 0.9, // Moonshine doesn't expose per-token confidence
            segments: vec![TranscriptSegment {
                text: String::new(), // Full text already in result.text
                start_ms: 0,
                end_ms: duration_ms as i64,
            }],
        })
    }

    /// Argmax over a flat f32 slice — returns the index of the maximum value
    fn argmax(data: &[f32]) -> i64 {
        data.iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(idx, _)| idx as i64)
            .unwrap_or(EOS_TOKEN_ID)
    }
}

impl Default for MoonshineStt {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SpeechToText for MoonshineStt {
    fn name(&self) -> &'static str {
        "moonshine"
    }

    fn description(&self) -> &'static str {
        "Moonshine STT (ONNX, Useful Sensors) — fast local inference, 5-15x faster than Whisper on short audio"
    }

    fn is_initialized(&self) -> bool {
        MOONSHINE_MODEL.get().is_some()
    }

    async fn initialize(&self) -> Result<(), STTError> {
        if MOONSHINE_MODEL.get().is_some() {
            info!("Moonshine: Already initialized");
            return Ok(());
        }

        let model_dir = self.find_model_dir();
        info!("Moonshine: Loading models from {:?}", model_dir);

        if !Self::dir_has_all_files(&model_dir) {
            let missing: Vec<&str> = Self::REQUIRED_FILES
                .iter()
                .filter(|f| !model_dir.join(f).exists())
                .copied()
                .collect();
            return Err(STTError::ModelNotLoaded(format!(
                "Missing model files in {model_dir:?}: {missing:?}. Download from https://huggingface.co/UsefulSensors/moonshine"
            )));
        }

        let preprocess = Self::build_session(&model_dir.join("preprocess.onnx"))?;
        let encoder = Self::build_session(&model_dir.join("encode.int8.onnx"))?;
        let uncached_decoder =
            Self::build_session(&model_dir.join("uncached_decode.int8.onnx"))?;
        let cached_decoder =
            Self::build_session(&model_dir.join("cached_decode.int8.onnx"))?;

        info!(
            "Moonshine: Uncached decoder has {} outputs ({} KV cache tensors)",
            uncached_decoder.outputs.len(),
            uncached_decoder.outputs.len().saturating_sub(1)
        );

        let vocab = Self::load_vocab(&model_dir)?;

        let model = MoonshineModel {
            preprocess,
            encoder,
            uncached_decoder,
            cached_decoder,
            vocab,
        };

        MOONSHINE_MODEL
            .set(Arc::new(model))
            .map_err(|_| STTError::ModelNotLoaded("Failed to set global model".into()))?;

        info!("Moonshine: All models loaded successfully");
        Ok(())
    }

    async fn transcribe(
        &self,
        samples: Vec<f32>,
        _language: Option<&str>,
    ) -> Result<TranscriptResult, STTError> {
        let model = MOONSHINE_MODEL
            .get()
            .ok_or_else(|| {
                STTError::ModelNotLoaded(
                    "Moonshine not initialized. Call initialize() first.".into(),
                )
            })?
            .clone();

        tokio::task::spawn_blocking(move || Self::transcribe_sync(&model, samples))
            .await
            .map_err(|e| STTError::InferenceFailed(format!("Task join error: {e}")))?
    }

    fn supported_languages(&self) -> Vec<&'static str> {
        vec!["en"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_moonshine_creation() {
        let adapter = MoonshineStt::new();
        assert_eq!(adapter.name(), "moonshine");
        assert!(!adapter.is_initialized());
    }

    #[test]
    fn test_explicit_model_dir() {
        let dir = PathBuf::from("/tmp/test-moonshine");
        let adapter = MoonshineStt::with_model_dir(dir.clone());
        assert_eq!(adapter.find_model_dir(), dir);
    }

    #[test]
    fn test_model_search_dirs_not_empty() {
        let dirs = MoonshineStt::model_search_dirs();
        assert!(!dirs.is_empty());
        assert!(dirs[0].ends_with("models/moonshine"));
    }

    #[test]
    fn test_decode_tokens_basic() {
        let vocab: Vec<String> = vec![
            "<unk>".into(),   // 0
            "<s>".into(),     // 1  (BOS)
            "</s>".into(),    // 2  (EOS)
            "▁Hello".into(),  // 3
            "▁world".into(),  // 4
            "!".into(),       // 5
        ];

        let tokens = vec![3, 4, 5];
        let text = MoonshineStt::decode_tokens(&vocab, &tokens);
        assert_eq!(text, "Hello world!");
    }

    #[test]
    fn test_decode_tokens_skips_special() {
        let vocab: Vec<String> = vec![
            "<unk>".into(),
            "<s>".into(),
            "</s>".into(),
            "▁Hi".into(),
        ];

        let tokens = vec![1, 3, 2];
        let text = MoonshineStt::decode_tokens(&vocab, &tokens);
        assert_eq!(text, "Hi");
    }

    #[test]
    fn test_decode_tokens_byte_fallback() {
        let vocab: Vec<String> = vec![
            "<unk>".into(),
            "<s>".into(),
            "</s>".into(),
            "<0x41>".into(), // 'A'
            "<0x42>".into(), // 'B'
        ];

        let tokens = vec![3, 4];
        let text = MoonshineStt::decode_tokens(&vocab, &tokens);
        assert_eq!(text, "AB");
    }

    #[test]
    fn test_argmax() {
        let data = vec![0.1, 0.5, 0.3, 0.8, 0.2];
        assert_eq!(MoonshineStt::argmax(&data), 3);
    }

    #[test]
    fn test_argmax_empty() {
        let data: Vec<f32> = vec![];
        assert_eq!(MoonshineStt::argmax(&data), EOS_TOKEN_ID);
    }

    #[test]
    fn test_variant_preference_order() {
        assert_eq!(MoonshineStt::VARIANT_PREFERENCE[0].0, "tiny");
        assert_eq!(MoonshineStt::VARIANT_PREFERENCE[1].0, "base");
    }

    #[test]
    fn test_required_files_count() {
        assert_eq!(MoonshineStt::REQUIRED_FILES.len(), 5);
    }
}
