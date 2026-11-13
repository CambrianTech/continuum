//! Live/media consumers and the avatar module: wiring that sits on the live and resource
//! layers only (cut 5, MICROKERNEL-DECOMPOSITION.md). Core's ipc startup registers them.

pub mod avatar;
// The Bevy renderer's residency consumer: only with the renderer (`avatar-3d`).
#[cfg(feature = "avatar-3d")]
pub mod bevy_consumer;
pub mod live_session_consumer;
pub mod perception_consumer;
