//! Voice Activity Detection (VAD) Module
//!
//! Modular VAD system supporting multiple algorithms:
//! - RMS Threshold (fast, primitive)
//! - Silero VAD (ML-based, accurate)
//!
//! Follows polymorphism pattern (like OpenCV cv::Algorithm):
//! - Runtime swappable implementations
//! - Trait-based abstraction
//! - Factory creation by name

use async_trait::async_trait;

pub mod rms_threshold;
pub mod silero;
pub mod silero_raw;

// Re-export implementations
pub use rms_threshold::RmsThresholdVAD;
pub use silero::SileroVAD;
pub use silero_raw::SileroRawVAD;

/// VAD Error
#[derive(Debug, thiserror::Error)]
pub enum VADError {
    #[error("Model not loaded: {0}")]
    ModelNotLoaded(String),

    #[error("Invalid audio: {0}")]
    InvalidAudio(String),

    #[error("Inference failed: {0}")]
    InferenceFailed(String),
}

/// Voice Activity Detection result
#[derive(Debug, Clone, Copy)]
pub struct VADResult {
    /// Is speech detected? (true = speech, false = silence/noise)
    pub is_speech: bool,

    /// Confidence score (0.0 = definitely not speech, 1.0 = definitely speech)
    pub confidence: f32,
}

/// Voice Activity Detection trait
///
/// Implementations must be Send + Sync for multi-threaded use.
/// Follows polymorphism pattern for runtime swappable algorithms.
#[async_trait]
pub trait VoiceActivityDetection: Send + Sync {
    /// Algorithm name
    fn name(&self) -> &'static str;

    /// Algorithm description
    fn description(&self) -> &'static str;

    /// Is the VAD initialized and ready?
    fn is_initialized(&self) -> bool;

    /// Initialize the VAD (load models, etc.)
    async fn initialize(&self) -> Result<(), VADError>;

    /// Detect voice activity in audio samples
    ///
    /// # Arguments
    /// * `samples` - Audio samples (i16 PCM, 16kHz mono)
    ///
    /// # Returns
    /// * `VADResult` with is_speech boolean and confidence score
    async fn detect(&self, samples: &[i16]) -> Result<VADResult, VADError>;

    /// Get recommended silence threshold in frames
    ///
    /// How many consecutive non-speech frames before declaring silence.
    /// Default: 22 frames (704ms at 32ms/frame)
    fn silence_threshold_frames(&self) -> u32 {
        22
    }

    /// Should this frame trigger transcription?
    ///
    /// Some VADs may want to skip certain frames even if speech-like
    /// (e.g., very short bursts, background music patterns)
    fn should_transcribe(&self, result: &VADResult) -> bool {
        result.is_speech && result.confidence > 0.5
    }
}

/// VAD Factory - create VAD by name
pub struct VADFactory;

impl VADFactory {
    /// Create a VAD instance by name
    ///
    /// Supported:
    /// - "rms" - Fast RMS threshold (primitive but low latency)
    /// - "silero" - ML-based Silero VAD (accurate, rejects background noise)
    /// - "silero-raw" - Silero with raw ONNX Runtime (no external crate dependencies)
    pub fn create(name: &str) -> Result<Box<dyn VoiceActivityDetection>, VADError> {
        match name {
            "rms" => Ok(Box::new(rms_threshold::RmsThresholdVAD::new())),
            "silero" => Ok(Box::new(silero::SileroVAD::new())),
            "silero-raw" => Ok(Box::new(silero_raw::SileroRawVAD::new())),
            _ => Err(VADError::ModelNotLoaded(format!(
                "Unknown VAD: '{}'. Supported: rms, silero, silero-raw",
                name
            ))),
        }
    }

    /// Get default VAD (Silero if available, RMS fallback)
    pub fn default() -> Box<dyn VoiceActivityDetection> {
        // Try Silero raw ONNX first (best quality, fewest dependencies)
        if let Ok(silero) = Self::create("silero-raw") {
            return silero;
        }

        // Try original Silero with external crate
        if let Ok(silero) = Self::create("silero") {
            return silero;
        }

        // Fallback to RMS (always works, no dependencies)
        Box::new(rms_threshold::RmsThresholdVAD::new())
    }
}
