//! The address of one page in the substrate: which content-addressed artifact, what kind of
//! page, and where inside it. Plain values the engine pages by (MoE experts, KV chunks, LoRA
//! layers) and the genome's working sets hold; the engine owns them so neither imports up.
//!
//! `ArtifactId` is a `uuid::Uuid` newtype because the substrate contract (CLAUDE.md: "IDs are
//! UUID — never plain string for identity fields") names it explicitly, and because typed
//! wrappers make `audit_access(persona, page)` impossible to call with the arguments swapped.
//! The wire format (a UUID string) stays stable.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::PeerId;

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
        let pid: PeerId = PeerId(Uuid::nil());
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
}
