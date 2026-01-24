//! RMS Threshold VAD
//!
//! Fast, primitive voice activity detection using RMS energy.
//! Cannot distinguish speech from background noise (music, TV, etc).
//!
//! Use cases:
//! - Low-latency applications where accuracy can be sacrificed
//! - Fallback when ML models unavailable
//! - Simple volume gating

use super::{VADError, VADResult, VoiceActivityDetection};
use async_trait::async_trait;

/// RMS Threshold VAD
///
/// Detects "sound vs silence" using root-mean-square energy.
/// Does NOT distinguish speech from background noise.
pub struct RmsThresholdVAD {
    /// RMS threshold - anything above this is considered "speech"
    /// 500.0 is current default (very permissive - triggers on TV audio)
    threshold: f32,
}

impl RmsThresholdVAD {
    pub fn new() -> Self {
        Self { threshold: 500.0 }
    }

    pub fn with_threshold(threshold: f32) -> Self {
        Self { threshold }
    }

    /// Calculate RMS (root mean square) of audio samples
    fn calculate_rms(samples: &[i16]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum_squares: f64 = samples.iter().map(|&s| (s as f64).powi(2)).sum();
        (sum_squares / samples.len() as f64).sqrt() as f32
    }
}

impl Default for RmsThresholdVAD {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VoiceActivityDetection for RmsThresholdVAD {
    fn name(&self) -> &'static str {
        "rms_threshold"
    }

    fn description(&self) -> &'static str {
        "Fast RMS energy threshold (cannot reject background noise)"
    }

    fn is_initialized(&self) -> bool {
        true // No initialization needed
    }

    async fn initialize(&self) -> Result<(), VADError> {
        Ok(()) // Nothing to initialize
    }

    async fn detect(&self, samples: &[i16]) -> Result<VADResult, VADError> {
        if samples.is_empty() {
            return Err(VADError::InvalidAudio("Empty samples".into()));
        }

        let rms = Self::calculate_rms(samples);
        let is_speech = rms >= self.threshold;

        // Confidence is rough approximation based on how far above threshold
        // Scale: threshold = 0.5, 2x threshold = 1.0
        let confidence = if is_speech {
            ((rms / self.threshold) - 1.0).min(1.0)
        } else {
            0.0
        };

        Ok(VADResult {
            is_speech,
            confidence,
        })
    }

    fn silence_threshold_frames(&self) -> u32 {
        // RMS is noisy - need more frames to be confident
        22 // 704ms
    }

    fn should_transcribe(&self, result: &VADResult) -> bool {
        // RMS cannot distinguish speech from noise
        // Accept anything above threshold
        result.is_speech
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rms_silence() {
        let vad = RmsThresholdVAD::new();
        let silence = vec![0i16; 320]; // One frame of silence

        let result = vad.detect(&silence).await.unwrap();
        assert!(!result.is_speech);
    }

    #[tokio::test]
    async fn test_rms_loud_audio() {
        let vad = RmsThresholdVAD::new();
        let loud = vec![5000i16; 320]; // Loud audio

        let result = vad.detect(&loud).await.unwrap();
        assert!(result.is_speech); // RMS thinks loud = speech (wrong!)
    }
}
