//! Per-persona workspace assembly + registry — the "one soul, many rooms" seam.
//!
//! This is the constructor `ai/should-respond` (and the bring-up harness) resolve
//! a persona's mind through. The load-bearing decision (PERSONA-BRAIN-
//! ARCHITECTURE.md §2.9) is structural: **one `WorkspaceCycle` per persona**,
//! keyed by `persona_id` — NOT by `(persona_id, room_id)`. A persona is one
//! continuous self across every room it services; its unified `AdmissionState`
//! (the hippocampus) spans all its activities. Keying the registry by persona is
//! what makes the citizen continuous instead of *severed* per-room.
//!
//! The same cycle is invoked for whatever room the persona is servicing; the room
//! supplies the per-tick world-state (the consolidated burst), the persona
//! supplies the unified memory + identity + faculties.

use continuum_base::time::EpochMs;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

// parking_lot::Mutex, not std::sync::Mutex — matching `working_memory.rs`, this module's
// sibling holding the same kind of per-persona state. std's `lock()` returns a Result that
// is Err only when ANOTHER thread panicked while holding it, so `.lock()` converts
// one persona's failure into a process-wide cascade: the registry every other persona needs
// dies with it. parking_lot does not poison, so there is no Result and no unwrap to get
// wrong. This also collapses two mutex types for one concern back to one.
use parking_lot::Mutex;

use uuid::Uuid;

use crate::identity::{PeerId, PersonaRef};

use super::deferred_faculty::DeferredFaculty;
use super::llm_deliberation_faculty::LlmDeliberationFaculty;
use super::rag_source_faculty::{RagSourceFaculty, SaliencePolicy};
use super::recall_faculty::RecallFaculty;
use super::working_memory::WorkingMemory;
use super::workspace::{ActingBody, Faculty, SituationFocusArbiter, WorkspaceCycle};
use crate::ai::adapter::AIProviderAdapter;
use crate::ai::embedding::{CachingEmbeddingProvider, EmbeddingProvider, LexicalEmbedder};
use crate::cognition::admission_state::AdmissionState;
use crate::cognition::rag_budget::RagSource;

/// Explicit offline adoption of a selected legacy checkpoint; shares this
/// owner's schema, native path, and cross-process checkpoint exclusion.
pub mod checkpoint_adoption;

mod act_output;
mod rehome;
pub use rehome::RehomeReason;

pub use crate::cognition::handoff::SaveReason;

// The tests gate themselves (`#![cfg(test)]` in tests.rs), so the scanner reads the file as test code.
mod tests;

mod default_workspace_capacity;
pub use default_workspace_capacity::*;
mod acting_hands;
pub use acting_hands::*;
mod root_acting_workspace;
pub use root_acting_workspace::*;
mod owner_checkpoint;
pub use owner_checkpoint::OwnerCheckpointError;
mod task_consumption;
pub use task_consumption::{ResidentTaskSink, TaskConsumptionError, TaskSinkError};
mod own_speech_persisted;
pub use own_speech_persisted::VolatileReceipt;
mod mind_results_reporter;
pub use mind_results_reporter::MindResultsReporter;
pub(crate) use own_speech_persisted::*;
