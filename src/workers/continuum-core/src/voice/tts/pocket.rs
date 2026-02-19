//! Pocket-TTS Adapter
//!
//! Kyutai's 100M parameter CPU-native TTS using Candle.
//! Voice cloning from reference WAV audio. ≤600ms TTFA.
//!
//! Features:
//! - 100M params — lightweight, fast on any CPU
//! - Voice cloning from 5-15s reference WAV files
//! - Streaming audio generation
//! - 8 built-in preset voices
//! - int8 quantization support
//!
//! Model auto-downloads from HuggingFace on first use (~400MB).
//!
//! Voice cloning: Place reference WAV files in models/pocket-tts/voices/
//! Named <voice_id>.wav (e.g., morgan-freeman.wav).
//! Sentinels can download reference audio from the web and store here.

use super::audio_utils;
use super::{SynthesisResult, TTSError, TextToSpeech, VoiceInfo};
use crate::audio_constants::AUDIO_SAMPLE_RATE;
use async_trait::async_trait;
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

/// Preset voices shipped with Pocket-TTS (Les Misérables characters)
const PRESET_VOICES: &[(&str, &str, &str)] = &[
    ("alba", "Alba", "female"),
    ("fantine", "Fantine", "female"),
    ("cosette", "Cosette", "female"),
    ("eponine", "Eponine", "female"),
    ("azelma", "Azelma", "female"),
    ("marius", "Marius", "male"),
    ("javert", "Javert", "male"),
    ("jean", "Jean", "male"),
];

/// Default model variant (short hash identifier used by pocket-tts crate)
const DEFAULT_VARIANT: &str = "b6369a24";

/// Global model (Mutex because TTSModel::generate needs &self but voice cache needs &mut)
static POCKET_MODEL: OnceCell<Arc<Mutex<PocketState>>> = OnceCell::new();

/// Loaded model + cached voice states
struct PocketState {
    model: pocket_tts::TTSModel,
    /// Cached voice states keyed by voice identifier
    voice_cache: HashMap<String, pocket_tts::ModelState>,
    /// Model's native sample rate (Hz)
    native_sample_rate: u32,
}

/// Pocket-TTS Adapter
///
/// 100M parameter CPU-native TTS with voice cloning.
/// Uses Candle for inference — no Python, no GPU required.
/// Auto-downloads model from HuggingFace Hub on first init.
pub struct PocketTTS {
    /// Optional directory containing reference WAV files for voice cloning
    voice_dir: Option<PathBuf>,
}

impl PocketTTS {
    pub fn new() -> Self {
        Self { voice_dir: None }
    }

    pub fn with_voice_dir(dir: PathBuf) -> Self {
        Self {
            voice_dir: Some(dir),
        }
    }

    /// Standard search directories for reference voice WAV files
    fn voice_search_dirs() -> Vec<PathBuf> {
        let mut dirs = vec![PathBuf::from("models/pocket-tts/voices")];
        if let Some(data_dir) = dirs::data_dir() {
            dirs.push(data_dir.join("pocket-tts/voices"));
        }
        dirs
    }

    /// Find a WAV file for voice cloning by name
    fn find_voice_wav(&self, voice_name: &str) -> Option<PathBuf> {
        if let Some(ref dir) = self.voice_dir {
            let path = dir.join(format!("{voice_name}.wav"));
            if path.exists() {
                return Some(path);
            }
        }

        for dir in Self::voice_search_dirs() {
            let path = dir.join(format!("{voice_name}.wav"));
            if path.exists() {
                return Some(path);
            }
        }

        None
    }

    /// Synchronous synthesis pipeline (runs on blocking thread via Mutex)
    fn synthesize_sync(
        state: &mut PocketState,
        text: &str,
        voice: &str,
        voice_wav: Option<PathBuf>,
    ) -> Result<SynthesisResult, TTSError> {
        if text.is_empty() {
            return Err(TTSError::InvalidText("Empty text".into()));
        }

        // Resolve voice state (cached or new)
        let voice_state = Self::resolve_voice_state(state, voice, voice_wav.as_deref())?;

        info!(
            "Pocket-TTS: Synthesizing with voice '{}': '{}'",
            voice,
            super::truncate_str(text, 50)
        );

        let start = std::time::Instant::now();

        // Generate audio tensor
        let audio_tensor = state
            .model
            .generate(text, &voice_state)
            .map_err(|e| TTSError::SynthesisFailed(format!("Generation failed: {e}")))?;

        let gen_ms = start.elapsed().as_millis();

        // Tensor → f32 PCM
        let f32_samples: Vec<f32> = audio_tensor
            .flatten_all()
            .and_then(|t| t.to_vec1())
            .map_err(|e| TTSError::SynthesisFailed(format!("Tensor extract: {e}")))?;

        // Normalize to standard 16kHz i16 PCM via shared audio utilities
        let result = audio_utils::normalize_audio(&f32_samples, state.native_sample_rate)?;

        info!(
            "Pocket-TTS: {} samples ({}ms audio) in {}ms",
            result.samples.len(),
            result.duration_ms,
            gen_ms
        );

        Ok(result)
    }

    /// Load a preset voice embedding as audio_prompt safetensors.
    ///
    /// Preset voices are pre-computed audio prompts (safetensors with 'audio_prompt' key).
    /// Downloaded from HF embeddings/ directory, cached locally.
    ///
    /// Sources (tried in order):
    /// 1. Local: models/pocket-tts/voices/<name>.safetensors
    /// 2. HuggingFace: hf://kyutai/pocket-tts/embeddings/<name>.safetensors
    fn load_preset_voice(
        model: &pocket_tts::TTSModel,
        name: &str,
    ) -> Result<pocket_tts::ModelState, TTSError> {
        // Try local file first, then download from HF
        let file_path = Self::find_voice_embedding(name)
            .map(Ok)
            .unwrap_or_else(|| {
                let hf_path = format!("hf://kyutai/pocket-tts/embeddings/{name}.safetensors");
                info!("Pocket-TTS: Downloading preset voice '{}' from HF", name);
                pocket_tts::weights::download_if_necessary(&hf_path)
                    .map_err(|e| {
                        let msg = format!("{e}");
                        if msg.contains("403") || msg.contains("401") {
                            TTSError::ModelNotLoaded(format!(
                                "Pocket-TTS voice '{name}' requires HuggingFace access. \
                                 Accept terms at: https://huggingface.co/kyutai/pocket-tts \
                                 then set HF_TOKEN in .continuum/config.env"
                            ))
                        } else {
                            TTSError::SynthesisFailed(format!("Voice download failed: {e}"))
                        }
                    })
            })?;

        info!("Pocket-TTS: Loading preset voice '{}' from {:?}", name, file_path);
        model
            .get_voice_state_from_prompt_file(&file_path)
            .map_err(|e| TTSError::SynthesisFailed(format!("Preset voice '{name}' load failed: {e}")))
    }

    /// Find a local .safetensors embedding file for a preset voice
    fn find_voice_embedding(name: &str) -> Option<PathBuf> {
        let filename = format!("{name}.safetensors");
        for dir in Self::voice_search_dirs() {
            let path = dir.join(&filename);
            if path.exists() {
                return Some(path);
            }
        }
        None
    }

    /// Get or create a voice state with caching
    fn resolve_voice_state(
        state: &mut PocketState,
        voice: &str,
        voice_wav: Option<&Path>,
    ) -> Result<pocket_tts::ModelState, TTSError> {
        // Cache hit
        if let Some(cached) = state.voice_cache.get(voice) {
            return Ok(cached.clone());
        }

        let voice_state = if let Some(wav_path) = voice_wav {
            // Voice cloning from reference WAV audio
            info!(
                "Pocket-TTS: Cloning voice from {:?}",
                wav_path.file_name().unwrap_or_default()
            );
            state
                .model
                .get_voice_state(wav_path)
                .map_err(|e| TTSError::SynthesisFailed(format!("Voice clone failed: {e}")))?
        } else if PRESET_VOICES.iter().any(|(id, _, _)| *id == voice) {
            // Preset voice — load audio_prompt safetensors embedding
            Self::load_preset_voice(&state.model, voice)?
        } else {
            // Unknown voice — use default preset
            info!("Pocket-TTS: Unknown voice '{}', using 'alba'", voice);
            Self::load_preset_voice(&state.model, "alba")?
        };

        state
            .voice_cache
            .insert(voice.to_string(), voice_state.clone());

        Ok(voice_state)
    }

    /// Discover WAV files in voice directories
    fn discover_cloned_voices(&self) -> Vec<VoiceInfo> {
        let mut voices = Vec::new();
        let mut seen = std::collections::HashSet::new();

        let mut dirs = Self::voice_search_dirs();
        if let Some(ref dir) = self.voice_dir {
            dirs.insert(0, dir.clone());
        }

        for dir in &dirs {
            let entries = match std::fs::read_dir(dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "wav") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if seen.insert(stem.to_string()) {
                            voices.push(VoiceInfo {
                                id: stem.to_string(),
                                name: stem.to_string(),
                                language: "en".to_string(),
                                gender: None,
                                description: Some("Cloned voice from reference audio".into()),
                            });
                        }
                    }
                }
            }
        }

        voices
    }
}

impl Default for PocketTTS {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TextToSpeech for PocketTTS {
    fn name(&self) -> &'static str {
        "pocket"
    }

    fn description(&self) -> &'static str {
        "Pocket-TTS (100M, Candle) — fast CPU TTS with voice cloning from reference audio"
    }

    fn is_initialized(&self) -> bool {
        POCKET_MODEL.get().is_some()
    }

    async fn initialize(&self) -> Result<(), TTSError> {
        if POCKET_MODEL.get().is_some() {
            info!("Pocket-TTS: Already initialized");
            return Ok(());
        }

        info!(
            "Pocket-TTS: Loading model '{}' (auto-download from HuggingFace)...",
            DEFAULT_VARIANT
        );

        // Model load is CPU-bound + network I/O (first run downloads ~400MB)
        let model = tokio::task::spawn_blocking(|| {
            pocket_tts::TTSModel::load(DEFAULT_VARIANT)
                .map_err(|e| {
                    let msg = format!("{e}");
                    if msg.contains("403") || msg.contains("status code 403") {
                        TTSError::ModelNotLoaded(
                            "Pocket-TTS model is gated on HuggingFace. \
                             To enable: 1) Accept terms at https://huggingface.co/kyutai/pocket-tts \
                             2) Set HF_TOKEN in .continuum/config.env \
                             System will use next available TTS adapter.".into()
                        )
                    } else {
                        TTSError::ModelNotLoaded(format!("Pocket-TTS model load: {e}"))
                    }
                })
        })
        .await
        .map_err(|e| TTSError::ModelNotLoaded(format!("Task join: {e}")))??;

        let native_sample_rate = model.sample_rate as u32;
        info!(
            "Pocket-TTS: Model loaded (native rate: {}Hz, output: {}Hz)",
            native_sample_rate, AUDIO_SAMPLE_RATE
        );

        let pocket_state = PocketState {
            model,
            voice_cache: HashMap::new(),
            native_sample_rate,
        };

        let _ = POCKET_MODEL
            .set(Arc::new(Mutex::new(pocket_state)));
        // OnceLock::set Err = another thread already initialized — that's fine

        info!(
            "Pocket-TTS: Ready — {} preset voices + WAV voice cloning",
            PRESET_VOICES.len()
        );
        Ok(())
    }

    async fn synthesize(&self, text: &str, voice: &str) -> Result<SynthesisResult, TTSError> {
        let model_arc = POCKET_MODEL
            .get()
            .ok_or_else(|| {
                TTSError::ModelNotLoaded("Pocket-TTS not initialized. Call initialize() first.".into())
            })?
            .clone();

        // Check for WAV file voice cloning
        let voice_wav = if voice.ends_with(".wav") && Path::new(voice).exists() {
            Some(PathBuf::from(voice))
        } else {
            self.find_voice_wav(voice)
        };

        let text = text.to_string();
        let voice = voice.to_string();

        // Run synthesis on blocking thread (CPU-bound inference)
        tokio::task::spawn_blocking(move || {
            let mut state = model_arc.lock();
            Self::synthesize_sync(&mut state, &text, &voice, voice_wav)
        })
        .await
        .map_err(|e| TTSError::SynthesisFailed(format!("Task join: {e}")))?
    }

    fn available_voices(&self) -> Vec<VoiceInfo> {
        let mut voices: Vec<VoiceInfo> = PRESET_VOICES
            .iter()
            .map(|(id, name, gender)| VoiceInfo {
                id: id.to_string(),
                name: name.to_string(),
                language: "en".to_string(),
                gender: Some(gender.to_string()),
                description: Some(format!("Pocket-TTS preset ({gender})")),
            })
            .collect();

        // Append discovered cloned voices from WAV files
        voices.extend(self.discover_cloned_voices());
        voices
    }

    fn default_voice(&self) -> &str {
        "alba"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pocket_tts_basics() {
        let adapter = PocketTTS::new();
        assert_eq!(adapter.name(), "pocket");
        assert!(!adapter.is_initialized());
        assert_eq!(adapter.default_voice(), "alba");
    }

    #[test]
    fn test_preset_voices() {
        let adapter = PocketTTS::new();
        let voices = adapter.available_voices();
        assert!(voices.len() >= PRESET_VOICES.len());

        let ids: Vec<&str> = voices.iter().map(|v| v.id.as_str()).collect();
        assert!(ids.contains(&"alba"));
        assert!(ids.contains(&"marius"));
        assert!(ids.contains(&"cosette"));
        assert!(ids.contains(&"javert"));
    }

    #[test]
    fn test_voice_search_dirs() {
        let dirs = PocketTTS::voice_search_dirs();
        assert!(!dirs.is_empty());
        assert!(dirs[0].ends_with("models/pocket-tts/voices"));
    }

    #[test]
    fn test_with_voice_dir() {
        let dir = PathBuf::from("/tmp/test-voices");
        let adapter = PocketTTS::with_voice_dir(dir);
        assert!(adapter.voice_dir.is_some());
    }

    /// Integration test — requires model download (~400MB first run)
    #[test]
    #[ignore]
    fn test_pocket_tts_synthesize_live() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let adapter = PocketTTS::new();

        rt.block_on(async {
            adapter.initialize().await.expect("Init should succeed");
        });

        assert!(adapter.is_initialized());

        let result = rt.block_on(async {
            adapter
                .synthesize("Hello world, this is Pocket TTS.", "alba")
                .await
        });

        let synthesis = result.expect("Synthesis should succeed");
        assert!(synthesis.samples.len() > 1000, "Should produce audio");
        assert_eq!(synthesis.sample_rate, AUDIO_SAMPLE_RATE);
        assert!(synthesis.duration_ms > 200, "Should be >200ms for a sentence");

        let max_amp = synthesis.samples.iter().map(|s| s.abs()).max().unwrap_or(0);
        assert!(max_amp > 100, "Audio should not be silence, max amp: {}", max_amp);

        println!(
            "Pocket-TTS: {} samples, {}Hz, {}ms, max amp: {}",
            synthesis.samples.len(),
            synthesis.sample_rate,
            synthesis.duration_ms,
            max_amp
        );
    }
}
