//! Tier types — `TierRole`, `EvictionPolicy`, `TierCapacity`,
//! `EvictionRecord`, `TierError`.
//!
//! Discrete-GPU hardware has five distinct tiers; unified-memory
//! hardware collapses Fast+Warm into one. Subsystems address tiers by
//! role (the enum), not by ordinal position — that's what makes
//! "L1→L2 eviction on UMA" structurally impossible.
//!
//! Per GENOME-FOUNDRY-SENTINEL Part 2.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::working_set::PageRef;

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

impl TierRole {
    /// Whether this role is present on UMA-class hardware. `Warm` is
    /// structurally omitted on UMA (Fast and Warm would share the same
    /// physical bytes). The governor uses this to build a
    /// `Vec<TierConfig>` of the right shape at boot.
    pub fn is_present_on_uma(&self) -> bool {
        !matches!(self, TierRole::Warm)
    }
}

/// Per-tier eviction policy. The variants are dimensioned by the
/// per-role table in GENOME-FOUNDRY-SENTINEL Part 2:
///
/// | Role | Policy | When eviction fires |
/// |------|--------|---------------------|
/// | Fast | `LruWithinTurn` | sub-step needs a page not resident |
/// | Warm | `LruAcrossTurns { window }` (discrete-GPU only) | Fast spill |
/// | Bench | `LfuPlusRecency` | Warm spill (discrete) / Fast spill (UMA) |
/// | Cold | `DemandAlignedWithRefinedPreference` | Bench spill |
/// | Frozen | `AppendOnlyGcOnSleep` | never in hot path |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/EvictionPolicy.ts"
)]
pub enum EvictionPolicy {
    /// LRU within a single turn. Resets between turns.
    LruWithinTurn,
    /// LRU across a rolling window of N turns. Governor sets N
    /// (default 100 per the spec).
    LruAcrossTurns {
        #[serde(rename = "windowTurns")]
        #[ts(rename = "windowTurns", type = "number")]
        window_turns: u32,
    },
    /// LFU + recency tiebreak. Broad-use pages get a retention bonus
    /// the substrate computes from cross-persona access frequency.
    LfuPlusRecency,
    /// Demand-aligned with a preference for sentinel-refined pages
    /// over imported pages of equal demand. Imported pages can be
    /// re-pulled from the genome catalog; refined pages embody work
    /// that took compute to produce.
    DemandAlignedWithRefinedPreference,
    /// Append-only with provenance preserved. GC only during sleep
    /// / opportunistic idle. Frozen tier — never in hot path.
    AppendOnlyGcOnSleep,
}

impl EvictionPolicy {
    /// The canonical policy for a given tier role (what the spec's
    /// per-role table prescribes). Governor implementations are free
    /// to override per-policy but this is the default the type system
    /// can guarantee. `Warm` has no canonical policy on UMA (it isn't
    /// configured there at all); calling `canonical_for(TierRole::Warm)`
    /// returns the discrete-GPU default.
    pub fn canonical_for(role: TierRole) -> Self {
        match role {
            TierRole::Fast => EvictionPolicy::LruWithinTurn,
            TierRole::Warm => EvictionPolicy::LruAcrossTurns { window_turns: 100 },
            TierRole::Bench => EvictionPolicy::LfuPlusRecency,
            TierRole::Cold => EvictionPolicy::DemandAlignedWithRefinedPreference,
            TierRole::Frozen => EvictionPolicy::AppendOnlyGcOnSleep,
        }
    }
}

/// Current vs configured byte capacity of a tier. The governor sets
/// `configured_limit` from the policy file (Part 11). The tier itself
/// reports `current_used` from its backing store. The delta is the
/// available headroom; when `current_used` approaches `configured_limit`,
/// the tier triggers eviction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/genome/TierCapacity.ts")]
pub struct TierCapacity {
    /// Bytes currently in use by this tier's backing store.
    #[ts(type = "number")]
    pub current_used: u64,
    /// Bytes the tier is configured to hold (policy limit, NOT a
    /// hardware ceiling). The governor enforces; the tier respects.
    #[ts(type = "number")]
    pub configured_limit: u64,
}

impl TierCapacity {
    /// Bytes available before eviction must run. `0` means the tier
    /// is at-or-over its policy limit and any new write triggers an
    /// eviction first.
    pub fn available_bytes(&self) -> u64 {
        self.configured_limit.saturating_sub(self.current_used)
    }

    /// Fraction-of-limit currently used. `1.0` = at limit; `> 1.0` =
    /// over (the tier ran past its budget — usually transient between
    /// the trigger and the eviction completing). Returns `0.0` if
    /// `configured_limit == 0` to avoid divide-by-zero.
    pub fn utilization(&self) -> f64 {
        if self.configured_limit == 0 {
            return 0.0;
        }
        self.current_used as f64 / self.configured_limit as f64
    }
}

/// Typed record emitted to the trace bus every time a page is evicted
/// from some tier. The reason carries the policy that fired (LRU,
/// LFU, etc.). Recurring evictions of the same page across turns are
/// the signal sentinel uses to upgrade the page's tier policy.
///
/// Per GENOME-FOUNDRY-SENTINEL Part 2: "every evicted page emits an
/// EvictionRecord to the trace bus." PR-3 wires this through my just-
/// shipped artifact dispatch (#1339 + #1343); PR-1 ships the shape.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/EvictionRecord.ts"
)]
pub struct EvictionRecord {
    /// The page that was evicted.
    pub page: PageRef,
    /// Which tier evicted it.
    pub from_role: TierRole,
    /// Where the page went (Some) or whether it was dropped entirely
    /// (None — only valid for Cold/Frozen during GC).
    #[ts(optional)]
    pub to_role: Option<TierRole>,
    /// The policy that fired this eviction. Lets the trace bus
    /// reconstruct *why* without re-running the policy.
    pub policy_fired: EvictionPolicy,
    /// Time spent on the eviction itself (selection + tier-write +
    /// metadata update). Doesn't include the time the calling
    /// page_in/page_out spent blocked on it — that's a separate
    /// signal on the caller side.
    #[ts(type = "number")]
    pub elapsed_us: u64,
}

/// Errors a tier's read/write operations can surface. PR-1 ships
/// the shape; PR-2's `TierStore` trait returns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/genome/TierError.ts")]
pub enum TierError {
    /// The requested page isn't in this tier and a higher tier
    /// couldn't be paged in (chain exhausted).
    PageNotFound { page: PageRef },
    /// Tier write would exceed configured_limit and no eviction
    /// candidate is available (every page is pinned, etc.).
    NoEvictionCandidate {
        from_role: TierRole,
        #[ts(type = "number")]
        bytes_needed: u64,
    },
    /// Backing-store I/O error. The inner message is the OS-level
    /// reason; not structured because backends differ.
    BackingStoreIo { reason: String },
    /// Caller asked for a tier role this hardware doesn't have
    /// (e.g. `Warm` on UMA). Defensive; type system should already
    /// have caught it at registration but the runtime still asserts.
    RoleNotConfigured { role: TierRole },
}

impl std::fmt::Display for TierError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TierError::PageNotFound { page } => write!(f, "tier: page not found: {page:?}"),
            TierError::NoEvictionCandidate {
                from_role,
                bytes_needed,
            } => write!(
                f,
                "tier {from_role:?}: no eviction candidate for {bytes_needed} bytes"
            ),
            TierError::BackingStoreIo { reason } => write!(f, "tier I/O: {reason}"),
            TierError::RoleNotConfigured { role } => {
                write!(f, "tier role {role:?} not configured on this hardware")
            }
        }
    }
}

impl std::error::Error for TierError {}

#[cfg(test)]
mod tests {
    //! Pin the invariants the type system + serde encoding guarantee
    //! for PR-1's tier surface. Each test corresponds to a "what if a
    //! downstream PR / consumer subtly changes this" failure mode.
    use super::*;

    /// What this catches: TierRole's wire form is lowercase strings
    /// ("fast", "warm", ...) — TypeScript + downstream tooling will
    /// parse these strings. If a future PR renames a variant or
    /// changes the serde casing, the wire breaks.
    #[test]
    fn tier_role_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&TierRole::Fast).unwrap(), "\"fast\"");
        assert_eq!(serde_json::to_string(&TierRole::Warm).unwrap(), "\"warm\"");
        assert_eq!(
            serde_json::to_string(&TierRole::Bench).unwrap(),
            "\"bench\""
        );
        assert_eq!(serde_json::to_string(&TierRole::Cold).unwrap(), "\"cold\"");
        assert_eq!(
            serde_json::to_string(&TierRole::Frozen).unwrap(),
            "\"frozen\""
        );
    }

    /// What this catches: `Warm` is the only role omitted on UMA.
    /// If a future PR adds another UMA-omitted role (e.g. an embedded
    /// target dropping Bench), it should be a deliberate flip of this
    /// test — not a silent change that breaks UMA governor builds.
    #[test]
    fn only_warm_is_omitted_on_uma() {
        assert!(TierRole::Fast.is_present_on_uma());
        assert!(!TierRole::Warm.is_present_on_uma());
        assert!(TierRole::Bench.is_present_on_uma());
        assert!(TierRole::Cold.is_present_on_uma());
        assert!(TierRole::Frozen.is_present_on_uma());
    }

    /// What this catches: EvictionPolicy serializes with the
    /// per-variant `kind` tag (camelCase) plus camelCase field names
    /// (e.g. `windowTurns`). Wire stability — TS consumers narrow by
    /// `kind`. Field name `windowTurns` deliberately matches the
    /// camelCase TS convention.
    #[test]
    fn eviction_policy_serializes_with_kind_tag() {
        let p = EvictionPolicy::LruAcrossTurns { window_turns: 100 };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"kind\":\"lruAcrossTurns\""), "got {json}");
        assert!(json.contains("\"windowTurns\":100"), "got {json}");

        assert!(serde_json::to_string(&EvictionPolicy::LruWithinTurn)
            .unwrap()
            .contains("\"kind\":\"lruWithinTurn\""));
        assert!(serde_json::to_string(&EvictionPolicy::LfuPlusRecency)
            .unwrap()
            .contains("\"kind\":\"lfuPlusRecency\""));
    }

    /// What this catches: each role gets the canonical policy from
    /// GENOME-FOUNDRY-SENTINEL Part 2's per-role table. If a future
    /// PR changes a default (e.g. flips Bench from LFU+recency to
    /// LRU), this test flags it — that's a substrate policy change
    /// that needs deliberate review, not a refactor accident.
    #[test]
    fn canonical_eviction_policy_matches_spec_table() {
        assert_eq!(
            EvictionPolicy::canonical_for(TierRole::Fast),
            EvictionPolicy::LruWithinTurn
        );
        assert_eq!(
            EvictionPolicy::canonical_for(TierRole::Warm),
            EvictionPolicy::LruAcrossTurns { window_turns: 100 }
        );
        assert_eq!(
            EvictionPolicy::canonical_for(TierRole::Bench),
            EvictionPolicy::LfuPlusRecency
        );
        assert_eq!(
            EvictionPolicy::canonical_for(TierRole::Cold),
            EvictionPolicy::DemandAlignedWithRefinedPreference
        );
        assert_eq!(
            EvictionPolicy::canonical_for(TierRole::Frozen),
            EvictionPolicy::AppendOnlyGcOnSleep
        );
    }

    /// What this catches: TierCapacity's available_bytes saturates
    /// to zero on overage instead of underflowing into a giant
    /// "available" number that would defeat eviction triggers.
    #[test]
    fn tier_capacity_available_saturates_on_overage() {
        let over = TierCapacity {
            current_used: 1_000_000,
            configured_limit: 500_000,
        };
        assert_eq!(over.available_bytes(), 0);

        let under = TierCapacity {
            current_used: 100,
            configured_limit: 500,
        };
        assert_eq!(under.available_bytes(), 400);
    }

    /// What this catches: utilization handles configured_limit == 0
    /// (a tier that hasn't been configured yet) without divide-by-zero.
    /// Real configs always have a non-zero limit, but during boot the
    /// governor briefly sees zero — must not panic.
    #[test]
    fn tier_capacity_utilization_handles_zero_limit() {
        let zero = TierCapacity {
            current_used: 0,
            configured_limit: 0,
        };
        assert_eq!(zero.utilization(), 0.0);
    }

    /// What this catches: TierError implements Display + Error so it
    /// works in `?` chains. Without this, callers would need manual
    /// `.map_err()` boilerplate everywhere.
    #[test]
    fn tier_error_implements_error_trait() {
        let e = TierError::NoEvictionCandidate {
            from_role: TierRole::Fast,
            bytes_needed: 4096,
        };
        let _: &dyn std::error::Error = &e;
        let display = format!("{e}");
        assert!(display.contains("Fast"));
        assert!(display.contains("4096"));
    }

    /// What this catches: TierError variants serialize with the
    /// `kind` tag — TS consumers will narrow by it. Same wire
    /// stability check as EvictionPolicy.
    #[test]
    fn tier_error_serializes_with_kind_tag() {
        let e = TierError::RoleNotConfigured {
            role: TierRole::Warm,
        };
        let json = serde_json::to_string(&e).unwrap();
        assert!(
            json.contains("\"kind\":\"roleNotConfigured\""),
            "got {json}"
        );
        assert!(json.contains("\"role\":\"warm\""), "got {json}");
    }
}
