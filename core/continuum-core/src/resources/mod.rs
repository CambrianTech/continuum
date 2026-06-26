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
//!
//! # Two axes, one authority
//!
//! There are two complementary admission axes under this module, NOT two rival
//! managers:
//!
//! - **Throughput admission** ([`ResourceBroker`] over [`ThroughputLeaseRegistry`]):
//!   *how many transient jobs run at once* per concurrency lane (cost-units,
//!   slots). A persona's generation occupies a GPU slot for the length of one
//!   reply. This is the existing slice.
//! - **Byte residency** ([`ledger::ResourceLeaseLedger`]): *which subsystem
//!   holds how many physical bytes right now* (VRAM/RAM/disk), refused against
//!   *scanned available* capacity so serving cannot over-commit blind to Bevy
//!   and LiveKit (the OOM bug task #56 fixes). A loaded model sits in VRAM for
//!   hours — a residency, not a transient job.
//!
//! Slots ≠ bytes: a box can have 8 GPU slots yet fit only one 18 GB model in
//! 24 GB of VRAM. The two axes coexist under this one module; a future
//! `SubstrateGovernor`-fed daemon owns both ledgers plus the consumers.
//!
//! # The byte-residency layer (task #56)
//!
//! - [`lease`] — the grant vocabulary: [`ResourceKind`], [`ReclaimPolicy`],
//!   [`ResourceLease`], [`LeaseRequest`], [`LeaseError`].
//! - [`ledger`] — pure accounting: the over-commit guard, reservation floors
//!   (real-time fairness — a live call keeps its bytes), and min-dwell
//!   hysteresis (anti-thrash — fresh grants aren't ripped back out).
//! - [`consumer`] — the async-reclaim interface a leaseholder implements:
//!   the authority *asks* for bytes back and *waits*, never yanks.
//!
//! These are pure mechanism. The *values* — reservation floors, dwell windows,
//! how aggressively to reclaim vs refuse — are policy a higher arbiter (the
//! daemon, reading governor pressure such as `UserActive` during a call)
//! supplies. Mechanism enforces; policy decides.

pub mod broker;
pub mod consumer;
pub mod ledger;
pub mod lease;

pub use crate::cognition::{
    ResourceClass, TargetSilicon, ThroughputLease, ThroughputLeaseError, ThroughputLeaseRegistry,
    ThroughputLeaseRevocationPolicy, ThroughputLeaseSnapshot,
};

pub use broker::{
    ResourceAdmissionReport, ResourceBroker, ResourceBrokerConfig, ResourceDemand,
    ResourceLaneBudget, ResourceRefusalReason,
};

pub use consumer::{
    ConsumerFootprint, ReclaimOutcome, ReclaimReason, ReclaimRequest, ReclaimStatus,
    ResourceConsumer,
};
pub use ledger::{KindLedger, LeaseBoard, ResourceLeaseLedger};
pub use lease::{LeaseError, LeaseRequest, ReclaimPolicy, ResourceKind, ResourceLease};
