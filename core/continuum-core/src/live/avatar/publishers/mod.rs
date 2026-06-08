//! Platform-specific FramePublisher implementations.
//!
//! Each publisher implements the FramePublisher trait from frame_publisher.rs.
//! The factory in frame_publisher.rs selects the best publisher at runtime.
//!
//! Publisher cascade (first success wins):
//!   macOS:     GpuBridgePublisher → NativeBufferPublisher → WgpuI420Publisher → CpuI420Publisher
//!   Windows:   WgpuI420Publisher → CpuI420Publisher
//!   Linux:     WgpuI420Publisher → CpuI420Publisher

#[cfg(all(feature = "livekit-webrtc", target_os = "macos"))]
pub mod native_buffer;

#[cfg(all(feature = "livekit-webrtc", target_os = "macos"))]
pub mod gpu_bridge;

/// Stub: GPU bridge unavailable (non-macOS or livekit-webrtc disabled).
#[cfg(not(all(feature = "livekit-webrtc", target_os = "macos")))]
pub mod gpu_bridge {
    pub fn has_bridge<T>(_slot_id: T) -> bool {
        false
    }
}

/// Cross-platform GPU-accelerated I420 publisher via wgpu compute shader.
#[cfg(feature = "livekit-webrtc")]
pub mod wgpu_i420;
