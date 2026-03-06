//! Avatar rendering backends.
//!
//! Each backend implements `AvatarRenderer` for a different rendering technology:
//! - `procedural`: CPU-rendered colored circles (zero-dependency fallback)
//! - `bevy_3d`: GPU-rendered 3D VRM models via Bevy headless
//! - `live2d`: 2D sprite-sheet compositing for Live2D-style avatars

pub mod bevy_3d;
pub mod live2d;
pub mod procedural;

pub use bevy_3d::{Bevy3DBackend, BevyChannelRenderer};
pub use live2d::{Live2DBackend, Live2DRenderer};
pub use procedural::{ProceduralBackend, ProceduralRenderer};
