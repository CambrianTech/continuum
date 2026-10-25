//! Per-persona service loop — the wire driver between airc and the
//! brain.
//!
//! Takes a [`HostedPersona`] (`PersonaContext`) and a "talk to the
//! grid" abstraction ([`PersonaConversation`]) and drives one
//! cognition turn per incoming message through THE BRAIN — the
//! canonical agent pipeline:
//!
//!   subscribe → for each event:
//!     • skip pre-watermark / self / non-text
//!     • lease the brain (`ctx.cognition.lock().await`)
//!     • `compose_for_turn` — engram + airc through the
//!       FlexboxRagBudgetAdapter
//!     • project into `RespondInput` (canonical agent contract: media,
//!       capabilities, tools, recalled engrams, room context)
//!     • `persona::response::respond(input)` — the substrate's per-
//!       persona cognition cycle (shared analyze + score + genome
//!       activate + evaluate_response + clean_and_validate +
//!       tool_executor + audit + record_turn)
//!     • post the resulting text via `conversation.say` (or honor
//!       `PersonaResponse::Silent { reason, .. }`)
//!
//! **See `docs/architecture/PERSONA-COGNITION-PIPELINE.md` for the
//! full per-persona cycle.** The bypass that previously called
//! `inspect_persona_rag_with_inference` (a `will_respond + response`
//! chatbot wrapper around the inspection function) was removed in
//! slice 1C of task #160. `rag_inspect.rs::inspect_persona_rag` stays
//! as the mechanic's-view introspection function it was named for.
//!
//! ## Doctrine
//!
//! - [[no-if-statements-use-llms-for-cognition]]: the loop does the
//!   minimum substrate filtering — pre-watermark / self / non-text —
//!   and hands the rest to `respond()`. No heuristic "should I respond"
//!   gate; the brain's cycle (with `full_evaluate` + `score_persona`
//!   + the LLM's own decision via `evaluate_response`) owns that.
//! - [[no-fallbacks-ever]]: per-message errors are logged + counted on
//!   the outcome, not swallowed; the loop continues with the next
//!   message rather than substituting a default response.
//! - [[no-stdio-piping-for-process-ipc]]: the loop talks to airc only
//!   through the [`PersonaConversation`] trait. The trait is the
//!   substrate's IPC boundary; tests stub it without any daemon.

use continuum_base::time::EpochMs;
use crate::persona::supervisor::HostedPersona;
use crate::cognition::conversation::workspace_turns::*;
use crate::cognition::conversation::{IncomingMessage, PersonaConversation};
use uuid::Uuid;

mod remains;
pub use remains::*;
mod serve_persona_loop_inner;
use serve_persona_loop_inner::*;
mod wake_select;
use wake_select::*;
mod self_tick;
use self_tick::*;
mod backlog_resync;
use backlog_resync::*;
mod turn_context;
use turn_context::*;
mod turn_cycle;
use turn_cycle::*;
mod turn_settle;
use turn_settle::*;
mod beat_outcome;
pub use beat_outcome::*;
mod burst_fingerprint;
use burst_fingerprint::*;

#[cfg(test)]
mod tests;
