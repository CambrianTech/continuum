//! Curated Rust model catalog.
//!
//! Runtime model truth lives here, not in TypeScript maps or editable TOML.
//! Discovery may propose candidates elsewhere; admission only chooses from
//! this vetted catalog.

use super::registry::{Registry, RegistryError};
use super::types::{
    Arch, AuthKind, Capability, Model, ModelSampling, ModelServingPrefs, MultiPartyChatStrategy, Provider,
    ProviderCapabilities, ProviderKind, ToolProtocol,
};
use std::collections::BTreeSet;
use std::path::PathBuf;

mod qwen35_chat_template;
pub use qwen35_chat_template::*;
mod models;
pub use models::*;
mod providers;
pub use providers::*;
