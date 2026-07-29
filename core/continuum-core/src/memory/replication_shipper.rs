//! Persona-RAID slice 2b — the write-behind SHIPPER.
//!
//! The RTOS-shape consumer of the [`ReplicationLedger`]: an owned task on a
//! fixed `interval` (the cadence IS the maximum amnesia window) that, per
//! journaled persona, reads the journal tail past the peer's acked high-water,
//! ships it to a residency-eligible peer's `memory/replicate-batch`, and
//! advances the high-water on ack. State is published via a `watch` snapshot.
//! Design: docs/architecture/PERSONA-RAID-WRITE-BEHIND.md.
//!
//! Concurrency (CONCURRENCY-STYLE-GUIDE): own task + `tokio::time::interval` +
//! `watch::Sender<ShipperSnapshot>` + injected deps (ledger, transport, peer
//! source). Shipping is best-effort write-behind — a slow/absent peer grows the
//! reported lag, NEVER blocks the cognition hot path (the tee is elsewhere).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tokio::sync::watch;

use crate::memory::replication::ReplicationLedger;
use crate::routing::command_uri::PeerRef;
use crate::routing::route_decision::RouteDecision;
use crate::routing::transport::Transport;
use crate::runtime::service_module::CommandResult;

/// How many journal entries ship per persona per tick — bounds one batch so a
/// long-cold persona catches up over several ticks instead of one huge frame.
const SHIP_BATCH_LIMIT: usize = 256;

/// Default cadence — the maximum amnesia window. One knob (design doc).
pub const DEFAULT_SHIP_INTERVAL: Duration = Duration::from_secs(30);

/// Chooses the peer to replicate a persona TO. Injected so the runtime can back
/// it with the live residency/grid view while tests use a fixed peer.
pub trait ReplicaPeerSource: Send + Sync {
    /// A residency-eligible peer to hold `persona_id`'s replica, or None when
    /// the grid has no eligible peer (then the persona stays single-copy —
    /// honestly `degraded_unreplicated`).
    fn replica_peer_for(&self, persona_id: &str) -> Option<PeerRef>;
}

/// Published shipper state — per persona, the last seq a peer has acked.
#[derive(Debug, Clone, Default)]
pub struct ShipperSnapshot {
    /// persona_id → (acked_seq, peer_label). Absent = never shipped.
    pub acked: HashMap<String, (u64, String)>,
    /// Ticks completed since boot (liveness).
    pub ticks: u64,
}

/// The shipper engine — owns the per-persona acked high-water, ships one tick
/// on demand. Split from the task loop so it is unit-testable with a fake
/// transport + peer source (no timers, no runtime).
pub struct ReplicationShipper {
    ledger: Arc<ReplicationLedger>,
    transport: Arc<dyn Transport>,
    peers: Arc<dyn ReplicaPeerSource>,
    acked: HashMap<String, (u64, String)>,
    snapshot_tx: watch::Sender<ShipperSnapshot>,
    ticks: u64,
}

impl ReplicationShipper {
    pub fn new(
        ledger: Arc<ReplicationLedger>,
        transport: Arc<dyn Transport>,
        peers: Arc<dyn ReplicaPeerSource>,
    ) -> (Self, watch::Receiver<ShipperSnapshot>) {
        let (snapshot_tx, snapshot_rx) = watch::channel(ShipperSnapshot::default());
        (
            Self {
                ledger,
                transport,
                peers,
                acked: HashMap::new(),
                snapshot_tx,
                ticks: 0,
            },
            snapshot_rx,
        )
    }

    /// One replication pass over all journaled personas. Best-effort: a persona
    /// whose peer is absent or whose ship fails is simply skipped this tick
    /// (its lag grows, reported in the snapshot next publish). Returns the
    /// number of personas whose high-water advanced.
    pub async fn tick(&mut self) -> usize {
        let mut advanced = 0;
        for persona_id in self.ledger.journaled_personas() {
            let after = self.acked.get(&persona_id).map(|(s, _)| *s).unwrap_or(0);
            let batch = self.ledger.read_tail(&persona_id, after, SHIP_BATCH_LIMIT);
            if batch.is_empty() {
                continue;
            }
            let Some(peer) = self.peers.replica_peer_for(&persona_id) else {
                continue; // no eligible peer — stays single-copy (honest degrade)
            };
            let peer_label = peer.to_string();
            let params = json!({ "persona_id": persona_id, "entries": batch });
            let decision = RouteDecision::Peer {
                peer,
                node: None,
                env: None,
                path: "memory/replicate-batch".to_string(),
                query: None,
                fragment: None,
            };
            match self.transport.dispatch(decision, params).await {
                Ok(result) => {
                    if let Some(acked_seq) = extract_acked_seq(&result) {
                        self.acked.insert(persona_id.clone(), (acked_seq, peer_label));
                        advanced += 1;
                    }
                }
                Err(e) => {
                    tracing::warn!(persona_id, "persona-RAID ship failed (lag grows): {e}");
                }
            }
        }
        self.ticks += 1;
        let _ = self.snapshot_tx.send(ShipperSnapshot {
            acked: self.acked.clone(),
            ticks: self.ticks,
        });
        advanced
    }

    /// Spawn the RTOS task: tick every `interval` until the process ends.
    /// Consumes self (moves into the task). The runtime holds the returned
    /// snapshot receiver for `serving/status`-style inspection.
    pub fn spawn(mut self, interval: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                self.tick().await;
            }
        })
    }
}

/// Pull `acked_seq` out of the receiver's CommandResult (the
/// `MemoryReplicateBatchResult` JSON). Absent/garbled → None (no advance).
fn extract_acked_seq(result: &CommandResult) -> Option<u64> {
    result
        .to_json_value()
        .ok()
        .and_then(|v| v.get("acked_seq").and_then(|s| s.as_u64()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::route_decision::RouteDecision;
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// Fake transport: records the persona_id + entry count it was asked to
    /// ship, and acks the highest seq in the batch (like the real receiver).
    #[derive(Debug, Default)]
    struct FakeTransport {
        shipped: Mutex<Vec<(String, usize)>>,
    }

    #[async_trait]
    impl Transport for FakeTransport {
        async fn dispatch(
            &self,
            decision: RouteDecision,
            params: serde_json::Value,
        ) -> Result<CommandResult, String> {
            let path = match &decision {
                RouteDecision::Peer { path, .. } => path.clone(),
                _ => return Err("unexpected decision".into()),
            };
            assert_eq!(path, "memory/replicate-batch");
            let pid = params["persona_id"].as_str().unwrap().to_string();
            let entries = params["entries"].as_array().unwrap();
            let max_seq = entries.iter().map(|e| e["seq"].as_u64().unwrap()).max().unwrap();
            self.shipped.lock().unwrap().push((pid, entries.len()));
            Ok(CommandResult::Json(json!({ "acked_seq": max_seq })))
        }
    }

    struct FixedPeer;
    impl ReplicaPeerSource for FixedPeer {
        fn replica_peer_for(&self, _persona_id: &str) -> Option<PeerRef> {
            Some(PeerRef::Name("peer-mac".to_string()))
        }
    }

    fn temp_ledger() -> Arc<ReplicationLedger> {
        let root = std::env::temp_dir().join(format!("raid-ship-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Arc::new(ReplicationLedger::new(
            root.join("personas"),
            root.join("replicas"),
            "node-a".into(),
        ))
    }

    // what this catches: the shipper must ship only the UNACKED tail and advance
    // its high-water on ack — re-shipping acked entries (no advance, or shipping
    // from seq 0 every tick) would flood the peer and never converge.
    #[tokio::test]
    async fn ships_tail_once_and_advances_highwater() {
        let ledger = temp_ledger();
        let pid = "persona-x";
        ledger.journal_admit(pid, "memory", &json!({"id": "m1"}));
        ledger.journal_admit(pid, "memory", &json!({"id": "m2"}));
        let transport = Arc::new(FakeTransport::default());
        let (mut shipper, mut rx) =
            ReplicationShipper::new(ledger.clone(), transport.clone(), Arc::new(FixedPeer));

        // Tick 1: ships both entries, acks seq 2.
        assert_eq!(shipper.tick().await, 1);
        rx.changed().await.unwrap();
        assert_eq!(rx.borrow().acked.get(pid).map(|(s, _)| *s), Some(2));

        // Tick 2 with no new admits: nothing to ship (tail empty past seq 2).
        assert_eq!(shipper.tick().await, 0);

        // A new admit: tick 3 ships ONLY the new entry.
        ledger.journal_admit(pid, "memory", &json!({"id": "m3"}));
        assert_eq!(shipper.tick().await, 1);
        let shipped = transport.shipped.lock().unwrap();
        assert_eq!(shipped.len(), 2, "two ships total (m1+m2, then m3)");
        assert_eq!(shipped[0], (pid.to_string(), 2), "first ship: 2 entries");
        assert_eq!(shipped[1], (pid.to_string(), 1), "second ship: 1 new entry only");
    }
}
