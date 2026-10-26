//! Unified Persona Evaluator — ALL pre-response gates in one call.
//!
//! Consolidates 5 sequential TypeScript gates + Rust fast-path into a single
//! `full_evaluate()` function. One IPC call, <1ms, zero GC.
//!
//! Gate order (short-circuits on first SILENT):
//! 1. Sleep mode — checks SleepMode + topic similarity (persona's own opt-out)
//! 2. Undirected persona chatter — one persona turn must not recursively summon another
//! 3. Self-message — infinite loop prevention (inside fast_path)
//! 4. Fast-path decision — delegates to PersonaCognitionEngine::fast_path_decision
//!
//! Note: response_count is collected as a SIGNAL (LLM sees it in social_signals
//! and can self-quiet if a conversation is getting too noisy) but is NOT a hard
//! gate. Local models have no cost; cloud rate limits belong at the provider
//! layer (their billing + RPS quotas), not as a hardcoded 50-message veto here.
//! The previous response_cap gate was a cloud-provider concern that leaked onto
//! every persona including local ones — removed to honor "LLM decides, not dumb
//! heuristics" (the philosophy this module already preaches).
//!
//! Types exported to TypeScript via ts-rs.
//!
//! # Module layout (continuum#1208)
//!
//! Split out of a single 1231-LOC file into focused submodules:
//! - [`sleep_state`] — `SleepMode` + `SleepState` (Gate 1 input)
//! - [`rate_limiter`] — `RateLimiterState` + `RoomRateState` (signal source)
//! - [`adequacy`] — post-inference response-adequacy check (`check_response_adequacy`)
//!
//! This module (the gate orchestrator) owns `FullEvaluateRequest`,
//! `FullEvaluateResult`, `GateDetails`, `SocialSignals`, and the
//! `full_evaluate` function that composes the submodules' state. Submodule
//! types are re-exported at the parent path so existing callers don't
//! see the move.

pub mod adequacy;
pub mod rate_limiter;
pub mod sleep_state;

pub use adequacy::{check_response_adequacy, AdequacyResult, RecentResponse};
pub use rate_limiter::{RateLimiterState, RoomRateState};
pub use sleep_state::{SleepMode, SleepState};

use crate::persona::cognition::PersonaCognitionEngine;
use crate::persona::message_cache::RecentMessageCache;
use continuum_text_analysis as text_analysis;
use crate::cognition::conversation::types::{InboxMessage, Modality, SenderType};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use ts_rs::TS;
use uuid::Uuid;

// =============================================================================
// REQUEST / RESULT TYPES (ts-rs exported)
// =============================================================================

mod full_evaluate_request;
pub use full_evaluate_request::*;

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests;
