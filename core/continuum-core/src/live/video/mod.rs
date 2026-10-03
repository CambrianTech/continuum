//! Video: frame sources, capture, and (under `avatar-3d`) the headless Bevy avatar
//! renderer with its GPU→GPU frame converters.
//!
//! `avatar_types` (Emotion, Gesture, slot/resolution constants) is ungated — it is
//! vocabulary, not rendering. Everything that links bevy/wgpu sits behind the
//! `avatar-3d` cargo feature (see core/continuum-core/Cargo.toml).

pub mod avatar_types;
#[cfg(feature = "avatar-3d")]
pub mod bevy_renderer;
#[cfg(feature = "livekit-webrtc")]
pub mod capture;
pub mod generator;
#[cfg(feature = "avatar-3d")]
pub mod memory_reporter;
#[cfg(all(feature = "avatar-3d", feature = "livekit-webrtc", target_os = "macos"))]
pub mod metal_gpu_convert;
pub mod source;
#[cfg(feature = "avatar-3d")]
pub mod wgpu_gpu_convert;
