//! DivisionPolicy — the governor's VRAM-division RL brain (#2/#3, the second control rung).
//!
//! The pager decides WHICH experts stay resident (reward = per-token hit-rate, cheap → online
//! [`crate::decay::DecayBandit`]). This rung decides HOW TO DIVIDE the card: how much VRAM goes to the
//! resident (non-expert) weights vs the expert cache, to MAXIMIZE tok/s. The reward here — actual
//! tok/s — is EXPENSIVE to sample (a serve + warm ≈ minutes), so a naive online bandit would flail.
//! The fix is SIM-WARM-START: predict tok/s for every candidate division OFFLINE from the measured
//! coverage curve (no serve), pick from that, then a SLOW bandit refines each arm from real measured
//! tok/s. Warm-start from the model, reinforcement-optimize on the real hardware.
//!
//! Same shape as the pager: POLICY lives here (windows-clean, testable standalone); the serving_daemon
//! ACTUATES it (feeds the `--resident-only` tier manifests + live tok/s, applies the chosen
//! {resident_tier, device_budget_bytes} to the plan file). This is the reusable brain of governor #2.
//!
//! The control law is FRACTAL: pager (experts↔hit-rate) → THIS (VRAM split↔tok/s) → grid (node↔throughput).

/// One resident precision tier, read from a `--resident-only` sidecar manifest
/// (`{tier_label, resident_bytes}`). resident_bytes is the MEASURED footprint at that precision.
#[derive(Debug, Clone)]
pub struct ResidentTier {
    pub label: String,
    pub resident_bytes: u64,
}

/// The device's fixed budget the division must fit inside (all VRAM figures, bytes).
#[derive(Debug, Clone, Copy)]
pub struct HardwareBudget {
    pub vram_total_bytes: u64,
    pub kv_bytes: u64,               // KV cache reservation
    pub compute_reserve_bytes: u64,  // graph scratch + activations headroom
}

impl HardwareBudget {
    /// VRAM left for the expert cache after a given resident tier is placed. Saturating at 0
    /// (a tier whose resident + KV + reserve exceeds VRAM is infeasible → no cache).
    pub fn expert_cache_bytes(&self, resident_bytes: u64) -> u64 {
        self.vram_total_bytes
            .saturating_sub(resident_bytes)
            .saturating_sub(self.kv_bytes)
            .saturating_sub(self.compute_reserve_bytes)
    }
}

/// The per-model MoE shape that turns a cache size into a fetch cost.
#[derive(Debug, Clone, Copy)]
pub struct MoeShape {
    pub expert_bytes: u64,       // one expert record (all matrices packed), e.g. 8_093_696 for K3
    pub experts_per_token: u64,  // routed activations per decode token (≈ top_k * n_moe_layers)
}

/// Coverage(cache_slots) = fraction of a token's experts already resident, as a piecewise-linear
/// curve over MEASURED points. Monotone non-decreasing, saturating. Defaults to the K3 trace-replay
/// numbers ([[k3-coverage-vs-vram-curve]]); a model supplies its own points as it's measured.
#[derive(Debug, Clone)]
pub struct CoverageModel {
    points: Vec<(u64, f64)>, // (slots, coverage in [0,1]), sorted ascending by slots
}

impl CoverageModel {
    /// `points` need not be pre-sorted; (0,0) is implied. Coverage is clamped to [0,1].
    pub fn new(mut points: Vec<(u64, f64)>) -> Self {
        points.push((0, 0.0));
        points.sort_by_key(|p| p.0);
        points.dedup_by_key(|p| p.0);
        for p in &mut points {
            p.1 = p.1.clamp(0.0, 1.0);
        }
        Self { points }
    }

    /// The measured K3 bandit-residency curve (slots → coverage), from real trace replay.
    pub fn k3_measured() -> Self {
        Self::new(vec![
            (250, 0.138),
            (500, 0.235),
            (1000, 0.364),
            (1500, 0.451),
            (2000, 0.513),
            (4024, 0.657),
        ])
    }

    /// Interpolate coverage at `slots` (piecewise-linear; flat beyond the last measured point —
    /// we never extrapolate an optimistic coverage past what was measured).
    pub fn coverage(&self, slots: u64) -> f64 {
        if self.points.is_empty() {
            return 0.0;
        }
        let last = self.points.last().unwrap();
        if slots >= last.0 {
            return last.1; // saturate, don't extrapolate upward
        }
        // find the bracketing segment
        let mut prev = self.points[0];
        for &cur in &self.points[1..] {
            if slots < cur.0 {
                let span = (cur.0 - prev.0) as f64;
                let t = if span > 0.0 { (slots - prev.0) as f64 / span } else { 0.0 };
                return prev.1 + t * (cur.1 - prev.1);
            }
            prev = cur;
        }
        prev.1
    }
}

/// A candidate way to divide the card.
#[derive(Debug, Clone)]
pub struct DivisionConfig {
    pub tier_idx: usize,            // index into the tier catalog
    pub device_budget_bytes: u64,  // expert cache budget this tier frees
    pub cache_slots: u64,          // device_budget_bytes / expert_bytes
}

/// Predicted decode tok/s for a division, computed OFFLINE (no serve). This is the warm-start prior.
///
/// coverage = model(slots); a cache HIT skips the per-token H2D, a MISS pays it:
///   h2d_bytes/token = (1 - coverage) * experts_per_token * expert_bytes
///   t_token ≈ h2d_bytes / pcie_h2d_bps + compute_floor_s
///   tok/s   ≈ 1 / t_token
/// Higher coverage → less H2D → faster. `compute_floor_s` is the irreducible per-token compute
/// (matmuls + kernel launches) that residency can't remove — the ceiling the curve approaches.
pub fn predict_tok_s(
    cfg: &DivisionConfig,
    model: &CoverageModel,
    shape: &MoeShape,
    pcie_h2d_bps: f64,
    compute_floor_s: f64,
) -> f64 {
    let coverage = model.coverage(cfg.cache_slots);
    let h2d_bytes = (1.0 - coverage) * shape.experts_per_token as f64 * shape.expert_bytes as f64;
    let t_token = (h2d_bytes / pcie_h2d_bps.max(1.0)) + compute_floor_s.max(0.0);
    if t_token > 0.0 { 1.0 / t_token } else { 0.0 }
}

/// Enumerate the feasible divisions over a tier catalog: each tier that fits VRAM yields one config
/// with the cache budget its resident footprint frees. Infeasible tiers (resident > VRAM) are dropped.
pub fn feasible_divisions(
    tiers: &[ResidentTier],
    hw: &HardwareBudget,
    shape: &MoeShape,
) -> Vec<DivisionConfig> {
    let mut out = Vec::new();
    for (i, t) in tiers.iter().enumerate() {
        let budget = hw.expert_cache_bytes(t.resident_bytes);
        let slots = if shape.expert_bytes > 0 { budget / shape.expert_bytes } else { 0 };
        if budget == 0 || slots == 0 {
            continue; // this tier leaves no room for a cache — not a useful division
        }
        out.push(DivisionConfig { tier_idx: i, device_budget_bytes: budget, cache_slots: slots });
    }
    out
}

/// EMA rate for the measured-tok/s reward — how fast a division's value tracks real serves.
const REWARD_ALPHA: f64 = 0.4;

/// The slow bandit over divisions. Every arm starts with its OFFLINE predicted tok/s as a prior;
/// the arm that gets SERVED updates its value toward the MEASURED tok/s (EMA). `choose` serves the
/// best current value, so a good prediction is exploited immediately and a wrong one is corrected as
/// soon as it's measured — the expensive reward is spent only on the arm we actually run.
#[derive(Debug, Clone)]
pub struct DivisionBandit {
    configs: Vec<DivisionConfig>,
    value: Vec<f64>,     // current tok/s estimate per arm (prior, then EMA of measured)
    measured: Vec<bool>, // has this arm been served at least once?
}

impl DivisionBandit {
    /// Warm-start every arm from the offline predictor. Empty catalog → an empty bandit (`choose`
    /// returns None); the caller falls back to its current fixed division.
    pub fn warm_start(
        configs: Vec<DivisionConfig>,
        model: &CoverageModel,
        shape: &MoeShape,
        pcie_h2d_bps: f64,
        compute_floor_s: f64,
    ) -> Self {
        let value = configs
            .iter()
            .map(|c| predict_tok_s(c, model, shape, pcie_h2d_bps, compute_floor_s))
            .collect();
        let measured = vec![false; configs.len()];
        Self { configs, value, measured }
    }

    /// The division to serve now: argmax current value (predicted-or-measured). Ties → lowest index.
    pub fn choose(&self) -> Option<&DivisionConfig> {
        self.value
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| &self.configs[i])
    }

    /// Feed back the MEASURED tok/s for the tier that was served (the reward). First measurement
    /// REPLACES the prior (the real number beats the prediction outright); later ones EMA in, so a
    /// non-stationary workload keeps the estimate fresh.
    pub fn observe(&mut self, tier_idx: usize, measured_tok_s: f64) {
        if let Some(pos) = self.configs.iter().position(|c| c.tier_idx == tier_idx) {
            if self.measured[pos] {
                self.value[pos] = (1.0 - REWARD_ALPHA) * self.value[pos] + REWARD_ALPHA * measured_tok_s;
            } else {
                self.value[pos] = measured_tok_s; // prior → truth on first real serve
                self.measured[pos] = true;
            }
        }
    }

    pub fn predicted_value(&self, tier_idx: usize) -> Option<f64> {
        self.configs.iter().position(|c| c.tier_idx == tier_idx).map(|p| self.value[p])
    }
    pub fn len(&self) -> usize { self.configs.len() }
    pub fn is_empty(&self) -> bool { self.configs.is_empty() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k3_shape() -> MoeShape {
        MoeShape { expert_bytes: 8_093_696, experts_per_token: 1472 }
    }

    /// what this catches: the coverage model interpolates between measured points, saturates past the
    /// last one (never extrapolates optimistic), and is monotone. A regression here corrupts every
    /// tok/s prediction downstream.
    #[test]
    fn coverage_interpolates_saturates_and_is_monotone() {
        let m = CoverageModel::k3_measured();
        assert!((m.coverage(0) - 0.0).abs() < 1e-9);
        assert!((m.coverage(250) - 0.138).abs() < 1e-6);
        // midpoint 375 slots is between (250,.138) and (500,.235)
        let mid = m.coverage(375);
        assert!(mid > 0.138 && mid < 0.235, "interp {mid}");
        // saturates, doesn't extrapolate above the last measured coverage
        assert!((m.coverage(9999) - 0.657).abs() < 1e-6);
        // monotone non-decreasing
        let mut prev = 0.0;
        for s in (0..4500).step_by(100) {
            let c = m.coverage(s);
            assert!(c + 1e-9 >= prev, "non-monotone at {s}: {c} < {prev}");
            prev = c;
        }
    }

    /// what this catches: THE load-bearing property — more expert-cache VRAM (a smaller/cheaper
    /// resident) predicts MORE tok/s, because coverage rises and per-token H2D falls. If this inverts,
    /// the governor would "optimize" toward starving the cache. Uses the measured K3 curve + shape.
    #[test]
    fn more_cache_predicts_more_tok_s() {
        let model = CoverageModel::k3_measured();
        let shape = k3_shape();
        let (bps, floor) = (25.0e9, 0.010); // ~25 GB/s H2D, 10 ms compute floor
        let small = DivisionConfig { tier_idx: 0, device_budget_bytes: 500 * shape.expert_bytes, cache_slots: 500 };
        let big   = DivisionConfig { tier_idx: 1, device_budget_bytes: 2000 * shape.expert_bytes, cache_slots: 2000 };
        let t_small = predict_tok_s(&small, &model, &shape, bps, floor);
        let t_big   = predict_tok_s(&big, &model, &shape, bps, floor);
        assert!(t_big > t_small, "more cache must predict faster: {t_big} !> {t_small}");
    }

    /// what this catches: a tier whose resident overflows VRAM is dropped (no negative/huge budget),
    /// and a tier that fits yields slots = freed/expert_bytes.
    #[test]
    fn feasible_divisions_drop_overflow_and_size_cache() {
        let hw = HardwareBudget {
            vram_total_bytes: 32 * 1024 * 1024 * 1024,
            kv_bytes: 1 * 1024 * 1024 * 1024,
            compute_reserve_bytes: 2 * 1024 * 1024 * 1024,
        };
        let shape = k3_shape();
        let tiers = vec![
            ResidentTier { label: "q6_K".into(), resident_bytes: 33 * 1024 * 1024 * 1024 }, // overflows 32GB
            ResidentTier { label: "q4_K".into(), resident_bytes: 25 * 1024 * 1024 * 1024 }, // fits
            ResidentTier { label: "q3_K".into(), resident_bytes: 16 * 1024 * 1024 * 1024 }, // fits, more cache
        ];
        let divs = feasible_divisions(&tiers, &hw, &shape);
        assert_eq!(divs.len(), 2, "q6_K overflows and must be dropped");
        // the q3_K tier frees more VRAM than q4_K → more slots
        let q4 = divs.iter().find(|d| d.tier_idx == 1).unwrap();
        let q3 = divs.iter().find(|d| d.tier_idx == 2).unwrap();
        assert!(q3.cache_slots > q4.cache_slots);
    }

    /// what this catches: the bandit warm-starts from the predictor and then a MEASURED reward
    /// overrides the prior on first serve (real number beats prediction). Encodes the sim-warm-start
    /// + slow-refine contract that makes an expensive-reward learner viable.
    #[test]
    fn bandit_warm_starts_then_measurement_overrides_prior() {
        let hw = HardwareBudget {
            vram_total_bytes: 32 * 1024 * 1024 * 1024,
            kv_bytes: 1 * 1024 * 1024 * 1024,
            compute_reserve_bytes: 2 * 1024 * 1024 * 1024,
        };
        let shape = k3_shape();
        let model = CoverageModel::k3_measured();
        let tiers = vec![
            ResidentTier { label: "q4_K".into(), resident_bytes: 25 * 1024 * 1024 * 1024 },
            ResidentTier { label: "q3_K".into(), resident_bytes: 16 * 1024 * 1024 * 1024 },
        ];
        let divs = feasible_divisions(&tiers, &hw, &shape);
        let mut bandit = DivisionBandit::warm_start(divs, &model, &shape, 25.0e9, 0.010);
        assert_eq!(bandit.len(), 2);
        // warm-start: the bigger-cache tier (q3_K, idx 1) should predict faster and be chosen
        assert_eq!(bandit.choose().unwrap().tier_idx, 1);
        // now REALITY says q4_K (idx 0) actually served much faster (e.g. q3_K crushed quality/thrashed)
        bandit.observe(0, 5.0);
        assert_eq!(bandit.choose().unwrap().tier_idx, 0, "measured reward must override the prior");
        // a second measurement EMAs, not replaces
        let before = bandit.predicted_value(0).unwrap();
        bandit.observe(0, 1.0);
        let after = bandit.predicted_value(0).unwrap();
        assert!(after < before && after > 1.0, "EMA blend: {after} between prior 5.0 and new 1.0");
    }

    /// what this catches: the RL division bandit, fed the REAL measured V4-Flash residency curve
    /// (BigMama RTX 5090, DeepSeek-V4-Flash UD-IQ2_M, `--n-cpu-moe` sweep), learns the SATURATION KNEE
    /// from measurement — it picks the tok/s-max division (8 resident layers), NOT the max-residency
    /// one, even though the latter pins ~11 GB more VRAM. This is the empirical proof that static
    /// residency saturates: 0→8 layers buys +0.30 tok/s, 8→14 buys nothing (1.69 vs 1.68). A bandit
    /// that merely maximized residency would waste the VRAM the device cache should own — which is why
    /// "minimal static residency + max device cache" is the division the governor must converge to.
    /// Measured 2026-08-03; sweep = scratch-v4flash-sweep.sh, curve = division-curve.jsonl.
    #[test]
    fn bandit_learns_residency_saturation_from_measured_v4flash_curve() {
        // Each arm is one `--n-cpu-moe` division. This axis is STATIC layer residency (not the device
        // cache), so budget/slots are placeholders and the MEASURED reward — not the coverage prior —
        // drives the choice. Flat prior (empty coverage model → equal warm-start) makes that explicit.
        let divs = vec![
            DivisionConfig { tier_idx: 0, device_budget_bytes: 0, cache_slots: 0 }, // ncpu=48, 0 resident
            DivisionConfig { tier_idx: 1, device_budget_bytes: 0, cache_slots: 0 }, // ncpu=40, 8 resident
            DivisionConfig { tier_idx: 2, device_budget_bytes: 0, cache_slots: 0 }, // ncpu=34, 14 resident
        ];
        let flat = CoverageModel::new(vec![]);
        let shape = MoeShape { expert_bytes: 1, experts_per_token: 1 };
        let mut bandit = DivisionBandit::warm_start(divs, &flat, &shape, 25.0e9, 0.010);
        assert_eq!(bandit.len(), 3);
        // feed the REAL measured decode tok/s (BigMama 5090, this session)
        bandit.observe(0, 1.39); // 0 resident (all experts stream from NVMe)
        bandit.observe(1, 1.69); // 8 resident  — the knee
        bandit.observe(2, 1.68); // 14 resident — saturated, no gain over 8 at +11 GB VRAM
        // the bandit converges on the KNEE (idx 1), not the max-residency arm (idx 2)
        assert_eq!(bandit.choose().unwrap().tier_idx, 1, "must learn the saturation knee, not max residency");
        // strictly better than the all-stream baseline
        assert!(bandit.predicted_value(1).unwrap() > bandit.predicted_value(0).unwrap());
        // saturation: MORE residency (idx 2) is NOT better than the knee (idx 1)
        assert!(bandit.predicted_value(2).unwrap() <= bandit.predicted_value(1).unwrap());
    }
}
