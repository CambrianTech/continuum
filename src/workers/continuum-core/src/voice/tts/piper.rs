//! Piper TTS Adapter
//!
//! Local TTS inference using Piper (ONNX-based, no Python dependencies).
//! High-quality voices, efficient inference, designed for production use.
//! Used by Home Assistant and other production systems.

use super::audio_utils;
use super::{Phonemizer, SynthesisResult, TTSError, TextToSpeech, VoiceInfo};
use async_trait::async_trait;
use ndarray;
use once_cell::sync::OnceCell;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

/// Global Piper session
static PIPER_SESSION: OnceCell<Arc<Mutex<PiperModel>>> = OnceCell::new();

/// Piper model wrapper
struct PiperModel {
    session: Session,
    sample_rate: u32,
    phonemizer: Phonemizer,
}

/// Piper TTS Adapter
pub struct PiperTTS {
    model_path: Option<PathBuf>,
}

impl PiperTTS {
    pub fn new() -> Self {
        Self { model_path: None }
    }

    pub fn with_model_path(model_path: PathBuf) -> Self {
        Self {
            model_path: Some(model_path),
        }
    }

    /// Find model in common locations
    fn find_model_path(&self) -> Option<PathBuf> {
        if let Some(ref path) = self.model_path {
            if path.exists() {
                return Some(path.clone());
            }
        }

        let candidates = [
            PathBuf::from("models/piper/en_US-libritts_r-medium.onnx"),  // Primary
            PathBuf::from("models/piper/en_US-amy-medium.onnx"),          // Alternative
            PathBuf::from("models/piper/piper.onnx"),                     // Generic
            PathBuf::from("models/tts/piper.onnx"),
            dirs::data_dir()
                .unwrap_or_default()
                .join("piper/en_US-libritts_r-medium.onnx"),
            PathBuf::from("/usr/local/share/piper/en_US-libritts_r-medium.onnx"),
        ];

        candidates.into_iter().find(|path| path.exists())
    }

    /// Synchronous synthesis
    fn synthesize_sync(
        session: &Arc<Mutex<PiperModel>>,
        text: &str,
        voice: &str,  // Speaker ID for multi-speaker models (0-246 for LibriTTS)
        _speed: f32,   // TODO: Implement speed control via length_scale parameter
    ) -> Result<SynthesisResult, TTSError> {
        if text.is_empty() {
            return Err(TTSError::InvalidText("Text cannot be empty".into()));
        }

        let model = session.lock();

        // Phonemize text to get phoneme IDs using model's phonemizer
        let phoneme_ids = model.phonemizer.text_to_phoneme_ids(text);

        // Reshape to [1, len] for batch dimension
        let len = phoneme_ids.len();
        let text_array = ndarray::Array2::from_shape_vec((1, len), phoneme_ids)
            .map_err(|e| TTSError::SynthesisFailed(format!("Failed to reshape input: {e}")))?;

        // Speaker ID (for multi-speaker models like LibriTTS which has 247 speakers)
        // Parse voice as speaker ID, default to 0 if invalid
        let speaker_id: i64 = voice.parse().unwrap_or(0).clamp(0, 246);
        let sid_array = ndarray::Array1::from_vec(vec![speaker_id]);

        // Inference parameters from model config
        // Format: [noise_scale, length_scale, noise_w]
        let scales_array = ndarray::Array1::from_vec(vec![0.333_f32, 1.0_f32, 0.333_f32]);

        // Run inference
        let outputs = model
            .session
            .run(ort::inputs![
                "input" => text_array.view(),
                "input_lengths" => ndarray::Array1::from_vec(vec![len as i64]).view(),
                "scales" => scales_array.view(),
                "sid" => sid_array.view()
            ]?)
            .map_err(|e| TTSError::SynthesisFailed(format!("ONNX inference failed: {e}")))?;

        // Extract audio
        let audio_output = outputs
            .iter()
            .next()
            .ok_or_else(|| TTSError::SynthesisFailed("No audio output from model".into()))?
            .1;

        let (_, audio_data) = audio_output
            .try_extract_raw_tensor::<f32>()
            .map_err(|e| TTSError::SynthesisFailed(format!("Failed to extract audio: {e}")))?;

        // Normalize to standard 16kHz i16 PCM via shared audio utilities
        let f32_samples: Vec<f32> = audio_data.to_vec();
        let result = audio_utils::normalize_audio(&f32_samples, model.sample_rate)?;

        info!(
            "Piper synthesized {} samples ({}ms) for '{}...'",
            result.samples.len(),
            result.duration_ms,
            super::truncate_str(text, 30)
        );

        Ok(result)
    }

}

impl Default for PiperTTS {
    fn default() -> Self {
        Self::new()
    }
}

// Note: From<ort::Error> for TTSError already implemented in kokoro.rs

#[async_trait]
impl TextToSpeech for PiperTTS {
    fn name(&self) -> &'static str {
        "piper"
    }

    fn description(&self) -> &'static str {
        "Piper TTS (high-quality, ONNX-based)"
    }

    fn is_initialized(&self) -> bool {
        PIPER_SESSION.get().is_some()
    }

    async fn initialize(&self) -> Result<(), TTSError> {
        if PIPER_SESSION.get().is_some() {
            info!("Piper already initialized");
            return Ok(());
        }

        let model_path = match self.find_model_path() {
            Some(path) => path,
            None => {
                warn!("Piper model not found. Should be auto-downloaded at:");
                warn!("  models/piper/en_US-libritts_r-medium.onnx");
                return Err(TTSError::ModelNotLoaded(
                    "Piper ONNX model not found".into(),
                ));
            }
        };

        info!("Loading Piper model from: {:?}", model_path);

        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(num_cpus::get().min(4))?
            .commit_from_file(&model_path)?;

        // Load phonemizer from model config
        let config_path = model_path.with_extension("onnx.json");
        let phonemizer = Phonemizer::load_from_config(
            config_path.to_str().unwrap_or("models/piper/en_US-libritts_r-medium.onnx.json")
        ).map_err(|e| TTSError::ModelNotLoaded(format!("Failed to load phonemizer: {e}")))?;

        let model = PiperModel {
            session,
            sample_rate: 22050,
            phonemizer,
        };

        let _ = PIPER_SESSION
            .set(Arc::new(Mutex::new(model)));
        // OnceLock::set Err = another thread already initialized — that's fine

        info!("Piper model loaded successfully");
        Ok(())
    }

    async fn synthesize(&self, text: &str, voice: &str) -> Result<SynthesisResult, TTSError> {
        let session = PIPER_SESSION
            .get()
            .ok_or_else(|| TTSError::ModelNotLoaded("Piper not initialized".into()))?
            .clone();

        let text = text.to_string();
        let voice = voice.to_string();

        tokio::task::spawn_blocking(move || Self::synthesize_sync(&session, &text, &voice, 1.0))
            .await
            .map_err(|e| TTSError::SynthesisFailed(format!("Task join error: {e}")))?
    }

    fn available_voices(&self) -> Vec<VoiceInfo> {
        vec![VoiceInfo {
            id: "default".to_string(),
            name: "LibriTTS (High Quality)".to_string(),
            language: "en-US".to_string(),
            gender: Some("neutral".to_string()),
            description: Some("High-quality English voice from LibriTTS dataset".to_string()),
        }]
    }

    fn default_voice(&self) -> &str {
        "default"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_piper_adapter() {
        let adapter = PiperTTS::new();
        assert_eq!(adapter.name(), "piper");
        assert!(!adapter.is_initialized());
        assert_eq!(adapter.default_voice(), "default");
    }

    #[test]
    fn test_available_voices() {
        let adapter = PiperTTS::new();
        let voices = adapter.available_voices();
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].id, "default");
    }

    // Resample tests moved to audio_utils::tests (shared across all adapters)
}
