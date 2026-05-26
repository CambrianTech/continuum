//! Rust-native AIRC integration primitives.
//!
//! This package is the no-Node boundary for agent flywheel work. Transport
//! process handling, queue validation, and typed queue envelopes live here so
//! ServiceModule wrappers stay thin and future AIRC commands reuse one path.

pub mod client;
pub mod daemon_endpoint;
pub mod daemon_transport;
pub mod event_transport;
pub mod process;
pub mod realtime;
pub mod realtime_store;
pub mod types;

pub use client::{AircQueueClient, CliAircQueueClient};
pub use daemon_endpoint::default_socket_path_in;
pub use daemon_transport::{AircDaemonClient, DaemonAircEventTransport};
pub use event_transport::{AircEventTransport, StoreAircEventTransport};
pub use process::{AircCommandRunner, AircInvocation, TokioAircCommandRunner};
pub use realtime::{
    AircMediaControlEvent, AircPeerCapability, AircPeerManifest, AircPresenceEvent,
    AircPresenceState, AircRealtimeDelivery, AircRealtimeEnvelope, AircRealtimePayload,
    AircRealtimePayloadRef, AircRealtimeSchema, AircReceipt, AircReplayCursor,
    AircSubscriptionAction, AircSubscriptionEvent,
};
pub use realtime_store::{
    AircCapabilityIndexEntry, AircRealtimePublishParams, AircRealtimePublishResult,
    AircRealtimeReplayParams, AircRealtimeReplayResult, AircRealtimeStore,
    InMemoryAircRealtimeStore,
};
pub use types::{
    AircQueueCardEnvelope, AircQueueIssue, AircQueueListEnvelope, AircQueueListRequest,
    AircQueueScanError, AircQueueScanErrorKind, AircQueueScanParams, AircQueueScanResult,
};
