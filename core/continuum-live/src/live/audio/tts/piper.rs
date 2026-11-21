//! Piper TTS Adapter
//!
//! Local TTS inference using Piper (ONNX-based, no Python dependencies).
//! High-quality voices, efficient inference, designed for production use.
//! Used by Home Assistant and other production systems.

use super::normalize;
use super::{Phonemizer, SynthesisResult, TTSError, TextToSpeech, VoiceInfo};
use crate::gpu::memory_manager::{GpuPriority, GpuSubsystem};
use crate::gpu::tracker::GpuModelTracker;
use crate::live::audio::reloadable::ReloadableModel;
use crate::{clog_info, clog_warn};
use async_trait::async_trait;
use continuum_base::part::{Identity, Part};
use ndarray;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;

/// Global Piper session
static PIPER_SESSION: ReloadableModel<Mutex<PiperModel>> = ReloadableModel::new("Piper");

/// GPU allocation tracking for Piper ONNX model
static PIPER_GPU: GpuModelTracker = GpuModelTracker::new("Piper");

/// Piper model wrapper
struct PiperModel {
    session: Session,
    /// The model's speaker count and output rate, read from its own `.onnx.json`.
    voice: PiperVoiceConfig,
    phonemizer: Phonemizer,
}

/// The one voice Piper advertises: the model's first speaker.
const DEFAULT_VOICE: &str = "default";

/// The facts a Piper voice config (`<model>.onnx.json`) states about its model.
#[derive(serde::Deserialize)]
struct PiperVoiceConfig {
    num_speakers: u32,
    audio: PiperAudioConfig,
}

#[derive(serde::Deserialize)]
struct PiperAudioConfig {
    sample_rate: u32,
}

impl PiperVoiceConfig {
    /// Read the config beside the model (a disk format Piper's exporter writes).
    fn load(path: &std::path::Path) -> Result<Self, TTSError> {
        let file = std::fs::File::open(path)?;
        serde_json::from_reader(std::io::BufReader::new(file))
            .map_err(|cause| TTSError::ModelConfig { path: path.to_path_buf(), cause })
    }

    /// The speaker a voice id names: the advertised default voice is speaker 0 and a number
    /// names that speaker. Any other id, or a speaker the model does not have, is refused.
    fn speaker_id(&self, voice: &str) -> Result<i64, TTSError> {
        let speaker = if voice == DEFAULT_VOICE {
            0
        } else {
            voice.parse::<u32>().map_err(TTSError::SpeakerUnparsable)?
        };
        if speaker >= self.num_speakers {
            return Err(TTSError::SpeakerOutOfRange { speaker, speakers: self.num_speakers });
        }
        Ok(i64::from(speaker))
    }
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

        use crate::live::audio::model_root::voice_model_path;
        let mut candidates = vec![
            voice_model_path("piper/en_US-libritts_r-medium.onnx"), // Primary
            voice_model_path("piper/en_US-amy-medium.onnx"),        // Alternative
            voice_model_path("piper/piper.onnx"),                   // Generic
            voice_model_path("tts/piper.onnx"),
        ];
        // A platform with no data directory has no candidate there (never a cwd-relative one).
        if let Some(data_dir) = dirs::data_dir() {
            candidates.push(data_dir.join("piper/en_US-libritts_r-medium.onnx"));
        }
        candidates.push(PathBuf::from("/usr/local/share/piper/en_US-libritts_r-medium.onnx"));

        candidates.into_iter().find(|path| path.exists())
    }

    /// Synchronous synthesis
    fn synthesize_sync(
        session: &Arc<Mutex<PiperModel>>,
        text: &str,
        voice: &str, // "default" (speaker 0) or a speaker number the model has
        _speed: f32, // TODO: Implement speed control via length_scale parameter
    ) -> Result<SynthesisResult, TTSError> {
        if text.is_empty() {
            return Err(TTSError::InvalidText("Text cannot be empty".into()));
        }

        let mut model = session.lock();
        let sample_rate = model.voice.audio.sample_rate; // Cache before mutable borrow
        let speaker_id = model.voice.speaker_id(voice)?;

        // Phonemize text to get phoneme IDs using model's phonemizer
        let phoneme_ids = model.phonemizer.text_to_phoneme_ids(text);

        // Reshape to [1, len] for batch dimension
        let len = phoneme_ids.len();
        let text_array = ndarray::Array2::from_shape_vec((1, len), phoneme_ids)
            .map_err(|e| TTSError::SynthesisFailed(format!("Failed to reshape input: {e}")))?;

        let sid_array = ndarray::Array1::from_vec(vec![speaker_id]);

        // Inference parameters from model config
        // Format: [noise_scale, length_scale, noise_w]
        let scales_array = ndarray::Array1::from_vec(vec![0.333_f32, 1.0_f32, 0.333_f32]);

        // Run inference — ort 2.x requires explicit Tensor/TensorRef construction
        let input_lengths = ndarray::Array1::from_vec(vec![len as i64]);
        let input_tensor = ort::value::TensorRef::from_array_view(text_array.view())
            .map_err(|e| TTSError::SynthesisFailed(format!("Input tensor: {e}")))?;
        let lengths_tensor = ort::value::TensorRef::from_array_view(input_lengths.view())
            .map_err(|e| TTSError::SynthesisFailed(format!("Lengths tensor: {e}")))?;
        let scales_tensor = ort::value::TensorRef::from_array_view(scales_array.view())
            .map_err(|e| TTSError::SynthesisFailed(format!("Scales tensor: {e}")))?;
        let sid_tensor = ort::value::TensorRef::from_array_view(sid_array.view())
            .map_err(|e| TTSError::SynthesisFailed(format!("SID tensor: {e}")))?;
        let outputs = model
            .session
            .run(ort::inputs![
                "input" => input_tensor,
                "input_lengths" => lengths_tensor,
                "scales" => scales_tensor,
                "sid" => sid_tensor
            ])
            .map_err(|e| TTSError::SynthesisFailed(format!("ONNX inference failed: {e}")))?;

        // Extract audio
        let (_, audio_data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| TTSError::SynthesisFailed(format!("Failed to extract audio: {e}")))?;

        // Normalize to standard 16kHz i16 PCM via shared audio utilities
        let f32_samples: Vec<f32> = audio_data.to_vec();
        let result = normalize::normalize_audio(&f32_samples, sample_rate)?;

        clog_info!(
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

impl Part for PiperTTS {
    fn identity(&self) -> Identity {
        Identity { name: "piper", description: "Piper TTS (high-quality, ONNX-based)" }
    }
}

#[async_trait]
impl TextToSpeech for PiperTTS {
    fn is_initialized(&self) -> bool {
        PIPER_SESSION.is_loaded()
    }

    async fn initialize(&self) -> Result<(), TTSError> {
        if PIPER_SESSION.is_loaded() {
            clog_info!("Piper already initialized");
            return Ok(());
        }

        let model_path = match self.find_model_path() {
            Some(path) => path,
            None => {
                clog_warn!("Piper model not found. Should be auto-downloaded at:");
                clog_warn!("  models/piper/en_US-libritts_r-medium.onnx");
                return Err(TTSError::ModelNotLoaded(
                    "Piper ONNX model not found".into(),
                ));
            }
        };

        clog_info!("Loading Piper model from: {:?}", model_path);

        let session = {
            let mut builder = Session::builder()?;
            // GPU execution providers via the centralized helper. Per
            // architecture, CPU fallback is forbidden — TTS matmul must
            // land on GPU. The prior cfg gate (`feature = "coreml"`)
            // didn't match any actual cargo feature, so the CoreML EP
            // was never added — ORT's implicit CPU EP took every op
            // (#964 family). The helper uses the correct `feature =
            // "metal"` gate that matches Cargo.toml.
            let providers = crate::inference::ort_providers::build_ort_gpu_execution_providers()
                .map_err(|e| {
                    TTSError::ModelNotLoaded(format!("ORT GPU EP setup failed (Piper TTS): {e}"))
                })?;
            builder = builder.with_execution_providers(providers)?;
            builder
                .with_optimization_level(GraphOptimizationLevel::Level3)?
                .with_intra_threads(num_cpus::get().min(4))?
                .commit_from_file(&model_path)?
        };

        // Load phonemizer and the model's voice facts from the model's own config
        let config_path = model_path.with_extension("onnx.json");
        let config_path_text = config_path
            .to_str()
            .ok_or_else(|| TTSError::NonUtf8Path(config_path.clone()))?;
        let phonemizer = Phonemizer::load_from_config(config_path_text)
            .map_err(|e| TTSError::ModelNotLoaded(format!("Failed to load phonemizer: {e}")))?;
        let voice = PiperVoiceConfig::load(&config_path)?;

        let model = PiperModel {
            session,
            voice,
            phonemizer,
        };

        // Track GPU/memory allocation for ONNX model (non-critical: proceed on failure)
        let _ = PIPER_GPU.track_file(
            GpuSubsystem::Tts,
            &model_path,
            super::gpu_manager(),
            GpuPriority::Interactive,
        );

        let _ = PIPER_SESSION.load_with(|| Ok::<_, TTSError>(Mutex::new(model)));

        clog_info!("Piper model loaded successfully");
        Ok(())
    }

    async fn synthesize(&self, text: &str, voice: &str) -> Result<SynthesisResult, TTSError> {
        PIPER_GPU.touch();

        let session = PIPER_SESSION
            .get()
            .ok_or_else(|| TTSError::ModelNotLoaded("Piper not initialized".into()))?;

        let text = text.to_string();
        let voice = voice.to_string();

        tokio::task::spawn_blocking(move || Self::synthesize_sync(&session, &text, &voice, 1.0))
            .await
            .map_err(|e| TTSError::SynthesisFailed(format!("Task join error: {e}")))?
    }

    async fn shutdown(&self) -> Result<(), TTSError> {
        if PIPER_SESSION.unload() {
            PIPER_GPU.release();
            clog_info!("Piper: Models unloaded (~100MB freed)");
        }
        Ok(())
    }

    fn available_voices(&self) -> Vec<VoiceInfo> {
        vec![VoiceInfo {
            id: DEFAULT_VOICE.to_string(),
            name: "LibriTTS (High Quality)".to_string(),
            language: "en-US".to_string(),
            gender: Some("neutral".to_string()),
            description: Some("High-quality English voice from LibriTTS dataset".to_string()),
        }]
    }

    fn default_voice(&self) -> &str {
        DEFAULT_VOICE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_piper_adapter() {
        let adapter = PiperTTS::new();
        assert_eq!(adapter.identity().name, "piper");
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

    // what this catches: an unparsable voice id spoken as speaker 0 and an out-of-range speaker
    // clamped to a guessed ceiling; the speaker count is the model's own config.
    #[test]
    fn a_speaker_is_one_the_model_has() {
        let config: PiperVoiceConfig =
            serde_json::from_str(r#"{"num_speakers": 3, "audio": {"sample_rate": 22050}}"#).unwrap();
        assert_eq!(config.speaker_id(DEFAULT_VOICE).unwrap(), 0);
        assert_eq!(config.speaker_id("2").unwrap(), 2);
        assert!(config.speaker_id("3").is_err(), "the model has speakers 0..3");
        assert!(config.speaker_id("af_bella").is_err(), "another adapter's voice is no speaker");
    }
}
