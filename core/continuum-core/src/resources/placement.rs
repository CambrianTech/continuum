//! Inference-lane ADMISSION planning — the intelligent "does this fit, and how?"
//! decision the memory authority owes every new lane demand.
//!
//! [`ResourceGovernor::reconcile_for_demand`](super::governor::ResourceGovernor::reconcile_for_demand)
//! is the RECLAIM half: given over-budget/expiry/relief pressure it frees bytes.
//! This module is the missing ADMISSION half: given the physically-resident set and
//! ONE new lane demand, it returns what the daemon should DO — share an existing
//! lane, spawn a new one, tier lower-priority lanes down first, or (honestly) spill
//! to CPU. It is a PURE function so the canonical scenarios (benchmark eval, a
//! LiveKit huddle, self-scaling mid-call) are unit tests with hand-computed optimal
//! allocations — the spec IS the oracle ([[capacity-fabric-live-never-block-sim-as-gym]]).
//!
//! The gotcha this exists to kill (glass-boxed 2026-07-20): the eval path grabbed a
//! GPU lease LOCALLY, against an under-reported footprint, and stood up a SECOND full
//! copy of a base model that was ALREADY resident — two 24B weight copies don't fit
//! one GPU, Metal 500'd, and the poison took the live lane down too. The authority
//! must own this decision, and its first rule is the one that incident violated:
//! **a demand for a base that is already resident SHARES its weights — it is a
//! co-tenant slot, never a duplicate copy** ([[benchmark-needs-its-own-serving-lane]]
//! UPDATE, [[verify-real-device-numbers-not-a-clamp-premise]]).

/// Priority classes, highest priority FIRST (derived `Ord`: `Live < Eval < Background`).
/// A demand may tier DOWN any resident lane of strictly LOWER priority (numerically
/// greater) to make room; it never preempts an equal-or-higher tier. Live conversation
/// outranks a benchmark, which outranks background work (dreams, training, foraging).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum DemandTier {
    /// A person or persona is waiting on this turn right now (chat, LiveKit huddle).
    Live,
    /// A proctored measurement — important, but preemptible by live work.
    Eval,
    /// Dreams, training, foraging — yields to everything.
    Background,
}

/// A lane the serving layer already has physically resident. `footprint()` is its REAL
/// occupied bytes (weights + KV for its live slots + a compute buffer) — the number the
/// authority must trust, not an accounting stand-in.
#[derive(Clone, Debug)]
pub struct ResidentLane {
    pub lane_id: String,
    pub base_model_id: String,
    /// One copy of the base weights (bytes) — shared across all this lane's slots.
    pub weights_bytes: u64,
    pub slots: u32,
    pub window: u32,
    pub kv_per_token: u64,
    /// Decode-time compute/command-buffer headroom this lane needs resident.
    pub compute_buffer: u64,
    pub tier: DemandTier,
    /// Actively serving / protected — never a preemption victim regardless of tier.
    pub pinned: bool,
}

impl ResidentLane {
    /// Total physical bytes this lane occupies: weights once + KV across all slots +
    /// the compute buffer. This is the concurrent-worst-case (every slot's window full),
    /// which is the number that must fit — sizing for empty slots is what OOM'd the huddle.
    pub fn footprint(&self) -> u64 {
        kv_bytes(self.slots, self.window, self.kv_per_token)
            .saturating_add(self.weights_bytes)
            .saturating_add(self.compute_buffer)
    }
}

/// A request to place one inference lane.
#[derive(Clone, Debug)]
pub struct LaneDemand {
    pub base_model_id: String,
    pub weights_bytes: u64,
    pub slots: u32,
    pub window: u32,
    pub kv_per_token: u64,
    pub compute_buffer: u64,
    pub tier: DemandTier,
    /// ISOLATION POLICY. A hard, disruption-intolerant task (a benchmark exam, an academy
    /// round) demands its OWN dedicated lane — never a co-tenant slot on a lane other
    /// consumers are hammering. A shared slot STARVES it behind their turns (glass-boxed
    /// 2026-07-21: a benchmark forked onto the live persona lane starved behind the woken
    /// personas → empty/timeout, which looked like the model "spinning" but was the lane
    /// being taken away mid-thought). When `true` and a fresh copy fits, [`plan_placement`]
    /// spawns a DEDICATED lane even if the base is already resident; it falls back to sharing
    /// only under real memory pressure (graceful on smaller hardware, never an OOM). Default
    /// `false` → co-tenant/share as before (personas WANT to batch together — never forgotten).
    /// This is the one-boolean policy the exam/academy flip; refine into a richer lease later.
    pub isolate: bool,
}

impl LaneDemand {
    /// KV bytes for THIS demand's slots at its window — the ONLY physical cost when the
    /// base is already resident (weights are shared, not re-loaded).
    pub fn kv_bytes(&self) -> u64 {
        kv_bytes(self.slots, self.window, self.kv_per_token)
    }

    /// Full footprint of a FRESH copy (a base not already resident): weights + KV + buffer.
    pub fn footprint(&self) -> u64 {
        self.kv_bytes()
            .saturating_add(self.weights_bytes)
            .saturating_add(self.compute_buffer)
    }
}

/// KV bytes for `slots` concurrent slots each holding a full `window` of tokens.
fn kv_bytes(slots: u32, window: u32, kv_per_token: u64) -> u64 {
    (slots as u64)
        .saturating_mul(window as u64)
        .saturating_mul(kv_per_token)
}

/// The admission decision. `reclaim` (usually empty) names lower-priority resident lanes
/// the daemon must tier down BEFORE acting, so the action always lands in freed space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Placement {
    /// The base is already resident → add `add_slots` co-tenant slots to `lane_id`. NO new
    /// weights. The daemon PINS the lane for the demand's duration so grow-back / self-heal
    /// can't relaunch it underneath the work (the benchmark's correct home).
    ShareLane {
        lane_id: String,
        add_slots: u32,
        reclaim: Vec<String>,
    },
    /// The base is absent → spawn a new lane (its own weight copy) in the freed space.
    SpawnLane { reclaim: Vec<String> },
    /// Won't fit on the accelerator even after tiering down everything preemptible →
    /// place on CPU (slow but honest), never OOM the resident set to force it.
    CpuSpill { reason: String },
}

/// THE admission planner. Pure: no I/O, no clock. `capacity` is the physical ceiling for
/// the device (already net of any external reserve); `resident` is what physically holds
/// bytes right now; `demand` is the one lane we want to place.
///
/// Order of preference, cheapest/least-disruptive first:
///   1. base already resident            → `ShareLane` (share weights; cost = added KV only)
///   2. fresh copy fits in free space     → `SpawnLane` with no reclaim
///   3. fits after tiering lower tiers    → `ShareLane`/`SpawnLane` with a `reclaim` list
///   4. doesn't fit even then             → `CpuSpill` (loud reason)
pub fn plan_placement(capacity: u64, resident: &[ResidentLane], demand: &LaneDemand) -> Placement {
    let used: u64 = resident.iter().map(ResidentLane::footprint).sum();
    let free = capacity.saturating_sub(used);

    // Rule 0 — ISOLATION POLICY. A hard task (exam/academy) that cannot tolerate disruption
    // wants its OWN lane, never a co-tenant slot on a lane other consumers are hammering
    // (which starves it). When a fresh copy fits, spawn a DEDICATED lane even though the base
    // is already resident — the isolation IS the point, not weight-efficiency. If a fresh copy
    // won't fit, fall through to Rule 1: share the resident base under memory pressure (never
    // OOM), so this degrades gracefully. Non-isolated demands skip this entirely and share as
    // before. See `LaneDemand::isolate`.
    if demand.isolate {
        if let Ok(reclaim) = plan_to_fit(free, demand.footprint(), resident, demand.tier) {
            return Placement::SpawnLane { reclaim };
        }
        // A dedicated copy won't fit even after tiering down preemptible lanes → fall through:
        // share the resident base if present, else the fresh-copy branch reports the CpuSpill.
    }

    // Rule 1 — the base is ALREADY resident: share it. The only new physical cost is the
    // added slots' KV; the weights are not duplicated. Preferred whenever possible: it is
    // strictly cheaper than a second copy AND it is the difference between fitting and
    // OOMing on a single accelerator.
    if let Some(lane) = resident
        .iter()
        .find(|l| l.base_model_id == demand.base_model_id)
    {
        return match plan_to_fit(free, demand.kv_bytes(), resident, demand.tier) {
            Ok(reclaim) => Placement::ShareLane {
                lane_id: lane.lane_id.clone(),
                add_slots: demand.slots,
                reclaim,
            },
            Err(max_free) => Placement::CpuSpill {
                reason: format!(
                    "sharing {} needs {} KV bytes for {} slot(s) but only {} free even after \
                     tiering down every preemptible lane",
                    demand.base_model_id,
                    demand.kv_bytes(),
                    demand.slots,
                    max_free
                ),
            },
        };
    }

    // Rule 2/3 — a base not resident needs a full new copy (weights + KV + buffer).
    match plan_to_fit(free, demand.footprint(), resident, demand.tier) {
        Ok(reclaim) => Placement::SpawnLane { reclaim },
        Err(max_free) => Placement::CpuSpill {
            reason: format!(
                "a fresh lane for {} needs {} bytes but only {} free even after tiering down \
                 every preemptible lane",
                demand.base_model_id,
                demand.footprint(),
                max_free
            ),
        },
    }
}

/// Can `need` bytes be made free? `Ok(reclaim)` = yes, after tiering down these lanes
/// (empty = already fits). `Err(max_free)` = no, even after tiering down everything
/// preemptible — carries the largest free we could reach so the caller can explain it.
///
/// Victim order: lowest priority first (Background before Eval); within a tier, LARGEST
/// footprint first so the fewest lanes are disturbed to reach `need`.
fn plan_to_fit(
    free: u64,
    need: u64,
    resident: &[ResidentLane],
    demand_tier: DemandTier,
) -> Result<Vec<String>, u64> {
    if need <= free {
        return Ok(Vec::new());
    }
    let mut candidates: Vec<&ResidentLane> = resident
        .iter()
        .filter(|l| l.tier > demand_tier && !l.pinned)
        .collect();
    // Lower priority first (tier desc), then largest footprint first (fewest victims).
    candidates.sort_by(|a, b| {
        b.tier
            .cmp(&a.tier)
            .then_with(|| b.footprint().cmp(&a.footprint()))
    });
    let mut freed = free;
    let mut victims = Vec::new();
    for lane in candidates {
        if freed >= need {
            break;
        }
        freed = freed.saturating_add(lane.footprint());
        victims.push(lane.lane_id.clone());
    }
    if freed >= need {
        Ok(victims)
    } else {
        Err(freed)
    }
}

// ============================================================================
// Grid-aware admission — the SAME model-aware kernel, across nodes that come and go.
// ============================================================================
//
// `capacity::grid` owns the grid SUBSTRATE (#176): the gossiped [`GridSnapshot`], the
// deterministic sim, and the resilience invariants (a `reachable` verdict per peer,
// stranded-lane detection, per-node OOM). But its `LocalFirstFitPolicy` places lanes
// MODEL-BLIND — by free bytes — so it would spill a lane to the peer with the most free
// RAM even when another peer already has that base model WARM, paying a needless cold-load
// + weight transfer. This is the grid's version of the single-node incident: it doesn't
// know that "same base already resident" is nearly free.
//
// `plan_grid_placement` is the convergence: it runs the model-aware [`plan_placement`]
// kernel per reachable node and routes by AFFINITY FIRST — a node that already holds the
// weights (`ShareLane`) beats a node with more free bytes every time. `capacity::grid`'s
// `GridPlacementPolicy` becomes a thin adapter over this once its `GridSnapshot` carries
// each node's resident models (the integration slice) — NOT a third parallel allocator.
//
// ─── STATUS 2026-08-10: THE PARAGRAPH ABOVE IS A PLAN, NOT A DESCRIPTION ───────────
//
// Read it as intent. Neither seam is on a live path, and the wording above has now
// misled two agents in a single day — once into nominating `capacity::grid` as the
// surviving allocator, and once into building a pricing policy behind
// `GridPlacementPolicy` because "AffinityFit uses it" implied traffic.
//
// Verified by both of us independently, today:
//   * `GridPlacementPolicy` — zero non-test callers. The ONLY reference outside
//     `capacity/grid.rs` is this comment. Its impls are exercised solely by the
//     `GridScenario` sim in that same file, which is what makes them look alive.
//   * `plan_grid_placement` (below) — zero non-test callers, despite its own doc
//     calling it "THE grid admission planner".
//
// Note the trap in the sentence above: "becomes a thin adapter over this" DEMOTES
// `GridPlacementPolicy` (the pronoun is `plan_grid_placement`), yet it scans as an
// endorsement because it names `GridPlacementPolicy` last. If you are choosing a
// survivor, do not read it as a vote.
//
// The seam that IS live is `modules::grid::router::GridRouter::route`, reached via
// `GridInterceptor → try_route_remote`. Eligibility gating landed there in #2230;
// ranking/pricing composes onto it AFTER the eligibility filter. Anything built
// behind the two policy seams above is unreachable in production until something
// calls them — see #2227 / task #68 for the cleanup decision.

/// One node in the grid's live view: its physical ceiling + what it already serves +
/// the network's `reachable` verdict THIS instant. An unreachable node is a memory, not
/// an offer ([[seamless-persona-failover-model-and-genome]]) — mirrors `capacity::grid::PeerCapacity`.
#[derive(Clone, Debug)]
pub struct GridNode {
    pub node_id: String,
    pub capacity: u64,
    pub resident: Vec<ResidentLane>,
    /// The network's verdict right now. `false` ⇒ not an offer (peer-loss / partition).
    pub reachable: bool,
    /// The local node — no network hop, no weight transfer. Tie-break toward it.
    pub local: bool,
}

impl GridNode {
    /// Free bytes on this node right now = ceiling − everything resident.
    pub fn free(&self) -> u64 {
        self.capacity
            .saturating_sub(self.resident.iter().map(ResidentLane::footprint).sum())
    }
}

/// Where a demand lands on the grid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GridPlacement {
    /// Host on `node_id` via the model-aware per-node decision (share / spawn / cpu-spill).
    Place {
        node_id: String,
        placement: Placement,
    },
    /// No node is reachable at all (a total partition with not even a local offer — a
    /// defensive case; the local node is normally always reachable). Never returned just
    /// because the grid is full: a full grid still returns the best node's `CpuSpill`
    /// (honest, slow), because the fabric NEVER blocks ([[capacity-fabric-live-never-block-sim-as-gym]]).
    NoNodeReachable,
}

/// Rank a per-node decision: cheapest/least-disruptive first.
fn placement_rank(p: &Placement) -> u8 {
    match p {
        Placement::ShareLane { .. } => 0, // weights already warm — no cold-load, no transfer
        Placement::SpawnLane { reclaim } if reclaim.is_empty() => 1, // fits fresh, disturbs nothing
        Placement::SpawnLane { .. } => 2, // fits only after tiering lower tiers down
        Placement::CpuSpill { .. } => 3,  // last resort — slow but never blocks
    }
}

/// THE grid admission planner. Pure. Runs [`plan_placement`] on every REACHABLE node and
/// picks the best by: affinity/least-disruption first (share > clean-spawn > preempt >
/// cpu-spill), then local before remote, then most-free. A joined node simply appears in
/// `nodes` and becomes eligible; a departed node has `reachable=false` and is ignored, so
/// re-planning a stranded demand fails it over to a live node — all derived from the live
/// `nodes` snapshot, never held ([[grid-agreements-swappable-policy-deterministic-rails]]).
pub fn plan_grid_placement(nodes: &[GridNode], demand: &LaneDemand) -> GridPlacement {
    let mut evals: Vec<(&GridNode, Placement)> = nodes
        .iter()
        .filter(|n| n.reachable)
        .map(|n| (n, plan_placement(n.capacity, &n.resident, demand)))
        .collect();

    if evals.is_empty() {
        return GridPlacement::NoNodeReachable;
    }

    evals.sort_by(|(na, pa), (nb, pb)| {
        placement_rank(pa)
            .cmp(&placement_rank(pb))
            .then_with(|| nb.local.cmp(&na.local)) // local (true) before remote (false)
            .then_with(|| nb.free().cmp(&na.free())) // most free first
    });

    let (node, placement) = evals.into_iter().next().expect("non-empty checked above");
    GridPlacement::Place {
        node_id: node.node_id.clone(),
        placement,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;
    // Representative Devstral-24B Q4 numbers used across the scenarios. kv_per_token is
    // chosen so a slot at window 43264 costs ~6.8 GiB — the geometry of the 2026-07-20
    // incident, so the tests reproduce the REAL fit/no-fit boundary, not a toy one.
    const WEIGHTS: u64 = 14 * GIB;
    const KV_PER_TOKEN: u64 = 160 * 1024; // 160 KiB/token
    const COMPUTE: u64 = GIB; // ~1 GiB decode headroom

    fn lane(
        id: &str,
        model: &str,
        slots: u32,
        window: u32,
        tier: DemandTier,
        pinned: bool,
    ) -> ResidentLane {
        ResidentLane {
            lane_id: id.into(),
            base_model_id: model.into(),
            weights_bytes: WEIGHTS,
            slots,
            window,
            kv_per_token: KV_PER_TOKEN,
            compute_buffer: COMPUTE,
            tier,
            pinned,
        }
    }

    fn demand(model: &str, slots: u32, window: u32, tier: DemandTier) -> LaneDemand {
        LaneDemand {
            base_model_id: model.into(),
            weights_bytes: WEIGHTS,
            slots,
            window,
            kv_per_token: KV_PER_TOKEN,
            compute_buffer: COMPUTE,
            tier,
            isolate: false,
        }
    }

    // what this catches: SCENARIO B — a benchmark eval of the SAME base the live personas
    // are served on, on a CONTENDED accelerator, must SHARE the lane (co-tenant slot), NOT
    // stand up a second weight copy. This is the exact 2026-07-20 hard-rs incident: a 2nd
    // 24B copy didn't fit and OOM'd both lanes. Capacity here (~38 GiB, Comfort-fractioned)
    // is one where the second copy genuinely can't fit but the shared slot can.
    #[test]
    fn benchmark_same_base_shares_the_live_lane_never_a_second_copy() {
        let capacity = 38 * GIB;
        // Live lane: 14 (weights) + 1 slot × 43264 × 160KiB (~6.6 GiB) + 1 (compute) ≈ 21.6 GiB.
        let resident = vec![lane("live", "devstral", 1, 43264, DemandTier::Live, true)];
        // Eval wants the SAME base, one slot, the same window.
        let d = demand("devstral", 1, 43264, DemandTier::Eval);

        // A fresh copy (weights+KV+buffer ≈ 21.6 GiB) would NOT fit in the ~16.4 GiB free —
        // proving the incident. Sharing costs only the added KV (~6.6 GiB), which DOES fit.
        assert!(
            d.footprint() > capacity - resident[0].footprint(),
            "a 2nd copy must not fit — else the scenario is toothless"
        );
        assert!(
            d.kv_bytes() <= capacity - resident[0].footprint(),
            "the shared slot must fit"
        );

        match plan_placement(capacity, &resident, &d) {
            Placement::ShareLane {
                lane_id,
                add_slots,
                reclaim,
            } => {
                assert_eq!(lane_id, "live");
                assert_eq!(add_slots, 1);
                assert!(
                    reclaim.is_empty(),
                    "personas are idle enough — no preemption needed"
                );
            }
            other => panic!("same-base eval must SHARE the live lane, got {other:?}"),
        }
    }

    // what this catches: SCENARIO A — a fresh base with plenty of room spawns a co-resident
    // lane and disturbs nothing.
    #[test]
    fn fresh_base_with_room_spawns_coresident_no_reclaim() {
        let capacity = 64 * GIB;
        let resident = vec![lane("live", "devstral", 2, 8192, DemandTier::Live, true)];
        let d = demand("qwen-1.5b", 1, 8192, DemandTier::Eval);
        assert_eq!(
            plan_placement(capacity, &resident, &d),
            Placement::SpawnLane { reclaim: vec![] }
        );
    }

    // what this catches: SCENARIO C — a DIFFERENT base that doesn't fit alongside live serving
    // tiers down a BACKGROUND lane (never the pinned Live lane) to make room, rather than
    // spilling or OOMing. Priority ordering is load-bearing here.
    #[test]
    fn different_base_preempts_background_not_the_live_lane() {
        let capacity = 40 * GIB;
        let resident = vec![
            lane("live", "devstral", 1, 16384, DemandTier::Live, true), // ~17.6 GiB, pinned
            lane(
                "dream",
                "devstral-dream",
                1,
                16384,
                DemandTier::Background,
                false,
            ), // ~17.6 GiB
        ];
        // A fresh Live-tier lane that needs a full copy — only fits if the dream lane yields.
        let d = demand("qwen-coder-14b", 1, 16384, DemandTier::Live);
        match plan_placement(capacity, &resident, &d) {
            Placement::SpawnLane { reclaim } => {
                assert_eq!(
                    reclaim,
                    vec!["dream".to_string()],
                    "must tier down the Background lane, not the pinned Live lane"
                );
            }
            other => panic!("expected SpawnLane preempting the dream lane, got {other:?}"),
        }
    }

    // what this catches: an Eval demand must NOT preempt a Live lane even when that's the
    // only way it would fit — live conversation outranks a benchmark. It spills to CPU instead.
    #[test]
    fn eval_never_preempts_live_it_spills_to_cpu() {
        let capacity = 30 * GIB;
        // One pinned Live lane nearly fills the device; a 2nd DIFFERENT-base copy can't fit.
        let resident = vec![lane("live", "devstral", 1, 24576, DemandTier::Live, true)];
        let d = demand("qwen-coder-14b", 1, 24576, DemandTier::Eval);
        match plan_placement(capacity, &resident, &d) {
            Placement::CpuSpill { .. } => {}
            other => panic!("eval must not preempt Live — expected CpuSpill, got {other:?}"),
        }
    }

    // what this catches: the ISOLATION POLICY — an `isolate` demand (an exam / academy round)
    // gets its OWN dedicated lane even though the base is ALREADY resident, instead of a
    // co-tenant slot that would starve it behind the resident lane's other consumers. This is
    // the autonomic fix for the 2026-07-21 "benchmark forked onto the live lane and starved"
    // incident — no `base_model_id` hand-holding needed. Non-isolated same-base demands still
    // SHARE (proven by same_base_eval_shares_the_live_lane above).
    #[test]
    fn isolated_demand_spawns_dedicated_lane_even_when_base_resident() {
        let capacity = 40 * GIB; // live ~17.5 GiB + a fresh exam copy ~16.25 GiB both fit
        let resident = vec![lane("live", "devstral", 2, 8192, DemandTier::Live, true)];
        let mut d = demand("devstral", 1, 8192, DemandTier::Eval);
        d.isolate = true;
        match plan_placement(capacity, &resident, &d) {
            Placement::SpawnLane { reclaim } => {
                assert!(
                    reclaim.is_empty(),
                    "plenty of room — no preemption to isolate"
                );
            }
            other => panic!("isolated exam must get its OWN lane, got {other:?}"),
        }
    }

    // what this catches: the isolation policy degrades GRACEFULLY — when a dedicated second copy
    // won't fit, the isolate demand falls back to a co-tenant SHARE of the resident base rather
    // than OOMing or spilling. Isolation is a preference, not a hard requirement; on smaller
    // hardware the exam still runs (shared), it just isn't insulated from contention.
    #[test]
    fn isolated_demand_falls_back_to_share_under_memory_pressure() {
        let capacity = 28 * GIB; // live ~17.5 GiB leaves ~10.5 free: a fresh ~16.25 copy won't
                                 // fit, but the ~1.25 GiB share KV does.
        let resident = vec![lane("live", "devstral", 2, 8192, DemandTier::Live, true)];
        let mut d = demand("devstral", 1, 8192, DemandTier::Eval);
        d.isolate = true;
        match plan_placement(capacity, &resident, &d) {
            Placement::ShareLane { lane_id, .. } => assert_eq!(lane_id, "live"),
            other => {
                panic!("under pressure the isolate demand must fall back to SHARE, got {other:?}")
            }
        }
    }

    // what this catches: SCENARIO D — the concurrent-worst-case invariant. A LiveKit huddle
    // asks for N concurrent slots; footprint() must count KV for ALL of them (not one), so a
    // window that fits one slot but overflows N is correctly rejected. Sizing for empty slots
    // is exactly what tipped the shared lane into decode-OOM.
    #[test]
    fn huddle_counts_kv_for_every_concurrent_slot() {
        // 4 slots × 32768 × 160KiB = ~20 GiB of KV alone; + 14 weights + 1 compute ≈ 35 GiB.
        let d = demand("devstral", 4, 32768, DemandTier::Live);
        let one_slot = demand("devstral", 1, 32768, DemandTier::Live);
        assert_eq!(
            d.kv_bytes(),
            4 * one_slot.kv_bytes(),
            "KV must scale with concurrent slots — the worst-case that must fit"
        );
        // On a device that holds one slot's worth of a fresh copy but not four, the 4-slot
        // demand for a fresh base must not claim it fits.
        let capacity = 22 * GIB; // fits a 1-slot copy (~15.6 GiB), not a 4-slot copy (~35 GiB)
        assert!(one_slot.footprint() <= capacity);
        assert!(d.footprint() > capacity);
        assert!(matches!(
            plan_placement(capacity, &[], &d),
            Placement::CpuSpill { .. }
        ));
    }

    // ---- grid scenarios: nodes coming online / going offline --------------------

    fn node(
        id: &str,
        capacity_gib: u64,
        resident: Vec<ResidentLane>,
        reachable: bool,
        local: bool,
    ) -> GridNode {
        GridNode {
            node_id: id.into(),
            capacity: capacity_gib * GIB,
            resident,
            reachable,
            local,
        }
    }

    // what this catches: THE MoE-affinity win. A demand routes to the node that already has
    // the weights WARM (ShareLane) even when another reachable node has far MORE free bytes.
    // The model-blind byte-fill (LocalFirstFitPolicy) would pick the emptier node and pay a
    // needless cold-load + weight transfer — this is the grid version of the 2nd-copy incident.
    #[test]
    fn affinity_routes_to_the_node_that_already_holds_the_weights() {
        let empty_big = node("big", 80, vec![], true, false); // huge + empty, but cold for devstral
        let warm = node(
            "warm",
            40,
            vec![lane(
                "warm-live",
                "devstral",
                1,
                8192,
                DemandTier::Live,
                true,
            )],
            true,
            false,
        );
        let d = demand("devstral", 1, 8192, DemandTier::Eval);
        match plan_grid_placement(&[empty_big, warm], &d) {
            GridPlacement::Place { node_id, placement } => {
                assert_eq!(
                    node_id, "warm",
                    "must route to the node with devstral warm, not the emptier one"
                );
                assert!(matches!(placement, Placement::ShareLane { .. }));
            }
            other => panic!("expected affinity Place on warm, got {other:?}"),
        }
    }

    // what this catches: a JOINED node absorbs demand the busy local node can't fit. Before
    // the peer appears the demand cpu-spills locally; the instant it's in the snapshot,
    // reachable, the demand goes there on GPU. The grid GROWS with no held state.
    #[test]
    fn a_joined_node_absorbs_demand_the_local_node_cannot_fit() {
        // Local is nearly full with a pinned Live devstral lane; a fresh DIFFERENT base won't fit.
        let local_full = node(
            "local",
            24,
            vec![lane("l", "devstral", 1, 20480, DemandTier::Live, true)],
            true,
            true,
        );
        let d = demand("qwen-coder-14b", 1, 16384, DemandTier::Live);

        // Before the join: only the full local node → best it can do is CPU spill.
        match plan_grid_placement(std::slice::from_ref(&local_full), &d) {
            GridPlacement::Place { node_id, placement } => {
                assert_eq!(node_id, "local");
                assert!(
                    matches!(placement, Placement::CpuSpill { .. }),
                    "local can't GPU-host it"
                );
            }
            other => panic!("expected local CpuSpill pre-join, got {other:?}"),
        }

        // A peer joins with room → the demand lands there on GPU (SpawnLane), no reclaim.
        let joined = node("peer-new", 48, vec![], true, false);
        match plan_grid_placement(&[local_full, joined], &d) {
            GridPlacement::Place { node_id, placement } => {
                assert_eq!(node_id, "peer-new", "the joined node must absorb it");
                assert_eq!(placement, Placement::SpawnLane { reclaim: vec![] });
            }
            other => panic!("expected Place on joined peer, got {other:?}"),
        }
    }

    // what this catches: peer-loss FAILOVER. The node that HELD the weights goes unreachable;
    // it must stop being an offer (its warm affinity is a memory, not an offer), and re-planning
    // the stranded demand routes it to a live node — never stalls on the dead one.
    #[test]
    fn a_departed_node_is_not_an_offer_demand_fails_over() {
        // 'dead' has devstral warm (affinity) but is unreachable; 'live' is empty + reachable.
        let dead = node(
            "dead",
            40,
            vec![lane("d", "devstral", 1, 8192, DemandTier::Live, true)],
            false,
            false,
        );
        let live = node("live", 48, vec![], true, false);
        let d = demand("devstral", 1, 8192, DemandTier::Eval);
        match plan_grid_placement(&[dead, live], &d) {
            GridPlacement::Place { node_id, .. } => {
                assert_eq!(
                    node_id, "live",
                    "must fail over to the reachable node, not the dead affinity node"
                );
            }
            other => panic!("expected failover Place on live, got {other:?}"),
        }
    }

    // what this catches: total PARTITION degrades to local-only and never blocks. All peers
    // unreachable → only the local node is an offer; it serves what it honestly fits.
    #[test]
    fn total_partition_degrades_to_local_only() {
        let local = node("local", 64, vec![], true, true);
        let gone_a = node("a", 80, vec![], false, false);
        let gone_b = node("b", 80, vec![], false, false);
        let d = demand("devstral", 1, 8192, DemandTier::Live);
        match plan_grid_placement(&[local, gone_a, gone_b], &d) {
            GridPlacement::Place { node_id, placement } => {
                assert_eq!(node_id, "local");
                assert_eq!(placement, Placement::SpawnLane { reclaim: vec![] });
            }
            other => panic!("expected local-only Place under partition, got {other:?}"),
        }
    }
}
