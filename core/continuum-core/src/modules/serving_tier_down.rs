//! `TierDownPolicy` — the swappable intelligence behind serving's *shrink instead
//! of go-dark* lever (#79).
//!
//! When the resource authority asks serving for VRAM back under [`Pressure`], the
//! whole-lease answer is a full unload (serving goes dark until a re-load). But
//! serving has a gentler lever: re-home to a SMALLER base model — keep answering,
//! just with less residency. Which smaller model (if any) is a *decision*, and
//! this is the seam that owns it.
//!
//! # Mechanism, not policy
//!
//! Per the substrate doctrine, the consumer must never bake in a downgrade ladder
//! ("30B → 14B → 7B"). Target SELECTION is a [`TierDownPolicy`] the daemon
//! injects. The same trait is implemented by:
//!
//! - a hardcoded/heuristic ladder (largest smaller model that fits the reduced
//!   budget),
//! - a persona-intelligence policy (the being decides how much of itself to shed
//!   under load),
//! - an ML classifier / RL policy (learned from which tier-downs preserved the
//!   most capability per byte freed).
//!
//! The [`ServingConsumer`](super::serving_consumer::ServingConsumer) that carries
//! the decision out is identical across all of them — it orchestrates the honest
//! async re-home handshake, the policy just chooses.
//!
//! # Why the policy sizes its own target
//!
//! [`TierDown::resident_after`] is the total resident bytes serving will hold once
//! the swap lands (the smaller model's weights + its KV). The policy returns it
//! because sizing a candidate is footprint knowledge the policy already needs to
//! rank options — and the daemon is the one footprint authority a real policy is
//! built against. The consumer only asserts the decision is a genuine *shrink*
//! (`resident_after < current`) before carrying it out; it never invents a target.
//!
//! [`Pressure`]: crate::resources::ReclaimReason::Pressure

use crate::resources::ReclaimRequest;

/// The situation a [`TierDownPolicy`] judges: serving is under a reclaim ask it
/// could answer by shrinking to a smaller base instead of fully unloading.
pub struct TierDownContext<'a> {
    /// The model serving is running right now.
    pub active_model: &'a str,
    /// Its current TOTAL resident bytes (weights + per-lane KV) — the same number
    /// serving reports as its footprint.
    pub current_bytes: u64,
    /// The live serving shape: per-slot served window + lane count. A policy that
    /// tier-downs by shrinking the window/lanes rather than the base uses these.
    pub served_window: u32,
    pub lanes: u32,
    /// The ask driving this — bytes wanted, deadline, reason. A policy may size
    /// its target to just clear `target_bytes` rather than shed maximally.
    pub request: &'a ReclaimRequest,
}

/// A tier-down proposal: re-home serving to a smaller model to free some VRAM
/// without going dark.
pub struct TierDown {
    /// The smaller model to re-home to. Must be a real servable id; the consumer
    /// pins it and the daemon's reconcile swaps to it (the re-home seam, #105).
    pub target_model: String,
    /// Total resident bytes serving will hold AFTER the swap lands. Freed delta =
    /// `ctx.current_bytes − resident_after`; the consumer rejects a proposal that
    /// is not a genuine shrink.
    pub resident_after: u64,
}

/// Swappable intelligence: given the pressure situation, decide whether to tier
/// down (and to what) or decline. Object-safe so the consumer holds
/// `Arc<dyn TierDownPolicy>` and any implementation drops in without touching the
/// handshake.
pub trait TierDownPolicy: Send + Sync {
    /// Choose a tier-down target, or `None` to decline — in which case the
    /// consumer falls through to a full unload (the honest whole-lease lever).
    fn choose(&self, ctx: &TierDownContext) -> Option<TierDown>;
}

/// The default the daemon wires until a real selection intelligence is authored:
/// always decline. This is not a silent fallback — it is the honest current
/// capability ("no tier-down policy is installed, so the only lever is a full
/// unload"). Swapping in a `CatalogTierDownPolicy` / `PersonaTierDownPolicy` /
/// `MlTierDownPolicy` is a one-line wiring change in `register_as_consumer`, with
/// zero change to the consumer's handshake.
pub struct DeclineTierDown;

impl TierDownPolicy for DeclineTierDown {
    fn choose(&self, _ctx: &TierDownContext) -> Option<TierDown> {
        None
    }
}

/// One servable model as the tier-down ranker sees it, at the CURRENT serving shape
/// (window × lanes): its id, how capable it is, and the total bytes it would be
/// resident at if serving re-homed to it. Decoupled from `serving_plan::ModelFootprint`
/// so this policy module owns no footprint math — the daemon computes `resident_bytes`
/// with its one footprint authority and hands the ranker the finished numbers.
#[derive(Debug, Clone)]
pub struct TierCandidate {
    pub model_id: String,
    /// Higher = more capable. The ranker keeps the MOST capable model that still frees
    /// enough — never sheds more capability than the pressure demands.
    pub capability_rank: u8,
    /// Total resident bytes (weights + per-lane KV × lanes) at the ctx's window/lanes.
    pub resident_bytes: u64,
}

/// Provider of the live servable-model candidates AT a given serving shape. The daemon
/// wires this from its `live_candidates()` (suppress/pin/eligibility already applied) ×
/// `ModelFootprint::resident_bytes(window, lanes)`, so the ranker sees exactly the models
/// the autonomic plan could serve, sized to what serving is running right now.
pub type TierCandidatesFn = std::sync::Arc<dyn Fn(u32, u32) -> Vec<TierCandidate> + Send + Sync>;

/// The first real tier-down intelligence (outlier A — a heuristic ladder, the
/// substrate-doctrine "mechanism, not policy" note above). Under a VRAM reclaim ask
/// (a game grabbed the GPU, a peer needs the bytes), instead of declining → full unload
/// (serving goes dark), re-home to the MOST CAPABLE smaller model that still frees enough
/// to clear the ask. "Use the memory that's available; when it's taken, take our own
/// capacity down to yield — but keep answering, just smaller." When pressure clears, the
/// autonomic plan grows back up on its own (it always picks the most capable model that
/// fits the now-larger budget). Declines (→ full unload, the honest max lever) only when
/// no smaller model frees enough. A later `PersonaTierDownPolicy` / `MlTierDownPolicy`
/// drops in here unchanged.
pub struct CatalogTierDownPolicy {
    candidates: TierCandidatesFn,
}

impl CatalogTierDownPolicy {
    pub fn new(candidates: TierCandidatesFn) -> Self {
        Self { candidates }
    }
}

impl TierDownPolicy for CatalogTierDownPolicy {
    fn choose(&self, ctx: &TierDownContext) -> Option<TierDown> {
        // The most bytes we may still hold after the swap and STILL satisfy the ask:
        // free `target_bytes`, i.e. land at or below `current − target`. Saturating so a
        // request for more than we hold demands we shed everything (→ nothing qualifies →
        // decline → full unload, correct: no smaller model frees THAT much).
        let max_resident_after = ctx.current_bytes.saturating_sub(ctx.request.target_bytes);
        let lanes = ctx.lanes.max(1);
        (self.candidates)(ctx.served_window, lanes)
            .into_iter()
            .filter(|c| c.model_id != ctx.active_model) // re-home to a DIFFERENT model
            .filter(|c| c.resident_bytes < ctx.current_bytes) // a genuine shrink
            .filter(|c| c.resident_bytes <= max_resident_after) // frees enough to clear the ask
            // Keep the MOST capable qualifying model; tie-break toward the one using the
            // most of the freed budget (most capability per byte kept).
            .max_by(|a, b| {
                a.capability_rank
                    .cmp(&b.capability_rank)
                    .then(a.resident_bytes.cmp(&b.resident_bytes))
            })
            .map(|c| TierDown {
                target_model: c.model_id,
                resident_after: c.resident_bytes,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::{ReclaimReason, ReclaimRequest, ResourceKind};

    fn ctx_asking<'a>(
        active: &'a str,
        current_bytes: u64,
        req: &'a ReclaimRequest,
    ) -> TierDownContext<'a> {
        TierDownContext {
            active_model: active,
            current_bytes,
            served_window: 8192,
            lanes: 2,
            request: req,
        }
    }

    fn req(target_bytes: u64) -> ReclaimRequest {
        ReclaimRequest {
            kind: ResourceKind::Vram,
            target_bytes,
            deadline_ms: 0,
            reason: ReclaimReason::Pressure,
        }
    }

    fn cands(list: &[(&str, u8, u64)]) -> TierCandidatesFn {
        let v: Vec<TierCandidate> = list
            .iter()
            .map(|(id, rank, bytes)| TierCandidate {
                model_id: id.to_string(),
                capability_rank: *rank,
                resident_bytes: *bytes,
            })
            .collect();
        std::sync::Arc::new(move |_w, _l| v.clone())
    }

    // what this catches: the core tier-down choice — under a reclaim ask, re-home to the
    // MOST CAPABLE smaller model that still frees enough, never shedding more than needed.
    #[test]
    fn picks_the_most_capable_model_that_frees_enough() {
        // Running a 24GB model; a game wants 8GB back. Land at ≤ 16GB.
        let policy = CatalogTierDownPolicy::new(cands(&[
            ("big-30b", 9, 24 * GB),   // the current model (excluded)
            ("mid-14b", 6, 15 * GB),   // fits (≤16), most capable qualifier → WINNER
            ("small-7b", 4, 8 * GB),   // fits but less capable
            ("tiny-3b", 2, 4 * GB),    // fits but least capable
        ]));
        let r = req(8 * GB);
        let td = policy
            .choose(&ctx_asking("big-30b", 24 * GB, &r))
            .expect("a smaller model frees enough");
        assert_eq!(td.target_model, "mid-14b", "most-capable model that clears the ask");
        assert_eq!(td.resident_after, 15 * GB);
    }

    // what this catches: a harder ask than any smaller model can satisfy → decline, so the
    // consumer falls through to a full unload (the honest max lever), never a fake shrink.
    #[test]
    fn declines_when_no_smaller_model_frees_enough() {
        let policy = CatalogTierDownPolicy::new(cands(&[
            ("big-30b", 9, 24 * GB),
            ("mid-14b", 6, 20 * GB), // only frees 4GB — not enough
        ]));
        // Need 8GB back → must land ≤ 16GB; nothing qualifies.
        let r = req(8 * GB);
        assert!(policy.choose(&ctx_asking("big-30b", 24 * GB, &r)).is_none());
    }

    // what this catches: never "shrink" to something at or above the current residency
    // (that frees nothing) even if it's a different model.
    #[test]
    fn never_re_homes_to_a_non_shrink() {
        let policy = CatalogTierDownPolicy::new(cands(&[
            ("big-30b", 9, 24 * GB),
            ("other-big", 8, 26 * GB), // bigger, not a shrink
        ]));
        assert!(policy.choose(&ctx_asking("big-30b", 24 * GB, &req(1 * GB))).is_none());
    }

    const GB: u64 = 1024 * 1024 * 1024;
}
