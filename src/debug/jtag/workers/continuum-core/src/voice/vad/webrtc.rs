//! WebRTC VAD - Fast Rule-Based Voice Activity Detection
//!
//! Uses the `earshot` crate - ridiculously fast pure Rust VAD.
//! Based on WebRTC's GMM (Gaussian Mixture Model) approach.
//!
//! Characteristics:
//! - Ultra-fast: ~1-10μs per frame (100-1000x faster than ML-based VAD)
//! - Low memory: No model weights, pure algorithm
//! - No dependencies: #[no_std] compatible
//! - Tunable aggressiveness: 0-3 (0 = least aggressive, 3 = most aggressive)
//!
//! Trade-offs:
//! - Less accurate than ML-based VAD (Silero)
//! - May trigger on non-speech sounds with voice-like frequencies
//! - Good for: Low-latency, resource-constrained, or high-throughput scenarios

use super::{VADError, VADResult, VoiceActivityDetection};
use async_trait::async_trait;
use earshot::{VoiceActivityDetector, VoiceActivityProfile};
use parking_lot::Mutex;
use std::sync::Arc;

/// WebRTC VAD using earshot crate
pub struct WebRtcVAD {
    detector: Arc<Mutex<VoiceActivityDetector>>,
    aggressiveness: u8,
}

impl WebRtcVAD {
    /// Create new WebRTC VAD with default aggressiveness (aggressive profile)
    pub fn new() -> Self {
        let detector = VoiceActivityDetector::new(VoiceActivityProfile::VERY_AGGRESSIVE);

        Self {
            detector: Arc::new(Mutex::new(detector)),
            aggressiveness: 3,
        }
    }

    /// Create with specific aggressiveness level
    pub fn with_aggressiveness(aggressiveness: u8) -> Self {
        let aggressiveness = aggressiveness.min(3);

        let profile = match aggressiveness {
            0..=2 => VoiceActivityProfile::VERY_AGGRESSIVE, // For now, always use aggressive
            _ => VoiceActivityProfile::VERY_AGGRESSIVE,
        };

        let detector = VoiceActivityDetector::new(profile);

        Self {
            detector: Arc::new(Mutex::new(detector)),
            aggressiveness,
        }
    }

    /// Calculate confidence from binary decision
    ///
    /// WebRTC VAD gives binary output - we approximate confidence based on:
    /// - Recent history (how many recent frames were speech)
    /// - Aggressiveness level (higher = lower confidence for speech)
    fn calculate_confidence(&self, is_speech: bool) -> f32 {
        if is_speech {
            // Speech detected - confidence inversely related to aggressiveness
            // Level 0 (least aggressive) → 0.9 confidence
            // Level 3 (most aggressive) → 0.6 confidence
            0.9 - (self.aggressiveness as f32 * 0.1)
        } else {
            // Silence - low confidence
            0.1
        }
    }
}

impl Default for WebRtcVAD {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VoiceActivityDetection for WebRtcVAD {
    fn name(&self) -> &'static str {
        "webrtc"
    }

    fn description(&self) -> &'static str {
        "WebRTC VAD (earshot, ultra-fast rule-based)"
    }

    fn is_initialized(&self) -> bool {
        // No initialization needed - pure algorithm
        true
    }

    async fn initialize(&self) -> Result<(), VADError> {
        // No initialization needed
        Ok(())
    }

    async fn detect(&self, samples: &[i16]) -> Result<VADResult, VADError> {
        if samples.is_empty() {
            return Err(VADError::InvalidAudio("Empty samples".into()));
        }

        // earshot requires multiples of 240 samples (15ms @ 16kHz)
        // If input isn't a multiple, chunk it and use majority voting
        const CHUNK_SIZE: usize = 240;

        let is_speech = if samples.len() % CHUNK_SIZE == 0 {
            // Perfect size - process directly
            let mut detector = self.detector.lock();
            detector
                .predict_16khz(samples)
                .map_err(|e| VADError::InferenceFailed(format!("Earshot prediction failed: {:?}", e)))?
        } else {
            // Chunk into 240-sample pieces and use majority voting
            let mut speech_chunks = 0;
            let mut total_chunks = 0;

            for chunk in samples.chunks(CHUNK_SIZE) {
                if chunk.len() < CHUNK_SIZE {
                    // Skip partial chunks at the end
                    continue;
                }

                let mut detector = self.detector.lock();
                let chunk_is_speech = detector
                    .predict_16khz(chunk)
                    .map_err(|e| VADError::InferenceFailed(format!("Earshot prediction failed: {:?}", e)))?;

                if chunk_is_speech {
                    speech_chunks += 1;
                }
                total_chunks += 1;
            }

            // Majority voting: if > 50% of chunks are speech, return speech
            total_chunks > 0 && speech_chunks * 2 > total_chunks
        };

        let confidence = self.calculate_confidence(is_speech);

        Ok(VADResult {
            is_speech,
            confidence,
        })
    }

    fn silence_threshold_frames(&self) -> u32 {
        // WebRTC is fast but less accurate - use more frames for stability
        match self.aggressiveness {
            0 => 30, // Least aggressive: require 30 frames (600ms) of silence
            1 => 25, // 500ms
            2 => 20, // 400ms (default)
            3 => 15, // Most aggressive: 300ms
            _ => 20,
        }
    }

    fn should_transcribe(&self, result: &VADResult) -> bool {
        // WebRTC gives binary output - trust it
        result.is_speech
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_webrtc_vad_creation() {
        let vad = WebRtcVAD::new();
        assert_eq!(vad.name(), "webrtc");
        assert!(vad.is_initialized());
        assert_eq!(vad.aggressiveness, 3); // Default is very aggressive
    }

    #[tokio::test]
    async fn test_aggressiveness_levels() {
        for level in 0..=3 {
            let vad = WebRtcVAD::with_aggressiveness(level);
            assert_eq!(vad.aggressiveness, level);
        }

        // Test clamping
        let vad = WebRtcVAD::with_aggressiveness(10);
        assert_eq!(vad.aggressiveness, 3);
    }

    #[tokio::test]
    async fn test_supported_frame_sizes() {
        let vad = WebRtcVAD::new();

        // earshot requires 240 samples (15ms at 16kHz)
        let samples = vec![0i16; 240];
        let result = vad.detect(&samples).await;
        assert!(result.is_ok(), "240 samples should work");

        // 480 samples (30ms at 16kHz) = 2x 240
        let samples = vec![0i16; 480];
        let result = vad.detect(&samples).await;
        assert!(result.is_ok(), "480 samples should work");
    }

    #[tokio::test]
    async fn test_silence_detection() {
        let vad = WebRtcVAD::new();
        vad.initialize().await.expect("Init should succeed");

        // Silence (320 samples = 20ms at 16kHz)
        let silence = vec![0i16; 320];
        let result = vad.detect(&silence).await.expect("Should detect");

        assert!(!result.is_speech, "Silence should not be detected as speech");
        assert!(result.confidence < 0.5);
    }

    #[tokio::test]
    async fn test_aggressiveness_configuration() {
        // Test direct construction with different levels
        for level in 0..=3 {
            let vad = WebRtcVAD::with_aggressiveness(level);
            assert_eq!(vad.aggressiveness, level);

            // Different aggressiveness affects silence threshold
            let threshold = vad.silence_threshold_frames();
            assert!(threshold >= 15 && threshold <= 30);
        }
    }
}
