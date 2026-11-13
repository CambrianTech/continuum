//! Per-identity command pump: makes a persona (and the node) a command-callable peer
//! on the airc grid.
//!
//! The pump reads ONE source: its identity's addressed inbox
//! (`routing::addressed_inbox`), where airc delivers `Command` frames it has verified
//! (signature, sender, addressee). It hands each frame to the installed command receiver
//! (`ipc::command_ingress`), whose ingress contract decides it. Room subscriptions are
//! perception's alone and carry no command handling: on 2026-10-30 an untrusted peer's
//! commands published into #general were recognized by a header string on room events,
//! executed, and fed to residents as speech.
//!
//! Per `[[personas-are-citizens-airc-is-identity-provider]]` the pump lives on the
//! persona's airc handle; the NODE runs one too ([`spawn_node_pump`]) because capacity
//! is offered and placement targets at node granularity.
//!
//! Lifecycle: spawned at persona boot; the `JoinHandle` is held and aborted on shutdown.
//! Per `[[no-fallbacks-ever]]` an inbox that cannot be opened fails the spawn loudly; a
//! closed inbox (daemon restart) is re-opened with a bounded backoff and says so.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use airc_lib::{Airc, AircError};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::routing::addressed_inbox::{AddressedHandler, AddressedInbox};
use crate::routing::epoch_watermark::{OrmEpochWatermark, WatermarkError};
use crate::routing::grid_capability::GrantAuthorizer;
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
/// DURABLE ORM store under the persona home (survives restart — the review hard
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
    let watermark = OrmEpochWatermark::open(&home.join("grant_watermark.sqlite"))
        .await
        .map_err(GrantAuthorizerBuildError::Watermark)?;
    Ok(Arc::new(GrantAuthorizer::with_watermark(
        owner_pubkey,
        mesh,
        Arc::new(watermark),
    )))
}

/// THE NODE'S OWN COMMAND PUMP (2026-09-17, cards ad96f5d1 / 7c72c0f0).
///
/// Capacity is ADVERTISED per node (`grid.capacity.offer` rides
/// `airc/realtime-publish`, stamped with the daemon's scope identity),
/// placement TARGETS per node (`model_override.json` `remote_peer` = that
/// stamp; the transport addresses `MentionTarget::Peer(stamp)`), and
/// inference was SERVED per persona only — the pumps below, each under her
/// own peer id. Three layers each right, no join key: every `ai/generate`
/// a remote-bound mind sent was refused by every citizen on the host,
/// correctly, as not-for-me — 27 `airc.command.request_not_for_me` on the
/// 5090 in one afternoon, `airc.command.accepted` 0, and
/// `remote_lane.answered` 0 across the Intel node's whole retained history
/// (Cormac). Cross-grid inference had never been addressable.
///
/// This attaches a handle under the MACHINE-ACCOUNT home's default identity
/// — the keypair the singular daemon speaks as, the one the beacon is
/// stamped with — and runs the same pump on it. A node's spare inference is a
/// node-level service, answered at the granularity it is offered.
/// Executes through the substrate's wired executor under the CALLER's
/// identity (the envelope's), gated by the same grant authorizer the
/// citizens use — the node lends its lane, never its seat.
///
/// Failure is loud and named; a node that cannot answer must not look
/// like one that can (the beacon still goes out — the seat-side breaker
/// and fall-home cover the gap, as they did before this existed).
pub async fn spawn_node_pump(
    airc_home: &Path,
    daemon_socket: std::path::PathBuf,
    executor: Arc<CommandExecutor>,
    state_home: &Path,
) -> Result<Uuid, NodePumpError> {
    // The MACHINE-ACCOUNT home (`$HOME/.airc`), not the scope's: the singular daemon
    // lives there and stamps everything it publishes — the beacon included — with
    // THAT home's identity. Attaching at the continuum root gave a third identity
    // (`9237e018` on the 5090's first boot of this code, beside the beacon's
    // `e85a5bb3`) that nothing on the grid addresses.
    let home = airc_lib::machine_account_home(airc_home);
    let airc = Arc::new(
        Airc::attach(home.as_path().to_path_buf(), daemon_socket)
            .await
            .map_err(NodePumpError::Attach)?,
    );
    let peer = airc.peer_id().as_uuid();
    std::fs::create_dir_all(state_home).map_err(|e| NodePumpError::StateHome {
        path: state_home.to_path_buf(),
        source: e,
    })?;
    let grant_authorizer = build_grant_authorizer(&airc, state_home)
        .await
        .map_err(NodePumpError::GrantAuthorizer)?;
    // The node never changes rooms on its own; the sender lives with the pump
    // for the life of the process so the loop never reads "runtime dropped".
    let (membership_tx, membership_rx) = watch::channel(0u64);
    let pump =
        PersonaCommandInboundPump::spawn(peer, airc, executor, grant_authorizer, membership_rx)
            .await
            .map_err(NodePumpError::Pump)?;
    NODE_PUMP
        .set((pump, membership_tx))
        .map_err(|_| NodePumpError::AlreadyInstalled)?;
    continuum_base::probe!(
        class = "node.command_pump.online",
        peer_id = %peer,
        "the node answers cross-grid commands under its own identity — the one its capacity beacon is stamped with"
    );
    Ok(peer)
}

static NODE_PUMP: std::sync::OnceLock<(PersonaCommandInboundPump, watch::Sender<u64>)> =
    std::sync::OnceLock::new();

#[derive(Debug, thiserror::Error)]
pub enum NodePumpError {
    #[error("attach the node's own airc identity: {0}")]
    Attach(#[source] AircError),
    #[error("node pump state home {path}: {source}")]
    StateHome {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("node pump grant authorizer: {0}")]
    GrantAuthorizer(#[source] GrantAuthorizerBuildError),
    #[error("node pump: {0}")]
    Pump(#[source] PumpSpawnError),
    #[error("the node command pump is already installed")]
    AlreadyInstalled,
}

/// Builds the command receiver a pump drives: the command-ingress handler, which the
/// socket layer owns (`ipc::command_ingress`) and installs here at boot. The persona owns
/// its pump and the seam; it never names the layer that answers commands.
pub type CommandHandlerFactory =
    fn(Arc<Airc>, Arc<CommandExecutor>, Arc<GrantAuthorizer>) -> Arc<dyn AddressedHandler>;

static HANDLER_FACTORY: parking_lot::RwLock<Option<CommandHandlerFactory>> =
    parking_lot::RwLock::new(None);

/// Install the one command-handler factory. A second install replaces the first.
pub fn install_command_handler(factory: CommandHandlerFactory) {
    *HANDLER_FACTORY.write() = Some(factory);
}

/// Why a pump could not start.
#[derive(Debug, thiserror::Error)]
pub enum PumpSpawnError {
    #[error("open the identity's addressed inbox: {0}")]
    Inbox(#[source] airc_lib::AddressedError),
    /// Boot never installed the command handler, so a pump would hear commands and answer
    /// none; refused rather than pretend the persona is addressable.
    #[error("no command handler installed (boot installs it before any pump starts)")]
    NoHandler,
}

/// One identity's command pump. Holds the JoinHandle so the owning runtime can abort +
/// await on shutdown.
pub struct PersonaCommandInboundPump {
    persona_id: Uuid,
    handle: JoinHandle<()>,
}

/// Open `airc`'s addressed inbox behind the core's seam: the ONLY way a command reaches
/// this identity. Room subscriptions are perception's and carry no command handling.
async fn open_inbox(airc: &Airc) -> Result<Box<dyn AddressedInbox>, airc_lib::AddressedError> {
    let deadline = tokio::time::Instant::now() + airc_ipc::client::DEFAULT_RPC_TIMEOUT;
    Ok(Box::new(airc.addressed_inbox(deadline).await?))
}

impl PersonaCommandInboundPump {
    /// Spawn the pump. The inbox is opened SYNCHRONOUSLY before the task spawns, so a
    /// failure surfaces at the call site (per `[[no-fallbacks-ever]]`) instead of being
    /// buried in a log line and a silent-task exit.
    ///
    /// Once `spawn()` returns `Ok`, the pump reads the inbox until the runtime drops its
    /// membership sender (the identity is torn down) or the JoinHandle is aborted; a
    /// closed inbox (daemon restart) is re-opened with a bounded backoff.
    pub async fn spawn(
        persona_id: Uuid,
        airc: Arc<Airc>,
        executor: Arc<CommandExecutor>,
        grant_authorizer: Arc<GrantAuthorizer>,
        membership: watch::Receiver<u64>,
    ) -> Result<Self, PumpSpawnError> {
        let factory = (*HANDLER_FACTORY.read()).ok_or(PumpSpawnError::NoHandler)?;
        let inbox = open_inbox(&airc).await.map_err(PumpSpawnError::Inbox)?;
        // The handler applies the ingress contract (direct, addressed here, the sender's
        // trust tier meets the command's) and VERIFIES presented capability grants.
        let handler = factory(Arc::clone(&airc), executor, grant_authorizer);
        let handle = tokio::spawn(supervise(persona_id, airc, handler, inbox, membership));
        Ok(Self { persona_id, handle })
    }

    /// Persona this pump belongs to.
    pub fn persona_id(&self) -> Uuid {
        self.persona_id
    }

    /// Whether the pump TASK is still running. Liveness is the task, not the handle
    /// (2026-09-06: four of five citizens held a `Some` whose task had exited hours earlier).
    pub fn is_alive(&self) -> bool {
        !self.handle.is_finished()
    }

    /// Abort the pump task and await its exit. Idempotent.
    pub async fn shutdown(self) {
        self.handle.abort();
        // Joining an aborted task yields a Cancelled JoinError; the only failure that
        // matters, a panic before the abort, the runtime already surfaces.
        let _ = self.handle.await;
    }

    /// Fire-and-forget abort, for a sync `Drop`.
    pub fn abort(&self) {
        self.handle.abort();
    }
}

/// Delay before the n-th consecutive re-open attempt: 1 s doubling to a 30 s cap.
fn reopen_delay(attempt: u32) -> Duration {
    Duration::from_secs(1u64 << attempt.min(5)).min(Duration::from_secs(30))
}

/// Owns the supervisor's inner pump task: dropping it (the supervisor returns, or is
/// aborted by [`PersonaCommandInboundPump::shutdown`]) aborts the task. Awaiting a bare
/// `JoinHandle` and then dropping it DETACHES the task, so a shut-down pump kept reading
/// and kept its handler, and every generation it was serving, alive (card b3e5b582).
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn supervise(
    persona_id: Uuid,
    airc: Arc<Airc>,
    handler: Arc<dyn AddressedHandler>,
    first_inbox: Box<dyn AddressedInbox>,
    mut membership: watch::Receiver<u64>,
) {
    let mut inbox = Some(first_inbox);
    let mut respawns: u32 = 0;
    loop {
        let this_inbox = match inbox.take() {
            Some(i) => i,
            None => match tokio::select! {
                result = open_inbox(&airc) => result,
                _ = runtime_ended(&mut membership) => return,
            } {
                Ok(i) => i,
                Err(e) => {
                    continuum_base::probe!(
                        class = "persona.command_pump.reopen_failed",
                        persona_id = %persona_id,
                        attempt = respawns,
                        error = %e,
                        "command pump could not re-open its addressed inbox after a panic; retrying"
                    );
                    tokio::select! {
                        _ = tokio::time::sleep(reopen_delay(respawns.min(5))) => {},
                        _ = runtime_ended(&mut membership) => return,
                    }
                    continue;
                }
            },
        };
        let mut task = AbortOnDrop(tokio::spawn(run(
            persona_id,
            Arc::clone(&airc),
            Arc::clone(&handler),
            this_inbox,
            membership.clone(),
        )));
        match (&mut task.0).await {
            // `run` returns only when the runtime dropped its membership sender.
            Ok(()) => return,
            Err(e) if e.is_panic() => {
                respawns = respawns.saturating_add(1);
                continuum_base::probe!(
                    class = "persona.command_pump.respawned_after_panic",
                    persona_id = %persona_id,
                    respawns = respawns,
                    "command pump task panicked; respawning with a fresh inbox"
                );
                tokio::select! {
                    _ = tokio::time::sleep(reopen_delay(respawns.min(5))) => {},
                    _ = runtime_ended(&mut membership) => return,
                }
            }
            // Cancelled: the supervisor itself was aborted (shutdown).
            Err(_) => return,
        }
    }
}

async fn while_runtime_alive(
    work: impl std::future::Future<Output = ()>,
    membership: &mut watch::Receiver<u64>,
) -> bool {
    tokio::select! {
        _ = work => true,
        _ = runtime_ended(membership) => false,
    }
}

async fn runtime_ended(membership: &mut watch::Receiver<u64>) {
    while membership.changed().await.is_ok() {}
}

async fn run(
    persona_id: Uuid,
    airc: Arc<Airc>,
    handler: Arc<dyn AddressedHandler>,
    mut inbox: Box<dyn AddressedInbox>,
    mut membership: watch::Receiver<u64>,
) {
    continuum_base::probe!(
        class = "persona.command_pump.installed",
        persona_id = %persona_id,
        peer_id = %airc.peer_id().0,
        "command pump reading the addressed inbox; the citizen is addressable"
    );
    loop {
        // Read until the inbox closes (a daemon restart or update) or the runtime drops.
        loop {
            tokio::select! {
                next = inbox.next() => match next {
                    Ok(frame) => {
                        if !while_runtime_alive(handler.on_addressed(frame), &mut membership).await {
                            return;
                        }
                    },
                    Err(airc_lib::InboxError::Closed) => break,
                    Err(error) => {
                        continuum_base::probe!(
                            class = "persona.command_pump.inbox_loss",
                            persona_id = %persona_id,
                            error = %error,
                            "addressed command refused or lost; its exchange cannot be completed"
                        );
                    }
                },
                changed = membership.changed() => {
                    if changed.is_err() {
                        // The runtime dropped its sender: the citizen is being torn down.
                        continuum_base::probe!(
                            class = "persona.command_pump.ended",
                            persona_id = %persona_id,
                            reason = "runtime_dropped",
                            "command pump exiting with its runtime"
                        );
                        return;
                    }
                    // A room join changes perception's subscriptions, not the inbox.
                }
            }
        }
        continuum_base::probe!(
            class = "persona.command_pump.ended",
            persona_id = %persona_id,
            reason = "inbox_closed",
            "addressed inbox closed; re-opening"
        );
        // A closed owner must be released before acquiring its replacement.
        drop(inbox);
        let mut attempt: u32 = 0;
        inbox = loop {
            let delay = reopen_delay(attempt);
            let reopened = tokio::select! {
                result = async { tokio::time::sleep(delay).await; open_inbox(&airc).await } => result,
                _ = runtime_ended(&mut membership) => return,
            };
            match reopened {
                Ok(next) => {
                    continuum_base::probe!(
                        class = "persona.command_pump.reopened",
                        persona_id = %persona_id,
                        attempt = attempt,
                        delay_ms = delay.as_millis() as u64,
                        "addressed inbox re-opened; the citizen is addressable again"
                    );
                    break next;
                }
                Err(e) => {
                    attempt = attempt.saturating_add(1);
                    continuum_base::probe!(
                        class = "persona.command_pump.reopen_failed",
                        persona_id = %persona_id,
                        attempt = attempt,
                        error = %e,
                        "command pump could not re-open its addressed inbox; retrying"
                    );
                }
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the re-open schedule must be bounded on BOTH ends —
    // a zero first delay would busy-loop against a daemon that is down (the
    // exact shape the message pump's membership contract test pins), and an
    // unbounded doubling would leave a citizen unaddressable for minutes after
    // one transient failure. 1 s doubling, capped at 30 s.
    #[test]
    fn the_reopen_schedule_is_bounded_on_both_ends() {
        assert_eq!(reopen_delay(0), Duration::from_secs(1));
        assert_eq!(reopen_delay(1), Duration::from_secs(2));
        assert_eq!(reopen_delay(4), Duration::from_secs(16));
        assert_eq!(reopen_delay(5), Duration::from_secs(30));
        assert_eq!(reopen_delay(40), Duration::from_secs(30));
    }
    // Membership changes only affect perception. The command owner must survive them,
    // while runtime teardown must interrupt even a pending retry or attach.
    #[tokio::test]
    async fn runtime_end_ignores_room_changes_but_observes_sender_drop() {
        let (tx, mut rx) = watch::channel(0u64);
        let mut ended = Box::pin(runtime_ended(&mut rx));
        assert!(futures::poll!(&mut ended).is_pending());
        tx.send(1).expect("runtime receiver exists");
        assert!(futures::poll!(&mut ended).is_pending());
        drop(tx);
        assert!(futures::poll!(&mut ended).is_ready());
    }
    #[tokio::test]
    async fn runtime_drop_cancels_a_blocked_handler_future() {
        let (tx, mut rx) = watch::channel(0u64);
        let dropped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        struct DropFlag(std::sync::Arc<std::sync::atomic::AtomicBool>);
        impl Drop for DropFlag {
            fn drop(&mut self) {
                self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
        let flag = DropFlag(dropped.clone());
        let work = async move {
            let _flag = flag;
            std::future::pending::<()>().await;
        };
        let mut running = Box::pin(while_runtime_alive(work, &mut rx));
        assert!(futures::poll!(&mut running).is_pending());
        drop(tx);
        assert!(!running.await);
        assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));
    }
}
