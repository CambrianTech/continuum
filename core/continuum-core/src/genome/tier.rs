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
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/TierCapacity.ts"
)]
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

/// One typed entry in the governor's `Vec<TierConfig>`. Per
/// GENOME-FOUNDRY-SENTINEL Part 2: the cache hierarchy is a SEQUENCE
/// of tier roles parameterized by hardware class, NOT a fixed L1..L5
/// enum. Discrete-GPU hardware emits five entries (Fast/Warm/Bench/
/// Cold/Frozen). UMA hardware emits four — `Warm` is structurally
/// omitted because Fast and Warm would share the same physical bytes
/// and an `Fast→Warm` eviction would be a no-op.
///
/// Subsystems address tiers by `role`, never by index. That's what
/// makes the "L1→L2 eviction on UMA" failure mode structurally
/// impossible: there is no `Warm` entry in the vec on UMA, so no
/// caller can name one.
///
/// `backing` is intentionally absent at this slice. The realization
/// ladder is:
///   1. (THIS SLICE) shape + structural-absence invariant — every
///      subsystem can address tiers by role, the type system
///      guarantees UMA-omits-Warm.
///   2. NEXT SLICE: TierStore handle threading — `backing` becomes
///      `Arc<dyn TierStore>` populated by the genome subsystem at boot.
///   3. LATER: policy_file TOML loading writes real `configured_limit`
///      bytes from the per-hardware-anchor numbers in the doc
///      (M2 Air: Fast 2 LoRA layers + 2k KV tokens; 5090: Fast 8 + 16k;
///      etc.). Today's `tier_configs_for` returns `configured_limit = 0`
///      across the board — the shape is right, the numbers come later.
///
/// The compression principle: a TierConfig is the singular substrate
/// description of "one tier on this hardware." Capacity + eviction
/// policy + (eventually) backing store live together because they're
/// the same logical decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/TierConfig.ts"
)]
pub struct TierConfig {
    /// Which named role this entry fills. Address by this, never by
    /// vec index.
    pub role: TierRole,
    /// Current/configured byte capacity. Today's
    /// `tier_configs_for` returns zeros — TOML loading fills real
    /// numbers in a follow-up slice. Subsystems should never assume
    /// `configured_limit > 0` until the governor has loaded a policy.
    pub capacity: TierCapacity,
    /// Per-role canonical eviction policy from
    /// `EvictionPolicy::canonical_for(role)`. A future slice may let
    /// the policy file override this per-installation; today the
    /// canonical mapping is the single source.
    pub eviction: EvictionPolicy,
}

/// Build the governor's `Vec<TierConfig>` for the given hardware
/// class. The vec length is structurally hardware-determined:
///
/// - **UMA** (`HardwareClass.uma == true`) → 4 entries:
///   `[Fast, Bench, Cold, Frozen]`. `Warm` is OMITTED because Fast
///   and Warm would share the same physical bytes — an `Fast→Warm`
///   eviction would be a no-op.
/// - **Discrete-GPU** (`uma == false`) → 5 entries:
///   `[Fast, Warm, Bench, Cold, Frozen]`. Full ladder. `Warm` is the
///   PCIe-attached host RAM tier the GPU reaches via copy.
///
/// Order is invariant: `Fast` is always index 0, `Frozen` always last.
/// Subsystems that page across tiers walk in role order. The realization
/// pins this order with `tier_configs_for_uma_has_no_warm` /
/// `tier_configs_for_discrete_has_warm`.
///
/// Today's emission has all `capacity.configured_limit = 0`. The shape
/// is correct; the numbers are a future slice via policy_file TOML
/// (per the per-hardware-anchor table in GENOME-FOUNDRY-SENTINEL.md
/// Part 2 "Hardware Anchors"). A caller that depends on real capacity
/// values today is using `tier_configs_for` before the policy has
/// loaded — that's a startup ordering bug to surface at the caller,
/// not papered over here.
///
/// Eviction policy comes from the canonical per-role table —
/// `EvictionPolicy::canonical_for(role)`. The doc's per-role table:
/// Fast=LruWithinTurn, Warm=LruAcrossTurns{100}, Bench=LfuPlusRecency,
/// Cold=DemandAlignedWithRefinedPreference, Frozen=AppendOnlyGcOnSleep.
pub fn tier_configs_for(hw: &crate::governor::types::HardwareClass) -> Vec<TierConfig> {
    // UMA test: `HardwareClass` doesn't yet expose `uma: bool`
    // explicitly — the implicit invariant from `classify_hardware`
    // (governor/types.rs:372) is `vram_mb == 0` ⇒ UMA, since UMA
    // targets report zero VRAM by spec (the governor draws inference
    // budget from `system_ram_mb`). Discrete GPUs always report a
    // non-zero VRAM. Promoting `uma` to an explicit `HardwareClass`
    // field is a future ergonomics PR; today this implicit contract
    // is what the rest of the substrate relies on.
    let is_uma = hw.vram_mb == 0;

    // The order is invariant — Fast first, Frozen last. Warm sits
    // between Fast and Bench when present.
    let roles: &[TierRole] = if is_uma {
        // UMA: four roles. Warm omitted by design (not "configured to
        // zero" — structurally absent).
        &[
            TierRole::Fast,
            TierRole::Bench,
            TierRole::Cold,
            TierRole::Frozen,
        ]
    } else {
        // Discrete-GPU: full five-role ladder.
        &[
            TierRole::Fast,
            TierRole::Warm,
            TierRole::Bench,
            TierRole::Cold,
            TierRole::Frozen,
        ]
    };

    roles
        .iter()
        .copied()
        .map(|role| TierConfig {
            role,
            capacity: TierCapacity {
                current_used: 0,
                configured_limit: 0,
            },
            eviction: EvictionPolicy::canonical_for(role),
        })
        .collect()
}

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

    // ── tier_configs_for: realization-priority #3 first slice ──
    //
    // GENOME-FOUNDRY-SENTINEL.md Part 2 Hardware Anchors: UMA → 4
    // tier roles (no Warm), discrete-GPU → 5 (full ladder). The
    // structural-absence invariant — "an Fast→Warm eviction on UMA
    // is structurally impossible because there is no Warm tier" — is
    // load-bearing. These tests pin it.

    use crate::governor::types::{HardwareClass, PowerSource, TargetSilicon, ThermalClass};

    /// Minimal fixture for UMA hardware. The realization keys on
    /// `vram_mb == 0` per `classify_hardware`'s implicit contract;
    /// other fields are populated with sensible defaults so the
    /// fixture survives a `HardwareClass` shape extension.
    fn uma_fixture() -> HardwareClass {
        HardwareClass {
            silicon: TargetSilicon::AppleM,
            silicon_model: "M2".to_string(),
            vram_mb: 0,
            system_ram_mb: 16 * 1024,
            power_source: PowerSource::Plugged,
            thermal_class: ThermalClass::ThinAndLight,
            battery_pct: Some(100),
            thermal_headroom_pct: Some(80),
        }
    }

    /// Discrete-GPU fixture. `vram_mb > 0` is the implicit signal
    /// classify_hardware uses to mark a target as non-UMA.
    fn discrete_fixture() -> HardwareClass {
        HardwareClass {
            silicon: TargetSilicon::NvidiaCuda,
            silicon_model: "RTX 5090".to_string(),
            vram_mb: 32 * 1024,
            system_ram_mb: 64 * 1024,
            power_source: PowerSource::Plugged,
            thermal_class: ThermalClass::Workstation,
            battery_pct: None,
            thermal_headroom_pct: Some(90),
        }
    }

    /// THE load-bearing invariant. UMA hardware MUST emit 4 entries
    /// with no Warm — the doc's "Fast→Warm eviction on UMA is
    /// structurally impossible" line is the entire point. If this
    /// regresses, the substrate has lost its hardware-class
    /// portability and Vision Pro / iOS / M-series MacBooks will
    /// page through a no-op tier.
    #[test]
    fn tier_configs_for_uma_has_no_warm() {
        let cfgs = tier_configs_for(&uma_fixture());
        assert_eq!(
            cfgs.len(),
            4,
            "UMA must emit exactly 4 tiers (Fast/Bench/Cold/Frozen), got {cfgs:?}"
        );
        assert!(
            !cfgs.iter().any(|c| c.role == TierRole::Warm),
            "UMA must structurally omit Warm — got {cfgs:?}"
        );
        // Order pin: Fast first, Frozen last.
        assert_eq!(cfgs.first().map(|c| c.role), Some(TierRole::Fast));
        assert_eq!(cfgs.last().map(|c| c.role), Some(TierRole::Frozen));
    }

    /// Discrete-GPU MUST emit all 5 tiers including Warm. The 5090
    /// + Vulkan + CUDA + AMD ROCm all share this shape — Warm is the
    /// PCIe-attached host RAM the accelerator reaches via copy.
    #[test]
    fn tier_configs_for_discrete_has_warm() {
        let cfgs = tier_configs_for(&discrete_fixture());
        assert_eq!(
            cfgs.len(),
            5,
            "discrete-GPU must emit exactly 5 tiers, got {cfgs:?}"
        );
        let roles: Vec<TierRole> = cfgs.iter().map(|c| c.role).collect();
        assert_eq!(
            roles,
            vec![
                TierRole::Fast,
                TierRole::Warm,
                TierRole::Bench,
                TierRole::Cold,
                TierRole::Frozen,
            ],
            "discrete tier order must be Fast/Warm/Bench/Cold/Frozen"
        );
    }

    /// Every emitted TierConfig MUST carry the canonical eviction
    /// policy from `EvictionPolicy::canonical_for(role)`. A future
    /// PR may let policy_file TOML override the policy per-
    /// installation, but the substrate's default-from-role mapping
    /// is the single source for this slice. If it regresses, a
    /// caller that did `EvictionPolicy::canonical_for(c.role)` to
    /// double-check would suddenly disagree with the emitted config.
    #[test]
    fn tier_configs_for_uses_canonical_eviction_policy_per_role() {
        for cfgs in [
            tier_configs_for(&uma_fixture()),
            tier_configs_for(&discrete_fixture()),
        ] {
            for cfg in &cfgs {
                assert_eq!(
                    cfg.eviction,
                    EvictionPolicy::canonical_for(cfg.role),
                    "tier {:?} must carry canonical eviction policy, got {:?}",
                    cfg.role,
                    cfg.eviction
                );
            }
        }
    }

    /// This slice ships SHAPE, not numbers. The configured_limit MUST
    /// be 0 across the board — real values come from policy_file
    /// TOML loading in a follow-up slice. A regression here (a
    /// well-meaning PR baking in default capacity numbers) would
    /// silently establish "wrong" defaults that subsystems start
    /// believing before the policy loader has run.
    #[test]
    fn tier_configs_for_emits_zero_capacity_until_policy_file_loads() {
        for cfgs in [
            tier_configs_for(&uma_fixture()),
            tier_configs_for(&discrete_fixture()),
        ] {
            for cfg in &cfgs {
                assert_eq!(
                    cfg.capacity.current_used, 0,
                    "shape-only slice: current_used must be 0, got {cfg:?}"
                );
                assert_eq!(
                    cfg.capacity.configured_limit, 0,
                    "shape-only slice: configured_limit must be 0 \
                     (policy_file TOML fills this in a follow-up), got {cfg:?}"
                );
            }
        }
    }

    /// The substrate doctrine: subsystems address tiers by role, not
    /// by ordinal. This test exercises that — given an arbitrary
    /// hardware class, find the Fast tier by role lookup. The fact
    /// that this works the SAME WAY on UMA and discrete (despite
    /// different vec lengths) is the architectural point.
    #[test]
    fn tier_configs_for_addressable_by_role_uniformly_across_hardware() {
        for hw in [uma_fixture(), discrete_fixture()] {
            let cfgs = tier_configs_for(&hw);
            let fast = cfgs.iter().find(|c| c.role == TierRole::Fast);
            assert!(fast.is_some(), "Fast must exist on every hardware class");
            let frozen = cfgs.iter().find(|c| c.role == TierRole::Frozen);
            assert!(
                frozen.is_some(),
                "Frozen must exist on every hardware class"
            );
        }
    }
}
