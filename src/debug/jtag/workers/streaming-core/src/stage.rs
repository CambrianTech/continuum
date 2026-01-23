//! Processing Stages
//!
//! Stages transform frames in the pipeline.
//! Each stage pulls from input, processes, pushes to output.
//! Zero-copy where possible via SlotRef passing.

use crate::frame::Frame;
use async_trait::async_trait;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StageError {
    #[error("Processing failed: {0}")]
    ProcessingFailed(String),

    #[error("Model not loaded")]
    ModelNotLoaded,

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),
}

/// Processing stage trait
#[async_trait]
pub trait Stage: Send + Sync {
    /// Stage name for logging/discovery
    fn name(&self) -> &'static str;

    /// Process a single frame (may produce 0, 1, or N output frames)
    async fn process(&mut self, input: Frame) -> Result<Vec<Frame>, StageError>;

    /// Flush any buffered output (called at stream end)
    async fn flush(&mut self) -> Result<Vec<Frame>, StageError> {
        Ok(vec![])
    }

    /// Reset stage state (between streams)
    async fn reset(&mut self) -> Result<(), StageError> {
        Ok(())
    }
}

// ============================================================================
// STUBBED STAGES - Implement these with real models
// ============================================================================

/// Stub: Voice Activity Detection
pub struct VadStage {
    /// Minimum speech duration in ms to trigger
    #[allow(dead_code)]
    min_speech_ms: u32,
    /// Buffer for accumulating audio
    buffer: Vec<i16>,
}

impl VadStage {
    pub fn new(min_speech_ms: u32) -> Self {
        Self {
            min_speech_ms,
            buffer: Vec::new(),
        }
    }
}

#[async_trait]
impl Stage for VadStage {
    fn name(&self) -> &'static str {
        "vad"
    }

    async fn process(&mut self, input: Frame) -> Result<Vec<Frame>, StageError> {
        match input {
            Frame::Audio(audio) => {
                // TODO: Run Silero VAD or similar
                // For now, pass through all audio
                Ok(vec![Frame::Audio(audio)])
            }
            _ => Err(StageError::InvalidInput("Expected audio frame".to_string())),
        }
    }

    async fn flush(&mut self) -> Result<Vec<Frame>, StageError> {
        self.buffer.clear();
        Ok(vec![])
    }

    async fn reset(&mut self) -> Result<(), StageError> {
        self.buffer.clear();
        Ok(())
    }
}

/// Stub: Speech-to-Text (Whisper)
pub struct SttStage {
    /// Model path
    model_path: Option<String>,
    /// Accumulated audio for batching
    audio_buffer: Vec<i16>,
    /// Sample rate
    sample_rate: u32,
}

impl SttStage {
    pub fn new() -> Self {
        Self {
            model_path: None,
            audio_buffer: Vec::new(),
            sample_rate: 16000,
        }
    }

    pub fn with_model(mut self, path: String) -> Self {
        self.model_path = Some(path);
        self
    }
}

impl Default for SttStage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Stage for SttStage {
    fn name(&self) -> &'static str {
        "stt-whisper"
    }

    async fn process(&mut self, input: Frame) -> Result<Vec<Frame>, StageError> {
        match input {
            Frame::Audio(audio) => {
                // Accumulate audio
                self.audio_buffer.extend(&audio.samples);

                // TODO: Run Whisper inference when enough audio accumulated
                // For now, return partial text every second of audio
                let samples_per_second = self.sample_rate as usize;
                if self.audio_buffer.len() >= samples_per_second {
                    self.audio_buffer.clear();
                    Ok(vec![Frame::Text(crate::frame::TextFrame::text(
                        "[transcribed text]".to_string(),
                        audio.timestamp_us,
                        false,
                    ))])
                } else {
                    Ok(vec![])
                }
            }
            _ => Err(StageError::InvalidInput("Expected audio frame".to_string())),
        }
    }

    async fn flush(&mut self) -> Result<Vec<Frame>, StageError> {
        // Process remaining audio
        if !self.audio_buffer.is_empty() {
            self.audio_buffer.clear();
            Ok(vec![Frame::Text(crate::frame::TextFrame::text(
                "[final transcription]".to_string(),
                0,
                true,
            ))])
        } else {
            Ok(vec![])
        }
    }

    async fn reset(&mut self) -> Result<(), StageError> {
        self.audio_buffer.clear();
        Ok(())
    }
}

/// Stub: Text-to-Speech (XTTS/MeloTTS)
pub struct TtsStage {
    /// Model path
    model_path: Option<String>,
    /// Voice/speaker ID
    speaker_id: Option<String>,
    /// Output sample rate
    sample_rate: u32,
}

impl TtsStage {
    pub fn new() -> Self {
        Self {
            model_path: None,
            speaker_id: None,
            sample_rate: 24000,
        }
    }

    pub fn with_model(mut self, path: String) -> Self {
        self.model_path = Some(path);
        self
    }

    pub fn with_speaker(mut self, speaker_id: String) -> Self {
        self.speaker_id = Some(speaker_id);
        self
    }
}

impl Default for TtsStage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Stage for TtsStage {
    fn name(&self) -> &'static str {
        "tts"
    }

    async fn process(&mut self, input: Frame) -> Result<Vec<Frame>, StageError> {
        match input {
            Frame::Text(text) => {
                // TODO: Run TTS inference, stream audio chunks
                // For now, return empty audio frame
                let text_content = text.as_text().unwrap_or("[tokens]");
                let _ = text_content; // Use in real impl

                Ok(vec![Frame::Audio(crate::frame::AudioFrame::new(
                    vec![0i16; (self.sample_rate / 50) as usize], // 20ms of silence
                    text.timestamp_us,
                    self.sample_rate,
                ))])
            }
            _ => Err(StageError::InvalidInput("Expected text frame".to_string())),
        }
    }
}

/// Stub: LLM inference stage
pub struct LlmStage {
    /// Model identifier
    model_id: String,
    /// Max tokens to generate
    max_tokens: u32,
}

impl LlmStage {
    pub fn new(model_id: String) -> Self {
        Self {
            model_id,
            max_tokens: 256,
        }
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }
}

#[async_trait]
impl Stage for LlmStage {
    fn name(&self) -> &'static str {
        "llm"
    }

    async fn process(&mut self, input: Frame) -> Result<Vec<Frame>, StageError> {
        match input {
            Frame::Text(text) => {
                // TODO: Call inference-grpc worker or Ollama
                // Stream tokens as they're generated
                let _ = &self.model_id;
                let _ = self.max_tokens;

                Ok(vec![Frame::Text(crate::frame::TextFrame::text(
                    "[LLM response]".to_string(),
                    text.timestamp_us,
                    true,
                ))])
            }
            _ => Err(StageError::InvalidInput("Expected text frame".to_string())),
        }
    }
}

/// Stub: Image generation (Stable Diffusion / Flux)
#[allow(dead_code)]
pub struct ImageGenStage {
    /// Model path
    model_path: Option<String>,
    /// Output dimensions
    width: u32,
    height: u32,
    /// Number of inference steps
    steps: u32,
}

impl ImageGenStage {
    pub fn new() -> Self {
        Self {
            model_path: None,
            width: 512,
            height: 512,
            steps: 20,
        }
    }

    pub fn with_model(mut self, path: String) -> Self {
        self.model_path = Some(path);
        self
    }

    pub fn with_size(mut self, width: u32, height: u32) -> Self {
        self.width = width;
        self.height = height;
        self
    }
}

impl Default for ImageGenStage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Stage for ImageGenStage {
    fn name(&self) -> &'static str {
        "image-gen"
    }

    async fn process(&mut self, input: Frame) -> Result<Vec<Frame>, StageError> {
        match input {
            Frame::Text(text) => {
                // TODO: Run Stable Diffusion / Flux inference
                // Progress events emitted during denoising steps
                let prompt = text.as_text().unwrap_or("");
                let _ = prompt;

                // Return placeholder image
                let pixels = self.width * self.height * 4; // RGBA
                Ok(vec![Frame::Image(crate::frame::ImageFrame::from_bytes(
                    vec![0u8; pixels as usize],
                    self.width,
                    self.height,
                ))])
            }
            _ => Err(StageError::InvalidInput("Expected text prompt".to_string())),
        }
    }
}

/// Stub: Video generation (Mochi / CogVideoX)
#[allow(dead_code)]
pub struct VideoGenStage {
    /// Model path
    model_path: Option<String>,
    /// Output dimensions
    width: u32,
    height: u32,
    /// Frame rate
    fps: u32,
    /// Duration in seconds
    duration_sec: f32,
}

impl VideoGenStage {
    pub fn new() -> Self {
        Self {
            model_path: None,
            width: 480,
            height: 480,
            fps: 24,
            duration_sec: 5.0,
        }
    }

    pub fn with_model(mut self, path: String) -> Self {
        self.model_path = Some(path);
        self
    }
}

impl Default for VideoGenStage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Stage for VideoGenStage {
    fn name(&self) -> &'static str {
        "video-gen"
    }

    async fn process(&mut self, input: Frame) -> Result<Vec<Frame>, StageError> {
        match input {
            Frame::Text(text) => {
                // TODO: Run video generation model
                // Stream video frames as they're generated
                let prompt = text.as_text().unwrap_or("");
                let _ = prompt;

                // Return single video frame placeholder
                Ok(vec![Frame::Video(crate::frame::VideoFrame::new(
                    0, // GPU texture ID
                    self.width as u16,
                    self.height as u16,
                    text.timestamp_us,
                ))])
            }
            _ => Err(StageError::InvalidInput("Expected text prompt".to_string())),
        }
    }
}

/// Stub: Avatar animation (LivePortrait / SadTalker)
pub struct AvatarStage {
    /// Model path
    model_path: Option<String>,
    /// Reference image/video for avatar
    reference_texture: Option<u64>,
}

impl AvatarStage {
    pub fn new() -> Self {
        Self {
            model_path: None,
            reference_texture: None,
        }
    }

    pub fn with_model(mut self, path: String) -> Self {
        self.model_path = Some(path);
        self
    }

    pub fn with_reference(mut self, texture_id: u64) -> Self {
        self.reference_texture = Some(texture_id);
        self
    }
}

impl Default for AvatarStage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Stage for AvatarStage {
    fn name(&self) -> &'static str {
        "avatar"
    }

    async fn process(&mut self, input: Frame) -> Result<Vec<Frame>, StageError> {
        // Avatar can take audio (lip sync) or video (face swap) as input
        match input {
            Frame::Audio(audio) => {
                // Generate lip-synced video from audio
                Ok(vec![Frame::Video(crate::frame::VideoFrame::new(
                    self.reference_texture.unwrap_or(0),
                    512,
                    512,
                    audio.timestamp_us,
                ))])
            }
            Frame::Video(video) => {
                // Face swap / expression transfer
                Ok(vec![Frame::Video(video)])
            }
            _ => Err(StageError::InvalidInput(
                "Expected audio or video frame".to_string(),
            )),
        }
    }
}
