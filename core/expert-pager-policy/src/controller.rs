//! The rung-2 control loop closed: observations → [`DecayBandit`] →
//! hot-routed pin list → the atomic `GGML_MOE_PLAN_FILE` writer
//! (#276 final glue; contract + division of labor locked with BigMama
//! on #general 2026-08-01).
//!
//! Roles across the seam: her C++ `ResidencyCache` (k3-adopt
//! f44ba7848) is the MECHANISM — it polls the plan file per token and
//! enforces {pins, window_k, budget}. This controller is the POLICY —
//! it folds per-token routed-expert activations into the bandit's
//! per-arm scoreboards and emits the chosen arm's top-N experts as the
//! pin list. Shared-expert (shexp) tensors are trunk-resident in the
//! fitted layout and never appear here: pins are HOT ROUTED experts
//! only, sized as a meaningful cache fraction (the corrected model
//! after her 184-guess null run).
//!
//! Identity discipline: the bandit's scoreboards key opaque `u64`s, but
//! pins must be REAL `(layer, expert)` coordinates — so observation
//! packs [`ExpertId`] losslessly (`layer << 32 | expert`) and the pin
//! list unpacks it. No hashed uids ever reach the plan file.

use std::collections::HashSet;
use std::path::Path;

use crate::decay::DecayBandit;
use crate::expert_id::ExpertId;
use crate::plan_file::{write_plan_file, PlanFileDocument, PlanPin};

fn pack(e: ExpertId) -> u64 {
    (u64::from(e.layer) << 32) | u64::from(e.expert)
}

fn unpack(v: u64) -> ExpertId {
    ExpertId {
        layer: (v >> 32) as u32,
        expert: (v & 0xFFFF_FFFF) as u32,
    }
}

/// The policy half of the plan-file seam. Owns the bandit; the caller
/// (ServingExpertPager tick, or a trace-replay bootstrap) drives it
/// one decode token at a time.
#[derive(Debug, Clone)]
pub struct BanditPlanController {
    bandit: DecayBandit,
    /// Residency budget in EXPERT SLOTS the bandit predicts against
    /// (the mechanism's cache capacity in experts, e.g. ~16k on her
    /// 40GB host cache).
    budget_slots: usize,
}

impl BanditPlanController {
    pub fn new(budget_slots: usize) -> Self {
        Self {
            bandit: DecayBandit::default(),
            budget_slots,
        }
    }

    /// Fold one decode token's routed-expert activations in. Returns
    /// the serving arm's realized hit fraction — the reward line that
    /// feeds `PagerCaptureEvent.reward` / `.hit_rate`.
    pub fn observe_token(&mut self, activated: &[ExpertId]) -> f64 {
        let packed: HashSet<u64> = activated.iter().map(|&e| pack(e)).collect();
        self.bandit.step(&packed, self.budget_slots)
    }

    /// The current hot-routed pin list: the chosen arm's top-`top_n`
    /// experts by score, in real coordinates.
    pub fn pin_list(&self, top_n: usize) -> Vec<PlanPin> {
        let mut pins: Vec<PlanPin> = self
            .bandit
            .predict(top_n)
            .into_iter()
            .map(|v| {
                let e = unpack(v);
                // Residency-only hints for now: the bandit ranks WHAT to
                // keep; precision tiers join when the controller learns a
                // rate-distortion policy over the container's ladder.
                PlanPin::residency(e.layer, e.expert)
            })
            .collect();
        // Deterministic order for the wire document (and for her logs).
        pins.sort_by_key(|p| (p.layer, p.expert));
        pins
    }

    /// Capture projections for `PagerCaptureEvent`.
    pub fn chosen_decay(&self) -> f64 {
        self.bandit.chosen_decay()
    }

    pub fn per_arm_reward(&self) -> Vec<f64> {
        self.bandit.per_arm_reward()
    }

    /// Publish the current plan atomically at `path` — the actuator
    /// write her per-token mtime poll picks up.
    pub fn write_plan(
        &self,
        path: &Path,
        budget_bytes: u64,
        window_k: u32,
        top_n: usize,
    ) -> std::io::Result<()> {
        let doc = PlanFileDocument::new(budget_bytes, window_k, self.pin_list(top_n));
        write_plan_file(path, &doc)
    }

    /// Publish the plan with the rate-distortion split (the beat-WASTE
    /// write): the hot top-N pins carry `pin_tier` (high-fidelity bank)
    /// and the entire unpinned cold tail fetches from `default_tier`
    /// (small-quant bank). Either may be `None` to leave that side at
    /// the container default — `(None, None)` degenerates to
    /// [`Self::write_plan`] exactly.
    pub fn write_tiered_plan(
        &self,
        path: &Path,
        budget_bytes: u64,
        window_k: u32,
        top_n: usize,
        pin_tier: Option<u32>,
        default_tier: Option<u32>,
    ) -> std::io::Result<()> {
        let mut pins = self.pin_list(top_n);
        if let Some(t) = pin_tier {
            for p in &mut pins {
                p.tier = Some(t);
            }
        }
        let mut doc = PlanFileDocument::new(budget_bytes, window_k, pins);
        doc.default_tier = default_tier;
        write_plan_file(path, &doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(layer: u32, expert: u32) -> ExpertId {
        ExpertId { layer, expert }
    }

    /// what this catches: identity round trip — the pack/unpack that
    /// carries real coordinates through the bandit's opaque-u64
    /// scoreboards must be lossless across the full u32 range, or the
    /// plan file pins the WRONG experts while every local test of the
    /// bandit itself stays green.
    #[test]
    fn pack_unpack_is_lossless_across_the_range() {
        for id in [
            e(0, 0),
            e(0, 255),
            e(25, 183),
            e(60, 4095),
            e(u32::MAX, u32::MAX),
        ] {
            assert_eq!(unpack(pack(id)), id);
        }
    }

    /// what this catches (#276 the datum's precondition): experts that
    /// are genuinely hot across tokens must surface in the pin list in
    /// REAL coordinates, and one-shot noise must not — this is the
    /// "trace-derived hot ROUTED set" the whole seam exists to deliver.
    /// Also pins the deterministic (layer, expert) ordering her logs
    /// diff against.
    #[test]
    fn hot_routed_experts_surface_as_ordered_pins() {
        let mut ctl = BanditPlanController::new(64);
        // 12 tokens: a stable hot set of 8 experts across 2 layers +
        // rotating one-shot noise.
        let hot: Vec<ExpertId> = (0..4)
            .map(|i| e(3, 100 + i))
            .chain((0..4).map(|i| e(7, 200 + i)))
            .collect();
        for t in 0..12u32 {
            let mut tok = hot.clone();
            tok.push(e(50, 9000 + t)); // never repeats
            let hit = ctl.observe_token(&tok);
            assert!((0.0..=1.0).contains(&hit));
        }
        let pins = ctl.pin_list(8);
        assert_eq!(pins.len(), 8);
        for h in &hot {
            assert!(
                pins.contains(&PlanPin::residency(h.layer, h.expert)),
                "hot expert {h:?} missing from pins {pins:?}"
            );
        }
        let mut sorted = pins.clone();
        sorted.sort_by_key(|p| (p.layer, p.expert));
        assert_eq!(pins, sorted, "pin order must be deterministic");
    }

    /// what this catches: the end-to-end actuator write — observe →
    /// write_plan → the ON-DISK document her consumer polls carries the
    /// hot pins, the knobs, and the frozen wire version.
    #[test]
    fn observed_hot_set_reaches_the_plan_file() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().join("moe-plan.json");
        let mut ctl = BanditPlanController::new(16);
        for _ in 0..6 {
            ctl.observe_token(&[e(1, 10), e(1, 11), e(2, 20)]);
        }
        ctl.write_plan(&path, 40 * 1024 * 1024 * 1024, 24, 3)
            .expect("plan write");
        let doc: PlanFileDocument =
            serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");
        assert_eq!(doc.version, crate::plan_file::PLAN_FILE_VERSION);
        assert_eq!(doc.budget_bytes, 40 * 1024 * 1024 * 1024);
        assert_eq!(doc.window_k, 24);
        assert_eq!(doc.pin_list.len(), 3);
        for pin in [
            PlanPin::residency(1, 10),
            PlanPin::residency(1, 11),
            PlanPin::residency(2, 20),
        ] {
            assert!(doc.pin_list.contains(&pin), "missing {pin:?}");
        }
    }

    /// what this catches (the beat-WASTE write): write_tiered_plan must
    /// stamp EVERY hot pin with `pin_tier` and carry `default_tier` for
    /// the unpinned cold tail — the on-disk rate-distortion split her
    /// consumer enacts. And the (None, None) degenerate case must stay
    /// byte-identical to write_plan (no accidental tier keys).
    #[test]
    fn tiered_write_splits_hot_fidelity_from_cold_default() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().join("moe-plan.json");
        let mut ctl = BanditPlanController::new(16);
        for _ in 0..6 {
            ctl.observe_token(&[e(1, 10), e(2, 20)]);
        }

        ctl.write_tiered_plan(&path, 1_000, 8, 2, Some(0), Some(2))
            .expect("tiered write");
        let doc: PlanFileDocument =
            serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");
        assert_eq!(doc.default_tier, Some(2), "cold tail must carry default_tier");
        assert_eq!(doc.pin_list.len(), 2);
        for p in &doc.pin_list {
            assert_eq!(p.tier, Some(0), "every hot pin must carry pin_tier: {p:?}");
        }

        // Degenerate (None, None) == plain write_plan, byte for byte.
        let tiered_off = dir.path().join("off.json");
        let plain = dir.path().join("plain.json");
        ctl.write_tiered_plan(&tiered_off, 1_000, 8, 2, None, None)
            .expect("write");
        ctl.write_plan(&plain, 1_000, 8, 2).expect("write");
        assert_eq!(
            std::fs::read(&tiered_off).expect("read"),
            std::fs::read(&plain).expect("read"),
            "(None, None) must degenerate to the plain v1 document"
        );
    }
}
