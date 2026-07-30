//! `run_session` — the transport-generic async session task (slice
//! 2D-3).
//!
//! This is the future that turns the synchronous [`Connection`] state
//! machine into a live positron session: it reads `ClientMessage`s
//! from an inbound channel, drives [`Connection::handle`] (which emits
//! the snapshot + `CommandFailed` frames), and — the new part —
//! attaches a live [`Broadcast`](crate::Broadcast) `watch` receiver per
//! subscribed/observed kind so subsequent `Substrate::store`s fan out
//! as `ServerMessage::State` frames without another round-trip.
//!
//! ## Transport-generic on purpose
//!
//! It speaks `mpsc<ClientMessage>` in / `mpsc<ServerMessage>` out — NOT
//! a socket. `continuum-core`'s WS adapter decodes frames onto the
//! inbound channel and serializes the outbound channel back to the
//! wire; a UDS or an airc-subscription binding would do the same. That
//! keeps this task unit-testable here with in-memory channels + a
//! scripted dispatcher, and keeps the crate free of any continuum-core
//! dependency (the [`CommandDispatch`] trait is the seam).
//!
//! ## The snapshot→live handoff has NO lost-update window
//!
//! `Substrate::store` writes the cache THEN the broadcast, and
//! [`Connection::handle_subscribe`] / `handle_observe` read the cache
//! **synchronously** (no `.await` between the frame arriving and the
//! snapshot being computed). So this task creates the live `watch`
//! receivers for a frame's kinds *before* calling `handle` — with no
//! await in between, no other task's `store` can interleave. The
//! receiver captures the broadcast version as of the same instant the
//! snapshot reads the cache, so the forwarder emits exactly the
//! updates *after* the snapshot: no duplicate of the snapshot revision,
//! and no dropped update. This is the structural fix for #794 ("AI
//! messages not realtime") — realtime is the default path, not a
//! best-effort add-on.
//!
//! ## Rate: subscription is unlimited, observers are budgeted
//!
//! A subscribed kind (a human renderer) forwards every change. An
//! observed-only kind (AI perception) forwards at most its
//! `budget_hz`. When a kind is both, the subscription wins (unlimited).
//! When several observers name the same kind on one connection, the
//! highest budget wins — one socket carries one stream per kind, and
//! the most-demanding watcher sets its cadence. `budget_hz == 0` means
//! snapshot-only: the observer got its snapshot from `handle`, but no
//! live forwarder is attached.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use positron_core::session::{ClientMessage, ServerMessage};
use positron_core::wire::StateEnvelope;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use crate::connection::Connection;
use crate::dispatch::CommandDispatch;
use crate::scoping::SessionSubstrate;

/// How fast a kind's live `State` frames are forwarded on this
/// connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForwardRate {
    /// Every change — a subscribed human renderer wants all of them.
    Unlimited,
    /// At most `hz` frames per second — an observer's perception
    /// budget. `hz` is always `>= 1` (0-budget observers get no
    /// forwarder at all, so they never reach this variant).
    Hz(u32),
}

impl ForwardRate {
    /// Minimum spacing between forwarded frames. `Unlimited` → zero.
    fn min_interval(self) -> Duration {
        match self {
            ForwardRate::Unlimited => Duration::ZERO,
            ForwardRate::Hz(hz) => Duration::from_secs_f64(1.0 / hz as f64),
        }
    }
}

/// A live forwarder task + the rate it was spawned at (so a re-Observe
/// that changes the budget can detect the change and restart it).
struct Forwarder {
    handle: JoinHandle<()>,
    rate: ForwardRate,
}

impl Forwarder {
    /// Abort the task. Dropping a `JoinHandle` only detaches it, so an
    /// explicit abort is what actually stops a forwarder whose kind is
    /// no longer wanted (or when the session ends).
    fn stop(self) {
        self.handle.abort();
    }
}

/// The kinds a frame references — the set this task must have live
/// receivers for. `Command` references none.
fn frame_kinds(msg: &ClientMessage) -> Vec<String> {
    match msg {
        ClientMessage::Subscribe { kinds, .. } => kinds.clone(),
        ClientMessage::Observe { spec, .. } => spec.kinds.clone(),
        ClientMessage::Command(_) => Vec::new(),
    }
}

/// A subscribed renderer's frame budget. NOT `Unlimited`: state kinds are
/// latest-wins snapshots riding a `watch` channel, so intermediate revisions
/// are legally skippable — and a boot-replay/burst that folds thousands of
/// messages must NOT become thousands of socket frames (the 2026-07-30 "load
/// storm": every replayed message pushed individually, raining into the tab).
/// The forwarder sends the FIRST change instantly and only spaces subsequent
/// frames, so a lone chat message has zero added latency; only bursts
/// coalesce, at worst 100ms behind. Token streams ride the separate stream
/// rail, untouched by this.
const RENDERER_HZ: u32 = 10;

/// Compute the desired forward rate per kind from the connection's
/// current subscription + observers. Subscription → `RENDERER_HZ`;
/// observers → the highest positive `budget_hz` naming the kind;
/// the highest rate wins for the same kind.
fn desired_rates(conn: &Connection) -> HashMap<String, ForwardRate> {
    let mut rates: HashMap<String, ForwardRate> = HashMap::new();
    for kind in &conn.subscription.kinds {
        rates.insert(kind.clone(), ForwardRate::Hz(RENDERER_HZ));
    }
    for obs in conn.observers.values() {
        if obs.budget_hz == 0 {
            // snapshot-only: the Observe frame already delivered the
            // snapshot; no ongoing perception stream.
            continue;
        }
        for kind in &obs.kinds {
            match rates.get(kind) {
                // A subscription on this kind outranks any observer.
                Some(ForwardRate::Unlimited) => {}
                Some(ForwardRate::Hz(existing)) if *existing >= obs.budget_hz => {}
                _ => {
                    rates.insert(kind.clone(), ForwardRate::Hz(obs.budget_hz));
                }
            }
        }
    }
    rates
}

/// One kind's live forwarder: block on the broadcast `watch`, forward
/// each new envelope as a `State` frame, throttle to `rate`.
///
/// The receiver was created (subscribed) by the session task *before*
/// the snapshot was read, so its first `changed()` fires on the first
/// `store` AFTER the snapshot — the snapshot revision is never
/// re-sent, and no update between snapshot and attach is dropped.
async fn forward_kind(
    mut rx: watch::Receiver<Option<Arc<StateEnvelope>>>,
    rate: ForwardRate,
    outbound: mpsc::Sender<ServerMessage>,
) {
    let min_interval = rate.min_interval();
    loop {
        // Err => the broadcast sender was dropped (substrate gone).
        if rx.changed().await.is_err() {
            break;
        }
        // Clone the Arc out, then drop the borrow guard BEFORE the
        // await (a watch Ref is not Send across await points).
        let envelope = {
            let guard = rx.borrow_and_update();
            match guard.as_ref() {
                Some(env) => Arc::clone(env),
                // Cold-start sentinel: a kind can broadcast `None`
                // before its first real state. Nothing to forward.
                None => continue,
            }
        };
        if outbound
            .send(ServerMessage::State((*envelope).clone()))
            .await
            .is_err()
        {
            // Outbound closed → the connection is gone.
            break;
        }
        // Rate limit. `watch` already coalesces to the latest value, so
        // sleeping here samples the newest state after the interval —
        // intermediate churn is dropped, which is exactly the budget.
        if !min_interval.is_zero() {
            tokio::time::sleep(min_interval).await;
        }
    }
}

/// Run one positron client session to completion.
///
/// Returns `Ok(())` when the inbound channel closes (client
/// disconnected cleanly). Returns `Err` when [`Connection::handle`]
/// reports a structural protocol violation (a malformed frame that
/// carries no correlation id to answer) — the caller (which owns
/// logging) surfaces it; the session ends loud, never limps on.
///
/// `D` is the dispatcher's type; passing an `Arc<dyn CommandDispatch>`
/// binds `D = dyn CommandDispatch`.
pub async fn run_session<D, S>(
    mut inbound: mpsc::Receiver<ClientMessage>,
    outbound: mpsc::Sender<ServerMessage>,
    substrate: S,
    dispatcher: Arc<D>,
) -> Result<(), String>
where
    D: CommandDispatch + ?Sized,
    S: SessionSubstrate,
{
    let mut conn = Connection::new();
    let mut forwarders: HashMap<String, Forwarder> = HashMap::new();

    let result = loop {
        let Some(msg) = inbound.recv().await else {
            break Ok(()); // inbound closed — clean disconnect.
        };

        // Pre-attach live receivers for the kinds this frame references,
        // synchronously, BEFORE `handle` reads the cache. No await
        // between here and the snapshot read → no `store` can interleave
        // → the snapshot→live handoff loses nothing (see module docs).
        let mut pending: HashMap<String, watch::Receiver<Option<Arc<StateEnvelope>>>> =
            HashMap::new();
        for kind in frame_kinds(&msg) {
            if !forwarders.contains_key(&kind) && !pending.contains_key(&kind) {
                pending.insert(kind.clone(), substrate.subscribe_kind(&kind));
            }
        }

        let frames = match conn.handle(msg, &substrate, dispatcher.as_ref()).await {
            Ok(frames) => frames,
            Err(e) => break Err(e),
        };
        for frame in frames {
            if outbound.send(frame).await.is_err() {
                break; // outbound gone; fall through to teardown.
            }
        }

        reconcile_forwarders(&conn, &substrate, &outbound, &mut forwarders, pending);
    };

    // Always abort live forwarders on exit — a detached forwarder would
    // park on `changed()` forever, and its outbound clone would keep the
    // sender task's channel open.
    for (_, forwarder) in forwarders.drain() {
        forwarder.stop();
    }
    result
}

/// Reconcile the running forwarders against the connection's desired
/// rates: spawn new kinds (reusing the pre-attached receiver so the
/// no-lost-update guarantee holds), restart kinds whose rate changed,
/// and abort kinds no longer wanted.
fn reconcile_forwarders<S: SessionSubstrate>(
    conn: &Connection,
    substrate: &S,
    outbound: &mpsc::Sender<ServerMessage>,
    forwarders: &mut HashMap<String, Forwarder>,
    mut pending: HashMap<String, watch::Receiver<Option<Arc<StateEnvelope>>>>,
) {
    let desired = desired_rates(conn);

    for (kind, rate) in &desired {
        // Keep an existing forwarder iff its rate is unchanged.
        if matches!(forwarders.get(kind), Some(f) if f.rate == *rate) {
            continue;
        }
        // Rate changed → stop the stale one first.
        if let Some(stale) = forwarders.remove(kind) {
            stale.stop();
        }
        // Reuse the pre-attached receiver (closes the race); only a
        // rate-change restart on an already-live kind subscribes fresh,
        // where a brief gap is fine because the client already has the
        // latest.
        let rx = pending
            .remove(kind)
            .unwrap_or_else(|| substrate.subscribe_kind(kind));
        let handle = tokio::spawn(forward_kind(rx, *rate, outbound.clone()));
        forwarders.insert(
            kind.clone(),
            Forwarder {
                handle,
                rate: *rate,
            },
        );
    }

    // Abort forwarders whose kind is no longer subscribed or observed.
    let dropped: HashSet<String> = forwarders
        .keys()
        .filter(|k| !desired.contains_key(*k))
        .cloned()
        .collect();
    for kind in dropped {
        if let Some(gone) = forwarders.remove(&kind) {
            gone.stop();
        }
    }
    // Any `pending` receivers not consumed belong to kinds that didn't
    // end up desired (e.g. a re-Subscribe that dropped a kind in the
    // same frame it was named — impossible today, but harmless): drop.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::substrate::Substrate;
    use async_trait::async_trait;
    use positron_core::session::KindRevision;
    use positron_core::wire::{CommandEnvelope, CommandSource, ObserverSpec, StateLayer};
    use std::sync::Mutex;
    use uuid::Uuid;

    fn envelope(kind: &str, revision: u64) -> StateEnvelope {
        StateEnvelope {
            kind: kind.to_string(),
            revision: Some(revision),
            layer: StateLayer::Session,
            payload: serde_json::json!({ "rev": revision }),
        }
    }

    struct ScriptedDispatcher {
        calls: Mutex<Vec<CommandEnvelope>>,
        outcome: Result<(), String>,
    }
    impl ScriptedDispatcher {
        fn ok() -> Arc<Self> {
            Arc::new(Self {
                calls: Mutex::new(Vec::new()),
                outcome: Ok(()),
            })
        }
        fn err(msg: &str) -> Arc<Self> {
            Arc::new(Self {
                calls: Mutex::new(Vec::new()),
                outcome: Err(msg.to_string()),
            })
        }
        fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }
    }
    #[async_trait]
    impl CommandDispatch for ScriptedDispatcher {
        async fn dispatch(&self, envelope: CommandEnvelope) -> Result<(), String> {
            self.calls.lock().unwrap().push(envelope);
            self.outcome.clone()
        }
    }

    fn subscribe(kinds: &[&str]) -> ClientMessage {
        ClientMessage::Subscribe {
            kinds: kinds.iter().map(|k| k.to_string()).collect(),
            layers: vec![StateLayer::Session],
            last_seen: vec![],
        }
    }

    fn state_rev(msg: &ServerMessage) -> Option<u64> {
        match msg {
            ServerMessage::State(env) => env.revision,
            _ => None,
        }
    }

    // what this catches: the whole 2D-3 reason for existing — after a
    // Subscribe, a subsequent `Substrate::store` must reach the client
    // as a live `State` frame WITHOUT another request. A regression that
    // only served the snapshot (never attached the forwarder) leaves
    // #794 unfixed; this fails immediately.
    #[tokio::test]
    async fn subscribe_then_store_delivers_a_live_state_frame() {
        let substrate = Substrate::new();
        substrate.store(envelope("chat", 1));

        let (in_tx, in_rx) = mpsc::channel(8);
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let task = tokio::spawn(run_session(
            in_rx,
            out_tx,
            substrate.clone(),
            ScriptedDispatcher::ok(),
        ));

        in_tx.send(subscribe(&["chat"])).await.unwrap();

        // Snapshot first (revision 1). Receiving it proves the frame was
        // processed AND — since the receiver is created before the cache
        // read — that the forwarder's receiver is already attached at
        // revision 1.
        let snapshot = out_rx.recv().await.expect("snapshot frame");
        assert_eq!(
            state_rev(&snapshot),
            Some(1),
            "snapshot is the current revision"
        );

        // A later store must arrive live, exactly once, at revision 2.
        substrate.store(envelope("chat", 2));
        let live = out_rx.recv().await.expect("live frame");
        assert_eq!(
            state_rev(&live),
            Some(2),
            "the store fanned out as a live State frame"
        );

        drop(in_tx);
        task.await.unwrap().unwrap();
    }

    // what this catches: a store to a kind this connection did NOT
    // subscribe to must never be forwarded. A regression that fanned
    // every kind to every connection would leak unrelated state (and
    // waste the socket). We assert the NEXT frame after a store to the
    // subscribed kind is the subscribed one — the unrelated store in
    // between produced nothing.
    #[tokio::test]
    async fn store_to_unsubscribed_kind_is_not_forwarded() {
        let substrate = Substrate::new();
        substrate.store(envelope("chat", 1));

        let (in_tx, in_rx) = mpsc::channel(8);
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let task = tokio::spawn(run_session(
            in_rx,
            out_tx,
            substrate.clone(),
            ScriptedDispatcher::ok(),
        ));

        in_tx.send(subscribe(&["chat"])).await.unwrap();
        let _snapshot = out_rx.recv().await.expect("snapshot");

        // Store to an UNsubscribed kind — must produce no frame.
        substrate.store(envelope("presence", 5));
        // Then store to the subscribed kind — must produce a frame.
        substrate.store(envelope("chat", 2));

        let next = out_rx.recv().await.expect("a frame");
        assert_eq!(
            state_rev(&next),
            Some(2),
            "the next frame is the chat store; the presence store was not forwarded"
        );

        drop(in_tx);
        task.await.unwrap().unwrap();
    }

    // what this catches: re-Subscribe is declarative-replace, and the
    // forwarder set must track it — a kind dropped from the subscription
    // must stop streaming. A regression that only ever ADDED forwarders
    // would keep leaking the old kind after the client stopped wanting
    // it.
    #[tokio::test]
    async fn resubscribe_drops_the_old_kind_forwarder() {
        let substrate = Substrate::new();
        substrate.store(envelope("chat", 1));
        substrate.store(envelope("presence", 1));

        let (in_tx, in_rx) = mpsc::channel(8);
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let task = tokio::spawn(run_session(
            in_rx,
            out_tx,
            substrate.clone(),
            ScriptedDispatcher::ok(),
        ));

        in_tx.send(subscribe(&["chat"])).await.unwrap();
        let _ = out_rx.recv().await.expect("chat snapshot");

        // Switch to presence only. chat must stop; presence must start.
        in_tx.send(subscribe(&["presence"])).await.unwrap();
        let _ = out_rx.recv().await.expect("presence snapshot");

        // A chat store now must NOT be forwarded; a presence store must.
        substrate.store(envelope("chat", 2));
        substrate.store(envelope("presence", 2));

        let next = out_rx.recv().await.expect("a frame");
        assert_eq!(
            state_rev(&next),
            Some(2),
            "presence live frame arrives; the dropped chat kind did not forward"
        );
        // And confirm it was the presence kind, not a stray chat frame.
        match next {
            ServerMessage::State(env) => assert_eq!(env.kind, "presence"),
            other => panic!("expected State, got {other:?}"),
        }

        drop(in_tx);
        task.await.unwrap().unwrap();
    }

    // what this catches: an Observe with budget_hz == 0 is snapshot-only
    // — it must deliver the snapshot but attach NO live forwarder. A
    // regression that always streamed would blow an AI observer's
    // perception budget.
    #[tokio::test]
    async fn zero_budget_observer_gets_snapshot_but_no_live_stream() {
        let substrate = Substrate::new();
        substrate.store(envelope("chat", 1));

        let (in_tx, in_rx) = mpsc::channel(8);
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let task = tokio::spawn(run_session(
            in_rx,
            out_tx,
            substrate.clone(),
            ScriptedDispatcher::ok(),
        ));

        in_tx
            .send(ClientMessage::Observe {
                spec: ObserverSpec {
                    observer_id: "maya".into(),
                    budget_hz: 0,
                    kinds: vec!["chat".into()],
                    layers: vec![StateLayer::Session],
                },
                last_seen: vec![],
            })
            .await
            .unwrap();

        let snapshot = out_rx.recv().await.expect("snapshot");
        assert_eq!(state_rev(&snapshot), Some(1));

        // A store must NOT be forwarded (0hz = snapshot-only). Then close
        // inbound; the ONLY frame that could appear before the task ends
        // would be an (illegal) live frame. try_recv after task end must
        // be empty.
        substrate.store(envelope("chat", 2));
        drop(in_tx);
        task.await.unwrap().unwrap();
        assert!(
            out_rx.try_recv().is_err(),
            "0hz observer received a live frame it should not have"
        );
    }

    // what this catches: a command still dispatches through the session
    // task, and a FAILED command surfaces its CommandFailed frame to the
    // originating connection. A regression that dropped command handling
    // when the streaming path was added would silently break writes.
    #[tokio::test]
    async fn command_failure_surfaces_command_failed() {
        let substrate = Substrate::new();
        let dispatcher = ScriptedDispatcher::err("policy denied");

        let (in_tx, in_rx) = mpsc::channel(8);
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let task = tokio::spawn(run_session(
            in_rx,
            out_tx,
            substrate.clone(),
            Arc::clone(&dispatcher),
        ));

        let cid = Uuid::from_u128(0xbeef);
        in_tx
            .send(ClientMessage::Command(CommandEnvelope {
                kind: "chat".into(),
                command: "chat/send".into(),
                params: serde_json::json!({ "text": "hi" }),
                correlation_id: cid,
                source: CommandSource::Human,
            }))
            .await
            .unwrap();

        let frame = out_rx.recv().await.expect("a frame");
        match frame {
            ServerMessage::CommandFailed {
                correlation_id,
                error,
            } => {
                assert_eq!(correlation_id, cid);
                assert_eq!(error, "policy denied");
            }
            other => panic!("expected CommandFailed, got {other:?}"),
        }
        assert_eq!(
            dispatcher.call_count(),
            1,
            "the command reached the dispatcher"
        );

        drop(in_tx);
        task.await.unwrap().unwrap();
    }

    // what this catches: a reconnect that already carries the current
    // revision in last_seen must NOT re-receive the snapshot (the
    // exact-equality skip must survive into the session task), AND the
    // forwarder must still attach so the NEXT store is delivered live.
    // A regression that either re-sent the snapshot or failed to attach
    // the forwarder on a skipped subscribe would break resync.
    #[tokio::test]
    async fn matching_last_seen_skips_snapshot_but_still_streams_live() {
        let substrate = Substrate::new();
        substrate.store(envelope("chat", 7));

        let (in_tx, in_rx) = mpsc::channel(8);
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let task = tokio::spawn(run_session(
            in_rx,
            out_tx,
            substrate.clone(),
            ScriptedDispatcher::ok(),
        ));

        // Reconnect already at revision 7 → snapshot skipped.
        in_tx
            .send(ClientMessage::Subscribe {
                kinds: vec!["chat".into()],
                layers: vec![StateLayer::Session],
                last_seen: vec![KindRevision {
                    kind: "chat".into(),
                    revision: 7,
                }],
            })
            .await
            .unwrap();

        // The next store must be the FIRST frame we see (no snapshot 7
        // preceded it), and it must be revision 8.
        substrate.store(envelope("chat", 8));
        let live = out_rx.recv().await.expect("live frame");
        assert_eq!(
            state_rev(&live),
            Some(8),
            "snapshot 7 was skipped (exact-equality), forwarder still attached and streamed 8"
        );

        drop(in_tx);
        task.await.unwrap().unwrap();
    }
}
