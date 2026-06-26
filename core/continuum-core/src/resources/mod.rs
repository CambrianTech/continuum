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
//!
//! # Forward design: adaptive quality scaling (persona tier up/down)
//!
//! The arbiter's eventual job is not just *evict vs keep* but *resize* — the
//! cognition analogue of adaptive video bitrate / CPU DVFS. Under VRAM
//! pressure, downgrade a persona's base model (qwen-30B → qwen-7B) instead of
//! killing it; when a call ends and headroom returns, upgrade it back. The
//! authority is the natural home because only it sees the whole footprint
//! (Bevy + LiveKit + N personas) at once.
//!
//! The reclaim interface here ALREADY accommodates the **downgrade** half: a
//! consumer (serving) can satisfy a [`ReclaimRequest`] by swapping to a smaller
//! base and reporting [`ReclaimStatus::Partial`] with the bytes that move freed
//! — the persona stays alive at lower fidelity rather than evicting. The
//! consumer owns *how* it frees; tier-down is one strategy among unload / cache-
//! drop. No new type is needed for this direction.
//!
//! The **upgrade** half is the one open seam: there is no `offer`-direction to
//! mirror `reclaim` — no way for the authority to say "headroom appeared, want
//! to grow?". When built, it pairs with the lease being keyed on the
//! `(base_model, genome)` it is contracted at (see the seamless-failover work),
//! so a tier change is a re-contract, not an evict+respawn. Until then: do NOT
//! bake model-size policy into the ledger — it stays in the arbiter, and the
//! ledger only ever sees the resulting byte deltas.

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
