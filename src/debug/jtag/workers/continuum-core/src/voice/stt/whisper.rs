//! Whisper STT Adapter
//!
//! Local Whisper inference using whisper-rs (bindings to whisper.cpp).
//! Runs on CPU with optional GPU acceleration.

use super::{STTError, SpeechToText, TranscriptResult, TranscriptSegment};
use crate::audio_constants::AUDIO_SAMPLE_RATE;
use async_trait::async_trait;
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Whisper model context (loaded once)
static WHISPER_CTX: OnceCell<Arc<Mutex<WhisperContext>>> = OnceCell::new();

/// Whisper STT Adapter - local inference
pub struct WhisperSTT {
    model_path: Option<PathBuf>,
}

impl WhisperSTT {
    pub fn new() -> Self {
        Self { model_path: None }
    }

    pub fn with_model_path(model_path: PathBuf) -> Self {
        Self {
            model_path: Some(model_path),
        }
    }

    /// Model preference order: best quality/speed ratio first.
    /// turbo is nearly as accurate as large-v3 but ~3x faster.
    const MODEL_PREFERENCE: &'static [(&'static str, &'static str)] = &[
        ("large-v3-turbo", "ggml-large-v3-turbo.bin"),
        ("large-v3", "ggml-large-v3.bin"),
        ("medium", "ggml-medium.en.bin"),
        ("small", "ggml-small.en.bin"),
        ("base", "ggml-base.en.bin"),
    ];

    /// Search directories for model files
    fn model_search_dirs() -> Vec<PathBuf> {
        let mut dirs = vec![PathBuf::from("models/whisper")];
        if let Some(data_dir) = dirs::data_dir() {
            dirs.push(data_dir.join("whisper"));
        }
        dirs.push(PathBuf::from("/usr/local/share/whisper"));
        dirs
    }

    /// Find the best available model on disk.
    ///
    /// Priority:
    /// 1. Explicit `model_path` field (constructor override)
    /// 2. `WHISPER_MODEL` env var (user override)
    /// 3. Auto-select: scan disk for best available (turbo > large-v3 > medium > small > base)
    fn find_model_path(&self) -> PathBuf {
        // 1. Explicit model path from constructor
        if let Some(ref path) = self.model_path {
            return path.clone();
        }

        let search_dirs = Self::model_search_dirs();

        // 2. WHISPER_MODEL env var override
        if let Ok(model_name) = std::env::var("WHISPER_MODEL") {
            let model_file = Self::MODEL_PREFERENCE
                .iter()
                .find(|(name, _)| *name == model_name)
                .map(|(_, file)| *file);

            if let Some(file) = model_file {
                for dir in &search_dirs {
                    let path = dir.join(file);
                    if path.exists() {
                        info!("Whisper: Using model from WHISPER_MODEL env: {} ({:?})", model_name, path);
                        return path;
                    }
                }
                warn!(
                    "Whisper: WHISPER_MODEL='{}' set but file not found, falling back to auto-select",
                    model_name
                );
            } else {
                warn!(
                    "Whisper: Unknown WHISPER_MODEL='{}', falling back to auto-select",
                    model_name
                );
            }
        }

        // 3. Auto-select: scan for best available model
        for (name, file) in Self::MODEL_PREFERENCE {
            for dir in &search_dirs {
                let path = dir.join(file);
                if path.exists() {
                    info!("Whisper: Auto-selected best available model: {} ({:?})", name, path);
                    return path;
                }
            }
        }

        // Nothing found — return default path so the error message is helpful
        warn!("Whisper: No model files found. Download from:");
        warn!("  https://huggingface.co/ggerganov/whisper.cpp/tree/main");
        warn!("  Recommended: ggml-large-v3-turbo.bin (best speed/quality)");
        warn!("  Place in: models/whisper/");
        PathBuf::from("models/whisper/ggml-large-v3-turbo.bin")
    }

    /// Synchronous transcription (runs on blocking thread)
    fn transcribe_sync(
        ctx: &Arc<Mutex<WhisperContext>>,
        mut samples: Vec<f32>,
        language: Option<&str>,
    ) -> Result<TranscriptResult, STTError> {
        if samples.is_empty() {
            return Err(STTError::InvalidAudio("Empty audio samples".into()));
        }

        // CRITICAL: Whisper requires minimum 1000ms at AUDIO_SAMPLE_RATE
        // Pad to 1050ms to account for Whisper's internal rounding (it reports 990ms for 16000 samples)
        // 1050ms * AUDIO_SAMPLE_RATE / 1000 = minimum samples needed
        let whisper_min_samples = (1050 * AUDIO_SAMPLE_RATE as usize) / 1000;
        if samples.len() < whisper_min_samples {
            let original_len = samples.len();
            let padding = whisper_min_samples - samples.len();
            samples.resize(whisper_min_samples, 0.0); // Pad with silence
            info!(
                "Whisper: Padded audio from {}ms to 1050ms ({} silence samples)",
                (original_len * 1000) / AUDIO_SAMPLE_RATE as usize,
                padding
            );
        }

        // Validate sample range
        let max_sample = samples.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        if max_sample > 1.5 {
            warn!(
                "Audio samples out of range (max: {}), may need normalization",
                max_sample
            );
        }

        let ctx_guard = ctx.lock();

        // Configure parameters
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        // Language setting
        if let Some(lang) = language {
            if lang != "auto" {
                params.set_language(Some(lang));
            }
        }

        // Performance settings
        params.set_n_threads(num_cpus::get().min(4) as i32);
        params.set_translate(false);
        params.set_no_context(true);
        params.set_single_segment(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        // Create state and run inference
        let mut state = ctx_guard
            .create_state()
            .map_err(|e| STTError::InferenceFailed(format!("Failed to create state: {e}")))?;

        state
            .full(params, &samples)
            .map_err(|e| STTError::InferenceFailed(format!("Inference failed: {e}")))?;

        // Extract results
        let num_segments = state
            .full_n_segments()
            .map_err(|e| STTError::InferenceFailed(format!("Failed to get segments: {e}")))?;

        let mut full_text = String::new();
        let mut segments = Vec::new();

        for i in 0..num_segments {
            let segment_text = state.full_get_segment_text(i).map_err(|e| {
                STTError::InferenceFailed(format!("Failed to get segment {i}: {e}"))
            })?;

            let start_ms = state
                .full_get_segment_t0(i)
                .map_err(|_| STTError::InferenceFailed("Failed to get segment start".into()))?
                * 10;

            let end_ms = state
                .full_get_segment_t1(i)
                .map_err(|_| STTError::InferenceFailed("Failed to get segment end".into()))?
                * 10;

            full_text.push_str(&segment_text);

            segments.push(TranscriptSegment {
                text: segment_text.trim().to_string(),
                start_ms,
                end_ms,
            });
        }

        // Detect language
        let detected_lang = state
            .full_lang_id_from_state()
            .map(|id| whisper_rs::get_lang_str(id).unwrap_or("en"))
            .unwrap_or("en")
            .to_string();

        Ok(TranscriptResult {
            text: full_text.trim().to_string(),
            language: detected_lang,
            confidence: 0.9, // Whisper doesn't expose confidence easily
            segments,
        })
    }
}

impl Default for WhisperSTT {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SpeechToText for WhisperSTT {
    fn name(&self) -> &'static str {
        "whisper"
    }

    fn description(&self) -> &'static str {
        "Local Whisper inference (whisper.cpp)"
    }

    fn is_initialized(&self) -> bool {
        WHISPER_CTX.get().is_some()
    }

    async fn initialize(&self) -> Result<(), STTError> {
        if WHISPER_CTX.get().is_some() {
            info!("Whisper already initialized");
            return Ok(());
        }

        let model_path = self.find_model_path();
        info!("Loading Whisper model from: {:?}", model_path);

        if !model_path.exists() {
            warn!("Whisper model not found at {:?}", model_path);
            warn!("Download from: https://huggingface.co/ggerganov/whisper.cpp/tree/main");
            warn!("Place ggml-base.en.bin in models/whisper/");

            return Err(STTError::ModelNotLoaded(format!(
                "Model not found: {model_path:?}. Download ggml-base.en.bin from HuggingFace whisper.cpp repo"
            )));
        }

        // Load model
        let params = WhisperContextParameters::default();
        let ctx = WhisperContext::new_with_params(model_path.to_str().unwrap_or(""), params)
            .map_err(|e| STTError::ModelNotLoaded(e.to_string()))?;

        WHISPER_CTX
            .set(Arc::new(Mutex::new(ctx)))
            .map_err(|_| STTError::ModelNotLoaded("Failed to set global context".into()))?;

        info!("Whisper model loaded successfully");
        Ok(())
    }

    async fn transcribe(
        &self,
        samples: Vec<f32>,
        language: Option<&str>,
    ) -> Result<TranscriptResult, STTError> {
        let ctx = WHISPER_CTX
            .get()
            .ok_or_else(|| {
                STTError::ModelNotLoaded("Whisper not initialized. Call initialize() first.".into())
            })?
            .clone();

        let lang = language.map(|s| s.to_string());

        // Run inference on blocking thread pool
        tokio::task::spawn_blocking(move || Self::transcribe_sync(&ctx, samples, lang.as_deref()))
            .await
            .map_err(|e| STTError::InferenceFailed(format!("Task join error: {e}")))?
    }

    fn supported_languages(&self) -> Vec<&'static str> {
        vec![
            "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar",
            "sv", "it", "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs", "ro", "da", "hu",
            "ta", "no", "th", "ur", "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk", "te", "fa",
            "lv", "bn", "sr", "az", "sl", "kn", "et", "mk", "br", "eu", "is", "hy", "ne", "mn",
            "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si", "km", "sn", "yo", "so", "af", "oc",
            "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo", "ht", "ps", "tk", "nn",
            "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt", "haw", "ln", "ha", "ba", "jw",
            "su",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_whisper_adapter_creation() {
        let adapter = WhisperSTT::new();
        assert_eq!(adapter.name(), "whisper");
        assert!(!adapter.is_initialized());
    }

    #[test]
    fn test_model_preference_order() {
        // turbo should be first (best speed/quality ratio)
        assert_eq!(WhisperSTT::MODEL_PREFERENCE[0].0, "large-v3-turbo");
        assert_eq!(WhisperSTT::MODEL_PREFERENCE[1].0, "large-v3");
        assert_eq!(WhisperSTT::MODEL_PREFERENCE[4].0, "base");
    }

    #[test]
    fn test_explicit_model_path() {
        let path = PathBuf::from("/tmp/test-model.bin");
        let adapter = WhisperSTT::with_model_path(path.clone());
        // Explicit path should be returned directly regardless of existence
        assert_eq!(adapter.find_model_path(), path);
    }

    #[test]
    fn test_model_search_dirs_not_empty() {
        let dirs = WhisperSTT::model_search_dirs();
        assert!(!dirs.is_empty());
        assert!(dirs[0].ends_with("models/whisper"));
    }
}
