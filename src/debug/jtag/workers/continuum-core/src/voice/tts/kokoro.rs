//! Kokoro TTS Adapter
//!
//! Local TTS inference using Kokoro-82M v1.0 via ONNX Runtime.
//! Uses espeak-ng for phonemization + Kokoro vocab for tokenization.
//!
//! GPU Acceleration:
//! - CoreML (Apple Silicon) - macOS
//! - CUDA (NVIDIA GPUs) - Linux/Windows

use super::{SynthesisResult, TTSError, TextToSpeech, VoiceInfo};
use async_trait::async_trait;
use ndarray;
use once_cell::sync::OnceCell;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tracing::{info, warn};

/// Global Kokoro session + tokenizer
static KOKORO_SESSION: OnceCell<Arc<Mutex<KokoroModel>>> = OnceCell::new();

/// Kokoro model wrapper
struct KokoroModel {
    session: Session,
    vocab: HashMap<char, i64>,
    voices_dir: PathBuf,
    /// Cached voice embeddings: voice_id -> (N, 256) float32 array
    voice_cache: HashMap<String, Vec<Vec<f32>>>,
    #[allow(dead_code)]
    sample_rate: u32,
}

/// Max phoneme/token length for Kokoro v1.0
const MAX_TOKEN_LENGTH: usize = 510;

/// Available Kokoro voices
const KOKORO_VOICES: &[(&str, &str, &str)] = &[
    ("af", "American Female (default)", "en-US"),
    ("af_bella", "Bella", "en-US"),
    ("af_nicole", "Nicole", "en-US"),
    ("af_sarah", "Sarah", "en-US"),
    ("af_sky", "Sky", "en-US"),
    ("am_adam", "Adam", "en-US"),
    ("am_michael", "Michael", "en-US"),
    ("bf_emma", "Emma", "en-GB"),
    ("bf_isabella", "Isabella", "en-GB"),
    ("bm_george", "George", "en-GB"),
    ("bm_lewis", "Lewis", "en-GB"),
];

/// Kokoro TTS Adapter
pub struct KokoroTTS {
    model_path: Option<PathBuf>,
}

impl KokoroTTS {
    pub fn new() -> Self {
        Self { model_path: None }
    }

    #[allow(dead_code)]
    pub fn with_model_path(model_path: PathBuf) -> Self {
        Self {
            model_path: Some(model_path),
        }
    }

    /// Find model ONNX file in common locations
    fn find_model_path(&self) -> Option<PathBuf> {
        if let Some(ref path) = self.model_path {
            if path.exists() {
                return Some(path.clone());
            }
        }

        let candidates = [
            // v1.0 quantized (preferred — smaller, faster)
            PathBuf::from("models/kokoro/kokoro-v1.0-q8.onnx"),
            // v1.0 full precision
            PathBuf::from("models/kokoro/kokoro-v1.0.onnx"),
            // Generic names
            PathBuf::from("models/kokoro/model_quantized.onnx"),
            PathBuf::from("models/kokoro/model.onnx"),
            PathBuf::from("models/kokoro/kokoro.onnx"),
            // Legacy v0.19
            PathBuf::from("models/kokoro/kokoro-v0_19.onnx"),
        ];

        candidates.into_iter().find(|path| path.exists())
    }

    /// Find voices directory
    fn find_voices_dir() -> Option<PathBuf> {
        let candidates = [
            PathBuf::from("models/kokoro/voices"),
            PathBuf::from("models/kokoro"),
        ];
        candidates.into_iter().find(|path| path.is_dir())
    }

    /// Load Kokoro vocab from JSON file
    fn load_vocab() -> Result<HashMap<char, i64>, TTSError> {
        let vocab_path = PathBuf::from("models/kokoro/vocab.json");
        if !vocab_path.exists() {
            return Err(TTSError::ModelNotLoaded(
                "Kokoro vocab.json not found at models/kokoro/vocab.json".into(),
            ));
        }

        let content = std::fs::read_to_string(&vocab_path)
            .map_err(|e| TTSError::IoError(e))?;

        let raw: HashMap<String, i64> = serde_json::from_str(&content)
            .map_err(|e| TTSError::ModelNotLoaded(format!("Failed to parse vocab.json: {e}")))?;

        // Convert string keys to char keys
        let mut vocab = HashMap::new();
        for (key, value) in raw {
            if let Some(ch) = key.chars().next() {
                vocab.insert(ch, value);
            }
        }

        info!("Kokoro vocab loaded: {} entries", vocab.len());
        Ok(vocab)
    }

    /// Load voice embedding from .bin file
    fn load_voice_embedding(voices_dir: &PathBuf, voice_id: &str) -> Result<Vec<Vec<f32>>, TTSError> {
        let voice_path = voices_dir.join(format!("{}.bin", voice_id));
        if !voice_path.exists() {
            // Try default voice
            let default_path = voices_dir.join("af.bin");
            if !default_path.exists() {
                return Err(TTSError::VoiceNotFound(format!(
                    "Voice file not found: {} (and default af.bin also missing)",
                    voice_path.display()
                )));
            }
            warn!("Voice '{}' not found, using default 'af'", voice_id);
            return Self::load_voice_embedding(voices_dir, "af");
        }

        let bytes = std::fs::read(&voice_path)
            .map_err(|e| TTSError::IoError(e))?;

        // Parse as float32 array
        let num_floats = bytes.len() / 4;
        let mut floats = Vec::with_capacity(num_floats);
        for chunk in bytes.chunks_exact(4) {
            floats.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }

        // Reshape to (N, 256)
        let embedding_dim = 256;
        let num_rows = num_floats / embedding_dim;
        let mut embeddings = Vec::with_capacity(num_rows);
        for row in 0..num_rows {
            let start = row * embedding_dim;
            let end = start + embedding_dim;
            embeddings.push(floats[start..end].to_vec());
        }

        info!(
            "Loaded voice '{}': {} style vectors ({}x{})",
            voice_id, num_rows, num_rows, embedding_dim
        );

        Ok(embeddings)
    }

    /// Normalize voice ID to a known voice
    fn normalize_voice(voice: &str) -> &str {
        for (id, _, _) in KOKORO_VOICES {
            if *id == voice {
                return id;
            }
        }
        "af" // Default
    }

    /// Call espeak-ng to phonemize text (same as Piper, but returns raw IPA string)
    fn phonemize(text: &str) -> Result<String, TTSError> {
        let output = Command::new("/opt/homebrew/bin/espeak-ng")
            .args(&["-v", "en-us", "-q", "--ipa=3"])
            .arg(text)
            .output()
            .map_err(|e| TTSError::SynthesisFailed(format!("Failed to run espeak-ng: {e}")))?;

        if !output.status.success() {
            return Err(TTSError::SynthesisFailed(format!(
                "espeak-ng failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let phonemes = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string()
            .replace('\u{200D}', "") // Zero-width joiner
            .replace('\u{200C}', "") // Zero-width non-joiner
            .replace('\u{FEFF}', "") // Zero-width no-break space
            .replace('\n', " ")
            .replace('\r', " ");

        Ok(phonemes)
    }

    /// Tokenize phoneme string using Kokoro vocab
    fn tokenize(phonemes: &str, vocab: &HashMap<char, i64>) -> Vec<i64> {
        let mut tokens = Vec::with_capacity(phonemes.len() + 2);

        // Start padding token
        tokens.push(0);

        // Map each phoneme character through vocab (skip unknown chars)
        for ch in phonemes.chars() {
            if let Some(&id) = vocab.get(&ch) {
                tokens.push(id);
            }
            // Unknown chars are silently skipped (filtered out)
        }

        // End padding token
        tokens.push(0);

        // Enforce max length
        if tokens.len() > MAX_TOKEN_LENGTH + 2 {
            tokens.truncate(MAX_TOKEN_LENGTH + 2);
            // Ensure end padding
            let last = tokens.len() - 1;
            tokens[last] = 0;
        }

        tokens
    }

    /// Synchronous synthesis
    fn synthesize_sync(
        session: &Arc<Mutex<KokoroModel>>,
        text: &str,
        voice: &str,
        speed: f32,
    ) -> Result<SynthesisResult, TTSError> {
        if text.is_empty() {
            return Err(TTSError::InvalidText("Text cannot be empty".into()));
        }

        let voice_id = Self::normalize_voice(voice);
        let mut model = session.lock();

        // Step 1: Phonemize text via espeak-ng
        let phonemes = Self::phonemize(text)?;
        info!("Kokoro phonemized: '{}' -> '{}'", &text[..text.len().min(40)], &phonemes[..phonemes.len().min(60)]);

        // Step 2: Tokenize using Kokoro vocab
        let tokens = Self::tokenize(&phonemes, &model.vocab);
        let token_count = tokens.len();
        info!("Kokoro tokenized: {} tokens", token_count);

        if token_count < 3 {
            return Err(TTSError::InvalidText("Text produced no valid tokens".into()));
        }

        // Step 3: Get voice embedding for this token count
        // Load voice if not cached
        if !model.voice_cache.contains_key(voice_id) {
            let embeddings = Self::load_voice_embedding(&model.voices_dir, voice_id)?;
            model.voice_cache.insert(voice_id.to_string(), embeddings);
        }

        let voice_embeddings = model.voice_cache.get(voice_id).unwrap();

        // Select style vector based on token count (clamped to available range)
        let style_idx = token_count.min(voice_embeddings.len().saturating_sub(1));
        let style_vector = &voice_embeddings[style_idx];

        // Step 4: Build ONNX input tensors
        // input_ids: shape (1, token_count)
        let input_ids = ndarray::Array2::from_shape_vec(
            (1, token_count),
            tokens,
        ).map_err(|e| TTSError::SynthesisFailed(format!("Failed to create input_ids tensor: {e}")))?;

        // style: shape (1, 256)
        let style = ndarray::Array2::from_shape_vec(
            (1, 256),
            style_vector.clone(),
        ).map_err(|e| TTSError::SynthesisFailed(format!("Failed to create style tensor: {e}")))?;

        // speed: shape (1,)
        let speed_tensor = ndarray::Array1::from_vec(vec![speed]);

        // Step 5: Run ONNX inference
        let outputs = model
            .session
            .run(ort::inputs![
                "input_ids" => input_ids,
                "style" => style,
                "speed" => speed_tensor
            ]?)
            .map_err(|e| TTSError::SynthesisFailed(format!("ONNX inference failed: {e}")))?;

        // Step 6: Extract audio output
        let audio_output = outputs
            .iter()
            .next()
            .ok_or_else(|| TTSError::SynthesisFailed("No audio output from model".into()))?
            .1;

        let (_, audio_data) = audio_output
            .try_extract_raw_tensor::<f32>()
            .map_err(|e| TTSError::SynthesisFailed(format!("Failed to extract audio: {e}")))?;

        // Step 7: Convert f32 to i16 and resample from 24kHz to 16kHz
        let samples_24k: Vec<i16> = audio_data
            .iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
            .collect();

        use crate::audio_constants::AUDIO_SAMPLE_RATE;
        let samples_resampled = Self::resample_24k_to_target(&samples_24k, AUDIO_SAMPLE_RATE);

        let duration_ms = (samples_resampled.len() as u64 * 1000) / AUDIO_SAMPLE_RATE as u64;

        info!(
            "Kokoro synthesized {} samples ({}ms) for '{}...'",
            samples_resampled.len(),
            duration_ms,
            &text[..text.len().min(30)]
        );

        Ok(SynthesisResult {
            samples: samples_resampled,
            sample_rate: AUDIO_SAMPLE_RATE,
            duration_ms,
        })
    }

    /// Resample from 24kHz to target rate (simple linear interpolation)
    fn resample_24k_to_target(samples: &[i16], target_rate: u32) -> Vec<i16> {
        let ratio = 24000.0 / target_rate as f64;
        let output_len = (samples.len() as f64 / ratio) as usize;
        let mut output = Vec::with_capacity(output_len);

        for i in 0..output_len {
            let src_pos = i as f64 * ratio;
            let src_idx = src_pos as usize;
            let frac = src_pos - src_idx as f64;

            let sample = if src_idx + 1 < samples.len() {
                let s0 = samples[src_idx] as f64;
                let s1 = samples[src_idx + 1] as f64;
                (s0 + frac * (s1 - s0)) as i16
            } else {
                samples.get(src_idx).copied().unwrap_or(0)
            };

            output.push(sample);
        }

        output
    }
}

impl Default for KokoroTTS {
    fn default() -> Self {
        Self::new()
    }
}

impl From<ort::Error> for TTSError {
    fn from(e: ort::Error) -> Self {
        TTSError::SynthesisFailed(e.to_string())
    }
}

#[async_trait]
impl TextToSpeech for KokoroTTS {
    fn name(&self) -> &'static str {
        "kokoro"
    }

    fn description(&self) -> &'static str {
        "Local Kokoro TTS (82M params, ONNX, espeak-ng phonemizer)"
    }

    fn is_initialized(&self) -> bool {
        KOKORO_SESSION.get().is_some()
    }

    async fn initialize(&self) -> Result<(), TTSError> {
        if KOKORO_SESSION.get().is_some() {
            info!("Kokoro already initialized");
            return Ok(());
        }

        // Find model
        let model_path = match self.find_model_path() {
            Some(path) => path,
            None => {
                warn!("Kokoro model not found. Download from:");
                warn!("  https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX");
                warn!("Place ONNX file in: models/kokoro/kokoro-v1.0-q8.onnx");
                return Err(TTSError::ModelNotLoaded(
                    "Kokoro ONNX model not found".into(),
                ));
            }
        };

        // Find voices directory
        let voices_dir = Self::find_voices_dir().unwrap_or_else(|| {
            warn!("Kokoro voices directory not found, using models/kokoro/voices");
            PathBuf::from("models/kokoro/voices")
        });

        // Load vocab
        let vocab = Self::load_vocab()?;

        info!("Loading Kokoro model from: {:?}", model_path);

        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(num_cpus::get().min(4))?
            .commit_from_file(&model_path)?;

        let model = KokoroModel {
            session,
            vocab,
            voices_dir,
            voice_cache: HashMap::new(),
            sample_rate: 24000,
        };

        KOKORO_SESSION
            .set(Arc::new(Mutex::new(model)))
            .map_err(|_| TTSError::ModelNotLoaded("Failed to set global session".into()))?;

        info!("Kokoro model loaded successfully");
        Ok(())
    }

    async fn synthesize(&self, text: &str, voice: &str) -> Result<SynthesisResult, TTSError> {
        let session = KOKORO_SESSION
            .get()
            .ok_or_else(|| TTSError::ModelNotLoaded("Kokoro not initialized".into()))?
            .clone();

        let text = text.to_string();
        let voice = voice.to_string();

        tokio::task::spawn_blocking(move || Self::synthesize_sync(&session, &text, &voice, 1.0))
            .await
            .map_err(|e| TTSError::SynthesisFailed(format!("Task join error: {e}")))?
    }

    fn available_voices(&self) -> Vec<VoiceInfo> {
        KOKORO_VOICES
            .iter()
            .map(|(id, name, lang)| VoiceInfo {
                id: id.to_string(),
                name: name.to_string(),
                language: lang.to_string(),
                gender: if id.contains("m_") {
                    Some("male".to_string())
                } else {
                    Some("female".to_string())
                },
                description: None,
            })
            .collect()
    }

    fn default_voice(&self) -> &str {
        "af"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_constants::AUDIO_SAMPLE_RATE;

    #[test]
    fn test_kokoro_adapter() {
        let adapter = KokoroTTS::new();
        assert_eq!(adapter.name(), "kokoro");
        assert!(!adapter.is_initialized());
        assert_eq!(adapter.default_voice(), "af");
    }

    #[test]
    fn test_available_voices() {
        let adapter = KokoroTTS::new();
        let voices = adapter.available_voices();
        assert!(!voices.is_empty());
        assert!(voices.iter().any(|v| v.id == "af"));
    }

    #[test]
    fn test_resample() {
        let input: Vec<i16> = vec![100, 200, 300, 400, 500, 600];
        let output = KokoroTTS::resample_24k_to_target(&input, AUDIO_SAMPLE_RATE);
        assert_eq!(output.len(), 4);
    }

    #[test]
    fn test_tokenize_basic() {
        let mut vocab = HashMap::new();
        vocab.insert('h', 50);
        vocab.insert('e', 47);
        vocab.insert('l', 54);
        vocab.insert('o', 57);

        let tokens = KokoroTTS::tokenize("helo", &vocab);
        // Should be: [0, 50, 47, 54, 57, 0]
        assert_eq!(tokens[0], 0); // Start padding
        assert_eq!(tokens[1], 50); // h
        assert_eq!(tokens[2], 47); // e
        assert_eq!(tokens[3], 54); // l
        assert_eq!(tokens[4], 57); // o
        assert_eq!(tokens[tokens.len() - 1], 0); // End padding
    }
}
