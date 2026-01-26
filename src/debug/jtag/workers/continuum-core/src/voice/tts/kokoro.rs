//! Kokoro TTS Adapter
//!
//! Local TTS inference using Kokoro (~82M params) via ONNX Runtime.
//! Excellent quality for a lightweight model.

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

/// Global Kokoro session
static KOKORO_SESSION: OnceCell<Arc<Mutex<KokoroModel>>> = OnceCell::new();

/// Kokoro model wrapper
struct KokoroModel {
    session: Session,
    #[allow(dead_code)]
    sample_rate: u32,
}

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
            PathBuf::from("models/kokoro/kokoro-v0_19.onnx"),
            PathBuf::from("models/kokoro/kokoro.onnx"),
            PathBuf::from("models/tts/kokoro.onnx"),
            dirs::data_dir()
                .unwrap_or_default()
                .join("kokoro/kokoro-v0_19.onnx"),
            PathBuf::from("/usr/local/share/kokoro/kokoro.onnx"),
        ];

        candidates.into_iter().find(|path| path.exists())
    }

    /// Normalize voice ID
    fn normalize_voice(voice: &str) -> &'static str {
        for (id, _, _) in KOKORO_VOICES {
            if *id == voice {
                return id;
            }
        }
        "af" // Default
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
        let model = session.lock();

        // Tokenize text
        let text_tokens: Vec<i64> = text
            .chars()
            .filter_map(|c| if c.is_ascii() { Some(c as i64) } else { None })
            .collect();
        let text_array = ndarray::Array1::from_vec(text_tokens);

        // Voice embedding (simplified)
        let voice_embedding = Self::get_voice_embedding(voice_id);
        let voice_array = ndarray::Array1::from_vec(voice_embedding);

        // Speed
        let speed_array = ndarray::Array1::from_vec(vec![speed]);

        // Run inference
        let outputs = model
            .session
            .run(ort::inputs![
                "tokens" => text_array,
                "voice" => voice_array,
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

        // Convert f32 to i16 and resample from 24kHz to 16kHz
        let samples_24k: Vec<i16> = audio_data
            .iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
            .collect();

        // Resample from Kokoro's 24kHz to standard audio rate
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

    /// Voice embedding (placeholder - real impl loads from disk)
    fn get_voice_embedding(voice_id: &str) -> Vec<f32> {
        let seed = voice_id
            .bytes()
            .fold(0u32, |acc, b| acc.wrapping_add(b as u32));
        let mut embedding = vec![0.0f32; 256];

        for (i, val) in embedding.iter_mut().enumerate() {
            *val = ((seed.wrapping_mul(i as u32 + 1) % 1000) as f32 / 1000.0) * 2.0 - 1.0;
        }

        embedding
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
        "Local Kokoro TTS (82M params, ONNX)"
    }

    fn is_initialized(&self) -> bool {
        KOKORO_SESSION.get().is_some()
    }

    async fn initialize(&self) -> Result<(), TTSError> {
        if KOKORO_SESSION.get().is_some() {
            info!("Kokoro already initialized");
            return Ok(());
        }

        let model_path = match self.find_model_path() {
            Some(path) => path,
            None => {
                warn!("Kokoro model not found. Download from:");
                warn!("  https://huggingface.co/hexgrad/Kokoro-82M/tree/main");
                warn!("Place ONNX file in: models/kokoro/kokoro-v0_19.onnx");
                return Err(TTSError::ModelNotLoaded(
                    "Kokoro ONNX model not found".into(),
                ));
            }
        };

        info!("Loading Kokoro model from: {:?}", model_path);

        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(num_cpus::get().min(4))?
            .commit_from_file(&model_path)?;

        let model = KokoroModel {
            session,
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
        // 6 samples at 24kHz should become 4 samples at 16kHz
        let input: Vec<i16> = vec![100, 200, 300, 400, 500, 600];
        let output = KokoroTTS::resample_24k_to_target(&input, 16000);
        assert_eq!(output.len(), 4);
    }
}
