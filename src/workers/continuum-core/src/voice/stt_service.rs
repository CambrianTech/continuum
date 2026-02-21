//! STT Service - Handles speech-to-text transcription requests
//!
//! This is the proper layer between IPC and the STT adapters.
//! IPC should NOT directly call STT - it should call this service.

use crate::voice::stt::{self, STTError, TranscriptResult};
use crate::utils::audio::i16_to_f32;

/// Transcribe speech from audio samples (async version).
///
/// Use this from async contexts (IPC handlers, tokio tasks).
/// Initializes STT system on first call.
///
/// # Arguments
/// * `samples` - Audio samples as i16 PCM, 16kHz mono
/// * `language` - Language code (e.g., "en") or None for auto-detection
pub async fn transcribe_speech_async(
    samples: &[i16],
    language: Option<&str>,
) -> Result<TranscriptResult, STTError> {
    let f32_samples = i16_to_f32(samples);

    // Initialize STT system if needed
    if !stt::is_initialized() {
        stt::init_registry();
        stt::initialize().await?;
    }

    // Use active adapter (configured in registry)
    stt::transcribe(f32_samples, language).await
}

/// Transcribe speech from audio samples (sync version).
///
/// Use this ONLY from non-async contexts (plain std::threads).
/// Creates a new tokio runtime — will PANIC if called from within
/// an existing tokio runtime (e.g., from a spawned tokio task).
///
/// For IPC handlers (which run as tokio tasks), use `transcribe_speech_async`.
pub fn transcribe_speech_sync(
    samples: &[i16],
    language: Option<&str>,
) -> Result<TranscriptResult, STTError> {
    let f32_samples = i16_to_f32(samples);

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| STTError::InferenceFailed(format!("Failed to create runtime: {e}")))?;
    rt.block_on(async {
        if !stt::is_initialized() {
            stt::init_registry();
            stt::initialize().await?;
        }
        stt::transcribe(f32_samples, language).await
    })
}

/// Check if STT system is ready
pub fn is_ready() -> bool {
    stt::is_initialized()
}
