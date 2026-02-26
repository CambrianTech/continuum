//! Frame Publisher — cross-platform abstraction for converting rendered RGBA frames
//! into a format LiveKit can consume and publishing them.
//!
//! Architecture:
//!   AvatarRenderer → RgbaFrame → crossbeam channel → FramePublisher → LiveKit NativeVideoSource
//!
//! The video loop calls `try_publish()` each iteration — it doesn't know or care
//! what format conversion (I420, CVPixelBuffer, NativeBuffer) happens inside.
//!
//! Implementations:
//!   - CpuI420Publisher: RGBA → I420 (BT.601), works everywhere (default)
//!   - NativeBufferPublisher: RGBA → CVPixelBuffer (macOS only, skips I420)
//!   - GpuBridgePublisher: IOSurface zero-copy (macOS, future — Phase 4)

use crossbeam_channel::{Receiver, TryRecvError};
use livekit::webrtc::video_frame::{I420Buffer, VideoFrame, VideoRotation};
use livekit::webrtc::video_source::native::NativeVideoSource;

use super::frame::RgbaFrame;

// =============================================================================
// FramePublisher trait
// =============================================================================

/// Converts rendered RGBA frames to a format LiveKit can consume.
/// Selected at startup based on platform + capabilities.
///
/// The video loop calls `try_publish()` each iteration — it doesn't know
/// or care what format conversion happens inside.
pub trait FramePublisher: Send {
    /// Human-readable name for logging (e.g. "cpu-i420", "native-buffer").
    fn name(&self) -> &'static str;

    /// Try to publish the next available frame. Non-blocking.
    /// Returns Ok(true) if a frame was published, Ok(false) if none ready.
    fn try_publish(&mut self, source: &NativeVideoSource) -> Result<bool, PublishError>;
}

/// Errors from frame publishing.
#[derive(Debug)]
pub enum PublishError {
    /// The frame channel was closed (renderer thread exited).
    ChannelClosed,
}

impl std::fmt::Display for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PublishError::ChannelClosed => write!(f, "frame channel closed"),
        }
    }
}

// =============================================================================
// CpuI420Publisher — default, cross-platform
// =============================================================================

/// CPU-based frame publisher: RGBA → I420 (BT.601) → LiveKit.
///
/// Works on all platforms. Each frame allocates a new I420Buffer because
/// LiveKit's VideoFrame takes ownership (C++ UniquePtr, not reusable).
/// At 15fps per agent this is ~460KB × 15 = ~6.9MB/s per agent.
pub struct CpuI420Publisher {
    frame_rx: Receiver<RgbaFrame>,
    width: u32,
    height: u32,
    frame_count: u64,
}

impl CpuI420Publisher {
    pub fn new(frame_rx: Receiver<RgbaFrame>, width: u32, height: u32) -> Self {
        Self { frame_rx, width, height, frame_count: 0 }
    }
}

impl FramePublisher for CpuI420Publisher {
    fn name(&self) -> &'static str { "cpu-i420" }

    fn try_publish(&mut self, source: &NativeVideoSource) -> Result<bool, PublishError> {
        match self.frame_rx.try_recv() {
            Ok(frame) => {
                let mut buffer = I420Buffer::new(self.width, self.height);
                rgba_to_i420_into(&frame.data, &mut buffer, self.width, self.height);
                let video_frame = VideoFrame {
                    rotation: VideoRotation::VideoRotation0,
                    timestamp_us: 0, // auto-timestamp
                    buffer,
                };
                source.capture_frame(&video_frame);

                self.frame_count += 1;
                // Periodic health log: every 450 frames (~30s at 15fps)
                if self.frame_count == 1 || self.frame_count % 450 == 0 {
                    crate::clog_info!(
                        "📹 CpuI420Publisher: {} frames published ({}×{})",
                        self.frame_count, self.width, self.height
                    );
                }

                Ok(true)
            }
            Err(TryRecvError::Empty) => Ok(false),
            Err(TryRecvError::Disconnected) => Err(PublishError::ChannelClosed),
        }
    }
}

// =============================================================================
// Factory — selects the best publisher for the current platform
// =============================================================================

/// Create the best available FramePublisher for the current platform.
///
/// Currently uses CpuI420Publisher on all platforms (proven stable).
/// NativeBufferPublisher (macOS CVPixelBuffer) is implemented but disabled —
/// WebRTC's encoder may not handle BGRA CVPixelBuffers correctly (dark frames).
/// Enable via CONTINUUM_NATIVE_VIDEO=1 environment variable for testing.
pub fn create_publisher(
    frame_rx: Receiver<RgbaFrame>,
    width: u32,
    height: u32,
) -> Box<dyn FramePublisher> {
    #[cfg(target_os = "macos")]
    {
        if std::env::var("CONTINUUM_NATIVE_VIDEO").map(|v| v == "1").unwrap_or(false) {
            use super::publishers::native_buffer::NativeBufferPublisher;
            match NativeBufferPublisher::try_new(frame_rx.clone(), width, height) {
                Ok(publisher) => {
                    crate::clog_info!("📹 NativeBufferPublisher enabled via CONTINUUM_NATIVE_VIDEO=1");
                    return Box::new(publisher);
                }
                Err(e) => {
                    crate::clog_warn!("📹 NativeBufferPublisher failed: {}, falling back to CPU I420", e);
                }
            }
        }
    }
    Box::new(CpuI420Publisher::new(frame_rx, width, height))
}

// =============================================================================
// RGBA → I420 conversion (BT.601)
// =============================================================================

/// Convert RGBA8 pixel data into an existing I420 buffer.
/// Uses BT.601 color space conversion with fixed-point arithmetic.
///
/// The buffer must have been created with the same width/height dimensions.
/// Public within the crate for use by both CpuI420Publisher and direct publish paths.
pub(crate) fn rgba_to_i420_into(rgba: &[u8], buffer: &mut I420Buffer, width: u32, height: u32) {
    let w = width as usize;
    let h = height as usize;

    let (data_y, data_u, data_v) = buffer.data_mut();

    // Y plane: one luma value per pixel
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let r = rgba[i] as i32;
            let g = rgba[i + 1] as i32;
            let b = rgba[i + 2] as i32;
            // BT.601: Y = (66R + 129G + 25B + 128) >> 8 + 16
            data_y[y * w + x] = (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(0, 255) as u8;
        }
    }

    // U and V planes: one chroma value per 2x2 block (subsampled)
    let cw = (w + 1) / 2;
    let ch = (h + 1) / 2;
    for cy in 0..ch {
        for cx in 0..cw {
            let px = cx * 2;
            let py = cy * 2;
            let i = (py * w + px) * 4;
            let r = rgba[i] as i32;
            let g = rgba[i + 1] as i32;
            let b = rgba[i + 2] as i32;
            // BT.601: U = (-38R - 74G + 112B + 128) >> 8 + 128
            data_u[cy * cw + cx] = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
            // BT.601: V = (112R - 94G - 18B + 128) >> 8 + 128
            data_v[cy * cw + cx] = (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgba_to_i420_black_frame() {
        let width = 4u32;
        let height = 4u32;
        let rgba = vec![0u8; (width * height * 4) as usize]; // All black
        let mut buffer = I420Buffer::new(width, height);
        rgba_to_i420_into(&rgba, &mut buffer, width, height);

        let (y, u, v) = buffer.data();
        // Black in BT.601: Y=16, U=128, V=128
        assert!(y.iter().all(|&val| val == 16), "Y plane should be 16 for black");
        assert!(u.iter().all(|&val| val == 128), "U plane should be 128 for black");
        assert!(v.iter().all(|&val| val == 128), "V plane should be 128 for black");
    }

    #[test]
    fn test_rgba_to_i420_white_frame() {
        let width = 2u32;
        let height = 2u32;
        let rgba = vec![255u8; (width * height * 4) as usize]; // All white (RGBA=255)
        let mut buffer = I420Buffer::new(width, height);
        rgba_to_i420_into(&rgba, &mut buffer, width, height);

        let (y, u, v) = buffer.data();
        // White in BT.601: Y=235, U=128, V=128
        assert!(y.iter().all(|&val| val == 235), "Y plane should be 235 for white");
        assert!(u.iter().all(|&val| val == 128), "U plane should be 128 for white");
        assert!(v.iter().all(|&val| val == 128), "V plane should be 128 for white");
    }
}
