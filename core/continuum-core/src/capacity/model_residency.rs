//! Model residency across the grid — WHICH peer currently holds WHICH model resident. The
//! ELIGIBILITY half of grid-overflow routing; the CONCURRENCY half is [`super::grid`].
//!
//! ## Why this is a separate abstraction (orthogonal, never absorbed)
//!
//! `serving_plan` reasons about MODEL RESIDENCY — can a node hold model M's weights + per-lane
//! KV at the served window. `capacity/grid` reasons about CONCURRENCY SPIKES — does a node have
//! a free lane RIGHT NOW ([`super::LeaseRequest`]). Settled with BigMama 2026-07-27: these are
//! **orthogonal and compose** — neither should absorb the other, the mapping is the only
//! crossing point ([`super::serving_plan`]'s `grid_lease_request` is that one bridge). So
//! residency does NOT belong on [`super::grid::PeerCapacity`] (that would blur the concurrency
//! abstraction with a residency fact); it lives here, and the governor **composes** the two at
//! the placement filter.
//!
//! ## Why residency gates grid overflow
//!
//! A grid-overflow hop routes a persona's generation to a peer. If that peer already holds M
//! resident, the hop is fast — it needs only a free lane (the concurrency check). If it merely
//! has free VRAM but NOT M, accepting the hop forces a cold full-weights load (seconds to
//! minutes for a large model) — which defeats the entire point of overflowing for speed. So the
//! fast overflow path is eligible only for peers that already hold M. A peer with unknown
//! residency (never beaconed) is NOT eligible for the fast path — conservative by construction,
//! same spirit as [`super::residency_detect`] never claiming a promotion is faster than it is.
//!
//! ## The compose point
//!
//! [`ModelResidencyView::residency_eligible`] takes a live [`GridSnapshot`] and returns a
//! SMALLER snapshot — local device untouched (the overflowing node holds M by definition; it's
//! the one serving it), peers filtered to those holding M. The unchanged capacity placement
//! policy ([`super::grid::LocalFirstFitPolicy`]) then runs on that smaller snapshot: it never
//! learns about residency, it just sees fewer peers. Reachability stays the policy's job — an
//! unreachable-but-resident peer survives this filter and is dropped downstream by `place()`,
//! keeping the two concerns cleanly separate.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::grid::GridSnapshot;
use crate::identity::PeerId;

/// Per-peer set of model ids that peer currently holds resident, folded from residency beacons.
///
/// Keyed on the peer's `Uuid` (via [`PeerId::as_uuid`]) — the same choice
/// [`super::gossip::GridCapacityLedger`] makes for capacity offers, so residency and capacity
/// index the grid identically. A peer absent from the map has UNKNOWN residency (never
/// beaconed), which [`Self::holds`] reports as `false`: not eligible for the fast overflow path.
#[derive(Debug, Clone, Default)]
pub struct ModelResidencyView {
    by_peer: HashMap<Uuid, HashSet<String>>,
}

impl ModelResidencyView {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record (latest-wins) the full set of models a peer currently holds resident. Latest-wins
    /// because residency is a live fact — a model paged out is no longer held, so a fresh beacon
    /// REPLACES the peer's set rather than merging (a merge would resurrect evicted models).
    pub fn set_resident<I, S>(&mut self, peer: PeerId, models: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.by_peer
            .insert(peer.as_uuid(), models.into_iter().map(Into::into).collect());
    }

    /// Does this peer currently hold `model_id` resident? `false` for a peer that never beaconed
    /// (unknown residency) — the conservative default that keeps the fast overflow path honest.
    pub fn holds(&self, peer: &PeerId, model_id: &str) -> bool {
        self.by_peer
            .get(&peer.as_uuid())
            .is_some_and(|set| set.contains(model_id))
    }

    /// Number of peers with a known residency beacon — probe surface, mirrors
    /// [`super::gossip::GridCapacityLedger::heard_count`].
    pub fn known_peers(&self) -> usize {
        self.by_peer.len()
    }

    /// Compose this residency view with a live capacity snapshot for `model_id`: keep the local
    /// device (the overflowing node holds M by definition) and keep only peers that hold M
    /// resident. The returned snapshot feeds the UNCHANGED capacity placement policy — concurrency
    /// logic never learns about residency, it just sees a shorter peer list. Reachability is NOT
    /// applied here (that stays the policy's job downstream), so an unreachable-but-resident peer
    /// survives this filter and is reclaimed by `place()`. Orthogonal, composed at exactly one
    /// point, neither abstraction absorbing the other.
    pub fn residency_eligible(&self, snapshot: &GridSnapshot, model_id: &str) -> GridSnapshot {
        let mut eligible = snapshot.clone();
        eligible.peers.retain(|peer| self.holds(&peer.peer, model_id));
        eligible
    }
}

/// One node's residency beacon — the wire payload advertising which models it currently holds
/// resident. Rides its OWN `grid_residency` `EphemeralCoalesced` envelope, separate from
/// [`super::gossip::CapacityOffer`]'s `grid_capacity`: residency changes on model page-in/out
/// (minute-scale), not the 10s capacity beat, so coupling them would either over-publish
/// residency or under-refresh capacity. Names + timestamp only; peer identity is the WIRE's
/// (the transcript event's authenticated peer id), never the payload's — same rule as
/// `CapacityOffer`, so a peer cannot beacon residency on another's behalf.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResidencyBeacon {
    /// Model ids this node holds resident (warm) RIGHT NOW — its base plus any co-resident.
    pub resident_models: Vec<String>,
    /// Sender clock when the reading was taken (ms since epoch). Displayed, not trusted:
    /// freshness is judged by RECEIVER clock at hear-time, exactly like `CapacityOffer.at_ms`.
    pub at_ms: u64,
}

/// A heard beacon + the receiver-clock instant it arrived (the freshness anchor).
#[derive(Debug, Clone)]
struct HeardBeacon {
    models: Vec<String>,
    heard_at_ms: u64,
}

/// Beacons silent past this drop from the projected view entirely. GENEROUS versus capacity's
/// eviction because the two abstractions stay orthogonal: residency is STICKY (a model stays
/// resident across many capacity beats), and the COMPOSED capacity snapshot already gates
/// reachability — so a residency reading never has to prove liveness itself (that would blur
/// residency into concurrency). 6× the capacity eviction window: a peer whose residency we
/// haven't reheard in that long falls back to UNKNOWN (not asserted-resident), keeping the fast
/// overflow path honest rather than routing to a model a long-silent peer may have paged out.
pub const RESIDENCY_EVICTION_WINDOW_MS: u64 = 6 * super::gossip::EVICTION_WINDOW_MS;

/// Process-global ledger of heard residency beacons, keyed by the WIRE's peer id — the
/// residency sibling of [`super::gossip::GridCapacityLedger`], same identity + freshness
/// discipline, different (slower) cadence and payload.
#[derive(Default)]
pub struct ResidencyLedger {
    heard: DashMap<Uuid, HeardBeacon>,
}

/// The one process-global residency ledger — the resource it mirrors (this node's view of who
/// holds what across the grid) is process-global, same granularity argument as
/// [`super::gossip::global_ledger`].
pub fn global_residency_ledger() -> &'static ResidencyLedger {
    static LEDGER: OnceLock<ResidencyLedger> = OnceLock::new();
    LEDGER.get_or_init(ResidencyLedger::default)
}

impl ResidencyLedger {
    /// Fold one heard beacon in (latest per peer wins — residency is a live fact). `from_peer`
    /// is the transcript event's transport identity, never payload-declared. Returns `true` when
    /// this peer is NEW to the ledger — the probe-on-join surface; steady re-beacons stay silent.
    pub fn hear(&self, from_peer: Uuid, beacon: ResidencyBeacon, heard_at_ms: u64) -> bool {
        self.heard
            .insert(
                from_peer,
                HeardBeacon { models: beacon.resident_models, heard_at_ms },
            )
            .is_none()
    }

    /// Project the ledger into a [`ModelResidencyView`] the governor composes with the capacity
    /// snapshot, evicting beacons silent past [`RESIDENCY_EVICTION_WINDOW_MS`] as it goes.
    /// `own_peer` is excluded — the local node's residency is its OWN serving truth (it holds M
    /// by definition when it overflows), not a round-tripped beacon.
    pub fn view(&self, own_peer: Uuid, now_ms: u64) -> ModelResidencyView {
        let mut view = ModelResidencyView::new();
        self.heard.retain(|peer, heard| {
            let age = now_ms.saturating_sub(heard.heard_at_ms);
            if age > RESIDENCY_EVICTION_WINDOW_MS {
                return false; // silent too long — residency falls back to unknown
            }
            if *peer != own_peer {
                view.by_peer
                    .insert(*peer, heard.models.iter().cloned().collect());
            }
            true
        });
        view
    }

    /// Number of peers currently on the ledger (self included if echoed) — probe surface,
    /// mirrors [`super::gossip::GridCapacityLedger::heard_count`].
    pub fn heard_count(&self) -> usize {
        self.heard.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capacity::grid::{GridPlacementPolicy, GridSnapshot, LocalFirstFitPolicy, PeerCapacity};
    use crate::capacity::{DeviceCapacity, LeaseRequest};

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

    // what this catches: the residency ELIGIBILITY gate. Only peers that beaconed the model as
    // resident survive the filter; a peer with plenty of free VRAM but NOT holding M is dropped
    // (routing to it would force a cold full-weights load — the slow path the gate exists to
    // avoid), and a peer that never beaconed at all (unknown residency) is dropped too. The local
    // device is always kept — the overflowing node holds M by definition.
    #[test]
    fn only_peers_holding_the_model_are_eligible() {
        let snap = GridSnapshot {
            local: dev(2),
            peers: vec![
                peer(1, 40, true), // holds qwen-coder
                peer(2, 40, true), // holds something else, NOT qwen-coder
                peer(3, 40, true), // never beaconed — unknown residency
            ],
        };
        let mut view = ModelResidencyView::new();
        view.set_resident(peer_id(1), ["qwen-coder", "embed-small"]);
        view.set_resident(peer_id(2), ["llama-70b"]);
        // peer 3 intentionally not recorded.

        let eligible = view.residency_eligible(&snap, "qwen-coder");
        assert_eq!(eligible.local, snap.local, "local device is always kept — it holds M");
        assert_eq!(eligible.peers.len(), 1, "only the peer holding qwen-coder survives");
        assert_eq!(eligible.peers[0].peer.as_uuid(), Uuid::from_u128(1));
    }

    // what this catches: latest-wins REPLACE, not merge. A peer that paged qwen-coder OUT (its
    // fresh beacon lists only what it still holds) must stop being eligible — a merge would
    // resurrect the evicted model and route a hop to a peer that no longer has it.
    #[test]
    fn fresh_beacon_replaces_so_evicted_models_stop_being_eligible() {
        let mut view = ModelResidencyView::new();
        view.set_resident(peer_id(1), ["qwen-coder", "llama-70b"]);
        assert!(view.holds(&peer_id(1), "qwen-coder"));

        // Peer paged qwen-coder out; next beacon lists only llama-70b.
        view.set_resident(peer_id(1), ["llama-70b"]);
        assert!(!view.holds(&peer_id(1), "qwen-coder"), "evicted model must not linger");
        assert!(view.holds(&peer_id(1), "llama-70b"));
    }

    // what this catches: the COMPOSE contract end-to-end — residency filters WHO is eligible,
    // then the unchanged capacity policy places lanes on the survivors and applies reachability
    // itself. A resident+reachable peer gets lanes; a resident+UNREACHABLE peer survives the
    // residency filter (reachability isn't residency's job) but is reclaimed by place(); a
    // non-resident peer is absent from placement entirely. Two orthogonal gates, composed.
    #[test]
    fn residency_then_capacity_policy_compose() {
        let snap = GridSnapshot {
            local: dev(1), // no local room — force the spill onto peers
            peers: vec![
                peer(1, 40, true),  // resident + reachable → should get lanes
                peer(2, 40, false), // resident + UNREACHABLE → reclaimed by place()
                peer(3, 40, true),  // reachable but NOT resident → filtered out before place()
            ],
        };
        let mut view = ModelResidencyView::new();
        view.set_resident(peer_id(1), ["qwen-coder"]);
        view.set_resident(peer_id(2), ["qwen-coder"]);
        // peer 3 holds nothing relevant.

        let eligible = view.residency_eligible(&snap, "qwen-coder");
        assert_eq!(eligible.peers.len(), 2, "peers 1 & 2 are resident; peer 3 filtered out");

        // Small spike so many lanes fit per peer — we're testing WHO gets lanes, not how many.
        let req = LeaseRequest {
            consumer: "qwen-coder".into(),
            want_concurrency: 4,
            spike_bytes: GB,
        };
        let placement = LocalFirstFitPolicy { safety_margin_bytes: GB }.place(&eligible, &req);

        // The reachable resident peer carries the remote lanes; the unreachable one gets none.
        let peer1_lanes: u32 = placement
            .remote
            .iter()
            .filter(|(p, _)| p.as_uuid() == Uuid::from_u128(1))
            .map(|(_, n)| *n)
            .sum();
        let peer2_lanes: u32 = placement
            .remote
            .iter()
            .filter(|(p, _)| p.as_uuid() == Uuid::from_u128(2))
            .map(|(_, n)| *n)
            .sum();
        let peer3_present = placement.remote.iter().any(|(p, _)| p.as_uuid() == Uuid::from_u128(3));

        assert!(peer1_lanes > 0, "resident + reachable peer must carry overflow lanes");
        assert_eq!(peer2_lanes, 0, "resident but unreachable peer is reclaimed by place()");
        assert!(!peer3_present, "non-resident peer never reaches placement");
    }

    fn beacon(models: &[&str], at_ms: u64) -> ResidencyBeacon {
        ResidencyBeacon {
            resident_models: models.iter().map(|s| s.to_string()).collect(),
            at_ms,
        }
    }

    // what this catches: the beacon RECEIVE→PROJECT pipeline — a heard beacon projects into a
    // ModelResidencyView that holds() reflects, and the node's OWN echoed beacon is excluded
    // (the local node's residency is its own serving truth, arriving live, not a round-trip).
    // This is the residency sibling of gossip's loopback contract.
    #[test]
    fn heard_beacon_projects_and_own_echo_is_excluded() {
        let ledger = ResidencyLedger::default();
        let me = Uuid::from_u128(1);
        let other = Uuid::from_u128(2);
        ledger.hear(me, beacon(&["qwen-coder"], 1_000), 1_000); // our own beacon, round-tripped
        ledger.hear(other, beacon(&["qwen-coder", "llama-70b"], 1_000), 1_000);

        let view = ledger.view(me, 2_000);
        assert!(!view.holds(&peer_id(1), "qwen-coder"), "own echo excluded from the peer view");
        assert!(view.holds(&peer_id(2), "qwen-coder"), "the real peer's residency projects");
        assert!(view.holds(&peer_id(2), "llama-70b"));
        assert_eq!(view.known_peers(), 1, "only the genuine peer, not the echo");
    }

    // what this catches: eviction after long silence — a peer we haven't reheard past the
    // (generous) residency window falls back to UNKNOWN residency, so the fast overflow path
    // won't route to a model that long-silent peer may since have paged out. A fresh beacon
    // brings it right back (grow is first-class).
    #[test]
    fn stale_beacon_evicts_then_a_fresh_one_restores() {
        let ledger = ResidencyLedger::default();
        let me = Uuid::from_u128(1);
        let peer = Uuid::from_u128(2);
        ledger.hear(peer, beacon(&["qwen-coder"], 0), 0);
        assert!(ledger.view(me, 1_000).holds(&peer_id(2), "qwen-coder"));

        // Silent past the residency eviction window: gone from the view (unknown, not asserted).
        let t_evict = RESIDENCY_EVICTION_WINDOW_MS + 1;
        assert!(
            !ledger.view(me, t_evict).holds(&peer_id(2), "qwen-coder"),
            "long-silent peer's residency falls back to unknown"
        );

        // It beacons again: instantly back.
        ledger.hear(peer, beacon(&["qwen-coder"], t_evict), t_evict);
        assert!(
            ledger.view(me, t_evict + 1).holds(&peer_id(2), "qwen-coder"),
            "a returning peer's residency is adopted on its first fresh beacon"
        );
    }

    // what this catches: the wire payload survives serde round-trip byte-for-byte (camelCase
    // like every realtime inline payload). If resident_models renamed or at_ms narrowed, heard
    // residency would silently drift from published — the grid would gate overflow on models
    // nobody actually beaconed.
    #[test]
    fn beacon_round_trips_through_json() {
        let b = beacon(&["qwen-coder", "embed-small"], 42);
        let json = serde_json::to_string(&b).expect("serialize");
        assert!(json.contains("residentModels"), "camelCase wire field: {json}");
        let back: ResidencyBeacon = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, b, "beacon must survive the wire byte-for-byte");
    }
}
