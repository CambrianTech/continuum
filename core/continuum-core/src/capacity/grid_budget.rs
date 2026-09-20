//! `grid_budget` — what the best single reachable node can serve, right now.
//!
//! The CONSUME half of the grid loop. The contribute half already runs:
//! [`crate::modules::grid_capacity::GridCapacityModule`] publishes this node's
//! `CapacityOffer` every tick, and `airc::inbound_attach` folds every heard offer
//! into [`crate::capacity::gossip::global_ledger`]. This module reads that
//! ledger's snapshot and answers one question: **what is the largest serving
//! budget available to me on the grid this instant?**
//!
//! See `docs/architecture/GRID-ELASTIC-CAPABILITY.md`. Three constraints shape
//! every line here, and all three are Joel's:
//!
//! ## 1. NEVER A POOL — `max`, never `sum`
//!
//! `provisioning/placement_planner.rs` asserts it directly: *two 20GiB peers must
//! NOT fit a 40GiB model*. Summing memory across machines to host one bigger
//! model is the exo approach, and it trades a working mind for a slow one. So the
//! grid budget is the **best single node's** usable bytes. A grid of ten laptops
//! is still a laptop-sized budget; it is ten laptops' worth of *citizens*, which
//! is a different lever (see the doc's population axis).
//!
//! ## 2. BOTH ENDS — solo is a grid of one, and there is NO branch
//!
//! Every node runs the local end (fit to my own capacity — the end that never
//! blocks, including in total partition) and the grid end (consume peers' spare)
//! through the *same code*. There is deliberately no `if peers.is_empty()` fast
//! path: `max` over `{local}` is `local`, which is exactly today's behavior. The
//! tempting optimization would reintroduce two paths that drift, and the
//! solo-node test below exists to catch anyone adding it back.
//!
//! ## 3. SYMMETRIC ROLE — no node types, no central authority
//!
//! Every node computes this for itself from its own view. There is no
//! coordinator, no placer, and no "provider node" type — a machine that only ever
//! provides is simply a node whose local demand is currently zero and which could
//! consume at any instant. This function is therefore pure and local: it reads a
//! snapshot and returns a number. It never decides anything *for* another node.

use crate::capacity::grid::GridSnapshot;
use crate::capacity::DeviceCapacity;

/// Which node a budget came from — carried so the decision is legible rather than
/// an unexplained number.
///
/// "The budget went up" with no attribution is the shape of bug this codebase
/// keeps relearning; a consumer that can name the source can also notice when the
/// source is itself wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetSource {
    /// This machine. Also the answer when there are no reachable peers at all —
    /// solo is a grid of one, not a special case.
    Local,
    /// A reachable peer, identified for the record.
    Peer(String),
}

/// The best serving budget reachable this instant, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridBudget {
    /// Free serving bytes on the winning node. NEVER a sum across nodes.
    pub usable_bytes: u64,
    /// The winning node's PHYSICAL ceiling.
    ///
    /// Carried because the consumer clamps free against total
    /// (`host_budget_from` does `available.min(total_vram)`), and that clamp must
    /// use the winner's device — not this machine's. Passing a 32GB peer's free
    /// bytes while still clamping to a local 8GB card would silently discard the
    /// entire point of the projection, and it would look like the feature simply
    /// did nothing.
    pub total_bytes: u64,
    pub source: BudgetSource,
    /// How many nodes were considered, local included. `1` means solo.
    ///
    /// This is the population signal — the count lever, kept separate from the
    /// byte lever so nobody is tempted to conflate "more nodes" with "more bytes".
    pub reachable_nodes: u32,
}

impl GridBudget {
    /// Is the winning budget somewhere other than this machine?
    pub fn is_remote(&self) -> bool {
        matches!(self.source, BudgetSource::Peer(_))
    }
}

/// What a single device can actually put toward serving.
///
/// Free bytes, not total: a node with a 32GB card already hosting something has
/// less to offer than its spec sheet says, and offering the spec sheet is the
/// fabricated-capacity lie the per-node honesty rules exist to prevent.
fn servable_bytes(cap: &DeviceCapacity) -> u64 {
    // Clamp PER NODE, mirroring `host_budget_from`'s `available.min(total_vram)`.
    // It must happen here, before the max, because clamping afterwards would
    // measure a remote node against the LOCAL card's ceiling.
    cap.gpu_free_bytes_live.min(cap.gpu_total_bytes)
}

/// The best single reachable node's serving budget.
///
/// Pure: a snapshot in, a number out. No I/O, no locks, no decisions on behalf of
/// any other node — see the module docs on symmetric role.
///
/// Unreachable peers are excluded rather than counted at zero. An unreachable
/// node is a memory, not an offer; counting it as a zero-byte participant would
/// quietly drag the population signal while contributing nothing.
pub fn grid_budget(snapshot: &GridSnapshot) -> GridBudget {
    let local_bytes = servable_bytes(&snapshot.local);

    // Start from local and let peers WIN, never ACCUMULATE. The fold is a max,
    // and that is the entire never-a-pool discipline in one operator.
    let mut best = GridBudget {
        usable_bytes: local_bytes,
        total_bytes: snapshot.local.gpu_total_bytes,
        source: BudgetSource::Local,
        reachable_nodes: 1, // this node always counts; it is never unreachable to itself
    };

    for peer in snapshot.peers.iter().filter(|p| p.reachable) {
        best.reachable_nodes = best.reachable_nodes.saturating_add(1);
        let bytes = servable_bytes(&peer.capacity);
        // Strictly greater: ties keep LOCAL. Preferring home on a tie avoids
        // pointlessly nominating a remote node for a budget we already have here,
        // and it keeps the solo-equivalence property exact.
        if bytes > best.usable_bytes {
            best.usable_bytes = bytes;
            best.total_bytes = peer.capacity.gpu_total_bytes;
            best.source = BudgetSource::Peer(peer.peer.to_string());
        }
    }

    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capacity::grid::PeerCapacity;

    fn dev(free_gb: u64) -> DeviceCapacity {
        DeviceCapacity {
            gpu_total_bytes: 64 * 1024 * 1024 * 1024,
            gpu_free_bytes_live: free_gb * 1024 * 1024 * 1024,
            system_ram_free_bytes: 16 * 1024 * 1024 * 1024,
        }
    }

    fn peer(id: u128, free_gb: u64, reachable: bool) -> PeerCapacity {
        PeerCapacity {
            peer: airc_core::PeerId(uuid::Uuid::from_u128(id)),
            capacity: dev(free_gb),
            reachable,
        }
    }

    fn gb(n: u64) -> u64 {
        n * 1024 * 1024 * 1024
    }

    /// what this catches: an accidental grid-mode/solo-mode branch. A node with no
    /// peers must plan EXACTLY as it does today — that is the whole "both ends, no
    /// branch" rule (doc §3b). If someone adds `if peers.is_empty() { ... }`, the
    /// two paths will drift and this is the test that notices.
    #[test]
    fn solo_is_a_grid_of_one_and_equals_the_local_budget() {
        let snap = GridSnapshot {
            local: dev(20),
            peers: vec![],
        };
        let b = grid_budget(&snap);
        assert_eq!(b.usable_bytes, gb(20), "solo budget IS the local budget");
        assert_eq!(b.source, BudgetSource::Local);
        assert_eq!(b.reachable_nodes, 1, "the node always counts itself");
        assert!(!b.is_remote());
    }

    /// what this catches: THE never-a-pool violation. Two 20GiB peers must not add
    /// up to a 40GiB budget. If this ever fails we have become exo — sharding a
    /// model across machines to make it "fit" — which trades a working mind for a
    /// slow one.
    #[test]
    fn two_peers_never_pool_into_a_bigger_budget() {
        let snap = GridSnapshot {
            local: dev(8),
            peers: vec![peer(1, 20, true), peer(2, 20, true)],
        };
        let b = grid_budget(&snap);
        assert_eq!(
            b.usable_bytes,
            gb(20),
            "max, never sum — 20 not 40, and not 48"
        );
        assert_eq!(b.reachable_nodes, 3, "all three counted for POPULATION");
    }

    /// what this catches: the actual point of the feature. A capable peer joining
    /// must raise what this node can plan for.
    #[test]
    fn a_bigger_reachable_peer_raises_the_budget_and_is_named() {
        let snap = GridSnapshot {
            local: dev(8),
            peers: vec![peer(7, 32, true)],
        };
        let b = grid_budget(&snap);
        assert_eq!(b.usable_bytes, gb(32));
        assert!(b.is_remote(), "the win came from a peer and says so");
        assert_eq!(
            b.source,
            BudgetSource::Peer(airc_core::PeerId(uuid::Uuid::from_u128(7)).to_string())
        );
    }

    /// what this catches: an unreachable node treated as an offer. A peer we
    /// cannot reach is a memory, not capacity — counting it would promise a budget
    /// that cannot be spent.
    #[test]
    fn unreachable_peers_are_not_offers() {
        let snap = GridSnapshot {
            local: dev(8),
            peers: vec![peer(1, 64, false)],
        };
        let b = grid_budget(&snap);
        assert_eq!(
            b.usable_bytes,
            gb(8),
            "the 64GB peer is unreachable — not an offer"
        );
        assert_eq!(b.source, BudgetSource::Local);
        assert_eq!(
            b.reachable_nodes, 1,
            "and it must not inflate the population count either"
        );
    }

    /// what this catches: total partition, which is doc §3b test 2. When every
    /// peer drops, the node must fall back to EXACTLY its solo budget — the local
    /// end never blocks.
    #[test]
    fn total_partition_falls_back_to_exactly_the_solo_budget() {
        let local = dev(12);
        let solo = grid_budget(&GridSnapshot {
            local: local.clone(),
            peers: vec![],
        });
        let partitioned = grid_budget(&GridSnapshot {
            local,
            peers: vec![peer(1, 64, false), peer(2, 48, false)],
        });
        assert_eq!(
            partitioned, solo,
            "a partitioned node and a solo node must be indistinguishable"
        );
    }

    /// what this catches: a tie nominating a remote node for no reason. Equal
    /// budgets should stay home — it keeps solo-equivalence exact and avoids
    /// advertising a remote source we gain nothing from.
    #[test]
    fn ties_keep_the_budget_local() {
        let snap = GridSnapshot {
            local: dev(16),
            peers: vec![peer(1, 16, true)],
        };
        assert_eq!(grid_budget(&snap).source, BudgetSource::Local);
    }

    /// what this catches: reading the spec sheet instead of the live reading. A
    /// node with a big card that is already full has little to offer, and offering
    /// its TOTAL would be the fabricated-capacity lie per-node honesty prevents.
    #[test]
    fn a_full_big_card_offers_its_free_bytes_not_its_total() {
        let mut busy = dev(1);
        busy.gpu_total_bytes = gb(80);
        let snap = GridSnapshot {
            local: dev(10),
            peers: vec![PeerCapacity {
                peer: airc_core::PeerId(uuid::Uuid::from_u128(9)),
                capacity: busy,
                reachable: true,
            }],
        };
        let b = grid_budget(&snap);
        assert_eq!(
            b.usable_bytes,
            gb(10),
            "1GB free beats nothing, not 80GB total"
        );
        assert_eq!(b.source, BudgetSource::Local);
        assert_eq!(
            b.total_bytes,
            gb(64),
            "the LOCAL ceiling travels with a local win"
        );
    }

    /// what this catches: the clamp being applied against the wrong device. The
    /// consumer does `available.min(total_vram)`; if a peer wins, that ceiling has
    /// to be the PEER's card. Clamping a 40GB peer to an 8GB local card would
    /// silently discard the whole projection and look like the feature did
    /// nothing at all.
    #[test]
    fn a_remote_win_carries_the_remote_ceiling_not_the_local_one() {
        let mut small_local = dev(4);
        small_local.gpu_total_bytes = gb(8);
        let mut big_peer = dev(40);
        big_peer.gpu_total_bytes = gb(48);
        let snap = GridSnapshot {
            local: small_local,
            peers: vec![PeerCapacity {
                peer: airc_core::PeerId(uuid::Uuid::from_u128(3)),
                capacity: big_peer,
                reachable: true,
            }],
        };
        let b = grid_budget(&snap);
        assert_eq!(b.usable_bytes, gb(40));
        assert_eq!(
            b.total_bytes,
            gb(48),
            "the winner's ceiling must travel with it, or the consumer clamps to 8GB"
        );
    }

    /// what this catches: a peer reporting free > total (a bad or hostile offer).
    /// Per-node clamping keeps one broken reading from inventing capacity.
    #[test]
    fn a_peer_claiming_more_free_than_it_has_is_clamped_to_its_own_total() {
        let mut liar = dev(0);
        liar.gpu_total_bytes = gb(4);
        liar.gpu_free_bytes_live = gb(999);
        let snap = GridSnapshot {
            local: dev(10),
            peers: vec![PeerCapacity {
                peer: airc_core::PeerId(uuid::Uuid::from_u128(4)),
                capacity: liar,
                reachable: true,
            }],
        };
        let b = grid_budget(&snap);
        assert_eq!(
            b.usable_bytes,
            gb(10),
            "999GB claim clamps to its own 4GB total"
        );
        assert_eq!(b.source, BudgetSource::Local);
    }
}
