//! WebSocket Audio Adapter
//!
//! Bridges browser WebSocket audio streams to the pipeline.
//! Receives Int16 PCM at 16kHz, outputs Int16 PCM at 16kHz.

use crate::adapter::{AdapterError, InputAdapter, OutputAdapter};
use crate::frame::{AudioFrame, Frame};
use crate::handle::Handle;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::RwLock;

/// Message types for WebSocket communication
#[derive(Debug, Clone)]
pub enum WsMessage {
    /// Binary audio data (Int16 PCM)
    Audio(Vec<i16>),

    /// JSON message
    Json(WsJsonMessage),

    /// Connection closed
    Close,
}

/// JSON messages from/to client
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum WsJsonMessage {
    /// Transcription result
    #[serde(rename = "transcription")]
    Transcription { text: String, is_final: bool },

    /// AI response text
    #[serde(rename = "ai_response")]
    AiResponse { text: String },

    /// Client interrupt request (barge-in)
    #[serde(rename = "interrupt")]
    Interrupt,

    /// Error message
    #[serde(rename = "error")]
    Error { message: String },

    /// Voice activity detected
    #[serde(rename = "vad")]
    Vad { is_speaking: bool },
}

/// WebSocket Audio Input Adapter
///
/// Receives audio from browser WebSocket, converts to AudioFrames.
pub struct WsAudioInputAdapter {
    handle: Option<Handle>,
    streaming: bool,

    /// Receiver for audio data from WebSocket handler
    audio_rx: mpsc::Receiver<Vec<i16>>,

    /// Sender for JSON messages to WebSocket handler
    json_tx: mpsc::Sender<WsJsonMessage>,

    /// Timestamp counter
    timestamp_us: u64,

    /// Sample rate
    sample_rate: u32,
}

impl WsAudioInputAdapter {
    /// Create new adapter with channels for WebSocket communication
    pub fn new(
        audio_rx: mpsc::Receiver<Vec<i16>>,
        json_tx: mpsc::Sender<WsJsonMessage>,
    ) -> Self {
        Self {
            handle: None,
            streaming: false,
            audio_rx,
            json_tx,
            timestamp_us: 0,
            sample_rate: 16000,
        }
    }

    /// Send JSON message to client
    pub async fn send_json(&self, message: WsJsonMessage) -> Result<(), AdapterError> {
        self.json_tx
            .send(message)
            .await
            .map_err(|_| AdapterError::StreamClosed)
    }
}

#[async_trait]
impl InputAdapter for WsAudioInputAdapter {
    fn name(&self) -> &'static str {
        "ws-audio-input"
    }

    async fn start(&mut self) -> Result<Handle, AdapterError> {
        let handle = Handle::new();
        self.handle = Some(handle);
        self.streaming = true;
        self.timestamp_us = 0;
        Ok(handle)
    }

    async fn read_frame(&mut self) -> Result<Option<Frame>, AdapterError> {
        if !self.streaming {
            return Ok(None);
        }

        // Wait for audio data from WebSocket
        match self.audio_rx.recv().await {
            Some(samples) => {
                let frame = AudioFrame::new(samples, self.timestamp_us, self.sample_rate);

                // Advance timestamp (20ms per frame at 16kHz = 320 samples)
                self.timestamp_us += (frame.duration_ms() * 1000.0) as u64;

                Ok(Some(Frame::Audio(frame)))
            }
            None => {
                // Channel closed
                self.streaming = false;
                Ok(None)
            }
        }
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

/// WebSocket Audio Output Adapter
///
/// Sends audio back to browser via WebSocket.
pub struct WsAudioOutputAdapter {
    handle: Option<Handle>,
    active: bool,

    /// Sender for audio data to WebSocket handler
    audio_tx: mpsc::Sender<Vec<i16>>,

    /// Sender for JSON messages to WebSocket handler
    json_tx: mpsc::Sender<WsJsonMessage>,
}

impl WsAudioOutputAdapter {
    /// Create new adapter with channels for WebSocket communication
    pub fn new(
        audio_tx: mpsc::Sender<Vec<i16>>,
        json_tx: mpsc::Sender<WsJsonMessage>,
    ) -> Self {
        Self {
            handle: None,
            active: false,
            audio_tx,
            json_tx,
        }
    }

    /// Send JSON message to client
    pub async fn send_json(&self, message: WsJsonMessage) -> Result<(), AdapterError> {
        self.json_tx
            .send(message)
            .await
            .map_err(|_| AdapterError::StreamClosed)
    }
}

#[async_trait]
impl OutputAdapter for WsAudioOutputAdapter {
    fn name(&self) -> &'static str {
        "ws-audio-output"
    }

    async fn start(&mut self, handle: Handle) -> Result<(), AdapterError> {
        self.handle = Some(handle);
        self.active = true;
        Ok(())
    }

    async fn write_frame(&mut self, frame: &Frame) -> Result<(), AdapterError> {
        if !self.active {
            return Err(AdapterError::StreamClosed);
        }

        match frame {
            Frame::Audio(audio) => {
                // Send audio to WebSocket
                self.audio_tx
                    .send(audio.samples.clone())
                    .await
                    .map_err(|_| AdapterError::StreamClosed)?;
            }
            Frame::Text(text) => {
                // Send transcription/response as JSON
                if let Some(content) = text.as_text() {
                    let message = if text.is_final {
                        WsJsonMessage::AiResponse {
                            text: content.to_string(),
                        }
                    } else {
                        WsJsonMessage::Transcription {
                            text: content.to_string(),
                            is_final: text.is_final,
                        }
                    };
                    self.send_json(message).await?;
                }
            }
            _ => {
                return Err(AdapterError::InvalidFormat(
                    "Expected audio or text frame".to_string(),
                ));
            }
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

/// Voice session state
pub struct VoiceSession {
    pub handle: Handle,
    pub room_id: String,

    /// Channels for audio/message exchange
    pub audio_to_pipeline: mpsc::Sender<Vec<i16>>,
    pub audio_from_pipeline: mpsc::Receiver<Vec<i16>>,
    pub json_to_client: mpsc::Receiver<WsJsonMessage>,
    pub json_from_client: mpsc::Sender<WsJsonMessage>,

    /// Interrupt flag
    pub interrupted: Arc<RwLock<bool>>,
}

impl VoiceSession {
    /// Create a new voice session with all channels
    pub fn new(room_id: String) -> (Self, WsAudioInputAdapter, WsAudioOutputAdapter) {
        let handle = Handle::new();

        // Channels: Browser -> Pipeline
        let (audio_to_pipeline_tx, audio_to_pipeline_rx) = mpsc::channel(64);

        // Channels: Pipeline -> Browser
        let (audio_from_pipeline_tx, audio_from_pipeline_rx) = mpsc::channel(64);

        // Channels: JSON messages
        let (json_to_client_tx, json_to_client_rx) = mpsc::channel(32);
        let (json_from_client_tx, _json_from_client_rx) = mpsc::channel(32);

        let session = VoiceSession {
            handle,
            room_id,
            audio_to_pipeline: audio_to_pipeline_tx,
            audio_from_pipeline: audio_from_pipeline_rx,
            json_to_client: json_to_client_rx,
            json_from_client: json_from_client_tx.clone(),
            interrupted: Arc::new(RwLock::new(false)),
        };

        let input_adapter = WsAudioInputAdapter::new(audio_to_pipeline_rx, json_to_client_tx.clone());

        let output_adapter = WsAudioOutputAdapter::new(audio_from_pipeline_tx, json_to_client_tx);

        (session, input_adapter, output_adapter)
    }

    /// Handle interrupt request from client
    pub async fn interrupt(&self) {
        let mut interrupted = self.interrupted.write().await;
        *interrupted = true;
    }

    /// Check and clear interrupt flag
    pub async fn check_interrupt(&self) -> bool {
        let mut interrupted = self.interrupted.write().await;
        let was_interrupted = *interrupted;
        *interrupted = false;
        was_interrupted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_voice_session_creation() {
        let (session, _input, _output) = VoiceSession::new("general".to_string());
        assert_eq!(session.room_id, "general");
    }

    #[tokio::test]
    async fn test_ws_input_adapter() {
        let (tx, rx) = mpsc::channel(16);
        let (json_tx, _json_rx) = mpsc::channel(16);

        let mut adapter = WsAudioInputAdapter::new(rx, json_tx);

        // Start adapter
        let _handle = adapter.start().await.unwrap();
        assert!(adapter.is_streaming());

        // Send test audio
        tx.send(vec![0i16; 320]).await.unwrap();

        // Read frame
        let frame = adapter.read_frame().await.unwrap();
        assert!(frame.is_some());

        if let Some(Frame::Audio(audio)) = frame {
            assert_eq!(audio.samples.len(), 320);
        } else {
            panic!("Expected audio frame");
        }

        // Stop adapter
        adapter.stop().await.unwrap();
        assert!(!adapter.is_streaming());
    }

    #[tokio::test]
    async fn test_ws_output_adapter() {
        let (audio_tx, mut audio_rx) = mpsc::channel(16);
        let (json_tx, _json_rx) = mpsc::channel(16);

        let mut adapter = WsAudioOutputAdapter::new(audio_tx, json_tx);

        // Start adapter
        adapter.start(Handle::new()).await.unwrap();
        assert!(adapter.is_active());

        // Write audio frame
        let frame = Frame::Audio(AudioFrame::new(vec![100i16; 320], 0, 16000));
        adapter.write_frame(&frame).await.unwrap();

        // Receive on channel
        let received = audio_rx.recv().await.unwrap();
        assert_eq!(received.len(), 320);
        assert_eq!(received[0], 100);
    }
}
