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
}
