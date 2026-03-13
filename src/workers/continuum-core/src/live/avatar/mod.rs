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

pub mod backend;
pub mod backends;
pub mod catalog;
pub mod frame;
pub mod frame_analysis;
pub mod frame_publisher;
pub mod gender;
pub mod hash;
pub mod publishers;
pub mod registry;
pub mod render_loop;
pub mod renderer;
pub mod selection;
pub mod types;

// Re-export everything at the module level for backward compatibility.
// Call sites use `crate::live::avatar::RgbaFrame`, etc.
pub use backend::{AvatarError, ModelFormat, RenderBackend};
pub use backends::{
    Bevy3DBackend, BevyChannelRenderer, Live2DBackend, Live2DRenderer, ProceduralBackend,
    ProceduralRenderer,
};
pub use catalog::{avatar_model_path, AvatarCatalog, AVATAR_CATALOG};
pub use frame::{AvatarConfig, ResolutionTier, RgbaFrame};
pub use frame_analysis::{FrameAnalysis, HealthVerdict};
pub use frame_publisher::{create_publisher, CpuI420Publisher, FramePublisher, PublishError};
pub use gender::{gender_from_identity, gender_from_voice_name};
pub use hash::{deterministic_index, deterministic_pick, fnv1a_hash};
#[cfg(target_os = "macos")]
pub use publishers::gpu_bridge::GpuBridgePublisher;
pub use render_loop::{
    allocate_bevy_slot, create_renderer, reset_slot_pool, spawn_renderer_loop,
    BevySlotAllocation, SlotGuard,
};
pub use renderer::AvatarRenderer;
pub use selection::{
    allocate_avatars_batch, allocate_dynamic_batch, select_avatar_by_identity,
    select_avatar_for_agent, select_avatar_for_voice, select_dynamic_avatar, select_from_catalog,
    select_from_catalog_by_identity,
};
pub use types::*;

#[cfg(test)]
pub use selection::reset_allocation;
