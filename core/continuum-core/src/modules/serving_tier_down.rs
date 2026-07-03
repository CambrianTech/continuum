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
