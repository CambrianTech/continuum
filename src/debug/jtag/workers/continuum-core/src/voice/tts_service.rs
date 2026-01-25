//! TTS Service - Handles text-to-speech synthesis requests
//!
//! This is the proper layer between IPC and the TTS adapters.
//! IPC should NOT directly call TTS - it should call this service.

use crate::voice::tts::{self, SynthesisResult, TTSError};

/// Synthesize speech from text using the active TTS adapter
///
/// This is the ONLY function IPC should call for TTS.
/// All TTS logic, initialization, error handling happens here.
///
/// This is a synchronous wrapper that creates its own runtime if needed.
pub fn synthesize_speech_sync(
    text: &str,
    voice: Option<&str>,
    _adapter: Option<&str>,
) -> Result<SynthesisResult, TTSError> {
    // Try to use existing runtime, or create one
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            // We're in an async context, use it
            handle.block_on(async {
                synthesize_speech_impl(text, voice, _adapter).await
            })
        },
        Err(_) => {
            // No runtime, create one
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| TTSError::SynthesisFailed(format!("Failed to create runtime: {}", e)))?;
            rt.block_on(async {
                synthesize_speech_impl(text, voice, _adapter).await
            })
        }
    }
}

async fn synthesize_speech_impl(
    text: &str,
    voice: Option<&str>,
    _adapter: Option<&str>,
) -> Result<SynthesisResult, TTSError> {
    // Initialize TTS system if needed
    if !tts::is_initialized() {
        tts::init_registry();
        tts::initialize().await?;
    }

    // Use active adapter (configured in registry)
    let voice_id = voice.unwrap_or("default");
    tts::synthesize(text, voice_id).await
}

/// Check if TTS system is ready
pub fn is_ready() -> bool {
    tts::is_initialized()
}

/// Get available voices
pub fn get_voices() -> Vec<crate::voice::tts::VoiceInfo> {
    tts::available_voices()
}
