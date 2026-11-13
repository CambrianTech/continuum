//! Avatar rendering backends.
//!
//! Each backend implements `AvatarRenderer` for a different rendering technology:
//! - `procedural`: CPU-rendered colored circles (zero-dependency fallback)
//! - `bevy_3d`: GPU-rendered 3D VRM models via Bevy headless (`avatar-3d` feature only)
//! - `live2d`: 2D sprite-sheet compositing for Live2D-style avatars

#[cfg(feature = "avatar-3d")]
pub mod bevy_3d;
pub mod live2d;
pub mod procedural;

#[cfg(feature = "avatar-3d")]
pub use bevy_3d::{Bevy3DBackend, BevyChannelRenderer};
pub use live2d::{Live2DBackend, Live2DRenderer};
pub use procedural::{ProceduralBackend, ProceduralRenderer};
