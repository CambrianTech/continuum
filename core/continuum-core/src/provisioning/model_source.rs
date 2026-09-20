//! `ArtifactSource` for model weights — the OUTLIER that validates the abstraction.
//!
//! Maximally different from `AvatarSource`: HF-repo source (not a zip URL), GGUF on
//! disk (not a VRM), path resolved by identity-token match under the model roots +
//! HF cache (not a filename lookup). If the ONE `ArtifactSource` trait fits both the
//! avatar catalog AND this without forcing, the abstraction is proven and voices /
//! binaries are trivial.

use super::{disk_state_of, ArtifactKind, ArtifactSource, ArtifactSpec, DiskState, SourceKind};
use crate::model_registry::artifacts::resolve_gguf_for_model_id;
use crate::model_registry::catalog;

/// Provisions GGUF model weights from `model_registry::catalog`. Only rows with a
/// `gguf_hint` are provisionable artifacts (a download source); cloud/API models have
/// no local weight to cache and are skipped.
pub struct ModelSource;

impl ArtifactSource for ModelSource {
    fn kind(&self) -> ArtifactKind {
        ArtifactKind::Model
    }

    fn catalog(&self) -> Vec<ArtifactSpec> {
        catalog::models()
            .into_iter()
            .filter_map(|m| {
                let url = m.gguf_hint?; // no hint ⇒ not a downloadable artifact
                Some(ArtifactSpec {
                    id: m.id,
                    url,
                    source_kind: SourceKind::HfFile,
                    // GGUF footprint is param×quant-derivable; known exactly once on
                    // disk. Left None here (a later refinement feeds the cache budget).
                    size_bytes: None,
                    checksum: None,
                    license: None,
                })
            })
            .collect()
    }

    fn disk_state(&self, id: &str) -> DiskState {
        // resolve_gguf_for_model_id DERIVES the path (id-token match under
        // ~/.continuum/genome/models/ → HF cache via hint) — never a hardcoded path.
        match resolve_gguf_for_model_id(id) {
            Some(path) => disk_state_of(path),
            None => DiskState::Absent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the model catalog projects onto the SAME ArtifactSpec the
    // avatar catalog does — proving the trait fits a maximally-different source
    // (HF/GGUF vs zip/VRM) without avatar- or model-specific shims. Every provisionable
    // model yields a non-empty id + an HF url, and the teacher (coder-14b) is present.
    #[test]
    fn model_source_projects_provisionable_models() {
        let src = ModelSource;
        assert_eq!(src.kind(), ArtifactKind::Model);
        let specs = src.catalog();
        assert!(!specs.is_empty(), "no provisionable models");
        for s in &specs {
            assert!(!s.id.is_empty(), "spec has no id");
            assert!(!s.url.is_empty(), "{} has no url", s.id);
            assert_eq!(s.source_kind, SourceKind::HfFile);
        }
        assert!(
            specs
                .iter()
                .any(|s| s.id == "continuum-ai/qwen2.5-coder-14b-instruct-GGUF"),
            "the coder-14b teacher must be a provisionable artifact"
        );
    }

    // what this catches: disk_state for an unknown model id is Absent (no panic, no
    // hardcoded path) — the cache-reasoning primitive works for the outlier too.
    #[test]
    fn model_disk_state_absent_for_unknown_id() {
        let src = ModelSource;
        assert_eq!(src.disk_state("no-such-model-xyz"), DiskState::Absent);
    }
}
