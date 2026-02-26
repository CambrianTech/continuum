//! Platform-specific FramePublisher implementations.
//!
//! Each publisher implements the FramePublisher trait from frame_publisher.rs.
//! The factory in frame_publisher.rs selects the best publisher at runtime.

#[cfg(target_os = "macos")]
pub mod native_buffer;
