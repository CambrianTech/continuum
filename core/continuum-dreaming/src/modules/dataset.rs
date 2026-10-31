//! Dataset Module — Import, manage, and query training datasets.
//!
//! All heavy data processing (CSV parsing, JSONL conversion, file I/O)
//! happens here in Rust off the main thread. TypeScript is a thin API layer.
//!
//! Supports RealClassEval (arxiv:2510.26130) and generic CSV imports.
//!
//! The six `dataset/*` verbs are typed [`ActionCommand`](crate::sdk_codegen::ActionCommand)s
//! living under `commands/dataset/`; this module owns the [`DatasetService`] they
//! capture and exposes them via [`ServiceModule::commands`]. The legacy
//! `handle_command` match is retired — it now fails loud.

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::any::Any;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use ts_rs::TS;

mod candidate;
pub use candidate::{CandidateReservation, DatasetCandidate};

mod dataset_manifest;
pub use dataset_manifest::*;
mod dataset_service;
pub use dataset_service::*;
mod turn_to_example;
use turn_to_example::*;

#[cfg(test)]
mod tests;
