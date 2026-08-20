//! Rust-native AIRC integration primitives.
//!
//! This package is the no-Node boundary for agent flywheel work. Transport
//! process handling, queue validation, and typed queue envelopes live here so
//! ServiceModule wrappers stay thin and future AIRC commands reuse one path.

pub mod bridge_protocol;
pub mod client;
pub mod daemon_endpoint;
pub mod daemon_transport;
pub mod discovery;
pub mod discovery_aggregate;
pub mod discovery_state;
pub mod event_transport;
pub mod inbound_attach;
pub mod process;
pub mod realtime;
pub mod realtime_store;
pub mod realtime_wire;
pub mod types;

pub use bridge_protocol::{
    format_airc_bridge_chat_text, parse_airc_bridge_message, room_from_airc_channel,
    summarize_bridge_response, BridgeAction, ParseOptions, ParsedBridgeMessage,
};
pub use discovery_aggregate::discover;
pub use discovery_state::{AircDiscovery, DiscoveryFailure, PartialDiscovery};

pub use client::{AircQueueClient, CliAircQueueClient};
#[allow(deprecated)]
pub use daemon_endpoint::default_socket_path_in;
pub use daemon_transport::{AircDaemonClient, DaemonAircEventTransport};
pub use discovery::{
    discover_airc_socket, discover_default_channel, discover_default_room_name, discover_peer_id,
    DiscoveryError,
};
pub use event_transport::{AircEventTransport, StoreAircEventTransport};
pub use inbound_attach::spawn_daemon_attach;
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
