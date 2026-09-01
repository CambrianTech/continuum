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

    /// Bind a CORE-assigned outbound media channel (binary plane, 2026-09-01):
    /// after this, frames for `(call_id, user_id, kind)` arrive as
    /// [`MEDIA_MAGIC`] binary frames on `channel` — identity once, then
    /// streaming. Replaces per-frame `Speak`/`InjectAudio`/`PublishVideoFrame`
    /// JSON envelopes for continuous media.
    OpenMediaOut {
        channel: u16,
        kind: MediaKind,
        call_id: String,
        user_id: String,
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

    /// Bind a BRIDGE-assigned inbound media channel (binary plane,
    /// 2026-09-01): after this, `(call_id, speaker_id, kind)`'s media arrives
    /// as [`MEDIA_MAGIC`] binary frames on `channel` — identity once, then
    /// streaming. Replaces the per-frame `AudioFrame`/`VideoFrame` JSON
    /// envelopes for continuous media.
    MediaChannelOpened {
        channel: u16,
        kind: MediaKind,
        call_id: String,
        speaker_id: String,
        speaker_name: String,
        track_sid: String,
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
// BINARY MEDIA PLANE — handle + stream, never JSON-per-frame (2026-09-01)
// =============================================================================
//
// Continuous media (audio PCM, avatar/video frames — and tomorrow, 3D/game
// state streams) must NEVER pay a JSON envelope per frame. The old shape —
// `{type:"AudioFrame", call_id:"<uuid>", speaker_id:"<uuid>", …}` serialized
// and DOUBLE-parsed 50×/sec/speaker, and `PublishVideoFrame` JSON wrapping
// ~1MB of RGBA 30×/sec/persona — is the handle-less anti-pattern: identity
// restated per frame, parse cost per frame, allocation per frame.
//
// The plane: identity binds ONCE on the control plane (a JSON channel-open
// message), yielding a u16 CHANNEL handle; media then flows as tight binary
// frames. A JSON frame always begins with `{` (0x7B); a media frame begins
// with [`MEDIA_MAGIC`] — one byte disambiguates the planes on the same socket
// with zero scanning.
//
//   [len: u32 LE] [0xB1] [kind: u8] [channel: u16 LE] [payload…]
//     kind 1 = AUDIO_PCM:  payload = i16 LE mono PCM at the core rate (16k)
//     kind 2 = VIDEO_RGBA: payload = [w: u16 LE][h: u16 LE][rgba bytes]
//     kind 3 = VIDEO_JPEG: payload = [w: u16 LE][h: u16 LE][jpeg bytes]
//
// Channel id spaces are PER-DIRECTION (each sender assigns its own ids and
// announces the binding), so no negotiation and no races. Receivers hold a
// tiny channel→binding map and touch no strings on the frame path.

/// First byte of a binary media frame. JSON frames start with `{` (0x7B).
pub const MEDIA_MAGIC: u8 = 0xB1;

/// Media payload kinds for the binary plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    AudioPcm = 1,
    VideoRgba = 2,
    VideoJpeg = 3,
}

impl MediaKind {
    pub fn from_u8(b: u8) -> Option<Self> {
        match b {
            1 => Some(Self::AudioPcm),
            2 => Some(Self::VideoRgba),
            3 => Some(Self::VideoJpeg),
            _ => None,
        }
    }
}

/// Encode one binary media frame into `buf` (cleared first, capacity reused —
/// the caller keeps one buffer per stream so the frame path allocates nothing
/// once warm).
pub fn encode_media_frame_into(buf: &mut Vec<u8>, kind: MediaKind, channel: u16, payload: &[u8]) {
    buf.clear();
    let total = 1 + 1 + 2 + payload.len();
    buf.reserve(4 + total);
    buf.extend_from_slice(&(total as u32).to_le_bytes());
    buf.push(MEDIA_MAGIC);
    buf.push(kind as u8);
    buf.extend_from_slice(&channel.to_le_bytes());
    buf.extend_from_slice(payload);
}

/// Parse a frame (length prefix already stripped) as a binary media frame.
/// `None` = not a media frame (fall through to the JSON plane).
pub fn parse_media_frame(frame: &[u8]) -> Option<(MediaKind, u16, &[u8])> {
    if frame.len() < 4 || frame[0] != MEDIA_MAGIC {
        return None;
    }
    let kind = MediaKind::from_u8(frame[1])?;
    let channel = u16::from_le_bytes([frame[2], frame[3]]);
    Some((kind, channel, &frame[4..]))
}

/// Prepend a `[w][h]` dimension header to a video payload (RGBA or JPEG).
pub fn encode_video_payload_into(buf: &mut Vec<u8>, width: u16, height: u16, pixels: &[u8]) {
    buf.clear();
    buf.reserve(4 + pixels.len());
    buf.extend_from_slice(&width.to_le_bytes());
    buf.extend_from_slice(&height.to_le_bytes());
    buf.extend_from_slice(pixels);
}

/// Split a video payload into `(width, height, pixels)`.
pub fn parse_video_payload(payload: &[u8]) -> Option<(u16, u16, &[u8])> {
    if payload.len() < 4 {
        return None;
    }
    let w = u16::from_le_bytes([payload[0], payload[1]]);
    let h = u16::from_le_bytes([payload[2], payload[3]]);
    Some((w, h, &payload[4..]))
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
    fn media_frame_roundtrip_and_json_disambiguation() {
        // what this catches: the binary media plane and the JSON plane share
        // one socket — a media frame must round-trip losslessly AND a JSON
        // frame must NEVER parse as media (0xB1 vs '{' is the whole contract).
        let mut buf = Vec::new();
        let pcm: Vec<u8> = (0u16..320).flat_map(|s| s.to_le_bytes()).collect();
        encode_media_frame_into(&mut buf, MediaKind::AudioPcm, 7, &pcm);
        let len = u32::from_le_bytes(buf[0..4].try_into().unwrap()) as usize;
        let (kind, channel, payload) = parse_media_frame(&buf[4..4 + len]).unwrap();
        assert_eq!(kind, MediaKind::AudioPcm);
        assert_eq!(channel, 7);
        assert_eq!(payload, &pcm[..]);

        // JSON frames fall through to the JSON plane.
        assert!(parse_media_frame(b"{\"type\":\"Speak\"}").is_none());
        // Unknown kind byte refuses rather than mis-routing.
        let mut bogus = buf[4..4 + len].to_vec();
        bogus[1] = 99;
        assert!(parse_media_frame(&bogus).is_none());

        // Video payload dims survive the [w][h] header round-trip.
        let mut vbuf = Vec::new();
        encode_video_payload_into(&mut vbuf, 640, 360, &[9, 8, 7]);
        let (w, h, px) = parse_video_payload(&vbuf).unwrap();
        assert_eq!((w, h, px), (640, 360, &[9u8, 8, 7][..]));
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
