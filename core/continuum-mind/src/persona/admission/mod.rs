//! Admission Gate + IsMemorable Recipe (continuum#1121 PR-2)
//!
//! Layers the admission policy machinery over the storage-shape types
//! shipped in PR-1 (`persona::engram`). Splits cleanly into two responsibilities:
//!
//! - **Gate (structural)** — `AdmissionGate::admit()` runs the prereqs that
//!   are independent of any specific persona's policy: envelope structure
//!   verification, trust-tier threshold check, replay protection. Failures
//!   here return typed `AdmissionError` variants, never silent drops.
//! - **Recipe (policy)** — implementations of the `IsMemorable` trait
//!   decide whether a candidate that *passed* the structural prereqs should
//!   be admitted, dropped, or quarantined. Different personas plug in
//!   different recipes (a fuzzy/agent persona may use a permissive
//!   `HeuristicIsMemorable`; a SOC governance persona may use a strict
//!   policy-driven recipe). The trait is the seam.
//!
//! # Design choices
//!
//! - **Stateless gate, injected stores.** `AdmissionGate::admit` is a free
//!   function (no `Self`). State lives in `AdmissionContext`'s lookup
//!   trait objects (`SeenContentLookup`, `SeenEventLookup`). Keeps the
//!   gate trivially testable + composable; same shape as how `recorder`
//!   takes the trace as parameter rather than owning it.
//! - **Caller stores admitted engrams.** The gate returns the
//!   `AdmissionDecision`; the caller is responsible for inserting into
//!   whatever engram store backs the persona. This keeps gate concerns
//!   orthogonal to persistence (PR-3+ adds the ORM persistence path).
//! - **Trace seam emitted unconditionally.** Whether the call returns
//!   `Ok(decision)` or `Err(error)`, a `SEAM_ADMISSION` entry is appended
//!   to the trace. Forensics need to see the gate ran even on error,
//!   matching `recorder.rs`'s always-call-record_turn discipline.
//! - **No panic-catching around recipes.** Recipes return `Result`. If
//!   one panics, that's a bug — let it propagate so the caller sees it.
//!   Same anti-fallback discipline as the rest of the cognition path.
//! - **Envelope verification is structural in v1.** Cryptographic
//!   signature verification against the AIRC pubkey infrastructure is
//!   deferred to a follow-up PR (airc#561 is formalizing the envelope
//!   format). v1 enforces that signed origins have non-empty
//!   signature/content_hash/schema_version fields; the cryptographic
//!   verifier hook lives in [`verify_envelope`] for the real impl to
//!   replace.
//!
//! Pairs with:
//! - [`persona::engram`] — storage-shape types this module operates over.
//! - [`persona::trace`] — `SEAM_ADMISSION` constant + `CognitionTrace`.
//! - `docs/grid/COGNITIVE-IMMUNE-MODEL.md` — defense posture this gate
//!   participates in (apoptosis-cheaper-than-corruption, B-cell anergy,
//!   forensic-not-destructive).
//!
//! # Module layout (continuum#1208)
//!
//! Split out of a 1225-LOC file:
//! - this `mod.rs` — gate machinery (`AdmissionGate::admit`), candidate
//!   + context types, IsMemorable trait, structural-gate tests,
//!   helpers (build_engram_from_candidate, envelope verification,
//!   trace seam emission).
//! - [`recipes`] — concrete `IsMemorable` implementations Continuum
//!   ships (currently `HeuristicIsMemorable`); re-exported here so
//!   external callers see no API change.

pub mod recipes;

pub use recipes::HeuristicIsMemorable;

use continuum_base::time::EpochMs;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

// Re-exported pub so submodules (`recipes`) can import via `super::`
// without reaching across to `crate::persona::engram` for every type.
use super::engram::Engram;
pub use super::engram::{AdmissionDecision, AdmissionDropReason, AdmissionError, AircMessageRef, EngramKind, EngramOrigin, TrustState};
use super::trace::{CognitionTrace, SEAM_ADMISSION};

//=============================================================================
// CANDIDATE: input to the admission pipeline
//=============================================================================

mod admission_candidate;
pub use admission_candidate::*;

#[cfg(test)]
mod tests;
