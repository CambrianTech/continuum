//! Avatar rendering backends.
//!
//! Each backend implements `AvatarRenderer` for a different rendering technology:
//! - `procedural`: CPU-rendered colored circles (zero-dependency fallback)
//! - `bevy_3d`: GPU-rendered 3D VRM models via Bevy headless
//! - `live2d`: 2D sprite-sheet compositing for Live2D-style avatars

pub mod procedural;
pub mod bevy_3d;
pub mod live2d;

pub use procedural::{ProceduralRenderer, ProceduralBackend};
pub use bevy_3d::{BevyChannelRenderer, Bevy3DBackend};
pub use live2d::{Live2DRenderer, Live2DBackend};
