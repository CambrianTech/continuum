//! Bevy Headless Avatar Renderer
//!
//! A single Bevy app instance renders all AI persona avatars simultaneously.
//! Each avatar gets its own RenderLayer + Camera + render target texture.
//! GPU readback delivers RGBA frames via crossbeam channels to LiveKit video loops.
//!
//! Architecture:
//!   BevyAvatarSystem (singleton)
//!     └── Bevy App (dedicated OS thread)
//!           ├── Avatar slot 0: Camera → RenderTarget → Readback → channel
//!           ├── Avatar slot 1: Camera → RenderTarget → Readback → channel
//!           ├── ...
//!           └── Avatar slot 15: Camera → RenderTarget → Readback → channel
//!
//! Performance: 16 avatars × 640×360 @ ~7fps effective readback (15fps Bevy tick).
//! On Apple Silicon (shared memory + GPU bridge), readback is zero-copy IOSurface.
//!
//! ## Module Structure
//!
//! - `api` — BevyAvatarSystem singleton, public methods
//! - `app` — Bevy App setup and system registration
//! - `commands` — AvatarCommand processing
//! - `readback` — GPU readback management
//! - `setup` — Render slot allocation, readback entity spawning
//! - `stats` — Memory stats, load monitoring, light visibility
//! - `animation/` — Per-concern animation systems (blinking, breathing, etc.)
//! - `types` — All components, resources, enums, structs
//! - `scene/` — Scene graph, room, animation config
//! - `skeleton` — Bone discovery, T-pose fix, scene tree helpers
//! - `vrm` — VRM extension parsing (blend shapes, humanoid bones, lookAt)

mod animation;
mod api;
mod app;
mod commands;
mod coordinate;
mod mesh_fixup;
mod readback;
pub(crate) mod scene;
mod setup;
mod skeleton;
mod stats;
pub(crate) mod types;
mod vrm;

/// Maximum number of concurrent avatar render slots.
pub const MAX_AVATAR_SLOTS: u8 = 16;

/// Default render resolution per avatar.
pub const AVATAR_WIDTH: u32 = 640;
pub const AVATAR_HEIGHT: u32 = 360;

/// Target framerate for avatar rendering — the live-call bar is 30 (Joel,
/// 2026-08-31: "live 30fps rendering for avatar scenes in the live video
/// calls"; LIVE-AVATAR-BUDGET's speaker lane). The idle-cadence system keeps
/// non-speakers throttled (see `sync_idle_cadence`), so this is the SPEAKER'S
/// rate, never 16 slots × 30 — the budget doc's engines-not-percentages shape.
const AVATAR_FPS: f64 = 30.0;

/// HD render target resolution.
const HD_WIDTH: u32 = 1280;
const HD_HEIGHT: u32 = 720;
/// Every slot can be spotlighted — pool must cover all slots.
/// 16 × 1280×720 × 4 bytes = ~53MB (trivial vs model memory).
const MAX_HD_SLOTS: usize = MAX_AVATAR_SLOTS as usize;

// Re-export public API
pub use api::{
    get_or_init, is_running, set_gpu_manager, shutdown, subscribe_ready, try_get, BevyAvatarSystem,
};
pub use types::{AvatarCommand, BevyMemoryStats, Emotion, Gesture, SpeechAnimationClip};

// Re-export for metal_gpu_convert (crate-internal)
pub(crate) use scene::SlotRegistry;
pub(crate) use types::{FrameNotifiers, SlotDimensions};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert!(
            MAX_AVATAR_SLOTS >= 14,
            "Need at least 14 slots for all personas"
        );
        assert_eq!(AVATAR_WIDTH, 640);
        assert_eq!(AVATAR_HEIGHT, 360);
        assert!(
            AVATAR_FPS >= 10.0 && AVATAR_FPS <= 60.0,
            "FPS must be 10-60 — below 10 looks choppy, above 60 wastes GPU"
        );
    }
}
