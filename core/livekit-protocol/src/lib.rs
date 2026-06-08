//! IPC protocol between continuum-core and livekit-bridge.
//!
//! Wire format: length-prefixed frames over Unix socket.
//!
//!   [4 bytes: total frame length (u32 LE)]
//!   [JSON header bytes]
//!   [0x00 separator (only if binary payload follows)]
//!   [binary payload bytes]
//!
//! Audio is 16kHz mono i16 PCM — the universal format across VAD, STT, TTS,
//! and LiveKit's AudioFrame. No conversion needed at any boundary.
//!
//! This crate has ZERO heavy dependencies (no ort, no livekit, no webrtc-sys).
//! Both sides depend on it for shared types only.

use serde::{Deserialize, Serialize};

// =============================================================================
// Audio constants (shared between core and bridge)
// =============================================================================

/// Audio sample rate for all voice processing (VAD, STT, TTS, LiveKit).
pub const SAMPLE_RATE: u32 = 16_000;

/// Mono channel count.
pub const CHANNELS: u32 = 1;

/// Samples per 10ms frame (LiveKit's native frame size).
pub const SAMPLES_PER_10MS: u32 = SAMPLE_RATE / 100;

// =============================================================================
// Core → Bridge commands
// =============================================================================

/// Commands sent from continuum-core to livekit-bridge.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BridgeCommand {
    /// Connect a persona agent to a LiveKit room.
    JoinRoom {
        call_id: String,
        user_id: String,
        display_name: String,
    },

    /// Disconnect a persona agent from a LiveKit room.
    LeaveRoom {
        call_id: String,
        user_id: String,
    },

    /// Remove all agents for a call (call ended).
    LeaveAllAgents {
        call_id: String,
    },

    /// Start STT listener for a call (subscribes to human audio tracks).
    StartListener {
        call_id: String,
    },

    /// Stop STT listener for a call.
    StopListener {
        call_id: String,
    },

    /// Publish TTS audio through a persona's LiveKit track.
    /// Binary payload: i16 PCM samples (little-endian).
    Speak {
        call_id: String,
        user_id: String,
        /// Number of i16 samples in the binary payload.
        sample_count: u32,
    },

    /// Inject raw audio through a persona's track (non-TTS).
    /// Binary payload: i16 PCM samples (little-endian).
    InjectAudio {
        call_id: String,
        user_id: String,
        sample_count: u32,
    },

    /// Publish a video frame for a persona's avatar.
    /// Binary payload: RGBA pixel data.
    PublishVideoFrame {
        call_id: String,
        user_id: String,
        width: u32,
        height: u32,
    },

    /// Publish transcription subtitle via LiveKit's native API.
    PublishTranscription {
        call_id: String,
        user_id: String,
        text: String,
        track_sid: String,
        r#final: bool,
    },

    /// Add ambient audio source to a call.
    AddAmbient {
        call_id: String,
        source_name: String,
    },

    /// Inject ambient audio samples.
    /// Binary payload: i16 PCM samples (little-endian).
    InjectAmbient {
        call_id: String,
        handle: String,
        sample_count: u32,
    },

    /// Remove ambient audio source.
    RemoveAmbient {
        call_id: String,
        handle: String,
    },

    /// Set cognitive animation state (thinking/speaking/idle).
    SetCognitiveState {
        call_id: String,
        user_id: String,
        state: String,
    },

    /// Request room/participant snapshot for diagnostics.
    SnapshotRoom,
    SnapshotParticipant {
        identity: String,
    },
}

// =============================================================================
// Bridge → Core events
// =============================================================================

/// Events sent from livekit-bridge to continuum-core.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BridgeEvent {
    /// Raw audio from a human participant (for VAD → STT processing).
    /// Binary payload: i16 PCM samples (little-endian).
    AudioFrame {
        call_id: String,
        speaker_id: String,
        speaker_name: String,
        track_sid: String,
        sample_count: u32,
    },

    /// Video frame from a human participant (for vision processing).
    /// Binary payload: RGBA pixel data.
    VideoFrame {
        call_id: String,
        speaker_id: String,
        speaker_name: String,
        width: u32,
        height: u32,
    },

    /// Human participant connected to the LiveKit room.
    ParticipantJoined {
        call_id: String,
        identity: String,
        name: String,
    },

    /// Human participant disconnected from the LiveKit room.
    ParticipantLeft {
        call_id: String,
        identity: String,
    },

    /// Agent successfully connected to room.
    AgentConnected {
        call_id: String,
        user_id: String,
        audio_track_sid: String,
    },

    /// Agent disconnected (error or intentional).
    AgentDisconnected {
        call_id: String,
        user_id: String,
        reason: String,
    },

    /// STT listener ready for a call.
    ListenerReady {
        call_id: String,
    },

    /// LiveKit room closed/disconnected.
    RoomDisconnected {
        call_id: String,
        reason: String,
    },

    /// Ambient audio source created.
    AmbientCreated {
        call_id: String,
        handle: String,
        source_name: String,
    },

    /// Tile resolution changed by browser participant.
    TileResolution {
        call_id: String,
        user_id: String,
        width: u32,
        height: u32,
    },

    /// Room snapshot response.
    RoomSnapshot {
        json: String,
    },

    /// Participant snapshot response.
    ParticipantSnapshot {
        json: String,
    },
}

// =============================================================================
// Bridge responses (for request/response commands)
// =============================================================================

/// Response to a BridgeCommand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeResponse {
    /// Echoed from the command's request_id.
    pub request_id: u64,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Command-specific response data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// =============================================================================
// Frame codec — shared between both sides
// =============================================================================

/// Encode a JSON message with optional binary payload into a length-prefixed frame.
pub fn encode_frame(json: &[u8], binary: Option<&[u8]>) -> Vec<u8> {
    let total = if let Some(bin) = binary {
        json.len() + 1 + bin.len() // json + separator + binary
    } else {
        json.len()
    };

    let mut frame = Vec::with_capacity(4 + total);
    frame.extend_from_slice(&(total as u32).to_le_bytes());
    frame.extend_from_slice(json);
    if let Some(bin) = binary {
        frame.push(0x00); // separator
        frame.extend_from_slice(bin);
    }
    frame
}

/// Decode a frame into JSON header bytes and optional binary payload.
/// Input should NOT include the 4-byte length prefix (already stripped by reader).
pub fn decode_frame(frame: &[u8]) -> (&[u8], Option<&[u8]>) {
    if let Some(sep) = frame.iter().position(|&b| b == 0x00) {
        (&frame[..sep], Some(&frame[sep + 1..]))
    } else {
        (frame, None)
    }
}

// =============================================================================
// Participant metadata (shared between bridge and browser)
// =============================================================================

/// LiveKit participant role — determines audio routing and UI visibility.
/// Serialized as JSON in the LiveKit JWT metadata field.
/// Must match src/shared/LiveKitTypes.ts enum values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantRole {
    Human,
    AiPersona,
    SttListener,
    Ambient,
}

/// Metadata attached to each LiveKit participant's JWT.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantMetadata {
    pub role: ParticipantRole,
}

impl ParticipantMetadata {
    pub fn new(role: ParticipantRole) -> Self {
        Self { role }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_json(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_roundtrip_json_only() {
        let json = b"{\"type\":\"JoinRoom\"}";
        let frame = encode_frame(json, None);
        assert_eq!(frame.len(), 4 + json.len());

        let len = u32::from_le_bytes(frame[0..4].try_into().unwrap()) as usize;
        let (decoded_json, decoded_bin) = decode_frame(&frame[4..4 + len]);
        assert_eq!(decoded_json, json);
        assert!(decoded_bin.is_none());
    }

    #[test]
    fn test_frame_roundtrip_with_binary() {
        let json = b"{\"type\":\"Speak\"}";
        let audio: Vec<u8> = vec![0x01, 0x02, 0x03, 0x04];
        let frame = encode_frame(json, Some(&audio));

        let len = u32::from_le_bytes(frame[0..4].try_into().unwrap()) as usize;
        let (decoded_json, decoded_bin) = decode_frame(&frame[4..4 + len]);
        assert_eq!(decoded_json, json);
        assert_eq!(decoded_bin.unwrap(), &audio[..]);
    }

    #[test]
    fn test_participant_metadata_serde() {
        let meta = ParticipantMetadata::new(ParticipantRole::AiPersona);
        let json = meta.to_json();
        assert!(json.contains("ai_persona"));
        let roundtrip = ParticipantMetadata::from_json(&json).unwrap();
        assert_eq!(roundtrip.role, ParticipantRole::AiPersona);
    }
}
