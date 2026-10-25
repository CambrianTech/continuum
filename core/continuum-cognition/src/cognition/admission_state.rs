//! Per-Persona Admission State (continuum#1121 PR-4)
//!
//! Owns the per-persona admission machinery + the in-memory side-effect
//! stores that turn the stateless runner from PR-3 into a stateful loop.
//! This is the bridge between the IPC layer (`cognition/admit-inbox-message`)
//! and the pure-Rust admission gate from PRs 1-3.
//!
//! # What ships
//!
//! - [`AdmissionState`] — bundles a `InboxAdmissionRunner<HeuristicIsMemorable>`
//!   plus in-memory `SeenContentLookup` + `SeenEventLookup` impls plus a
//!   simple `Vec<Engram>` admitted-engram store. One per persona, owned by
//!   `PersonaCognition` (see `persona::unified`).
//! - `admit(message, trace)` — runs the full pipeline AND records the
//!   side-effects (admitted engram added to store, content_hash recorded
//!   for dedup, AIRC event_id recorded for replay protection).
//! - Read-only inspection: `engram_count()`, `engram_at()`,
//!   `is_content_seen()`, `is_event_seen()` — for tests + future recall
//!   surface (PR-5+).
//!
//! # What this PR does NOT ship (deferred)
//!
//! - **ORM persistence.** Engrams stay in-memory for v1. PR-5 swaps in
//!   ORM-backed lookups + the entity registry path so admitted engrams
//!   survive restarts.
//! - **Recall surface.** Reading admitted engrams back out is just
//!   `engram_at(idx)` for v1. PR-5+ adds a typed query API.
//! - **Quarantine store.** `Quarantine` decisions don't actually quarantine
//!   anywhere; the engram is dropped on the floor for now. (Replay
//!   protection still records the event_id, which is correct.) PR-5+ adds
//!   the quarantine store.
//! - **Per-persona config customization.** All personas use the same
//!   `default_v1()` runner config in this PR. Config-per-persona ships
//!   when the IPC layer needs it.
//!
//! # Concurrency
//!
//! `AdmissionState` is `Send + Sync`. Internal mutability via `Mutex` so
//! the struct can be borrowed immutably (`&AdmissionState`) and called
//! concurrently from per-persona task tasks. Same shape as `PersonaInbox`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use uuid::Uuid;

use super::admission::{HeuristicIsMemorable, SeenContentLookup, SeenEventLookup};
use super::engram::{AdmissionDecision, AdmissionDropReason, AdmissionError, Engram, EngramOrigin};
use super::inbox_admission::{content_hash_sha256, InboxAdmissionRunner};
use super::trace::CognitionTrace;
use crate::cognition::conversation::types::InboxMessage;

//=============================================================================
// IN-MEMORY ORACLES (private, used by AdmissionState)
//=============================================================================

mod in_memory_seen_content;
pub use in_memory_seen_content::*;
mod admission_state;
mod admission_state_2;
pub use admission_state_2::*;

//=============================================================================
// TESTS
//=============================================================================

#[cfg(test)]
mod tests;
