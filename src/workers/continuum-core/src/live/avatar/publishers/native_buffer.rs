//! NativeBufferPublisher — macOS NV12 CVPixelBuffer frame publisher.
//!
//! Publishes frames as NV12 CVPixelBuffers via LiveKit's NativeBuffer,
//! bypassing I420Buffer allocation entirely. NV12 (kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange)
//! is VideoToolbox's native input format — the hardware encoder processes it directly
//! without any pixel format conversion.
//!
//! RGBA → NV12 conversion uses the same BT.601 math as CpuI420Publisher's I420 path,
//! but writes interleaved UV instead of separate U/V planes. The conversion cost is
//! comparable (~1ms for VGA), but the encoder-side savings (no NV12→NV12 passthrough
//! vs I420→NV12 conversion) improve overall latency.
//!
//! Previous attempt used BGRA format (kCVPixelFormatType_32BGRA) which produced dark
//! frames — WebRTC's VideoToolbox encoder path expects YUV input, not RGB.
//!
//! CVPixelBuffer lifecycle:
//!   1. Allocate per-frame via CVPixelBufferCreate (kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange)
//!   2. Lock base address → write NV12 Y and UV planes → unlock
//!   3. Retain (bump refcount to 2)
//!   4. Wrap in NativeBuffer::from_cv_pixel_buffer (doesn't bump refcount)
//!   5. VideoFrame { buffer: native_buffer } → capture_frame
//!   6. When VideoFrame drops, C++ releases the CVPixelBuffer (refcount → 1)
//!   7. We CVPixelBufferRelease our retain (refcount → 0, freed)

#![cfg(target_os = "macos")]

use crossbeam_channel::{Receiver, TryRecvError};
use livekit::webrtc::video_frame::{VideoFrame, VideoRotation, native::NativeBuffer};
use livekit::webrtc::video_source::native::NativeVideoSource;

use crate::{clog_info, clog_warn};
use crate::live::avatar::frame::RgbaFrame;
use super::super::frame_publisher::{FramePublisher, PublishError};

// =============================================================================
// CoreVideo FFI — minimal bindings for CVPixelBuffer (NV12 bi-planar)
// =============================================================================

type CVReturn = i32;
type CVPixelBufferRef = *mut std::ffi::c_void;
type CFAllocatorRef = *const std::ffi::c_void;
type CFDictionaryRef = *const std::ffi::c_void;
type OSType = u32;

const K_CV_RETURN_SUCCESS: CVReturn = 0;

/// kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange = '420v' = 0x34323076
/// VideoToolbox's native H.264 encoder input format. No pixel format conversion needed.
const K_CV_PIXEL_FORMAT_TYPE_420V: OSType = 0x34323076;

extern "C" {
    fn CVPixelBufferCreate(
        allocator: CFAllocatorRef,
        width: usize,
        height: usize,
        pixel_format_type: OSType,
        pixel_buffer_attributes: CFDictionaryRef,
        pixel_buffer_out: *mut CVPixelBufferRef,
    ) -> CVReturn;

    fn CVPixelBufferLockBaseAddress(
        pixel_buffer: CVPixelBufferRef,
        lock_flags: u64,
    ) -> CVReturn;

    fn CVPixelBufferUnlockBaseAddress(
        pixel_buffer: CVPixelBufferRef,
        lock_flags: u64,
    ) -> CVReturn;

    fn CVPixelBufferGetBaseAddressOfPlane(
        pixel_buffer: CVPixelBufferRef,
        plane_index: usize,
    ) -> *mut u8;

    fn CVPixelBufferGetBytesPerRowOfPlane(
        pixel_buffer: CVPixelBufferRef,
        plane_index: usize,
    ) -> usize;

    fn CVPixelBufferRelease(pixel_buffer: CVPixelBufferRef);

    fn CVPixelBufferRetain(pixel_buffer: CVPixelBufferRef) -> CVPixelBufferRef;
}

// =============================================================================
// NativeBufferPublisher — NV12 CVPixelBuffer
// =============================================================================

/// macOS-specific frame publisher: RGBA → NV12 → CVPixelBuffer → NativeBuffer → LiveKit.
///
/// Uses NV12 (bi-planar YUV 4:2:0, video range) which is VideoToolbox's native format.
/// Each frame allocates a CVPixelBuffer (managed by CoreFoundation refcounting)
/// and writes NV12 Y + UV plane data.
pub struct NativeBufferPublisher {
    frame_rx: Receiver<RgbaFrame>,
    width: u32,
    height: u32,
    /// Frame counter for periodic logging.
    frame_count: u64,
    /// Count of CVPixelBuffer allocation failures (should stay at 0).
    alloc_failures: u64,
}

impl NativeBufferPublisher {
    /// Try to create a NativeBufferPublisher. Returns Err if NV12 CVPixelBuffer creation
    /// fails (e.g. CoreVideo unavailable, out of memory).
    pub fn try_new(
        frame_rx: Receiver<RgbaFrame>,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        // Smoke test: create and immediately release an NV12 CVPixelBuffer
        // to verify CoreVideo supports this format.
        let mut test_pb: CVPixelBufferRef = std::ptr::null_mut();
        let result = unsafe {
            CVPixelBufferCreate(
                std::ptr::null(),
                width as usize,
                height as usize,
                K_CV_PIXEL_FORMAT_TYPE_420V,
                std::ptr::null(),
                &mut test_pb,
            )
        };
        if result != K_CV_RETURN_SUCCESS || test_pb.is_null() {
            return Err(format!(
                "CVPixelBufferCreate NV12 smoke test failed (CVReturn={}, null={})",
                result, test_pb.is_null()
            ));
        }

        // Verify bi-planar access works
        unsafe {
            let lock = CVPixelBufferLockBaseAddress(test_pb, 0);
            if lock != K_CV_RETURN_SUCCESS {
                CVPixelBufferRelease(test_pb);
                return Err(format!("CVPixelBufferLockBaseAddress failed (CVReturn={})", lock));
            }
            let y_base = CVPixelBufferGetBaseAddressOfPlane(test_pb, 0);
            let uv_base = CVPixelBufferGetBaseAddressOfPlane(test_pb, 1);
            let y_stride = CVPixelBufferGetBytesPerRowOfPlane(test_pb, 0);
            let uv_stride = CVPixelBufferGetBytesPerRowOfPlane(test_pb, 1);
            CVPixelBufferUnlockBaseAddress(test_pb, 0);

            if y_base.is_null() || uv_base.is_null() {
                CVPixelBufferRelease(test_pb);
                return Err("CVPixelBuffer NV12 plane base address is null".into());
            }

            clog_info!(
                "📹 NativeBufferPublisher: NV12 smoke test passed ({}×{}, Y stride={}, UV stride={})",
                width, height, y_stride, uv_stride
            );
        }

        unsafe { CVPixelBufferRelease(test_pb); }

        Ok(Self { frame_rx, width, height, frame_count: 0, alloc_failures: 0 })
    }
}

impl FramePublisher for NativeBufferPublisher {
    fn name(&self) -> &'static str { "native-nv12" }

    fn try_publish(&mut self, source: &NativeVideoSource) -> Result<bool, PublishError> {
        let frame = match self.frame_rx.try_recv() {
            Ok(f) => f,
            Err(TryRecvError::Empty) => return Ok(false),
            Err(TryRecvError::Disconnected) => return Err(PublishError::ChannelClosed),
        };

        // Validate frame data size (RGBA = 4 bytes per pixel)
        let expected_size = (self.width * self.height * 4) as usize;
        if frame.data.len() != expected_size {
            if self.frame_count == 0 {
                clog_warn!(
                    "📹 NativeBufferPublisher: frame data size mismatch (got {} bytes, expected {} for {}×{})",
                    frame.data.len(), expected_size, self.width, self.height
                );
            }
            return Ok(false);
        }

        // Allocate an NV12 CVPixelBuffer for this frame
        let mut cv_pb: CVPixelBufferRef = std::ptr::null_mut();
        let result = unsafe {
            CVPixelBufferCreate(
                std::ptr::null(),
                self.width as usize,
                self.height as usize,
                K_CV_PIXEL_FORMAT_TYPE_420V,
                std::ptr::null(),
                &mut cv_pb,
            )
        };
        if result != K_CV_RETURN_SUCCESS || cv_pb.is_null() {
            self.alloc_failures += 1;
            if self.alloc_failures == 1 || self.alloc_failures % 100 == 0 {
                clog_warn!(
                    "📹 NativeBufferPublisher: CVPixelBufferCreate NV12 failed (CVReturn={}, failures={})",
                    result, self.alloc_failures
                );
            }
            return Ok(false);
        }

        // Lock, write NV12 data, unlock
        unsafe {
            let lock_result = CVPixelBufferLockBaseAddress(cv_pb, 0);
            if lock_result != K_CV_RETURN_SUCCESS {
                clog_warn!("📹 NativeBufferPublisher: CVPixelBufferLockBaseAddress failed (CVReturn={})", lock_result);
                CVPixelBufferRelease(cv_pb);
                return Ok(false);
            }

            // Y plane (plane 0): one luma byte per pixel
            let y_base = CVPixelBufferGetBaseAddressOfPlane(cv_pb, 0);
            let y_stride = CVPixelBufferGetBytesPerRowOfPlane(cv_pb, 0);

            // UV plane (plane 1): interleaved Cb/Cr, one pair per 2×2 block
            let uv_base = CVPixelBufferGetBaseAddressOfPlane(cv_pb, 1);
            let uv_stride = CVPixelBufferGetBytesPerRowOfPlane(cv_pb, 1);

            if y_base.is_null() || uv_base.is_null() {
                clog_warn!("📹 NativeBufferPublisher: NV12 plane base address null");
                CVPixelBufferUnlockBaseAddress(cv_pb, 0);
                CVPixelBufferRelease(cv_pb);
                return Ok(false);
            }

            // Log stride info on first frame
            if self.frame_count == 0 {
                clog_info!(
                    "📹 NativeBufferPublisher NV12: first frame — {}×{}, Y stride={}, UV stride={}",
                    self.width, self.height, y_stride, uv_stride
                );
            }

            // RGBA → NV12 conversion (BT.601 video range)
            rgba_to_nv12(
                &frame.data,
                self.width as usize,
                self.height as usize,
                y_base,
                y_stride,
                uv_base,
                uv_stride,
            );

            CVPixelBufferUnlockBaseAddress(cv_pb, 0);

            // Retain the CVPixelBuffer before wrapping in NativeBuffer.
            // from_cv_pixel_buffer does NOT bump the refcount, but the C++ destructor
            // WILL release it when VideoFrame drops. Retain → wrap → C++ releases → balanced.
            CVPixelBufferRetain(cv_pb);

            let native_buffer = NativeBuffer::from_cv_pixel_buffer(cv_pb);
            let video_frame = VideoFrame {
                rotation: VideoRotation::VideoRotation0,
                timestamp_us: 0,
                buffer: native_buffer,
            };
            source.capture_frame(&video_frame);

            // Release our original ownership (from CVPixelBufferCreate).
            // The C++ NativeBuffer holds the retain'd reference.
            CVPixelBufferRelease(cv_pb);
        }

        self.frame_count += 1;

        // Periodic health log: every 450 frames (~30s at 15fps)
        if self.frame_count == 1 || self.frame_count % 450 == 0 {
            clog_info!(
                "📹 NativeBufferPublisher NV12: {} frames published (alloc_failures={})",
                self.frame_count, self.alloc_failures
            );
        }

        Ok(true)
    }
}

// =============================================================================
// RGBA → NV12 conversion (BT.601 video range)
// =============================================================================

/// Convert RGBA8 pixel data into NV12 bi-planar format in pre-locked CVPixelBuffer planes.
///
/// Same BT.601 math as `rgba_to_i420_into` in frame_publisher.rs, but UV is interleaved
/// into a single plane (NV12 format) instead of separate U/V planes (I420 format).
///
/// NV12 layout:
///   - Y plane: width × height bytes (one luma per pixel, stride may be padded)
///   - UV plane: width × (height/2) bytes (interleaved U,V pairs per 2×2 block)
///
/// # Safety
///
/// Caller must ensure:
///   - `y_base` and `uv_base` point to locked CVPixelBuffer plane memory
///   - Y plane has at least `height` rows of `y_stride` bytes each
///   - UV plane has at least `height/2` rows of `uv_stride` bytes each
///   - `rgba` has exactly `width * height * 4` bytes
pub(crate) unsafe fn rgba_to_nv12(
    rgba: &[u8],
    width: usize,
    height: usize,
    y_base: *mut u8,
    y_stride: usize,
    uv_base: *mut u8,
    uv_stride: usize,
) {
    // Y plane: BT.601 luma, one value per pixel
    for row in 0..height {
        let y_row = y_base.add(row * y_stride);
        for col in 0..width {
            let i = (row * width + col) * 4;
            let r = *rgba.get_unchecked(i) as i32;
            let g = *rgba.get_unchecked(i + 1) as i32;
            let b = *rgba.get_unchecked(i + 2) as i32;
            // BT.601: Y = (66R + 129G + 25B + 128) >> 8 + 16
            *y_row.add(col) = (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(0, 255) as u8;
        }
    }

    // UV plane: interleaved Cb/Cr, subsampled 2×2
    let ch = (height + 1) / 2;
    let cw = (width + 1) / 2;
    for cy in 0..ch {
        let uv_row = uv_base.add(cy * uv_stride);
        let py = cy * 2;
        for cx in 0..cw {
            let px = cx * 2;
            let i = (py * width + px) * 4;
            let r = *rgba.get_unchecked(i) as i32;
            let g = *rgba.get_unchecked(i + 1) as i32;
            let b = *rgba.get_unchecked(i + 2) as i32;
            // BT.601: U = (-38R - 74G + 112B + 128) >> 8 + 128
            *uv_row.add(cx * 2) = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
            // BT.601: V = (112R - 94G - 18B + 128) >> 8 + 128
            *uv_row.add(cx * 2 + 1) = (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
        }
    }
}
