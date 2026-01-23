//! Stub STT Adapter
//!
//! Returns pre-configured test transcriptions for development/testing.
//! No actual speech recognition - just returns dummy text based on audio length.

use super::{STTError, SpeechToText, TranscriptResult, TranscriptSegment};
use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};

/// Minimum audio duration for transcription (ms)
/// Stub adapter requires at least 500ms of audio
const STUB_MIN_AUDIO_MS: i64 = 500;

/// Confidence score returned by stub (0.0-1.0)
const STUB_CONFIDENCE: f32 = 0.95;

/// Default language code
const STUB_DEFAULT_LANGUAGE: &str = "en";

/// Stub STT Adapter
///
/// Returns dummy transcriptions for testing without requiring actual Whisper model.
/// Useful for:
/// - Testing the STT adapter pattern
/// - Development without model files
/// - Performance testing the pipeline
pub struct StubSTT {
    initialized: AtomicBool,
}

impl StubSTT {
    pub fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
        }
    }

    /// Generate dummy transcription based on audio length
    fn generate_dummy_text(&self, duration_ms: i64) -> String {
        match duration_ms {
            0..=999 => "Test.".to_string(),
            1000..=1999 => "Test audio transcription.".to_string(),
            2000..=2999 => "This is a test audio transcription from the stub adapter.".to_string(),
            _ => format!(
                "This is a test transcription for audio duration of {duration_ms} milliseconds."
            ),
        }
    }
}

#[async_trait]
impl SpeechToText for StubSTT {
    fn name(&self) -> &'static str {
        "stub"
    }

    fn description(&self) -> &'static str {
        "Stub STT adapter for testing (returns dummy text)"
    }

    fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Relaxed)
    }

    async fn initialize(&self) -> Result<(), STTError> {
        tracing::info!("StubSTT: Initializing (no-op)");
        self.initialized.store(true, Ordering::Relaxed);
        Ok(())
    }

    async fn transcribe(
        &self,
        samples: Vec<f32>,
        language: Option<&str>,
    ) -> Result<TranscriptResult, STTError> {
        if !self.is_initialized() {
            return Err(STTError::ModelNotLoaded(
                "Stub STT not initialized".to_string(),
            ));
        }

        // Calculate audio duration (samples are at 16kHz)
        let duration_ms = (samples.len() as i64 * 1000) / 16000;

        if duration_ms < STUB_MIN_AUDIO_MS {
            return Err(STTError::InvalidAudio(format!(
                "Audio too short: {duration_ms}ms < {STUB_MIN_AUDIO_MS}ms minimum"
            )));
        }

        let text = self.generate_dummy_text(duration_ms);
        let lang = language.unwrap_or(STUB_DEFAULT_LANGUAGE).to_string();

        tracing::debug!(
            "StubSTT: Generated dummy transcription for {}ms audio: \"{}\"",
            duration_ms,
            text
        );

        Ok(TranscriptResult {
            text: text.clone(),
            language: lang,
            confidence: STUB_CONFIDENCE,
            segments: vec![TranscriptSegment {
                text,
                start_ms: 0,
                end_ms: duration_ms,
            }],
        })
    }

    fn supported_languages(&self) -> Vec<&'static str> {
        vec!["en", "es", "fr", "de", "ja", "zh"]
    }

    fn get_param(&self, name: &str) -> Option<String> {
        match name {
            "min_audio_ms" => Some(STUB_MIN_AUDIO_MS.to_string()),
            "confidence" => Some(STUB_CONFIDENCE.to_string()),
            _ => None,
        }
    }

    fn set_param(&self, _name: &str, _value: &str) -> Result<(), STTError> {
        // Stub adapter doesn't support runtime configuration
        Ok(())
    }
}

impl Default for StubSTT {
    fn default() -> Self {
        Self::new()
    }
}
