// ╔══════════════════════════════════════════════════════════════════════╗
// ║  CRITICAL: DO NOT REORDER THESE MODULE DECLARATIONS                ║
// ║  DO NOT let cargo fmt or any tool alphabetize these.               ║
// ║                                                                    ║
// ║  webrtc-sys (libwebrtc.a) and ort (libonnxruntime.a) both          ║
// ║  statically link DIFFERENT versions of C++ protobuf.               ║
// ║  webrtc modules (avatar, video, transport) MUST be declared        ║
// ║  BEFORE audio (which uses ort) so the linker resolves webrtc's     ║
// ║  protobuf symbols first.                                           ║
// ║                                                                    ║
// ║  Wrong order → SIGSEGV in RepeatedPtrFieldBase::AddOutOfLineHelper ║
// ║  This has caused production crashes TWICE (cargo fmt alphabetized  ║
// ║  these, putting `audio` first, causing immediate segfault).        ║
// ╚══════════════════════════════════════════════════════════════════════╝

// webrtc-dependent modules FIRST (link order critical)
pub mod avatar;
pub mod transport;
pub mod video;
// order-independent modules
pub mod handle;
pub mod session;
pub mod types;
// ort-dependent module LAST (link order critical)
pub mod audio;

pub use audio::capabilities::{AudioCapabilities, AudioRouting, ModelCapabilityRegistry};
pub use audio::router::{AudioEvent, AudioRouter, RoutedParticipant};
pub use session::orchestrator::VoiceOrchestrator;
pub use types::*;
