//! Rust-native AIRC integration primitives.
//!
//! This package is the no-Node boundary for agent flywheel work. Transport
//! process handling, queue validation, and typed queue envelopes live here so
//! ServiceModule wrappers stay thin and future AIRC commands reuse one path.

pub mod client;
pub mod process;
pub mod realtime;
pub mod types;

pub use client::{AircQueueClient, CliAircQueueClient};
pub use process::{AircCommandRunner, AircInvocation, TokioAircCommandRunner};
pub use realtime::{
    AircMediaControlEvent, AircPresenceEvent, AircPresenceState, AircRealtimeDelivery,
    AircRealtimeEnvelope, AircRealtimePayload, AircRealtimePayloadRef, AircRealtimeSchema,
    AircReceipt, AircReplayCursor, AircSubscriptionAction, AircSubscriptionEvent,
};
pub use types::{
    AircQueueCardEnvelope, AircQueueIssue, AircQueueListEnvelope, AircQueueListRequest,
    AircQueueScanError, AircQueueScanErrorKind, AircQueueScanParams, AircQueueScanResult,
};
