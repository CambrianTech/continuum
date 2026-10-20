//! ForgeModule — IPC commands for the foundry pipeline.
//!
//! Phase 4 of continuum#1164 (design at FORGE-RECIPE-AS-ENTITY.md).
//! v1 is a stub: `forge/run` accepts a `ForgeRecipe` payload and
//! returns a synthetic `ForgeArtifact` populated with placeholder
//! execution outputs. Real stage execution (prune / train / lora /
//! quant / eval) lands in Phase 5+ when the foundry executor is
//! ported into Rust.
//!
//! Commands:
//! - `forge/run`: Take a ForgeRecipe + hardware node label, return a
//!   stub ForgeArtifact with `recipe_id` lineage + `forged_at_ms`
//!   timestamp + an `alloy_hash` derived from the recipe's content
//!   hash. Caller persists the artifact via `data/upsert` against
//!   the `forge_artifacts` collection (Phase 3 #1180 wired the entity
//!   registration).
//!
//! Stub semantics for Phase 4:
//! - No models are loaded.
//! - No stages execute.
//! - No HuggingFace publishing.
//! - The artifact's `results` / `receipt` / `integrity` fields stay
//!   `None`. `hardware_verified` is empty.
//! - `alloy_hash` is `"sha256:stub-<recipe_id_short>"` so the
//!   placeholder is identifiable but doesn't collide with real hashes.
//!
//! This proves the IPC reachability + recipe→artifact transformation
//! shape end-to-end without claiming to forge anything. Phase 5
//! replaces the stub with the real executor.

use crate::forge::custodian_client::ForgeCustodianHttp;
use crate::forge::{ForgeArtifact, ForgeRecipe};
use crate::runtime::{
    CommandResult, MessageBus, ModuleConfig, ModuleContext, ModulePriority, ServiceModule,
};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::any::Any;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

#[cfg(test)]
mod tests;

mod forge_module;
pub use forge_module::*;
mod stays;
pub use stays::*;
