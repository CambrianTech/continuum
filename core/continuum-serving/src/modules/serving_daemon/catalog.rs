//! Catalog ownership for the serving daemon.

use super::*;

/// The servable candidates from the LIVE model universe — every model that is
/// [`Availability::Ready`] (its artifact is on disk, or it was just pulled and
/// flipped Ready with its real path) and yields a footprint. Planning off the
/// snapshot (not the immutable seed) is what makes a runtime acquisition
/// servable without a reboot: `models/pull` writes the path + Ready into this
/// same snapshot, and the next tick's `candidates_from_snapshot` includes it.
/// A `NotDownloaded` model is correctly excluded — we only offer what we can
/// actually serve right now. The footprint resolves through the live model's
/// `gguf_local_path` (which `resolve_gguf` prefers when present), so the bytes
/// counted are the bytes that will be loaded.
/// The servable candidate set for THIS host right now: on-disk models, minus the
/// operator-suppressed set, minus the persona-ineligible benchmark/opponent rows (a
/// PIN bypasses eligibility — pinning IS operator consent, #142). The ONE definition of
/// "what the autonomic plan may serve," shared by [`ServingDaemonModule::live_candidates`]
/// (the plan) and the tier-down ranker (#56, so a shrink can only re-home to a model the
/// plan would itself have picked — never a divergent second catalog).
/// Why a model on disk is not a candidate to host minds. Stated, never implied: a node
/// that says "no servable model" while its weights sit there sends whoever is reading it
/// hunting for a missing file (2026-09-08: the 5090 hosted zero citizens for an hour, and
/// three of us spent it eliminating the wrong causes, because `candidates: 0` named none
/// of the models it had refused or why).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusedCandidate {
    pub model_id: String,
    pub reason: &'static str,
}

pub const REFUSED_SUPPRESSED: &str =
    "suppressed by an unload this boot — serving/load re-permits it";
pub const REFUSED_NOT_PINNED: &str = "another model is pinned — serving/unpin frees the choice";
pub const REFUSED_INELIGIBLE: &str =
    "not a base model for citizens (persona_serving_eligible = false)";

/// The candidates, and everything that was refused with its reason. Pure over the
/// snapshot so the accounting is tested rather than restated at the probe site.
pub(super) fn servable_candidates_with_refusals(
    snapshot: &CatalogSnapshot,
    suppressed: &HashSet<String>,
    pinned: &Option<String>,
) -> (Vec<ModelFootprint>, Vec<RefusedCandidate>) {
    let ineligible: HashSet<String> = snapshot
        .models
        .values()
        .filter(|live| !live.model.persona_serving_eligible)
        .map(|live| live.model.id.clone())
        .collect();
    let mut kept = Vec::new();
    let mut refused = Vec::new();
    for c in candidates_from_snapshot(snapshot) {
        let reason = if suppressed.contains(&c.model_id) {
            Some(REFUSED_SUPPRESSED)
        } else {
            match pinned.as_ref() {
                Some(pin) if pin != &c.model_id => Some(REFUSED_NOT_PINNED),
                None if ineligible.contains(&c.model_id) => Some(REFUSED_INELIGIBLE),
                _ => None,
            }
        };
        match reason {
            Some(reason) => refused.push(RefusedCandidate {
                model_id: c.model_id.clone(),
                reason,
            }),
            None => kept.push(c),
        }
    }
    (kept, refused)
}

pub(super) fn servable_candidates(
    snapshot: &CatalogSnapshot,
    suppressed: &HashSet<String>,
    pinned: &Option<String>,
) -> Vec<ModelFootprint> {
    let (kept, refused) = servable_candidates_with_refusals(snapshot, suppressed, pinned);
    // Say WHY there is nothing to serve, at the only moment anyone cares: when the answer
    // is none. A refusal list of one line per model beats an hour of elimination.
    if kept.is_empty() && !refused.is_empty() {
        for r in &refused {
            continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.plan.candidate_refused",
                model_id = %r.model_id,
                reason = r.reason,
                // NAME THE CONSTRAINT, not just its category. `REFUSED_NOT_PINNED` says
                // "another model is pinned" and never says WHICH — so a node dark behind
                // a bad pin emitted three refusals a second, none of which identified the
                // thing doing the refusing, and finding it took a separate grep of a
                // different log for `serving.pin.restored` (#3955). A rejection reason
                // that cannot name what blocked you turns a read into archaeology.
                pinned = pinned.as_deref().unwrap_or("(none)"), // unwrap_or: no pin is a real state, and "(none)" says so rather than eliding the field
                "a model on disk was refused as a base for citizens — this is why the node has no candidate"
            );
        }
    }
    kept
}

/// Only a model that can SPEAK is a base-model candidate. An embedding or reranking
/// model is a real, Ready row in the catalog with a real GGUF, and the planner picked
/// one as the base on the 5090 after a reboot (BigMama, 2026-09-06 — card af635057):
/// nothing served, ping read ready-less, and no line said why. Pinning bypasses
/// eligibility (operator consent) but never this: a mind cannot run on a model with
/// no text generation.
pub fn can_serve_minds(model: &Model) -> bool {
    use crate::model_registry::types::Capability;
    model.capabilities.contains(&Capability::TextGeneration)
        || model.capabilities.contains(&Capability::Chat)
}

/// True the first time a mute catalog row is seen by this process.
pub(super) fn first_time_named_mute(model_id: &str) -> bool {
    static NAMED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    let set = NAMED.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()));
    let mut guard = set.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or: a poisoned name set still answers; the set is bookkeeping, never truth
    guard.insert(model_id.to_string())
}

pub fn candidates_from_snapshot(snapshot: &CatalogSnapshot) -> Vec<ModelFootprint> {
    snapshot
        .models
        .values()
        .filter(|live| live.status.availability == Availability::Ready)
        .filter(|live| {
            let ok = can_serve_minds(&live.model);
            if !ok && first_time_named_mute(&live.model.id) {
                // Once per model per process: the plan recomputes every few seconds
                // and this fired per pass per mute row (228 rows / 12 min on the M5,
                // 2026-09-07) — a fact about a catalog row, not about the pass.
                continuum_base::probe!(
                    target: "continuum_core::modules::serving_daemon", class = "serving.plan.candidate_cannot_speak",
                    model_id = %live.model.id,
                    "a Ready model with no text generation is not a base-model candidate (named once)"
                );
            }
            ok
        })
        .filter_map(|live| footprint_for(&live.model))
        .collect()
}

/// Classify the detected GPU into the persona spawner's tier inputs — the
/// `n_gpu_layers` + roster decision. Retires the old hardcoded `CpuOnly +
/// Compat` (which clamped an M5 Pro to the LCD 0.5B on CPU). Conservative:
/// hardware we don't positively recognize falls back to `Compat`/`CpuOnly`
/// (the safe path), so the clamp only lifts when we KNOW the silicon.
/// `tier_category` is load-bearing (drives `n_gpu_layers`); `HwCapabilityTier`
/// is currently informational, so coarse generation mapping suffices.
pub fn detect_tier(gpu_name: &str) -> (HwCapabilityTier, HwTierCategory, &'static str) {
    let g = gpu_name.to_lowercase();
    if g.contains("apple") {
        let cap = if g.contains("m5") {
            HwCapabilityTier::M5UmaProMax
        } else if g.contains("m4") {
            HwCapabilityTier::M4UmaProMax
        } else if g.contains("m3") {
            HwCapabilityTier::M3UmaProMax
        } else if g.contains("m2") {
            HwCapabilityTier::M2UmaProMax
        } else {
            HwCapabilityTier::M1Uma16Gb
        };
        return if g.contains("pro") || g.contains("max") || g.contains("ultra") {
            (cap, HwTierCategory::MSeriesPro, "apple-mseries-pro")
        } else {
            (cap, HwTierCategory::MSeries, "apple-mseries")
        };
    }
    if g.contains("nvidia") || g.contains("geforce") || g.contains("rtx") || g.contains("cuda") {
        return (HwCapabilityTier::Sm89, HwTierCategory::Cuda, "nvidia-cuda");
    }
    // Intel-Mac discrete Metal is a known garbled-token path; unknown/CPU
    // hardware stays on the safe LCD/CPU tier.
    (HwCapabilityTier::CpuOnly, HwTierCategory::Compat, "compat")
}

impl ServingDaemonModule {
    /// Test seam: override how planned model ids resolve to [`Model`] structs,
    /// so the reconcile wiring runs without a populated global registry.
    #[cfg(test)]
    pub(super) fn set_model_resolver(&mut self, resolver: ModelResolver) {
        self.model_resolver = resolver;
    }

    /// Test seam: own the inherited-lane evidence instead of reading the operator's real
    /// pidfile + registry. See [`InheritedLaneSource`] for why this MUST be injected — the
    /// production default reads a filesystem singleton, and the identical leak was already
    /// caught by measurement in `serving_consumer`.
    #[cfg(test)]
    pub(super) fn set_inherited_lane(&mut self, source: InheritedLaneSource) {
        self.inherited_lane = source;
    }

    /// The servable candidates RIGHT NOW: the on-disk Ready models, MINUS any an
    /// operator has explicitly unloaded (`serving/unload`), and — when a force-pin
    /// is set (`serving/pin`) — INTERSECTED to just the pinned model. The single
    /// chokepoint every plan flows through, so a suppressed model can never be
    /// planned and a pin can never be escaped: pin set → the planner has exactly
    /// one candidate, so the reconcile serves that model or (if it has dropped off
    /// disk) nothing. Suppress subtracts; pin intersects; the planner still owns
    /// the choice among whatever remains.
    /// What to tell a human when nothing is serving. "No local weights" is a LIE when the
    /// weights are there and every candidate was refused for a nameable reason.
    pub(super) fn no_candidate_reason(&self) -> String {
        let intent = self.intent.snapshot();
        let (kept, refused) = servable_candidates_with_refusals(
            &self.catalog.snapshot(),
            &intent.suppressed,
            &intent.pinned,
        );
        if !kept.is_empty() {
            return "a candidate exists but no plan was produced — read serving.plan".to_string();
        }
        if refused.is_empty() {
            return "no servable model on disk — the planner found no local weights to \
                    bring up (pull one with `continuum models/pull`)"
                .to_string();
        }
        let named = refused
            .iter()
            .map(|r| format!("{} ({})", r.model_id, r.reason))
            .collect::<Vec<_>>()
            .join("; ");
        format!(
            "no candidate to host citizens — {} model(s) on disk were refused: {named}",
            refused.len()
        )
    }

    pub(super) fn live_candidates(&self) -> Vec<ModelFootprint> {
        let intent = self.intent.snapshot();
        servable_candidates(&self.catalog.snapshot(), &intent.suppressed, &intent.pinned)
    }

    /// The detected hardware tier for this host, for the persona spawner's
    /// `n_gpu_layers` + roster shape. Same GPU detection that feeds the serving
    /// plan — one hardware authority, not two.
    pub fn detected_tier(&self) -> (HwCapabilityTier, HwTierCategory, &'static str) {
        detect_tier(self.gpu.gpu_name())
    }
}
