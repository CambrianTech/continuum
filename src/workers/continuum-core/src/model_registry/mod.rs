//! Model registry — single source of truth for model + provider metadata.
//!
//! Replaces the dozens of hardcoded `ModelInfo` entries, per-model
//! HashMap literals, and `match arch { "qwen35" => ... }` branches
//! scattered across `ai/` and `inference/`. Adding a new model is a
//! TOML row. Code consumes *capabilities*, not identity.
//!
//! Joel's rule (2026-04-20): "code should NEVER (other than ONE place)
//! be allowed to know the model. config gives it."
//!
//! This module IS the ONE place.
//!
//! Invariants:
//! - Nothing outside this module knows any specific model ID or arch
//!   string. Callers ask for a `Model` by id (opaque string from config)
//!   and check capabilities.
//! - Enum variants (`Arch`, `Capability`, `AuthKind`) are the closed
//!   vocabulary. Adding a model with a new arch means adding an `Arch::`
//!   variant AND a TOML row — but the TOML rows for existing arches
//!   remain unaffected.

pub mod types;
pub mod loader;
pub mod singleton;

pub use types::{Arch, AuthKind, Capability, Model, Provider};
pub use loader::{Registry, RegistryError, load_registry, load_models, load_providers};
pub use singleton::{global, init_global};
