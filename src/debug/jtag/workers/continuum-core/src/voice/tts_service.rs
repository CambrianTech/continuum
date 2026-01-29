//! TTS Service - Handles text-to-speech synthesis requests
//!
//! This is the proper layer between IPC and the TTS adapters.
//! IPC should NOT directly call TTS - it should call this service.

use crate::voice::tts::{self, SynthesisResult, TTSError};

/// Synthesize speech from text using a TTS adapter
///
/// This is the ONLY function IPC should call for TTS.
/// All TTS logic, initialization, error handling happens here.
///
/// If `adapter` is specified, uses that adapter directly.
/// Otherwise, uses the active adapter from the registry.
///
/// This is a synchronous wrapper that creates its own tokio runtime.
///
/// IMPORTANT: Always creates a NEW runtime. IPC handler threads are spawned
/// via std::thread::spawn from within #[tokio::main], so they inherit the
/// global runtime handle. Calling handle.block_on() from such threads panics
/// with "Cannot block the current thread from within a runtime". Creating a
/// fresh runtime avoids this entirely.
pub fn synthesize_speech_sync(
    text: &str,
    voice: Option<&str>,
    adapter: Option<&str>,
) -> Result<SynthesisResult, TTSError> {
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| TTSError::SynthesisFailed(format!("Failed to create runtime: {}", e)))?;
    rt.block_on(async {
        synthesize_speech_impl(text, voice, adapter).await
    })
}

async fn synthesize_speech_impl(
    text: &str,
    voice: Option<&str>,
    adapter: Option<&str>,
) -> Result<SynthesisResult, TTSError> {
    // Initialize TTS system if needed
    if !tts::is_initialized() {
        tts::init_registry();
        tts::initialize().await?;
    }

    let voice_id = voice.unwrap_or("default");

    // Use specific adapter if requested, otherwise use active adapter
    match adapter {
        Some(name) => tts::synthesize_with(text, voice_id, name).await,
        None => tts::synthesize(text, voice_id).await,
    }
}

/// Check if TTS system is ready
pub fn is_ready() -> bool {
    tts::is_initialized()
}

/// Get available voices
pub fn get_voices() -> Vec<crate::voice::tts::VoiceInfo> {
    tts::available_voices()
}
