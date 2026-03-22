//! Platform-specific FramePublisher implementations.
//!
//! Each publisher implements the FramePublisher trait from frame_publisher.rs.
//! The factory in frame_publisher.rs selects the best publisher at runtime.
//!
//! Publisher cascade (first success wins):
//!   macOS:     GpuBridgePublisher → NativeBufferPublisher → WgpuI420Publisher → CpuI420Publisher
//!   Windows:   WgpuI420Publisher → CpuI420Publisher
//!   Linux:     WgpuI420Publisher → CpuI420Publisher

#[cfg(target_os = "macos")]
pub mod native_buffer;

#[cfg(target_os = "macos")]
pub mod gpu_bridge;

/// Cross-platform stub: GPU bridge is macOS-only (Metal IOSurface).
/// On other platforms, always returns false — falls back to wgpu compute path.
#[cfg(not(target_os = "macos"))]
pub mod gpu_bridge {
    pub fn has_bridge<T>(_slot_id: T) -> bool { false }
}

/// Cross-platform GPU-accelerated I420 publisher via wgpu compute shader.
pub mod wgpu_i420;
