//! Central resource contract for the Rust runtime.
//!
//! This module is the low-level admission surface every expensive subsystem
//! should converge on: persona cognition, RAG, embeddings, local generation,
//! genome/LoRA paging, live media, Bevy rendering, storage pruning, and grid
//! work. Policy lives here; callers submit resource demands and receive leases
//! or explicit refusal reasons.
//!
//! The older throughput primitives still live in `cognition` because that is
//! where the first slice landed. Re-exporting them here gives new code a
//! stable, subsystem-neutral import path while follow-up slices move call sites
//! off `crate::cognition::*`.

pub use crate::cognition::{
    ResourceClass, TargetSilicon, ThroughputLease, ThroughputLeaseError, ThroughputLeaseRegistry,
    ThroughputLeaseRevocationPolicy, ThroughputLeaseSnapshot,
};

pub mod broker;

pub use broker::{
    ResourceAdmissionReport, ResourceBroker, ResourceBrokerConfig, ResourceDemand,
    ResourceLaneBudget, ResourceRefusalReason,
};
