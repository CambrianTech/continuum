//! The Global Workspace — the brain's integration core.
//!
//! See `docs/architecture/PERSONA-BRAIN-ARCHITECTURE.md`. Cognition is a
//! federation of **faculties** (swappable ML adapters). Each service tick (over
//! a *consolidated burst*, never per-event) the faculties bid in parallel into
//! a bounded **Workspace**; a pluggable **Arbiter** integrates their bids
//! (attention), and the winners are broadcast. The persona's participation
//! **Decision** is the *output of the deliberation faculty's thinking* over that
//! workspace — never a heuristic gate, never an `@`-trigger, never a sender caste.
//!
//! This is built ALONGSIDE the live loop; it does not yet replace
//! `calculate_priority`/`fast_path`. Cut-over lands once it's tested + the
//! recipe-executor (the servicing substrate) calls into it.
//!
//! ## Interface contract (for the recipe-executor `ai/should-respond` step)
//! - `Faculty::contribute` is **async** (backends do inference/IPC).
//! - It consumes `&Workspace` (the consolidated world-state + current broadcast).
//! - It returns `Option<Contribution>` (`None` = abstain this tick).
//! - Faculties are **per-persona instances** (each mind owns its faculties);
//!   model *backends* may be shared. Look one up by `FacultyId`.
//! - The participation result is a typed [`Decision`] carried by the
//!   deliberation faculty's contribution.

use std::sync::Arc;

use async_trait::async_trait;
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::llm_deliberation_faculty::{
    empty_genome, relaxed_decoding, DecodingHandle, GenomeHandle, ModelBinding, ModelBindingHandle,
};
use crate::ai::types::{ActiveAdapterRequest, ToolCall};

mod capacity;
pub use capacity::*;
mod edit;
pub use edit::*;
mod cause;
pub use cause::*;
mod arbiter;
pub use arbiter::*;
mod workspace_cycle;
mod workspace_cycle_2;
mod render_custody;

#[cfg(test)]
mod tests;
