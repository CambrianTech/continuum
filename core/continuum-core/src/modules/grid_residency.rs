//! GridResidencyModule — this node's residency-beacon gossip publisher (grid-overflow slice 3).
//!
//! The residency sibling of [`super::grid_capacity::GridCapacityModule`]. Every
//! [`RESIDENCY_PUBLISH_INTERVAL_MS`] tick it reads the SAME live serving plan the daemon
//! already computes (one source, no parallel probe) and broadcasts a [`ResidencyBeacon`] over
//! airc as an `EphemeralCoalesced` `grid_residency` realtime envelope: which models this node
//! holds resident — the grid-overflow ELIGIBILITY signal. The receive half lives in
//! [`crate::airc::inbound_attach`], which folds heard beacons (our own echo included — the
//! loopback proof) into [`crate::capacity::model_residency::global_residency_ledger`], whose
//! `view()` IS the `ModelResidencyView` the governor composes with the capacity snapshot.
//!
//! ## Why a SEPARATE module + slower cadence
//!
//! Residency and capacity are orthogonal (settled with BigMama 2026-07-27): capacity is
//! free-VRAM RIGHT NOW (10s beat, [`super::grid_capacity`]); residency is which models are warm
//! (minute-scale, changes only on page-in/out). Coupling them onto one envelope would either
//! over-publish residency or under-refresh capacity. Same module shape as the style guide
//! mandates — no new tokio task, the runtime tick drives it; the plan read is a lock-free
//! `watch` snapshot; the publish is one small envelope through the existing
//! `airc/realtime-publish` command surface (the same path capacity + chat ride).
//!
//! ## Today's local residency = the base model
//!
//! `ServingPlan` carries a single `base_model_id` + a `resident_models` COUNT (not a per-id
//! set), so today this node's honest resident set is `[base_model_id]`. When multi-model
//! residency lands (a per-id resident list on the plan), only [`Self::current_beacon`] changes
//! — the wire, ledger, and governor compose are already model-set-shaped.

use std::any::Any;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::watch;

use crate::capacity::model_residency::{
    global_residency_ledger, ResidencyBeacon, RESIDENCY_PUBLISH_INTERVAL_MS,
};
use crate::cognition::serving_plan::ServingPlan;
use crate::runtime::{
    CommandExecutor, CommandResult, LateBound, ModuleConfig, ModulePriority, ServiceModule,
};
use airc_core::RoomId;
use std::sync::Arc;

pub struct GridResidencyModule {
    /// Lock-free live snapshot of the daemon's serving plan — the SAME source the prefill
    /// valve and serving control derive from (no parallel probe).
    plan_rx: watch::Receiver<Option<ServingPlan>>,
    /// The node's discovered default room — where the grid rendezvous happens today.
    room: RoomId,
    executor_slot: Arc<LateBound<CommandExecutor>>,
    /// Last published resident set — the glass box speaks on CHANGE, not on every beat (a
    /// steady residency is silence). `None` = never published, so the first real plan speaks.
    last_published: Mutex<Option<Vec<String>>>,
}

impl GridResidencyModule {
    pub fn new(plan_rx: watch::Receiver<Option<ServingPlan>>, room: RoomId) -> Self {
        Self {
            plan_rx,
            room,
            executor_slot: Arc::new(LateBound::new("grid-residency::executor")),
            last_published: Mutex::new(None),
        }
    }

    /// Build this node's residency beacon from the live serving plan. `None` when no plan is
    /// computed yet (nothing resident to advertise) — honest silence, never a fabricated set.
    /// Today the resident set is `[base_model_id]`; a multi-model plan extends only this line.
    fn current_beacon(&self) -> Option<ResidencyBeacon> {
        let plan = self.plan_rx.borrow().clone()?;
        Some(ResidencyBeacon {
            resident_models: vec![plan.base_model_id],
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
impl ServiceModule for GridResidencyModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "grid-residency",
            priority: ModulePriority::Background,
            command_prefixes: &[],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: Some(Duration::from_millis(RESIDENCY_PUBLISH_INTERVAL_MS)),
        }
    }

    async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        Err(format!(
            "grid-residency has no command surface — '{command}' (beacons publish on the module \
             tick; read the grid via capacity::model_residency::global_residency_ledger)"
        ))
    }

    async fn tick(&self) -> Result<(), String> {
        // Boot ordering: a beat or two may fire before start_server installs the executor —
        // transient, skip (the next beat publishes). Same guaranteed-installed path as capacity.
        let Some(executor) = self.executor_slot.cloned() else {
            return Ok(());
        };
        let Some(beacon) = self.current_beacon() else {
            // No serving plan yet — nothing resident to advertise. Honest silence.
            return Ok(());
        };

        let envelope = json!({
            "eventId": uuid::Uuid::new_v4().to_string(),
            "roomId": self.room.as_uuid().to_string(),
            "sourceId": "grid-residency",
            "createdAtMs": beacon.at_ms,
            "delivery": "ephemeral_coalesced",
            "payload": {
                "kind": "existing_schema",
                "payload": {
                    "schema": "grid_residency",
                    "inline": serde_json::to_value(&beacon)
                        .map_err(|e| format!("residency beacon encode failed: {e}"))?,
                }
            },
        });
        executor
            .execute_json("airc/realtime-publish", json!({ "envelope": envelope }))
            .await
            .map_err(|e| format!("grid-residency beacon publish failed: {e}"))?;

        // Glass box: speak on change (resident set differs), silent on a steady residency.
        let mut last = self.last_published.lock().map_err(|e| format!("residency lock: {e}"))?;
        if last.as_deref() != Some(beacon.resident_models.as_slice()) {
            crate::probe!(
                class = "grid.residency.beacon",
                models = ?beacon.resident_models,
                heard_peers = global_residency_ledger().heard_count(),
                "residency beacon published to the grid",
            );
            *last = Some(beacon.resident_models);
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
