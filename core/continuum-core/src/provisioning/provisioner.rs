//! The Provisioner — the orchestrator that composes the `ArtifactSource`s (slice 1)
//! with the cache `reconcile` (slice 2) into one decision across ALL artifact types.
//!
//! This is the "own the catalog" seam: one object that knows every downloadable
//! artifact (models + avatars + voices…), what's on disk, and — given a demand plan
//! + a disk budget — what to fetch and what to evict. The `Downloader` (fetch) and
//! the eviction I/O (delete) are later slices that ACT on the returned decision; a
//! `cu provision` command drives it, and the core self-provisions on launch.

use super::{
    reconcile, ArtifactSource, AvatarSource, CacheDecision, CacheEntry, ModelSource, ProvisionPlan,
};

/// Owns the artifact catalog across all sources + turns a demand plan into a cache
/// decision. Sources are trait objects so voices/binaries slot in with no changes.
pub struct Provisioner {
    sources: Vec<Box<dyn ArtifactSource>>,
}

impl Default for Provisioner {
    fn default() -> Self {
        Self::with_default_sources()
    }
}

impl Provisioner {
    /// The production set of sources: models + avatars (voices/binaries land here as
    /// they're built — one line each, the whole point of the abstraction).
    pub fn with_default_sources() -> Self {
        Self {
            sources: vec![Box::new(ModelSource), Box::new(AvatarSource)],
        }
    }

    /// For tests / custom topologies.
    pub fn new(sources: Vec<Box<dyn ArtifactSource>>) -> Self {
        Self { sources }
    }

    /// Every catalogued artifact id across all sources — the full known catalog.
    pub fn all_ids(&self) -> Vec<String> {
        self.sources
            .iter()
            .flat_map(|s| s.catalog().into_iter().map(|spec| spec.id))
            .collect()
    }

    /// Reconcile the whole store against `plan` + `budget_bytes`: enumerate every
    /// artifact, mark the plan's `needed` ids as PINNED, read each one's on-disk state,
    /// and run the cache reconcile. Pure — returns the decision; performs no I/O.
    pub fn plan_reconcile(&self, plan: &ProvisionPlan, budget_bytes: u64) -> CacheDecision {
        let mut entries = Vec::new();
        for source in &self.sources {
            for spec in source.catalog() {
                let disk = source.disk_state(&spec.id);
                let pinned = plan.needed.iter().any(|n| n == &spec.id);
                entries.push(CacheEntry::new(spec.id, disk, pinned));
            }
        }
        reconcile(&entries, budget_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provisioning::{ArtifactKind, ArtifactSpec, DiskState, SourceKind};
    use std::path::PathBuf;

    /// A stand-in source with fully-controlled disk states, so the orchestrator's
    /// pin/fetch/evict wiring is testable without touching the real catalogs.
    struct MockSource {
        kind: ArtifactKind,
        specs: Vec<(String, DiskState)>,
    }
    impl ArtifactSource for MockSource {
        fn kind(&self) -> ArtifactKind {
            self.kind
        }
        fn catalog(&self) -> Vec<ArtifactSpec> {
            self.specs
                .iter()
                .map(|(id, _)| ArtifactSpec {
                    id: id.clone(),
                    url: "http://x".into(),
                    source_kind: SourceKind::Direct,
                    size_bytes: None,
                    checksum: None,
                    license: None,
                })
                .collect()
        }
        fn disk_state(&self, id: &str) -> DiskState {
            self.specs
                .iter()
                .find(|(i, _)| i == id)
                .map(|(_, d)| d.clone())
                .unwrap_or(DiskState::Absent)
        }
    }

    fn present(bytes: u64) -> DiskState {
        DiskState::Present { path: PathBuf::from("/x"), bytes }
    }

    // what this catches: the orchestrator threads pin (from the plan) + disk_state
    // (from each source) into the reconcile — so a needed-but-absent model is FETCHED,
    // an unpinned cached avatar is EVICTED to fit the budget, and the pinned model is
    // kept. Proves models + avatars reconcile TOGETHER through one call.
    #[test]
    fn provisioner_reconciles_across_sources_with_a_plan() {
        let models = MockSource {
            kind: ArtifactKind::Model,
            specs: vec![
                ("brain-here".into(), present(60)),
                ("brain-missing".into(), DiskState::Absent),
            ],
        };
        let avatars = MockSource {
            kind: ArtifactKind::Avatar,
            specs: vec![("old-face".into(), present(50))],
        };
        let prov = Provisioner::new(vec![Box::new(models), Box::new(avatars)]);
        let plan = ProvisionPlan {
            needed: vec!["brain-here".into(), "brain-missing".into()],
        };
        // used = 60 (brain-here) + 50 (old-face) = 110; budget 80.
        let d = prov.plan_reconcile(&plan, 80);
        assert_eq!(d.fetch, vec!["brain-missing".to_string()], "needed+absent → fetch");
        assert_eq!(d.evict, vec!["old-face".to_string()], "unpinned avatar evicted to fit");
        assert!(!d.is_shortfall(), "60 pinned fits the 80 budget after eviction");
    }

    // what this catches: default sources compose without panicking and the coder-14b
    // teacher is in the known catalog — the real orchestrator is wired.
    #[test]
    fn default_provisioner_knows_the_real_catalog() {
        let prov = Provisioner::with_default_sources();
        let ids = prov.all_ids();
        assert!(ids.iter().any(|i| i == "continuum-ai/qwen2.5-coder-14b-instruct-GGUF"));
        assert!(ids.iter().any(|i| i.starts_with("vroid-")), "avatars present too");
    }
}
