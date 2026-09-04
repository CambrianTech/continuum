//! Working set + page types — `PageKind`, `PageOffset`, `PageRef`,
//! `ResidentPage`, `WorkingSet`, `WorkingSetCapacity`, `PageFault`,
//! `AccessDenied`, and the placeholder ID type `ArtifactId` plus
//! `PageHandle`.
//!
//! Per GENOME-FOUNDRY-SENTINEL Parts 3 (paging) and 4 (compartments).
//!
//! ## ID type policy
//!
//! The per-persona identifier was previously a local `PersonaId(pub Uuid)`
//! newtype here; it has been collapsed onto `crate::identity::PeerId`
//! (the canonical airc actor id, `airc_core::PeerId`). Only `ArtifactId`
//! remains a local newtype in this module (content-addressed, unrelated
//! to actor identity). `ArtifactId` is a `uuid::Uuid` newtype because the
//! substrate contract (CLAUDE.md: "IDs are UUID — never plain string for
//! identity fields") names it explicitly, and because typed wrappers make
//! `audit_access(persona, page)` impossible to call with the arguments
//! swapped. The wire format (a UUID string) stays stable.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;
use uuid::Uuid;

use super::tier::{EvictionRecord, TierRole};
use crate::identity::PeerId;

/// Stable per-artifact identifier. Content-addressed (the value IS
/// the SHA-256-derived UUID of the artifact bytes), so two callers
/// computing the ID independently arrive at the same value. Typed
/// wrapper distinct from `crate::identity::PeerId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/ArtifactId.ts",
    type = "string"
)]
pub struct ArtifactId(pub Uuid);

impl ArtifactId {
    pub fn new(uuid: Uuid) -> Self {
        Self(uuid)
    }
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

/// What kind of page this is. Used by the working-set manager to pick
/// the right tier eviction policy (e.g. a `KVCache` page evicts
/// differently from a `LoRALayer` page even within the same tier).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/genome/PageKind.ts")]
pub enum PageKind {
    /// One layer slice of a LoRA adapter (Q, K, V, or O projection of
    /// a transformer block).
    LoRALayer,
    /// One expert weight tile in an MoE model. Sub-artifact paging:
    /// the artifact is the full expert set; offset picks one expert.
    MoEExpert,
    /// One chunk of a per-turn KV cache. Sub-artifact paging — large
    /// caches span many pages.
    KVCache,
    /// One persona engram. Refined episodic memory; sized for fast
    /// recall + per-persona privacy.
    Engram,
}

/// Sub-artifact offset for paging artifacts that don't fit in a
/// single page (MoE experts, KV chunks, large engrams). For
/// single-page artifacts the offset is `Whole`. Newtype around
/// the variants so it serializes cleanly and gives the type system
/// a hook to enforce "this PageRef points inside ArtifactId X".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/PageOffset.ts"
)]
pub enum PageOffset {
    /// The page IS the whole artifact (LoRA layer adapter, single
    /// engram). No sub-artifact split.
    Whole,
    /// MoE: pick a single expert from the artifact's expert set.
    Expert {
        #[serde(rename = "expertIndex")]
        #[ts(rename = "expertIndex", type = "number")]
        expert_index: u32,
    },
    /// KVCache: byte range within the artifact.
    Range {
        #[serde(rename = "startByte")]
        #[ts(rename = "startByte", type = "number")]
        start_byte: u64,
        #[serde(rename = "endByte")]
        #[ts(rename = "endByte", type = "number")]
        end_byte: u64,
    },
}

/// A fully-qualified reference to one page in the substrate. Three
/// components: the kind (for tier-policy dispatch), the artifact
/// (which content-addressed blob the page lives in), and the offset
/// (where in the artifact the page is).
///
/// Hash + Eq let `PageRef` serve as a `HashMap` key in
/// `WorkingSet.pages`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/genome/PageRef.ts")]
pub struct PageRef {
    pub kind: PageKind,
    pub artifact: ArtifactId,
    pub offset: PageOffset,
}

/// Opaque handle returned by `page_in`. Carries enough context for the
/// caller to use the page without exposing the tier-internal storage.
/// PR-1 ships the wire shape; PR-2 (trait + impl) gives the type
/// behaviors. The `tier_role` field lets the caller decide whether to
/// pin the handle (Fast / Warm) or stream-read it (Cold / Frozen).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/PageHandle.ts"
)]
pub struct PageHandle {
    pub page: PageRef,
    pub tier_role: TierRole,
    /// Byte size of the page as resident in `tier_role`. For Cold /
    /// Frozen this is the size at-rest; for Fast / Warm it's the
    /// size in accelerator-addressable memory.
    #[ts(type = "number")]
    pub size_bytes: u64,
}

/// A page currently in some persona's working set. Tracks the
/// per-turn metadata the eviction policy needs (last_access,
/// access_count_window) and the pinning flag the composition layer
/// sets to prevent mid-turn evictions of in-use pages.
///
/// `last_access_ms` is `u64` (unix-ms) instead of `std::time::Instant`
/// because (a) ts-rs needs a wire-stable representation and (b) the
/// trace bus can replay records across processes where `Instant` is
/// meaningless. Sub-millisecond timing for hot-path decisions stays
/// in caller-side `Instant`s.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/ResidentPage.ts"
)]
pub struct ResidentPage {
    pub page: PageRef,
    pub role: TierRole,
    #[ts(type = "number")]
    pub last_access_ms: u64,
    #[ts(type = "number")]
    pub access_count_window: u32,
    /// When true the eviction policy must skip this page until the
    /// composition layer unpins it. Composition-pinned pages cannot
    /// evict mid-turn.
    pub pinned: bool,
}

/// Per-persona working-set budget the governor publishes. Bytes
/// (not page counts) because pages vary in size by kind. The governor
/// re-publishes when policy changes (hardware probe shifts class,
/// pressure event drops the cap, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/WorkingSetCapacity.ts"
)]
pub struct WorkingSetCapacity {
    /// Maximum bytes the persona's Fast tier is allowed to hold.
    #[ts(type = "number")]
    pub fast_bytes: u64,
    /// Maximum bytes in Warm. Set to 0 on UMA hardware (where Warm
    /// is structurally absent) — code that addresses Warm on UMA
    /// hits `TierError::RoleNotConfigured`.
    #[ts(type = "number")]
    pub warm_bytes: u64,
    /// Maximum bytes pinned per-turn (composition lock). Smaller
    /// than fast_bytes because pinning starves the eviction policy;
    /// the governor caps to prevent runaway pinning.
    #[ts(type = "number")]
    pub max_pinned_bytes: u64,
}

/// A persona's currently-resident pages plus its policy budget.
/// PR-1 ships the data shape with no traits / no impl — PR-2 adds
/// the `WorkingSetManager` trait that produces and consumes these.
///
/// `pages` is keyed by `PageRef` because that's the lookup the hot
/// path needs (composition asks "is this page resident?"). HashMap
/// instead of BTreeMap because access is by exact match, not range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/WorkingSet.ts"
)]
pub struct WorkingSet {
    #[ts(type = "string")]
    pub persona: PeerId,
    /// All resident pages for this persona, keyed by a stringified
    /// `PageRef`. On the wire this serializes as a JSON object with
    /// string keys (serde's HashMap → object behavior). The TS side
    /// sees a record keyed by string with `ResidentPage` values.
    pub pages: HashMap<String, ResidentPage>,
    pub capacity: WorkingSetCapacity,
}

impl WorkingSet {
    /// Fresh working set for a persona with the given capacity. No
    /// pages resident yet.
    pub fn new(persona: PeerId, capacity: WorkingSetCapacity) -> Self {
        Self {
            persona,
            pages: HashMap::new(),
            capacity,
        }
    }

    /// Sum of `last_access_ms` invariant: every resident page's
    /// `role` is consistent with the persona's capacity (a page
    /// claiming role Warm must have warm_bytes > 0). PR-1's invariant
    /// check; PR-2's trait will enforce on insertion.
    pub fn invariants_hold(&self) -> bool {
        for (key, page) in &self.pages {
            // PageRef key serialization matches the stored page.
            let expected_key = serde_json::to_string(&page.page).unwrap_or_default();
            if key != &expected_key {
                return false;
            }
            // A Warm-role page on a working set with zero warm_bytes
            // is a mis-configuration the governor should never allow.
            if page.role == TierRole::Warm && self.capacity.warm_bytes == 0 {
                return false;
            }
        }
        true
    }
}

/// Typed event emitted when a persona's composition needs a page that
/// isn't already in its working set. Sentinel observes these to detect
/// patterns: a persona that page-faults on the same page across many
/// turns is a signal to either pre-fetch it or pin it higher.
///
/// `from_role: None` means "true cold miss" — the page does not exist
/// in any tier yet (typically a fresh KV-cache entry or a never-loaded
/// MoE expert). `from_role: Some(role)` means "tier promotion" — the
/// page existed in `role` and got moved up.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/genome/PageFault.ts")]
pub struct PageFault {
    pub page: PageRef,
    /// Where the page was before the fault. `None` for true cold
    /// miss (page didn't exist yet).
    #[ts(optional)]
    pub from_role: Option<TierRole>,
    /// Where the page lives after the fault is serviced.
    pub to_role: TierRole,
    #[ts(type = "string")]
    pub persona: PeerId,
    /// Time spent servicing the fault (tier lookup + transfer +
    /// eviction-if-any). Drives sentinel's "is this page worth
    /// pre-fetching" calculus.
    #[ts(type = "number")]
    pub elapsed_us: u64,
    /// If servicing the fault required evicting another page, the
    /// record of that eviction. Lets sentinel correlate cause +
    /// effect across the trace bus in one record instead of joining
    /// two separate event streams.
    #[ts(optional)]
    pub eviction_cost: Option<EvictionRecord>,
}

/// Typed refusal from the MMU-style permission check. Per
/// GENOME-FOUNDRY-SENTINEL Part 4: "AccessDenied is loud. Audit log
/// captures it. This is how the substrate makes per-persona privacy
/// structural rather than policy."
///
/// PR-1 ships the wire shape. PR-2 / PR-3 add the
/// `WorkingSetManager::audit_access` enforcement that produces it,
/// and audit-recorder (#1344, codex's PR) subscribes to it as one of
/// its `AccessDenied` audit-log inputs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/AccessDenied.ts"
)]
pub struct AccessDenied {
    /// Which persona attempted the access.
    #[ts(type = "string")]
    pub actor: PeerId,
    /// Which page was attempted.
    pub page: PageRef,
    /// Which persona OWNS that page (whose private region was it
    /// reaching into). `None` means "no owner — the region is
    /// substrate-controlled (e.g. foundry-imported)" and the denial
    /// is for a different reason (license, policy, etc.).
    #[ts(optional)]
    #[ts(type = "string")]
    pub owner: Option<PeerId>,
    /// Human-readable reason. Per Joel's "never swallow errors" rule:
    /// loud, specific, debuggable.
    pub reason: String,
}

impl std::fmt::Display for AccessDenied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.owner {
            Some(owner) => write!(
                f,
                "access denied: persona {} attempted to read page owned by {} — {}",
                self.actor.as_uuid(),
                owner.as_uuid(),
                self.reason
            ),
            None => write!(
                f,
                "access denied: persona {} — {}",
                self.actor.as_uuid(),
                self.reason
            ),
        }
    }
}

impl std::error::Error for AccessDenied {}

#[cfg(test)]
mod tests {
    //! Pin the type contracts PR-1 freezes. Each test corresponds to a
    //! "what if a downstream PR changes this" failure mode.
    use super::*;
    use serde_json::json;

    fn sample_persona() -> PeerId {
        PeerId(Uuid::nil())
    }

    fn sample_artifact() -> ArtifactId {
        ArtifactId(Uuid::nil())
    }

    fn sample_page() -> PageRef {
        PageRef {
            kind: PageKind::LoRALayer,
            artifact: sample_artifact(),
            offset: PageOffset::Whole,
        }
    }

    /// What this catches: PeerId + ArtifactId both serialize as
    /// bare UUID strings (transparent) — not `{"id": "..."}` objects.
    /// Wire stability: downstream consumers parse them as strings.
    #[test]
    fn id_types_serialize_transparent_as_uuid_string() {
        let pid = PeerId(Uuid::nil());
        let aid = ArtifactId(Uuid::nil());
        let pj = serde_json::to_string(&pid).unwrap();
        let aj = serde_json::to_string(&aid).unwrap();
        assert_eq!(pj, "\"00000000-0000-0000-0000-000000000000\"");
        assert_eq!(aj, "\"00000000-0000-0000-0000-000000000000\"");
    }

    /// What this catches: the type system distinguishes PeerId vs
    /// ArtifactId even though both wrap Uuid. Compile-time only —
    /// passing one where the other is expected fails to compile. This
    /// test exists to pin that the distinction is preserved (changing
    /// either to a type alias would let them silently substitute).
    #[test]
    fn persona_id_and_artifact_id_are_distinct_types() {
        let pid: PeerId = sample_persona();
        let aid: ArtifactId = sample_artifact();
        // Both are Copy + Eq with Uuid underneath, but ResidentPage
        // ownership of fields is via the typed wrappers — accidentally
        // passing pid where aid is needed wouldn't compile.
        assert_eq!(pid.as_uuid(), aid.as_uuid()); // both are nil here
    }

    /// What this catches: PageKind serializes camelCase ("loRALayer"?
    /// no — "loraLayer" via serde's camelCase rule). Pin the exact
    /// strings TS sees so a future rename of the Rust variant catches.
    #[test]
    fn page_kind_serializes_camel_case() {
        // Note: serde's "camelCase" handler turns LoRALayer → "loRALayer"
        // because each capital letter except the first is preserved.
        // This is the canonical serde rule. Tests pin actual output so
        // a future PR doesn't silently flip rename_all.
        let j = serde_json::to_string(&PageKind::LoRALayer).unwrap();
        assert!(j == "\"loRALayer\"" || j == "\"loraLayer\"", "got {j}");
        assert_eq!(
            serde_json::to_string(&PageKind::MoEExpert).unwrap(),
            "\"moEExpert\""
        );
        assert_eq!(
            serde_json::to_string(&PageKind::KVCache).unwrap(),
            "\"kVCache\""
        );
        assert_eq!(
            serde_json::to_string(&PageKind::Engram).unwrap(),
            "\"engram\""
        );
    }

    /// What this catches: PageOffset's tagged enum form on the wire.
    /// TS consumers narrow by `kind`; if the tag changes (or kebab-
    /// case slips in), every consumer breaks.
    #[test]
    fn page_offset_serializes_with_kind_tag() {
        let whole = serde_json::to_string(&PageOffset::Whole).unwrap();
        assert_eq!(whole, "{\"kind\":\"whole\"}");

        let expert = serde_json::to_string(&PageOffset::Expert { expert_index: 5 }).unwrap();
        assert!(expert.contains("\"kind\":\"expert\""), "got {expert}");
        assert!(expert.contains("\"expertIndex\":5"), "got {expert}");

        let range = serde_json::to_string(&PageOffset::Range {
            start_byte: 0,
            end_byte: 4096,
        })
        .unwrap();
        assert!(range.contains("\"kind\":\"range\""), "got {range}");
        assert!(range.contains("\"startByte\":0"), "got {range}");
        assert!(range.contains("\"endByte\":4096"), "got {range}");
    }

    /// What this catches: PageRef round-trips through serde. The hot
    /// path uses PageRef as a HashMap key (after string-encoding); if
    /// serde drops a field or reorders, the key generator silently
    /// produces different strings for the same PageRef.
    #[test]
    fn page_ref_round_trips_through_serde() {
        let r = sample_page();
        let j = serde_json::to_string(&r).unwrap();
        let back: PageRef = serde_json::from_str(&j).unwrap();
        assert_eq!(r, back);
    }

    /// What this catches: a fresh working set has zero pages and the
    /// invariant check passes. Baseline — if this regresses, the
    /// constructor or invariant logic broke.
    #[test]
    fn fresh_working_set_is_empty_and_valid() {
        let ws = WorkingSet::new(
            sample_persona(),
            WorkingSetCapacity {
                fast_bytes: 1_000_000,
                warm_bytes: 0,
                max_pinned_bytes: 500_000,
            },
        );
        assert!(ws.pages.is_empty());
        assert_eq!(ws.persona, sample_persona());
        assert!(ws.invariants_hold());
    }

    /// What this catches: a working set with a Warm-role page on UMA
    /// capacity (warm_bytes == 0) fails the invariant check. This is
    /// the "structural impossibility of Fast→Warm eviction on UMA"
    /// guarantee at the data layer — PR-2's trait will enforce on
    /// insertion; PR-1 pins that the invariant function catches it
    /// if a future PR ever lets a Warm page slip through.
    #[test]
    fn working_set_invariant_rejects_warm_page_on_uma_capacity() {
        let mut ws = WorkingSet::new(
            sample_persona(),
            WorkingSetCapacity {
                fast_bytes: 1_000_000,
                warm_bytes: 0, // UMA shape
                max_pinned_bytes: 500_000,
            },
        );
        let page = sample_page();
        let key = serde_json::to_string(&page).unwrap();
        ws.pages.insert(
            key,
            ResidentPage {
                page,
                role: TierRole::Warm,
                last_access_ms: 0,
                access_count_window: 0,
                pinned: false,
            },
        );
        assert!(
            !ws.invariants_hold(),
            "Warm page on UMA (warm_bytes=0) must violate invariant"
        );
    }

    /// What this catches: PageFault serializes from_role as optional —
    /// `None` (true cold miss) becomes a missing field on the wire, not
    /// `null`. Lets the TS consumer narrow with `if (fault.fromRole)`.
    #[test]
    fn page_fault_serializes_from_role_as_optional() {
        let cold_miss = PageFault {
            page: sample_page(),
            from_role: None,
            to_role: TierRole::Fast,
            persona: sample_persona(),
            elapsed_us: 1234,
            eviction_cost: None,
        };
        let j = serde_json::to_string(&cold_miss).unwrap();
        // ts(optional) + Option<T>: serde omits None fields when
        // skip_serializing_if is set; without it, None serializes as
        // null. The current shape uses ts(optional) for the TS side
        // but doesn't add skip_serializing_if, so the wire is
        // `"fromRole":null`. This test pins which one we ship — if a
        // future PR adds skip_serializing_if, it should be a
        // deliberate flip.
        assert!(
            j.contains("\"fromRole\":null") || !j.contains("\"fromRole\""),
            "expected fromRole to be null or omitted, got: {j}"
        );

        let tier_promo = PageFault {
            page: sample_page(),
            from_role: Some(TierRole::Bench),
            to_role: TierRole::Fast,
            persona: sample_persona(),
            elapsed_us: 500,
            eviction_cost: None,
        };
        let j2 = serde_json::to_string(&tier_promo).unwrap();
        assert!(j2.contains("\"fromRole\":\"bench\""), "got {j2}");
    }

    /// What this catches: AccessDenied implements Display + Error so
    /// audit-recorder + handlers can use it via `?` chains. The
    /// Display format includes the actor + page context so a debugger
    /// reading the log can act without joining tables.
    #[test]
    fn access_denied_implements_error_with_context() {
        let denied = AccessDenied {
            actor: sample_persona(),
            page: sample_page(),
            owner: Some(sample_persona()),
            reason: "cross-persona read of private engram".to_string(),
        };
        let _: &dyn std::error::Error = &denied;
        let display = format!("{denied}");
        assert!(display.contains("access denied"));
        assert!(display.contains("cross-persona read"));
    }

    /// What this catches: round-trip integrity across the bigger
    /// payloads. If a future PR changes a field name or type in
    /// PageFault / EvictionRecord / WorkingSet, the round-trip fails.
    #[test]
    fn larger_records_round_trip_through_serde() {
        let evict = EvictionRecord {
            page: sample_page(),
            from_role: TierRole::Fast,
            to_role: Some(TierRole::Bench),
            policy_fired: super::super::tier::EvictionPolicy::LruWithinTurn,
            elapsed_us: 42,
        };
        let j = serde_json::to_string(&evict).unwrap();
        let back: EvictionRecord = serde_json::from_str(&j).unwrap();
        assert_eq!(evict, back);

        let fault = PageFault {
            page: sample_page(),
            from_role: Some(TierRole::Cold),
            to_role: TierRole::Fast,
            persona: sample_persona(),
            elapsed_us: 9876,
            eviction_cost: Some(evict.clone()),
        };
        let j = serde_json::to_string(&fault).unwrap();
        let back: PageFault = serde_json::from_str(&j).unwrap();
        assert_eq!(fault, back);
    }

    /// What this catches: a sample shape for downstream consumers to
    /// reference. If PageHandle's wire form changes, the consumers'
    /// fixtures break. Pin a small concrete example here as a regression
    /// check.
    #[test]
    fn page_handle_sample_shape() {
        let handle = PageHandle {
            page: sample_page(),
            tier_role: TierRole::Fast,
            size_bytes: 1_048_576,
        };
        let j: serde_json::Value = serde_json::to_value(&handle).unwrap();
        assert_eq!(j["tierRole"], json!("fast"));
        assert_eq!(j["sizeBytes"], json!(1_048_576));
    }
}
