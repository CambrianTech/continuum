//! Per-persona command inbound pump — makes a persona a real
//! command-callable peer on the airc grid.
//!
//! ## What this exists for
//!
//! PR #1560 + #1563 proved the cross-grid command WIRE works
//! end-to-end IN TESTS — but the wiring was always manual: each test
//! constructed a `CommandRequestHandler` + spawned a subscribe loop
//! against the test's airc peer. Production never installed the
//! handler anywhere.
//!
//! Production substrates today therefore SILENTLY ignore incoming
//! `AircCommandRequest` envelopes. The wire-test green-light reads
//! "we proved the substrate CAN respond"; what's missing is "the
//! substrate actually IS responding."
//!
//! This module fills the install step. Per
//! `[[personas-are-citizens-airc-is-identity-provider]]`: the
//! substrate has no airc identity of its own — only personas do. So
//! the pump lives ON THE PERSONA, attached to its `Arc<Airc>` handle.
//! Every persona that boots gets one; when peer_b dispatches
//! `airc://<persona-uuid>/ai/generate`, that persona's pump receives
//! the envelope, hands it to a `CommandRequestHandler`, the handler
//! dispatches via the substrate's global `CommandExecutor`, and the
//! reply ships back automatically.
//!
//! ## Why a separate task from the persona's chat subscribe loop
//!
//! `airc_persona_conversation.rs::next_message` is the chat pump. It
//! filters by `body.as_text()` — anything that isn't a text body is
//! silently dropped. Command envelopes carry `Body::Json` so the
//! chat pump never sees them.
//!
//! Two options for the command path:
//!
//! 1. **Extend the chat pump** to also dispatch command envelopes
//!    inline. Couples two concerns into one loop; cognition / lag
//!    behavior gets tangled.
//! 2. **Spawn a second subscribe loop** on the same airc handle.
//!    airc-lib's `subscribe()` is broadcast-shape (verified in PR
//!    #1563 R3): every subscriber gets every event, no contention.
//!
//! Option 2 wins on separation of concerns + composability. The chat
//! pump stays single-purpose; the command pump stays single-purpose;
//! airc-lib does the fan-out.
//!
//! ## Lifecycle
//!
//! - Spawn at persona boot — same place the chat pump primes.
//! - Hold the `JoinHandle`. On persona shutdown, abort the task and
//!   await it so the airc subscribe drops cleanly.
//! - Per `[[no-fallbacks-ever]]`: if the airc subscribe call FAILS
//!   at spawn, the pump task surfaces the error LOUDLY via tracing
//!   and exits. A persona that can't subscribe can't receive cross-
//!   grid commands; we refuse to pretend it can.
//!
//! ## Doctrinal alignment
//!
//! - `[[headless-success-is-personas-talking-over-airc]]` — this IS
//!   the install step. Without it, "personas as command-callable
//!   peers" is a doctrine without a wire.
//! - `[[personas-are-citizens-airc-is-identity-provider]]` — pump
//!   binds to a persona's airc handle, not a substrate singleton.
//! - `[[no-fallbacks-ever]]` — subscribe failure surfaces; command
//!   dispatch failure surfaces; the pump never silently drops what
//!   it's supposed to be carrying.

use std::sync::Arc;

use airc_lib::adapter::ConsumerAdapter;
use airc_lib::{Airc, AircError, EventStream};
use continuum_airc_protocol::{COMMAND_REQUEST_BODY_HINT, HEADER_CONTINUUM_BODY_HINT};
use futures::stream::StreamExt;
use tokio::task::JoinHandle;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::routing::CommandRequestHandler;
use crate::runtime::command_executor::CommandExecutor;

/// One persona's command inbound pump. Holds the JoinHandle so the
/// owning `PersonaAircRuntime` can abort + await on shutdown.
pub struct PersonaCommandInboundPump {
    persona_id: Uuid,
    handle: JoinHandle<()>,
}

impl PersonaCommandInboundPump {
    /// Spawn the pump. The airc subscribe call happens SYNCHRONOUSLY
    /// before the task spawns — so a subscribe failure surfaces at
    /// the call site immediately (per `[[no-fallbacks-ever]]`) instead
    /// of getting buried in a log line + a silent-task exit. The
    /// caller (`PersonaAircRuntime::bootstrap` once #222 lands) can
    /// fail the persona's bootstrap with the same error rather than
    /// declaring the persona ready when it's actually unaddressable.
    ///
    /// Once `spawn()` returns `Ok`, the subscribe loop runs until
    /// either:
    ///   (a) the airc subscribe stream ends (daemon disconnect),
    ///   (b) the JoinHandle is aborted by the caller.
    ///
    /// `airc` is the persona's airc handle. `executor` is the
    /// substrate's command executor — typically the global handle
    /// from `crate::runtime::command_executor::executor()`.
    ///
    /// Per R2 of PR #1567: returning `Result` here closes the
    /// "loud-once" gap. The OLD shape logged `error!` and exited the
    /// task; the operator's only feedback was one log line + a
    /// persona that mysteriously stopped answering. The new shape
    /// makes the failure unmissable.
    pub async fn spawn(
        persona_id: Uuid,
        airc: Arc<Airc>,
        executor: Arc<CommandExecutor>,
    ) -> Result<Self, AircError> {
        // Subscribe BEFORE spawning so failure surfaces at the call
        // site. The stream moves into the spawned task; subsequent
        // stream errors (lag, end-of-stream) are runtime concerns,
        // not install-time concerns.
        let stream = airc.subscribe().await?;
        let handler = CommandRequestHandler::new(Arc::clone(&airc), executor);
        let handle = tokio::spawn(run(persona_id, airc, handler, stream));
        Ok(Self { persona_id, handle })
    }

    /// Persona this pump belongs to. Useful for telemetry +
    /// debug-logging in the owning runtime.
    pub fn persona_id(&self) -> Uuid {
        self.persona_id
    }

    /// Abort the pump task and await its exit. Drop alone would
    /// detach the task; callers that want clean shutdown should call
    /// this. Idempotent — re-aborting an already-finished task is
    /// a no-op.
    pub async fn shutdown(self) {
        self.handle.abort();
        // Joining an aborted task yields a Cancelled JoinError; we
        // don't care — the only failure path that matters here is
        // "the task panic'd before abort", and the tokio runtime
        // already surfaces panics via its own diagnostic channel.
        let _ = self.handle.await;
    }
}

/// The subscribe loop. Extracted for readability; spawned as a
/// tokio task by `spawn()`. The stream is opened by the caller
/// BEFORE spawning so subscribe failure surfaces at the call site
/// (see the doc on `spawn` for rationale).
async fn run(
    persona_id: Uuid,
    airc: Arc<Airc>,
    handler: Arc<CommandRequestHandler>,
    mut stream: EventStream,
) {
    let self_id = airc.peer_id();
    debug!(
        persona_id = %persona_id,
        self_peer_id = %self_id.0,
        "PersonaCommandInboundPump: subscribed; awaiting command envelopes"
    );

    while let Some(event) = stream.next().await {
        let event = match event {
            Ok(e) => e,
            Err(lag) => {
                // Lag is a broadcast-channel artifact — we missed
                // some events but the stream is still alive. Log
                // + continue per the existing chat pump's pattern
                // (airc_persona_conversation.rs).
                warn!(
                    persona_id = %persona_id,
                    "PersonaCommandInboundPump: subscribe lag: {lag}"
                );
                continue;
            }
        };

        // Self-events come from our own publishes. Skip.
        if event.peer_id == self_id {
            continue;
        }

        // Only command-request envelopes go through the handler.
        // Any other body_hint (chat, event-subscribe, future
        // shapes) is for some other consumer to handle.
        let hint = event
            .headers
            .get(HEADER_CONTINUUM_BODY_HINT)
            .map(|s| s.as_str());
        if hint != Some(COMMAND_REQUEST_BODY_HINT) {
            continue;
        }

        // Deref-clone the broadcast Arc — `ConsumerAdapter::on_envelope`
        // takes the owned TranscriptEvent. Same pattern as the e2e
        // integration test in PR #1563.
        if let Err(e) = handler.on_envelope((*event).clone()).await {
            warn!(
                persona_id = %persona_id,
                error = %e,
                "PersonaCommandInboundPump: handler.on_envelope rejected an envelope"
            );
        }
    }

    debug!(
        persona_id = %persona_id,
        "PersonaCommandInboundPump: subscribe stream ended (daemon disconnect); pump exiting"
    );
}
