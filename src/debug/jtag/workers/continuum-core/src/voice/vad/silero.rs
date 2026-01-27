//! Silero VAD
//!
//! ML-based voice activity detection using Silero VAD ONNX model.
//! Accurately distinguishes speech from background noise (music, TV, etc).
//!
//! Model: https://github.com/snakers4/silero-vad
//! License: MIT
//! Size: ~1.8MB (onnx)
//!
//! Features:
//! - Trained on 6000+ hours of speech
//! - Rejects background noise, music, TV audio
//! - 8ms chunk processing (ultra low latency)
//! - Works on 8kHz and 16kHz audio

use super::{VADError, VADResult, VoiceActivityDetection};
use crate::audio_constants::AUDIO_SAMPLE_RATE;
use async_trait::async_trait;
use ndarray::{Array1, Array2};
use once_cell::sync::OnceCell;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

/// Silero VAD model session (loaded once)
static SILERO_SESSION: OnceCell<Arc<Mutex<Session>>> = OnceCell::new();

/// Silero VAD state (h and c tensors for LSTM)
struct SileroState {
    h: Array2<f32>,
    c: Array2<f32>,
}

impl Default for SileroState {
    fn default() -> Self {
        // Initial state is zeros (2 x 1 x 64)
        Self {
            h: Array2::zeros((2, 64)),
            c: Array2::zeros((2, 64)),
        }
    }
}

/// Silero VAD
///
/// ML-based VAD that can reject background noise.
/// Uses ONNX Runtime for inference.
pub struct SileroVAD {
    model_path: Option<PathBuf>,
    /// LSTM state (h, c tensors) - persists across frames
    state: Arc<Mutex<SileroState>>,
    /// Speech threshold (0.0-1.0, default 0.5)
    threshold: f32,
}

impl SileroVAD {
    pub fn new() -> Self {
        Self {
            model_path: None,
            state: Arc::new(Mutex::new(SileroState::default())),
            threshold: 0.5,
        }
    }

    pub fn with_model_path(model_path: PathBuf) -> Self {
        Self {
            model_path: Some(model_path),
            state: Arc::new(Mutex::new(SileroState::default())),
            threshold: 0.5,
        }
    }

    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Find the model file in common locations
    fn find_model_path(&self) -> PathBuf {
        if let Some(ref path) = self.model_path {
            return path.clone();
        }

        // Get model preference from SILERO_VAD_MODEL env var
        let model_name = std::env::var("SILERO_VAD_MODEL")
            .unwrap_or_else(|_| "silero_vad.onnx".to_string());

        // Search for the model in common locations
        let candidates = vec![
            PathBuf::from(format!("models/vad/{}", model_name)),
            dirs::data_dir()
                .unwrap_or_default()
                .join(format!("silero/{}", model_name)),
            PathBuf::from(format!("/usr/local/share/silero/{}", model_name)),
        ];

        for path in &candidates {
            if path.exists() {
                return path.clone();
            }
        }

        // Default - will fail if not found, but error message will be helpful
        PathBuf::from(format!("models/vad/{}", model_name))
    }

    /// Preprocess audio samples for Silero
    ///
    /// Silero expects:
    /// - Float samples normalized to [-1, 1]
    /// - Shape: [batch=1, samples]
    fn preprocess_audio(&self, samples: &[i16]) -> Array2<f32> {
        let float_samples: Vec<f32> = samples
            .iter()
            .map(|&s| s as f32 / 32768.0) // i16 to [-1, 1]
            .collect();

        Array2::from_shape_vec((1, float_samples.len()), float_samples)
            .expect("Failed to create audio array")
    }

    /// Run inference on blocking thread
    fn infer_sync(
        session: &Session,
        audio: Array2<f32>,
        h: Array2<f32>,
        c: Array2<f32>,
        sr: i64,
    ) -> Result<(f32, Array2<f32>, Array2<f32>), VADError> {
        // Prepare inputs
        let inputs = ort::inputs![
            "input" => audio.view(),
            "h" => h.view(),
            "c" => c.view(),
            "sr" => Array1::from_vec(vec![sr]).view()
        ]
        .map_err(|e| VADError::InferenceFailed(format!("Failed to create inputs: {e}")))?;

        // Run inference
        let outputs = session
            .run(inputs)
            .map_err(|e| VADError::InferenceFailed(format!("Inference failed: {e}")))?;

        // Extract outputs
        let output = outputs["output"]
            .try_extract_tensor::<f32>()
            .map_err(|e| VADError::InferenceFailed(format!("Failed to extract output: {e}")))?;
        let hn = outputs["hn"]
            .try_extract_tensor::<f32>()
            .map_err(|e| VADError::InferenceFailed(format!("Failed to extract hn: {e}")))?;
        let cn = outputs["cn"]
            .try_extract_tensor::<f32>()
            .map_err(|e| VADError::InferenceFailed(format!("Failed to extract cn: {e}")))?;

        // Get speech probability (output is [1, 1])
        let speech_prob = output
            .view()
            .into_dimensionality::<ndarray::Ix2>()
            .map_err(|e| VADError::InferenceFailed(format!("Invalid output shape: {e}")))?
            [[0, 0]];

        // Convert h and c to owned arrays for next iteration
        let h_3d = hn
            .view()
            .into_dimensionality::<ndarray::Ix3>()
            .map_err(|e| VADError::InferenceFailed(format!("Invalid h shape: {e}")))?;
        let h_next = h_3d.index_axis(ndarray::Axis(1), 0).to_owned();

        let c_3d = cn
            .view()
            .into_dimensionality::<ndarray::Ix3>()
            .map_err(|e| VADError::InferenceFailed(format!("Invalid c shape: {e}")))?;
        let c_next = c_3d.index_axis(ndarray::Axis(1), 0).to_owned();

        Ok((speech_prob, h_next, c_next))
    }
}

impl Default for SileroVAD {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VoiceActivityDetection for SileroVAD {
    fn name(&self) -> &'static str {
        "silero"
    }

    fn description(&self) -> &'static str {
        "ML-based Silero VAD (accurate background noise rejection)"
    }

    fn is_initialized(&self) -> bool {
        SILERO_SESSION.get().is_some()
    }

    async fn initialize(&self) -> Result<(), VADError> {
        if SILERO_SESSION.get().is_some() {
            info!("Silero VAD already initialized");
            return Ok(());
        }

        let model_path = self.find_model_path();
        info!("Loading Silero VAD model from: {:?}", model_path);

        if !model_path.exists() {
            warn!("Silero VAD model not found at {:?}", model_path);
            warn!("Download from: https://github.com/snakers4/silero-vad/blob/master/files/silero_vad.onnx");
            warn!("Place silero_vad.onnx in models/vad/");

            return Err(VADError::ModelNotLoaded(format!(
                "Model not found: {model_path:?}. Download silero_vad.onnx from GitHub"
            )));
        }

        // Load model with ONNX Runtime
        let session = Session::builder()
            .map_err(|e| VADError::ModelNotLoaded(e.to_string()))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| VADError::ModelNotLoaded(e.to_string()))?
            .with_intra_threads(num_cpus::get().min(4))
            .map_err(|e| VADError::ModelNotLoaded(e.to_string()))?
            .commit_from_file(model_path)
            .map_err(|e| VADError::ModelNotLoaded(e.to_string()))?;

        SILERO_SESSION
            .set(Arc::new(Mutex::new(session)))
            .map_err(|_| VADError::ModelNotLoaded("Failed to set global session".into()))?;

        info!("Silero VAD model loaded successfully");
        Ok(())
    }

    async fn detect(&self, samples: &[i16]) -> Result<VADResult, VADError> {
        if samples.is_empty() {
            return Err(VADError::InvalidAudio("Empty samples".into()));
        }

        let session = SILERO_SESSION
            .get()
            .ok_or_else(|| {
                VADError::ModelNotLoaded(
                    "Silero VAD not initialized. Call initialize() first.".into(),
                )
            })?
            .clone();

        // Preprocess audio
        let audio = self.preprocess_audio(samples);

        // Get current state (clone to avoid holding lock across await)
        let (h, c) = {
            let state_guard = self.state.lock();
            (state_guard.h.clone(), state_guard.c.clone())
        };

        // Sample rate for Silero
        let sr = AUDIO_SAMPLE_RATE as i64;

        // Run inference on blocking thread
        let (speech_prob, h_next, c_next) = tokio::task::spawn_blocking(move || {
            let session_guard = session.lock();
            Self::infer_sync(&session_guard, audio, h, c, sr)
        })
        .await
        .map_err(|e| VADError::InferenceFailed(format!("Task join error: {e}")))?
        .map_err(|e| VADError::InferenceFailed(format!("Inference error: {e}")))?;

        // Update state for next frame
        {
            let mut state_guard = self.state.lock();
            state_guard.h = h_next;
            state_guard.c = c_next;
        }

        // Determine if speech
        let is_speech = speech_prob >= self.threshold;

        Ok(VADResult {
            is_speech,
            confidence: speech_prob,
        })
    }

    fn silence_threshold_frames(&self) -> u32 {
        // Silero is accurate - can use fewer frames
        10 // 320ms at 32ms/frame
    }

    fn should_transcribe(&self, result: &VADResult) -> bool {
        // Silero is accurate - trust it
        // Only transcribe if high confidence speech
        result.is_speech && result.confidence > self.threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_silero_creation() {
        let vad = SileroVAD::new();
        assert_eq!(vad.name(), "silero");
        assert!(!vad.is_initialized());
    }

    // Note: Full inference tests require model file download
    // Run manually: cargo test --release -- --ignored test_silero_inference
    #[tokio::test]
    #[ignore]
    async fn test_silero_inference() {
        let vad = SileroVAD::new();
        vad.initialize().await.expect("Failed to initialize");

        let silence = vec![0i16; 512]; // 32ms at 16kHz
        let result = vad.detect(&silence).await.unwrap();

        // Silence should have low probability
        assert!(result.confidence < 0.3);
    }
}
