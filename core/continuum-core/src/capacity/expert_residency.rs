//! Expert residency — the DYNAMIC, RUNTIME, RUST port of Plasticity Compaction
//! (`tools/scripts/compaction/*.py`, proven static+offline on qwen3.5-35b-a3b).
//!
//! Joel 2026-07-25: "We can build this dynamic system. We did it more statically
//! before. Again, never Python." The Python did it once, offline, to bake a
//! pruned model. This does it CONTINUOUSLY, in the serving path, to a FROZEN
//! full expert pool — so one K3-class model (896 experts, 16 awake/token) serves
//! frontend-code, chat, and vision each from its OWN hot subset, no re-prune.
//! This is sentinel-AI-as-PGO ([[genome-foundry-sentinel]]).
//!
//! ## The three signals (ported from the Python steps)
//!
//! 1. **Gate magnitude** (`analyze_gate_weights.py`): the router's per-expert
//!    weight magnitude = the model's baked-in preference, readable from the GGUF
//!    without a forward pass. The STATIC seed of residency priority — good on a
//!    cold start before any live hits.
//! 2. **Live activation hits** (`profile_expert_activation.py`): the serving
//!    path tallies which experts actually fire, per (persona, TaskKind). The
//!    DYNAMIC signal — this is what makes a coding lane pin coding experts and a
//!    chat lane pin different ones from the SAME frozen pool.
//! 3. **Residency plan** (`prune_experts.py`): keep the top experts by combined
//!    priority that fit live VRAM (hot), the next tier in RAM (warm, faulted to
//!    GPU per token — the KTransformers / llama.cpp `-ot` split), the rest frozen
//!    on disk (cold, faulted on a miss). LRU on the resident set.
//!
//! Why locality makes K3-on-one-5090 real (BigMama's feasibility math + web):
//! 50–60B active/token @4bit ≈ 27–32GB reads/token IF every expert is a cold
//! disk miss → 0.3–0.5 tok/s, unusable. But a task domain reuses a NARROW expert
//! subset, so with subset residency the disk reads drop 10–100×. Residency is
//! not an optimization; it is what makes the model servable at all.
//!
//! Pure + sim-provable, exactly like [`super::lease::decide_lane`]: reads a
//! profile + a live capacity and returns a plan. The impure parts — reading GGUF
//! gate magnitudes, tallying live hits, driving llama.cpp `-ot` — are BigMama's
//! serving mechanics; this is the brain they consult.

use std::collections::HashMap;

use super::{lanes_that_fit, DeviceCapacity};

/// One expert, addressed by (layer, index) — matches the GGUF tensor naming and
/// the router's per-layer expert space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExpertId {
    pub layer: u32,
    pub expert: u32,
}

/// The per-(persona, TaskKind) residency signal. `gate_magnitude` is the static
/// seed (from the GGUF); `hits` is the live PGO tally accumulated in the serving
/// path. The combined priority ranks experts for residency.
#[derive(Debug, Clone, Default)]
pub struct ExpertActivationProfile {
    /// Static seed: router weight magnitude per expert (higher = preferred).
    /// Empty on a model whose gate tensors haven't been scanned yet — the plan
    /// then rides on `hits` alone (and cold-starts to all-warm, never wrong).
    pub gate_magnitude: HashMap<ExpertId, f32>,
    /// Dynamic PGO: how many times each expert fired on THIS lane's workload.
    /// The serving path increments these; a decay is applied by the caller so
    /// the profile tracks the current task, not all history.
    pub hits: HashMap<ExpertId, u64>,
}

impl ExpertActivationProfile {
    /// Combined residency priority for one expert. Live hits dominate (measured
    /// beats predicted — the PGO principle); gate magnitude breaks ties and
    /// seeds cold-start. Normalized so the two signals compose regardless of
    /// scale: hits are the integer rank, magnitude the fractional tiebreak.
    fn priority(&self, e: &ExpertId) -> f64 {
        let hits = *self.hits.get(e).unwrap_or(&0) as f64;
        let mag = *self.gate_magnitude.get(e).unwrap_or(&0.0) as f64;
        // hits is the dominant integer term; magnitude in [0,1)-ish rides the
        // fraction so it only decides among equal-hit experts.
        hits + (mag.tanh() * 0.999)
    }

    /// Every expert the profile knows about (union of both signals).
    fn known_experts(&self) -> Vec<ExpertId> {
        let mut set: std::collections::BTreeSet<ExpertId> =
            self.hits.keys().copied().collect();
        set.extend(self.gate_magnitude.keys().copied());
        set.into_iter().collect()
    }
}

/// Where each expert lives this tick. `hot` faults never (GPU-resident); `warm`
/// faults GPU-per-token from RAM (bounded, fast); `cold` faults RAM-per-miss
/// from disk (rare when locality holds). The dense trunk (attention + shared)
/// is always hot and is NOT in this plan — it's the non-expert residency the
/// serving launch pins unconditionally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpertResidencyPlan {
    pub hot: Vec<ExpertId>,
    pub warm: Vec<ExpertId>,
    pub cold: Vec<ExpertId>,
}

/// Decide expert residency for one lane against LIVE capacity. The pure brain.
///
/// - `hot` = the top experts by combined priority that fit live free VRAM after
///   the margin (`lanes_that_fit` — the SAME fit rule everywhere).
/// - `warm` = the next tier that fits system RAM (fault-to-GPU per token).
/// - `cold` = the rest (frozen on disk, faulted on a miss).
///
/// Degenerate-safe: `expert_bytes == 0` (unmeasured) puts everything WARM
/// (honest "don't pin what we can't size" — never a false hot claim). An empty
/// profile with sized experts still fits a hot tier by count; priority is 0 so
/// the choice is arbitrary-but-bounded until live hits arrive.
pub fn plan_expert_residency(
    profile: &ExpertActivationProfile,
    cap: &DeviceCapacity,
    expert_bytes: u64,
    margin_bytes: u64,
) -> ExpertResidencyPlan {
    let mut experts = profile.known_experts();
    // Highest priority first — hot experts are the ones the workload routes to.
    experts.sort_by(|a, b| {
        profile
            .priority(b)
            .partial_cmp(&profile.priority(a))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(b)) // stable, deterministic tiebreak
    });

    if expert_bytes == 0 {
        // Can't size residency — warm everything (RAM-faulted), pin nothing.
        return ExpertResidencyPlan {
            hot: Vec::new(),
            warm: experts,
            cold: Vec::new(),
        };
    }

    let hot_cap =
        lanes_that_fit(cap.gpu_free_bytes_live, margin_bytes, expert_bytes) as usize;
    let warm_cap =
        lanes_that_fit(cap.system_ram_free_bytes, margin_bytes, expert_bytes) as usize;

    let mut hot = Vec::new();
    let mut warm = Vec::new();
    let mut cold = Vec::new();
    for e in experts {
        if hot.len() < hot_cap {
            hot.push(e);
        } else if warm.len() < warm_cap {
            warm.push(e);
        } else {
            cold.push(e);
        }
    }
    ExpertResidencyPlan { hot, warm, cold }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;

    fn dev(gpu_free_gb: u64, ram_free_gb: u64) -> DeviceCapacity {
        DeviceCapacity {
            gpu_total_bytes: 32 * GB,
            gpu_free_bytes_live: gpu_free_gb * GB,
            system_ram_free_bytes: ram_free_gb * GB,
        }
    }

    fn e(layer: u32, expert: u32) -> ExpertId {
        ExpertId { layer, expert }
    }

    // what this catches: LOCALITY is what makes K3-on-one-5090 real. A workload
    // that routes heavily to a small subset pins THAT subset hot (VRAM-resident),
    // so the disk-fault-every-token disaster is avoided. If priority ordering or
    // the fit rule drifts, the hot set stops tracking the workload and residency
    // degrades to the naive all-cold case (0.3 tok/s).
    #[test]
    fn hot_set_tracks_the_workload_and_fits_vram() {
        let mut p = ExpertActivationProfile::default();
        // 8 experts; a coding workload hammered experts 0..3, barely touched 4..7.
        for x in 0..4 {
            p.hits.insert(e(0, x), 1000 - x as u64); // hot subset
        }
        for x in 4..8 {
            p.hits.insert(e(0, x), 1); // cold tail
        }
        // 2GB/expert, VRAM fits ~4 experts after a 2GB margin (10GB free).
        let plan = plan_expert_residency(&p, &dev(10, 64), 2 * GB, 2 * GB);
        // The 4 hottest experts (0..3) are hot; the cold tail is warm/cold.
        assert_eq!(plan.hot.len(), 4, "hot set sized to VRAM");
        for x in 0..4 {
            assert!(plan.hot.contains(&e(0, x)), "hot expert {x} pinned");
        }
        for x in 4..8 {
            assert!(!plan.hot.contains(&e(0, x)), "cold-tail expert {x} not hot");
        }
    }

    // what this catches: gate magnitude is the cold-start seed — with ZERO live
    // hits, the highest-magnitude experts still lead residency (the model's baked
    // preference), so a fresh lane isn't random before profiling warms up.
    #[test]
    fn gate_magnitude_seeds_cold_start_before_any_hits() {
        let mut p = ExpertActivationProfile::default();
        p.gate_magnitude.insert(e(0, 0), 5.0); // preferred
        p.gate_magnitude.insert(e(0, 1), 0.1);
        p.gate_magnitude.insert(e(0, 2), 0.1);
        // VRAM fits exactly 1 expert.
        let plan = plan_expert_residency(&p, &dev(4, 64), 2 * GB, 2 * GB);
        assert_eq!(plan.hot, vec![e(0, 0)], "highest-magnitude expert leads");
    }

    // what this catches: the degenerate-safe contracts — unmeasured expert size
    // warms everything (never a false hot pin), and the RAM tier bounds warm so
    // the true cold set spills to disk (the 896-frozen-on-D case).
    #[test]
    fn unsized_warms_all_and_ram_bounds_the_warm_tier() {
        let mut p = ExpertActivationProfile::default();
        for x in 0..8 {
            p.hits.insert(e(0, x), 10);
        }
        let no_size = plan_expert_residency(&p, &dev(10, 64), 0, 0);
        assert_eq!(no_size.warm.len(), 8, "unsized ⇒ all warm, none pinned");
        assert!(no_size.hot.is_empty());

        // VRAM fits 1, RAM fits 2 more → 1 hot, 2 warm, 5 cold.
        let tiny = plan_expert_residency(&p, &dev(4, 6), 2 * GB, 2 * GB);
        assert_eq!(tiny.hot.len(), 1);
        assert_eq!(tiny.warm.len(), 2);
        assert_eq!(tiny.cold.len(), 5, "the rest freeze on disk");
    }
}
