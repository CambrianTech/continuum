//! Provisioning — one Rust loader/cache for every downloadable artifact.
//!
//! See `docs/architecture/PROVISIONING-SYSTEM.md`. This replaces the pile of
//! install/download shell scripts with ONE abstraction: a model download and an
//! avatar download are the same operation over different catalogs.
//!
//! ## It's a CACHE, not just a downloader
//!
//! Disk is finite on the misfit grid (a MacBook Air is not the 16 TB store). The
//! artifact store is a **cache**: the currently-needed set (active personas' models,
//! their avatars/voices) is PINNED and guaranteed present ("we need what we need"),
//! while everything else is evictable when space runs short — LRU/least-needed first,
//! the same shape as the genome pager. So every `ArtifactSource` must answer not just
//! "where does this come from" but "is it on disk, and how big" (`disk_state`) — the
//! reasoning primitive the cache manager (a later slice) needs to budget + evict.

use std::path::PathBuf;

pub mod avatar_source;
pub mod cache;
pub mod downloader;
pub mod fetch;
pub mod model_catalog;
pub mod model_source;
pub mod placement_planner;
pub mod provisioner;
pub mod scaling;

pub use avatar_source::AvatarSource;
pub use cache::{reconcile, CacheDecision, CacheEntry, ProvisionPlan};
pub use downloader::{DownloadError, Downloader};
pub use fetch::{fetch_and_place, FetchError};
pub use model_catalog::{
    budget_for_mode, parse_quant, plan_family_fetch, plan_model_fetch, provision_model,
    select_best_fit, select_for_mode, serving_mode_for_pressure, CatalogError, GgufCandidate,
    ModelFamily, ModelFetchPlan, PowerMode, ProvisionModelError,
};
pub use model_source::ModelSource;
pub use placement_planner::{
    grid_has_fit, resolve_from_footprint, resolve_placement, PlacementResolution,
};
pub use provisioner::{EvictionReport, Provisioner};
pub use scaling::{DefaultScalingPolicy, DemandContext, ScalingPolicy};

/// What kind of artifact this is — for routing + human-readable provisioning reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    /// LLM / VLM / embedding weights (GGUF).
    Model,
    /// A VRM/GLB avatar mesh.
    Avatar,
    /// A TTS/voice model.
    Voice,
    /// An executable the runtime shells out to (e.g. llama-server).
    Binary,
}

/// How the `Downloader` (a later slice) must fetch `url`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// A Hugging Face repo — resolve the file(s) + fetch (the model path).
    HfFile,
    /// A zip archive to download + extract (the VRoid VRM path).
    Zip,
    /// A direct file URL.
    Direct,
}

/// One downloadable artifact, projected from an `ArtifactSource`'s catalog. The
/// single source of truth for "where does this come from + how do I verify it" —
/// never a hardcoded local path (that's DERIVED via `disk_state`).
#[derive(Debug, Clone)]
pub struct ArtifactSpec {
    /// Stable id (matches the catalog id — the resolver key).
    pub id: String,
    /// Where to fetch it from.
    pub url: String,
    /// How to fetch it.
    pub source_kind: SourceKind,
    /// On-disk size in bytes, if known ahead of download — the cache budget input.
    /// Often `None` until the file exists (HF file sizes need a HEAD; the quant
    /// footprint is param-count-derivable for models — a later refinement).
    pub size_bytes: Option<u64>,
    /// Content checksum for verify-after-download, if the catalog pins one.
    pub checksum: Option<String>,
    /// License identifier (e.g. "CC0").
    pub license: Option<String>,
}

/// Whether an artifact is on disk right now, and how big — the cache-reasoning
/// primitive. The path is always DERIVED (from the id / catalog), never hardcoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiskState {
    /// Present locally — the cache can serve it, or evict it if unpinned.
    Present { path: PathBuf, bytes: u64 },
    /// Not on disk — the provisioner must fetch it if it's needed.
    Absent,
}

impl DiskState {
    pub fn is_present(&self) -> bool {
        matches!(self, DiskState::Present { .. })
    }
    /// Bytes this artifact occupies on disk (0 if absent) — the cache's reclaim input.
    pub fn bytes(&self) -> u64 {
        match self {
            DiskState::Present { bytes, .. } => *bytes,
            DiskState::Absent => 0,
        }
    }
}

/// A catalog of one kind of downloadable artifact. The ONE abstraction models,
/// avatars, voices, and binaries all implement — so the `Downloader` +
/// `Provisioner` (later slices) treat them uniformly instead of a bespoke shell
/// script each. Two maximally-different impls (`AvatarSource` = zip/VRM,
/// `ModelSource` = HF/GGUF) validate the abstraction before the rest exist.
pub trait ArtifactSource: Send + Sync {
    /// What kind of artifact this source provides.
    fn kind(&self) -> ArtifactKind;

    /// The full catalog — the single source of truth for what's available + where
    /// it comes from. Adding an artifact is one catalog entry (the #1871 rule).
    fn catalog(&self) -> Vec<ArtifactSpec>;

    /// Is artifact `id` on disk right now, and how big? The cache-reasoning primitive
    /// — the path is DERIVED, never a hardcoded absolute path.
    fn disk_state(&self, id: &str) -> DiskState;
}

/// Helper for impls: the on-disk size of a file, or `Absent` if it doesn't exist.
pub(crate) fn disk_state_of(path: PathBuf) -> DiskState {
    match std::fs::metadata(&path) {
        Ok(meta) if meta.is_file() => DiskState::Present {
            bytes: meta.len(),
            path,
        },
        _ => DiskState::Absent,
    }
}
