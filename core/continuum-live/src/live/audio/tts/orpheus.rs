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

use super::normalize;
use super::{SynthesisResult, TTSError, TextToSpeech, VoiceInfo};
use crate::gpu::memory_manager::{GpuPriority, GpuSubsystem};
use crate::gpu::tracker::GpuModelTracker;
use crate::inference::vendored::quantized_llama::ModelWeights;
use crate::live::audio::reloadable::ReloadableModel;
use crate::{clog_info, clog_warn};
use async_trait::async_trait;
use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::generation::LogitsProcessor;
use ndarray::Array2;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::{Tensor as OrtTensor, Value};
use parking_lot::Mutex;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use tokenizers::Tokenizer;

mod protocol;
use protocol::{Protocol, FRAME_LAYOUT, NUM_CODEBOOKS};

/// SNAC native sample rate — Orpheus generates 24kHz audio
const SNAC_SAMPLE_RATE: u32 = 24000;

/// Sampling params — the canonical canopylabs values (verified 2026-09-02 via
/// continuum web/fetch of the reference impl: temperature=0.4, top_p=0.9,
/// repetition_penalty=1.1). The prior 0.6/0.95/no-penalty collapsed
/// generation early (short/empty audio) — rep-penalty is what keeps the
/// autoregressive audio stream from stalling into an early end-token.
const DEFAULT_TEMPERATURE: f64 = 0.4;
const DEFAULT_TOP_P: f64 = 0.9;
const REPEAT_PENALTY: f32 = 1.1;
/// Rep-penalty context window (reference applies over the running generation).
const REPEAT_LAST_N: usize = 64;

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
static ORPHEUS_MODEL: ReloadableModel<Mutex<OrpheusModel>> = ReloadableModel::new("Orpheus");

/// GPU allocation tracking — Orpheus has TWO models on GPU
static ORPHEUS_LLM_GPU: GpuModelTracker = GpuModelTracker::new("Orpheus LLM");
static ORPHEUS_SNAC_GPU: GpuModelTracker = GpuModelTracker::new("Orpheus SNAC");

/// Loaded Orpheus model pipeline
struct OrpheusModel {
    llm: ModelWeights,
    tokenizer: Tokenizer,
    snac_decoder: Session,
    device: Device,
    protocol: Protocol,
    /// Frames per SNAC decode, read from the decoder's own coarse input: `None`
    /// for a dynamic-axis export, which decodes the whole utterance at once.
    snac_window: Option<usize>,
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

    /// Required model files. The tokenizer is NOT one of them: the GGUF
    /// carries its own token table + merges, and the upstream tokenizer.json
    /// is HF-gated (401, measured 2026-09-02) — one artifact, no gated
    /// sidecar ([`Self::tokenizer_from_gguf`]).
    const REQUIRED_FILES: &'static [&'static str] = &[
        "snac_decoder.onnx",
        // GGUF file is found by glob (name varies by quantization)
    ];

    /// Build the tokenizer FROM the GGUF's own metadata (`tokenizer.ggml.tokens`
    /// + `tokenizer.ggml.merges`) — the model file is the single source of its
    /// own vocabulary. Byte-level BPE per `tokenizer.ggml.pre = "llama-bpe"`;
    /// special tokens never pass through encode (prompt framing is id-level),
    /// so only plain text takes this path.
    fn tokenizer_from_gguf(
        content: &gguf_file::Content,
    ) -> Result<(Tokenizer, Protocol), TTSError> {
        use gguf_file::Value as V;
        let meta_arr = |key: &str| -> Result<&Vec<V>, TTSError> {
            match content.metadata.get(key) {
                Some(V::Array(a)) => Ok(a),
                other => Err(TTSError::ModelNotLoaded(format!(
                    "GGUF missing {key} (got {other:?}) — cannot build tokenizer"
                ))),
            }
        };
        let tokens = meta_arr("tokenizer.ggml.tokens")?;
        let merges = meta_arr("tokenizer.ggml.merges")?;
        let mut vocab = tokenizers::models::bpe::Vocab::with_capacity(tokens.len());
        for (i, t) in tokens.iter().enumerate() {
            let s = t.to_string().map_err(|e| {
                TTSError::ModelNotLoaded(format!("GGUF token {i} not a string: {e}"))
            })?;
            vocab.insert(s.clone(), i as u32);
        }
        // SCHEME VERIFICATION, fail-loud: every framing id comes from this
        // vocabulary by name, so a different fine-tune fails here.
        let protocol = Protocol::resolve(|name| vocab.get(name).copied(), &protocol::SNAC_24KHZ)?;
        let merge_pairs: Vec<(String, String)> = merges
            .iter()
            .filter_map(|m| {
                let s = m.to_string().ok()?;
                let (a, b) = s.split_once(' ')?;
                Some((a.to_string(), b.to_string()))
            })
            .collect();
        let bpe = tokenizers::models::bpe::BpeBuilder::new()
            .vocab_and_merges(vocab, merge_pairs)
            // llama-bpe: exact-vocab matches win over merge walks (HF llama-3
            // tokenizer.json sets the same flag).
            .ignore_merges(true)
            .build()
            .map_err(|e| TTSError::ModelNotLoaded(format!("BPE build from GGUF: {e}")))?;
        let mut tk = Tokenizer::new(tokenizers::ModelWrapper::BPE(bpe));
        tk.with_pre_tokenizer(Some(
            tokenizers::pre_tokenizers::byte_level::ByteLevel::new(false, true, true),
        ));
        tk.with_decoder(Some(tokenizers::decoders::byte_level::ByteLevel::new(
            false, true, true,
        )));
        Ok((tk, protocol))
    }

    /// Search directories for model files
    fn model_search_dirs() -> Vec<PathBuf> {
        let mut dirs = vec![crate::live::audio::model_root::voice_model_path("orpheus")];
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
                clog_info!("Orpheus: Using model dir from ORPHEUS_MODEL_DIR: {:?}", p);
                return p;
            }
            clog_warn!(
                "Orpheus: ORPHEUS_MODEL_DIR='{}' set but files not found",
                dir
            );
        }

        for dir in &search_dirs {
            if Self::dir_has_required_files(dir) {
                clog_info!("Orpheus: Found model dir: {:?}", dir);
                return dir.clone();
            }
        }

        clog_warn!("Orpheus: No model files found. Download from:");
        clog_warn!("  Model: https://huggingface.co/canopylabs/orpheus-3b-0.1-ft");
        clog_warn!("  SNAC:  https://huggingface.co/hubertsiuzdak/snac_24khz");
        clog_warn!("  Place files in: models/orpheus/");
        crate::live::audio::model_root::voice_model_path("orpheus")
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

    /// Acquire the Metal GPU device for Orpheus inference. Fail-closed:
    /// no CPU fallback. Per CLAUDE.md off-main-thread rule + Joel's
    /// 2026-05-16 audit (vhsm-d1f4 flagged this exact site), TTS is
    /// GPU-only — any CPU path silently saturates the render loop and
    /// produces the 900%-CPU pathology seen during chat.
    ///
    /// If Metal isn't available, surface the candle error up so the
    /// caller can decide policy (refuse to load, surface to operator,
    /// pick a CPU-acceptable TTS engine if one is registered). The
    /// previous `Device::Cpu` fallback evaded the codified
    /// no-CPU-fallback contract by being on the Candle side rather
    /// than llamacpp/ort.
    fn select_device() -> Result<Device, TTSError> {
        let device = Device::new_metal(0).map_err(|e| {
            TTSError::ModelNotLoaded(format!(
                "Orpheus requires Metal GPU; no CPU fallback. \
                 Device::new_metal(0) failed: {e}"
            ))
        })?;
        clog_info!("Orpheus: Using Metal GPU");
        Ok(device)
    }

    /// Build SNAC decoder ONNX session
    fn build_snac_session(model_path: &Path) -> Result<Session, TTSError> {
        let threads = num_cpus::get().min(4);
        // GPU execution providers via the centralized helper (#985 / #964).
        // Per architecture, CPU fallback is forbidden — SNAC decoder must
        // run on GPU. Pre-this-PR Orpheus never configured an EP at all,
        // so ORT's implicit CPU EP took every op silently.
        let providers = crate::inference::ort_providers::build_ort_gpu_execution_providers()
            .map_err(|e| {
                TTSError::ModelNotLoaded(format!("ORT GPU EP setup failed (Orpheus SNAC): {e}"))
            })?;
        Session::builder()
            .map_err(|e| TTSError::ModelNotLoaded(format!("SNAC session builder: {e}")))?
            .with_execution_providers(providers)
            .map_err(|e| TTSError::ModelNotLoaded(format!("SNAC EP register: {e}")))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| TTSError::ModelNotLoaded(format!("SNAC optimization: {e}")))?
            .with_intra_threads(threads)
            .map_err(|e| TTSError::ModelNotLoaded(format!("SNAC threads: {e}")))?
            .commit_from_file(model_path)
            .map_err(|e| TTSError::ModelNotLoaded(format!("SNAC model load {model_path:?}: {e}")))
    }

    /// Frames per decode call, from the decoder's own input shapes (the export
    /// we ship fixes 12, measured 2026-09-02 from its dimension error:
    /// `codes0 … Expected: 12`; see [`protocol::snac_window`]).
    fn snac_window(session: &Session) -> Result<Option<usize>, TTSError> {
        let shapes = session
            .inputs()
            .iter()
            .map(|input| input.dtype().tensor_shape().map(|shape| shape.to_vec()))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| TTSError::ModelNotLoaded("SNAC decoder input is not a tensor".into()))?;
        protocol::snac_window(&shapes).map_err(TTSError::ModelNotLoaded)
    }

    /// Build the id-level Orpheus prompt: `[SOH] ++ encode("{voice}: {text}")
    /// ++ [EOT, EOH]`. Special tokens go in AS IDS — never as strings through
    /// the encoder (the byte-level BPE would shred them into text bytes).
    fn build_prompt_ids(
        tokenizer: &Tokenizer,
        protocol: &Protocol,
        text: &str,
        voice: &str,
    ) -> Result<Vec<u32>, TTSError> {
        let enc = tokenizer
            .encode(format!("{voice}: {text}"), false)
            .map_err(|e| TTSError::SynthesisFailed(format!("Tokenization failed: {e}")))?;
        let mut ids = Vec::with_capacity(enc.get_ids().len() + 3);
        ids.push(protocol.human_start);
        ids.extend_from_slice(enc.get_ids());
        ids.extend_from_slice(&[
            protocol.text_end,
            protocol.human_end,
            protocol.third_end,
            protocol.audio_start,
        ]);
        Ok(ids)
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

        // ── Step 1: Build the id-level prompt (real canopylabs framing) ───
        let prompt_tokens: Vec<u32> = Self::build_prompt_ids(&model.tokenizer, &model.protocol, text, voice)?;
        let prompt_len = prompt_tokens.len();
        clog_info!(
            "Orpheus: Prompt tokenized to {} tokens for voice '{}'",
            prompt_len,
            voice
        );

        // ── Step 2: Autoregressive generation ─────────────────────────────
        let audio_tokens = Self::generate_audio_tokens(model, &prompt_tokens)?;

        if audio_tokens.is_empty() {
            return Err(TTSError::SynthesisFailed(
                "No audio tokens generated (model produced EOS immediately)".into(),
            ));
        }

        clog_info!(
            "Orpheus: Generated {} audio tokens ({} frames)",
            audio_tokens.len(),
            audio_tokens.len() / FRAME_LAYOUT.len()
        );

        // ── Step 3: Redistribute into SNAC codebook layers ────────────────
        let layers = Self::redistribute_codes(&model.protocol, &audio_tokens)?;

        // ── Step 4: SNAC decode → 24kHz PCM ───────────────────────────────
        let pcm_24k = Self::snac_decode(&mut model.snac_decoder, model.snac_window, &layers)?;

        clog_info!(
            "Orpheus: SNAC decoded {} samples ({:.2}s at 24kHz)",
            pcm_24k.len(),
            pcm_24k.len() as f64 / SNAC_SAMPLE_RATE as f64
        );

        // ── Step 5: Normalize to standard 16kHz i16 PCM ──────────────────
        let result = normalize::normalize_audio(&pcm_24k, SNAC_SAMPLE_RATE)?;

        clog_info!(
            "Orpheus: Synthesized \"{}\" → {}ms audio",
            super::truncate_str(text, 50),
            result.duration_ms
        );

        Ok(result)
    }

    /// Autoregressive audio token generation with the Llama model
    fn generate_audio_tokens(
        model: &mut OrpheusModel,
        prompt_tokens: &[u32],
    ) -> Result<Vec<u32>, TTSError> {
        let seed = rand::random::<u64>();
        let mut logits_processor =
            LogitsProcessor::new(seed, Some(DEFAULT_TEMPERATURE), Some(DEFAULT_TOP_P));

        // The model's own trained window bounds a runaway generation: its rope
        // table ends there (ModelWeights::context_length, from the GGUF).
        let room = model.llm.context_length.saturating_sub(prompt_tokens.len());
        if room == 0 {
            return Err(TTSError::InvalidText(format!(
                "prompt of {} tokens fills the model's {}-token window",
                prompt_tokens.len(),
                model.llm.context_length
            )));
        }
        let mut all_tokens: Vec<u32> = prompt_tokens.to_vec();
        let mut audio_tokens: Vec<u32> = Vec::new();

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
        let last_logits = Self::penalize(&Self::extract_last_logits(&logits)?, &all_tokens)?;
        let mut next_token = logits_processor
            .sample(&last_logits)
            .map_err(|e| TTSError::SynthesisFailed(format!("Sampling: {e}")))?;

        all_tokens.push(next_token);
        if model.protocol.is_audio(next_token) {
            audio_tokens.push(next_token);
        }

        // Token-by-token generation
        for step in 1..room {
            if next_token == model.protocol.audio_end {
                break;
            }

            let input = Tensor::new(&[next_token], &model.device)
                .and_then(|t| t.unsqueeze(0))
                .map_err(|e| TTSError::SynthesisFailed(format!("Token tensor: {e}")))?;

            let pos = all_tokens.len() - 1;
            let logits = model
                .llm
                .forward(&input, pos)
                .map_err(|e| TTSError::SynthesisFailed(format!("LLM step {step}: {e}")))?;

            // Sync GPU periodically (every 16 tokens) to prevent command buffer buildup
            if step % 16 == 0 {
                model
                    .device
                    .synchronize()
                    .map_err(|e| TTSError::SynthesisFailed(format!("GPU sync: {e}")))?;
            }

            let last_logits = Self::penalize(&Self::extract_last_logits(&logits)?, &all_tokens)?;
            next_token = logits_processor
                .sample(&last_logits)
                .map_err(|e| TTSError::SynthesisFailed(format!("Sampling: {e}")))?;

            all_tokens.push(next_token);
            if model.protocol.is_audio(next_token) {
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
    /// Apply repetition penalty over the trailing generation window — the
    /// reference's 1.1 penalty is what keeps the audio stream from stalling
    /// into an early end-token (the short-audio failure, 2026-09-02).
    fn penalize(logits: &Tensor, context: &[u32]) -> Result<Tensor, TTSError> {
        let start = context.len().saturating_sub(REPEAT_LAST_N);
        candle_transformers::utils::apply_repeat_penalty(logits, REPEAT_PENALTY, &context[start..])
            .map_err(|e| TTSError::SynthesisFailed(format!("Repeat penalty: {e}")))
    }

    fn extract_last_logits(logits: &Tensor) -> Result<Tensor, TTSError> {
        let dims = logits.dims();
        let result = match dims.len() {
            2 => logits.squeeze(0),
            3 => {
                let squeezed = logits
                    .squeeze(0)
                    .map_err(|e| TTSError::SynthesisFailed(format!("Squeeze logits: {e}")))?;
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

    /// Redistribute flat audio token sequence into 3 SNAC codebook layers.
    ///
    /// Orpheus outputs audio tokens in the pattern: [c1, c2a, c2b, c3a, c3b, c3c, c3d]
    /// repeating for each audio frame. This splits them into 3 separate codebook streams.
    fn redistribute_codes(
        protocol: &Protocol,
        audio_tokens: &[u32],
    ) -> Result<[Vec<i64>; NUM_CODEBOOKS], TTSError> {
        // Only whole frames decode; a partial last frame is dropped.
        let frames = audio_tokens.chunks_exact(FRAME_LAYOUT.len());
        if frames.len() == 0 {
            return Err(TTSError::SynthesisFailed(format!(
                "Too few audio tokens for a complete frame (got {}, need at least {})",
                audio_tokens.len(),
                FRAME_LAYOUT.len()
            )));
        }
        let mut layers: [Vec<i64>; NUM_CODEBOOKS] = Default::default();
        for frame in frames {
            for (token, layer) in frame.iter().zip(FRAME_LAYOUT) {
                layers[layer].push(protocol.code(*token));
            }
        }
        Ok(layers)
    }

    /// Decode SNAC codebook layers → 24kHz PCM audio using the ONNX decoder,
    /// `window` frames per call (layers scale 1×/2×/4× per SNAC's hierarchy).
    fn snac_decode(
        session: &mut Session,
        window: Option<usize>,
        layers: &[Vec<i64>; NUM_CODEBOOKS],
    ) -> Result<Vec<f32>, TTSError> {
        // CHUNKED decode: the ONNX export takes a fixed 12-frame window (the
        // first live utterance generated 299 frames and the decoder refused
        // the whole strip). Feed 12-frame windows — 12/24/48 codes per layer —
        // and concatenate PCM. The final partial window is DROPPED, not
        // zero-padded: padding synthesizes a click of silence-codes at the
        // clip tail; losing <12 frames (~140ms) of trailing audio is the
        // honest trade until a dynamic-axis export replaces this decoder.
        let total_frames = layers[0].len();
        if total_frames == 0 {
            return Err(TTSError::SynthesisFailed(
                "No audio frames to decode".into(),
            ));
        }
        // PAD the final partial window by repeating the last frame's codes —
        // a held codec frame briefly extends the last sound (benign), unlike
        // zero-codes which click. The output PCM is TRIMMED back to the true
        // frame count below, so short utterances (a 3-frame selftest phrase,
        // measured 2026-09-02) and clip tails both survive losslessly.
        let frames_per_call = match window {
            Some(frames) => frames,
            None => total_frames,
        };
        let mut padded: [Vec<i64>; NUM_CODEBOOKS] = layers.clone();
        let windows = total_frames.div_ceil(frames_per_call);
        let padded_frames = windows * frames_per_call;
        for (i, layer) in padded.iter_mut().enumerate() {
            let per_frame = 1usize << i;
            let last = layer[layer.len() - per_frame..].to_vec();
            while layer.len() < padded_frames * per_frame {
                layer.extend_from_slice(&last);
            }
        }
        let layers = &padded;
        let mut pcm: Vec<f32> = Vec::new();
        for w in 0..windows {
            let mut named_inputs: Vec<(String, Value)> = Vec::with_capacity(NUM_CODEBOOKS);
            for (i, layer) in layers.iter().enumerate() {
                // Layer i carries 2^i codes per frame (1/2/4 — SNAC hierarchy).
                let per_frame = 1usize << i;
                let start = w * frames_per_call * per_frame;
                let len = frames_per_call * per_frame;
                let chunk = layer[start..start + len].to_vec();
                let array = Array2::from_shape_vec((1, len), chunk).map_err(|e| {
                    TTSError::SynthesisFailed(format!("SNAC input layer {i} reshape: {e}"))
                })?;
                let value: Value = OrtTensor::from_array(array)
                    .map(|v| v.into())
                    .map_err(|e| {
                        TTSError::SynthesisFailed(format!("SNAC input layer {i} to value: {e}"))
                    })?;
                let name = session.inputs()[i].name().to_string();
                named_inputs.push((name, value));
            }
            let outputs = session
                .run(named_inputs)
                .map_err(|e| TTSError::SynthesisFailed(format!("SNAC decoder run: {e}")))?;
            let (_shape, data) = outputs[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| TTSError::SynthesisFailed(format!("SNAC output extraction: {e}")))?;
            pcm.extend_from_slice(data);
        }
        // Trim the pad: samples-per-frame derives from the decoder's own
        // output (uniform per window), so the true length needs no constant.
        let samples_per_frame = pcm.len() / padded_frames;
        pcm.truncate(total_frames * samples_per_frame);
        clog_info!(
            "Orpheus: SNAC decoded {} windows × {} frames → {} samples ({} true frames)",
            windows,
            frames_per_call,
            pcm.len(),
            total_frames
        );
        Ok(pcm)
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
        ORPHEUS_MODEL.is_loaded()
    }

    async fn initialize(&self) -> Result<(), TTSError> {
        if ORPHEUS_MODEL.is_loaded() {
            clog_info!("Orpheus: Already initialized");
            return Ok(());
        }

        let model_dir = self.find_model_dir();
        clog_info!("Orpheus: Loading models from {:?}", model_dir);

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

        // Select compute device — fail-closed on no-Metal (no CPU fallback)
        let device = Self::select_device()?;

        // Load GGUF model — and build the tokenizer FROM it (one artifact;
        // the upstream tokenizer.json is HF-gated and mirrors ship the wrong
        // one — measured 2026-09-02).
        let gguf_path = Self::find_gguf_file(&model_dir).ok_or_else(|| {
            TTSError::ModelNotLoaded("No .gguf file found in model directory".into())
        })?;
        clog_info!("Orpheus: Loading GGUF model from {:?}", gguf_path);

        let mut gguf_file = std::fs::File::open(&gguf_path)
            .map_err(|e| TTSError::ModelNotLoaded(format!("Failed to open GGUF file: {e}")))?;
        let gguf_content = continuum_engine::inference_capability::gguf_keys::read_bounded_header(
            &mut gguf_file,
            &|| false,
        )
        .map_err(|e| TTSError::ModelNotLoaded(format!("Failed to read GGUF content: {e}")))?;

        let (tokenizer, protocol) = Self::tokenizer_from_gguf(&gguf_content)?;
        clog_info!(
            "Orpheus: tokenizer built from GGUF metadata ({} tokens, scheme verified)",
            tokenizer.get_vocab_size(true)
        );

        let mut reader =
            BufReader::new(std::fs::File::open(&gguf_path).map_err(|e| {
                TTSError::ModelNotLoaded(format!("Failed to reopen GGUF file: {e}"))
            })?);

        let llm = ModelWeights::from_gguf(gguf_content, &mut reader, &device)
            .map_err(|e| TTSError::ModelNotLoaded(format!("GGUF model load failed: {e}")))?;
        clog_info!("Orpheus: Llama model loaded on {:?}", device);

        // Track GPU allocation for LLM weights (non-critical: proceed on failure)
        let _ = ORPHEUS_LLM_GPU.track_file(
            GpuSubsystem::Tts,
            &gguf_path,
            super::gpu_manager(),
            GpuPriority::Interactive,
        );

        // Load SNAC decoder
        let snac_path = model_dir.join("snac_decoder.onnx");
        let snac_decoder = Self::build_snac_session(&snac_path)?;
        let snac_window = Self::snac_window(&snac_decoder)?;
        clog_info!(
            "Orpheus: SNAC decoder loaded ({} inputs, {} outputs)",
            snac_decoder.inputs().len(),
            snac_decoder.outputs().len()
        );

        // Track GPU allocation for SNAC decoder (non-critical)
        let _ = ORPHEUS_SNAC_GPU.track_file(
            GpuSubsystem::Tts,
            &snac_path,
            super::gpu_manager(),
            GpuPriority::Interactive,
        );

        let model = OrpheusModel {
            llm,
            tokenizer,
            snac_decoder,
            device,
            protocol,
            snac_window,
        };

        let _ = ORPHEUS_MODEL.load_with(|| Ok::<_, TTSError>(Mutex::new(model)));

        clog_info!("Orpheus: All models loaded successfully");
        Ok(())
    }

    async fn synthesize(&self, text: &str, voice: &str) -> Result<SynthesisResult, TTSError> {
        ORPHEUS_LLM_GPU.touch();
        ORPHEUS_SNAC_GPU.touch();

        let model_arc = ORPHEUS_MODEL.get().ok_or_else(|| {
            TTSError::ModelNotLoaded("Orpheus not initialized. Call initialize() first.".into())
        })?;

        // Validate voice
        let voice = if VOICES.iter().any(|(id, _, _)| *id == voice) {
            voice.to_string()
        } else {
            // Use default voice for unknown voice IDs
            clog_info!("Orpheus: Unknown voice '{}', using default 'tara'", voice);
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

    async fn shutdown(&self) -> Result<(), TTSError> {
        if ORPHEUS_MODEL.unload() {
            ORPHEUS_LLM_GPU.release();
            ORPHEUS_SNAC_GPU.release();
            clog_info!("Orpheus: Models unloaded (~2GB freed)");
        }
        Ok(())
    }

    fn available_voices(&self) -> Vec<VoiceInfo> {
        VOICES
            .iter()
            .map(|(id, name, gender)| VoiceInfo {
                id: id.to_string(),
                name: name.to_string(),
                language: "en".to_string(),
                gender: Some(gender.to_string()),
                description: Some(format!("Orpheus {gender} voice — supports emotion tags")),
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
    fn test_redistribute_codes_canonical_interleave() -> Result<(), TTSError> {
        // what this catches: the canopylabs frame layout is [L0,L1,L2,L2,L1,L2,L2]
        // — layer1 takes positions 1 AND 4. The first cut used [0,1,1,2,2,2,2],
        // which scrambled mid/fine codebooks into noise. Each position sits in
        // its own band (id = base + position*band + code).
        let p = protocol::canopylabs()?;
        let mut audio_tokens = Vec::new();
        for frame in 0..2u32 {
            for pos in 0..7u32 {
                // code value = frame*10 + pos, in position-band `pos`
                audio_tokens.push(p.audio_base + pos * p.band + frame * 10 + pos);
            }
        }

        let layers = OrpheusTts::redistribute_codes(&p, &audio_tokens)?;
        assert_eq!(layers[0].len(), 2); // 1/frame
        assert_eq!(layers[1].len(), 4); // 2/frame (positions 1 and 4)
        assert_eq!(layers[2].len(), 8); // 4/frame (positions 2,3,5,6)

        // Frame 0 codes: position p carries value p.
        assert_eq!(layers[0][0], 0); // pos 0
        assert_eq!(layers[1][0], 1); // pos 1
        assert_eq!(layers[1][1], 4); // pos 4 ← the interleave the first cut broke
        assert_eq!(layers[2][0], 2); // pos 2
        assert_eq!(layers[2][1], 3); // pos 3
        assert_eq!(layers[2][2], 5); // pos 5
        assert_eq!(layers[2][3], 6); // pos 6
        assert_eq!(layers[0][1], 10); // frame 1, pos 0
        Ok(())
    }

    #[test]
    fn test_redistribute_codes_too_few_tokens() -> Result<(), TTSError> {
        let p = protocol::canopylabs()?;
        let tokens = vec![p.audio_base; 5]; // Less than one frame
        assert!(OrpheusTts::redistribute_codes(&p, &tokens).is_err());
        Ok(())
    }

    #[test]
    fn test_redistribute_codes_truncates_partial_frame() -> Result<(), TTSError> {
        // 10 tokens: 1 complete frame (7) + 3 leftover (discarded)
        let p = protocol::canopylabs()?;
        let tokens: Vec<u32> = (0..10).map(|i| p.audio_base + i).collect();
        let layers = OrpheusTts::redistribute_codes(&p, &tokens)?;
        assert_eq!(layers[0].len(), 1); // Only 1 complete frame
        Ok(())
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
        // tokenizer.json must NOT be required: it is HF-gated upstream and the
        // GGUF carries its own token table (tokenizer_from_gguf). One artifact.
        assert!(!OrpheusTts::REQUIRED_FILES.contains(&"tokenizer.json"));
        assert!(OrpheusTts::REQUIRED_FILES.contains(&"snac_decoder.onnx"));
    }
}
