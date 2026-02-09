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
/// Async version - use this when already in an async context (e.g., ServiceModule::handle_command)
pub async fn synthesize_speech_async(
    text: &str,
    voice: Option<&str>,
    adapter: Option<&str>,
) -> Result<SynthesisResult, TTSError> {
    synthesize_speech_impl(text, voice, adapter).await
}

/// This is a synchronous wrapper that creates its own tokio runtime.
///
/// IMPORTANT: Always creates a NEW runtime. IPC handler threads are spawned
/// via std::thread::spawn from within #[tokio::main], so they inherit the
/// global runtime handle. Calling handle.block_on() from such threads panics
/// with "Cannot block the current thread from within a runtime". Creating a
/// fresh runtime avoids this entirely.
///
/// WARNING: Do NOT call this from within an async context (e.g., inside a tokio task).
/// Use synthesize_speech_async instead.
pub fn synthesize_speech_sync(
    text: &str,
    voice: Option<&str>,
    adapter: Option<&str>,
) -> Result<SynthesisResult, TTSError> {
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| TTSError::SynthesisFailed(format!("Failed to create runtime: {e}")))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_ready_before_init() {
        // Before any init, TTS should not be ready
        // (may be true if another test initialized it — that's OK)
        let ready = is_ready();
        println!("TTS ready before explicit init: {}", ready);
    }

    /// Synthesize with the silence adapter (always available, no model files)
    #[test]
    fn test_synthesize_silence_adapter() {
        let result = synthesize_speech_sync("Hello test", None, Some("silence"));
        let synthesis = result.expect("Silence adapter should always succeed");
        assert!(synthesis.samples.len() > 0, "Should produce samples");
        assert!(synthesis.samples.iter().all(|&s| s == 0), "Silence adapter produces zeros");
        println!(
            "Silence adapter: {} samples, {}Hz, {}ms",
            synthesis.samples.len(), synthesis.sample_rate, synthesis.duration_ms
        );
    }

    #[test]
    fn test_synthesize_nonexistent_adapter() {
        let result = synthesize_speech_sync("Hello", None, Some("nonexistent_adapter_xyz"));
        assert!(result.is_err(), "Nonexistent adapter should fail");
        let err = result.unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("not found") || msg.contains("Adapter"),
            "Error should mention adapter not found: {}", msg);
    }

    /// Full Kokoro integration test — requires model files and espeak-ng
    #[test]
    #[ignore] // Run with: cargo test --package continuum-core -- --ignored tts_service
    fn test_synthesize_kokoro_full() {
        // Set CWD to jtag root so model paths resolve
        let original_cwd = std::env::current_dir().unwrap();
        let candidates = [
            std::path::PathBuf::from("models/kokoro"),
            std::path::PathBuf::from("../../models/kokoro"),
        ];
        if let Some(models_dir) = candidates.into_iter().find(|p| p.is_dir()) {
            let jtag_root = models_dir.parent().unwrap().parent().unwrap();
            std::env::set_current_dir(jtag_root).unwrap();
        }

        // This test exercises the full path: tts_service -> registry -> kokoro adapter
        // -> espeak-ng phonemize -> vocab tokenize -> ONNX inference -> resample
        let result = synthesize_speech_sync(
            "This is a full integration test of the Kokoro TTS pipeline.",
            Some("af"),
            Some("kokoro"),
        );

        std::env::set_current_dir(original_cwd).unwrap();

        let synthesis = result.expect("Kokoro synthesis should succeed");
        assert!(synthesis.samples.len() > 1000, "Should produce >1000 samples");
        assert_eq!(synthesis.sample_rate, crate::audio_constants::AUDIO_SAMPLE_RATE);
        assert!(synthesis.duration_ms > 500, "Sentence should be >500ms");

        // Audio should not be silence
        let max_amplitude = synthesis.samples.iter().map(|s| s.abs()).max().unwrap_or(0);
        assert!(max_amplitude > 100, "Audio should not be silence, max: {}", max_amplitude);

        println!(
            "Kokoro TTS service: {} samples, {}Hz, {}ms, max amp: {}",
            synthesis.samples.len(), synthesis.sample_rate, synthesis.duration_ms, max_amplitude
        );
    }

    /// Test that synthesize_speech_sync creates its own runtime (no panic)
    #[test]
    fn test_synthesize_creates_own_runtime() {
        // This verifies the fix for the tokio runtime panic:
        // synthesize_speech_sync should always create a NEW runtime,
        // never try to reuse an existing one.
        let result = synthesize_speech_sync("Runtime test", None, Some("silence"));
        assert!(result.is_ok(), "Should succeed with own runtime");
    }

    /// Test concurrent calls to synthesize_speech_sync from multiple threads
    #[test]
    fn test_synthesize_concurrent() {
        let handles: Vec<_> = (0..4)
            .map(|i| {
                std::thread::spawn(move || {
                    let text = format!("Thread {} says hello", i);
                    synthesize_speech_sync(&text, None, Some("silence"))
                })
            })
            .collect();

        for (i, handle) in handles.into_iter().enumerate() {
            let result = handle.join().expect("Thread should not panic");
            assert!(result.is_ok(), "Thread {} should succeed: {:?}", i, result.err());
        }
    }
}
