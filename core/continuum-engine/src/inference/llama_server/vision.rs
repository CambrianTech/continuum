//! Vision responsibility of the llama-server process owner.
use super::*;
/// Pure readiness verdict for the VISION side of a lane (#106): given what the
/// catalog row declares, whether an mmproj projector resolved at spawn, and what
/// the running server's `/props` reports, decide whether this lane may claim
/// sight. Split from the daemon IO so every branch is unit-testable.
///
/// - `Ok(false)` — a text lane: the row never declared Vision. Not an error.
/// - `Ok(true)`  — declared Vision, `--mmproj` was passed, AND the server's own
///   `/props` confirms `modalities.vision`. The lane may claim sight.
/// - `Err(why)`  — the row declares Vision but the lane CANNOT verifiably see:
///   no projector resolved, the server reports no vision modality (projector
///   failed to load / wrong file), or the server build doesn't report
///   modalities at all (unverifiable ≠ working). The daemon logs the reason
///   loud and publishes `vision_ready: false` — the capability lie is surfaced,
///   never served ([[fallbacks-are-illegal-fail-loud]]).
/// Where a lane's resolved projector goes. ONE decision, pure, so the spawn site
/// and the test agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MainLaneMmproj {
    /// The row opted in (`mmproj_on_main_lane`) or this IS the sidecar lane: wear it.
    Wear,
    /// The row prefers a sidecar and one is available on this node: withhold, keep
    /// `cache_reuse`, sight routes through the sidecar + description bridge.
    Withhold,
    /// The row prefers a sidecar but this node has no other VL candidate: wear it
    /// anyway — the only eyes on the node beat `cache_reuse`.
    WearAsOnlyEyes,
    /// The row prefers a sidecar, but this model's memory cannot shift (recurrent
    /// layers), so `cache_reuse` never runs on it and withholding would buy nothing:
    /// wear it. Prefix reuse and context checkpoints are unaffected by the projector.
    WearReuseInert,
    /// Nothing resolved; nothing to decide.
    NoneResolved,
}


/// The sidecar + description bridge are a PROSTHESIS for text-only minds (Joel,
/// 2026-09-18: "be smart about models even needing the feature if all loaded have
/// it naturally"). A model that sees natively wears its own projector when that is
/// the only sight on the node; a node whose loaded models all see natively needs no
/// sidecar at all — the daemon drops any sidecar the moment the main lane verifies
/// sight (`main_sees`). The withhold exists only to preserve `cache_reuse` when a
/// SEPARATE VL lane can carry sight; it is never a reason for a node to be blind.
///
/// `sidecar_available` is `None` when the caller did not consult the sidecar search
/// because the answer does not depend on it (row opted in, this is the sidecar,
/// or nothing resolved).
pub(super) fn main_lane_mmproj_decision(
    row_opts_in: bool,
    is_sidecar_lane: bool,
    mmproj_resolved: bool,
    reuse_inert: bool,
    sidecar_available: Option<bool>,
) -> MainLaneMmproj {
    if !mmproj_resolved {
        return MainLaneMmproj::NoneResolved;
    }
    if row_opts_in || is_sidecar_lane {
        return MainLaneMmproj::Wear;
    }
    if reuse_inert {
        return MainLaneMmproj::WearReuseInert;
    }
    match sidecar_available {
        Some(true) => MainLaneMmproj::Withhold,
        // unwrap-free by construction: `None` here means "not consulted" on a row
        // that prefers a sidecar — the caller always consults in that case, and if
        // it ever did not, blindness is the wrong default, so the lane wears them.
        Some(false) | None => MainLaneMmproj::WearAsOnlyEyes,
    }
}


/// The live decision for a model row on THIS node: consults the registry + the
/// sidecar search only when the row prefers a sidecar and a projector resolved
/// (the two cases where the answer depends on what else is on the box). ONE call
/// site shape for the spawn, the grow-check and the serving daemon, so the intent
/// they compare against is the same intent.
pub(super) fn main_lane_mmproj_decision_for(
    model: &crate::model_registry::types::Model,
    is_sidecar_lane: bool,
) -> MainLaneMmproj {
    let mmproj_resolved =
        crate::model_registry::artifacts::resolve_mmproj_for_model(model).is_some();
    let decides = !model.serving.mmproj_on_main_lane && !is_sidecar_lane && mmproj_resolved;
    // Consulted only where the answer depends on it, like the sidecar search below.
    let reuse_inert = decides && memory_cannot_shift(model);
    let sidecar_available = if !decides || reuse_inert {
        None // not consulted: the decision does not depend on it
    } else {
        let rows: Vec<crate::model_registry::types::Model> = crate::model_registry::try_global()
            .map(|r| r.models().cloned().collect())
            .unwrap_or_default();
        // Availability only (does ANY sidecar row exist), not admission: the pin is judged
        // where the sidecar is launched, by the daemon that holds the operator's pins.
        Some(crate::inference::vision_sidecar::find_candidate(&rows, Some(&model.id), &[]).is_ok())
    };
    main_lane_mmproj_decision(
        model.serving.mmproj_on_main_lane,
        is_sidecar_lane,
        mmproj_resolved,
        reuse_inert,
        sidecar_available,
    )
}

/// Does this model's own header declare recurrent state? Memoized per artifact path:
/// the grow-check asks every tick, the header answers once. An unreadable header is
/// `false` — the decision then stands on the sidecar search exactly as before.
fn memory_cannot_shift(model: &crate::model_registry::types::Model) -> bool {
    static CACHE: std::sync::OnceLock<dashmap::DashMap<std::path::PathBuf, bool>> =
        std::sync::OnceLock::new();
    let Some(path) = crate::model_registry::artifacts::resolve_gguf_for_model(model) else {
        return false;
    };
    let cache = CACHE.get_or_init(dashmap::DashMap::new);
    if let Some(hit) = cache.get(&path) {
        return *hit;
    }
    let recurrent = (|| {
        let mut file = std::fs::File::open(&path).ok()?;
        let content = candle_core::quantized::gguf_file::Content::read(&mut file).ok()?;
        let arch = crate::inference_capability::gguf_keys::architecture(&content)?;
        Some(crate::inference_capability::gguf_keys::holds_recurrent_state(&content, &arch))
    })()
    .is_some_and(|r| r);
    cache.insert(path, recurrent);
    recurrent
}

impl MainLaneMmproj {
    /// Does this decision put a projector on the lane?
    pub(super) fn wears(self) -> bool {
        matches!(self, Self::Wear | Self::WearAsOnlyEyes | Self::WearReuseInert)
    }
}


pub fn vision_lane_ready(
    declares_vision: bool,
    mmproj_on_lane: bool,
    props: Option<MultimodalSupport>,
) -> Result<bool, String> {
    if !declares_vision {
        return Ok(false);
    }
    // `mmproj_on_lane` is whether the projector was PASSED TO THIS LANE — not whether
    // one exists on disk. A projector withheld for a sidecar is not a projector that
    // failed to load; conflating the two produced "--mmproj was passed but…" about a
    // lane that was never given one (2026-09-18).
    if !mmproj_on_lane {
        return Err(
            "model row declares Vision but this lane carries no mmproj projector — none \
             resolved at spawn (pull the model's `*-GGUF` repo, projector ships alongside, \
             or set `mmproj_local_path`), or it was withheld for a vision sidecar; the \
             lane serves TEXT ONLY"
                .to_string(),
        );
    }
    match props {
        Some(m) if m.vision => Ok(true),
        Some(_) => Err(
            "--mmproj was passed but the running server's /props reports \
             modalities.vision=false — the projector failed to load or is the wrong \
             modality (an audio-only mmproj?); the lane cannot see"
                .to_string(),
        ),
        None => Err(
            "--mmproj was passed but the running server's /props carries no `modalities` \
             block — this llama-server build cannot VERIFY multimodal readiness, and an \
             unverified claim of sight is a fabrication; upgrade the serving binary"
                .to_string(),
        ),
    }
}
