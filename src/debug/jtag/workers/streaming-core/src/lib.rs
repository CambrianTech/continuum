//! Streaming Core
//!
//! Universal streaming backbone for AI communication.
//!
//! # Architecture
//!
//! Everything is streaming - voice, images, video, training.
//! Same infrastructure, different timescales:
//! - Voice: 20ms frames
//! - Images: 2-30 seconds
//! - Video: 30-300 seconds
//! - Training: Hours
//!
//! # Core Primitives
//!
//! - **Handle**: Universal correlation ID (UUIDv4)
//! - **Frame**: Data unit (Audio, Video, Text, Image)
//! - **RingBuffer**: Lock-free queue with backpressure
//! - **Event**: Handle-correlated status updates
//!
//! # Pipeline Model
//!
//! ```text
//! InputAdapter -> [Stage1] -> [Stage2] -> ... -> OutputAdapter
//!      ↓              ↓           ↓                   ↓
//!   EventBus ← ← ← Events (Started, Progress, FrameReady, Completed)
//! ```
//!
//! # Zero-Copy Design
//!
//! - Ring buffers hold data, pass SlotRef (8 bytes)
//! - GPU textures stay on GPU, pass texture ID
//! - Only copy at boundaries (encode/decode)
//!
//! # Example Usage
//!
//! ```rust,ignore
//! use streaming_core::{Pipeline, PipelineBuilder, EventBus};
//! use std::sync::Arc;
//!
//! // Create event bus
//! let event_bus = Arc::new(EventBus::new(1024));
//!
//! // Build voice chat pipeline
//! let mut pipeline = PipelineBuilder::new(event_bus.clone())
//!     .voice_chat();
//!
//! // Subscribe to events
//! let mut events = event_bus.subscribe_handle(pipeline.handle());
//!
//! // Start pipeline
//! let handle = pipeline.start().await?;
//!
//! // Process events
//! while let Ok(event) = events.recv().await {
//!     match event {
//!         StreamEvent::FrameReady { .. } => { /* handle frame */ }
//!         StreamEvent::Completed { .. } => break,
//!         _ => {}
//!     }
//! }
//! ```

pub mod adapter;
pub mod call_server;
pub mod continuous; // Continuous transcription module
pub mod event;
pub mod frame;
pub mod handle;
pub mod mixer;
pub mod pipeline;
pub mod ring;
pub mod stage;
pub mod stt; // Speech-to-text adapter system (Whisper, etc.)
pub mod tts; // Text-to-speech adapter system (Kokoro, etc.)
pub mod vad; // Voice activity detection (Silero, RMS threshold, etc.)
pub mod ws_audio;

// gRPC voice service (requires proto compilation)
// TODO: Update voice_service to use new adapter system
// #[cfg(feature = "grpc")]
// pub mod voice_service;

// Re-export main types at crate root
pub use adapter::{AdapterError, InputAdapter, OutputAdapter};
pub use event::{EventBus, FrameType, StreamEvent};
pub use frame::{AudioFrame, Frame, ImageFrame, TextFrame, VideoFrame};
pub use handle::Handle;
pub use pipeline::{Pipeline, PipelineBuilder, PipelineConfig, PipelineError, PipelineState};
pub use ring::{PeekGuard, RingBuffer, SlotRef};
pub use stage::{Stage, StageError};

// Re-export stubbed adapters
pub use adapter::{
    CpalMicrophoneAdapter, CpalSpeakerAdapter, TwilioMediaAdapter, TwilioOutputAdapter,
    WebRtcInputAdapter, WebRtcOutputAdapter,
};

// Re-export stubbed stages
pub use stage::{
    AvatarStage, ImageGenStage, LlmStage, SttStage, TtsStage, VadStage, VideoGenStage,
};

// Re-export WebSocket audio types
pub use ws_audio::{
    VoiceSession, WsAudioInputAdapter, WsAudioOutputAdapter, WsJsonMessage, WsMessage,
};

// Re-export TTS adapter types
pub use tts::{KokoroTTS, SynthesisResult, TTSError, TTSRegistry, TextToSpeech, VoiceInfo};

// Re-export STT adapter types
pub use stt::{
    STTError, STTRegistry, SpeechToText, TranscriptResult, TranscriptSegment, WhisperSTT,
};

// Re-export VAD adapter types
pub use vad::{
    RmsThresholdVAD, SileroVAD, VADError, VADFactory, VADResult, VoiceActivityDetection,
};

// Re-export mixer types
pub use mixer::{AudioMixer, ParticipantStream};

// Re-export call server types
pub use call_server::{Call, CallManager, CallMessage};

// Re-export continuous transcription types
pub use continuous::SlidingAudioBuffer;
