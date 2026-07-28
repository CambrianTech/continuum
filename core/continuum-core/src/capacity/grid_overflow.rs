//! Grid-overflow routing — the DECISION half of the governor consumer: given a serving plan
//! that overflowed local capacity, decide which eligible peers take the overflow lanes. The
//! EFFECT half (the actual `Commands.execute("ai/generate", {aircPeer})` hop) is thin and lives
//! at the live seam; THIS is pure, deterministic, and fully unit-tested.
//!
//! ## Why overflow placement is REMOTE-ONLY (not [`super::grid::LocalFirstFitPolicy`])
//!
//! [`super::grid::LocalFirstFitPolicy`] fills local first and floors local at `≥1` (a resident
//! model must be able to run one prefill). That floor is correct for a FRESH placement but
//! WRONG for overflow: overflow lanes are BY DEFINITION the ones that already could not fit
//! locally (`ServingPlan.grid_overflow_lanes = demand − local_lanes`). Placing them local-first
//! would re-cram the very lanes the planner just declared didn't fit — the thrash the honest
//! "over local capacity by N" signal exists to avoid. So overflow placement never touches local:
//! it spills ONLY to eligible remote peers.
//!
//! ## The two gates, composed (never absorbed)
//!
//! 1. RESIDENCY ([`ModelResidencyView::residency_eligible`]): a peer is a fast overflow target
//!    only for a model it ALREADY holds — else the hop pays a cold full-weights load, defeating
//!    the point. Filters the snapshot to peers holding the model.
//! 2. CONCURRENCY (the misfit-parts fit, [`super::lanes_that_fit`]): among residency-eligible +
//!    REACHABLE peers, each takes at most what its OWN free budget fits for the prefill spike —
//!    the same per-node-fit rule the single-device and grid policies run.
//!
//! Reachability is applied HERE (an unreachable-but-resident peer is a memory, not an offer),
//! composing cleanly with residency without either abstraction absorbing the other.
//!
//! ## Unplaced lanes are SURFACED, never dropped
//!
//! When no eligible peer can take a lane, it lands in [`OverflowRouting::unplaced`] — the honest
//! "the grid couldn't absorb N of your overflow" signal the caller queues or degrades on. Never
//! silently swallowed ([[fallbacks-are-illegal-fail-loud]]).

use crate::identity::PeerId;

use super::grid::GridSnapshot;
use super::lanes_that_fit;
use super::model_residency::ModelResidencyView;
use super::LeaseRequest;

/// The routing decision for a plan's overflow lanes: where each lane lands (remote-only) plus
/// the honest count of lanes the reachable, residency-eligible grid could NOT absorb.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverflowRouting {
    /// Overflow lanes placed on named peers, most-free-first, each capped by its own fit.
    pub remote: Vec<(PeerId, u32)>,
    /// Overflow lanes no eligible+reachable peer could take — queue or degrade on these,
    /// never drop them silently.
    pub unplaced: u32,
}

impl OverflowRouting {
    /// Total overflow lanes actually placed on peers.
    pub fn placed(&self) -> u32 {
        self.remote.iter().map(|(_, n)| n).sum()
    }
}

/// Decide where a plan's overflow lanes run. `lease` is the demand→capacity projection the
/// serving side already built (`ModelFootprint::grid_lease_request(served_window, overflow_lanes)`):
/// `want_concurrency` = the overflow lane count, `spike_bytes` = the per-lane prefill transient.
/// `model_id` is what the overflowing node is serving (its `ServingPlan.base_model_id`) — the
/// residency key. REMOTE-ONLY by construction (see module docs): local is already saturated.
pub fn route_grid_overflow(
    model_id: &str,
    lease: &LeaseRequest,
    residency: &ModelResidencyView,
    snapshot: &GridSnapshot,
    safety_margin_bytes: u64,
) -> OverflowRouting {
    let want = lease.want_concurrency;
    if want == 0 {
        return OverflowRouting { remote: Vec::new(), unplaced: 0 };
    }

    // Gate 1 — residency: keep only peers that hold the model resident.
    let eligible = residency.residency_eligible(snapshot, model_id);

    // Gate 2 — reachability + per-node fit: reachable eligible peers, most-free-first (fewest
    // peers touched), each capped by its OWN budget for the prefill spike.
    let mut reachable: Vec<_> = eligible.peers.iter().filter(|p| p.reachable).collect();
    reachable.sort_by(|a, b| {
        b.capacity
            .gpu_free_bytes_live
            .cmp(&a.capacity.gpu_free_bytes_live)
    });

    let mut remaining = want;
    let mut remote = Vec::new();
    for peer in reachable {
        if remaining == 0 {
            break;
        }
        let fit = lanes_that_fit(
            peer.capacity.gpu_free_bytes_live,
            safety_margin_bytes,
            lease.spike_bytes,
        );
        let take = fit.min(remaining);
        if take > 0 {
            remote.push((peer.peer, take));
            remaining -= take;
        }
    }

    // Whatever the reachable, residency-eligible grid couldn't absorb is surfaced, not dropped.
    OverflowRouting { remote, unplaced: remaining }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capacity::grid::{GridSnapshot, PeerCapacity};
    use crate::capacity::DeviceCapacity;
    use uuid::Uuid;

    const GB: u64 = 1024 * 1024 * 1024;

    fn peer_id(n: u128) -> PeerId {
        PeerId::from_uuid(Uuid::from_u128(n))
    }

    fn dev(free_gb: u64) -> DeviceCapacity {
        DeviceCapacity {
            gpu_total_bytes: 80 * GB,
            gpu_free_bytes_live: free_gb * GB,
            system_ram_free_bytes: 64 * GB,
        }
    }

    fn peer(n: u128, free_gb: u64, reachable: bool) -> PeerCapacity {
        PeerCapacity {
            peer: peer_id(n),
            capacity: dev(free_gb),
            reachable,
        }
    }

    fn lease(want: u32, spike_gb: u64) -> LeaseRequest {
        LeaseRequest {
            consumer: "qwen-coder".into(),
            want_concurrency: want,
            spike_bytes: spike_gb * GB,
        }
    }

    fn view_holding(peers: &[(u128, &[&str])]) -> ModelResidencyView {
        let mut v = ModelResidencyView::new();
        for (n, models) in peers {
            v.set_resident(peer_id(*n), models.iter().map(|s| s.to_string()));
        }
        v
    }

    // what this catches: overflow placement is REMOTE-ONLY and residency+reachability gated.
    // Local is never assigned lanes (it's the saturated node that overflowed). Only a peer that
    // holds the model AND is reachable takes lanes; a resident-but-unreachable peer and a
    // reachable-but-not-resident peer both take nothing.
    #[test]
    fn overflow_routes_remote_only_to_reachable_resident_peers() {
        let snap = GridSnapshot {
            local: dev(1), // saturated — must never receive overflow lanes
            peers: vec![
                peer(1, 40, true),  // resident + reachable → takes lanes
                peer(2, 40, false), // resident + UNREACHABLE → nothing
                peer(3, 40, true),  // reachable but NOT resident → nothing
            ],
        };
        let residency = view_holding(&[(1, &["qwen-coder"]), (2, &["qwen-coder"])]);

        let routing = route_grid_overflow("qwen-coder", &lease(2, 1), &residency, &snap, GB);

        assert_eq!(routing.remote.len(), 1, "only peer 1 is eligible + reachable");
        assert_eq!(routing.remote[0].0.as_uuid(), Uuid::from_u128(1));
        assert_eq!(routing.placed(), 2, "both overflow lanes fit on peer 1");
        assert_eq!(routing.unplaced, 0);
    }

    // what this catches: unplaced lanes are SURFACED, never dropped. When the eligible grid
    // can't fit all overflow lanes (one small peer, a big per-lane spike), the shortfall is
    // reported so the caller queues/degrades — the honest "grid couldn't absorb N" signal.
    #[test]
    fn lanes_the_grid_cannot_absorb_are_surfaced_not_dropped() {
        let snap = GridSnapshot {
            local: dev(1),
            peers: vec![peer(1, 10, true)], // ~10GB free, but each lane spikes 8GB
        };
        let residency = view_holding(&[(1, &["qwen-coder"])]);

        // want 3 lanes, 8GB spike each: only 1 fits on the 10GB peer (net of 1GB margin).
        let routing = route_grid_overflow("qwen-coder", &lease(3, 8), &residency, &snap, GB);

        assert_eq!(routing.placed(), 1, "only one lane fits the peer's budget");
        assert_eq!(routing.unplaced, 2, "the other two are surfaced, not silently dropped");
    }

    // what this catches: no overflow (want == 0) is a clean no-op — nothing placed, nothing
    // unplaced. The common case (demand fit locally, grid_overflow_lanes == 0) costs nothing.
    #[test]
    fn zero_overflow_is_a_clean_noop() {
        let snap = GridSnapshot {
            local: dev(20),
            peers: vec![peer(1, 40, true)],
        };
        let residency = view_holding(&[(1, &["qwen-coder"])]);

        let routing = route_grid_overflow("qwen-coder", &lease(0, 1), &residency, &snap, GB);

        assert!(routing.remote.is_empty());
        assert_eq!(routing.unplaced, 0);
        assert_eq!(routing.placed(), 0);
    }

    // what this catches: spill spreads across peers most-free-first, each capped by its OWN fit
    // (the misfit-parts rule) — 12 aggregate GB across three 4GB peers can't run a 6-lane
    // placement if no single peer fits it, but distinct small lanes DO spread. Two reachable
    // resident peers each take their share until demand is met.
    #[test]
    fn spill_spreads_across_peers_most_free_first() {
        let snap = GridSnapshot {
            local: dev(1),
            peers: vec![
                peer(1, 20, true), // more free → sorted first
                peer(2, 12, true),
            ],
        };
        let residency = view_holding(&[(1, &["qwen-coder"]), (2, &["qwen-coder"])]);

        // 4 lanes, 8GB spike: peer1 (20-1 margin=19 → 2 lanes), peer2 (12-1=11 → 1 lane) = 3,
        // one unplaced. Peer 1 (most free) is filled before peer 2.
        let routing = route_grid_overflow("qwen-coder", &lease(4, 8), &residency, &snap, GB);

        assert_eq!(routing.remote[0].0.as_uuid(), Uuid::from_u128(1), "most-free peer first");
        assert_eq!(routing.placed() + routing.unplaced, 4, "every lane accounted for");
        assert!(routing.placed() >= 3, "peers absorb what their own budgets fit");
    }
}
