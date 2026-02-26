//! NativeBufferPublisher — macOS CVPixelBuffer frame publisher.
//!
//! Skips I420 conversion entirely by writing BGRA directly into a CVPixelBuffer,
//! which LiveKit's NativeBuffer wraps for zero-copy GPU path.
//!
//! RGBA→BGRA swizzle is a simple byte swap (~0.3ms for VGA) vs I420 conversion
//! which requires BT.601 color space math (~1ms for VGA). At 15 agents × 15fps
//! that saves ~160ms/s of CPU work.
//!
//! CVPixelBuffer lifecycle:
//!   1. Allocate per-frame via CVPixelBufferCreate (kCVPixelFormatType_32BGRA)
//!   2. Lock base address → write BGRA data → unlock
//!   3. Retain (bump refcount to 2)
//!   4. Wrap in NativeBuffer::from_cv_pixel_buffer (doesn't bump refcount)
//!   5. VideoFrame { buffer: native_buffer } → capture_frame
//!   6. When VideoFrame drops, C++ releases the CVPixelBuffer (refcount → 1)
//!   7. We CVPixelBufferRelease our retain (refcount → 0, freed)
//!
//! This is safe because:
//!   - CVPixelBuffer is refcounted (CoreFoundation semantics)
//!   - We retain before wrapping, so the buffer survives until we release

#![cfg(target_os = "macos")]

use crossbeam_channel::{Receiver, TryRecvError};
use livekit::webrtc::video_frame::{VideoFrame, VideoRotation, native::NativeBuffer};
use livekit::webrtc::video_source::native::NativeVideoSource;

use crate::{clog_info, clog_warn};
use crate::voice::avatar::frame::RgbaFrame;
use super::super::frame_publisher::{FramePublisher, PublishError};

// =============================================================================
// CoreVideo FFI — minimal bindings for CVPixelBuffer
// =============================================================================

type CVReturn = i32;
type CVPixelBufferRef = *mut std::ffi::c_void;
type CFAllocatorRef = *const std::ffi::c_void;
type CFDictionaryRef = *const std::ffi::c_void;
type OSType = u32;

const K_CV_RETURN_SUCCESS: CVReturn = 0;

/// kCVPixelFormatType_32BGRA = 'BGRA' = 0x42475241
const K_CV_PIXEL_FORMAT_TYPE_32BGRA: OSType = 0x42475241;

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

    fn CVPixelBufferGetBaseAddress(pixel_buffer: CVPixelBufferRef) -> *mut u8;

    fn CVPixelBufferGetBytesPerRow(pixel_buffer: CVPixelBufferRef) -> usize;

    fn CVPixelBufferRelease(pixel_buffer: CVPixelBufferRef);

    fn CVPixelBufferRetain(pixel_buffer: CVPixelBufferRef) -> CVPixelBufferRef;
}

// =============================================================================
// NativeBufferPublisher
// =============================================================================

/// macOS-specific frame publisher: RGBA → BGRA → CVPixelBuffer → NativeBuffer → LiveKit.
///
/// Skips I420 conversion entirely. Each frame allocates a CVPixelBuffer (managed by
/// CoreFoundation refcounting) and writes BGRA data directly.
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
    /// Try to create a NativeBufferPublisher. Returns Err if CVPixelBuffer creation
    /// fails (e.g. CoreVideo unavailable, out of memory).
    pub fn try_new(
        frame_rx: Receiver<RgbaFrame>,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        // Smoke test: create and immediately release a CVPixelBuffer
        // to verify CoreVideo is functional.
        let mut test_pb: CVPixelBufferRef = std::ptr::null_mut();
        let result = unsafe {
            CVPixelBufferCreate(
                std::ptr::null(),
                width as usize,
                height as usize,
                K_CV_PIXEL_FORMAT_TYPE_32BGRA,
                std::ptr::null(),
                &mut test_pb,
            )
        };
        if result != K_CV_RETURN_SUCCESS || test_pb.is_null() {
            return Err(format!(
                "CVPixelBufferCreate smoke test failed (CVReturn={}, null={})",
                result, test_pb.is_null()
            ));
        }
        unsafe { CVPixelBufferRelease(test_pb); }

        clog_info!(
            "📹 NativeBufferPublisher: CVPixelBuffer smoke test passed ({}×{} BGRA)",
            width, height
        );

        Ok(Self { frame_rx, width, height, frame_count: 0, alloc_failures: 0 })
    }
}

impl FramePublisher for NativeBufferPublisher {
    fn name(&self) -> &'static str { "native-buffer" }

    fn try_publish(&mut self, source: &NativeVideoSource) -> Result<bool, PublishError> {
        let frame = match self.frame_rx.try_recv() {
            Ok(f) => f,
            Err(TryRecvError::Empty) => return Ok(false),
            Err(TryRecvError::Disconnected) => return Err(PublishError::ChannelClosed),
        };

        // Validate frame data size
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

        // Allocate a CVPixelBuffer for this frame
        let mut cv_pb: CVPixelBufferRef = std::ptr::null_mut();
        let result = unsafe {
            CVPixelBufferCreate(
                std::ptr::null(),
                self.width as usize,
                self.height as usize,
                K_CV_PIXEL_FORMAT_TYPE_32BGRA,
                std::ptr::null(),
                &mut cv_pb,
            )
        };
        if result != K_CV_RETURN_SUCCESS || cv_pb.is_null() {
            self.alloc_failures += 1;
            if self.alloc_failures == 1 || self.alloc_failures % 100 == 0 {
                clog_warn!(
                    "📹 NativeBufferPublisher: CVPixelBufferCreate failed (CVReturn={}, failures={})",
                    result, self.alloc_failures
                );
            }
            return Ok(false);
        }

        // Lock, write BGRA data, unlock
        unsafe {
            let lock_result = CVPixelBufferLockBaseAddress(cv_pb, 0);
            if lock_result != K_CV_RETURN_SUCCESS {
                clog_warn!("📹 NativeBufferPublisher: CVPixelBufferLockBaseAddress failed (CVReturn={})", lock_result);
                CVPixelBufferRelease(cv_pb);
                return Ok(false);
            }

            let base_addr = CVPixelBufferGetBaseAddress(cv_pb);
            if base_addr.is_null() {
                clog_warn!("📹 NativeBufferPublisher: CVPixelBufferGetBaseAddress returned null");
                CVPixelBufferUnlockBaseAddress(cv_pb, 0);
                CVPixelBufferRelease(cv_pb);
                return Ok(false);
            }

            let bytes_per_row = CVPixelBufferGetBytesPerRow(cv_pb);
            let w = self.width as usize;
            let h = self.height as usize;

            // Log stride info on first frame
            if self.frame_count == 0 {
                clog_info!(
                    "📹 NativeBufferPublisher: first frame — {}×{}, stride={} (contiguous={})",
                    w, h, bytes_per_row, bytes_per_row == w * 4
                );
            }

            // RGBA → BGRA swizzle (swap R and B channels)
            if bytes_per_row == w * 4 {
                // Contiguous — process all pixels in one pass
                let total_pixels = w * h;
                let src = frame.data.as_ptr();
                let dst = base_addr;
                for i in 0..total_pixels {
                    let off = i * 4;
                    // RGBA → BGRA: swap R(0) ↔ B(2), keep G(1) and A(3)
                    *dst.add(off) = *src.add(off + 2);     // B
                    *dst.add(off + 1) = *src.add(off + 1); // G
                    *dst.add(off + 2) = *src.add(off);     // R
                    *dst.add(off + 3) = *src.add(off + 3); // A
                }
            } else {
                // Padded rows — copy row by row respecting stride
                for y in 0..h {
                    let src_row = &frame.data[y * w * 4..];
                    let dst_row = base_addr.add(y * bytes_per_row);
                    for x in 0..w {
                        let off = x * 4;
                        *dst_row.add(off) = src_row[off + 2];     // B
                        *dst_row.add(off + 1) = src_row[off + 1]; // G
                        *dst_row.add(off + 2) = src_row[off];     // R
                        *dst_row.add(off + 3) = src_row[off + 3]; // A
                    }
                }
            }

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
                "📹 NativeBufferPublisher: {} frames published (alloc_failures={})",
                self.frame_count, self.alloc_failures
            );
        }

        Ok(true)
    }
}
