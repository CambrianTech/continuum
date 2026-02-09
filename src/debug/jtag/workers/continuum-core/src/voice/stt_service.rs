//! STT Service - Handles speech-to-text transcription requests
//!
//! This is the proper layer between IPC and the STT adapters.
//! IPC should NOT directly call STT - it should call this service.

use crate::voice::stt::{self, STTError, TranscriptResult};
use crate::utils::audio::i16_to_f32;

/// Transcribe speech from audio samples using the active STT adapter
///
/// This is the ONLY function IPC should call for STT.
/// All STT logic, initialization, error handling happens here.
///
/// This is a synchronous wrapper that creates its own tokio runtime.
///
/// IMPORTANT: Always creates a NEW runtime. IPC handler threads are spawned
/// via std::thread::spawn from within #[tokio::main], so they inherit the
/// global runtime handle. Calling handle.block_on() from such threads panics
/// with "Cannot block the current thread from within a runtime". Creating a
/// fresh runtime avoids this entirely.
///
/// # Arguments
/// * `samples` - Audio samples as i16 PCM, 16kHz mono
/// * `language` - Language code (e.g., "en") or None for auto-detection
pub fn transcribe_speech_sync(
    samples: &[i16],
    language: Option<&str>,
) -> Result<TranscriptResult, STTError> {
    // Convert i16 to f32 for STT
    let f32_samples = i16_to_f32(samples);

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| STTError::InferenceFailed(format!("Failed to create runtime: {e}")))?;
    rt.block_on(async {
        transcribe_speech_impl(f32_samples, language).await
    })
}

async fn transcribe_speech_impl(
    samples: Vec<f32>,
    language: Option<&str>,
) -> Result<TranscriptResult, STTError> {
    // Initialize STT system if needed
    if !stt::is_initialized() {
        stt::init_registry();
        stt::initialize().await?;
    }

    // Use active adapter (configured in registry)
    stt::transcribe(samples, language).await
}

/// Check if STT system is ready
pub fn is_ready() -> bool {
    stt::is_initialized()
}
