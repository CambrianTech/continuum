//! Whisper STT Adapter
//!
//! Local Whisper inference using whisper-rs (bindings to whisper.cpp).
//! Runs on CPU with optional GPU acceleration.

use super::{STTError, SpeechToText, TranscriptResult, TranscriptSegment};
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

    /// Find the model file in common locations
    fn find_model_path(&self) -> PathBuf {
        if let Some(ref path) = self.model_path {
            return path.clone();
        }

        // Get model preference from WHISPER_MODEL env var (default: large-v3-turbo)
        let model_name = std::env::var("WHISPER_MODEL").unwrap_or_else(|_| "large-v3-turbo".to_string());

        // Map model name to filename
        let model_file = match model_name.as_str() {
            "base" => "ggml-base.en.bin",
            "small" => "ggml-small.en.bin",
            "medium" => "ggml-medium.en.bin",
            "large-v3" => "ggml-large-v3.bin",
            "large-v3-turbo" => "ggml-large-v3-turbo.bin",
            _ => {
                tracing::warn!("Unknown WHISPER_MODEL='{}', defaulting to large-v3-turbo", model_name);
                "ggml-large-v3-turbo.bin"
            }
        };

        // Search for the model in common locations
        let candidates = vec![
            PathBuf::from(format!("models/whisper/{}", model_file)),
            dirs::data_dir()
                .unwrap_or_default()
                .join(format!("whisper/{}", model_file)),
            PathBuf::from(format!("/usr/local/share/whisper/{}", model_file)),
        ];

        for path in &candidates {
            if path.exists() {
                return path.clone();
            }
        }

        // Default - will fail if not found, but error message will be helpful
        PathBuf::from(format!("models/whisper/{}", model_file))
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

        // CRITICAL: Whisper requires minimum 1000ms at 16kHz
        // Pad to 1050ms to account for Whisper's internal rounding (it reports 990ms for 16000 samples)
        const WHISPER_MIN_SAMPLES: usize = 16800; // 1050ms at 16kHz (safety margin)
        if samples.len() < WHISPER_MIN_SAMPLES {
            let original_len = samples.len();
            let padding = WHISPER_MIN_SAMPLES - samples.len();
            samples.resize(WHISPER_MIN_SAMPLES, 0.0); // Pad with silence
            info!(
                "Whisper: Padded audio from {}ms to 1050ms ({} silence samples)",
                (original_len * 1000) / 16000,
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
}
