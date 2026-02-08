//! Orpheus TTS Adapter
//!
//! Llama-3B fine-tuned for expressive speech synthesis with emotion tags.
//! Uses Candle for LLM inference (GGUF quantized) + SNAC neural audio codec decoder (ONNX).
//!
//! Pipeline: text → Llama tokenize → autoregressive audio token generation → SNAC decode → 24kHz PCM → resample 16kHz
//!
//! Features:
//! - Emotion control via inline tags: `<laugh>`, `<sigh>`, `<gasp>`, `<cry>`, etc.
//! - 8 built-in voices (4 female, 4 male)
//! - GGUF quantized (~2GB Q4_K_M) for fast CPU/GPU inference
//!
//! Model files (place in models/orpheus/):
//!   - model-q4_k_m.gguf (or similar quantized GGUF)
//!   - tokenizer.json (Orpheus-extended Llama 3 tokenizer)
//!   - snac_decoder.onnx (SNAC 24kHz neural audio codec decoder)
//!
//! Download from: https://huggingface.co/canopylabs/orpheus-3b-0.1-ft
//! SNAC decoder: https://huggingface.co/hubertsiuzdak/snac_24khz

use super::{SynthesisResult, TTSError, TextToSpeech, VoiceInfo};
use crate::audio_constants::AUDIO_SAMPLE_RATE;
use async_trait::async_trait;
use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::quantized_llama::ModelWeights;
use ndarray::Array2;
use once_cell::sync::OnceCell;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Value;
use parking_lot::Mutex;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokenizers::Tokenizer;
use tracing::{info, warn};

// ─── Orpheus Token Constants ──────────────────────────────────────────────────
// Audio tokens extend the Llama 3 vocabulary starting at this offset.
// 3 SNAC codebooks × 4096 codes each = 12288 audio tokens.
const AUDIO_TOKEN_OFFSET: u32 = 128266;
const CODEBOOK_SIZE: u32 = 4096;
const NUM_CODEBOOKS: usize = 3;
const TOKENS_PER_FRAME: usize = 7; // 1 coarse + 2 mid + 4 fine per audio frame

/// SNAC native sample rate — Orpheus generates 24kHz audio
const SNAC_SAMPLE_RATE: u32 = 24000;

/// Max audio tokens to generate (prevents runaway generation).
/// 7 tokens/frame × ~10 frames/sec = ~70 tokens/sec of audio.
/// 2100 tokens ≈ 30 seconds max.
const MAX_AUDIO_TOKENS: usize = 2100;

/// Temperature for audio token sampling (Orpheus default)
const DEFAULT_TEMPERATURE: f64 = 0.6;

/// Top-p for audio token sampling
const DEFAULT_TOP_P: f64 = 0.95;

// ─── Orpheus Voices ───────────────────────────────────────────────────────────
const VOICES: &[(&str, &str, &str)] = &[
    ("tara", "Tara", "female"),
    ("leah", "Leah", "female"),
    ("jess", "Jess", "female"),
    ("mia", "Mia", "female"),
    ("leo", "Leo", "male"),
    ("dan", "Dan", "male"),
    ("zac", "Zac", "male"),
    ("zoe", "Zoe", "female"),
];

// ─── Global Model (Mutex because ModelWeights::forward needs &mut self) ──────
static ORPHEUS_MODEL: OnceCell<Arc<Mutex<OrpheusModel>>> = OnceCell::new();

/// Loaded Orpheus model pipeline
struct OrpheusModel {
    llm: ModelWeights,
    tokenizer: Tokenizer,
    snac_decoder: Session,
    device: Device,
    /// Token ID for `<|audio_end|>` (EOS for audio generation)
    audio_end_token_id: u32,
}

/// Orpheus TTS Adapter — Llama-3B speech synthesis with emotion tags
pub struct OrpheusTts {
    model_dir: Option<PathBuf>,
}

impl OrpheusTts {
    pub fn new() -> Self {
        Self { model_dir: None }
    }

    pub fn with_model_dir(dir: PathBuf) -> Self {
        Self {
            model_dir: Some(dir),
        }
    }

    /// Required model files
    const REQUIRED_FILES: &'static [&'static str] = &[
        "tokenizer.json",
        "snac_decoder.onnx",
        // GGUF file is found by glob (name varies by quantization)
    ];

    /// Search directories for model files
    fn model_search_dirs() -> Vec<PathBuf> {
        let mut dirs = vec![PathBuf::from("models/orpheus")];
        if let Some(data_dir) = dirs::data_dir() {
            dirs.push(data_dir.join("orpheus"));
        }
        dirs.push(PathBuf::from("/usr/local/share/orpheus"));
        dirs
    }

    /// Find model directory with all required files
    fn find_model_dir(&self) -> PathBuf {
        if let Some(ref dir) = self.model_dir {
            return dir.clone();
        }

        let search_dirs = Self::model_search_dirs();

        if let Ok(dir) = std::env::var("ORPHEUS_MODEL_DIR") {
            let p = PathBuf::from(&dir);
            if Self::dir_has_required_files(&p) {
                info!("Orpheus: Using model dir from ORPHEUS_MODEL_DIR: {:?}", p);
                return p;
            }
            warn!("Orpheus: ORPHEUS_MODEL_DIR='{}' set but files not found", dir);
        }

        for dir in &search_dirs {
            if Self::dir_has_required_files(dir) {
                info!("Orpheus: Found model dir: {:?}", dir);
                return dir.clone();
            }
        }

        warn!("Orpheus: No model files found. Download from:");
        warn!("  Model: https://huggingface.co/canopylabs/orpheus-3b-0.1-ft");
        warn!("  SNAC:  https://huggingface.co/hubertsiuzdak/snac_24khz");
        warn!("  Place files in: models/orpheus/");
        PathBuf::from("models/orpheus")
    }

    fn dir_has_required_files(dir: &Path) -> bool {
        Self::REQUIRED_FILES.iter().all(|f| dir.join(f).exists())
            && Self::find_gguf_file(dir).is_some()
    }

    /// Find the GGUF model file in a directory (name varies by quantization)
    fn find_gguf_file(dir: &Path) -> Option<PathBuf> {
        let rd = std::fs::read_dir(dir).ok()?;
        for entry in rd.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "gguf" {
                    return Some(path);
                }
            }
        }
        None
    }

    /// Select the best compute device (Metal > CPU)
    fn select_device() -> Device {
        // Try Metal GPU first (Apple Silicon) — candle handles availability at runtime
        match Device::new_metal(0) {
            Ok(device) => {
                info!("Orpheus: Using Metal GPU");
                device
            }
            Err(_) => {
                info!("Orpheus: Using CPU (with Accelerate BLAS)");
                Device::Cpu
            }
        }
    }

    /// Build SNAC decoder ONNX session
    fn build_snac_session(model_path: &Path) -> Result<Session, TTSError> {
        let threads = num_cpus::get().min(4);
        Session::builder()
            .map_err(|e| TTSError::ModelNotLoaded(format!("SNAC session builder: {e}")))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| TTSError::ModelNotLoaded(format!("SNAC optimization: {e}")))?
            .with_intra_threads(threads)
            .map_err(|e| TTSError::ModelNotLoaded(format!("SNAC threads: {e}")))?
            .commit_from_file(model_path)
            .map_err(|e| TTSError::ModelNotLoaded(format!("SNAC model load {model_path:?}: {e}")))
    }

    /// Look up a special token ID from the tokenizer
    fn find_token_id(tokenizer: &Tokenizer, token: &str) -> Result<u32, TTSError> {
        tokenizer
            .token_to_id(token)
            .ok_or_else(|| {
                TTSError::ModelNotLoaded(format!(
                    "Token '{token}' not found in Orpheus tokenizer. \
                     Ensure you're using the Orpheus-specific tokenizer.json, \
                     not the base Llama tokenizer."
                ))
            })
    }

    /// Format the Orpheus prompt for TTS generation
    fn format_prompt(text: &str, voice: &str) -> String {
        // Orpheus prompt format: voice name on first line, then text, wrapped in special tokens
        format!("<|text_start|>{voice}\n{text}<|text_end|><|audio_start|>")
    }

    /// Synchronous synthesis pipeline (runs on blocking thread)
    fn synthesize_sync(
        model: &mut OrpheusModel,
        text: &str,
        voice: &str,
    ) -> Result<SynthesisResult, TTSError> {
        if text.is_empty() {
            return Err(TTSError::InvalidText("Empty text".into()));
        }

        let prompt = Self::format_prompt(text, voice);

        // ── Step 1: Tokenize ──────────────────────────────────────────────
        let encoding = model
            .tokenizer
            .encode(prompt.as_str(), true)
            .map_err(|e| TTSError::SynthesisFailed(format!("Tokenization failed: {e}")))?;

        let prompt_tokens: Vec<u32> = encoding.get_ids().to_vec();
        let prompt_len = prompt_tokens.len();
        info!(
            "Orpheus: Prompt tokenized to {} tokens for voice '{}'",
            prompt_len, voice
        );

        // ── Step 2: Autoregressive generation ─────────────────────────────
        let audio_tokens =
            Self::generate_audio_tokens(model, &prompt_tokens)?;

        if audio_tokens.is_empty() {
            return Err(TTSError::SynthesisFailed(
                "No audio tokens generated (model produced EOS immediately)".into(),
            ));
        }

        info!(
            "Orpheus: Generated {} audio tokens ({} frames)",
            audio_tokens.len(),
            audio_tokens.len() / TOKENS_PER_FRAME
        );

        // ── Step 3: Redistribute into SNAC codebook layers ────────────────
        let layers = Self::redistribute_codes(&audio_tokens)?;

        // ── Step 4: SNAC decode → 24kHz PCM ───────────────────────────────
        let pcm_24k = Self::snac_decode(&model.snac_decoder, &layers)?;

        info!(
            "Orpheus: SNAC decoded {} samples ({:.2}s at 24kHz)",
            pcm_24k.len(),
            pcm_24k.len() as f64 / SNAC_SAMPLE_RATE as f64
        );

        // ── Step 5: Resample 24kHz → 16kHz ───────────────────────────────
        let pcm_16k = Self::resample(&pcm_24k, SNAC_SAMPLE_RATE, AUDIO_SAMPLE_RATE)?;

        // ── Step 6: f32 → i16 ─────────────────────────────────────────────
        let samples: Vec<i16> = pcm_16k
            .iter()
            .map(|&s| {
                let clamped = s.clamp(-1.0, 1.0);
                (clamped * 32767.0) as i16
            })
            .collect();

        let duration_ms = (samples.len() as u64 * 1000) / AUDIO_SAMPLE_RATE as u64;

        info!(
            "Orpheus: Synthesized \"{}\" → {}ms audio",
            if text.len() > 50 {
                format!("{}...", &text[..50])
            } else {
                text.to_string()
            },
            duration_ms
        );

        Ok(SynthesisResult {
            samples,
            sample_rate: AUDIO_SAMPLE_RATE,
            duration_ms,
        })
    }

    /// Autoregressive audio token generation with the Llama model
    fn generate_audio_tokens(
        model: &mut OrpheusModel,
        prompt_tokens: &[u32],
    ) -> Result<Vec<u32>, TTSError> {
        let seed = rand::random::<u64>();
        let mut logits_processor =
            LogitsProcessor::new(seed, Some(DEFAULT_TEMPERATURE), Some(DEFAULT_TOP_P));

        let mut all_tokens: Vec<u32> = prompt_tokens.to_vec();
        let mut audio_tokens: Vec<u32> = Vec::with_capacity(MAX_AUDIO_TOKENS);

        // Prefill: process entire prompt in one forward pass
        let input = Tensor::new(prompt_tokens, &model.device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| TTSError::SynthesisFailed(format!("Prompt tensor: {e}")))?;

        let logits = model
            .llm
            .forward(&input, 0)
            .map_err(|e| TTSError::SynthesisFailed(format!("LLM prefill: {e}")))?;

        model
            .device
            .synchronize()
            .map_err(|e| TTSError::SynthesisFailed(format!("GPU sync: {e}")))?;

        // Sample first token from last position
        let last_logits = Self::extract_last_logits(&logits)?;
        let mut next_token = logits_processor
            .sample(&last_logits)
            .map_err(|e| TTSError::SynthesisFailed(format!("Sampling: {e}")))?;

        all_tokens.push(next_token);
        if Self::is_audio_token(next_token) {
            audio_tokens.push(next_token);
        }

        // Token-by-token generation
        for step in 1..MAX_AUDIO_TOKENS {
            if next_token == model.audio_end_token_id {
                break;
            }

            let input = Tensor::new(&[next_token], &model.device)
                .and_then(|t| t.unsqueeze(0))
                .map_err(|e| TTSError::SynthesisFailed(format!("Token tensor: {e}")))?;

            let pos = all_tokens.len() - 1;
            let logits = model
                .llm
                .forward(&input, pos)
                .map_err(|e| {
                    TTSError::SynthesisFailed(format!("LLM step {step}: {e}"))
                })?;

            // Sync GPU periodically (every 16 tokens) to prevent command buffer buildup
            if step % 16 == 0 {
                model
                    .device
                    .synchronize()
                    .map_err(|e| TTSError::SynthesisFailed(format!("GPU sync: {e}")))?;
            }

            let last_logits = Self::extract_last_logits(&logits)?;
            next_token = logits_processor
                .sample(&last_logits)
                .map_err(|e| TTSError::SynthesisFailed(format!("Sampling: {e}")))?;

            all_tokens.push(next_token);
            if Self::is_audio_token(next_token) {
                audio_tokens.push(next_token);
            }
        }

        // Final GPU sync
        model
            .device
            .synchronize()
            .map_err(|e| TTSError::SynthesisFailed(format!("Final GPU sync: {e}")))?;

        Ok(audio_tokens)
    }

    /// Extract logits for the last token position from the model output
    fn extract_last_logits(logits: &Tensor) -> Result<Tensor, TTSError> {
        let dims = logits.dims();
        let result = match dims.len() {
            2 => logits.squeeze(0),
            3 => {
                let squeezed = logits.squeeze(0).map_err(|e| {
                    TTSError::SynthesisFailed(format!("Squeeze logits: {e}"))
                })?;
                let seq_len = squeezed.dims()[0];
                if seq_len > 1 {
                    squeezed.get(seq_len - 1)
                } else {
                    squeezed.squeeze(0)
                }
            }
            _ => {
                return Err(TTSError::SynthesisFailed(format!(
                    "Unexpected logits shape: {dims:?}"
                )));
            }
        };
        result.map_err(|e| TTSError::SynthesisFailed(format!("Extract logits: {e}")))
    }

    /// Check if a token ID is in the audio token range
    fn is_audio_token(token_id: u32) -> bool {
        token_id >= AUDIO_TOKEN_OFFSET
            && token_id < AUDIO_TOKEN_OFFSET + (NUM_CODEBOOKS as u32) * CODEBOOK_SIZE
    }

    /// Redistribute flat audio token sequence into 3 SNAC codebook layers.
    ///
    /// Orpheus outputs audio tokens in the pattern: [c1, c2a, c2b, c3a, c3b, c3c, c3d]
    /// repeating for each audio frame. This splits them into 3 separate codebook streams.
    fn redistribute_codes(audio_tokens: &[u32]) -> Result<[Vec<i64>; NUM_CODEBOOKS], TTSError> {
        // Must be a multiple of TOKENS_PER_FRAME (7)
        let usable_len = (audio_tokens.len() / TOKENS_PER_FRAME) * TOKENS_PER_FRAME;
        if usable_len == 0 {
            return Err(TTSError::SynthesisFailed(format!(
                "Too few audio tokens for a complete frame (got {}, need at least {})",
                audio_tokens.len(),
                TOKENS_PER_FRAME
            )));
        }

        let num_frames = usable_len / TOKENS_PER_FRAME;
        let mut layer0: Vec<i64> = Vec::with_capacity(num_frames);
        let mut layer1: Vec<i64> = Vec::with_capacity(num_frames * 2);
        let mut layer2: Vec<i64> = Vec::with_capacity(num_frames * 4);

        for frame_idx in 0..num_frames {
            let base = frame_idx * TOKENS_PER_FRAME;
            let tokens = &audio_tokens[base..base + TOKENS_PER_FRAME];

            // Extract codebook values (strip offset, mod codebook size)
            let code = |t: u32| -> i64 { ((t - AUDIO_TOKEN_OFFSET) % CODEBOOK_SIZE) as i64 };

            // Pattern: [c1, c2a, c2b, c3a, c3b, c3c, c3d]
            layer0.push(code(tokens[0]));
            layer1.push(code(tokens[1]));
            layer1.push(code(tokens[2]));
            layer2.push(code(tokens[3]));
            layer2.push(code(tokens[4]));
            layer2.push(code(tokens[5]));
            layer2.push(code(tokens[6]));
        }

        Ok([layer0, layer1, layer2])
    }

    /// Decode SNAC codebook layers → 24kHz PCM audio using ONNX decoder
    fn snac_decode(
        session: &Session,
        layers: &[Vec<i64>; NUM_CODEBOOKS],
    ) -> Result<Vec<f32>, TTSError> {
        // Build input tensors for each codebook layer: [1, seq_len]
        let mut named_inputs: Vec<(String, Value)> = Vec::with_capacity(NUM_CODEBOOKS);

        for (i, layer) in layers.iter().enumerate() {
            let seq_len = layer.len();
            let array = Array2::from_shape_vec((1, seq_len), layer.clone()).map_err(|e| {
                TTSError::SynthesisFailed(format!("SNAC input layer {i} reshape: {e}"))
            })?;
            let value: Value = Value::from_array(array)
                .map(|v| v.into())
                .map_err(|e| {
                    TTSError::SynthesisFailed(format!("SNAC input layer {i} to value: {e}"))
                })?;

            // Use model's input names (discovered at runtime)
            let name = session.inputs[i].name.clone();
            named_inputs.push((name, value));
        }

        let outputs = session.run(named_inputs).map_err(|e| {
            TTSError::SynthesisFailed(format!("SNAC decoder run: {e}"))
        })?;

        // Extract output audio waveform (f32)
        let (shape, data) = outputs[0]
            .try_extract_raw_tensor::<f32>()
            .map_err(|e| TTSError::SynthesisFailed(format!("SNAC output extraction: {e}")))?;

        info!(
            "Orpheus: SNAC output shape: {:?} ({} samples)",
            shape,
            data.len()
        );

        Ok(data.to_vec())
    }

    /// Resample audio from source rate to target rate using rubato
    fn resample(
        samples: &[f32],
        from_rate: u32,
        to_rate: u32,
    ) -> Result<Vec<f32>, TTSError> {
        if from_rate == to_rate {
            return Ok(samples.to_vec());
        }

        use rubato::{FftFixedInOut, Resampler};

        let chunk_size = 1024;
        let mut resampler = FftFixedInOut::<f32>::new(
            from_rate as usize,
            to_rate as usize,
            chunk_size,
            1, // mono
        )
        .map_err(|e| TTSError::SynthesisFailed(format!("Resampler init: {e}")))?;

        let mut output = Vec::with_capacity(
            (samples.len() as f64 * to_rate as f64 / from_rate as f64) as usize + chunk_size,
        );

        // Process in chunks
        let input_frames_per_chunk = resampler.input_frames_next();
        let mut pos = 0;

        while pos + input_frames_per_chunk <= samples.len() {
            let chunk = &samples[pos..pos + input_frames_per_chunk];
            let result = resampler
                .process(&[chunk], None)
                .map_err(|e| TTSError::SynthesisFailed(format!("Resample chunk: {e}")))?;
            output.extend_from_slice(&result[0]);
            pos += input_frames_per_chunk;
        }

        // Handle remaining samples (zero-pad to fill last chunk)
        if pos < samples.len() {
            let remaining = &samples[pos..];
            let mut padded = vec![0.0f32; input_frames_per_chunk];
            padded[..remaining.len()].copy_from_slice(remaining);
            let result = resampler
                .process(&[&padded], None)
                .map_err(|e| TTSError::SynthesisFailed(format!("Resample tail: {e}")))?;
            // Only take proportional output for the non-padded input
            let output_samples =
                (remaining.len() as f64 * to_rate as f64 / from_rate as f64) as usize;
            let take = output_samples.min(result[0].len());
            output.extend_from_slice(&result[0][..take]);
        }

        Ok(output)
    }
}

impl Default for OrpheusTts {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TextToSpeech for OrpheusTts {
    fn name(&self) -> &'static str {
        "orpheus"
    }

    fn description(&self) -> &'static str {
        "Orpheus TTS (Llama-3B, GGUF) — expressive speech with emotion tags <laugh> <sigh> <gasp>"
    }

    fn is_initialized(&self) -> bool {
        ORPHEUS_MODEL.get().is_some()
    }

    async fn initialize(&self) -> Result<(), TTSError> {
        if ORPHEUS_MODEL.get().is_some() {
            info!("Orpheus: Already initialized");
            return Ok(());
        }

        let model_dir = self.find_model_dir();
        info!("Orpheus: Loading models from {:?}", model_dir);

        // Check required files
        if !Self::dir_has_required_files(&model_dir) {
            let mut missing: Vec<String> = Self::REQUIRED_FILES
                .iter()
                .filter(|f| !model_dir.join(f).exists())
                .map(|f| f.to_string())
                .collect();
            if Self::find_gguf_file(&model_dir).is_none() {
                missing.push("*.gguf (any quantized model file)".to_string());
            }
            return Err(TTSError::ModelNotLoaded(format!(
                "Missing model files in {model_dir:?}: {missing:?}. Download from https://huggingface.co/canopylabs/orpheus-3b-0.1-ft"
            )));
        }

        // Load tokenizer
        let tokenizer_path = model_dir.join("tokenizer.json");
        let tokenizer = Tokenizer::from_file(&tokenizer_path).map_err(|e| {
            TTSError::ModelNotLoaded(format!("Tokenizer load failed: {e}"))
        })?;
        info!("Orpheus: Tokenizer loaded ({} tokens)", tokenizer.get_vocab_size(true));

        // Look up special token IDs
        let audio_end_token_id = Self::find_token_id(&tokenizer, "<|audio_end|>")?;
        info!("Orpheus: audio_end token ID = {}", audio_end_token_id);

        // Select compute device
        let device = Self::select_device();

        // Load GGUF model
        let gguf_path = Self::find_gguf_file(&model_dir).ok_or_else(|| {
            TTSError::ModelNotLoaded("No .gguf file found in model directory".into())
        })?;
        info!("Orpheus: Loading GGUF model from {:?}", gguf_path);

        let mut gguf_file = std::fs::File::open(&gguf_path).map_err(|e| {
            TTSError::ModelNotLoaded(format!("Failed to open GGUF file: {e}"))
        })?;
        let gguf_content = gguf_file::Content::read(&mut gguf_file).map_err(|e| {
            TTSError::ModelNotLoaded(format!("Failed to read GGUF content: {e}"))
        })?;

        let mut reader = BufReader::new(std::fs::File::open(&gguf_path).map_err(|e| {
            TTSError::ModelNotLoaded(format!("Failed to reopen GGUF file: {e}"))
        })?);

        let llm = ModelWeights::from_gguf(gguf_content, &mut reader, &device).map_err(|e| {
            TTSError::ModelNotLoaded(format!("GGUF model load failed: {e}"))
        })?;
        info!("Orpheus: Llama model loaded on {:?}", device);

        // Load SNAC decoder
        let snac_path = model_dir.join("snac_decoder.onnx");
        let snac_decoder = Self::build_snac_session(&snac_path)?;
        info!(
            "Orpheus: SNAC decoder loaded ({} inputs, {} outputs)",
            snac_decoder.inputs.len(),
            snac_decoder.outputs.len()
        );

        let model = OrpheusModel {
            llm,
            tokenizer,
            snac_decoder,
            device,
            audio_end_token_id,
        };

        ORPHEUS_MODEL
            .set(Arc::new(Mutex::new(model)))
            .map_err(|_| TTSError::ModelNotLoaded("Failed to set global model".into()))?;

        info!("Orpheus: All models loaded successfully");
        Ok(())
    }

    async fn synthesize(&self, text: &str, voice: &str) -> Result<SynthesisResult, TTSError> {
        let model_arc = ORPHEUS_MODEL
            .get()
            .ok_or_else(|| {
                TTSError::ModelNotLoaded(
                    "Orpheus not initialized. Call initialize() first.".into(),
                )
            })?
            .clone();

        // Validate voice
        let voice = if VOICES.iter().any(|(id, _, _)| *id == voice) {
            voice.to_string()
        } else {
            // Use default voice for unknown voice IDs
            info!(
                "Orpheus: Unknown voice '{}', using default 'tara'",
                voice
            );
            "tara".to_string()
        };

        let text = text.to_string();

        // Run on blocking thread (CPU-bound LLM inference)
        tokio::task::spawn_blocking(move || {
            let mut model = model_arc.lock();
            Self::synthesize_sync(&mut model, &text, &voice)
        })
        .await
        .map_err(|e| TTSError::SynthesisFailed(format!("Task join error: {e}")))?
    }

    fn available_voices(&self) -> Vec<VoiceInfo> {
        VOICES
            .iter()
            .map(|(id, name, gender)| VoiceInfo {
                id: id.to_string(),
                name: name.to_string(),
                language: "en".to_string(),
                gender: Some(gender.to_string()),
                description: Some(format!(
                    "Orpheus {gender} voice — supports emotion tags"
                )),
            })
            .collect()
    }

    fn default_voice(&self) -> &str {
        "tara"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orpheus_creation() {
        let adapter = OrpheusTts::new();
        assert_eq!(adapter.name(), "orpheus");
        assert!(!adapter.is_initialized());
    }

    #[test]
    fn test_explicit_model_dir() {
        let dir = PathBuf::from("/tmp/test-orpheus");
        let adapter = OrpheusTts::with_model_dir(dir.clone());
        assert_eq!(adapter.find_model_dir(), dir);
    }

    #[test]
    fn test_model_search_dirs_not_empty() {
        let dirs = OrpheusTts::model_search_dirs();
        assert!(!dirs.is_empty());
        assert!(dirs[0].ends_with("models/orpheus"));
    }

    #[test]
    fn test_is_audio_token() {
        // Below range
        assert!(!OrpheusTts::is_audio_token(128265));
        // Start of range
        assert!(OrpheusTts::is_audio_token(AUDIO_TOKEN_OFFSET));
        // Middle of range
        assert!(OrpheusTts::is_audio_token(AUDIO_TOKEN_OFFSET + 6000));
        // End of range (3 codebooks × 4096 - 1)
        assert!(OrpheusTts::is_audio_token(
            AUDIO_TOKEN_OFFSET + NUM_CODEBOOKS as u32 * CODEBOOK_SIZE - 1
        ));
        // Above range
        assert!(!OrpheusTts::is_audio_token(
            AUDIO_TOKEN_OFFSET + NUM_CODEBOOKS as u32 * CODEBOOK_SIZE
        ));
    }

    #[test]
    fn test_redistribute_codes() {
        // Create 2 frames of audio tokens (14 tokens)
        let mut audio_tokens = Vec::new();
        for frame in 0..2u32 {
            let base = frame * 7;
            // codebook 0: values 0-4095
            audio_tokens.push(AUDIO_TOKEN_OFFSET + base);
            // codebook 1: values 0-4095, offset by CODEBOOK_SIZE
            audio_tokens.push(AUDIO_TOKEN_OFFSET + CODEBOOK_SIZE + base + 1);
            audio_tokens.push(AUDIO_TOKEN_OFFSET + CODEBOOK_SIZE + base + 2);
            // codebook 2: values 0-4095, offset by 2*CODEBOOK_SIZE
            audio_tokens.push(AUDIO_TOKEN_OFFSET + 2 * CODEBOOK_SIZE + base + 3);
            audio_tokens.push(AUDIO_TOKEN_OFFSET + 2 * CODEBOOK_SIZE + base + 4);
            audio_tokens.push(AUDIO_TOKEN_OFFSET + 2 * CODEBOOK_SIZE + base + 5);
            audio_tokens.push(AUDIO_TOKEN_OFFSET + 2 * CODEBOOK_SIZE + base + 6);
        }

        let layers = OrpheusTts::redistribute_codes(&audio_tokens).unwrap();

        // Layer 0: 1 value per frame = 2 values
        assert_eq!(layers[0].len(), 2);
        // Layer 1: 2 values per frame = 4 values
        assert_eq!(layers[1].len(), 4);
        // Layer 2: 4 values per frame = 8 values
        assert_eq!(layers[2].len(), 8);

        // Check first frame values (mod CODEBOOK_SIZE)
        assert_eq!(layers[0][0], 0); // frame 0, code 0
        assert_eq!(layers[1][0], 1); // frame 0, c2a
        assert_eq!(layers[1][1], 2); // frame 0, c2b
        assert_eq!(layers[2][0], 3); // frame 0, c3a
        assert_eq!(layers[2][1], 4); // frame 0, c3b
        assert_eq!(layers[2][2], 5); // frame 0, c3c
        assert_eq!(layers[2][3], 6); // frame 0, c3d
    }

    #[test]
    fn test_redistribute_codes_too_few_tokens() {
        let tokens = vec![AUDIO_TOKEN_OFFSET; 5]; // Less than 7
        let result = OrpheusTts::redistribute_codes(&tokens);
        assert!(result.is_err());
    }

    #[test]
    fn test_redistribute_codes_truncates_partial_frame() {
        // 10 tokens: 1 complete frame (7) + 3 leftover (discarded)
        let tokens: Vec<u32> = (0..10).map(|i| AUDIO_TOKEN_OFFSET + i).collect();
        let layers = OrpheusTts::redistribute_codes(&tokens).unwrap();
        assert_eq!(layers[0].len(), 1); // Only 1 complete frame
    }

    #[test]
    fn test_format_prompt() {
        let prompt = OrpheusTts::format_prompt("Hello world", "tara");
        assert_eq!(
            prompt,
            "<|text_start|>tara\nHello world<|text_end|><|audio_start|>"
        );
    }

    #[test]
    fn test_available_voices() {
        let adapter = OrpheusTts::new();
        let voices = adapter.available_voices();
        assert_eq!(voices.len(), 8);

        let names: Vec<&str> = voices.iter().map(|v| v.id.as_str()).collect();
        assert!(names.contains(&"tara"));
        assert!(names.contains(&"leo"));
        assert!(names.contains(&"zoe"));
    }

    #[test]
    fn test_default_voice() {
        let adapter = OrpheusTts::new();
        assert_eq!(adapter.default_voice(), "tara");
    }

    #[test]
    fn test_required_files() {
        assert!(OrpheusTts::REQUIRED_FILES.contains(&"tokenizer.json"));
        assert!(OrpheusTts::REQUIRED_FILES.contains(&"snac_decoder.onnx"));
    }
}
