//! Avatar Renderer — Abstracts video frame generation for AI persona avatars.
//!
//! Provides a trait-based rendering pipeline where different backends can produce
//! RGBA frames for LiveKit video publishing:
//!
//!   - ProceduralRenderer: Simple colored circle (current, zero-dependency)
//!   - BevyChannelRenderer: Full 3D VRM model rendering via Bevy headless
//!
//! The renderer runs on its own thread and delivers RGBA frames via crossbeam channel.
//! LiveKit's video loop consumes frames and feeds them to NativeVideoSource.
//!
//! Architecture:
//!   AvatarRenderer (trait) → RGBA frames → crossbeam channel → video loop → I420 → NativeVideoSource
//!
//! Module structure:
//!   types.rs      — AvatarModel, AvatarStyle, VoiceProfile, AvatarGender, etc.
//!   frame.rs      — RgbaFrame, AvatarConfig, ResolutionTier
//!   renderer.rs   — AvatarRenderer trait
//!   catalog.rs    — AVATAR_CATALOG, avatar_model_path
//!   selection.rs  — select_avatar_for_voice/identity/agent, allocate_avatars_batch
//!   gender.rs     — gender_from_voice_name, gender_from_identity
//!   hash.rs       — fnv1a_hash, deterministic_pick, deterministic_index
//!   render_loop.rs — spawn_renderer_loop, create_renderer
//!   backends/     — ProceduralRenderer, BevyChannelRenderer

pub mod types;
pub mod frame;
pub mod frame_publisher;
pub mod renderer;
pub mod catalog;
pub mod selection;
pub mod gender;
pub mod hash;
pub mod render_loop;
pub mod backend;
pub mod registry;
pub mod backends;
pub mod frame_analysis;
pub mod publishers;

// Re-export everything at the module level for backward compatibility.
// Call sites use `crate::voice::avatar::RgbaFrame`, etc.
pub use types::*;
pub use frame::{RgbaFrame, AvatarConfig, ResolutionTier};
pub use renderer::AvatarRenderer;
pub use backend::{RenderBackend, AvatarError, ModelFormat};
pub use frame_analysis::{FrameAnalysis, HealthVerdict};
pub use catalog::{AVATAR_CATALOG, avatar_model_path, AvatarCatalog};
pub use selection::{
    select_avatar_for_voice, select_avatar_by_identity,
    select_avatar_for_agent, allocate_avatars_batch,
    select_from_catalog, select_from_catalog_by_identity,
    select_dynamic_avatar, allocate_dynamic_batch,
};
pub use gender::{gender_from_voice_name, gender_from_identity};
pub use hash::{fnv1a_hash, deterministic_pick, deterministic_index};
pub use render_loop::{create_renderer, spawn_renderer_loop, allocate_bevy_slot, BevySlotAllocation, SlotGuard};
pub use frame_publisher::{FramePublisher, CpuI420Publisher, PublishError, create_publisher};
#[cfg(target_os = "macos")]
pub use publishers::gpu_bridge::GpuBridgePublisher;
pub use backends::{
    ProceduralRenderer, BevyChannelRenderer, ProceduralBackend, Bevy3DBackend,
    Live2DRenderer, Live2DBackend,
};

#[cfg(test)]
pub use selection::reset_allocation;
