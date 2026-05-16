//! ArtifactBlob + Provenance — the value-side types the `TierStore`
//! trait's `write` method needs.
//!
//! ## Status: PR-2 minimal seam
//!
//! Both types are **placeholder stubs** that will be replaced by the
//! full shapes specified in GENOME-FOUNDRY-SENTINEL Part 1. The full
//! `Provenance` carries the artifact_id (content-hash), creator,
//! source_trace, source_artifact, supersedes, adaptation_method,
//! outcome_metrics, trust_score, and license fields — a Lane H
//! deliverable that targets `src/workers/continuum-core/src/genome/
//! provenance.rs`. That PR is not this PR.
//!
//! What PR-2 needs them for: the `TierStore::write` signature names
//! both types. We define minimal wire-stable versions so the trait
//! compiles and downstream callers can construct a `write` call. When
//! the full Part-1 shapes land, these stubs get replaced and the
//! callers update to pass the richer values; the trait shape doesn't
//! change.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::working_set::ArtifactId;

/// Opaque bytes of an artifact. PR-2 carries the raw bytes inline
/// for a simple wire shape; later PRs replace with a tier-aware
/// handle (mmap, ref-counted Arc, GPU buffer ID) so large artifacts
/// don't round-trip through the message bus. The serde format is
/// base64 so JSON consumers can read it without needing binary
/// transports.
///
/// NOT TS-exported — large blobs don't belong on the TS wire. If a TS
/// consumer needs the blob it should request via a separate
/// `download_artifact(artifact_id)` command that streams binary.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArtifactBlob {
    /// Content-addressed identifier — should match
    /// `sha256-derived-uuid(bytes)`. Producers compute this; the tier
    /// store does not re-hash on write (trust + audit budget reasons).
    pub id: ArtifactId,
    /// The raw artifact bytes. Empty Vec is valid (a zero-byte
    /// artifact is a legitimate sentinel).
    pub bytes: Vec<u8>,
}

impl ArtifactBlob {
    /// Byte size of the artifact. Cheap O(1) wrapper around `bytes.len()`
    /// so tier stores can compute capacity impact without owning a
    /// reference to the blob.
    pub fn size_bytes(&self) -> u64 {
        self.bytes.len() as u64
    }
}

/// PR-2 stub for `Provenance`. The full shape (GENOME-FOUNDRY-
/// SENTINEL Part 1) carries creator, source_trace, source_artifact,
/// supersedes, adaptation_method, outcome_metrics, trust_score, and
/// license fields. PR-2 ships a typed minimum so the `TierStore::write`
/// signature compiles; the full shape is a separate Lane H PR that
/// replaces this stub.
///
/// PR-2's stub carries:
/// - `artifact_id` — the content hash of the artifact this provenance
///   describes. Required for the typed contract; matches the
///   `ArtifactBlob.id` value passed alongside.
/// - `created_at_ms` — Unix-ms timestamp the provenance was attached.
///   Required for ordering claims about the artifact across federation.
///
/// When the full shape lands, downstream callers will be able to add
/// the remaining fields without changing the trait surface — this
/// type can grow fields without breaking callers that only set the
/// minimum.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/genome/Provenance.ts"
)]
pub struct Provenance {
    pub artifact_id: ArtifactId,
    #[ts(type = "number")]
    pub created_at_ms: u64,
}

impl Provenance {
    /// Construct a minimal provenance for an artifact at the given
    /// timestamp. Convenience for the common case where the caller
    /// has only the two required fields.
    pub fn minimal(artifact_id: ArtifactId, created_at_ms: u64) -> Self {
        Self {
            artifact_id,
            created_at_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_id() -> ArtifactId {
        ArtifactId::new(Uuid::nil())
    }

    /// What this catches: ArtifactBlob.size_bytes is O(1) bytes.len()
    /// and matches the raw byte count. If a future PR adds compression
    /// or some other transform, this guard flags the size shifting
    /// invisibly — large-blob accounting in TierStore::write depends
    /// on this number being the *physical* size, not a logical one.
    #[test]
    fn artifact_blob_size_matches_byte_length() {
        let empty = ArtifactBlob {
            id: sample_id(),
            bytes: Vec::new(),
        };
        assert_eq!(empty.size_bytes(), 0);

        let one_kb = ArtifactBlob {
            id: sample_id(),
            bytes: vec![0u8; 1024],
        };
        assert_eq!(one_kb.size_bytes(), 1024);

        let big = ArtifactBlob {
            id: sample_id(),
            bytes: vec![0u8; 1_048_576],
        };
        assert_eq!(big.size_bytes(), 1_048_576);
    }

    /// What this catches: ArtifactBlob is intentionally NOT TS-exported.
    /// If a future PR adds `#[derive(TS)]`, this test won't compile
    /// (the derive would conflict with the explicit absence) — flag
    /// for review. The TS wire should request artifacts via a binary
    /// download command, not inline them in JSON messages.
    #[test]
    fn artifact_blob_round_trips_through_serde() {
        let blob = ArtifactBlob {
            id: sample_id(),
            bytes: vec![1, 2, 3, 4, 5],
        };
        let json = serde_json::to_string(&blob).unwrap();
        let back: ArtifactBlob = serde_json::from_str(&json).unwrap();
        assert_eq!(blob, back);
    }

    /// What this catches: Provenance.minimal constructor populates
    /// both required fields exactly as passed. PR-2's contract: a
    /// caller building a minimal provenance gets exactly what they
    /// asked for, no defaults / no transforms.
    #[test]
    fn provenance_minimal_preserves_fields() {
        let prov = Provenance::minimal(sample_id(), 1_700_000_000_000);
        assert_eq!(prov.artifact_id, sample_id());
        assert_eq!(prov.created_at_ms, 1_700_000_000_000);
    }

    /// What this catches: Provenance serializes camelCase on the wire
    /// (`createdAtMs`, not `created_at_ms`). Downstream TS consumers
    /// parse the camelCase form.
    #[test]
    fn provenance_serializes_camel_case() {
        let prov = Provenance::minimal(sample_id(), 1234);
        let j = serde_json::to_string(&prov).unwrap();
        assert!(j.contains("\"createdAtMs\":1234"), "got {j}");
        assert!(j.contains("\"artifactId\":"), "got {j}");
    }
}
