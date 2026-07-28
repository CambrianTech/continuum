//! The grid-overflow EFFECTOR (governor consumer slice 4b-ii) — composes the tested
//! decision path into the live per-persona adapter override that
//! [`super::supervisor::materialize_adapters`] consumes.
//!
//! When the node is over local capacity (`ServingPlan.grid_overflow_lanes > 0`) and a
//! reachable peer already holds a persona's model, this routes HER BRAIN to that peer: her
//! `DeliberationModelBinding.adapter` becomes an [`AircRemoteInferenceAdapter`] over airc, so
//! her inference crosses the grid transparently — the exact re-home the binding was designed
//! for. The persona doesn't know or care that her model runs on another machine.
//!
//! ## Defensive by construction — safe to ship before the live smoke
//!
//! The closure returns `None` (→ the local factory adapter, zero behavior change) on ANY
//! uncertainty: airc not yet attached, no serving plan, no overflow, no matching footprint, or
//! no residency-eligible reachable peer. So it can only ever be a **safe no-op or a correct
//! remote route** — never a self-route (this node's own peer is excluded via `airc.peer_id()`,
//! so its own residency-beacon loopback can't select itself) and never a panic. The one thing
//! the unit path can't prove is that the remote hop SUCCEEDS — that is what the live two-node
//! smoke validates; a hop that can't warm surfaces as a loud per-slot `AdapterWarmup` failure
//! ([[fallbacks-are-illegal-fail-loud]]), never a silent local downgrade.

use std::sync::Arc;

use airc_lib::Airc;
use tokio::sync::OnceCell;

use crate::ai::adapter::AIProviderAdapter;
use crate::capacity::gossip::global_ledger;
use crate::capacity::grid_overflow::route_grid_overflow;
use crate::capacity::model_residency::global_residency_ledger;
use crate::capacity::DeviceCapacity;
use crate::inference::airc_remote::adapter::AircRemoteInferenceAdapter;
use crate::inference::airc_remote::transport::AircLiveTransport;
use crate::modules::serving_daemon::ServingDaemonModule;
use crate::persona::inference_profile::PersonaInferenceProfile;

/// Headroom kept free on a peer before it may accept an overflow lane — same 1 GiB spirit as
/// the single-device [`crate::capacity::grid::LocalFirstFitPolicy`] safety margin.
const OVERFLOW_SAFETY_MARGIN_BYTES: u64 = 1024 * 1024 * 1024;

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Build the effector closure `spawn_all` / `materialize_adapters` consume. Captures the
/// late-bound airc handle cell (shared with the interceptor) and the serving daemon (the live
/// plan + footprint source). See the module docs for the defensive contract.
pub fn build_overflow_effector(
    airc_cell: Arc<OnceCell<Arc<Airc>>>,
    serving: Arc<ServingDaemonModule>,
) -> impl Fn(&PersonaInferenceProfile, usize) -> Option<Arc<dyn AIProviderAdapter>> {
    move |profile, _slot| {
        // airc not attached yet → local (the boot window before attach_as fills the cell).
        let airc = airc_cell.get()?.clone();
        // No plan, or demand fits locally → local. grid_overflow_lanes is the honest
        // "over local capacity by N" signal; 0 means nothing to spill.
        let plan = serving.compute_plan()?;
        if plan.grid_overflow_lanes == 0 {
            return None;
        }
        // The footprint for THIS persona's model — the lease's per-lane prefill spike.
        // Absent (model not a live candidate) → local, never a fabricated footprint.
        let footprint = serving
            .live_candidates()
            .into_iter()
            .find(|f| f.model_id == profile.model_id)?;
        let lease =
            footprint.grid_lease_request(plan.served_context_window, plan.grid_overflow_lanes);

        // Own peer id from the airc handle → excludes self from the residency view + gossip
        // snapshot (this node's own beacon loopback must never select itself as the target).
        let own = airc.peer_id().as_uuid();
        let now = now_ms();
        let residency = global_residency_ledger().view(own, now);
        // Overflow placement is REMOTE-ONLY, so local capacity is never read — a zeroed local
        // is the honest input (route_grid_overflow only ever inspects the peer list).
        let snapshot = global_ledger().snapshot(
            own,
            DeviceCapacity {
                gpu_total_bytes: 0,
                gpu_free_bytes_live: 0,
                system_ram_free_bytes: 0,
            },
            now,
        );

        let routing = route_grid_overflow(
            &profile.model_id,
            &lease,
            &residency,
            &snapshot,
            OVERFLOW_SAFETY_MARGIN_BYTES,
        );
        // First-cut assignment: this persona takes the first placed peer. No residency-eligible
        // reachable peer with room → local (queue/degrade is the planner's, not a silent drop).
        let (peer, _lanes) = routing.remote.first()?;
        let peer_uuid = peer.as_uuid();

        let transport = AircLiveTransport::new(airc, peer_uuid);
        let adapter = AircRemoteInferenceAdapter::new(transport);
        crate::probe!(
            class = "grid.overflow.route",
            persona = %profile.persona_name,
            model = %profile.model_id,
            peer = %peer_uuid,
            overflow_lanes = plan.grid_overflow_lanes,
            "routing persona brain OFF-BOX to a residency-eligible peer (grid overflow)",
        );
        Some(Arc::new(adapter) as Arc<dyn AIProviderAdapter>)
    }
}
