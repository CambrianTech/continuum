//! GridCapacityModule — this node's capacity-offer gossip publisher (#56 step 4).
//!
//! Every [`PUBLISH_INTERVAL_MS`] tick it reads the resource authority's live board (the
//! SAME two-axis truth the prefill valve and serving plan derive from — one source, no
//! parallel probe) and broadcasts a [`CapacityOffer`] over airc as an
//! `EphemeralCoalesced` `grid_capacity` realtime envelope: presence-of-compute, latest
//! wins, never replayed. The receive half lives in `airc::inbound_attach`, which folds
//! heard offers (our own echo included — the loopback proof) into
//! [`capacity::gossip::global_ledger`], whose snapshot IS the sim-proven `GridSnapshot`.
//!
//! Module shape per the concurrency style guide: no new tokio task, no new monitor —
//! the runtime's tick cadence drives it; the board read is a lock-free watch snapshot;
//! the publish is one small envelope through the existing `airc/realtime-publish`
//! command surface (the same path chat rides). The offer's room is the node's
//! DISCOVERED default room (AircModule discovery, same dep the persona bootstrap and
//! node-presence emitter consume) — never a hardcoded id (#124).

use std::any::Any;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::capacity::gossip::{global_ledger, CapacityOffer, PUBLISH_INTERVAL_MS};
use crate::resources::{ResourceDaemon, ResourceKind};
use crate::runtime::{
    CommandExecutor, CommandResult, LateBound, ModuleConfig, ModulePriority, ServiceModule,
};
use airc_core::RoomId;

pub struct GridCapacityModule {
    resource_daemon: Arc<ResourceDaemon>,
    /// The node's discovered default room — where the grid rendezvous happens today.
    room: RoomId,
    executor_slot: Arc<LateBound<CommandExecutor>>,
    /// Last probed free-GB value — the glass box speaks on CHANGE, not on every beat
    /// (a steady offer is silence; u64::MAX = never probed, so the first beat speaks).
    last_probed_free_gb: AtomicU64,
}

impl GridCapacityModule {
    pub fn new(resource_daemon: Arc<ResourceDaemon>, room: RoomId) -> Self {
        Self {
            resource_daemon,
            room,
            executor_slot: Arc::new(LateBound::new("grid-capacity::executor")),
            last_probed_free_gb: AtomicU64::new(u64::MAX),
        }
    }

    /// Build this node's offer from the authority's live board. `None` when VRAM is
    /// ungoverned (no monitor) — an ungoverned node has no honest number to offer,
    /// and offering a guess would be exactly the fabricated-capacity lie the grid's
    /// per-node-fit honesty exists to prevent.
    fn current_offer(&self) -> Option<CapacityOffer> {
        let board = self.resource_daemon.board();
        let vram = board.kinds.iter().find(|k| k.kind == ResourceKind::Vram)?;
        let ram_free = board
            .kinds
            .iter()
            .find(|k| k.kind == ResourceKind::Ram)
            .map(|k| k.available_bytes)
            .unwrap_or(0);
        Some(CapacityOffer {
            gpu_total_bytes: vram.capacity_bytes,
            gpu_free_bytes_live: vram.available_bytes,
            system_ram_free_bytes: ram_free,
            at_ms: now_ms(),
        })
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[async_trait]
impl ServiceModule for GridCapacityModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "grid-capacity",
            priority: ModulePriority::Background,
            command_prefixes: &[],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: Some(Duration::from_millis(PUBLISH_INTERVAL_MS)),
        }
    }

    async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        Err(format!(
            "grid-capacity has no command surface yet — '{command}' (offers publish on the \
             module tick; read the grid via capacity::gossip::global_ledger)"
        ))
    }

    async fn tick(&self) -> Result<(), String> {
        // Boot ordering: a beat or two may fire before start_server installs the
        // executor — transient, skip (the next beat publishes). Not a fallback: the
        // installed state is guaranteed by the same path every module rides.
        let Some(executor) = self.executor_slot.cloned() else {
            return Ok(());
        };
        let Some(offer) = self.current_offer() else {
            // Ungoverned VRAM — nothing honest to offer. Visible, not spammy: the
            // no-offer state surfaces once per free-GB "change" via the MAX sentinel.
            if self
                .last_probed_free_gb
                .swap(u64::MAX - 1, Ordering::AcqRel)
                != u64::MAX - 1
            {
                crate::probe!(
                    class = "grid.capacity.ungoverned",
                    "VRAM ungoverned on this node — no capacity offer published",
                );
            }
            return Ok(());
        };

        let envelope = json!({
            "eventId": uuid::Uuid::new_v4().to_string(),
            "roomId": self.room.as_uuid().to_string(),
            "sourceId": "grid-capacity",
            "createdAtMs": offer.at_ms,
            "delivery": "ephemeral_coalesced",
            "payload": {
                "kind": "existing_schema",
                "payload": {
                    "schema": "grid_capacity",
                    "inline": serde_json::to_value(offer)
                        .map_err(|e| format!("capacity offer encode failed: {e}"))?,
                }
            },
        });
        executor
            .execute_json("airc/realtime-publish", json!({ "envelope": envelope }))
            .await
            .map_err(|e| format!("grid-capacity offer publish failed: {e}"))?;

        // Glass box: speak on change (rounded GB), silent on a steady offer.
        let free_gb = offer.gpu_free_bytes_live / 1_000_000_000;
        if self.last_probed_free_gb.swap(free_gb, Ordering::AcqRel) != free_gb {
            crate::probe!(
                class = "grid.capacity.offer",
                free_gb = free_gb,
                total_gb = (offer.gpu_total_bytes / 1_000_000_000),
                heard_peers = global_ledger().heard_count(),
                "capacity offer published to the grid",
            );
        }
        Ok(())
    }

    fn install_executor(&self, executor: Arc<CommandExecutor>) {
        self.executor_slot.install(executor);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
