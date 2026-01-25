//! Silero VAD - Raw ONNX Runtime Implementation
//!
//! Direct ONNX Runtime implementation without external crates.
//! Uses the same ort crate we already have for TTS.

use super::{VADError, VADResult, VoiceActivityDetection};
use async_trait::async_trait;
use ndarray::{Array1, Array2};
use once_cell::sync::OnceCell;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;

/// Silero VAD session (loaded once)
static SILERO_SESSION: OnceCell<Arc<Mutex<Session>>> = OnceCell::new();

/// LSTM state for Silero VAD (HuggingFace model uses combined state)
#[derive(Clone)]
struct VadState {
    // Combined state tensor (2x1x128) = h (2x1x64) + c (2x1x64) concatenated
    state: ndarray::Array3<f32>,
}

impl Default for VadState {
    fn default() -> Self {
        Self {
            // Initial state is zeros (2 x 1 x 128) - h and c concatenated
            state: ndarray::Array3::zeros((2, 1, 128)),
        }
    }
}

/// Silero VAD using raw ONNX Runtime
pub struct SileroRawVAD {
    model_path: Option<PathBuf>,
    state: Arc<Mutex<VadState>>,
    threshold: f32,
}

impl SileroRawVAD {
    pub fn new() -> Self {
        Self {
            model_path: None,
            state: Arc::new(Mutex::new(VadState::default())),
            threshold: 0.5,
        }
    }

    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold.clamp(0.0, 1.0);
        self
    }

    fn find_model_path(&self) -> PathBuf {
        if let Some(ref path) = self.model_path {
            return path.clone();
        }

        let candidates = vec![
            PathBuf::from("models/vad/silero_vad.onnx"),
            PathBuf::from("workers/streaming-core/models/vad/silero_vad.onnx"),
        ];

        for path in &candidates {
            if path.exists() {
                return path.clone();
            }
        }

        PathBuf::from("models/vad/silero_vad.onnx")
    }

    /// Run inference (blocking)
    fn infer_sync(
        session: &Session,
        audio: Array2<f32>,
        state: VadState,
        sr: i64,
    ) -> Result<(f32, VadState), VADError> {
        // Create inputs (HuggingFace model uses combined "state" input)
        let inputs = ort::inputs![
            "input" => audio.view(),
            "state" => state.state.view(),
            "sr" => Array1::from_vec(vec![sr]).view()
        ]
        .map_err(|e| VADError::InferenceFailed(format!("Failed to create inputs: {e}")))?;

        // Run inference
        let outputs = session
            .run(inputs)
            .map_err(|e| VADError::InferenceFailed(format!("Inference failed: {e}")))?;

        // Extract speech probability
        let output = outputs["output"]
            .try_extract_tensor::<f32>()
            .map_err(|e| VADError::InferenceFailed(format!("Failed to extract output: {e}")))?;

        let speech_prob = output.view().into_dimensionality::<ndarray::Ix2>()
            .map_err(|e| VADError::InferenceFailed(format!("Invalid output shape: {e}")))?
            [[0, 0]];

        // Extract new state (HuggingFace model outputs "stateN")
        let state_n = outputs["stateN"]
            .try_extract_tensor::<f32>()
            .map_err(|e| VADError::InferenceFailed(format!("Failed to extract stateN: {e}")))?;

        // Convert to 3D array (2x1x128)
        let state_next = state_n.view().into_dimensionality::<ndarray::Ix3>()
            .map_err(|e| VADError::InferenceFailed(format!("Invalid stateN shape: {e}")))?
            .to_owned();

        let new_state = VadState {
            state: state_next,
        };

        Ok((speech_prob, new_state))
    }
}

impl Default for SileroRawVAD {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VoiceActivityDetection for SileroRawVAD {
    fn name(&self) -> &'static str {
        "silero-raw"
    }

    fn description(&self) -> &'static str {
        "Silero VAD (raw ONNX Runtime, no external crates)"
    }

    fn is_initialized(&self) -> bool {
        SILERO_SESSION.get().is_some()
    }

    async fn initialize(&self) -> Result<(), VADError> {
        if SILERO_SESSION.get().is_some() {
            return Ok(());
        }

        let model_path = self.find_model_path();

        if !model_path.exists() {
            return Err(VADError::ModelNotLoaded(format!(
                "Silero model not found at {model_path:?}"
            )));
        }

        // Load ONNX model
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

        Ok(())
    }

    async fn detect(&self, samples: &[i16]) -> Result<VADResult, VADError> {
        if samples.is_empty() {
            return Err(VADError::InvalidAudio("Empty samples".into()));
        }

        let session = SILERO_SESSION
            .get()
            .ok_or_else(|| VADError::ModelNotLoaded("Not initialized".into()))?
            .clone();

        // Convert to f32
        let float_samples: Vec<f32> = samples.iter().map(|&s| s as f32 / 32768.0).collect();
        let audio = Array2::from_shape_vec((1, float_samples.len()), float_samples)
            .map_err(|e| VADError::InvalidAudio(format!("Failed to create audio array: {e}")))?;

        // Get current state
        let state = self.state.lock().clone();

        // Run inference on blocking thread
        let threshold = self.threshold;
        let result = tokio::task::spawn_blocking(move || {
            let session_guard = session.lock();
            Self::infer_sync(&session_guard, audio, state, 16000)
        })
        .await
        .map_err(|e| VADError::InferenceFailed(format!("Task join error: {e}")))?;

        let (speech_prob, new_state) = result?;

        // Update state
        *self.state.lock() = new_state;

        Ok(VADResult {
            is_speech: speech_prob >= threshold,
            confidence: speech_prob,
        })
    }

    fn silence_threshold_frames(&self) -> u32 {
        10
    }

    fn should_transcribe(&self, result: &VADResult) -> bool {
        result.is_speech && result.confidence > self.threshold
    }
}
