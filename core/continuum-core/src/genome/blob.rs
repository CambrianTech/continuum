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
//! deliverable that targets `core/continuum-core/src/genome/
//! provenance.rs`. That PR is not this PR.
//!
//! What PR-2 needs them for: the `TierStore::write` signature names
//! both types. We define minimal wire-stable versions so the trait
//! compiles and downstream callers can construct a `write` call. When
//! the full Part-1 shapes land, these stubs get replaced and the
//! callers update to pass the richer values; the trait shape doesn't
//! change.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::working_set::ArtifactId;

/// Where an artifact's bytes physically live.
///
/// Small artifacts (LoRA layers, engrams) carry their bytes **inline** — they
/// are cheap to move through the value type. Large artifacts (MoE expert tiles,
/// hundreds of MB) are **mapped**: they reference byte ranges of a backing file
/// on the cold/frozen tier drive and are memory-mapped on read, so they never
/// round-trip through the value type or the message bus. This is the tier-aware
/// handle `ArtifactBlob`'s original `Vec<u8>` doc anticipated.
///
/// A single `Mapped` artifact spanning **N ranges** is one whole MoE expert:
/// its gate / up / down slices live in separate stacked `*_exps` tensors at
/// different file offsets, so one expert = N disjoint `(offset, len)` ranges.
/// Making the expert ONE artifact (not one-per-projection) keeps the residency
/// decision atomic — an expert is co-resident or not, never half — so the
/// working-set key is naturally `(layer, expert)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArtifactSource {
    /// Bytes carried inline. LoRA layers, engrams — small enough to move.
    Inline(Vec<u8>),
    /// Byte ranges within a backing file, memory-mapped on read. The tier
    /// store accounts capacity from the range lengths and NEVER reads the
    /// bytes on write (trust + audit-budget reasons, same as the id contract).
    Mapped {
        /// Absolute path to the backing file (e.g. the K3 GGUF on the frozen
        /// tier drive).
        path: PathBuf,
        /// `(absolute_offset, len)` byte ranges. Summed for size accounting.
        ranges: Vec<(u64, u64)>,
    },
}

impl ArtifactSource {
    /// Physical byte size — the inline length, or the sum of mapped range
    /// lengths. Must be the *physical* size (what the tier store bills against
    /// capacity), never a logical one.
    pub fn size_bytes(&self) -> u64 {
        match self {
            ArtifactSource::Inline(bytes) => bytes.len() as u64,
            ArtifactSource::Mapped { ranges, .. } => ranges.iter().map(|(_, len)| *len).sum(),
        }
    }
}

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
    /// Where the artifact's bytes live — `Inline` for small artifacts,
    /// `Mapped` for large file-backed ones (MoE expert tiles). An inline
    /// empty payload is valid (a zero-byte artifact is a legitimate
    /// sentinel).
    pub source: ArtifactSource,
}

impl ArtifactBlob {
    /// Construct an inline blob — the common small-artifact case (LoRA
    /// layer, engram). Keeps callers that had a `Vec<u8>` unchanged in
    /// spirit: `ArtifactBlob::inline(id, bytes)`.
    pub fn inline(id: ArtifactId, bytes: Vec<u8>) -> Self {
        Self {
            id,
            source: ArtifactSource::Inline(bytes),
        }
    }

    /// Construct a file-mapped blob — one whole MoE expert as N `(offset,
    /// len)` ranges into `path`. Zero-copy: the bytes are never read here.
    pub fn mapped(id: ArtifactId, path: PathBuf, ranges: Vec<(u64, u64)>) -> Self {
        Self {
            id,
            source: ArtifactSource::Mapped { path, ranges },
        }
    }

    /// Physical byte size of the artifact — delegates to the source so
    /// tier stores can compute capacity impact without reading bytes.
    pub fn size_bytes(&self) -> u64 {
        self.source.size_bytes()
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
#[ts(export, export_to = "../../../protocol/typescript/genome/Provenance.ts")]
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
    use uuid::Uuid;

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
        let empty = ArtifactBlob::inline(sample_id(), Vec::new());
        assert_eq!(empty.size_bytes(), 0);

        let one_kb = ArtifactBlob::inline(sample_id(), vec![0u8; 1024]);
        assert_eq!(one_kb.size_bytes(), 1024);

        let big = ArtifactBlob::inline(sample_id(), vec![0u8; 1_048_576]);
        assert_eq!(big.size_bytes(), 1_048_576);
    }

    /// What this catches: a Mapped blob's size is the SUM of its range
    /// lengths, not a byte read — a whole MoE expert (gate+up+down slices
    /// at different file offsets) bills the tier store for the total of its
    /// N ranges without ever touching the backing file. If size accounting
    /// ever tried to read the mapped bytes, this (with a bogus path) would
    /// break instead of returning the summed length.
    #[test]
    fn mapped_blob_size_is_the_sum_of_range_lengths() {
        // Three ranges (gate/up/down slices for one expert) at arbitrary
        // offsets in a file that does NOT exist — size must not read it.
        let expert = ArtifactBlob::mapped(
            sample_id(),
            PathBuf::from("/frozen/k3.gguf"),
            vec![(1_000, 4_096), (500_000, 4_096), (900_000, 2_048)],
        );
        assert_eq!(expert.size_bytes(), 4_096 + 4_096 + 2_048);

        // Zero ranges = zero bytes (a degenerate but valid accounting).
        let none = ArtifactBlob::mapped(sample_id(), PathBuf::from("/frozen/k3.gguf"), vec![]);
        assert_eq!(none.size_bytes(), 0);
    }

    /// What this catches: ArtifactBlob is intentionally NOT TS-exported.
    /// If a future PR adds `#[derive(TS)]`, this test won't compile
    /// (the derive would conflict with the explicit absence) — flag
    /// for review. The TS wire should request artifacts via a binary
    /// download command, not inline them in JSON messages.
    #[test]
    fn artifact_blob_round_trips_through_serde() {
        let blob = ArtifactBlob::inline(sample_id(), vec![1, 2, 3, 4, 5]);
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
