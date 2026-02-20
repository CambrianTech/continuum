use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtteranceEvent {
    pub session_id: Uuid,
    pub speaker_id: Uuid,
    pub speaker_name: String,
    pub speaker_type: SpeakerType,
    pub transcript: String,
    pub confidence: f32,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpeakerType {
    Human,
    Persona,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceParticipant {
    pub user_id: Uuid,
    pub display_name: String,
    pub participant_type: SpeakerType,
    pub expertise: Vec<String>,
    /// Audio-native models (Gemini Live, Qwen3-Omni, GPT-4o Realtime) hear raw audio
    /// through the mixer's mix-minus stream. They must NOT receive text transcriptions
    /// from on_utterance() — otherwise they respond twice (once to audio, once to text).
    #[serde(default)]
    pub is_audio_native: bool,
}

#[derive(Debug, Clone)]
pub struct ConversationContext {
    pub session_id: Uuid,
    pub room_id: Uuid,
    pub recent_utterances: Vec<UtteranceEvent>,
    pub last_responder_id: Option<Uuid>,
    pub turn_count: u32,
}

impl ConversationContext {
    pub fn new(session_id: Uuid, room_id: Uuid) -> Self {
        Self {
            session_id,
            room_id,
            recent_utterances: Vec::new(),
            last_responder_id: None,
            turn_count: 0,
        }
    }

    pub fn add_utterance(&mut self, event: UtteranceEvent) {
        self.recent_utterances.push(event);
        if self.recent_utterances.len() > 20 {
            self.recent_utterances.remove(0);
        }
        self.turn_count += 1;
    }
}

// ============================================================================
// Binary Frame Protocol
// ============================================================================
// Audio and video both arrive as binary WebSocket messages.
// First byte discriminates the frame type.

/// Binary frame type discriminator (first byte of every binary WebSocket message)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameKind {
    /// PCM16 audio samples (i16 little-endian)
    Audio = 0x01,
    /// Encoded video frame (VP8/H264/raw depending on negotiation)
    Video = 0x02,
    /// Avatar state update (JSON-encoded animation commands)
    AvatarState = 0x03,
}

impl FrameKind {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(Self::Audio),
            0x02 => Some(Self::Video),
            0x03 => Some(Self::AvatarState),
            _ => None,
        }
    }
}

/// Video pixel format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/VideoPixelFormat.ts")]
pub enum VideoPixelFormat {
    /// RGBA 8-bit per channel (browser canvas default)
    RGBA8,
    /// NV12 (luma + interleaved chroma, GPU-friendly)
    NV12,
    /// Encoded VP8 frame (compressed)
    VP8,
    /// Encoded H264 NAL unit (compressed)
    H264,
    /// JPEG (for low-bandwidth scenarios)
    JPEG,
}

/// A video frame header — precedes the raw pixel/encoded data in a binary message.
///
/// Wire format: [FrameKind::Video (1 byte)] [VideoFrameHeader (fixed)] [pixel data]
///
/// Header layout (16 bytes, little-endian):
///   bytes 0-1:  width  (u16)
///   bytes 2-3:  height (u16)
///   byte  4:    pixel_format (VideoPixelFormat as u8)
///   byte  5:    flags (reserved, 0 for now)
///   bytes 6-9:  timestamp_ms (u32, relative to call start)
///   bytes 10-13: sequence (u32, frame counter)
///   bytes 14-15: reserved (0)
#[derive(Debug, Clone, Copy)]
pub struct VideoFrameHeader {
    pub width: u16,
    pub height: u16,
    pub pixel_format: VideoPixelFormat,
    pub timestamp_ms: u32,
    pub sequence: u32,
}

impl VideoFrameHeader {
    pub const WIRE_SIZE: usize = 16;

    pub fn encode(&self) -> [u8; Self::WIRE_SIZE] {
        let mut buf = [0u8; Self::WIRE_SIZE];
        buf[0..2].copy_from_slice(&self.width.to_le_bytes());
        buf[2..4].copy_from_slice(&self.height.to_le_bytes());
        buf[4] = self.pixel_format as u8;
        buf[5] = 0; // flags reserved
        buf[6..10].copy_from_slice(&self.timestamp_ms.to_le_bytes());
        buf[10..14].copy_from_slice(&self.sequence.to_le_bytes());
        buf[14..16].copy_from_slice(&[0, 0]); // reserved
        buf
    }

    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::WIRE_SIZE {
            return None;
        }
        let width = u16::from_le_bytes([buf[0], buf[1]]);
        let height = u16::from_le_bytes([buf[2], buf[3]]);
        let pixel_format = match buf[4] {
            0 => VideoPixelFormat::RGBA8,
            1 => VideoPixelFormat::NV12,
            2 => VideoPixelFormat::VP8,
            3 => VideoPixelFormat::H264,
            4 => VideoPixelFormat::JPEG,
            _ => return None,
        };
        let timestamp_ms = u32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]);
        let sequence = u32::from_le_bytes([buf[10], buf[11], buf[12], buf[13]]);
        Some(Self {
            width,
            height,
            pixel_format,
            timestamp_ms,
            sequence,
        })
    }
}

// VideoPixelFormat serializes to u8 via `as u8` cast in VideoFrameHeader::encode().
// Decode uses the explicit match in VideoFrameHeader::decode().

/// A complete video frame: header + pixel data
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub header: VideoFrameHeader,
    pub data: Vec<u8>,
}

impl VideoFrame {
    /// Encode to binary wire format: [VideoFrameHeader][pixel data]
    pub fn to_bytes(&self) -> Vec<u8> {
        let header_bytes = self.header.encode();
        let mut buf = Vec::with_capacity(header_bytes.len() + self.data.len());
        buf.extend_from_slice(&header_bytes);
        buf.extend_from_slice(&self.data);
        buf
    }

    /// Decode from binary wire format
    pub fn from_bytes(buf: &[u8]) -> Option<Self> {
        let header = VideoFrameHeader::decode(buf)?;
        let data = buf[VideoFrameHeader::WIRE_SIZE..].to_vec();
        Some(Self { header, data })
    }

    /// Total byte size of the frame data (excluding header)
    pub fn data_size(&self) -> usize {
        self.data.len()
    }
}

/// Avatar animation state — sent from server to browser for driving avatar rendering.
///
/// The browser uses three.js/VRM or PixiJS/Live2D to render the avatar.
/// The server sends periodic state updates so the browser knows what to animate.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/AvatarState.ts")]
pub struct AvatarState {
    /// Which persona this state is for
    pub persona_id: String,
    /// Is this persona currently speaking?
    pub speaking: bool,
    /// Is this persona currently listening (paying attention)?
    pub listening: bool,
    /// Current emotion (neutral, happy, thinking, surprised, etc.)
    pub emotion: String,
    /// Viseme index for lip-sync (0-14, maps to standard viseme set)
    /// Only meaningful when speaking=true
    #[serde(default)]
    pub viseme: u8,
    /// Viseme blend weight (0.0 to 1.0)
    #[serde(default)]
    pub viseme_weight: f32,
    /// Head rotation (pitch, yaw, roll in radians)
    #[serde(default)]
    pub head_rotation: [f32; 3],
    /// Eye gaze target (x, y in normalized screen coordinates, -1 to 1)
    #[serde(default)]
    pub gaze_target: [f32; 2],
    /// Timestamp (ms since epoch)
    pub timestamp: u64,
}

#[cfg(test)]
mod frame_tests {
    use super::*;

    #[test]
    fn test_frame_kind_roundtrip() {
        assert_eq!(FrameKind::from_byte(0x01), Some(FrameKind::Audio));
        assert_eq!(FrameKind::from_byte(0x02), Some(FrameKind::Video));
        assert_eq!(FrameKind::from_byte(0x03), Some(FrameKind::AvatarState));
        assert_eq!(FrameKind::from_byte(0xFF), None);
    }

    #[test]
    fn test_video_frame_header_roundtrip() {
        let header = VideoFrameHeader {
            width: 640,
            height: 480,
            pixel_format: VideoPixelFormat::VP8,
            timestamp_ms: 12345,
            sequence: 42,
        };
        let bytes = header.encode();
        assert_eq!(bytes.len(), VideoFrameHeader::WIRE_SIZE);

        let decoded = VideoFrameHeader::decode(&bytes).unwrap();
        assert_eq!(decoded.width, 640);
        assert_eq!(decoded.height, 480);
        assert_eq!(decoded.pixel_format, VideoPixelFormat::VP8);
        assert_eq!(decoded.timestamp_ms, 12345);
        assert_eq!(decoded.sequence, 42);
    }

    #[test]
    fn test_video_frame_to_from_bytes() {
        let frame = VideoFrame {
            header: VideoFrameHeader {
                width: 320,
                height: 240,
                pixel_format: VideoPixelFormat::JPEG,
                timestamp_ms: 1000,
                sequence: 1,
            },
            data: vec![0xFF, 0xD8, 0xFF, 0xE0], // JPEG magic bytes
        };

        let bytes = frame.to_bytes();
        assert_eq!(bytes.len(), VideoFrameHeader::WIRE_SIZE + 4);

        let decoded = VideoFrame::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.header.width, 320);
        assert_eq!(decoded.header.height, 240);
        assert_eq!(decoded.data, vec![0xFF, 0xD8, 0xFF, 0xE0]);
    }

    #[test]
    fn test_avatar_state_serialize() {
        let state = AvatarState {
            persona_id: "test-ai".to_string(),
            speaking: true,
            listening: false,
            emotion: "happy".to_string(),
            viseme: 5,
            viseme_weight: 0.8,
            head_rotation: [0.1, -0.05, 0.0],
            gaze_target: [0.5, -0.3],
            timestamp: 1000,
        };

        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("speaking"));
        assert!(json.contains("happy"));

        let decoded: AvatarState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.viseme, 5);
        assert!(decoded.speaking);
    }

    #[test]
    fn test_video_pixel_format_u8_cast() {
        assert_eq!(VideoPixelFormat::RGBA8 as u8, 0);
        assert_eq!(VideoPixelFormat::NV12 as u8, 1);
        assert_eq!(VideoPixelFormat::VP8 as u8, 2);
        assert_eq!(VideoPixelFormat::H264 as u8, 3);
        assert_eq!(VideoPixelFormat::JPEG as u8, 4);
    }
}
