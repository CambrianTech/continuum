//! Compute-lease routing (#56 keystone) — the decision brain that turns the live grid
//! snapshot into a per-request lane choice: run this inference LOCAL, or lease a peer's lane.
//!
//! This is the seam between two things that already exist:
//! - the live [`GridSnapshot`] from [`super::gossip`] (real capacity, real peers), and
//! - the inter-core RPC ([`crate::routing::airc_transport`], `AircCommandRequest` → peer),
//!
//! and it is deliberately PURE: `decide_lane` reads a snapshot + a request and returns a
//! [`LaneDecision`]. No I/O, no serving touched — so it is deterministic, unit-testable, and
//! provable against the same sim scenarios the placement policies pass. The transport wiring
//! (package the request, send via AircTransport, fold the response back into the turn) is a
//! thin follow-up that CALLS this brain; keeping the decision separable is what let it be
//! built and proven with a benchmark running and no deploy window.
//!
//! ## The compute-lease boundary (doctrine, load-bearing)
//!
//! A remote peer can only serve what crosses the wire as **text**
//! ([[compute-lease-boundary]]): a peer holds a model and returns tokens, nothing more. So a
//! request is remotely leasable ONLY if its work is text-in/text-out. Anything that needs
//! THIS node's local state — the persona's files/tools (a coding act), local vision/audio
//! artifacts, THIS machine's workspace — must run local, full stop. `Leasability` makes that
//! a typed precondition, not a hopeful comment: a non-leasable request never even consults
//! the peer list.
//!
//! ## Local-first, always
//!
//! Even a leasable request prefers local: leasing adds a network RTT, so it is worth it only
//! when local truly can't serve now (the fit says zero local lanes fit the live free GPU).
//! This is the single-machine-first invariant — a node exhausts itself before spending the
//! grid — and it means a partitioned node degrades to exactly local-only, never blocks.

use super::grid::GridSnapshot;
use super::{lanes_that_fit, DeviceCapacity, LeaseRequest};
use crate::identity::PeerId;

/// Whether an inference request may cross the compute-lease boundary (text-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leasability {
    /// Text-in/text-out — a peer can serve it (a Speak turn, a pure-reasoning deliberation).
    TextOnly,
    /// Needs THIS node's local state (files/tools/vision/workspace) — must run local.
    LocalOnly,
}

/// Where a single inference request should run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneDecision {
    /// Run on this node's own serving lane (the default and the fallback).
    Local {
        /// Why local — for the glass box.
        reason: LocalReason,
    },
    /// Lease this peer's lane over airc (text-only work, local exhausted, peer fits).
    Remote { peer: PeerId },
}

/// Why a request stayed local — every non-remote path names itself (no silent default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalReason {
    /// The request needs local state — the compute-lease boundary forbids remoting it.
    NotLeasable,
    /// Local free GPU can serve it now — no reason to pay a network hop.
    LocalFits,
    /// Local is exhausted AND no reachable peer can fit it either — serve local anyway
    /// (queue at our own lane; never block, never strand on a peer that can't serve).
    NoPeerFits,
}

/// Decide the lane for ONE inference request against the LIVE grid snapshot.
///
/// The order encodes the doctrine: leasability gate → local-fit → most-free reachable peer →
/// local fallback. `safety_margin_bytes` is the same headroom the [`super::FitPolicy`] and the
/// prefill throttle use — one fit rule everywhere ([[capacity-fabric-live-never-block-sim-as-gym]]).
pub fn decide_lane(
    snapshot: &GridSnapshot,
    req: &LeaseRequest,
    leasability: Leasability,
    safety_margin_bytes: u64,
) -> LaneDecision {
    // Gate 1 — the compute-lease boundary. Non-text work never leaves this node.
    if leasability == Leasability::LocalOnly {
        return LaneDecision::Local { reason: LocalReason::NotLeasable };
    }

    // Gate 2 — local-first. If our own live free GPU fits even one spike, serve here: a
    // network RTT is only worth paying when local genuinely can't.
    let local_fits = fits_one(&snapshot.local, req, safety_margin_bytes);
    if local_fits {
        return LaneDecision::Local { reason: LocalReason::LocalFits };
    }

    // Gate 3 — lease the most-free REACHABLE peer that fits. Most-free-first keeps the busiest
    // node's spare capacity for the demand and touches the fewest peers. Unreachable peers are
    // memories, not offers (freshness already filtered them out of the snapshot's `reachable`).
    let best = snapshot
        .peers
        .iter()
        .filter(|p| p.reachable && fits_one(&p.capacity, req, safety_margin_bytes))
        .max_by_key(|p| p.capacity.gpu_free_bytes_live);
    match best {
        Some(peer) => LaneDecision::Remote { peer: peer.peer },
        // Gate 4 — nobody fits. Serve local anyway (queue at our lane); NEVER block waiting
        // for grid capacity that may never come, NEVER strand on a peer that can't serve.
        None => LaneDecision::Local { reason: LocalReason::NoPeerFits },
    }
}

/// Does this device's live free GPU fit at least ONE spike of the request after the margin?
/// One spike is the unit of "can serve at all" — the same never-below-1 floor the fit policy
/// uses (a resident model must be able to run one prefill).
fn fits_one(cap: &DeviceCapacity, req: &LeaseRequest, margin: u64) -> bool {
    lanes_that_fit(cap.gpu_free_bytes_live, margin, req.spike_bytes) >= 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capacity::gossip::CapacityOffer;

    const GB: u64 = 1024 * 1024 * 1024;

    fn dev(free_gb: u64) -> DeviceCapacity {
        DeviceCapacity {
            gpu_total_bytes: 32 * GB,
            gpu_free_bytes_live: free_gb * GB,
            system_ram_free_bytes: 16 * GB,
        }
    }
    fn peer(id: u128, free_gb: u64, reachable: bool) -> super::super::grid::PeerCapacity {
        super::super::grid::PeerCapacity {
            peer: PeerId::from_u128(id),
            capacity: dev(free_gb),
            reachable,
        }
    }
    fn req() -> LeaseRequest {
        LeaseRequest { consumer: "serving".into(), want_concurrency: 1, spike_bytes: 2 * GB }
    }
    // (constructor kept honest: CapacityOffer→capacity is the same shape the ledger folds)
    fn _offer_shape() -> DeviceCapacity {
        CapacityOffer { gpu_total_bytes: 32 * GB, gpu_free_bytes_live: 7 * GB, system_ram_free_bytes: 16 * GB, at_ms: 0 }.capacity()
    }

    // what this catches: THE COMPUTE-LEASE BOUNDARY. A request needing local state (a coding
    // act with the persona's files, local vision) must NEVER be routed to a peer, even when
    // local is exhausted and a huge peer is sitting idle — a remote peer serves TEXT only,
    // and remoting local-state work would silently drop the files/tools it depends on. If this
    // ever routes LocalOnly work remote, the boundary is broken and personas lose their hands.
    #[test]
    fn local_only_work_never_leases_even_with_local_exhausted_and_peers_idle() {
        let snap = GridSnapshot { local: dev(0), peers: vec![peer(1, 30, true)] };
        assert_eq!(
            decide_lane(&snap, &req(), Leasability::LocalOnly, GB),
            LaneDecision::Local { reason: LocalReason::NotLeasable },
            "local-state work stays local no matter how starved we are or how idle the grid is"
        );
    }

    // what this catches: local-first. When our own GPU can serve, we do NOT pay a network hop
    // even if a peer has more free memory — leasing is only worth it when local can't. A
    // regression that greedily offloaded to the biggest node would add RTT to every turn on a
    // perfectly capable machine (the single-machine-first invariant, violated).
    #[test]
    fn text_work_prefers_local_when_local_fits() {
        let snap = GridSnapshot { local: dev(13), peers: vec![peer(1, 30, true)] };
        assert_eq!(
            decide_lane(&snap, &req(), Leasability::TextOnly, GB),
            LaneDecision::Local { reason: LocalReason::LocalFits },
        );
    }

    // what this catches: THE LEASE — the payoff. Text work + local exhausted (a game ate the
    // GPU) + a reachable peer that fits → route to that peer. This is the M2-Air-gets-a-5090
    // moment: the small/starved node borrows a big node's lane for a pure-text turn. Most-free
    // wins so the demand lands on the roomiest peer.
    #[test]
    fn text_work_leases_the_most_free_reachable_peer_when_local_is_exhausted() {
        let snap = GridSnapshot {
            local: dev(1), // a game took the GPU — can't fit a 2GB spike
            peers: vec![peer(1, 7, true), peer(2, 30, true), peer(3, 5, true)],
        };
        assert_eq!(
            decide_lane(&snap, &req(), Leasability::TextOnly, GB),
            LaneDecision::Remote { peer: PeerId::from_u128(2) },
            "the roomiest reachable peer takes the leased turn",
        );
    }

    // what this catches: NEVER STRAND, NEVER BLOCK. Local exhausted, and every peer is either
    // unreachable (partition) or too small — serve local anyway (queue at our own lane). The
    // partition/no-fit case must degrade to local, never route to a peer that can't serve
    // (stranded) and never block waiting for capacity that isn't there.
    #[test]
    fn no_reachable_peer_fits_falls_back_to_local_never_strands() {
        // Unreachable big peer (partition) + reachable tiny peer (can't fit the 2GB spike).
        let snap = GridSnapshot {
            local: dev(1),
            peers: vec![peer(1, 30, false), peer(2, 2, true)],
        };
        assert_eq!(
            decide_lane(&snap, &req(), Leasability::TextOnly, GB),
            LaneDecision::Local { reason: LocalReason::NoPeerFits },
            "no reachable peer fits → serve local (queue), never strand on a dead/too-small peer",
        );
    }

    // what this catches: a lone node (no peers at all — the one-node grid, or full partition)
    // with local exhausted still decides Local, not a panic or a bogus remote. The gossip
    // loopback proves this shape live: before any peer joins, the ledger has only self.
    #[test]
    fn lone_node_with_no_peers_decides_local() {
        let snap = GridSnapshot { local: dev(0), peers: vec![] };
        assert_eq!(
            decide_lane(&snap, &req(), Leasability::TextOnly, GB),
            LaneDecision::Local { reason: LocalReason::NoPeerFits },
        );
    }
}
