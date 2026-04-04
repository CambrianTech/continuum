pub mod bridge_client;
pub mod call_server;
pub mod media;

// Legacy livekit_agent modules — kept for reference during migration.
// Will be deleted once bridge_client is fully validated.
#[cfg(feature = "livekit-webrtc")]
pub mod livekit_agent;
#[cfg(not(feature = "livekit-webrtc"))]
pub mod livekit_agent_stub;
#[cfg(not(feature = "livekit-webrtc"))]
pub use livekit_agent_stub as livekit_agent;
