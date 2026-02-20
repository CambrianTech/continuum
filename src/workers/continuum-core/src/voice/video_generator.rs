//! Test Video Frame Generator
//!
//! Generates simple test pattern video frames to prove the video stream
//! plumbing works end-to-end. This is a placeholder — real video sources
//! include: webcam capture (humans), bgfx-rs rendered scenes (AI avatars),
//! API-generated avatar video (personas), screen captures.
//!
//! The test pattern is SMPTE-style color bars with a moving scan line
//! to visually confirm frames are arriving and updating.

use crate::voice::types::{VideoFrame, VideoFrameHeader, VideoPixelFormat};

/// SMPTE color bar colors (RGBA8)
const COLOR_BARS: [[u8; 4]; 7] = [
    [192, 192, 192, 255], // White (75%)
    [192, 192, 0, 255],   // Yellow
    [0, 192, 192, 255],   // Cyan
    [0, 192, 0, 255],     // Green
    [192, 0, 192, 255],   // Magenta
    [192, 0, 0, 255],     // Red
    [0, 0, 192, 255],     // Blue
];

/// Generates test pattern video frames with a moving scan line.
pub struct TestPatternGenerator {
    width: u16,
    height: u16,
    sequence: u32,
    /// Pre-rendered color bar base (reused every frame, scan line drawn on top)
    base_frame: Vec<u8>,
}

impl TestPatternGenerator {
    /// Create a new test pattern generator.
    /// Default resolution: 160x120 (tiny — just proving the stream works)
    pub fn new(width: u16, height: u16) -> Self {
        let base_frame = Self::render_color_bars(width, height);
        Self {
            width,
            height,
            sequence: 0,
            base_frame,
        }
    }

    /// Create with default test resolution (160x120)
    pub fn default_test() -> Self {
        Self::new(160, 120)
    }

    /// Generate the next frame. Returns a complete VideoFrame ready for broadcasting.
    /// The scan line position is based on sequence number for visual motion.
    pub fn next_frame(&mut self, timestamp_ms: u32) -> VideoFrame {
        let w = self.width as usize;
        let h = self.height as usize;

        // Clone the base color bars
        let mut data = self.base_frame.clone();

        // Draw a moving horizontal scan line (bright white)
        let scan_y = (self.sequence as usize * 3) % h;
        let row_start = scan_y * w * 4;
        let row_end = row_start + w * 4;
        if row_end <= data.len() {
            for x in 0..w {
                let px = row_start + x * 4;
                data[px] = 255;     // R
                data[px + 1] = 255; // G
                data[px + 2] = 255; // B
                data[px + 3] = 255; // A
            }
        }

        // Draw frame counter in top-left corner (8x8 block, color cycles)
        let counter_color_idx = (self.sequence as usize) % COLOR_BARS.len();
        let color = COLOR_BARS[counter_color_idx];
        for y in 0..8.min(h) {
            for x in 0..8.min(w) {
                let px = (y * w + x) * 4;
                data[px] = color[0];
                data[px + 1] = color[1];
                data[px + 2] = color[2];
                data[px + 3] = color[3];
            }
        }

        let header = VideoFrameHeader {
            width: self.width,
            height: self.height,
            pixel_format: VideoPixelFormat::RGBA8,
            timestamp_ms,
            sequence: self.sequence,
        };

        self.sequence += 1;

        VideoFrame { header, data }
    }

    /// Render SMPTE-style color bars as RGBA8 pixel data
    fn render_color_bars(width: u16, height: u16) -> Vec<u8> {
        let w = width as usize;
        let h = height as usize;
        let mut data = vec![0u8; w * h * 4];

        let bar_width = w / COLOR_BARS.len();

        for y in 0..h {
            for x in 0..w {
                let bar_idx = (x / bar_width).min(COLOR_BARS.len() - 1);
                let color = COLOR_BARS[bar_idx];
                let px = (y * w + x) * 4;
                data[px] = color[0];
                data[px + 1] = color[1];
                data[px + 2] = color[2];
                data[px + 3] = color[3];
            }
        }

        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generator_creates_valid_frames() {
        let mut gen = TestPatternGenerator::default_test();
        let frame = gen.next_frame(0);

        assert_eq!(frame.header.width, 160);
        assert_eq!(frame.header.height, 120);
        assert_eq!(frame.header.pixel_format, VideoPixelFormat::RGBA8);
        assert_eq!(frame.header.sequence, 0);
        // 160 * 120 * 4 (RGBA) = 76800 bytes
        assert_eq!(frame.data.len(), 76800);
    }

    #[test]
    fn test_generator_increments_sequence() {
        let mut gen = TestPatternGenerator::new(80, 60);

        let f1 = gen.next_frame(0);
        let f2 = gen.next_frame(100);
        let f3 = gen.next_frame(200);

        assert_eq!(f1.header.sequence, 0);
        assert_eq!(f2.header.sequence, 1);
        assert_eq!(f3.header.sequence, 2);
    }

    #[test]
    fn test_generator_frames_differ() {
        let mut gen = TestPatternGenerator::default_test();

        let f1 = gen.next_frame(0);
        let f2 = gen.next_frame(100);

        // Frames should differ (scan line moves, counter block changes color)
        assert_ne!(f1.data, f2.data);
    }

    #[test]
    fn test_frame_wire_roundtrip() {
        let mut gen = TestPatternGenerator::default_test();
        let frame = gen.next_frame(12345);

        // Encode to wire format
        let bytes = frame.to_bytes();
        assert_eq!(bytes.len(), VideoFrameHeader::WIRE_SIZE + 76800);

        // Decode from wire format
        let decoded = VideoFrame::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.header.width, 160);
        assert_eq!(decoded.header.height, 120);
        assert_eq!(decoded.header.timestamp_ms, 12345);
        assert_eq!(decoded.header.sequence, 0);
        assert_eq!(decoded.data.len(), 76800);
        assert_eq!(decoded.data, frame.data);
    }

    #[test]
    fn test_custom_resolution() {
        let mut gen = TestPatternGenerator::new(320, 240);
        let frame = gen.next_frame(0);

        assert_eq!(frame.header.width, 320);
        assert_eq!(frame.header.height, 240);
        assert_eq!(frame.data.len(), 320 * 240 * 4);
    }
}
