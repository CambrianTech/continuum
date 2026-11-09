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

use continuum_base::time::EpochMs;
use std::any::Any;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::capacity::gossip::{global_ledger, CapacityOffer, PUBLISH_INTERVAL_MS};
use crate::resources::{ResourceDaemon, ResourceKind};
use crate::runtime::{CommandExecutor, CommandResult, LateBound, ModuleConfig, ModulePriority, ServiceModule};
use airc_core::RoomId;

pub struct GridCapacityModule {
    resource_daemon: Arc<ResourceDaemon>,
    /// The node's discovered default room — where the grid rendezvous happens today.
    room: RoomId,
    executor_slot: Arc<LateBound<CommandExecutor>>,
    /// The module registry, to ask the genome module which bases this host trains (card
    /// 6f05b89d). Read at offer time, so the order modules initialize in never matters.
    modules: LateBound<crate::runtime::ModuleRegistry>,
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
            modules: LateBound::new("grid-capacity::modules"),
            last_probed_free_gb: AtomicU64::new(u64::MAX),
        }
    }

    /// Build this node's offer from the authority's live board. `None` when VRAM is
    /// ungoverned (no monitor) — an ungoverned node has no honest number to offer,
    /// and offering a guess would be exactly the fabricated-capacity lie the grid's
    /// per-node-fit honesty exists to prevent.
    fn current_offer(&self) -> Option<CapacityOffer> {
        offer_from_board(&self.resource_daemon.board(), EpochMs::now().millis(), local_bases(), |base| self.trains_base_here(base))
    }

    /// Whether a trainer on this host takes LoRA jobs for `base`. Without the genome module
    /// (not registered, or not yet initialized) nothing is trainable here: the honest answer.
    fn trains_base_here(&self, base: &str) -> bool {
        self.modules.cloned().and_then(|modules| modules.get_by_name("genome")).is_some_and(|module| {
            module
                .as_any()
                .downcast_ref::<crate::modules::genome::GenomeModule>()
                .is_some_and(|genome| genome.trains_base_here(base))
        })
    }
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

    async fn initialize(&self, ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
        self.modules.install(Arc::clone(&ctx.registry));
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
                continuum_base::probe!(
                    class = "grid.capacity.ungoverned",
                    "no governed memory on this node (neither VRAM nor RAM) — no capacity offer published",
                );
            }
            return Ok(());
        };

        // Published as this core's own scope peer: the beacon's author speaking for itself (a typed
        // id, never a name). No scope peer means the airc module never attached, so nothing to beacon on.
        let Some(source) = crate::persona::self_peer::scope() else {
            return Ok(());
        };
        let envelope = json!({
            "eventId": uuid::Uuid::new_v4().to_string(),
            "roomId": self.room.as_uuid().to_string(),
            "sourceId": source.to_string(),
            "createdAtMs": offer.at_ms,
            "delivery": "ephemeral_coalesced",
            "payload": {
                "kind": "existing_schema",
                "payload": {
                    "schema": "grid_capacity",
                    "inline": serde_json::to_value(&offer)
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
            continuum_base::probe!(
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

/// The offer a node can honestly make from its resource board. A GOVERNED VRAM kind
/// makes an accelerator offer; a governed RAM kind alone makes a CPU offer (gpu
/// bytes 0). Neither governed = no offer (a guess would be the fabricated-capacity
/// lie the grid's per-node-fit honesty exists to prevent). Before 2026-09-14 a
/// RAM-only host (IntelMac's CPU node) never beaconed at all — one offer all day,
/// invisible to grid/nodes, the fleet fold and placement (card deb26770).
pub(crate) fn offer_from_board(
    board: &crate::resources::LeaseBoard,
    at_ms: u64,
    local: Vec<String>,
    trains_here: impl Fn(&str) -> bool,
) -> Option<CapacityOffer> {
    let serving = crate::inference::llama_server::current_serving();
    let vram = board.kinds.iter().find(|k| k.kind == ResourceKind::Vram);
    let ram = board.kinds.iter().find(|k| k.kind == ResourceKind::Ram);
    if vram.is_none() && ram.is_none() {
        return None;
    }
    Some(CapacityOffer {
        origin: crate::capacity::gossip::this_process_origin(),
        gpu_total_bytes: vram.map(|k| k.capacity_bytes).unwrap_or(0), // JUSTIFIED unwrap_or: no VRAM kind = a CPU node offers 0 accelerator bytes, which is the truth
        gpu_free_bytes_live: vram.map(|k| k.available_bytes).unwrap_or(0), // JUSTIFIED unwrap_or: same — absence of an accelerator is 0 bytes of it
        system_ram_free_bytes: ram.map(|k| k.available_bytes).unwrap_or(0), // JUSTIFIED unwrap_or: a VRAM-only board (no RAM kind governed) offers 0 RAM rather than a guess
        at_ms,
        build: crate::capacity::gossip::build_from_sha(env!("CONTINUUM_BUILD_GIT_SHA")),
        build_number: env!("CONTINUUM_BUILD_NUMBER").parse().unwrap_or(0), // JUSTIFIED unwrap_or: a build with no number beacons 0 = unknown, never a fake ordering
        served_model: serving.active_model.clone(),
        lanes: serving.lanes,
        residents: crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
            .map(|r| r.live_personas().len() as u32)
            .unwrap_or(0), // JUSTIFIED unwrap_or: no registry yet = no residents, the truth at boot
        // The seat's own admission truth, not roster arithmetic: lanes minus everything in
        // flight on them, local and leased-in alike, minus the slots it has GRANTED to
        // spillers that have not arrived yet (card c84d885a, S1) — one owner for the number.
        free_slots_live: crate::persona::placement_reservation::free_slots_live_now(serving.lanes, at_ms),
        lane_wait_p50_ms: crate::inference::resource_admission::leased_in_wait_p50_ms().0,
        lane_wait_samples: crate::inference::resource_admission::leased_in_wait_p50_ms().1,
        served_context_window: serving.served_context_window,
        // This seat's measured speed for what it serves, so peers can ORDER placement by it
        // (card 86fef3c3); unmeasured beacons 0.
        decode_tps_milli: serving
            .active_model
            .as_deref()
            .map_or(0, |model| crate::capacity::gossip::beacon_decode_milli(&crate::inference::decode_knee::rate_for(model))),
        // The affirmative capability list — one emission source (card 8bd75362, doctrine #3941):
        // a gate checks membership here and refuses on absence; an older core advertises
        // nothing (serde default), so typed absence is safe by construction.
        capabilities: crate::capacity::gossip::this_build_capabilities(),
        // What a peer may lease training to us for (card 6f05b89d).
        trainable_bases: trainable_bases(serving.active_model.as_deref(), local, trains_here),
        // The residents this node last judged stalled or regressed (card 1c157668).
        resident_alarms: crate::modules::citizen_health::current_resident_alarms(),
    })
}

/// The registry models whose weights are on this node (a resolved local GGUF): the bases a
/// trainer here could load without a download.
fn local_bases() -> Vec<String> {
    crate::model_registry::try_global()
        .map(|registry| registry.models().filter(|model| model.gguf_local_path.is_some()).map(|model| model.id.clone()).collect())
        .unwrap_or_default() // unwrap_or_default: no registry yet = no local bases, the truth at boot
}

/// Every base a peer may lease training to this node for: the served one and every base whose
/// weights are here, each only when a trainer on this host would take it (`trains_here`, the
/// coordinator's own gate). Only the served model used to be asked, so a node serving one base
/// never advertised another its engine lane would train from local weights, and a lease for
/// that base found no receiver (Codex on card 92cd2251). Sorted and unique, so the beacon is
/// stable. Admission is the lease's own (a lane's headroom is checked when it trains).
fn trainable_bases(served: Option<&str>, local: Vec<String>, trains_here: impl Fn(&str) -> bool) -> Vec<String> {
    let mut bases: Vec<String> = served.map(str::to_owned).into_iter().chain(local).collect();
    bases.sort();
    bases.dedup();
    bases.retain(|base| trains_here(base));
    bases
}

#[cfg(test)]
mod tests {
    use super::*;

    /// what this catches: a RAM-only node never beaconing (invisible to the grid).
    #[test]
    fn a_ram_only_board_offers_cpu_capacity_and_an_empty_board_offers_nothing() {
        use crate::resources::{KindLedger, LeaseBoard};
        let ram_only = LeaseBoard {
            kinds: vec![KindLedger { kind: ResourceKind::Ram, capacity_bytes: 16 << 30, granted_bytes: 0, available_bytes: 8 << 30, measured_bytes: 0, physical_used_bytes: 0, external_bytes: 0, lease_count: 0 }],
            leases: Vec::new(),
            attributions: Vec::new(),
        };
        let o = offer_from_board(&ram_only, 7, Vec::new(), |_| false).expect("RAM alone is a capacity"); // JUSTIFIED: the invariant under test
        assert_eq!(o.gpu_total_bytes, 0);
        assert_eq!(o.system_ram_free_bytes, 8 << 30);
        assert!(offer_from_board(&LeaseBoard { kinds: Vec::new(), leases: Vec::new(), attributions: Vec::new() }, 7, Vec::new(), |_| false).is_none(), "nothing governed = no offer");
    }

    // what this catches (Codex on card 92cd2251): a node advertising only the base it serves,
    // so a base its engine lane would train from local weights never reached the grid and a
    // lease for it found no receiver. Every local base a trainer here takes is advertised,
    // sorted and unique; one no trainer takes is not; with nothing served the locals still count.
    #[test]
    fn every_base_a_trainer_here_takes_is_advertised_not_only_the_served_one() {
        let trainable = |base: &str| base != "no-trainer";
        let local = vec!["qwen-0.5b".to_string(), "no-trainer".to_string(), "qwen-1.5b".to_string()];
        assert_eq!(trainable_bases(Some("qwen-1.5b"), local.clone(), trainable), ["qwen-0.5b", "qwen-1.5b"]);
        assert_eq!(trainable_bases(None, local, trainable), ["qwen-0.5b", "qwen-1.5b"]);
        assert!(trainable_bases(Some("qwen-1.5b"), Vec::new(), |_| false).is_empty(), "no trainer here, nothing advertised");
    }
}
