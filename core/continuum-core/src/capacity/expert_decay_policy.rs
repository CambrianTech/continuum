//! Learned-decay residency scoring + the online decay bandit — the
//! faithful port of BigMama's expert-pager RL prototypes (#276,
//! `docs/architecture/prototypes/expert-pager/` @ c687dcd13 on
//! feat/k3-expert-compaction-planner).
//!
//! The policy in one sentence: per-expert EMA activation score where the
//! decay is the recency↔frequency dial (decay→0 = pure last-token
//! recency, decay→1 = LFU-ish frequency), resident set = top-B by
//! score — and the ONLINE layer runs N decay candidates as bandit arms,
//! rewards each with its realized per-token hit-rate, and serves with
//! the current best arm, so the policy follows workload shifts a fixed
//! decay can't.
//!
//! Ported FAITHFULLY first ([[expert-pager-is-classic-control-sim-trained-ml-runs-it]]):
//! her measured numbers are properties of these exact constants and
//! reward math — offline learned decay = +5 pts held-out over pure
//! recency; online bandit 49.8% vs best-fixed 47.8% on the
//! non-stationary trace. The reproduction test below replays her
//! deterministic synthetic non-stationary workload (recency phase →
//! frequency phase) and pins the load-bearing ordering: the bandit
//! matches-or-beats the best fixed arm, which beats the worst.
//! Improvements come AFTER the port reproduces.
//!
//! This module is the policy BODY; the `TierPolicy` wiring
//! (`OnlineBanditTierPolicy` emitting `ExpertTierPlan` + the
//! `GGML_MOE_PLAN_FILE` control-file writer) composes it in the next
//! slice — mechanism (her C++ `ResidencyCache`) and policy (this) meet
//! only at that seam.

use std::collections::{HashMap, HashSet};

/// The decay candidates the bandit arbitrates over — her exact ladder.
/// 0.0 = pure last-token recency; 0.99 ≈ LFU frequency.
pub const DECAY_ARMS: [f64; 6] = [0.0, 0.3, 0.6, 0.85, 0.95, 0.99];

/// Reward-EMA rate — how fast the bandit adapts to a workload shift.
/// Her measured 49.8-vs-47.8 is at exactly this alpha.
pub const REWARD_ALPHA: f64 = 0.3;

/// Per-expert EMA activation scoreboard at one fixed decay. The unit of
/// both the offline learned-decay predictor and one bandit arm.
#[derive(Debug, Clone, Default)]
pub struct EmaScoreboard {
    decay: f64,
    score: HashMap<u64, f64>,
}

impl EmaScoreboard {
    pub fn new(decay: f64) -> Self {
        Self {
            decay,
            score: HashMap::new(),
        }
    }

    pub fn decay(&self) -> f64 {
        self.decay
    }

    /// Resident prediction = the top-`budget` experts by score. Her
    /// exact selection (select_nth on descending score).
    pub fn predict(&self, budget: usize) -> HashSet<u64> {
        let mut v: Vec<(f64, u64)> = self.score.iter().map(|(&k, &s)| (s, k)).collect();
        let b = budget.min(v.len());
        if b == 0 {
            return HashSet::new();
        }
        let idx = (b - 1).min(v.len() - 1);
        v.select_nth_unstable_by(idx, |a, c| {
            c.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal)
        });
        v[..b].iter().map(|x| x.1).collect()
    }

    /// Fold one token's activated experts in: decay everything, then +1
    /// each activation.
    pub fn observe(&mut self, token_experts: &HashSet<u64>) {
        for s in self.score.values_mut() {
            *s *= self.decay;
        }
        for &e in token_experts {
            *self.score.entry(e).or_insert(0.0) += 1.0;
        }
    }
}

/// One bandit arm: a scoreboard plus its realized-reward EMA.
#[derive(Debug, Clone)]
struct Arm {
    board: EmaScoreboard,
    reward: f64,
}

/// The online decay bandit — the ServingExpertPager's adaptation loop.
/// Each token: every arm predicts, every arm is rewarded with its
/// REALIZED hit fraction ("score everything" — the system emits the
/// reward), and the serving prediction comes from the current
/// best-reward arm.
#[derive(Debug, Clone)]
pub struct DecayBandit {
    arms: Vec<Arm>,
}

impl Default for DecayBandit {
    fn default() -> Self {
        Self::new(&DECAY_ARMS)
    }
}

impl DecayBandit {
    pub fn new(decays: &[f64]) -> Self {
        Self {
            arms: decays
                .iter()
                .map(|&d| Arm {
                    board: EmaScoreboard::new(d),
                    reward: 0.0,
                })
                .collect(),
        }
    }

    /// The arm the SYSTEM serves with right now (argmax realized
    /// reward; ties → first, her exact tie-break).
    pub fn chosen_arm(&self) -> usize {
        self.arms
            .iter()
            .enumerate()
            .max_by(|a, b| {
                a.1.reward
                    .partial_cmp(&b.1.reward)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// The active decay — `PagerCaptureEvent.chosen_decay`.
    pub fn chosen_decay(&self) -> f64 {
        self.arms[self.chosen_arm()].board.decay()
    }

    /// Per-arm reward EMAs — `PagerCaptureEvent.per_arm_reward`.
    pub fn per_arm_reward(&self) -> Vec<f64> {
        self.arms.iter().map(|a| a.reward).collect()
    }

    /// The serving prediction: the chosen arm's top-`budget` residents.
    pub fn predict(&self, budget: usize) -> HashSet<u64> {
        self.arms[self.chosen_arm()].board.predict(budget)
    }

    /// One control-loop step: score EVERY arm against the token that
    /// actually arrived (reward = realized hit fraction, EMA'd at
    /// [`REWARD_ALPHA`]), then fold the token into every scoreboard.
    /// Returns the hit fraction the SERVING arm (chosen before this
    /// token) realized — the number that feeds `PagerCaptureEvent`.
    pub fn step(&mut self, token_experts: &HashSet<u64>, budget: usize) -> f64 {
        let chosen = self.chosen_arm();
        let mut serving_hit = 0.0;
        for (i, arm) in self.arms.iter_mut().enumerate() {
            let resident = arm.board.predict(budget);
            let h = if token_experts.is_empty() {
                0.0
            } else {
                token_experts.iter().filter(|e| resident.contains(e)).count() as f64
                    / token_experts.len() as f64
            };
            arm.reward = (1.0 - REWARD_ALPHA) * arm.reward + REWARD_ALPHA * h;
            if i == chosen {
                serving_hit = h;
            }
        }
        for arm in self.arms.iter_mut() {
            arm.board.observe(token_experts);
        }
        serving_hit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Her synthetic NON-STATIONARY trace, verbatim: phase A strong
    /// recency (70% carryover walk), phase B strong frequency (fixed
    /// hot pool). Deterministic LCG — the same workload every run, so
    /// the measured property is a stable assertion, not a flake.
    fn synth_nonstationary() -> Vec<HashSet<u64>> {
        let mut rng = 0x9E3779B97F4A7C15u64;
        let mut r = || {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            rng >> 20
        };
        let per = 4000usize;
        let mut toks = Vec::new();
        let mut prev: Vec<u64> = (0..per as u64).collect();
        for _ in 0..15 {
            let mut s: HashSet<u64> = prev.iter().take(per * 7 / 10).copied().collect();
            while s.len() < per {
                s.insert(1_000_000 + (r() % 2_000_000));
            }
            prev = s.iter().copied().collect();
            toks.push(s);
        }
        let hot: Vec<u64> = (5_000_000..5_000_000 + (per as u64 * 2)).collect();
        for _ in 0..15 {
            let mut s: HashSet<u64> = HashSet::new();
            while s.len() < per * 6 / 10 {
                s.insert(hot[(r() as usize) % hot.len()]);
            }
            while s.len() < per {
                s.insert(9_000_000 + (r() % 5_000_000));
            }
            toks.push(s);
        }
        toks
    }

    /// Replays a trace through the bandit AND through each fixed arm,
    /// returning (online avg hit%, per-arm fixed avg hit%) — her
    /// prototype's `run()` reproduced against the ported types.
    fn replay(tokens: &[HashSet<u64>], budget: usize) -> (f64, Vec<f64>) {
        let mut bandit = DecayBandit::default();
        let mut fixed: Vec<EmaScoreboard> =
            DECAY_ARMS.iter().map(|&d| EmaScoreboard::new(d)).collect();
        let mut online_hit = 0.0f64;
        let mut fixed_hit = vec![0.0f64; fixed.len()];
        let mut cnt = 0usize;
        for (t, tok) in tokens.iter().enumerate() {
            if t > 0 {
                for (i, board) in fixed.iter().enumerate() {
                    let resident = board.predict(budget);
                    fixed_hit[i] +=
                        tok.iter().filter(|e| resident.contains(e)).count() as f64 / tok.len() as f64;
                }
                online_hit += bandit.step(tok, budget);
                cnt += 1;
            } else {
                bandit.step(tok, budget);
            }
            for board in fixed.iter_mut() {
                board.observe(tok);
            }
        }
        let scale = if cnt > 0 { 100.0 / cnt as f64 } else { 0.0 };
        (
            online_hit * scale,
            fixed_hit.into_iter().map(|h| h * scale).collect(),
        )
    }

    /// what this catches (#276 THE measured property, reproduced): on
    /// the non-stationary workload the ONLINE bandit must match-or-beat
    /// the best FIXED decay (her 49.8% vs 47.8%), because no single
    /// decay serves both phases — recency wins phase A, frequency wins
    /// phase B, and only the adaptive learner follows the shift. Small
    /// tolerance for the port's iteration-order neutrality; a real
    /// regression (broken reward math, wrong tie-break, decay ladder
    /// drift) craters this by whole points, not fractions.
    #[test]
    fn online_bandit_matches_or_beats_best_fixed_on_nonstationary() {
        let tokens = synth_nonstationary();
        let budget = tokens[1].len() * 3 / 2; // her budget: ~1.5 tokens resident
        let (online, per_arm) = replay(&tokens, budget);
        let best_fixed = per_arm.iter().copied().fold(0.0, f64::max);
        let worst_fixed = per_arm.iter().copied().fold(100.0, f64::min);
        assert!(
            online >= best_fixed - 0.5,
            "online {online:.1}% must match/beat best fixed {best_fixed:.1}% (arms: {per_arm:?})"
        );
        assert!(
            best_fixed > worst_fixed + 5.0,
            "the phases must genuinely separate the arms (best {best_fixed:.1}% vs worst {worst_fixed:.1}%) — \
             if they don't, the workload no longer exercises the non-stationarity the bandit exists for"
        );
    }

    /// what this catches: the scoreboard's basic contract — a strongly
    /// recurrent expert outranks one-shot noise at every decay, and the
    /// budget bounds the resident set.
    #[test]
    fn scoreboard_ranks_recurrent_experts_over_noise() {
        for &decay in &DECAY_ARMS {
            let mut board = EmaScoreboard::new(decay);
            for t in 0..10u64 {
                let mut tok: HashSet<u64> = (0..50).collect(); // hot set every token
                tok.insert(1_000 + t); // one-shot noise per token
                board.observe(&tok);
            }
            let resident = board.predict(50);
            assert_eq!(resident.len(), 50);
            let hot_hits = (0..50u64).filter(|e| resident.contains(e)).count();
            // At decay 0 (pure recency) every last-token member ties at
            // score 1.0, so the one same-token noise expert can
            // legitimately displace at most one hot member on the tie.
            // Any decay > 0 accumulates history and the hot set must
            // fill the budget outright.
            let floor = if decay == 0.0 { 49 } else { 50 };
            assert!(
                hot_hits >= floor,
                "decay {decay}: {hot_hits} of 50 hot experts resident (floor {floor})"
            );
        }
    }

    /// what this catches: the capture bindings — chosen_decay must be a
    /// member of the arm ladder and per_arm_reward must carry one EMA
    /// per arm, because these feed `PagerCaptureEvent.chosen_decay` /
    /// `.per_arm_reward` verbatim.
    #[test]
    fn capture_projections_match_the_arm_ladder() {
        let mut bandit = DecayBandit::default();
        let tok: HashSet<u64> = (0..100).collect();
        bandit.step(&tok, 64);
        bandit.step(&tok, 64);
        assert!(DECAY_ARMS.contains(&bandit.chosen_decay()));
        assert_eq!(bandit.per_arm_reward().len(), DECAY_ARMS.len());
        assert!(bandit.per_arm_reward().iter().all(|r| (0.0..=1.0).contains(r)));
    }
}
