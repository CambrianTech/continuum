//! Model registry — single source of truth for model + provider metadata.
//!
//! Replaces scattered `ModelInfo` entries, per-model HashMap literals,
//! TypeScript registries, and `match arch { "qwen35" => ... }` branches.
//! Runtime code consumes capabilities and requirements, not provider strings.
//!
//! This module is the one place allowed to know curated model facts.
//!
//! Invariants:
//! - Nothing outside this module should own specific model facts.
//! - Enum variants (`Arch`, `Capability`, `AuthKind`) are the closed
//!   vocabulary. Adding a model with a new arch means adding an `Arch::`
//!   variant and one catalog row.
//! - The Rust catalog (`catalog.rs`) is the ONLY hand-authored source.
//!   There is no TOML loader. Hand-authoring is for the residue no query
//!   can supply; everything else hydrates from artifact metadata.

pub mod artifacts;
pub mod catalog;
pub mod discovery;
pub mod hydrate;
pub mod live;
pub mod registry;
pub mod singleton;
pub mod types;

pub use artifacts::{
    expand_user_path, find_first_local_gguf, resolve_gguf_for_model, resolve_gguf_for_model_id,
    resolve_local_model_dir_for_model_id,
};
pub use catalog::{models as catalog_models, providers as catalog_providers};
pub use registry::{Registry, RegistryError};
pub use singleton::{global, init_global, try_global};
pub use types::{Arch, AuthKind, Capability, Model, Provider, ProviderKind, ToolProtocol};
