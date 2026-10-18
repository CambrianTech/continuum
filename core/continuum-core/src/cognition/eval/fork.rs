//! Forking the measurement copy of her mind: the shared-lane exam serving context for the live-lane
//! degrade, the bounded wait for a post-reboot workspace template, the dedicated-lane and live-lane
//! fork paths with their one offline refusal, and the warm gate that refuses to measure a cold model.

use super::*;

/// `clean_lane`: whether an eval-preemption quiesce lease was held for this run
/// (`Some(true)` = the live fleet was suspended, the fork measured on a clean GPU
/// lane; `None` = no lease acquired — no live roster, so the provenance is UNKNOWN,
/// never falsely claimed clean). This is the honesty stamp: a number carries whether
/// it was measured contended, on the durable row, so `cognition/observe` can light a
/// CLEAN/UNKNOWN chip instead of anyone inferring it. [[benchmark-numbers-carry-gpu-provenance]]
/// Build the Proctored Exam Session's serving context for the living-persona benchmark:
/// read the live serving snapshot + the governed budget, construct the resident set and the
/// exam's `LaneDemand`, and run the strategic [`ExamServingContext::acquire`] admission
/// decision. For the living-persona exam she is served on this base already, so the verdict
/// is `SharedLane` — she is measured as a co-tenant slot on her real lane (no second weight
/// copy) and the lane is held steady for the run. Making it an EXPLICIT decision (not the
/// old blind steady-hold) is the strategic layer plugged in at the exam seam
/// ([[lane-admission-planner-scenario-driven]]). When the live inputs aren't resolvable
/// (ungoverned host, model row missing, lane not yet ready) it falls back to holding the
/// lane steady on the historical share assumption — behavior never regresses, but the gap
/// is marked so it's visible. [[proctored-exam-session-dependable-benchmark]]
pub(super) async fn acquire_exam_serving_context() -> crate::cognition::exam_serving::ExamServingContext {
    use crate::cognition::exam_serving::ExamServingContext;
    use crate::resources::placement::{DemandTier, LaneDemand, ResidentLane};

    let snap = crate::inference::llama_server::current_serving();
    let (Some(active), true) = (snap.active_model.as_deref(), snap.ready) else {
        return ExamServingContext::ludicrous_fallback().await;
    };
    let Some(model) = crate::model_registry::try_global().and_then(|r| r.model(active).cloned())
    else {
        return ExamServingContext::ludicrous_fallback().await;
    };
    let Some(fp) = crate::cognition::serving_plan::footprint::footprint_for(&model) else {
        return ExamServingContext::ludicrous_fallback().await;
    };
    // Capacity = the ONE memory authority's governed budget (VRAM netted over every measured
    // consumer + external pressure), the same source `plan_serving` sizes against — never a
    // raw GPU probe. No authority (ungoverned host) or a zero budget ⇒ fall back.
    let Some(capacity) = crate::resources::ResourceDaemon::global()
        .map(|d| crate::modules::serving_daemon::governed_host_budget(&d).usable_bytes)
        .filter(|c| *c > 0)
    else {
        return ExamServingContext::ludicrous_fallback().await;
    };
    let window = snap.served_context_window.max(1);
    let compute_buffer = fp.compute_buffer_per_lane();
    let resident = ResidentLane {
        lane_id: "live".to_string(),
        base_model_id: active.to_string(),
        weights_bytes: fp.weights_bytes,
        slots: snap.lanes.max(1),
        window,
        kv_per_token: fp.kv_per_token,
        compute_buffer,
        tier: DemandTier::Live,
        pinned: true,
    };
    // The exam demand: her SAME base, one measured decode slot, at the live served window, at
    // Eval tier (preemptible by live work, never preempts it) — and ISOLATED. An exam is a
    // hard, disruption-intolerant task: a co-tenant slot on the live persona lane starves it
    // behind their turns (glass-boxed 2026-07-21 — looked like the model "spinning" but was the
    // lane taken away mid-thought). `isolate: true` makes the planner give it its OWN dedicated
    // lane when a fresh copy fits (autonomic — no `base_model_id` hand-holding), falling back to
    // a co-tenant share only under real memory pressure. See `LaneDemand::isolate`.
    // Size the DEDICATED exam lane's window to what actually fits beside the resident live
    // lane, instead of mirroring the live window. A fresh copy's KV at the full served window
    // (e.g. 47872 × 1 slot ≈ 8 GB) on top of the resident lane (2 slots at that window ≈ 30 GB)
    // can exceed the free budget — which forced the isolate demand back to a co-tenant SHARE
    // that then STARVED the exam behind live turns (glass-boxed 2026-07-21: dedicated lane never
    // spawned, 0/3 graded in 12 min). Compute the largest window whose fresh copy fits the free
    // space; a self-contained coder task needs far less than the live chat window, so this keeps
    // its OWN lane (the isolation is the point) at a window matched to the budget. If not even a
    // floor-sized copy fits, `isolate` still degrades to SHARE in plan_placement (never an OOM).
    let free = capacity.saturating_sub(resident.footprint());
    let kv_budget = free.saturating_sub(fp.weights_bytes.saturating_add(compute_buffer));
    let max_fit_window = if fp.kv_per_token > 0 {
        (kv_budget / fp.kv_per_token).min(u32::MAX as u64) as u32
    } else {
        window
    };
    // Capped at the live window (the exam never asks for MORE context than the persona actually
    // has) and at what genuinely fits beside the resident lane. The old `.max(EXAM_WINDOW_FLOOR
    // = 8192)` here was a magic floor that could exceed the real fit — asking for KV the host
    // could not hold, on the theory that 8k is "enough for a task". Honest fit is the floor: if
    // even a floor-sized copy won't fit, `isolate` degrades to SHARE in plan_placement rather
    // than inventing headroom. [[never-hardcode-a-context-window-4k-defaults-destroy-the-moe-thesis]]
    let exam_window = window
        .min(max_fit_window)
        .max(crate::inference::planning::MIN_SERVE_CTX);
    let demand = LaneDemand {
        base_model_id: active.to_string(),
        weights_bytes: fp.weights_bytes,
        slots: 1,
        window: exam_window,
        kv_per_token: fp.kv_per_token,
        compute_buffer,
        tier: DemandTier::Eval,
        isolate: true,
    };
    ExamServingContext::acquire(capacity, std::slice::from_ref(&resident), &demand).await
}

/// Seconds to wait (1s/try) for a persona's fork-template to appear before failing the
/// eval. Covers the post-reboot window where the supervisor's `register_from_cfg` is
/// still assembling the mind when an eval fires (the fork race, 2026-07-17).
const WORKSPACE_TEMPLATE_WAIT_TRIES: u32 = 10;

/// How a measurement copy of her mind is forked: whose mind, whether it gets hands, where
/// those hands act, and whether its episodic recall is suppressed. One value every fork
/// path reads, so the dedicated-lane and live-lane forks cannot drift apart.
pub(super) struct ForkSpec<'a> {
    pub(super) persona: Uuid,
    pub(super) needs_tools: bool,
    pub(super) workspace_root: Option<&'a str>,
    pub(super) suppress_recall: bool,
}

/// Fork a measurement copy onto a DEDICATED eval lane (a gene's base, a pinned base, or
/// her own base on a throwaway server), waiting out the post-reboot template race.
pub(super) async fn fork_onto_lane(
    spec: &ForkSpec<'_>,
    lane: &EvalLane,
) -> Result<crate::cognition::workspace::WorkspaceCycle, CommandError> {
    fork_eval_cycle_waiting(&spec.persona, || {
        crate::cognition::persona_workspace::global().fork_eval_cycle_with_adapter(
            &spec.persona,
            lane.adapter.clone(),
            lane.served_ctx,
            spec.needs_tools,
            spec.workspace_root,
            spec.suppress_recall,
            Vec::new(),
        )
    })
    .await
    .ok_or_else(|| persona_offline(spec.persona, "a measurement copy of her mind"))
}

/// Fork a measurement copy as a co-tenant on her LIVE lane — the degrade path when no
/// dedicated lane could be placed.
pub(super) async fn fork_onto_live_lane(
    spec: &ForkSpec<'_>,
) -> Result<crate::cognition::workspace::WorkspaceCycle, CommandError> {
    fork_eval_cycle_waiting(&spec.persona, || {
        crate::cognition::persona_workspace::global().fork_eval_cycle(
            &spec.persona,
            spec.needs_tools,
            spec.workspace_root,
            spec.suppress_recall,
        )
    })
    .await
    .ok_or_else(|| persona_offline(spec.persona, "a measurement copy without measuring her live mind"))
}

/// The one refusal every fork path gives when no workspace template appeared in time.
fn persona_offline(persona: Uuid, what: &str) -> CommandError {
    CommandError::NotFound(format!(
        "persona {persona} is not online — no workspace template after {WORKSPACE_TEMPLATE_WAIT_TRIES}s, \
         so eval cannot fork {what}. She either isn't running or the id is wrong; call \
         persona/instances/list for the personas online right now."
    ))
}

/// Fork a measurement copy, WAITING out the post-reboot template race. After a reboot
/// `register_from_cfg` assembles each persona's fork-template asynchronously, so an eval
/// fired seconds after boot can race ahead of it and see `None`. This race is NOT specific
/// to one lane — it hits the live-lane, base-model-lane, AND gene-lane forks identically —
/// so ALL of them retry through here (previously only the live-lane branch did, and a
/// tool-using eval on a base_model_id fired right after `continuum reboot` failed loud instead of
/// waiting). `fork` is the per-lane fork call; retried up to
/// [`WORKSPACE_TEMPLATE_WAIT_TRIES`] times, 1s apart, then `None` (caller fails loud).
pub(super) async fn fork_eval_cycle_waiting(
    persona_uuid: &Uuid,
    mut fork: impl FnMut() -> Option<crate::cognition::workspace::WorkspaceCycle>,
) -> Option<crate::cognition::workspace::WorkspaceCycle> {
    for attempt in 0..WORKSPACE_TEMPLATE_WAIT_TRIES {
        if let Some(cycle) = fork() {
            return Some(cycle);
        }
        if attempt == 0 {
            tracing::info!(
                %persona_uuid,
                "eval: workspace template not ready (post-spawn register_from_cfg race) — waiting"
            );
        }
        // Emit the wait as an EVENT so positronic layers show "preparing…" instead of a
        // dead spinner — this null-progress window otherwise looked identical to a hang.
        emit_eval_phase(
            "preparing",
            &format!("waiting for workspace template ({}s)", attempt + 1),
        );
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    None
}

/// WARM-GATE — never measure a COLD model. A just-loaded or just-swapped lane 500s
/// until its weights + KV are resident; firing the gym into it scores "inference
/// failed" on every task — a false ZERO that would LIE about how far behind (or
/// ahead) we are. A measurement that can't trust its own model is no measurement.
/// Poll with a tiny throwaway deliberation until the lane answers, then run; if it
/// never warms, fail LOUD rather than emit a bogus 0. (Cost is one cheap probe on a
/// warm lane — the common case returns on the first try.)
pub(super) async fn await_warm_cycle(
    cycle: &crate::cognition::workspace::WorkspaceCycle,
) -> Result<(), CommandError> {
    let warm_room = crate::identity::ActivityRoom::mint();
    let mut warm = false;
    for _ in 0..15u32 {
        let burst = exam_burst(warm_room, "ready check");
        let probe = crate::cognition::act_observe::drive_to_settle(
            cycle,
            burst,
            0, // no acts — a bare deliberation is enough to prove the lane answers
            crate::cognition::workspace::TurnFraming::directed(),
        )
        .await;
        if probe.inference_error.is_none() {
            warm = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
    }
    // Reset the volatile scratch the probe touched so task #1 starts from the same
    // clean frame every other task does.
    cycle.reset_working_memory();
    if !warm {
        return Err(CommandError::Internal(
            "serving lane not ready after ~60s of warm probes — refusing to run an \
             eval on a cold model (every task would score a false 'inference failed' \
             zero). Wait for the model to load, then retry."
                .to_string(),
        ));
    }
    Ok(())
}
