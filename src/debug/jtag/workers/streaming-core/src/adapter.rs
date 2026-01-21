//! Adapter Traits
//!
//! Adapters bridge external I/O to the internal ring buffer system.
//! Zero-copy: adapters write directly to ring slots.

use crate::frame::{AudioFrame, Frame};
use crate::handle::Handle;
use async_trait::async_trait;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AdapterError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Stream closed")]
    StreamClosed,

    #[error("Buffer overflow")]
    BufferOverflow,

    #[error("Invalid format: {0}")]
    InvalidFormat(String),

    #[error("Hardware error: {0}")]
    HardwareError(String),

    #[error("Not supported: {0}")]
    NotSupported(String),
}

/// Input adapter - sources frames into the pipeline
#[async_trait]
pub trait InputAdapter: Send + Sync {
    /// Adapter name for logging/discovery
    fn name(&self) -> &'static str;

    /// Start streaming (returns handle for correlation)
    async fn start(&mut self) -> Result<Handle, AdapterError>;

    /// Read next frame (blocks until available or stream ends)
    async fn read_frame(&mut self) -> Result<Option<Frame>, AdapterError>;

    /// Stop streaming
    async fn stop(&mut self) -> Result<(), AdapterError>;

    /// Check if adapter is currently streaming
    fn is_streaming(&self) -> bool;
}

/// Output adapter - sinks frames from the pipeline
#[async_trait]
pub trait OutputAdapter: Send + Sync {
    /// Adapter name for logging/discovery
    fn name(&self) -> &'static str;

    /// Start output (returns handle for correlation)
    async fn start(&mut self, handle: Handle) -> Result<(), AdapterError>;

    /// Write frame (zero-copy via SlotRef when possible)
    async fn write_frame(&mut self, frame: &Frame) -> Result<(), AdapterError>;

    /// Stop output
    async fn stop(&mut self) -> Result<(), AdapterError>;

    /// Check if adapter is currently active
    fn is_active(&self) -> bool;
}

// ============================================================================
// STUBBED ADAPTERS - Implement these with real backends
// ============================================================================

/// Stub: Local microphone input via cpal
pub struct CpalMicrophoneAdapter {
    handle: Option<Handle>,
    streaming: bool,
}

impl CpalMicrophoneAdapter {
    pub fn new() -> Self {
        Self {
            handle: None,
            streaming: false,
        }
    }
}

#[async_trait]
impl InputAdapter for CpalMicrophoneAdapter {
    fn name(&self) -> &'static str {
        "cpal-microphone"
    }

    async fn start(&mut self) -> Result<Handle, AdapterError> {
        // TODO: Initialize cpal stream
        let handle = Handle::new();
        self.handle = Some(handle);
        self.streaming = true;
        Ok(handle)
    }

    async fn read_frame(&mut self) -> Result<Option<Frame>, AdapterError> {
        if !self.streaming {
            return Ok(None);
        }
        // TODO: Read from cpal ring buffer
        // For now, return empty frame after delay
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        Ok(Some(Frame::Audio(AudioFrame::empty())))
    }

    async fn stop(&mut self) -> Result<(), AdapterError> {
        self.streaming = false;
        self.handle = None;
        Ok(())
    }

    fn is_streaming(&self) -> bool {
        self.streaming
    }
}

/// Stub: Twilio Media Streams input (WebSocket)
pub struct TwilioMediaAdapter {
    handle: Option<Handle>,
    streaming: bool,
}

impl TwilioMediaAdapter {
    pub fn new(_stream_sid: String) -> Self {
        Self {
            handle: None,
            streaming: false,
        }
    }
}

#[async_trait]
impl InputAdapter for TwilioMediaAdapter {
    fn name(&self) -> &'static str {
        "twilio-media-streams"
    }

    async fn start(&mut self) -> Result<Handle, AdapterError> {
        // TODO: Connect to Twilio WebSocket
        let handle = Handle::new();
        self.handle = Some(handle);
        self.streaming = true;
        Ok(handle)
    }

    async fn read_frame(&mut self) -> Result<Option<Frame>, AdapterError> {
        if !self.streaming {
            return Ok(None);
        }
        // TODO: Read from Twilio WebSocket, decode mulaw
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        Ok(Some(Frame::Audio(AudioFrame::empty())))
    }

    async fn stop(&mut self) -> Result<(), AdapterError> {
        self.streaming = false;
        self.handle = None;
        Ok(())
    }

    fn is_streaming(&self) -> bool {
        self.streaming
    }
}

/// Stub: WebRTC input adapter
pub struct WebRtcInputAdapter {
    handle: Option<Handle>,
    streaming: bool,
}

impl WebRtcInputAdapter {
    pub fn new() -> Self {
        Self {
            handle: None,
            streaming: false,
        }
    }
}

#[async_trait]
impl InputAdapter for WebRtcInputAdapter {
    fn name(&self) -> &'static str {
        "webrtc"
    }

    async fn start(&mut self) -> Result<Handle, AdapterError> {
        // TODO: Initialize WebRTC peer connection
        let handle = Handle::new();
        self.handle = Some(handle);
        self.streaming = true;
        Ok(handle)
    }

    async fn read_frame(&mut self) -> Result<Option<Frame>, AdapterError> {
        if !self.streaming {
            return Ok(None);
        }
        // TODO: Read from WebRTC track
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        Ok(Some(Frame::Audio(AudioFrame::empty())))
    }

    async fn stop(&mut self) -> Result<(), AdapterError> {
        self.streaming = false;
        self.handle = None;
        Ok(())
    }

    fn is_streaming(&self) -> bool {
        self.streaming
    }
}

/// Stub: Local speaker output via cpal
pub struct CpalSpeakerAdapter {
    handle: Option<Handle>,
    active: bool,
}

impl CpalSpeakerAdapter {
    pub fn new() -> Self {
        Self {
            handle: None,
            active: false,
        }
    }
}

#[async_trait]
impl OutputAdapter for CpalSpeakerAdapter {
    fn name(&self) -> &'static str {
        "cpal-speaker"
    }

    async fn start(&mut self, handle: Handle) -> Result<(), AdapterError> {
        // TODO: Initialize cpal output stream
        self.handle = Some(handle);
        self.active = true;
        Ok(())
    }

    async fn write_frame(&mut self, frame: &Frame) -> Result<(), AdapterError> {
        if !self.active {
            return Err(AdapterError::StreamClosed);
        }
        // TODO: Write to cpal output buffer
        match frame {
            Frame::Audio(_audio) => {
                // Write PCM samples to speaker
            }
            _ => return Err(AdapterError::InvalidFormat("Expected audio frame".to_string())),
        }
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), AdapterError> {
        self.active = false;
        self.handle = None;
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active
    }
}

/// Stub: Twilio Media Streams output (WebSocket)
pub struct TwilioOutputAdapter {
    handle: Option<Handle>,
    active: bool,
}

impl TwilioOutputAdapter {
    pub fn new(_stream_sid: String) -> Self {
        Self {
            handle: None,
            active: false,
        }
    }
}

#[async_trait]
impl OutputAdapter for TwilioOutputAdapter {
    fn name(&self) -> &'static str {
        "twilio-output"
    }

    async fn start(&mut self, handle: Handle) -> Result<(), AdapterError> {
        // TODO: Initialize Twilio output channel
        self.handle = Some(handle);
        self.active = true;
        Ok(())
    }

    async fn write_frame(&mut self, frame: &Frame) -> Result<(), AdapterError> {
        if !self.active {
            return Err(AdapterError::StreamClosed);
        }
        // TODO: Encode to mulaw, send via WebSocket
        match frame {
            Frame::Audio(_audio) => {
                // Encode and send
            }
            _ => return Err(AdapterError::InvalidFormat("Expected audio frame".to_string())),
        }
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), AdapterError> {
        self.active = false;
        self.handle = None;
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active
    }
}

/// Stub: WebRTC output adapter
pub struct WebRtcOutputAdapter {
    handle: Option<Handle>,
    active: bool,
}

impl WebRtcOutputAdapter {
    pub fn new() -> Self {
        Self {
            handle: None,
            active: false,
        }
    }
}

#[async_trait]
impl OutputAdapter for WebRtcOutputAdapter {
    fn name(&self) -> &'static str {
        "webrtc-output"
    }

    async fn start(&mut self, handle: Handle) -> Result<(), AdapterError> {
        // TODO: Add track to WebRTC peer connection
        self.handle = Some(handle);
        self.active = true;
        Ok(())
    }

    async fn write_frame(&mut self, _frame: &Frame) -> Result<(), AdapterError> {
        if !self.active {
            return Err(AdapterError::StreamClosed);
        }
        // TODO: Send frame via WebRTC track
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), AdapterError> {
        self.active = false;
        self.handle = None;
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active
    }
}
