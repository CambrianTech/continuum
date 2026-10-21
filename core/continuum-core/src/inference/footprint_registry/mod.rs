//! Per-component memory footprint registry — "what are we made of?"
//!
//! Per §13 of `docs/architecture/PERSONA-CONTEXT-PAGING.md`: GpuMonitor
//! (§12) tells the policy WHAT pressure looks like; the registry tells
//! it WHAT to do about it. Without per-component attribution the policy
//! knows "we're at 90% of process limit" but has no idea WHICH of N
//! things in our process is biggest, cheapest to spill, or worth
//! keeping hot.
//!
//! Every allocation site (KV slots, LoRA adapters, model weights,
//! render buffers, tokenizer caches, audio/video pipelines) reports
//! bytes via a single DashMap keyed on (persona, recipe, backend,
//! resource type, residency). Reporting is unconditional and cheap;
//! no `#[cfg]`, no platform branches.
//!
//! The registry's `cheapest_eviction_for` is what makes paging real:
//! given "free X bytes," it returns a plan picking the lowest-cost
//! combination of evictable entries. Cost-driven, not type-prioritized.
//!
//! Module layout:
//!
//! - `mod.rs` (this file) — `FootprintRegistry` impl, global singleton,
//!   integration tests across the registry's behavior.
//! - `types.rs` — pure data shapes (ResourceType, FootprintKey,
//!   FootprintEntry, EvictionPlan, RegistryHealth, RegistrySnapshot)
//!   + key constructors. Independently testable for layout/equality.
//! - `costs.rs` — spill/reload heuristics per ResourceType + tests for
//!   policy invariants (KV cheaper than ModelWeights to spill, etc.).
//!   The file Phase 4.0 telemetry will replace as measurements mature.

mod costs;
mod types;

pub use types::{
    EvictionPlan, FootprintEntry, FootprintKey, LeaseRevocationOutcome, RegistryHealth,
    RegistrySnapshot, ResourceType,
};

use crate::inference::planning::throughput_lease::{
    ThroughputLease, ThroughputLeaseError, ThroughputLeaseRevocationPolicy, ThroughputLeaseSnapshot,
};
use crate::paging::broker::PressureTier;
use crate::paging::lease_revocation::select_leases_to_revoke;
use dashmap::{mapref::entry::Entry, DashMap};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::SystemTime;
use uuid::Uuid;

mod footprint_lease_mirror;
pub use footprint_lease_mirror::*;

// ─── Tests — registry behavior + singleton ─────────────────────────────
//
// Type-shape tests (key distinctness, constructor field ownership) live
// in types::tests. Cost heuristic invariants live in costs::tests. The
// tests below exercise registry BEHAVIOR — adds, removes, queries,
// eviction planning, sanity check, snapshot, singleton identity.

#[cfg(test)]
mod tests;
