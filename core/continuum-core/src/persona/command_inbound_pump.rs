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

use std::path::Path;
use std::sync::Arc;

use airc_lib::adapter::ConsumerAdapter;
use airc_lib::{Airc, AircError, FilteredEventStream};
use continuum_airc_protocol::{COMMAND_REQUEST_BODY_HINT, HEADER_CONTINUUM_BODY_HINT};
use futures::stream::StreamExt;
use tokio::task::JoinHandle;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::routing::epoch_watermark::{SqliteEpochWatermark, WatermarkError};
use crate::routing::grid_capability::GrantAuthorizer;
use crate::routing::CommandRequestHandler;
use crate::runtime::command_executor::CommandExecutor;

/// Why constructing a persona's [`GrantAuthorizer`] failed at boot. Distinct from
/// [`AircError`] because the watermark store + owner-key resolution are continuum
/// concerns, not airc ones.
#[derive(Debug, thiserror::Error)]
pub enum GrantAuthorizerBuildError {
    /// This node's own enrolled public key is unavailable — it is self-enrolled at
    /// `Airc::open`, so this should not happen; if it does we refuse to build a
    /// verifier with no pinned issuer rather than trust the wrong key.
    #[error("this node's own enrolled public key is unavailable — cannot pin the grant issuer")]
    OwnerKeyUnavailable,
    /// Could not resolve this node's local mesh identity (the expected mesh a grant
    /// must be scoped to).
    #[error("resolve local mesh identity: {0}")]
    Mesh(#[source] AircError),
    /// Could not open the durable epoch-watermark store (the anti-replay state).
    #[error("open grant epoch watermark store: {0}")]
    Watermark(#[source] WatermarkError),
}

/// Build this persona node's [`GrantAuthorizer`] — the verifier for capability
/// grants visiting peers present. "This node is the owner": the trusted issuer key
/// is the node's OWN enrolled ed25519 key (it self-signs the grants it hands out),
/// the expected mesh is the node's own mesh, and the anti-replay watermark is a
/// DURABLE SQLite store under the persona home (survives restart — the review hard
/// gate). Provider≠owner (verifying grants the owner signed on a different box) is
/// a later generalization needing pinned-issuer-key distribution.
pub async fn build_grant_authorizer(
    airc: &Arc<Airc>,
    home: &Path,
) -> Result<Arc<GrantAuthorizer>, GrantAuthorizerBuildError> {
    let owner_pubkey = airc
        .peer_public_key(airc.peer_id())
        .ok_or(GrantAuthorizerBuildError::OwnerKeyUnavailable)?
        .to_vec();
    let mesh = airc
        .mesh_identity()
        .await
        .map_err(GrantAuthorizerBuildError::Mesh)?;
    let watermark = SqliteEpochWatermark::open(&home.join("grant_watermark.sqlite"))
        .map_err(GrantAuthorizerBuildError::Watermark)?;
    Ok(Arc::new(GrantAuthorizer::with_watermark(
        owner_pubkey,
        mesh,
        Arc::new(watermark),
    )))
}

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
        grant_authorizer: Arc<GrantAuthorizer>,
    ) -> Result<Self, AircError> {
        // Subscribe BEFORE spawning so failure surfaces at the call
        // site. The stream moves into the spawned task; subsequent
        // stream errors (lag, end-of-stream) are runtime concerns,
        // not install-time concerns.
        // Every room she is subscribed to, not just her default. A
        // cross-grid command envelope is addressed to the PERSONA, so
        // narrowing it by room made her un-callable from anywhere she
        // was not currently parked — the same one-channel narrowing
        // that made operator chat structurally invisible (task #64).
        let stream = crate::persona::airc_citizen::subscribe_every_room(&airc).await?;
        // The handler VERIFIES presented capability grants against the authorizer
        // (built from this node's own key + mesh + durable watermark). A peer
        // presenting an owner-signed grant gets the conferred command past its tier
        // ceiling; absent/invalid grants fall back to tier gating.
        let handler =
            CommandRequestHandler::with_grant_authorizer(Arc::clone(&airc), executor, grant_authorizer);
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

    /// Fire-and-forget abort. Used by `PersonaAircRuntime::drop`
    /// (which is sync and can't await). The tokio runtime reaps the
    /// aborted task asynchronously; the JoinError on the eventual
    /// await would be Cancelled, which is the expected shape.
    /// Callers that want clean shutdown WITH await should use
    /// [`shutdown`] instead.
    pub fn abort(&self) {
        self.handle.abort();
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
    mut stream: FilteredEventStream,
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
