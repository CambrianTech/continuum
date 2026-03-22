//! WgpuI420Publisher — GPU-accelerated I420 frame publisher.
//!
//! Receives pre-converted I420 data from the wgpu compute shader pipeline
//! (wgpu_gpu_convert.rs). No CPU color conversion needed — the GPU already
//! did RGBA→I420 via compute shader.
//!
//! The data arrives as raw I420 bytes via crossbeam channel:
//!   [Y plane: w*h] [U plane: (w/2)*(h/2)] [V plane: (w/2)*(h/2)]
//!
//! This publisher just copies the planes into LiveKit's I420Buffer and publishes.
//! The expensive RGBA→I420 conversion happened on GPU, saving ~1ms CPU per frame.

use crossbeam_channel::{Receiver, TryRecvError};
use livekit::webrtc::video_frame::{I420Buffer, VideoFrame, VideoRotation};
use livekit::webrtc::video_source::native::NativeVideoSource;

use super::super::frame::RgbaFrame;
use super::super::frame_publisher::{FramePublisher, PublishError};

/// GPU-accelerated I420 publisher.
/// Receives pre-converted I420 data from wgpu compute shader.
pub struct WgpuI420Publisher {
    frame_rx: Receiver<RgbaFrame>, // Actually I420 data, not RGBA
    width: u32,
    height: u32,
    frame_count: u64,
    started_at: std::time::Instant,
}

impl WgpuI420Publisher {
    pub fn new(frame_rx: Receiver<RgbaFrame>, width: u32, height: u32) -> Self {
        Self {
            frame_rx,
            width,
            height,
            frame_count: 0,
            started_at: std::time::Instant::now(),
        }
    }
}

impl FramePublisher for WgpuI420Publisher {
    fn name(&self) -> &'static str {
        "wgpu-i420"
    }

    fn try_publish(&mut self, source: &NativeVideoSource) -> Result<bool, PublishError> {
        match self.frame_rx.try_recv() {
            Ok(frame) => {
                let w = frame.width;
                let h = frame.height;
                let mut buffer = I420Buffer::new(w, h);

                // Copy pre-converted I420 planes from GPU output
                copy_i420_planes(&frame.data, &mut buffer, w, h);

                let video_frame = VideoFrame {
                    rotation: VideoRotation::VideoRotation0,
                    timestamp_us: 0,
                    buffer,
                };
                source.capture_frame(&video_frame);

                self.frame_count += 1;
                if self.frame_count == 1 || self.frame_count % 450 == 0 {
                    let elapsed = self.started_at.elapsed().as_secs_f64();
                    let fps = if elapsed > 0.0 {
                        self.frame_count as f64 / elapsed
                    } else {
                        0.0
                    };
                    crate::clog_info!(
                        "📹 WgpuI420Publisher: {} frames published ({}×{}, {:.1} fps avg, GPU-converted)",
                        self.frame_count,
                        w,
                        h,
                        fps
                    );
                }

                Ok(true)
            }
            Err(TryRecvError::Empty) => Ok(false),
            Err(TryRecvError::Disconnected) => Err(PublishError::ChannelClosed),
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        crate::clog_info!(
            "📹 WgpuI420Publisher: resize {}×{} → {}×{}",
            self.width,
            self.height,
            width,
            height
        );
        self.width = width;
        self.height = height;
    }
}

/// Copy I420 planes from a flat byte buffer into LiveKit's I420Buffer.
/// Layout: [Y: w*h bytes] [U: cw*ch bytes] [V: cw*ch bytes]
fn copy_i420_planes(i420_data: &[u8], buffer: &mut I420Buffer, width: u32, height: u32) {
    let w = width as usize;
    let h = height as usize;
    let cw = (w + 1) / 2;
    let ch = (h + 1) / 2;

    let y_size = w * h;
    let uv_size = cw * ch;

    let (data_y, data_u, data_v) = buffer.data_mut();

    // Copy Y plane
    let y_end = y_size.min(i420_data.len()).min(data_y.len());
    data_y[..y_end].copy_from_slice(&i420_data[..y_end]);

    // Copy U plane
    let u_start = y_size;
    let u_end = (u_start + uv_size).min(i420_data.len());
    let u_copy = (u_end - u_start).min(data_u.len());
    if u_copy > 0 {
        data_u[..u_copy].copy_from_slice(&i420_data[u_start..u_start + u_copy]);
    }

    // Copy V plane
    let v_start = y_size + uv_size;
    let v_end = (v_start + uv_size).min(i420_data.len());
    let v_copy = (v_end - v_start).min(data_v.len());
    if v_copy > 0 {
        data_v[..v_copy].copy_from_slice(&i420_data[v_start..v_start + v_copy]);
    }
}
