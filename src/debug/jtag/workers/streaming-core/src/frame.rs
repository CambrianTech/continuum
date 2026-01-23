//! Frame Types
//!
//! Frames are the data units that flow through the pipeline.
//! Each frame type is optimized for its modality.

use serde::{Deserialize, Serialize};

/// Audio frame - 20ms of audio at 16kHz mono
#[derive(Clone)]
pub struct AudioFrame {
    /// PCM samples (16-bit signed)
    pub samples: Vec<i16>,

    /// Timestamp in microseconds
    pub timestamp_us: u64,

    /// Sample rate (typically 16000)
    pub sample_rate: u32,

    /// Channel count (typically 1 for mono)
    pub channels: u8,
}

impl AudioFrame {
    /// Create a new audio frame
    pub fn new(samples: Vec<i16>, timestamp_us: u64, sample_rate: u32) -> Self {
        Self {
            samples,
            timestamp_us,
            sample_rate,
            channels: 1,
        }
    }

    /// Duration in milliseconds
    pub fn duration_ms(&self) -> f64 {
        (self.samples.len() as f64 / self.sample_rate as f64) * 1000.0
    }

    /// Create empty frame (for initialization)
    pub fn empty() -> Self {
        Self {
            samples: Vec::new(),
            timestamp_us: 0,
            sample_rate: 16000,
            channels: 1,
        }
    }
}

/// Video frame - GPU texture reference (zero-copy)
#[derive(Clone)]
pub struct VideoFrame {
    /// GPU texture ID (data stays on GPU)
    pub texture_id: u64,

    /// Frame dimensions
    pub width: u16,
    pub height: u16,

    /// Timestamp in microseconds
    pub timestamp_us: u64,

    /// Pixel format
    pub format: PixelFormat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PixelFormat {
    RGBA8,
    RGB8,
    NV12,
    YUV420,
}

impl VideoFrame {
    pub fn new(texture_id: u64, width: u16, height: u16, timestamp_us: u64) -> Self {
        Self {
            texture_id,
            width,
            height,
            timestamp_us,
            format: PixelFormat::RGBA8,
        }
    }
}

/// Text frame - tokenized text
#[derive(Clone)]
pub struct TextFrame {
    /// Token IDs (or raw text)
    pub content: TextContent,

    /// Timestamp in microseconds
    pub timestamp_us: u64,

    /// Whether this is a final or partial result
    pub is_final: bool,
}

#[derive(Clone)]
pub enum TextContent {
    /// Raw text string
    Text(String),

    /// Tokenized (token IDs)
    Tokens(Vec<u32>),
}

impl TextFrame {
    pub fn text(content: String, timestamp_us: u64, is_final: bool) -> Self {
        Self {
            content: TextContent::Text(content),
            timestamp_us,
            is_final,
        }
    }

    pub fn tokens(tokens: Vec<u32>, timestamp_us: u64, is_final: bool) -> Self {
        Self {
            content: TextContent::Tokens(tokens),
            timestamp_us,
            is_final,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match &self.content {
            TextContent::Text(s) => Some(s),
            TextContent::Tokens(_) => None,
        }
    }
}

/// Image frame - for generated images
#[derive(Clone)]
pub struct ImageFrame {
    /// Image data or GPU texture ID
    pub data: ImageData,

    /// Dimensions
    pub width: u32,
    pub height: u32,

    /// Timestamp
    pub timestamp_us: u64,
}

#[derive(Clone)]
pub enum ImageData {
    /// Raw bytes (RGBA)
    Bytes(Vec<u8>),

    /// GPU texture reference
    Texture(u64),

    /// File path (for large images)
    Path(String),
}

impl ImageFrame {
    pub fn from_bytes(data: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            data: ImageData::Bytes(data),
            width,
            height,
            timestamp_us: 0,
        }
    }

    pub fn from_texture(texture_id: u64, width: u32, height: u32) -> Self {
        Self {
            data: ImageData::Texture(texture_id),
            width,
            height,
            timestamp_us: 0,
        }
    }
}

/// Generic frame wrapper - for heterogeneous pipelines
#[derive(Clone)]
pub enum Frame {
    Audio(AudioFrame),
    Video(VideoFrame),
    Text(TextFrame),
    Image(ImageFrame),
}

impl Frame {
    pub fn timestamp_us(&self) -> u64 {
        match self {
            Frame::Audio(f) => f.timestamp_us,
            Frame::Video(f) => f.timestamp_us,
            Frame::Text(f) => f.timestamp_us,
            Frame::Image(f) => f.timestamp_us,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Frame::Audio(_) => "audio",
            Frame::Video(_) => "video",
            Frame::Text(_) => "text",
            Frame::Image(_) => "image",
        }
    }
}

impl From<AudioFrame> for Frame {
    fn from(f: AudioFrame) -> Self {
        Frame::Audio(f)
    }
}

impl From<VideoFrame> for Frame {
    fn from(f: VideoFrame) -> Self {
        Frame::Video(f)
    }
}

impl From<TextFrame> for Frame {
    fn from(f: TextFrame) -> Self {
        Frame::Text(f)
    }
}

impl From<ImageFrame> for Frame {
    fn from(f: ImageFrame) -> Self {
        Frame::Image(f)
    }
}
