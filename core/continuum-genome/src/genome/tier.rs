//! `TierRole`: where a recalled artifact lives, as the recall ranker scores it
//! (`recall_scoring::local_role_score`). Per GENOME-FOUNDRY-SENTINEL Part 2.
//!
//! Gene residency on the engine (device, host RAM) is `genome::gene_residency`; the per-persona
//! working-set stack these roles once fed (local_manager, manager, store, blob, bus, working_set)
//! was never wired and is deleted (Fable, Cormac agreed on airc 2026-10-24).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The five named tier roles. Discrete-GPU configurations populate
/// all five; UMA configurations omit `Warm` (Fast and Warm would
/// share the same physical bytes there — an `Fast`→`Warm` eviction
/// would be a no-op, so the type system removes the option). Vision
/// Pro / iOS / M-series MacBooks are UMA-class and have four roles
/// in their governor's `Vec<TierConfig>`. Embedded targets may drop
/// to three tiers (Fast, Cold, Frozen) if Bench would compete with
/// foreground responsiveness.
///
/// Tier semantics:
/// - `Fast` — bytes the accelerator can read at peak bandwidth.
///   Discrete GPU: VRAM. UMA: the hot portion of unified memory.
/// - `Warm` — bytes the accelerator can reach with a copy or a
///   tier-promotion. Discrete GPU: host RAM (PCIe-attached). UMA:
///   omitted (same pool as Fast).
/// - `Bench` — bytes the host can read at memory speed; cold to the
///   accelerator. A designated portion of system RAM holding the
///   genome catalog + recently-used artifacts. Always present.
/// - `Cold` — bytes on local SSD. The full genome pool lives here on
///   every hardware class. Read latency is milliseconds.
/// - `Frozen` — bytes on archive storage. Append-only with provenance
///   preserved. Never on the hot path; GC during sleep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../protocol/typescript/genome/TierRole.ts")]
pub enum TierRole {
    Fast,
    Warm,
    Bench,
    Cold,
    Frozen,
}
