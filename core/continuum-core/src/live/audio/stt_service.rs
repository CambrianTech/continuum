//! STT Service - Handles speech-to-text transcription requests
//!
//! This is the proper layer between IPC and the STT adapters.
//! IPC should NOT directly call STT - it should call this service.

use crate::live::audio::stt::{self, STTError, TranscriptResult};
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

#[cfg(test)]
mod tests {
    use super::*;

    /// THE voice TDD loop, runnable WITHOUT the core (Joel 2026-09-02: stop
    /// rebooting the 35B to test a 62M ONNX model). Synthesize a phrase with
    /// Kokoro, transcribe it back with the active STT (Moonshine), assert the
    /// words survive the round trip. Iterates in `cargo test` in seconds — the
    /// running Ornith lane, benchmarks, and citizens are never touched.
    ///
    /// Run: `cargo test -p continuum-core --features metal,accelerate \
    ///       -- --ignored voice_round_trip` (needs the model files on disk +
    /// CONTINUUM_MODELS_DIR, same as the Kokoro full-path test).
    #[test]
    #[ignore]
    fn voice_round_trip_tts_to_stt() {
        // Models resolve via CONTINUUM_MODELS_DIR (portable, no CWD juggling,
        // no OS-specific paths — set by the runner/start script on every OS).
        let phrase = "The quick brown fox jumps over the lazy dog.";
        let synth = crate::live::audio::tts_service::synthesize_speech_sync(
            phrase,
            Some("af"),
            Some("kokoro"),
            None,
        )
        .expect("Kokoro synth"); // safe: test asserts the pipeline; a synth error IS the failure
        assert!(synth.samples.len() > 8000, "expected real audio");

        let transcript = transcribe_speech_sync(&synth.samples, Some("en"))
            .expect("STT must run end to end (no swallowed error)"); // safe: same — STT failure is the test's point

        let lower = transcript.text.to_lowercase();
        // Content words that must survive TTS→STT — 3 of 5 = the chain works.
        let hits = ["quick", "brown", "fox", "lazy", "dog"]
            .iter()
            .filter(|w| lower.contains(**w))
            .count();
        assert!(
            hits >= 3,
            "round trip lost the phrase: got {:?} (hits={hits})",
            transcript.text
        );
    }
}
