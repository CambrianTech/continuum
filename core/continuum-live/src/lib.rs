//! continuum-live: cut out of continuum-core (MICROKERNEL-DECOMPOSITION.md). Depends only on the
//! engine, the kernel and crates below them; core re-exports these modules under their old names.

pub use continuum_engine::*;

// objc macros (sel!, msg_send!, class!) must be imported at crate root.
// Used by live::video::metal_gpu_convert for Metal compute shader dispatch —
// the macros' only user, so the import carries that module's exact gate.
#[cfg(all(target_os = "macos", feature = "avatar-3d", feature = "livekit-webrtc"))]
#[macro_use]
extern crate objc;

pub mod consumers;
pub mod live;
pub mod media;
