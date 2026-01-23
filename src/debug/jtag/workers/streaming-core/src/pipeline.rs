//! Pipeline Orchestration
//!
//! Pipelines connect adapters and stages into processing graphs.
//! Pull-based: downstream stages pull from upstream when ready.
//! Zero-copy where possible via ring buffers.

use crate::adapter::{AdapterError, InputAdapter, OutputAdapter};
use crate::event::{EventBus, FrameType, StreamEvent};
use crate::frame::Frame;
use crate::handle::Handle;
use crate::stage::{Stage, StageError};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

#[derive(Error, Debug)]
pub enum PipelineError {
    #[error("Adapter error: {0}")]
    Adapter(#[from] AdapterError),

    #[error("Stage error: {0}")]
    Stage(#[from] StageError),

    #[error("Pipeline not started")]
    NotStarted,

    #[error("Pipeline already running")]
    AlreadyRunning,

    #[error("Channel closed")]
    ChannelClosed,
}

/// Pipeline state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineState {
    Idle,
    Running,
    Paused,
    Completed,
    Failed,
}

/// Pipeline configuration
pub struct PipelineConfig {
    /// Ring buffer capacity for inter-stage queues
    pub ring_capacity: usize,
    /// Maximum frames to buffer before backpressure
    pub max_buffered_frames: usize,
    /// Enable detailed tracing
    pub trace_enabled: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            ring_capacity: 64,
            max_buffered_frames: 32,
            trace_enabled: false,
        }
    }
}

/// A processing pipeline
///
/// Connects: Input Adapter -> [Stages...] -> Output Adapter
/// Uses ring buffers between stages for zero-copy frame passing.
pub struct Pipeline {
    /// Pipeline handle for correlation
    handle: Handle,

    /// Current state
    state: PipelineState,

    /// Input adapter
    input: Option<Box<dyn InputAdapter>>,

    /// Processing stages (in order)
    stages: Vec<Box<dyn Stage>>,

    /// Output adapter
    output: Option<Box<dyn OutputAdapter>>,

    /// Event bus for publishing events
    event_bus: Arc<EventBus>,

    /// Configuration
    config: PipelineConfig,

    /// Cancel signal
    cancel_tx: Option<mpsc::Sender<()>>,
}

impl Pipeline {
    /// Create a new pipeline
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            handle: Handle::new(),
            state: PipelineState::Idle,
            input: None,
            stages: Vec::new(),
            output: None,
            event_bus,
            config: PipelineConfig::default(),
            cancel_tx: None,
        }
    }

    /// Create with custom configuration
    pub fn with_config(event_bus: Arc<EventBus>, config: PipelineConfig) -> Self {
        Self {
            config,
            ..Self::new(event_bus)
        }
    }

    /// Get pipeline handle
    pub fn handle(&self) -> Handle {
        self.handle
    }

    /// Get current state
    pub fn state(&self) -> PipelineState {
        self.state
    }

    /// Set input adapter
    pub fn input(mut self, adapter: Box<dyn InputAdapter>) -> Self {
        self.input = Some(adapter);
        self
    }

    /// Add a processing stage
    pub fn stage(mut self, stage: Box<dyn Stage>) -> Self {
        self.stages.push(stage);
        self
    }

    /// Set output adapter
    pub fn output(mut self, adapter: Box<dyn OutputAdapter>) -> Self {
        self.output = Some(adapter);
        self
    }

    /// Start the pipeline
    pub async fn start(&mut self) -> Result<Handle, PipelineError> {
        if self.state == PipelineState::Running {
            return Err(PipelineError::AlreadyRunning);
        }

        // Validate pipeline
        if self.input.is_none() {
            return Err(PipelineError::Adapter(AdapterError::NotSupported(
                "No input adapter".to_string(),
            )));
        }

        // Start input adapter
        let input_handle = self.input.as_mut().unwrap().start().await?;
        info!(
            "Pipeline {} started input adapter, got handle {}",
            self.handle.short(),
            input_handle.short()
        );

        // Start output adapter if present
        if let Some(output) = &mut self.output {
            output.start(self.handle).await?;
            info!("Pipeline {} started output adapter", self.handle.short());
        }

        // Create cancel channel
        let (cancel_tx, cancel_rx) = mpsc::channel(1);
        self.cancel_tx = Some(cancel_tx);

        self.state = PipelineState::Running;

        // Emit started event
        self.event_bus.publish(StreamEvent::Started {
            handle: self.handle,
        });

        // Run the pipeline loop
        self.run_loop(cancel_rx).await?;

        Ok(self.handle)
    }

    /// Run the main processing loop
    async fn run_loop(&mut self, mut cancel_rx: mpsc::Receiver<()>) -> Result<(), PipelineError> {
        let mut frame_count: u64 = 0;

        loop {
            // Check for cancellation
            if cancel_rx.try_recv().is_ok() {
                info!("Pipeline {} cancelled", self.handle.short());
                self.state = PipelineState::Completed;
                self.event_bus.publish(StreamEvent::Cancelled {
                    handle: self.handle,
                });
                break;
            }

            // Read from input
            let frame = match self.input.as_mut().unwrap().read_frame().await {
                Ok(Some(frame)) => frame,
                Ok(None) => {
                    // Stream ended
                    info!("Pipeline {} input ended", self.handle.short());
                    break;
                }
                Err(e) => {
                    error!("Pipeline {} input error: {}", self.handle.short(), e);
                    self.state = PipelineState::Failed;
                    self.event_bus.publish(StreamEvent::Failed {
                        handle: self.handle,
                        error: e.to_string(),
                    });
                    return Err(PipelineError::Adapter(e));
                }
            };

            frame_count += 1;
            if self.config.trace_enabled {
                debug!(
                    "Pipeline {} processing frame {} ({})",
                    self.handle.short(),
                    frame_count,
                    frame.kind()
                );
            }

            // Process through stages
            let mut frames = vec![frame];
            for stage in &mut self.stages {
                let mut output_frames = Vec::new();
                for f in frames {
                    match stage.process(f).await {
                        Ok(outputs) => output_frames.extend(outputs),
                        Err(e) => {
                            warn!(
                                "Pipeline {} stage {} error: {}",
                                self.handle.short(),
                                stage.name(),
                                e
                            );
                            // Continue processing other frames
                        }
                    }
                }
                frames = output_frames;
            }

            // Write to output
            if let Some(output) = &mut self.output {
                for frame in frames {
                    // Emit frame ready event
                    self.event_bus.publish(StreamEvent::FrameReady {
                        handle: self.handle,
                        frame_type: match &frame {
                            Frame::Audio(_) => FrameType::Audio,
                            Frame::Video(_) => FrameType::Video,
                            Frame::Text(_) => FrameType::Text,
                            Frame::Image(_) => FrameType::Image,
                        },
                        slot: 0, // TODO: Use actual ring buffer slot
                    });

                    if let Err(e) = output.write_frame(&frame).await {
                        warn!("Pipeline {} output error: {}", self.handle.short(), e);
                    }
                }
            }

            // Emit progress (every 100 frames)
            if frame_count % 100 == 0 {
                self.event_bus.publish(StreamEvent::Progress {
                    handle: self.handle,
                    progress: 0.0, // Unknown total for streams
                    message: Some(format!("Processed {frame_count} frames")),
                });
            }
        }

        // Flush stages
        for stage in &mut self.stages {
            if let Ok(flushed) = stage.flush().await {
                if let Some(output) = &mut self.output {
                    for frame in flushed {
                        let _ = output.write_frame(&frame).await;
                    }
                }
            }
        }

        // Stop adapters
        if let Some(input) = &mut self.input {
            let _ = input.stop().await;
        }
        if let Some(output) = &mut self.output {
            let _ = output.stop().await;
        }

        self.state = PipelineState::Completed;
        self.event_bus.publish(StreamEvent::Completed {
            handle: self.handle,
        });

        info!(
            "Pipeline {} completed, processed {} frames",
            self.handle.short(),
            frame_count
        );

        Ok(())
    }

    /// Cancel the pipeline
    pub async fn cancel(&mut self) -> Result<(), PipelineError> {
        if let Some(tx) = &self.cancel_tx {
            let _ = tx.send(()).await;
        }
        Ok(())
    }

    /// Reset pipeline for reuse
    pub async fn reset(&mut self) -> Result<(), PipelineError> {
        self.state = PipelineState::Idle;
        self.handle = Handle::new();
        self.cancel_tx = None;

        for stage in &mut self.stages {
            stage.reset().await?;
        }

        Ok(())
    }
}

/// Pipeline builder for common configurations
pub struct PipelineBuilder {
    event_bus: Arc<EventBus>,
    config: PipelineConfig,
}

impl PipelineBuilder {
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            event_bus,
            config: PipelineConfig::default(),
        }
    }

    pub fn with_config(mut self, config: PipelineConfig) -> Self {
        self.config = config;
        self
    }

    /// Build a voice chat pipeline: Mic -> VAD -> STT -> LLM -> TTS -> Speaker
    pub fn voice_chat(self) -> Pipeline {
        use crate::adapter::{CpalMicrophoneAdapter, CpalSpeakerAdapter};
        use crate::stage::{LlmStage, SttStage, TtsStage, VadStage};

        Pipeline::with_config(self.event_bus, self.config)
            .input(Box::new(CpalMicrophoneAdapter::new()))
            .stage(Box::new(VadStage::new(300)))
            .stage(Box::new(SttStage::new()))
            .stage(Box::new(LlmStage::new("llama3.2:3b".to_string())))
            .stage(Box::new(TtsStage::new()))
            .output(Box::new(CpalSpeakerAdapter::new()))
    }

    /// Build an IVR pipeline: Twilio -> VAD -> STT -> LLM -> TTS -> Twilio
    pub fn ivr(self, stream_sid: String) -> Pipeline {
        use crate::adapter::{TwilioMediaAdapter, TwilioOutputAdapter};
        use crate::stage::{LlmStage, SttStage, TtsStage, VadStage};

        Pipeline::with_config(self.event_bus, self.config)
            .input(Box::new(TwilioMediaAdapter::new(stream_sid.clone())))
            .stage(Box::new(VadStage::new(300)))
            .stage(Box::new(SttStage::new()))
            .stage(Box::new(LlmStage::new("llama3.2:3b".to_string())))
            .stage(Box::new(TtsStage::new()))
            .output(Box::new(TwilioOutputAdapter::new(stream_sid)))
    }

    /// Build image generation pipeline: Text -> ImageGen -> Output
    pub fn image_gen(self) -> Pipeline {
        use crate::stage::ImageGenStage;

        Pipeline::with_config(self.event_bus, self.config).stage(Box::new(ImageGenStage::new()))
    }

    /// Build video generation pipeline: Text -> VideoGen -> Output
    pub fn video_gen(self) -> Pipeline {
        use crate::stage::VideoGenStage;

        Pipeline::with_config(self.event_bus, self.config).stage(Box::new(VideoGenStage::new()))
    }

    /// Build avatar pipeline: Audio -> Avatar -> Video Output
    pub fn avatar(self) -> Pipeline {
        use crate::stage::AvatarStage;

        Pipeline::with_config(self.event_bus, self.config).stage(Box::new(AvatarStage::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pipeline_creation() {
        let event_bus = Arc::new(EventBus::new(64));
        let pipeline = Pipeline::new(event_bus);

        assert_eq!(pipeline.state(), PipelineState::Idle);
    }

    #[tokio::test]
    async fn test_pipeline_builder() {
        let event_bus = Arc::new(EventBus::new(64));
        let builder = PipelineBuilder::new(event_bus);

        let pipeline = builder.voice_chat();
        assert_eq!(pipeline.state(), PipelineState::Idle);
        assert_eq!(pipeline.stages.len(), 4); // VAD, STT, LLM, TTS
    }
}
