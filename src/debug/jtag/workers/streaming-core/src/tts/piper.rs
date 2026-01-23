//! Piper TTS Adapter
//!
//! Local TTS inference using Piper (ONNX-based, no Python dependencies).
//! High-quality voices, efficient inference, designed for production use.
//! Used by Home Assistant and other production systems.

use super::{SynthesisResult, TTSError, TextToSpeech, VoiceInfo};
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
        _voice: &str,  // Piper models are single-voice
        speed: f32,
    ) -> Result<SynthesisResult, TTSError> {
        if text.is_empty() {
            return Err(TTSError::InvalidText("Text cannot be empty".into()));
        }

        let model = session.lock();

        // Tokenize text (simplified - real Piper uses phonemization)
        let text_tokens: Vec<i64> = text
            .chars()
            .filter_map(|c| if c.is_ascii() { Some(c as i64) } else { None })
            .collect();
        let text_array = ndarray::Array1::from_vec(text_tokens);

        // Speed parameter
        let speed_array = ndarray::Array1::from_vec(vec![speed]);

        // Run inference
        let outputs = model
            .session
            .run(ort::inputs![
                "input" => text_array,
                "speed" => speed_array
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
        let source_rate = model.sample_rate;
        let samples_source: Vec<i16> = audio_data
            .iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
            .collect();

        // Resample from model's sample rate to 16000Hz
        let samples_16k = Self::resample_to_16k(&samples_source, source_rate);

        let duration_ms = (samples_16k.len() as u64 * 1000) / 16000;

        info!(
            "Piper synthesized {} samples ({}ms) for '{}...'",
            samples_16k.len(),
            duration_ms,
            &text[..text.len().min(30)]
        );

        Ok(SynthesisResult {
            samples: samples_16k,
            sample_rate: 16000,
            duration_ms,
        })
    }

    /// Resample from source sample rate to 16000Hz (linear interpolation)
    fn resample_to_16k(samples: &[i16], source_rate: u32) -> Vec<i16> {
        const TARGET_RATE: u32 = 16000;

        // If already at target rate, return as-is
        if source_rate == TARGET_RATE {
            return samples.to_vec();
        }

        let ratio = source_rate as f64 / TARGET_RATE as f64;
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

        let model = PiperModel {
            session,
            sample_rate: 22050,
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
        // 6 samples at 22050Hz should become ~4 samples at 16000Hz
        let input: Vec<i16> = vec![100, 200, 300, 400, 500, 600];
        let output = PiperTTS::resample_22k_to_16k(&input);
        assert!(output.len() >= 4 && output.len() <= 5);
    }
}
