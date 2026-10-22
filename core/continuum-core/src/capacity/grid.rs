//! Grid capacity — "the sum of all misfit parts, always."
//!
//! A grid node never owns its machine (a game eats or frees the GPU mid-session) and never owns
//! the grid (peers join, die, partition, and return mid-lease). This module extends the
//! deterministic simulator to that world BEFORE live serving touches it: a [`GridSnapshot`] is
//! the local device plus per-peer live readings + reachability, and a [`GridPlacementPolicy`]
//! spreads one demand across whatever is reachable THIS instant.
//!
//! The load-bearing arithmetic: capacity composes as a **sum of per-node fits**, never as an
//! aggregate pool. 12GB free "across the grid" cannot run a 6-lane placement when it's three
//! 4GB machines — the aggregate lies, the per-node fit ([`super::lanes_that_fit`], the same one
//! rule [`super::FitPolicy`] runs on a single device) tells the truth.
//!
//! The negative control is the **init-time trap** made executable: a policy that places once
//! and holds ([`StickyPlacementPolicy`]) keeps "granting" lanes through a peer that died
//! mid-lease — stranded lanes, silently-dropped personas — and never grows when nodes join
//! 20 minutes in. The live policy re-derives placement from every snapshot, so partition,
//! join, loss, and return all fall out of ONE rule.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::identity::PeerId;

use super::consumer::room_faculties;
use super::score::score_experience;
use super::{lanes_that_fit, DeviceCapacity, LeaseRequest, Score};

/// One peer's live reading as gossiped over airc (in prod; scripted in the sim). `reachable`
/// is the network's verdict THIS instant — an unreachable peer's last-known capacity is a
/// memory, not an offer.
#[derive(Debug, Clone)]
pub struct PeerCapacity {
    pub peer: PeerId,
    pub capacity: DeviceCapacity,
    pub reachable: bool,
}

/// The whole world one node can serve against, THIS instant: its own device + every peer it
/// has a live reading for. Losing the network is not an error state — it's a snapshot whose
/// peers are all unreachable, and the node keeps serving locally.
#[derive(Debug, Clone)]
pub struct GridSnapshot {
    pub local: DeviceCapacity,
    pub peers: Vec<PeerCapacity>,
}

/// Where the demanded lanes actually run: some local, some on named peers. Derived from the
/// live snapshot, never held across one — holding is the init-time trap.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub local_lanes: u32,
    pub remote: Vec<(PeerId, u32)>,
}

impl Placement {
    pub fn total(&self) -> u32 {
        self.local_lanes + self.remote.iter().map(|(_, n)| n).sum::<u32>()
    }
}

/// The grid-shaped optimizer seam — the [`super::AllocationPolicy`] sibling one level up.
/// Deterministic local-first bootstrap now; the learned/persona-negotiated placement
/// ([[grid-agreements-swappable-policy-deterministic-rails]], #103) lands behind the same
/// signature.
pub trait GridPlacementPolicy: Send + Sync {
    fn place(&self, grid: &GridSnapshot, req: &LeaseRequest) -> Placement;
    fn name(&self) -> &'static str;
}

/// Deterministic bootstrap: fill local first (cheapest lanes — no network hop), then spill the
/// remaining demand to REACHABLE peers, most-free-first, each capped by its OWN per-node fit.
/// Every knob re-derived per snapshot: a dead peer gets nothing (reclaim), a returning or new
/// peer gets used (grow), a total partition degrades to exactly the single-device [`super::FitPolicy`]
/// behavior — never blocks, serves what the local machine honestly fits.
pub struct LocalFirstFitPolicy {
    /// Headroom kept free on EVERY node, local and remote — same meaning as [`super::FitPolicy`].
    pub safety_margin_bytes: u64,
}

impl GridPlacementPolicy for LocalFirstFitPolicy {
    fn place(&self, grid: &GridSnapshot, req: &LeaseRequest) -> Placement {
        let want = req.want_concurrency.max(1);

        // Local first — same never-below-1 floor as FitPolicy (a resident model must be able
        // to run one prefill; that's a residency decision, not a concurrency one).
        let local_fit = lanes_that_fit(
            grid.local.gpu_free_bytes_live,
            self.safety_margin_bytes,
            req.spike_bytes,
        );
        let local_lanes = local_fit.clamp(1, want);
        let mut remaining = want - local_lanes;

        // Spill to reachable peers, most-free-first (fewest peers touched for the demand).
        // Each peer is capped by ITS OWN fit — the misfit-parts rule. Unreachable peers are
        // memories, not offers: they get nothing, which IS the mid-lease reclaim.
        let mut reachable: Vec<&PeerCapacity> = grid.peers.iter().filter(|p| p.reachable).collect();
        reachable.sort_by(|a, b| {
            b.capacity
                .gpu_free_bytes_live
                .cmp(&a.capacity.gpu_free_bytes_live)
        });

        let mut remote = Vec::new();
        for peer in reachable {
            if remaining == 0 {
                break;
            }
            let fit = lanes_that_fit(
                peer.capacity.gpu_free_bytes_live,
                self.safety_margin_bytes,
                req.spike_bytes,
            );
            let take = fit.min(remaining);
            if take > 0 {
                remote.push((peer.peer, take));
                remaining -= take;
            }
        }

        Placement {
            local_lanes,
            remote,
        }
    }
    fn name(&self) -> &'static str {
        "local-first-fit"
    }
}

/// MODEL-AWARE grid placement — the convergence of the [`super::super::resources::placement`]
/// kernel's affinity rule onto the grid policy shape (Slice A2). Where [`LocalFirstFitPolicy`]
/// fills by FREE BYTES (model-blind — it would cold-load a base onto the emptiest node even
/// when a peer already holds it WARM), this ranks nodes AFFINITY-FIRST: a node already serving
/// the demand's base beats a node with more free RAM every time, because routing there avoids a
/// cold weight-load + cross-node transfer — AND it frees the local GPU for whatever else is live
/// (a video huddle running while a benchmark rides a warm peer). Among equally-warm nodes it
/// prefers local (no network hop), then most-free. No local floor: if the base is warm on a peer
/// and cold locally, the whole demand can ride the peer, leaving the local accelerator untouched.
///
/// The warm-base map is carried on the policy (constructed per-scenario) rather than threaded
/// onto the widely-built [`GridSnapshot`]/[`LeaseRequest`] — that snapshot-threading is the
/// deliberate follow-up that lands WITH the live grid-serving consumer (the ~48-site schema
/// change buys nothing while these policies are simulator-only). Here the affinity IDEA is
/// proven in the gym against the byte-fill baseline. An empty `demand_base` (or empty `warm`)
/// degrades to locality-then-free ordering — the model-blind path, unchanged.
/// [[misfit-grid-is-a-distributed-moe]] [[seamless-persona-failover-model-and-genome]]
pub struct AffinityFitPolicy {
    /// Headroom kept free on EVERY node — same meaning as [`LocalFirstFitPolicy`].
    pub safety_margin_bytes: u64,
    /// The base the demand needs. Nodes already holding it warm are preferred. Empty =
    /// model-blind (no affinity signal → locality-then-free ordering).
    pub demand_base: String,
    /// Which bases each peer holds WARM this snapshot, keyed by peer. In the live path this
    /// moves onto `GridSnapshot`; carried here so the affinity idea is sim-validated with zero
    /// churn to the widely-constructed snapshot/request types.
    pub warm: HashMap<PeerId, Vec<String>>,
    /// Whether the LOCAL node already holds the demand's base warm.
    pub local_warm: bool,
}

impl AffinityFitPolicy {
    fn peer_is_warm(&self, peer: &PeerId) -> bool {
        !self.demand_base.is_empty()
            && self
                .warm
                .get(peer)
                .is_some_and(|bases| bases.iter().any(|b| b == &self.demand_base))
    }
}

impl GridPlacementPolicy for AffinityFitPolicy {
    fn place(&self, grid: &GridSnapshot, req: &LeaseRequest) -> Placement {
        let want = req.want_concurrency.max(1);

        // One candidate per servable node: local + every REACHABLE peer (unreachable peers are
        // memories, not offers — the same reclaim rule as local-first). `peer: None` = local.
        struct Cand {
            peer: Option<PeerId>,
            free: u64,
            warm: bool,
        }
        let mut cands: Vec<Cand> = Vec::with_capacity(1 + grid.peers.len());
        cands.push(Cand {
            peer: None,
            free: grid.local.gpu_free_bytes_live,
            warm: !self.demand_base.is_empty() && self.local_warm,
        });
        for p in grid.peers.iter().filter(|p| p.reachable) {
            cands.push(Cand {
                peer: Some(p.peer),
                free: p.capacity.gpu_free_bytes_live,
                warm: self.peer_is_warm(&p.peer),
            });
        }

        // Affinity FIRST (warm beats free — the whole point), then local before remote (no
        // network hop), then most-free. `sort_by` is STABLE, so equal keys keep insertion order
        // (local pushed first, then peers in snapshot order) — deterministic without needing an
        // Ord on PeerId.
        cands.sort_by(|a, b| {
            b.warm
                .cmp(&a.warm)
                .then_with(|| b.peer.is_none().cmp(&a.peer.is_none()))
                .then_with(|| b.free.cmp(&a.free))
        });

        // Fill the demand down the ranked order, each node capped by ITS OWN per-node fit (the
        // misfit-parts rule — the aggregate never gets a vote). No local floor: a fully-warm
        // peer can take the whole demand, leaving the local accelerator free for live work.
        let mut remaining = want;
        let mut local_lanes = 0u32;
        let mut remote: Vec<(PeerId, u32)> = Vec::new();
        for c in &cands {
            if remaining == 0 {
                break;
            }
            let take =
                lanes_that_fit(c.free, self.safety_margin_bytes, req.spike_bytes).min(remaining);
            if take == 0 {
                continue;
            }
            match c.peer {
                None => local_lanes += take,
                Some(p) => remote.push((p, take)),
            }
            remaining -= take;
        }
        Placement {
            local_lanes,
            remote,
        }
    }
    fn name(&self) -> &'static str {
        "affinity-fit"
    }
}

/// THE INIT-TIME TRAP, as a policy — the negative control every live policy must beat. Places
/// once against the first snapshot it sees, then holds that placement forever: exactly the
/// `classify_hardware → frozen policy file` shape the design doc forbids. Against a living
/// grid it fails in BOTH directions: lanes stay placed on a peer that died mid-lease
/// (stranded — silently-dropped personas), and nodes that join 20 minutes in are never used.
pub struct StickyPlacementPolicy<P: GridPlacementPolicy> {
    pub inner: P,
    cached: Mutex<Option<Placement>>,
}

impl<P: GridPlacementPolicy> StickyPlacementPolicy<P> {
    pub fn new(inner: P) -> Self {
        Self {
            inner,
            cached: Mutex::new(None),
        }
    }
}

impl<P: GridPlacementPolicy> GridPlacementPolicy for StickyPlacementPolicy<P> {
    fn place(&self, grid: &GridSnapshot, req: &LeaseRequest) -> Placement {
        let mut cached = self.cached.lock().expect("sim-only lock");
        cached
            .get_or_insert_with(|| self.inner.place(grid, req))
            .clone()
    }
    fn name(&self) -> &'static str {
        "sticky-init-time"
    }
}

/// Lanes this placement claims on nodes that cannot serve them RIGHT NOW: peers that are
/// unreachable or gone from the snapshot entirely. Every stranded lane is a persona who was
/// "granted" compute that does not exist — the silent-drop failure mode, hard-fail like OOM.
pub fn stranded_lanes(grid: &GridSnapshot, placement: &Placement) -> u32 {
    placement
        .remote
        .iter()
        .filter(|(peer, _)| !grid.peers.iter().any(|p| p.peer == *peer && p.reachable))
        .map(|(_, n)| n)
        .sum()
}

/// Per-node OOM verdicts for a placement: each node's assigned spikes must fit ITS OWN live
/// free GPU — the aggregate never gets a vote. Returns the number of overflowing nodes.
pub fn placement_oom_count(grid: &GridSnapshot, req: &LeaseRequest, placement: &Placement) -> u32 {
    let mut ooms = 0;
    if (placement.local_lanes as u64).saturating_mul(req.spike_bytes)
        > grid.local.gpu_free_bytes_live
    {
        ooms += 1;
    }
    for (peer, lanes) in &placement.remote {
        if let Some(p) = grid.peers.iter().find(|p| p.peer == *peer && p.reachable) {
            if (*lanes as u64).saturating_mul(req.spike_bytes) > p.capacity.gpu_free_bytes_live {
                ooms += 1;
            }
        }
        // Unreachable/gone peers are counted by `stranded_lanes`, not here.
    }
    ooms
}

/// Which node a verdict speaks about.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodePick {
    Local,
    Peer(PeerId),
}

/// The glass box's per-node fact at one tick: what the node offered, what the placement put on
/// it, and whether that was honest. Derived by the simulator from snapshot + placement — never
/// from the policy's internals — so EVERY policy (deterministic today, learned/persona later,
/// which can't explain themselves) gets the same audit for free. The trace is the explanation.
#[derive(Debug, Clone)]
pub struct NodeVerdict {
    pub node: NodePick,
    pub reachable: bool,
    pub free_bytes: u64,
    pub assigned_lanes: u32,
    /// Lanes assigned beyond what this node's live free GPU holds — the per-node OOM.
    pub overflowed: bool,
    /// Lanes assigned to a node that cannot serve them (unreachable/gone).
    pub stranded_lanes: u32,
}

/// One placement decision, glass-boxed: when, by whom, what it did to every node, and what the
/// world thought of it. This is the observability-as-substrate seam (CaptureSink pattern) for
/// the allocator: Noop by default at zero cost, recorded when we want to SEE what it did — in
/// the sim now, on the live governor's watch loop later (same type, probe!/jsonl sink).
#[derive(Debug, Clone)]
pub struct PlacementDecision {
    pub t_ms: u64,
    pub policy: &'static str,
    pub verdicts: Vec<NodeVerdict>,
    pub placement: Placement,
    pub want: u32,
    pub experience: f32,
}

impl PlacementDecision {
    /// One human-readable line per decision — the "what is it doing RIGHT NOW" view.
    pub fn render(&self) -> String {
        // Last 8 hex chars: as distinguishing as the first 8 for random v4 ids, and still
        // distinct for `from_u128` test ids (which zero-pad the FRONT).
        let short = |p: &PeerId| {
            let s = p.to_string();
            s.chars()
                .skip(s.len().saturating_sub(8))
                .collect::<String>()
        };
        let mut placed: Vec<String> = Vec::new();
        if self.placement.local_lanes > 0 {
            placed.push(format!("local:{}", self.placement.local_lanes));
        }
        for (peer, n) in &self.placement.remote {
            placed.push(format!("{}:{}", short(peer), n));
        }
        let mut trouble: Vec<String> = Vec::new();
        for v in &self.verdicts {
            let name = match &v.node {
                NodePick::Local => "local".to_string(),
                NodePick::Peer(p) => short(p),
            };
            if v.stranded_lanes > 0 {
                trouble.push(format!(
                    "STRANDED {} lanes on {} (unreachable)",
                    v.stranded_lanes, name
                ));
            } else if v.overflowed {
                trouble.push(format!(
                    "OOM on {} ({} lanes > {}GB free)",
                    name,
                    v.assigned_lanes,
                    v.free_bytes / GB
                ));
            } else if !v.reachable && v.assigned_lanes == 0 {
                trouble.push(format!("{} unreachable, skipped", name));
            }
        }
        format!(
            "t={}s [{}] {}/{} lanes → [{}] exp={:.2}{}",
            self.t_ms / 1000,
            self.policy,
            self.placement.total(),
            self.want,
            placed.join(" "),
            self.experience,
            if trouble.is_empty() {
                String::new()
            } else {
                format!(" | {}", trouble.join("; "))
            }
        )
    }
}

/// The capture seam. [`NoopGridCapture`] is the zero-cost default; [`RecordingGridCapture`]
/// keeps the full trace for tests, the gym's replay, and the (coming) `capacity/simulate`
/// command that lets anyone watch a policy think.
pub trait GridCaptureSink {
    fn capture(&mut self, decision: PlacementDecision);
}

/// Default: capture nothing, cost nothing.
pub struct NoopGridCapture;
impl GridCaptureSink for NoopGridCapture {
    fn capture(&mut self, _decision: PlacementDecision) {}
}

/// The glass box: every decision kept, renderable as a timeline a human (or the apex judge)
/// reads top to bottom.
#[derive(Default)]
pub struct RecordingGridCapture {
    pub decisions: Vec<PlacementDecision>,
}
impl GridCaptureSink for RecordingGridCapture {
    fn capture(&mut self, decision: PlacementDecision) {
        self.decisions.push(decision);
    }
}
impl RecordingGridCapture {
    pub fn render(&self) -> String {
        self.decisions
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// One tick of the grid world: the full snapshot AT this virtual time.
#[derive(Debug, Clone)]
pub struct GridEvent {
    pub t_ms: u64,
    pub grid: GridSnapshot,
}

/// A grid scenario = the world-over-time + the demand placed against it. Pure data, like
/// [`super::sim::Scenario`], one level up: peers join, die, partition, and return as timeline
/// facts, and playing the scenario IS the regression test for the invariants.
#[derive(Debug, Clone)]
pub struct GridScenario {
    pub name: &'static str,
    pub demand: LeaseRequest,
    pub timeline: Vec<GridEvent>,
}

/// Result of playing a grid scenario: the scalar score + the placement trace, so tests assert
/// the SHAPE of adaptation (spread → reclaim → regrow → degrade-local → adopt-new-node).
#[derive(Debug, Clone)]
pub struct GridRunResult {
    pub score: Score,
    pub placements: Vec<Placement>,
}

/// Plays a grid scenario through a placement policy on the virtual clock. Deterministic, no
/// I/O, no randomness — same scenario + same policy → same result, always.
pub struct GridSimulator;

impl GridSimulator {
    pub fn run(scenario: &GridScenario, policy: &dyn GridPlacementPolicy) -> GridRunResult {
        Self::run_traced(scenario, policy, &mut NoopGridCapture)
    }

    /// The glass-boxed run: same simulation, every decision captured with per-node verdicts.
    pub fn run_traced(
        scenario: &GridScenario,
        policy: &dyn GridPlacementPolicy,
        sink: &mut dyn GridCaptureSink,
    ) -> GridRunResult {
        let mut score = Score::default();
        let mut placements = Vec::with_capacity(scenario.timeline.len());
        let mut experience_sum = 0.0_f32;
        let mut last: Option<Placement> = None;

        for ev in &scenario.timeline {
            let placement = policy.place(&ev.grid, &scenario.demand);

            let ooms = placement_oom_count(&ev.grid, &scenario.demand, &placement);
            let stranded = stranded_lanes(&ev.grid, &placement);
            score.oom_count += ooms;
            score.stranded_lanes += stranded;
            if last.as_ref() != Some(&placement) {
                score.grant_changes += 1;
            }

            // The lived room experience under THIS placement: only lanes on nodes that can
            // actually serve count toward responsiveness; an OOM anywhere crashes the lane.
            let effective = placement.total().saturating_sub(stranded);
            let want = scenario.demand.want_concurrency.max(1) as f32;
            let served = (effective as f32 / want).clamp(0.0, 1.0);
            let experience = score_experience(&room_faculties(ooms == 0, served));
            experience_sum += experience;

            sink.capture(Self::explain(
                ev,
                &scenario.demand,
                &placement,
                policy.name(),
                experience,
            ));
            last = Some(placement.clone());
            placements.push(placement);
        }

        if !scenario.timeline.is_empty() {
            score.mean_experience = experience_sum / scenario.timeline.len() as f32;
        }
        GridRunResult { score, placements }
    }

    /// Build the per-node verdicts for one decision from FACTS (snapshot + placement) — the
    /// policy's internals never speak, so the audit works identically for a learned policy.
    fn explain(
        ev: &GridEvent,
        req: &LeaseRequest,
        placement: &Placement,
        policy: &'static str,
        experience: f32,
    ) -> PlacementDecision {
        let assigned = |peer: &PeerId| {
            placement
                .remote
                .iter()
                .find(|(p, _)| p == peer)
                .map(|(_, n)| *n)
                .unwrap_or(0)
        };
        let mut verdicts = vec![NodeVerdict {
            node: NodePick::Local,
            reachable: true,
            free_bytes: ev.grid.local.gpu_free_bytes_live,
            assigned_lanes: placement.local_lanes,
            overflowed: (placement.local_lanes as u64).saturating_mul(req.spike_bytes)
                > ev.grid.local.gpu_free_bytes_live,
            stranded_lanes: 0,
        }];
        for p in &ev.grid.peers {
            let lanes = assigned(&p.peer);
            verdicts.push(NodeVerdict {
                node: NodePick::Peer(p.peer),
                reachable: p.reachable,
                free_bytes: p.capacity.gpu_free_bytes_live,
                assigned_lanes: lanes,
                overflowed: p.reachable
                    && (lanes as u64).saturating_mul(req.spike_bytes)
                        > p.capacity.gpu_free_bytes_live,
                stranded_lanes: if p.reachable { 0 } else { lanes },
            });
        }
        // Placements referencing peers GONE from the snapshot entirely (not merely flagged
        // unreachable) — fully stranded, and the glass box must say so.
        for (peer, lanes) in &placement.remote {
            if !ev.grid.peers.iter().any(|p| p.peer == *peer) {
                verdicts.push(NodeVerdict {
                    node: NodePick::Peer(*peer),
                    reachable: false,
                    free_bytes: 0,
                    assigned_lanes: *lanes,
                    overflowed: false,
                    stranded_lanes: *lanes,
                });
            }
        }
        PlacementDecision {
            t_ms: ev.t_ms,
            policy,
            verdicts,
            placement: placement.clone(),
            want: req.want_concurrency,
            experience,
        }
    }
}

const GB: u64 = 1024 * 1024 * 1024;

/// The canonical grid week, as data — every invariant Joel named, on one timeline. A modest
/// local box plus the misfit fleet (an M2-Air-shaped small peer, a 5090-shaped big one, later
/// a 3090-shaped newcomer). Demand: 6 personas, 2GB spike each; margin 1GB per node.
///
/// | t      | world                          | live placement (local+peers)     |
/// |--------|--------------------------------|----------------------------------|
/// | 0      | all reachable                  | 2 + big 3 + small 1 = 6 (misfit sum) |
/// | 20 min | big peer DIES mid-lease        | 2 + small 1 = 3 (reclaim, no strand) |
/// | 40 min | big peer returns               | back to 6 (regrow)               |
/// | 60 min | TOTAL partition                | 2 local (never blocks)           |
/// | 80 min | network back + NEW node joins  | 2 + new 4 = 6 (adopt mid-session)|
pub fn grid_week() -> GridScenario {
    let dev = |free_gb: u64| DeviceCapacity {
        gpu_total_bytes: 32 * GB,
        gpu_free_bytes_live: free_gb * GB,
        system_ram_free_bytes: 16 * GB,
    };
    // Deterministic ids (same scenario → same result, always — PeerId::from_u128 exists for this).
    let small = PeerId::from_u128(1); // M2-Air-shaped: 3GB free → fits 1 lane
    let big = PeerId::from_u128(2); // 5090-shaped: 7GB free → fits 3 lanes
    let newcomer = PeerId::from_u128(3); // 3090-shaped: 11GB free → fits 5, capped by demand

    let peer = |id: PeerId, free_gb: u64, reachable: bool| PeerCapacity {
        peer: id,
        capacity: dev(free_gb),
        reachable,
    };
    // Local: 5GB free → fits 2 lanes. Demand wants 6 — local alone can never cover it.
    let local = dev(5);

    GridScenario {
        name: "grid-week",
        demand: LeaseRequest {
            consumer: "serving".into(),
            want_concurrency: 6,
            spike_bytes: 2 * GB,
        },
        timeline: vec![
            GridEvent {
                t_ms: 0,
                grid: GridSnapshot {
                    local,
                    peers: vec![peer(small, 3, true), peer(big, 7, true)],
                },
            },
            GridEvent {
                t_ms: 1_200_000, // 20 min: the big peer dies mid-lease
                grid: GridSnapshot {
                    local,
                    peers: vec![peer(small, 3, true), peer(big, 7, false)],
                },
            },
            GridEvent {
                t_ms: 2_400_000, // 40 min: it returns
                grid: GridSnapshot {
                    local,
                    peers: vec![peer(small, 3, true), peer(big, 7, true)],
                },
            },
            GridEvent {
                t_ms: 3_600_000, // 60 min: total partition — the network is GONE
                grid: GridSnapshot {
                    local,
                    peers: vec![peer(small, 3, false), peer(big, 7, false)],
                },
            },
            GridEvent {
                t_ms: 4_800_000, // 80 min: network back, and a brand-new node joins the grid
                grid: GridSnapshot {
                    local,
                    peers: vec![
                        peer(small, 3, false),
                        peer(big, 7, false),
                        peer(newcomer, 11, true),
                    ],
                },
            },
        ],
    }
}

/// Deterministic hash (splitmix64) — the swarm's churn source. NOT randomness: the same
/// (seed, peer, tick) always yields the same world, so a swarm scenario is as replayable as a
/// hand-written one. Determinism is what makes a 2000-decision swarm a regression TEST.
fn churn(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The BitTorrent-shaped world: `n_peers` endpoints that flap like download peers — each tick
/// any peer may vanish or reappear (~70% uptime), and each REACHABLE peer's free GPU wobbles
/// (2..10GB) because its OWNER's work (their game, their render, their own personas) eats and
/// frees it. Nobody is stable; the swarm as a whole is. The local node is deliberately tiny
/// (1 lane) so the demand can only be met by riding the churn.
pub fn bittorrent_swarm(n_peers: u64, n_ticks: u64) -> GridScenario {
    let dev = |free_gb: u64| DeviceCapacity {
        gpu_total_bytes: 16 * GB,
        gpu_free_bytes_live: free_gb * GB,
        system_ram_free_bytes: 16 * GB,
    };
    let timeline = (0..n_ticks)
        .map(|tick| {
            let peers = (0..n_peers)
                .map(|i| {
                    let roll = churn(i * 7919 + tick * 104_729);
                    PeerCapacity {
                        peer: PeerId::from_u128(100 + i as u128),
                        // ~70% uptime — endpoints appear and disappear like torrent peers.
                        reachable: roll % 10 < 7,
                        // Their own workloads eat and free THEIR GPUs tick to tick.
                        capacity: dev(2 + (roll >> 8) % 9),
                    }
                })
                .collect();
            GridEvent {
                t_ms: tick * 10_000,
                grid: GridSnapshot {
                    local: dev(3),
                    peers,
                },
            }
        })
        .collect();
    GridScenario {
        name: "bittorrent-swarm",
        demand: LeaseRequest {
            consumer: "serving".into(),
            want_concurrency: 8,
            spike_bytes: 2 * GB,
        },
        timeline,
    }
}

#[cfg(test)]
mod tests;
