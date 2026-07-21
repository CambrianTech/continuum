//! Forge — recipe-as-entity and foundry artifact types.
//!
//! Per the design at `docs/architecture/FORGE-RECIPE-AS-ENTITY.md`
//! (continuum#1164/#1165). Phase 1a: pure value types (recipe, artifact,
//! and supporting structs). Phase 1b: rename existing TS-side `ForgeAlloy`
//! to `ForgeArtifact` across the 15 referencing files. Phase 2: typed
//! `RecipeStage` enum and typed `AlloyResults`/`AlloyReceipt`/
//! `IntegrityAttestation` (currently `serde_json::Value` blobs). Phase 3:
//! entity registry registration plus the `forge/run` IPC.

pub mod adapter_manifest;
pub mod artifact;
pub mod custodian_client;
pub mod custodian_supervisor;
pub mod endpoint;
pub mod gene_handle;
pub mod grid_custodian;
pub mod hf_publisher;
pub mod lora_convert;
pub mod mlx_job;
pub mod mlx_train;
pub mod protocol;
pub mod publish_request;
pub mod publish_tags;
pub mod publisher;
pub mod recipe;

pub use artifact::{ForgeArtifact, HardwareProfile};
pub use endpoint::{can_accept_gguf_lora, ForgeEndpoint, ForgeHealth, ForgeLocator};
pub use grid_custodian::{GridDispatch, GridDispatchError, GridForgeCustodian};
pub use gene_handle::{AlloyHash, GeneHandle, GeneLocator};
pub use recipe::{
    AlloyHardware, AlloySource, BenchmarkDef, CorpusRef, ForgeRecipe, PriorBaseline, QuantTier,
};
