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

pub mod rms_threshold;
pub mod silero;
pub mod silero_raw;
pub mod test_audio;
pub mod webrtc;
pub mod metrics;
pub mod wav_loader;
pub mod production;
pub mod adaptive;

// Re-export implementations
pub use rms_threshold::RmsThresholdVAD;
pub use silero::SileroVAD;
pub use silero_raw::SileroRawVAD;
pub use test_audio::{TestAudioGenerator, Vowel};
pub use webrtc::WebRtcVAD;

// Re-export metrics
pub use metrics::{ConfusionMatrix, GroundTruth, Outcome, Prediction, VADEvaluator};

// Re-export production
pub use production::{ProductionVAD, ProductionVADConfig};

// Re-export adaptive
pub use adaptive::{AdaptiveConfig, AdaptiveVAD, NoiseLevel};

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
/// All methods are SYNC - VAD detection is pure computation, no async needed.
pub trait VoiceActivityDetection: Send + Sync {
    /// Algorithm name
    fn name(&self) -> &'static str;

    /// Algorithm description
    fn description(&self) -> &'static str;

    /// Is the VAD initialized and ready?
    fn is_initialized(&self) -> bool;

    /// Initialize the VAD (load models, etc.) - SYNC, model loading is sync anyway
    fn initialize(&self) -> Result<(), VADError>;

    /// Detect voice activity in audio samples (SYNC - no async needed for pure computation)
    ///
    /// # Arguments
    /// * `samples` - Audio samples (i16 PCM, 16kHz mono)
    ///
    /// # Returns
    /// * `VADResult` with is_speech boolean and confidence score
    fn detect(&self, samples: &[i16]) -> Result<VADResult, VADError>;

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
    /// - "webrtc" - WebRTC VAD (fast, rule-based, 1-10μs per frame)
    /// - "silero" - ML-based Silero VAD (accurate, rejects background noise)
    /// - "silero-raw" - Silero with raw ONNX Runtime (no external crate dependencies)
    pub fn create(name: &str) -> Result<Box<dyn VoiceActivityDetection>, VADError> {
        match name {
            "rms" => Ok(Box::new(rms_threshold::RmsThresholdVAD::new())),
            "webrtc" => Ok(Box::new(webrtc::WebRtcVAD::new())),
            "silero" => Ok(Box::new(silero::SileroVAD::new())),
            "silero-raw" => Ok(Box::new(silero_raw::SileroRawVAD::new())),
            _ => Err(VADError::ModelNotLoaded(format!(
                "Unknown VAD: '{name}'. Supported: rms, webrtc, silero, silero-raw"
            ))),
        }
    }

    /// Get best available VAD
    ///
    /// Priority:
    /// 1. Silero Raw (ML-based, most accurate)
    /// 2. Silero (ML-based with external crate)
    /// 3. WebRTC (fast, rule-based, good quality)
    /// 4. RMS (primitive fallback)
    pub fn best_available() -> Box<dyn VoiceActivityDetection> {
        // Try Silero raw ONNX first (best quality, fewest dependencies)
        if let Ok(silero) = Self::create("silero-raw") {
            return silero;
        }

        // Try original Silero with external crate
        if let Ok(silero) = Self::create("silero") {
            return silero;
        }

        // Try WebRTC (fast, better than RMS, always available)
        if let Ok(webrtc) = Self::create("webrtc") {
            return webrtc;
        }

        // Fallback to RMS (primitive but always works)
        Box::new(rms_threshold::RmsThresholdVAD::new())
    }
}
