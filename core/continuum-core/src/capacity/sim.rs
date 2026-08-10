//! The deterministic capacity simulator — the VDD gate AND (later) the training gym.
//!
//! The world is DATA (a [`Scenario`]: a timeline of live [`DeviceCapacity`] readings under
//! a workload demand). The [`Simulator`] plays it through the SAME [`AllocationPolicy`] the
//! production governor runs — no reimplementation — and folds the outcome into the scalar
//! [`Score`]. So a scenario that reproduces a live incident IS a permanent regression test,
//! provable with zero hardware, and the corpus of scenarios becomes the training set.
//!
//! Seed scope: one scenario (tonight's OOM — a game/browser eats the GPU mid-session, then
//! frees it) and the two outlier policies that prove the interface: the STATIC control that
//! must OOM (the bug) and the FIT policy that must not (the fix) and must regrow when the
//! capacity returns. Everything else (peers, network partition, multi-metric scoring, the
//! learned policy) grows on this exact shape.

use super::consumer::QualityModel;
use super::score::score_experience;
use super::{grant_would_oom, AllocationPolicy, DeviceCapacity, Grant, LeaseRequest, Score};

const GB: u64 = 1024 * 1024 * 1024;

/// One tick of the world: the live capacity AT this virtual time. Between events the world
/// holds; each event is a change (a game opens, a peer joins, the browser frees GPU).
#[derive(Debug, Clone, Copy)]
pub struct CapacityEvent {
    pub t_ms: u64,
    pub capacity: DeviceCapacity,
}

/// A scenario = the world over time + the workload demanded against it. Pure data; this is
/// the JSON/struct schema the design's §9 describes (serde derives land when we load them
/// from disk — the seed keeps them in-code so the first gate needs no fixtures).
#[derive(Debug, Clone)]
pub struct Scenario {
    pub name: &'static str,
    /// Constant demand for the seed (N personas wanting to prefill at once). Later a
    /// per-consumer timeline of demands.
    pub demand: LeaseRequest,
    /// Capacity readings over virtual time, ascending by `t_ms`.
    pub timeline: Vec<CapacityEvent>,
}

/// Result of playing a scenario: the scalar score AND the per-tick grant trace (so tests
/// can assert the *shape* of adaptation — shrink under pressure, regrow when it lifts).
#[derive(Debug, Clone)]
pub struct RunResult {
    pub score: Score,
    pub grants: Vec<Grant>,
}

/// Plays a scenario through a policy on a virtual clock. Deterministic: same scenario +
/// same policy + same quality model → same result, always. No real time, no I/O, no randomness.
pub struct Simulator;

impl Simulator {
    pub fn run(
        scenario: &Scenario,
        policy: &dyn AllocationPolicy,
        quality: &dyn QualityModel,
    ) -> RunResult {
        let mut score = Score::default();
        let mut grants = Vec::with_capacity(scenario.timeline.len());
        let mut experience_sum = 0.0_f32;
        let mut last: Option<Grant> = None;
        for ev in &scenario.timeline {
            // The governor's grant is DERIVED from the live capacity at this instant — the
            // whole point. A policy blind to `ev.capacity` (the static control) grants the
            // same thing as the world shrinks under it → OOM.
            let grant = policy.grant(&ev.capacity, &scenario.demand);
            if grant_would_oom(&ev.capacity, &scenario.demand, &grant) {
                score.oom_count += 1;
            }
            if last != Some(grant) {
                score.grant_changes += 1;
            }
            // The lived experience under THIS grant: the consumer's quality model maps the grant
            // to faculty scores, the gate composes them. A crash (OOM) zeroes the critical
            // faculties → the perceived-quality reward collapses, which is why shed-load beats
            // hold-and-crash on the number a learned policy climbs.
            experience_sum += score_experience(&quality.faculties(&ev.capacity, &scenario.demand, &grant));
            last = Some(grant);
            grants.push(grant);
        }
        if !scenario.timeline.is_empty() {
            score.mean_experience = experience_sum / scenario.timeline.len() as f32;
        }
        RunResult { score, grants }
    }
}

/// Tonight's incident, as data: a 24B on a 64GB box serving 4 personas. `spike_bytes` (~2GB)
/// is the transient prefill compute buffer per concurrent prefill — the term the static
/// `weights/16` reserve got wrong. At t=30s a game eats 6GB of GPU (free 13→7); at t=900s it
/// closes (free 7→13). 4 concurrent spikes × 2GB = 8GB does NOT fit 7GB free → a static
/// grant of 4 OOMs; a live-fit grant shrinks to 3 then regrows to 4.
pub fn opera_eats_gpu_mid_session() -> Scenario {
    let dev = |free_gb: u64| DeviceCapacity {
        gpu_total_bytes: 55 * GB,
        gpu_free_bytes_live: free_gb * GB,
        system_ram_free_bytes: 40 * GB,
    };
    Scenario {
        name: "opera-eats-gpu-mid-session",
        demand: LeaseRequest {
            consumer: "serving".into(),
            want_concurrency: 4,
            spike_bytes: 2 * GB,
        },
        timeline: vec![
            CapacityEvent { t_ms: 0, capacity: dev(13) },        // calm: room for 4
            CapacityEvent { t_ms: 30_000, capacity: dev(7) },    // game opens → must shrink
            CapacityEvent { t_ms: 900_000, capacity: dev(13) },  // game closes → must regrow
        ],
    }
}

// ─────────────────────────── the symmetric grid ───────────────────────────
// "All computers have both ends" (Joel): every node is the SAME role — consumer AND provider.
// There is no node TYPE and no central demand a placer hands out (that's the abandoned
// placement model in `grid.rs`, being deleted). A node PROVIDES exactly the surplus its
// capacity has beyond its OWN demand, and CONSUMES exactly the deficit its demand has beyond
// its capacity. An idle datacenter box is all-surplus THIS tick and can demand at any later
// tick — same node, same rule, different instantaneous workload. The grid is N of the ONE
// single-device primitive above: each node runs the SAME `FitPolicy` over its LOCAL free plus
// the reachable surplus it can borrow. Node join, drop, partition, return, and the
// provider→consumer flip are all just changes to who is present and to each node's demand.

/// One node's live reading: its OWN capacity and its OWN demand. Both, always — no
/// provider/consumer type split.
#[derive(Debug, Clone)]
pub struct NodeReading {
    pub capacity: DeviceCapacity,
    pub demand: LeaseRequest,
}

/// The surplus a node can lend to the pool: free GPU bytes beyond what its OWN demand needs.
/// Zero demand ⇒ its whole free is surplus (a pure provider this tick); a node whose demand
/// meets or exceeds its free lends nothing (all its free serves itself).
pub fn node_surplus(node: &NodeReading) -> u64 {
    let own_need = (node.demand.want_concurrency as u64).saturating_mul(node.demand.spike_bytes);
    node.capacity.gpu_free_bytes_live.saturating_sub(own_need)
}

/// A symmetric grid scenario: N nodes over a shared virtual clock. `ticks[t]` is the set of
/// nodes PRESENT at tick t (a node absent from a tick is partitioned / not-yet-joined / gone —
/// it neither lends nor borrows). Pure data, like [`Scenario`] one level out.
#[derive(Debug, Clone)]
pub struct SymmetricGridScenario {
    pub name: &'static str,
    pub ticks: Vec<Vec<(&'static str, NodeReading)>>,
}

/// Per-node outcome at one tick: the effective free it fit against (local + borrowed surplus)
/// and the grant that fell out of the ONE policy.
#[derive(Debug, Clone)]
pub struct NodeGrant {
    pub node: &'static str,
    pub effective_free_bytes: u64,
    pub grant: Grant,
}

/// Play the symmetric grid through ONE policy per node — no placer. Each PRESENT node fits its
/// OWN demand to its LOCAL free PLUS the reachable surplus it can borrow (Σ of the OTHER present
/// nodes' [`node_surplus`]). That Σ is exactly what a live tier-projection fills from real peers
/// in production; here it is scenario data. Deterministic: same scenario + policy → same trace.
///
/// FIRST-SLICE SCOPE (named, not hidden): the pool is summed and offered to each borrower in
/// full — exact when at most one node has a deficit per tick (the seed scenarios). Multi-borrower
/// CONTENTION (two deficits drawing the same surplus → fair draw-down, never double-lend) is the
/// next slice; [`super::lanes_that_fit`]'s "sum of per-node fits, never a hiding aggregate" rule
/// is where it lands.
pub fn run_symmetric_grid(
    scenario: &SymmetricGridScenario,
    policy: &dyn AllocationPolicy,
) -> Vec<Vec<NodeGrant>> {
    let mut out = Vec::with_capacity(scenario.ticks.len());
    for present in &scenario.ticks {
        let mut tick = Vec::with_capacity(present.len());
        for entry in present {
            let name: &'static str = entry.0;
            let node: &NodeReading = &entry.1;
            // Reachable surplus = Σ of the OTHER present nodes' lendable spare. In production
            // this Σ is the live tier-projection over reachable peers; here it is scenario data.
            let borrowable: u64 = present
                .iter()
                .filter(|other| other.0 != name)
                .map(|other| node_surplus(&other.1))
                .sum();
            let effective_free = node.capacity.gpu_free_bytes_live.saturating_add(borrowable);
            let view = DeviceCapacity {
                gpu_free_bytes_live: effective_free,
                ..node.capacity
            };
            tick.push(NodeGrant {
                node: name,
                effective_free_bytes: effective_free,
                grant: policy.grant(&view, &node.demand),
            });
        }
        out.push(tick);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capacity::consumer::LiveRoomServing;
    use crate::capacity::{FitPolicy, StaticConcurrencyPolicy};

    // what this catches: THE 2026-07-16 OOM, reproduced deterministically without hardware.
    // A policy that grants concurrency by persona count, blind to live free GPU, keeps
    // granting 4 concurrent prefill spikes after a game eats the GPU — 4×2GB=8GB into 7GB
    // free → OOM. This is the negative control the fit policy must beat; if it stops OOMing,
    // the scenario or the OOM model drifted and the gate is no longer proving anything.
    #[test]
    fn static_concurrency_ooms_when_a_game_eats_the_gpu() {
        let result = Simulator::run(
            &opera_eats_gpu_mid_session(),
            &StaticConcurrencyPolicy { fixed: 4 },
            &LiveRoomServing,
        );
        assert!(
            result.score.oom_count > 0,
            "a static concurrency blind to live free GPU MUST OOM when external load spikes \
             — this is tonight's bug reproduced. grants={:?}",
            result.grants
        );
    }

    // what this catches: THE FIX. Deriving concurrency from live free GPU each tick means the
    // grant shrinks under the game (never OOMs) and REGROWS when the game closes — the
    // bidirectional live adaptation the whole fabric is built on. Zero OOM AND a grant that
    // goes down-then-up over the timeline.
    #[test]
    fn fit_policy_never_ooms_and_regrows_when_the_gpu_frees() {
        let result = Simulator::run(
            &opera_eats_gpu_mid_session(),
            &FitPolicy { safety_margin_bytes: GB },
            &LiveRoomServing,
        );
        assert_eq!(
            result.score.oom_count, 0,
            "fit derives concurrency from LIVE free GPU → shrinks under the game, never OOMs. \
             grants={:?}",
            result.grants
        );
        // Shape of adaptation: full at calm, shrunk during the game, back to full after.
        let c: Vec<u32> = result.grants.iter().map(|g| g.concurrency).collect();
        assert_eq!(c[0], 4, "calm: grants the full demand");
        assert!(c[1] < c[0], "game opens: shrinks below full ({} !< {})", c[1], c[0]);
        assert_eq!(c[2], 4, "game closes: REGROWS to full — capacity growth is first-class");
    }

    // what this catches: THE PERCEPTION REWARD — the number a learned policy actually climbs.
    // Both policies run the same incident; the fit policy sheds one lane during the game while
    // the static policy holds and crashes. On oom_count they already differ; here we prove they
    // differ on MEAN EXPERIENCE, which is the reward the gym maximizes. If experience scoring
    // ever stopped punishing the crash (e.g. the critical gate went additive), these would
    // converge and a learned policy would have no signal telling it not to OOM the room.
    #[test]
    fn fit_beats_static_on_perceived_experience_not_just_oom_count() {
        let scenario = opera_eats_gpu_mid_session();
        let fit = Simulator::run(&scenario, &FitPolicy { safety_margin_bytes: GB }, &LiveRoomServing);
        let stat = Simulator::run(&scenario, &StaticConcurrencyPolicy { fixed: 4 }, &LiveRoomServing);
        assert!(
            fit.score.mean_experience > stat.score.mean_experience,
            "shedding a lane to stay alive must score HIGHER perceived experience than holding \
             and crashing (fit={}, static={})",
            fit.score.mean_experience,
            stat.score.mean_experience
        );
        assert!(
            fit.score.mean_experience > 0.85,
            "the fit policy keeps the room a good experience throughout, got {}",
            fit.score.mean_experience
        );
    }

    // ── the symmetric grid: every node consumer AND provider, ONE FitPolicy per node, no placer.
    // "All computers have both ends" — join/drop/partition/return and the provider→consumer flip
    // are all just changes to who is present and to each node's own demand, and accommodation
    // falls out of the SAME single-device fit applied over local + reachable surplus.
    mod symmetric_grid {
        use super::*;

        const MARGIN: u64 = GB;

        fn node(free_gb: u64, want: u32, spike_gb: u64) -> NodeReading {
            NodeReading {
                capacity: DeviceCapacity {
                    gpu_total_bytes: 64 * GB,
                    gpu_free_bytes_live: free_gb * GB,
                    system_ram_free_bytes: 40 * GB,
                },
                demand: LeaseRequest {
                    consumer: "serving".into(),
                    want_concurrency: want,
                    spike_bytes: spike_gb * GB,
                },
            }
        }

        fn conc(tick: &[NodeGrant], name: &str) -> u32 {
            tick.iter().find(|g| g.node == name).expect("node present this tick").grant.concurrency
        }

        // what this catches: THE GRID GROWS CAPABILITY ON JOIN. A laptop with a deficit (wants 4
        // spikes, local free fits only 2) grants 2 alone. When an idle provider joins (demand 0 →
        // all free is surplus), the laptop borrows it and grants its full 4 — one FitPolicy over
        // local+reachable, no placer. If it stops growing, the Σ-spare fold or the join drifted.
        #[test]
        fn a_joining_provider_grows_a_deficit_nodes_grant() {
            let scenario = SymmetricGridScenario {
                name: "join-grows",
                ticks: vec![
                    vec![("laptop", node(10, 4, 4))],
                    vec![("laptop", node(10, 4, 4)), ("server", node(40, 0, 4))],
                ],
            };
            let trace = run_symmetric_grid(&scenario, &FitPolicy { safety_margin_bytes: MARGIN });
            assert_eq!(conc(&trace[0], "laptop"), 2, "alone: (10-1)/4 = 2 fit locally");
            assert_eq!(
                conc(&trace[1], "laptop"), 4,
                "provider joins → laptop borrows 40GB surplus → grants its full demand of 4"
            );
        }

        // what this catches: A LONE NODE NEVER BLOCKS — the "works without grid" end. Total
        // partition, tiny local free that fits zero full spikes, still grants ≥1: a resident model
        // runs at least one prefill or it shouldn't be resident. Same code, zero peers, never zero.
        #[test]
        fn a_partitioned_node_never_blocks() {
            let scenario = SymmetricGridScenario {
                name: "partition-never-blocks",
                ticks: vec![vec![("solo", node(2, 4, 4))]],
            };
            let trace = run_symmetric_grid(&scenario, &FitPolicy { safety_margin_bytes: MARGIN });
            assert!(conc(&trace[0], "solo") >= 1, "a lone node with no borrowable spare still grants ≥1");
        }

        // what this catches: SHRINK-THEN-REGROW across a peer leaving and returning. Down is half
        // the law: when the provider partitions the laptop sheds to its local fit, and when it
        // returns the laptop regrows — same node, same policy, capacity events in both directions.
        #[test]
        fn a_node_sheds_when_its_peer_leaves_and_regrows_when_it_returns() {
            let full = || vec![("laptop", node(10, 4, 4)), ("server", node(40, 0, 4))];
            let scenario = SymmetricGridScenario {
                name: "shed-then-regrow",
                ticks: vec![full(), vec![("laptop", node(10, 4, 4))], full()],
            };
            let trace = run_symmetric_grid(&scenario, &FitPolicy { safety_margin_bytes: MARGIN });
            let c: Vec<u32> = (0..3).map(|t| conc(&trace[t], "laptop")).collect();
            assert_eq!(c[0], 4, "borrowing the peer's surplus");
            assert!(c[1] < c[0], "peer partitions → shed to local fit ({} !< {})", c[1], c[0]);
            assert_eq!(c[2], 4, "peer returns → REGROW — down is symmetric with up");
        }

        // what this catches: THE PROVIDER→CONSUMER FLIP — the richest invariant, and proof there is
        // no node "type". A server lends its whole free (demand 0) so the laptop runs 4. When the
        // SAME server picks up its OWN work (demand rises), it reclaims that surplus for itself and
        // the laptop must shed gracefully — no OOM against its shrunken effective view. A lender's
        // rising demand IS the grid shrinking for its borrowers: same mechanism as a partial drop.
        #[test]
        fn a_lenders_rising_demand_makes_its_borrower_shed_without_oom() {
            let scenario = SymmetricGridScenario {
                name: "provider-to-consumer-flip",
                ticks: vec![
                    vec![("laptop", node(10, 4, 4)), ("server", node(40, 0, 4))],
                    vec![("laptop", node(10, 4, 4)), ("server", node(40, 9, 4))],
                ],
            };
            let trace = run_symmetric_grid(&scenario, &FitPolicy { safety_margin_bytes: MARGIN });
            assert_eq!(conc(&trace[0], "laptop"), 4, "server all-surplus → laptop runs full 4");
            let borrower = trace[1].iter().find(|g| g.node == "laptop").unwrap();
            assert!(borrower.grant.concurrency < 4, "server reclaims its surplus → laptop sheds");
            assert!(
                (borrower.grant.concurrency as u64) * (4 * GB) <= borrower.effective_free_bytes,
                "the shed grant fits the shrunken effective free — graceful, never OOM"
            );
            assert!(conc(&trace[1], "server") >= 1, "the lender is now ALSO a consumer of its own work");
        }
    }
}
