//! GpuBridgePublisher — macOS IOSurface zero-copy frame publisher.
//!
//! Pre-allocates double-buffered NV12 IOSurfaces per slot. The Bevy ReadbackComplete
//! observer writes RGBA→NV12 directly to the IOSurface (no allocation). The publisher
//! wraps the IOSurface in a lightweight CVPixelBuffer for LiveKit consumption.
//!
//! Eliminates per-frame:
//!   - `pixel_bytes.to_vec()` (1.2MB heap alloc) — Bevy writes directly to IOSurface
//!   - `CVPixelBufferCreate` (460KB kernel alloc) — reuses pre-allocated IOSurface
//!   - crossbeam channel — AtomicU64 frame counter signals new frames
//!
//! Architecture:
//!   IoSurfacePair (double-buffered, one per slot)
//!     ├── surface[0]: IOSurface-backed NV12 CVPixelBuffer (640×480)
//!     └── surface[1]: IOSurface-backed NV12 CVPixelBuffer (640×480)
//!
//!   Bevy thread:  write_frame(rgba) → lock masters[N%2] → RGBA→NV12 → unlock → N++
//!   Tokio thread: take_frame() → surface[(N-1)%2] → CVPixelBufferCreateWithIOSurface → publish
//!
//! Per-frame cost: CVPixelBufferCreateWithIOSurface (~40 bytes metadata wrapper)
//!   + Retain/Release pair (~2 atomic ops). Orders of magnitude cheaper than
//!   1.2MB heap alloc + 460KB kernel alloc.

#![cfg(target_os = "macos")]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use crossbeam_channel::{Receiver, TryRecvError};
use livekit::webrtc::video_frame::{VideoFrame, VideoRotation, native::NativeBuffer};
use livekit::webrtc::video_source::native::NativeVideoSource;

use crate::{clog_info, clog_warn};
use crate::voice::avatar::frame::RgbaFrame;
use super::super::frame_publisher::{FramePublisher, PublishError};
use super::native_buffer::rgba_to_nv12;

// =============================================================================
// CoreVideo FFI — extensions for IOSurface-backed CVPixelBuffers
// =============================================================================
//
// CVPixelBuffer* functions are in CoreVideo.framework (already linked via livekit/webrtc-sys).
// CF* functions are in CoreFoundation.framework (already linked by Apple framework deps).
// No additional framework linking needed.

type CVReturn = i32;
type CVPixelBufferRef = *mut std::ffi::c_void;
type CFAllocatorRef = *const std::ffi::c_void;
type CFDictionaryRef = *const std::ffi::c_void;
type CFStringRef = *const std::ffi::c_void;
type IOSurfaceRef = *mut std::ffi::c_void;
type OSType = u32;

const K_CV_RETURN_SUCCESS: CVReturn = 0;
const K_CV_PIXEL_FORMAT_TYPE_420V: OSType = 0x34323076;

extern "C" {
    // CVPixelBuffer — same symbols as native_buffer.rs (separate file = separate extern block)
    fn CVPixelBufferCreate(
        allocator: CFAllocatorRef,
        width: usize,
        height: usize,
        pixel_format_type: OSType,
        pixel_buffer_attributes: CFDictionaryRef,
        pixel_buffer_out: *mut CVPixelBufferRef,
    ) -> CVReturn;
    fn CVPixelBufferLockBaseAddress(pixel_buffer: CVPixelBufferRef, lock_flags: u64) -> CVReturn;
    fn CVPixelBufferUnlockBaseAddress(pixel_buffer: CVPixelBufferRef, lock_flags: u64) -> CVReturn;
    fn CVPixelBufferGetBaseAddressOfPlane(pixel_buffer: CVPixelBufferRef, plane_index: usize) -> *mut u8;
    fn CVPixelBufferGetBytesPerRowOfPlane(pixel_buffer: CVPixelBufferRef, plane_index: usize) -> usize;
    fn CVPixelBufferRelease(pixel_buffer: CVPixelBufferRef);
    fn CVPixelBufferRetain(pixel_buffer: CVPixelBufferRef) -> CVPixelBufferRef;

    // IOSurface-backed CVPixelBuffer extensions
    fn CVPixelBufferGetIOSurface(pixel_buffer: CVPixelBufferRef) -> IOSurfaceRef;
    fn CVPixelBufferCreateWithIOSurface(
        allocator: CFAllocatorRef,
        surface: IOSurfaceRef,
        pixel_buffer_attributes: CFDictionaryRef,
        pixel_buffer_out: *mut CVPixelBufferRef,
    ) -> CVReturn;

    // IOSurface properties key (in CoreVideo)
    static kCVPixelBufferIOSurfacePropertiesKey: CFStringRef;
}

// CoreFoundation FFI — minimal, for building the IOSurface properties attribute dict
extern "C" {
    fn CFDictionaryCreate(
        allocator: CFAllocatorRef,
        keys: *const *const std::ffi::c_void,
        values: *const *const std::ffi::c_void,
        num_values: isize,
        key_callbacks: *const std::ffi::c_void,
        value_callbacks: *const std::ffi::c_void,
    ) -> CFDictionaryRef;
    fn CFRelease(cf: *const std::ffi::c_void);

    static kCFTypeDictionaryKeyCallBacks: std::ffi::c_void;
    static kCFTypeDictionaryValueCallBacks: std::ffi::c_void;
}

// =============================================================================
// IoSurfacePair — double-buffered IOSurface-backed NV12 pixel storage
// =============================================================================

/// Double-buffered IOSurface-backed NV12 pixel storage for zero-copy frame publishing.
///
/// Memory: 2 × ~460KB × 1 pair per slot = ~920KB one-time allocation per slot.
/// Compare: without this, 195 frames/sec × (1.2MB + 460KB) = ~324 MB/s allocation.
///
/// Write path (Bevy thread):
///   `write_frame(rgba)` → lock masters[N%2] → RGBA→NV12 → unlock → N++
///
/// Read path (tokio thread):
///   `take_frame(last_seen)` → surfaces[(N-1)%2] → IOSurfaceRef for wrapper creation
pub struct IoSurfacePair {
    /// Master CVPixelBuffers that own the pixel memory (IOSurface-backed).
    /// Held alive for the entire slot lifetime — pixel memory never freed until Drop.
    masters: [CVPixelBufferRef; 2],
    /// Backing IOSurfaces extracted from masters.
    /// Passed to CVPixelBufferCreateWithIOSurface for lightweight wrapper creation.
    surfaces: [IOSurfaceRef; 2],
    /// NV12 Y plane stride (bytes per row, identical for both buffers).
    y_stride: usize,
    /// NV12 UV plane stride (bytes per row, identical for both buffers).
    uv_stride: usize,
    /// Frame counter — incremented after each write completes.
    /// Writer uses Release ordering, reader uses Acquire ordering.
    frame_counter: AtomicU64,
    width: u32,
    height: u32,
}

// SAFETY: CVPixelBuffer and IOSurface are refcounted macOS kernel objects, thread-safe.
// Double-buffering ensures writer (Bevy thread) and reader (tokio thread) never touch
// the same surface simultaneously: writer uses [counter%2], reader uses [(counter-1)%2].
unsafe impl Send for IoSurfacePair {}
unsafe impl Sync for IoSurfacePair {}

impl IoSurfacePair {
    /// Create a double-buffered IOSurface pair for NV12 frames.
    ///
    /// Allocates two IOSurface-backed CVPixelBuffers and verifies bi-planar access.
    fn new(width: u32, height: u32) -> Result<Self, String> {
        let mut masters = [std::ptr::null_mut(); 2];
        let mut surfaces = [std::ptr::null_mut(); 2];
        let mut y_stride = 0usize;
        let mut uv_stride = 0usize;

        for i in 0..2 {
            let (pb, ios) = create_iosurface_backed_nv12(width, height)
                .map_err(|e| {
                    // Clean up already-allocated buffers on failure
                    for master in masters.iter().take(i) {
                        unsafe { CVPixelBufferRelease(*master); }
                    }
                    e
                })?;

            masters[i] = pb;
            surfaces[i] = ios;

            // Query plane strides from first buffer (identical for both)
            if i == 0 {
                unsafe {
                    let lock = CVPixelBufferLockBaseAddress(pb, 0);
                    if lock != K_CV_RETURN_SUCCESS {
                        CVPixelBufferRelease(pb);
                        return Err(format!("Lock failed querying strides (CVReturn={})", lock));
                    }
                    y_stride = CVPixelBufferGetBytesPerRowOfPlane(pb, 0);
                    uv_stride = CVPixelBufferGetBytesPerRowOfPlane(pb, 1);
                    CVPixelBufferUnlockBaseAddress(pb, 0);
                }
            }
        }

        Ok(Self {
            masters,
            surfaces,
            y_stride,
            uv_stride,
            frame_counter: AtomicU64::new(0),
            width,
            height,
        })
    }

    /// Write RGBA pixel data as NV12 to the current write buffer.
    /// Called from Bevy's ReadbackComplete observer (render thread).
    ///
    /// Double-buffer protocol: writes to masters[counter % 2], then increments counter.
    /// The reader always reads from surfaces[(counter-1) % 2] — the completed frame.
    pub fn write_frame(&self, rgba: &[u8]) {
        let expected = (self.width as usize) * (self.height as usize) * 4;
        if rgba.len() != expected {
            return; // Size mismatch, skip silently (logged elsewhere by health check)
        }

        let idx = (self.frame_counter.load(Ordering::Relaxed) % 2) as usize;
        let pb = self.masters[idx];

        unsafe {
            let lock = CVPixelBufferLockBaseAddress(pb, 0);
            if lock != K_CV_RETURN_SUCCESS {
                return; // Can't lock, skip this frame
            }

            let y_base = CVPixelBufferGetBaseAddressOfPlane(pb, 0);
            let uv_base = CVPixelBufferGetBaseAddressOfPlane(pb, 1);

            if !y_base.is_null() && !uv_base.is_null() {
                rgba_to_nv12(
                    rgba,
                    self.width as usize,
                    self.height as usize,
                    y_base,
                    self.y_stride,
                    uv_base,
                    self.uv_stride,
                );
            }

            CVPixelBufferUnlockBaseAddress(pb, 0);
        }

        // Signal new frame available. Release ordering ensures NV12 writes
        // are visible to any thread that reads the counter with Acquire.
        self.frame_counter.fetch_add(1, Ordering::Release);
    }

    /// Take the most recently completed frame's IOSurface, if a new frame is available.
    /// Called from GpuBridgePublisher::try_publish (tokio thread).
    ///
    /// Returns (IOSurfaceRef, frame_counter) if a new frame exists since `last_seen`,
    /// or None if no new frame is available.
    fn take_frame(&self, last_seen: u64) -> Option<(IOSurfaceRef, u64)> {
        let current = self.frame_counter.load(Ordering::Acquire);
        if current == 0 || current <= last_seen {
            return None;
        }
        // Reader takes the previously-completed buffer: (current-1) % 2
        // This is never the buffer currently being written to.
        let idx = ((current - 1) % 2) as usize;
        Some((self.surfaces[idx], current))
    }
}

impl Drop for IoSurfacePair {
    fn drop(&mut self) {
        unsafe {
            CVPixelBufferRelease(self.masters[0]);
            CVPixelBufferRelease(self.masters[1]);
            // IOSurfaces are owned by the CVPixelBuffers — freed automatically.
        }
    }
}

/// Create an IOSurface-backed NV12 CVPixelBuffer.
///
/// The `kCVPixelBufferIOSurfacePropertiesKey` attribute tells CoreVideo to allocate
/// pixel memory as an IOSurface (kernel-level shared memory, suitable for cross-thread
/// and cross-process access without copying).
///
/// Returns (CVPixelBufferRef, IOSurfaceRef) where the IOSurface is owned by the CVPixelBuffer.
fn create_iosurface_backed_nv12(width: u32, height: u32) -> Result<(CVPixelBufferRef, IOSurfaceRef), String> {
    unsafe {
        // Empty dict for IOSurface properties (default = system chooses backing store)
        let empty_dict = CFDictionaryCreate(
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            &kCFTypeDictionaryKeyCallBacks as *const _ as *const std::ffi::c_void,
            &kCFTypeDictionaryValueCallBacks as *const _ as *const std::ffi::c_void,
        );

        // Attributes dict: { kCVPixelBufferIOSurfacePropertiesKey: {} }
        let keys = [kCVPixelBufferIOSurfacePropertiesKey as *const std::ffi::c_void];
        let values = [empty_dict as *const std::ffi::c_void];
        let attrs = CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks as *const _ as *const std::ffi::c_void,
            &kCFTypeDictionaryValueCallBacks as *const _ as *const std::ffi::c_void,
        );
        CFRelease(empty_dict);

        // Create NV12 CVPixelBuffer with IOSurface backing
        let mut pb: CVPixelBufferRef = std::ptr::null_mut();
        let result = CVPixelBufferCreate(
            std::ptr::null(),
            width as usize,
            height as usize,
            K_CV_PIXEL_FORMAT_TYPE_420V,
            attrs,
            &mut pb,
        );
        CFRelease(attrs);

        if result != K_CV_RETURN_SUCCESS || pb.is_null() {
            return Err(format!(
                "CVPixelBufferCreate IOSurface-backed NV12 failed (CVReturn={})",
                result
            ));
        }

        // Extract backing IOSurface
        let surface = CVPixelBufferGetIOSurface(pb);
        if surface.is_null() {
            CVPixelBufferRelease(pb);
            return Err("CVPixelBuffer has no IOSurface backing".into());
        }

        // Verify bi-planar access works
        let lock = CVPixelBufferLockBaseAddress(pb, 0);
        if lock != K_CV_RETURN_SUCCESS {
            CVPixelBufferRelease(pb);
            return Err(format!("Lock failed verifying planes (CVReturn={})", lock));
        }
        let y_base = CVPixelBufferGetBaseAddressOfPlane(pb, 0);
        let uv_base = CVPixelBufferGetBaseAddressOfPlane(pb, 1);
        CVPixelBufferUnlockBaseAddress(pb, 0);

        if y_base.is_null() || uv_base.is_null() {
            CVPixelBufferRelease(pb);
            return Err("IOSurface-backed CVPixelBuffer NV12 plane access failed".into());
        }

        Ok((pb, surface))
    }
}

// =============================================================================
// Global GPU bridge registry
// =============================================================================
//
// Accessed from two threads:
//   - Bevy render thread (read): try_write_bridge() in ReadbackComplete observer
//   - Tokio task (write): register_bridge() / unregister_bridge() from GpuBridgePublisher
//
// RwLock allows concurrent reads (multiple Bevy observers) with exclusive writes.

static GPU_BRIDGES: std::sync::OnceLock<std::sync::RwLock<Vec<Option<Arc<IoSurfacePair>>>>> =
    std::sync::OnceLock::new();

fn bridges() -> &'static std::sync::RwLock<Vec<Option<Arc<IoSurfacePair>>>> {
    GPU_BRIDGES.get_or_init(|| {
        let slots = vec![None; crate::voice::bevy_renderer::MAX_AVATAR_SLOTS as usize];
        std::sync::RwLock::new(slots)
    })
}

fn register_bridge(slot: u8, pair: Arc<IoSurfacePair>) {
    let mut guard = bridges().write().unwrap();
    if let Some(entry) = guard.get_mut(slot as usize) {
        *entry = Some(pair);
        clog_info!("📹 GPU bridge registered for slot {}", slot);
    }
}

fn unregister_bridge(slot: u8) {
    let mut guard = bridges().write().unwrap();
    if let Some(entry) = guard.get_mut(slot as usize) {
        if entry.take().is_some() {
            clog_info!("📹 GPU bridge unregistered for slot {}", slot);
        }
    }
}

/// Called from ReadbackComplete observer in bevy_renderer.rs.
/// Writes RGBA→NV12 directly to the slot's pre-allocated IOSurface.
/// Returns true if frame was written (caller should skip channel path).
pub fn try_write_bridge(slot_id: u8, pixel_bytes: &[u8]) -> bool {
    if let Some(lock) = GPU_BRIDGES.get() {
        if let Ok(guard) = lock.read() {
            if let Some(Some(pair)) = guard.get(slot_id as usize) {
                pair.write_frame(pixel_bytes);
                return true;
            }
        }
    }
    false
}

// =============================================================================
// GpuBridgePublisher — zero-copy FramePublisher using IOSurface
// =============================================================================

/// macOS zero-copy frame publisher using pre-allocated IOSurface-backed CVPixelBuffers.
///
/// Each try_publish call:
///   1. Checks AtomicU64 frame counter for new frame (no channel overhead)
///   2. CVPixelBufferCreateWithIOSurface → ~40 bytes metadata wrapper (no pixel alloc)
///   3. Retain → NativeBuffer → capture_frame → Release
///
/// Total per-frame cost: ~2 atomic ops + ~40 byte alloc + function call overhead.
/// Compare: NativeBufferPublisher = 1.2MB to_vec + 460KB kernel CVPixelBufferCreate.
pub struct GpuBridgePublisher {
    pair: Arc<IoSurfacePair>,
    /// Last published frame counter (for new-frame detection).
    last_published: u64,
    /// Total frames published (for periodic logging).
    frame_count: u64,
    /// Channel receiver — NOT used for frames. Only for liveness detection
    /// (Disconnected = Bevy renderer died, all Senders dropped).
    liveness_rx: Receiver<RgbaFrame>,
    /// Slot number (for unregistering bridge on drop).
    slot: u8,
    width: u32,
    height: u32,
}

impl GpuBridgePublisher {
    /// Try to create a GpuBridgePublisher for the given slot.
    ///
    /// Creates an IoSurfacePair (double-buffered IOSurface-backed NV12 buffers)
    /// and registers it with the global bridge registry so the Bevy ReadbackComplete
    /// observer can write frames directly to the IOSurface.
    pub fn try_new(
        liveness_rx: Receiver<RgbaFrame>,
        width: u32,
        height: u32,
        slot: u8,
    ) -> Result<Self, String> {
        let pair = IoSurfacePair::new(width, height)?;
        let pair = Arc::new(pair);

        // Register with global bridge so Bevy observer writes directly to IOSurface
        register_bridge(slot, pair.clone());

        clog_info!(
            "📹 GpuBridgePublisher: IOSurface pair created for slot {} ({}×{}, Y stride={}, UV stride={})",
            slot, width, height, pair.y_stride, pair.uv_stride
        );

        Ok(Self {
            pair,
            last_published: 0,
            frame_count: 0,
            liveness_rx,
            slot,
            width,
            height,
        })
    }
}

impl Drop for GpuBridgePublisher {
    fn drop(&mut self) {
        unregister_bridge(self.slot);
    }
}

impl FramePublisher for GpuBridgePublisher {
    fn name(&self) -> &'static str { "gpu-bridge" }

    fn try_publish(&mut self, source: &NativeVideoSource) -> Result<bool, PublishError> {
        // Check for new frame via atomic counter (no channel overhead)
        let (surface, counter) = match self.pair.take_frame(self.last_published) {
            Some(result) => result,
            None => {
                // No new frame — check channel liveness (Senders alive = Bevy alive)
                match self.liveness_rx.try_recv() {
                    Err(TryRecvError::Disconnected) => return Err(PublishError::ChannelClosed),
                    _ => return Ok(false),
                }
            }
        };
        self.last_published = counter;

        // Create lightweight CVPixelBuffer wrapper around the IOSurface.
        // This allocates ~40 bytes of metadata, NOT pixel memory (~460KB savings per frame).
        let mut cv_pb: CVPixelBufferRef = std::ptr::null_mut();
        let result = unsafe {
            CVPixelBufferCreateWithIOSurface(
                std::ptr::null(),
                surface,
                std::ptr::null(),
                &mut cv_pb,
            )
        };
        if result != K_CV_RETURN_SUCCESS || cv_pb.is_null() {
            if self.frame_count == 0 {
                clog_warn!(
                    "📹 GpuBridgePublisher: CVPixelBufferCreateWithIOSurface failed (CVReturn={})",
                    result
                );
            }
            return Ok(false);
        }

        unsafe {
            // Retain before wrapping in NativeBuffer (same lifecycle as NativeBufferPublisher).
            // from_cv_pixel_buffer does NOT bump refcount; C++ destructor WILL release.
            CVPixelBufferRetain(cv_pb);

            let native_buffer = NativeBuffer::from_cv_pixel_buffer(cv_pb);
            let video_frame = VideoFrame {
                rotation: VideoRotation::VideoRotation0,
                timestamp_us: 0,
                buffer: native_buffer,
            };
            source.capture_frame(&video_frame);

            // Release our ownership from CVPixelBufferCreateWithIOSurface.
            // C++ NativeBuffer holds the retain'd reference.
            CVPixelBufferRelease(cv_pb);
        }

        self.frame_count += 1;

        // Periodic health log: every 450 frames (~30s at 15fps)
        if self.frame_count == 1 || self.frame_count % 450 == 0 {
            clog_info!(
                "📹 GpuBridgePublisher: {} frames published (slot={}, {}×{})",
                self.frame_count, self.slot, self.width, self.height
            );
        }

        Ok(true)
    }
}
