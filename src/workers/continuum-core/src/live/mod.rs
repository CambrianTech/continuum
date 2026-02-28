// Module declaration order matters for native library link ordering.
// avatar/video/transport use webrtc-sys (links libwebrtc.a with protobuf).
// audio uses ort/fastembed (links libonnxruntime.a with a DIFFERENT protobuf).
// Declaring webrtc-dependent modules FIRST ensures the linker resolves
// webrtc's protobuf symbols before ort's, avoiding NULL pointer crashes
// from protobuf version mismatch between the two static C++ libraries.
pub mod avatar;
pub mod video;
pub mod transport;
pub mod session;
pub mod handle;
pub mod types;
pub mod audio;

pub use audio::router::{AudioEvent, AudioRouter, RoutedParticipant};
pub use audio::capabilities::{AudioCapabilities, AudioRouting, ModelCapabilityRegistry};
pub use session::orchestrator::VoiceOrchestrator;
pub use types::*;
