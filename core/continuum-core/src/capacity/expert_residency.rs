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

/// Experts known to the profile, highest residency-priority first (stable tiebreak). The one
/// place the ranking lives — both the 3-tier convenience and the N-tier engine consult it.
fn ranked_experts(profile: &ExpertActivationProfile) -> Vec<ExpertId> {
    let mut experts = profile.known_experts();
    experts.sort_by(|a, b| {
        profile
            .priority(b)
            .partial_cmp(&profile.priority(a))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(b)) // stable, deterministic tiebreak
    });
    experts
}

/// Decide expert residency for one lane against LIVE capacity. The 3-tier convenience view —
/// hot(VRAM) / warm(RAM) / cold(disk) — which is the 2-promotable-tier special case of
/// [`plan_tiered_residency`] (VRAM + RAM). Use the tiered form directly when the machine has a
/// deeper storage hierarchy (VRAM → RAM → NVMe-flash → cold-RAID — Joel's real box).
///
/// - `hot` = the top experts by combined priority that fit live free VRAM after
///   the margin (`lanes_that_fit` — the SAME fit rule everywhere).
/// - `warm` = the next tier that fits system RAM (fault-to-GPU per token).
/// - `cold` = the rest (frozen on the backing store, faulted on a miss).
///
/// Degenerate-safe: `expert_bytes == 0` (unmeasured) puts everything WARM
/// (honest "don't pin what we can't size" — never a false hot claim; RAM-resident also beats
/// leaving a RAM-capable model on disk). An empty profile with sized experts still fits a hot
/// tier by count; priority is 0 so the choice is arbitrary-but-bounded until live hits arrive.
pub fn plan_expert_residency(
    profile: &ExpertActivationProfile,
    cap: &DeviceCapacity,
    expert_bytes: u64,
    margin_bytes: u64,
) -> ExpertResidencyPlan {
    if expert_bytes == 0 {
        // Can't size residency — warm everything (RAM-faulted), pin nothing. The 3-tier view's
        // RAM tier is its safe fallback; the general N-tier planner has no privileged RAM tier
        // and instead makes the honest "promote nothing" call (see plan_tiered_residency).
        return ExpertResidencyPlan {
            hot: Vec::new(),
            warm: ranked_experts(profile),
            cold: Vec::new(),
        };
    }
    let tiers = [
        ResidencyTier {
            medium: ResidencyMedium::Vram,
            free_bytes: cap.gpu_free_bytes_live,
        },
        ResidencyTier {
            medium: ResidencyMedium::Ram,
            free_bytes: cap.system_ram_free_bytes,
        },
    ];
    let plan = plan_tiered_residency(profile, &tiers, expert_bytes, margin_bytes);
    let mut it = plan.tiers.into_iter();
    let hot = it.next().map(|(_, v)| v).unwrap_or_default();
    let warm = it.next().map(|(_, v)| v).unwrap_or_default();
    ExpertResidencyPlan {
        hot,
        warm,
        cold: plan.cold,
    }
}

/// A storage medium in the residency hierarchy, ordered fastest-fault-first. Derived `Ord`
/// IS the fault-cost order: VRAM (never faults — resident) < RAM (fault-to-GPU per token) <
/// Flash (NVMe, single-digit-ms miss) < ColdDisk (the RAID that backs every expert). Joel's
/// box has all four; a MacBook Air collapses to VRAM/UMA + RAM + disk. The planner reads the
/// tiers it's handed and assumes no particular set exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResidencyMedium {
    Vram,
    Ram,
    Flash,
    ColdDisk,
}

/// One promotable tier: a medium and how many bytes are free on it RIGHT NOW (live, never a
/// constant — same doctrine as [`DeviceCapacity`]). Caller supplies them hottest-first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResidencyTier {
    pub medium: ResidencyMedium,
    pub free_bytes: u64,
}

/// Where each expert lives this tick across an N-tier hierarchy. `tiers[i]` holds the experts
/// promoted into the input `tiers[i]` (hottest first); `cold` holds the rest — resident only
/// on the backing store (the RAID that always holds all 896 experts), faulted up on a miss.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TieredResidencyPlan {
    pub tiers: Vec<(ResidencyMedium, Vec<ExpertId>)>,
    pub cold: Vec<ExpertId>,
}

/// Decide expert residency across an ORDERED storage hierarchy. The general pure brain that
/// [`plan_expert_residency`] is a 2-tier special case of.
///
/// **Extra tiers are OPTIONAL, never required.** The baseline is VRAM + RAM + disk and it
/// just works — a plain laptop (the common case; most users are not ML engineers with RAID
/// rigs) uses [`plan_expert_residency`] and never touches this. NVMe-flash and cold-RAID are
/// an *acceleration* for people who happen to have that hardware, offered by handing this
/// function more tiers — the system asks nobody to buy a drive. Give it whatever the machine
/// has, from one tier to four; it degrades, never demands.
///
/// Every expert already lives on the cold backing store; this promotes the highest-priority
/// ones up through the tiers, each filled by the SAME `lanes_that_fit` rule against THAT
/// tier's live free bytes. A tier that fits zero (too full / too small) is skipped and its
/// experts flow to the next tier with room; whatever exceeds every sized tier stays `cold`.
///
/// Degenerate-safe: `expert_bytes == 0` (unmeasured) promotes NOTHING — everything stays on
/// the cold backing store, because without a size we cannot honestly claim a promotion fits
/// (never a false pin). An empty profile yields empty tiers + empty cold.
pub fn plan_tiered_residency(
    profile: &ExpertActivationProfile,
    tiers: &[ResidencyTier],
    expert_bytes: u64,
    margin_bytes: u64,
) -> TieredResidencyPlan {
    let experts = ranked_experts(profile);
    let mut out: Vec<(ResidencyMedium, Vec<ExpertId>)> =
        tiers.iter().map(|t| (t.medium, Vec::new())).collect();

    if expert_bytes == 0 {
        // Can't size any promotion — leave everything on the cold backing store.
        return TieredResidencyPlan {
            tiers: out,
            cold: experts,
        };
    }

    // Per-tier capacity by the one fit rule, against each tier's own live free bytes.
    let caps: Vec<usize> = tiers
        .iter()
        .map(|t| lanes_that_fit(t.free_bytes, margin_bytes, expert_bytes) as usize)
        .collect();

    let mut cold = Vec::new();
    let mut ti = 0usize;
    for e in experts {
        // Advance past any tier that's full or fits zero — never step back (promotion order).
        while ti < out.len() && out[ti].1.len() >= caps[ti] {
            ti += 1;
        }
        if ti < out.len() {
            out[ti].1.push(e);
        } else {
            cold.push(e);
        }
    }
    TieredResidencyPlan { tiers: out, cold }
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

    // what this catches: the deep hierarchy on Joel's real box (VRAM → RAM → NVMe-flash →
    // cold-RAID) — the flash tier the 3-tier planner collapsed. The hottest experts must
    // promote fastest-first, each tier bounded by its OWN live free bytes, the rest staying on
    // the cold backing store. If tier ordering or the per-tier fit drifts, an expert lands a
    // tier too slow (flash-fault where it should be VRAM-resident) and locality degrades.
    #[test]
    fn four_tier_hierarchy_promotes_hottest_first() {
        let mut p = ExpertActivationProfile::default();
        for x in 0..10 {
            p.hits.insert(e(0, x), 1000 - x as u64); // 0 hottest … 9 coldest
        }
        // 2GB/expert, 2GB margin. VRAM 6GB→fits 2, RAM 8GB→3, Flash 10GB→4. 10 experts → 1 cold.
        let tiers = [
            ResidencyTier { medium: ResidencyMedium::Vram, free_bytes: 6 * GB },
            ResidencyTier { medium: ResidencyMedium::Ram, free_bytes: 8 * GB },
            ResidencyTier { medium: ResidencyMedium::Flash, free_bytes: 10 * GB },
        ];
        let plan = plan_tiered_residency(&p, &tiers, 2 * GB, 2 * GB);
        assert_eq!(plan.tiers[0], (ResidencyMedium::Vram, vec![e(0, 0), e(0, 1)]));
        assert_eq!(plan.tiers[1], (ResidencyMedium::Ram, vec![e(0, 2), e(0, 3), e(0, 4)]));
        assert_eq!(
            plan.tiers[2],
            (ResidencyMedium::Flash, vec![e(0, 5), e(0, 6), e(0, 7), e(0, 8)])
        );
        assert_eq!(plan.cold, vec![e(0, 9)], "the coldest expert stays on the RAID");
    }

    // what this catches: the tier vector expresses a MULTI-GPU box (Joel's 3×1080ti) with no
    // special-casing — three VRAM tiers, filled device-by-device before spilling to RAM. This
    // is the misfit-fleet payoff: the SAME planner consumes each machine's real hierarchy (one
    // 5090 or three 1080tis) just by handing it a different tier list.
    #[test]
    fn multi_gpu_fleet_box_fills_each_vram_tier() {
        let mut p = ExpertActivationProfile::default();
        for x in 0..7 {
            p.hits.insert(e(0, x), 100 - x as u64);
        }
        // Three 1080ti VRAM tiers (each fits 2) + a RAM tier (fits many). 7 experts.
        let tiers = [
            ResidencyTier { medium: ResidencyMedium::Vram, free_bytes: 6 * GB }, // gpu0: fits 2
            ResidencyTier { medium: ResidencyMedium::Vram, free_bytes: 6 * GB }, // gpu1: fits 2
            ResidencyTier { medium: ResidencyMedium::Vram, free_bytes: 6 * GB }, // gpu2: fits 2
            ResidencyTier { medium: ResidencyMedium::Ram, free_bytes: 64 * GB },
        ];
        let plan = plan_tiered_residency(&p, &tiers, 2 * GB, 2 * GB);
        assert_eq!(plan.tiers[0].1, vec![e(0, 0), e(0, 1)], "gpu0 gets the two hottest");
        assert_eq!(plan.tiers[1].1, vec![e(0, 2), e(0, 3)], "gpu1 next");
        assert_eq!(plan.tiers[2].1, vec![e(0, 4), e(0, 5)], "gpu2 next");
        assert_eq!(plan.tiers[3].1, vec![e(0, 6)], "the 7th spills to RAM");
        assert!(plan.cold.is_empty());
    }

    // what this catches: the N-tier degenerate contracts — unsized promotes NOTHING (all cold,
    // never a false pin), and a tier that fits zero (too small after margin) is SKIPPED so its
    // experts flow to the next tier with room rather than being dropped.
    #[test]
    fn unsized_promotes_nothing_and_zero_fit_tier_is_skipped() {
        let mut p = ExpertActivationProfile::default();
        for x in 0..3 {
            p.hits.insert(e(0, x), 10);
        }
        let tiers = [
            ResidencyTier { medium: ResidencyMedium::Vram, free_bytes: 6 * GB },
            ResidencyTier { medium: ResidencyMedium::Ram, free_bytes: 8 * GB },
        ];

        // Unsized → everything on the cold backing store, tiers empty.
        let no_size = plan_tiered_residency(&p, &tiers, 0, 0);
        assert!(no_size.tiers.iter().all(|(_, v)| v.is_empty()), "promote nothing unsized");
        assert_eq!(no_size.cold.len(), 3, "all on the RAID");

        // VRAM tier too small (3GB free, 2GB margin, 2GB/expert → fits 0) is skipped; RAM
        // (6GB→fits 2) takes over, the 3rd expert spills cold.
        let skip = [
            ResidencyTier { medium: ResidencyMedium::Vram, free_bytes: 3 * GB },
            ResidencyTier { medium: ResidencyMedium::Ram, free_bytes: 6 * GB },
        ];
        let plan = plan_tiered_residency(&p, &skip, 2 * GB, 2 * GB);
        assert!(plan.tiers[0].1.is_empty(), "zero-fit VRAM tier skipped");
        assert_eq!(plan.tiers[1].1, vec![e(0, 0), e(0, 1)], "RAM tier takes the hot pair");
        assert_eq!(plan.cold, vec![e(0, 2)]);
    }
}
