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

                // Log strides on first frame to diagnose green bars
                if self.frame_count == 0 {
                    let (sy, su, sv) = buffer.strides();
                    let cw = (w + 1) / 2;
                    crate::clog_info!(
                        "📹 WgpuI420Publisher: I420Buffer strides: Y={} (w={}), U={} V={} (cw={}), data_len={}",
                        sy, w, su, sv, cw, frame.data.len()
                    );
                }

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
/// Must respect I420Buffer strides — the buffer may have padded rows.
fn copy_i420_planes(i420_data: &[u8], buffer: &mut I420Buffer, width: u32, height: u32) {
    let w = width as usize;
    let h = height as usize;
    let cw = (w + 1) / 2;
    let ch = (h + 1) / 2;

    let src_y_size = w * h;
    let src_uv_size = cw * ch;

    let (stride_y, stride_u, stride_v) = buffer.strides();
    let stride_y = stride_y as usize;
    let stride_u = stride_u as usize;
    let stride_v = stride_v as usize;

    let (data_y, data_u, data_v) = buffer.data_mut();

    // Copy Y plane (row by row to handle stride != width)
    if stride_y == w {
        // Fast path: no padding
        let end = src_y_size.min(i420_data.len()).min(data_y.len());
        data_y[..end].copy_from_slice(&i420_data[..end]);
    } else {
        for row in 0..h {
            let src_off = row * w;
            let dst_off = row * stride_y;
            let copy_len = w.min(i420_data.len().saturating_sub(src_off)).min(data_y.len().saturating_sub(dst_off));
            if copy_len > 0 {
                data_y[dst_off..dst_off + copy_len].copy_from_slice(&i420_data[src_off..src_off + copy_len]);
            }
        }
    }

    // Copy U plane
    let u_src_start = src_y_size;
    if stride_u == cw {
        let u_end = src_uv_size.min(i420_data.len().saturating_sub(u_src_start)).min(data_u.len());
        if u_end > 0 {
            data_u[..u_end].copy_from_slice(&i420_data[u_src_start..u_src_start + u_end]);
        }
    } else {
        for row in 0..ch {
            let src_off = u_src_start + row * cw;
            let dst_off = row * stride_u;
            let copy_len = cw.min(i420_data.len().saturating_sub(src_off)).min(data_u.len().saturating_sub(dst_off));
            if copy_len > 0 {
                data_u[dst_off..dst_off + copy_len].copy_from_slice(&i420_data[src_off..src_off + copy_len]);
            }
        }
    }

    // Copy V plane
    let v_src_start = src_y_size + src_uv_size;
    if stride_v == cw {
        let v_end = src_uv_size.min(i420_data.len().saturating_sub(v_src_start)).min(data_v.len());
        if v_end > 0 {
            data_v[..v_end].copy_from_slice(&i420_data[v_src_start..v_src_start + v_end]);
        }
    } else {
        for row in 0..ch {
            let src_off = v_src_start + row * cw;
            let dst_off = row * stride_v;
            let copy_len = cw.min(i420_data.len().saturating_sub(src_off)).min(data_v.len().saturating_sub(dst_off));
            if copy_len > 0 {
                data_v[dst_off..dst_off + copy_len].copy_from_slice(&i420_data[src_off..src_off + copy_len]);
            }
        }
    }
}
