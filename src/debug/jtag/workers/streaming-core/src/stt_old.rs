//! Speech-to-Text (STT) using Whisper
//!
//! Runs Whisper inference on a dedicated thread pool to avoid blocking async runtime.
//! Uses whisper-rs (bindings to whisper.cpp) for efficient CPU/GPU inference.
//!
//! Architecture:
//! - Model loaded once at startup into static
//! - Inference runs on rayon thread pool via spawn_blocking
//! - Audio resampled to 16kHz (Whisper's native rate) if needed

use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tracing::{error, info, warn};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Global Whisper context (loaded once, reused)
static WHISPER_CTX: OnceCell<Arc<Mutex<WhisperContext>>> = OnceCell::new();

/// STT errors
#[derive(Error, Debug)]
pub enum STTError {
    #[error("Model not loaded: {0}")]
    ModelNotLoaded(String),

    #[error("Inference failed: {0}")]
    InferenceFailed(String),

    #[error("Invalid audio: {0}")]
    InvalidAudio(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Transcription result
#[derive(Debug, Clone)]
pub struct TranscriptResult {
    pub text: String,
    pub language: String,
    pub confidence: f32,
    pub segments: Vec<TranscriptSegment>,
}

/// Word/phrase segment with timing
#[derive(Debug, Clone)]
pub struct TranscriptSegment {
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
}

/// Initialize Whisper model (call once at startup)
pub fn init_whisper(model_path: Option<PathBuf>) -> Result<(), STTError> {
    if WHISPER_CTX.get().is_some() {
        info!("Whisper already initialized");
        return Ok(());
    }

    // Find model path
    let model_path = model_path.unwrap_or_else(|| {
        // Check common locations
        let candidates = [
            PathBuf::from("models/whisper/ggml-base.en.bin"),
            PathBuf::from("models/whisper/ggml-base.bin"),
            PathBuf::from("models/ggml-base.en.bin"),
            dirs::data_dir()
                .unwrap_or_default()
                .join("whisper/ggml-base.en.bin"),
            PathBuf::from("/usr/local/share/whisper/ggml-base.en.bin"),
        ];

        for path in candidates {
            if path.exists() {
                return path;
            }
        }

        // Default - will fail if not found
        PathBuf::from("models/whisper/ggml-base.en.bin")
    });

    info!("Loading Whisper model from: {:?}", model_path);

    if !model_path.exists() {
        // Try to download the model
        warn!("Whisper model not found at {:?}", model_path);
        warn!("Download from: https://huggingface.co/ggerganov/whisper.cpp/tree/main");
        warn!("Place ggml-base.en.bin in models/whisper/");

        return Err(STTError::ModelNotLoaded(format!(
            "Model not found: {:?}. Download ggml-base.en.bin from HuggingFace whisper.cpp repo",
            model_path
        )));
    }

    // Load model with GPU acceleration if available
    let params = WhisperContextParameters::default();

    let ctx = WhisperContext::new_with_params(
        model_path.to_str().unwrap_or(""),
        params,
    )
    .map_err(|e| STTError::ModelNotLoaded(e.to_string()))?;

    WHISPER_CTX
        .set(Arc::new(Mutex::new(ctx)))
        .map_err(|_| STTError::ModelNotLoaded("Failed to set global context".into()))?;

    info!("Whisper model loaded successfully");
    Ok(())
}

/// Check if Whisper is initialized
pub fn is_whisper_initialized() -> bool {
    WHISPER_CTX.get().is_some()
}

/// Transcribe audio samples (runs on thread pool, not blocking async)
///
/// # Arguments
/// * `samples` - Audio samples as f32 (-1.0 to 1.0), must be 16kHz mono
/// * `language` - Language code (e.g., "en") or "auto" for detection
///
/// # Returns
/// Transcription result with text, detected language, and segments
pub async fn transcribe(samples: Vec<f32>, language: Option<&str>) -> Result<TranscriptResult, STTError> {
    let ctx = WHISPER_CTX
        .get()
        .ok_or_else(|| STTError::ModelNotLoaded("Whisper not initialized. Call init_whisper() first.".into()))?
        .clone();

    let lang = language.map(|s| s.to_string());

    // Run inference on blocking thread pool (not tokio runtime)
    tokio::task::spawn_blocking(move || {
        transcribe_sync(&ctx, samples, lang.as_deref())
    })
    .await
    .map_err(|e| STTError::InferenceFailed(format!("Task join error: {}", e)))?
}

/// Synchronous transcription (runs on rayon/blocking thread)
fn transcribe_sync(
    ctx: &Arc<Mutex<WhisperContext>>,
    samples: Vec<f32>,
    language: Option<&str>,
) -> Result<TranscriptResult, STTError> {
    if samples.is_empty() {
        return Err(STTError::InvalidAudio("Empty audio samples".into()));
    }

    // Validate sample range
    let max_sample = samples.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
    if max_sample > 1.5 {
        warn!("Audio samples out of range (max: {}), may need normalization", max_sample);
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
    params.set_n_threads(num_cpus::get().min(4) as i32); // Use up to 4 threads
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
        .map_err(|e| STTError::InferenceFailed(format!("Failed to create state: {}", e)))?;

    state
        .full(params, &samples)
        .map_err(|e| STTError::InferenceFailed(format!("Inference failed: {}", e)))?;

    // Extract results
    let num_segments = state.full_n_segments()
        .map_err(|e| STTError::InferenceFailed(format!("Failed to get segments: {}", e)))?;

    let mut full_text = String::new();
    let mut segments = Vec::new();

    for i in 0..num_segments {
        let segment_text = state
            .full_get_segment_text(i)
            .map_err(|e| STTError::InferenceFailed(format!("Failed to get segment {}: {}", i, e)))?;

        let start_ms = state
            .full_get_segment_t0(i)
            .map_err(|_| STTError::InferenceFailed("Failed to get segment start".into()))?
            as i64 * 10; // Convert to ms

        let end_ms = state
            .full_get_segment_t1(i)
            .map_err(|_| STTError::InferenceFailed("Failed to get segment end".into()))?
            as i64 * 10;

        full_text.push_str(&segment_text);

        segments.push(TranscriptSegment {
            text: segment_text.trim().to_string(),
            start_ms,
            end_ms,
        });
    }

    // Detect language (Whisper auto-detection)
    let detected_lang = state
        .full_lang_id_from_state()
        .map(|id| whisper_rs::get_lang_str(id).unwrap_or("en"))
        .unwrap_or("en")
        .to_string();

    Ok(TranscriptResult {
        text: full_text.trim().to_string(),
        language: detected_lang,
        confidence: 0.9, // Whisper doesn't expose confidence scores easily
        segments,
    })
}

/// Convert i16 PCM samples to f32 (-1.0 to 1.0)
pub fn i16_to_f32(samples: &[i16]) -> Vec<f32> {
    samples.iter().map(|&s| s as f32 / 32768.0).collect()
}

/// Resample audio to 16kHz (Whisper's native rate)
pub fn resample_to_16k(samples: &[f32], from_rate: u32) -> Vec<f32> {
    if from_rate == 16000 {
        return samples.to_vec();
    }

    use rubato::Resampler;

    let params = rubato::FftFixedInOut::<f32>::new(
        from_rate as usize,
        16000,
        samples.len().min(1024),
        1, // mono
    );

    match params {
        Ok(mut resampler) => {
            let input = vec![samples.to_vec()];
            match resampler.process(&input, None) {
                Ok(output) => output.into_iter().next().unwrap_or_default(),
                Err(e) => {
                    error!("Resample failed: {}", e);
                    samples.to_vec()
                }
            }
        }
        Err(e) => {
            error!("Failed to create resampler: {}", e);
            samples.to_vec()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_i16_to_f32_conversion() {
        let samples: Vec<i16> = vec![0, 16384, -16384, 32767, -32768];
        let f32_samples = i16_to_f32(&samples);

        assert!((f32_samples[0] - 0.0).abs() < 0.001);
        assert!((f32_samples[1] - 0.5).abs() < 0.01);
        assert!((f32_samples[2] - -0.5).abs() < 0.01);
        assert!((f32_samples[3] - 1.0).abs() < 0.01);
        assert!((f32_samples[4] - -1.0).abs() < 0.01);
    }
}
