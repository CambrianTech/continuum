//! Silence TTS Adapter
//!
//! Generates silent audio (zeros) for testing.
//! Useful for validating TTS pipeline without actual synthesis.

use super::{SynthesisResult, TTSError, TextToSpeech, VoiceInfo};
use crate::audio_constants::AUDIO_SAMPLE_RATE;
use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};

/// Duration per character (milliseconds)
/// Simulates realistic speech timing: 150ms per character ≈ 400 WPM
const SILENCE_MS_PER_CHAR: u64 = 150;

/// Minimum audio duration (milliseconds)
const SILENCE_MIN_DURATION_MS: u64 = 100;

/// Maximum audio duration (milliseconds)
/// Prevents extremely long silent audio from text abuse
const SILENCE_MAX_DURATION_MS: u64 = 30000; // 30 seconds

/// Silence TTS Adapter
///
/// Generates silent audio (all zeros) with duration proportional to text length.
/// Useful for:
/// - Testing TTS adapter pattern
/// - Performance testing audio pipeline
/// - Development without model files
/// - Placeholder when TTS model unavailable
pub struct SilenceTTS {
    initialized: AtomicBool,
}

impl SilenceTTS {
    pub fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
        }
    }

    /// Calculate audio duration based on text length
    fn calculate_duration(&self, text: &str) -> u64 {
        let char_count = text.chars().count() as u64;
        let duration = char_count * SILENCE_MS_PER_CHAR;

        // Clamp to min/max bounds
        duration.clamp(SILENCE_MIN_DURATION_MS, SILENCE_MAX_DURATION_MS)
    }

    /// Generate silent audio samples
    fn generate_silence(&self, duration_ms: u64) -> Vec<i16> {
        let num_samples = (AUDIO_SAMPLE_RATE as u64 * duration_ms) / 1000;
        vec![0i16; num_samples as usize]
    }
}

#[async_trait]
impl TextToSpeech for SilenceTTS {
    fn name(&self) -> &'static str {
        "silence"
    }

    fn description(&self) -> &'static str {
        "Silence TTS adapter for testing (generates silent audio)"
    }

    fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Relaxed)
    }

    async fn initialize(&self) -> Result<(), TTSError> {
        tracing::info!("SilenceTTS: Initializing (no-op)");
        self.initialized.store(true, Ordering::Relaxed);
        Ok(())
    }

    async fn synthesize(&self, text: &str, _voice: &str) -> Result<SynthesisResult, TTSError> {
        if !self.is_initialized() {
            return Err(TTSError::ModelNotLoaded(
                "Silence TTS not initialized".to_string(),
            ));
        }

        if text.is_empty() {
            return Err(TTSError::InvalidText("Empty text".to_string()));
        }

        let duration_ms = self.calculate_duration(text);
        let samples = self.generate_silence(duration_ms);

        tracing::debug!(
            "SilenceTTS: Generated {}ms of silence for {} characters",
            duration_ms,
            text.len()
        );

        Ok(SynthesisResult {
            samples,
            sample_rate: AUDIO_SAMPLE_RATE,
            duration_ms,
        })
    }

    fn available_voices(&self) -> Vec<VoiceInfo> {
        vec![VoiceInfo {
            id: "default".to_string(),
            name: "Silent Voice".to_string(),
            language: "en".to_string(),
            gender: None,
            description: Some("Generates silent audio".to_string()),
        }]
    }

    fn default_voice(&self) -> &str {
        "default"
    }

    fn get_param(&self, name: &str) -> Option<String> {
        match name {
            "sample_rate" => Some(AUDIO_SAMPLE_RATE.to_string()),
            "ms_per_char" => Some(SILENCE_MS_PER_CHAR.to_string()),
            "min_duration_ms" => Some(SILENCE_MIN_DURATION_MS.to_string()),
            "max_duration_ms" => Some(SILENCE_MAX_DURATION_MS.to_string()),
            _ => None,
        }
    }

    fn set_param(&self, _name: &str, _value: &str) -> Result<(), TTSError> {
        // Silence adapter doesn't support runtime configuration
        Ok(())
    }
}

impl Default for SilenceTTS {
    fn default() -> Self {
        Self::new()
    }
}
