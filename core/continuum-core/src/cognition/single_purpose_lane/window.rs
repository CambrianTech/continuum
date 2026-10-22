//! The window a single-purpose lane is launched at: the model's own trained window, capped by
//! what the governed host budget holds, quantized to a stable rung.

/// Host-fit PHYSICAL launch window (`-c`) for the EPHEMERAL measurement lane —
/// derived from the LIVE serving budget through the SAME [`plan_serving`] classifier
/// the autonomic serving daemon uses, NEVER a baked constant (task #124; the old
/// `EVAL_LANE_CONTEXT: u32 = 16_384` was the exact "clamp a 128k window to a magic
/// number → the model is starved" anti-pattern [[no-hardcoded-context-numbers-derive-from-the-live-window]]).
///
/// The fork's COGNITION window is still read back from the lane's real `/props` after
/// spawn ([`EphemeralServingLane::served_context_window`]) — the SAME served-truth pin
/// the supervisor applies to the living persona, so a measurement copy budgets against
/// exactly what its lane serves ([[dreaming-mind-eval-must-match-live-cognition]], task
/// #50). This value only sizes the launch `-c` + KV allocation. Because it now comes
/// from `plan_serving` against live FREE VRAM (already net of the resident living lane,
/// the same figure [`decide_lane_placement`] reads), the throwaway lane's geometry
/// mirrors what production would serve THIS base on THIS host RIGHT NOW: roomy host →
/// big window up to the model's trained ceiling; contended host → honestly smaller;
/// always floored at [`MIN_SERVE_CTX`](crate::inference::planning::MIN_SERVE_CTX)
/// and capped at the base's own trained window.
///
/// Degrades honestly (never a fresh magic cap): no GPU monitor on the node (CPU-only
/// host) or a base whose GGUF can't be sized → fall back to the model's own trained
/// window. Short coder-eval prompts dominate, so an over-large `-c` on a CPU host costs
/// KV allocation but never starves cognition; the point is to stop clamping a capable
/// GPU host's window to a constant below what it could serve. Remaining edge: when
/// placement spills this lane to CPU, the window is still sized off the GPU free-VRAM
/// budget (under-sizes for the rare CPU-spilled eval) — acceptable for short prompts;
/// the two-phase device-aware sizing rides with #56's `ResourceGovernor`.
/// The eval lane's `-c` must be BOTH reproducible and honest to the model. Two failures bracket
/// this seam and the fix has to clear both:
///
/// 1. Sizing off LIVE free VRAM made `-c` swing run-to-run (placement captures 2872 ↔ 99371).
///    A different `n_ctx` changes llama.cpp's KV/tensor shapes enough that even GREEDY argmax
///    flips on borderline tokens, so the SAME task passed one run and failed the next and the
///    ABSOLUTE score meant nothing (glass-boxed 2026-07-21).
/// 2. The "fix" for (1) was `const EXAM_LANE_CTX: u32 = 16384` — which re-introduced the exact
///    anti-pattern task #124 had already deleted from this file (`EVAL_LANE_CONTEXT: u32 =
///    16_384`, see the doc block above, still describing the removal). It scored a 1M-context
///    MoE at 16k. That is not a measurement of the model; it is a measurement of the constant,
///    and it deletes the reason to run MoE at all (Joel, 2026-08-03: "these 4k or smaller
///    windows turn 1M context models into stupid numbers... eliminating all reason to have an
///    MoE"). [[never-hardcode-a-context-window-4k-defaults-destroy-the-moe-thesis]]
///
/// The resolution: the instability in (1) came from the SOURCE being live free VRAM, not from
/// deriving at all. So derive from the model's own trained window, capped by what the GOVERNED
/// host budget can actually hold — and quantize a capped result down to a power-of-two rung so
/// ordinary budget flutter cannot move it. On a host that can hold the model's full trained
/// window the exam runs at that window EXACTLY (deterministic, and as large as the model
/// deserves — 1M stays 1M). On a host that cannot, it runs at a stable rung below the fit,
/// identical across runs on that machine. No invented ceiling at any point.
///
/// Degrades honestly: ungoverned host, or a base whose GGUF can't be sized ⇒ the model's own
/// trained window (never a fresh magic cap). Fit is still decided downstream by
/// `decide_lane_placement`; if the window won't fit, the lane spawn fails and the caller
/// degrades to a co-tenant SHARE rather than OOM.
pub(crate) fn plan_lane_ctx(base: &crate::model_registry::Model) -> u32 {
    use crate::inference::planning::MIN_SERVE_CTX;
    let trained = base.context_window.max(MIN_SERVE_CTX);
    match lane_fit_window(base) {
        // The host can hold the model's whole trained window — use it exactly.
        Some(fit) if trained <= fit => trained,
        // It cannot: largest stable rung under the real fit.
        Some(fit) => stable_window_rung(fit).max(MIN_SERVE_CTX),
        None => trained,
    }
}

/// Largest window whose KV + weights + compute buffer fit the GOVERNED host budget — the same
/// memory authority `plan_serving` sizes against, never a raw GPU probe. `None` when the host is
/// ungoverned or the model can't be sized, which the caller reads as "no cap".
pub(crate) fn lane_fit_window(base: &crate::model_registry::Model) -> Option<u32> {
    let fp = crate::cognition::serving_plan::footprint::footprint_for(base)?;
    if fp.kv_per_token == 0 {
        return None;
    }
    let budget = crate::resources::ResourceDaemon::global()
        .map(|d| crate::modules::serving_daemon::governed_host_budget(&d).usable_bytes)
        .filter(|b| *b > 0)?;
    let kv_budget = budget
        .checked_sub(fp.weights_bytes)?
        .checked_sub(fp.compute_buffer_per_lane())?;
    Some((kv_budget / fp.kv_per_token).min(u32::MAX as u64) as u32)
}

/// Round DOWN to the nearest power of two. A quantization for run-to-run stability — a few
/// hundred MB of budget flutter must not change `n_ctx` and thus the greedy token path. It is
/// deliberately NOT a cap: it only ever applies to a value already limited by real fit.
pub(crate) fn stable_window_rung(window: u32) -> u32 {
    if window == 0 {
        0
    } else {
        1u32 << (u32::BITS - 1 - window.leading_zeros())
    }
}
