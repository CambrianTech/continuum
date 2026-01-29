//! Piper TTS Adapter
//!
//! Local TTS inference using Piper (ONNX-based, no Python dependencies).
//! High-quality voices, efficient inference, designed for production use.
//! Used by Home Assistant and other production systems.

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
        let speaker_id: i64 = voice.parse().unwrap_or(0).min(246).max(0);
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

        // Convert f32 to i16 (Piper outputs at model sample rate, we need 16kHz)
        const PCM_I16_MAX: f32 = 32767.0;  // Maximum value for signed 16-bit PCM
        const AUDIO_RANGE_MIN: f32 = -1.0;
        const AUDIO_RANGE_MAX: f32 = 1.0;

        let source_rate = model.sample_rate;

        let samples_source: Vec<i16> = audio_data
            .iter()
            .map(|&s| (s.clamp(AUDIO_RANGE_MIN, AUDIO_RANGE_MAX) * PCM_I16_MAX) as i16)
            .collect();

        // Resample from model's sample rate to standard audio rate
        use crate::audio_constants::AUDIO_SAMPLE_RATE;
        let samples_resampled = Self::resample_to_target(&samples_source, source_rate, AUDIO_SAMPLE_RATE);

        let duration_ms = (samples_resampled.len() as u64 * 1000) / AUDIO_SAMPLE_RATE as u64;

        info!(
            "Piper synthesized {} samples ({}ms) for '{}...'",
            samples_resampled.len(),
            duration_ms,
            super::truncate_str(&text, 30)
        );

        Ok(SynthesisResult {
            samples: samples_resampled,
            sample_rate: AUDIO_SAMPLE_RATE,
            duration_ms,
        })
    }

    /// Resample from source sample rate to target rate (linear interpolation)
    fn resample_to_target(samples: &[i16], source_rate: u32, target_rate: u32) -> Vec<i16> {
        // If already at target rate, return as-is
        if source_rate == target_rate {
            return samples.to_vec();
        }

        let ratio = source_rate as f64 / target_rate as f64;
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
        ).map_err(|e| TTSError::ModelNotLoaded(format!("Failed to load phonemizer: {}", e)))?;

        let model = PiperModel {
            session,
            sample_rate: 22050,
            phonemizer,
        };

        PIPER_SESSION
            .set(Arc::new(Mutex::new(model)))
            .map_err(|_| TTSError::ModelNotLoaded("Failed to set global session".into()))?;

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
    use crate::audio_constants::AUDIO_SAMPLE_RATE;

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

    #[test]
    fn test_resample() {
        // 6 samples at 22050Hz should become ~4 samples at AUDIO_SAMPLE_RATE
        let input: Vec<i16> = vec![100, 200, 300, 400, 500, 600];
        let output = PiperTTS::resample_to_target(&input, 22050, AUDIO_SAMPLE_RATE);
        // 6 * 16000 / 22050 ≈ 4.35 samples
        assert!(output.len() >= 4 && output.len() <= 5);
    }
}
