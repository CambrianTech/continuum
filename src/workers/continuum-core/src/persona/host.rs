//! Persona host helper — slice 12 of #133.
//!
//! Composes the substrate's per-persona pieces into one tokio task:
//!
//!   `HostedPersona` (slice 9)
//!     + `PersonaAircRuntime` (#87, registered by `bootstrap_one`)
//!     → `AircPersonaConversation` (slice 11)
//!     + `Arc<Airc>` as `AircTranscriptReader`
//!     → `serve_persona_loop` (slice 10)
//!     → `JoinHandle<Result<ServeOutcome, String>>`
//!
//! ## What slice 12 ships
//!
//! - [`spawn_persona_service`] — the 5-line composition that takes
//!   already-bootstrapped pieces and starts hosting a persona.
//!
//! ## What slice 13 will do (not this commit)
//!
//! Rewrite the IPC boot loop at
//! `crate::ipc::start_server` (~line 1024) so that after
//! `bootstrap_one(&intent)` succeeds, the boot path:
//!
//! 1. Materializes the persona's inference profile via
//!    `build_profile` (slice 5) against the model registry and
//!    detected hardware tier.
//! 2. Builds the adapter via `LlamaCppPersonaAdapterFactory`
//!    (slice 9).
//! 3. Constructs a `HostedPersona` from
//!    `(role, info, adapter)`.
//! 4. Calls `spawn_persona_service` to start the per-persona
//!    serve loop.
//!
//! The boot loop then collects the `JoinHandle`s for graceful
//! shutdown on server stop.
//!
//! Splitting this into slice 12 (helper) + slice 13 (wire-up) keeps
//! each commit reviewable — slice 12 is the unit-testable
//! composition seam; slice 13 is the boot-flow rewrite that consumes
//! it. Per [[organization-purity-as-we-migrate]] the slice-12 helper
//! is NOT dead code: it ships with slice 13's wire-up as a paired
//! follow-up in the same sprint.
//!
//! ## Doctrine
//!
//! - [[no-stdio-piping-for-process-ipc]]: the spawn helper never
//!   touches stdin/stdout for IPC. The substrate talks to airc only
//!   via `Arc<PersonaAircRuntime>` and the airc-lib socket protocol.
//! - [[substrate-is-a-good-citizen-on-the-host]]: per-persona tasks
//!   run on the supplied `tokio::runtime::Handle` — caller controls
//!   the scheduling pool. No hidden thread spawns.

use crate::persona::airc_persona_conversation::AircPersonaConversation;
use crate::persona::airc_runtime::PersonaAircRuntime;
use crate::persona::airc_source::AircTranscriptReader;
use crate::persona::service_loop::{serve_persona_loop, ServeOptions, ServeOutcome};
use crate::persona::supervisor::HostedPersona;
use std::sync::Arc;
use tokio::task::JoinHandle;

/// Spawn one tokio task that hosts a single persona on the airc
/// grid: subscribes to her room, runs `serve_persona_loop` against
/// the cognition path, posts replies through `runtime.say`.
///
/// Returns immediately with a `JoinHandle`. The task runs until:
///
/// - the airc subscribe stream ends (daemon disconnect) — handle
///   resolves with `Ok(ServeOutcome { ... })` summarizing what
///   happened.
/// - `serve_persona_loop` returns `Err(message)` from a
///   non-recoverable error (e.g., `high_water_mark` failed at
///   start) — handle resolves with `Err(message)`.
/// - the handle is `.abort()`'d by the caller — the slice 13
///   shutdown path uses this for graceful drain.
///
/// `hosted.adapter` must reference the SAME inference adapter the
/// substrate intends to keep using for this persona for the full
/// task lifetime — the loop clone-shares it into the RAG layer
/// every turn. Re-spawning with a fresh adapter is the supervisor's
/// signal that the prior task should be aborted first.
pub fn spawn_persona_service(
    hosted: HostedPersona,
    runtime: Arc<PersonaAircRuntime>,
    opts: ServeOptions,
    rt_handle: tokio::runtime::Handle,
) -> JoinHandle<Result<ServeOutcome, String>> {
    // Up-cast the persona's Arc<Airc> to Arc<dyn AircTranscriptReader>.
    // `impl AircTranscriptReader for airc_lib::Airc` already exists
    // in airc_source.rs (line 74), so this is a zero-cost type-level
    // coercion — same heap pointer, different vtable view.
    let reader: Arc<dyn AircTranscriptReader> = runtime.airc().clone();
    let mut conversation = AircPersonaConversation::new(runtime);
    rt_handle.spawn(async move {
        serve_persona_loop(&hosted, &mut conversation, reader, opts).await
    })
}
