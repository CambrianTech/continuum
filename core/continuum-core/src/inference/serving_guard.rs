//! Serving guard — the pre-flight that GUARANTEES the local single-resident lane is
//! serving the model this request names before a generation is trusted, and refuses a
//! prompt that alone overflows the served slot (#175). Carved out of
//! `openai_adapter::generate_stream` (pure code-motion, 2026-09-03 — the S3b decompose:
//! that function was ~1.3k lines doing everything; each concern becomes a module named
//! for the concern, never the wire format). Behaviour-identical to the inline block.

use serde_json::Value;

use crate::ai::openai_adapter::OpenAICompatibleConfig;

/// Why a generation was refused when the requested model is not guaranteed resident, said
/// in the words the caller needs to act on. THREE distinct situations reach this point and
/// only one of them is a fault:
///
/// | snapshot | meaning | caller should |
/// |---|---|---|
/// | never reconciled | core is still starting; nobody has looked yet | retry |
/// | reconciled, `active_model == None` | a lane is being torn down / rebuilt | retry |
/// | `active_model == Some(other)` | a DIFFERENT model is resident | NOT retry |
///
/// Both retry-able cases used to print the fault sentence. The cost is measured twice:
/// 116 false alarms over three days from the startup case (#350), and then — after that
/// split shipped — three citizens taking the same fault sentence 59 seconds into a #175
/// wedge self-heal that completed normally. `ServingSnapshot::empty()` is published by the
/// daemon on EVERY teardown (no servable plan, a re-home, a wedge relaunch), so `<none>` is
/// the ordinary appearance of a lane in transition, not evidence of breakage.
///
/// Pure by construction: takes the snapshot and the latch as arguments rather than reading
/// the process-global `SERVING_STATE`/`FIRST_RECONCILE`, so all three branches are testable
/// without a set-once global that would make test order load-bearing
/// ([[a-process-global-read-inside-a-decision-makes-tests-order-dependent]]).
/// Does `snap` GUARANTEE that the local single-resident gateway will answer as `model`?
///
/// The gateway serves ONE resident model and answers every request as that model whatever
/// the request's `model` field says, so "guaranteed" means the daemon has PUBLISHED that
/// this exact model is the live one — on the main lane, or on the verified #106 vision
/// sidecar (whose `/props` the daemon checked before publishing `vision_ready`).
///
/// One predicate, two readers: the pre-flight guard below and the post-wait re-check. Written
/// out once so the two can never drift into disagreeing about what "serving our model" means.
pub(crate) fn snapshot_guarantees(
    snap: &crate::inference::llama_server::ServingSnapshot,
    model: &str,
) -> bool {
    snap.ready
        && (snap.active_model.as_deref() == Some(model)
            || (snap.vision_ready && snap.vision_model.as_deref() == Some(model)))
}

/// Is refusing POINTLESS to wait out — i.e. has the daemon SETTLED on a different model?
///
/// A failed guarantee is one of two very different situations, and only one of them is
/// terminal:
///
/// - **Settled mismatch** — some other model is ready and active. The daemon has made its
///   choice; waiting cannot change it, and residency arbitration is the serving layer's job
///   (#109), never a generate's. Refuse immediately and loudly.
/// - **Transition** — no model is resident (`empty()`, published on EVERY teardown: no
///   servable plan, a re-home, a #175 wedge relaunch), or a lane is up but not yet decode-
///   ready. Nothing has failed; the lane is simply mid-flight and comes back on its own.
///
/// Measured 2026-08-07: a wedge self-heal flipped the snapshot not-ready at +374s and the
/// daemon republished ready at +436s — a 62-second window. Three citizens' turns landed
/// inside it and were refused outright, 9 seconds before the lane came back. The self-tick
/// readiness gate (#350) cannot cover this: it reads the snapshot BEFORE a deliberation that
/// takes tens of seconds, so a teardown starting mid-deliberation always outruns it. The gate
/// stops a turn that was doomed at its start; this stops one that was overtaken in flight.
pub(crate) fn settled_on_another_model(snap: &crate::inference::llama_server::ServingSnapshot) -> bool {
    snap.is_live()
}

pub(crate) fn unguaranteed_model_refusal(
    provider: &str,
    model: &str,
    snap: &crate::inference::llama_server::ServingSnapshot,
    served_before: bool,
) -> String {
    if !served_before {
        return format!(
            "{provider}: serving daemon has not completed its first reconcile yet (core is \
             still starting) — model '{model}' cannot be guaranteed until it does. This is \
             STARTUP, not a serving fault: it clears on its own, typically within seconds, \
             and the caller should retry rather than treat the lane as broken."
        );
    }
    let Some(active) = snap.active_model.as_deref() else {
        return format!(
            "{provider}: no model is resident right now (the serving daemon is between \
             lanes — a re-home or a self-healing relaunch), so model '{model}' cannot be \
             guaranteed. This is a serving TRANSITION, not a fault: it clears when the next \
             reconcile publishes a ready lane, and the caller should retry rather than \
             treat the lane as broken."
        );
    };
    format!(
        "{provider}: model '{model}' is not the active served model (serving: {active}, \
         ready: {}); the serving daemon owns which single model is resident — refusing to \
         generate against an unguaranteed model",
        snap.ready
    )
}

/// #175 overflow backstop: does this request body's PROMPT ALONE meet/exceed the served
/// per-slot window? Returns `Some(estimated_prompt_tokens)` when it does — the
/// unambiguous overflow that (with context-shift off) 500s AND poisons the slot for
/// every later request, so the caller must refuse to send rather than take the shared
/// lane down. `served_window == 0` (window unknown, e.g. mid-relaunch) → `None` (never
/// block on an unknown budget). Estimate is chars/4 — the same conservative heuristic as
/// the `serving.ctx_overshoot` alarm; we only trip on prompt-alone-overflows so a
/// legitimately-budgeted request (which always leaves reply headroom) is never blocked.
pub(crate) fn prompt_alone_overflows_served(body: &serde_json::Value, served_window: u32) -> Option<usize> {
    if served_window == 0 {
        return None;
    }
    let prompt_tokens = body
        .get("messages")
        .and_then(|m| m.as_array())
        .map(|msgs| {
            msgs.iter()
                .filter_map(|m| m.get("content").and_then(|c| c.as_str()))
                .map(|c| c.len() / 4)
                .sum::<usize>()
        })
        .unwrap_or(0);
    (prompt_tokens >= served_window as usize).then_some(prompt_tokens)
}

/// The pre-flight guard as one call. `Ok(())` = the lane guarantees `model` (or this
/// adapter is not a single-resident gateway / runs a dedicated lane); `Err` = the refusal
/// text the turn surfaces. `caller` names the requester for the overflow refusal.
pub(crate) async fn guard_resident_model(
    cfg: &OpenAICompatibleConfig,
    dedicated_lane: bool,
    model: &str,
    body: &Value,
    caller: &str,
) -> Result<(), String> {
    // Pre-flight the single-resident gateway: GUARANTEE our model is the one
    // actually serving before we trust a generation. The local gateway
    // (llama-server) serves ONE resident model, fixed at process launch, and
    // answers EVERY request as that model regardless of the request's `model`
    // field — so generating while a DIFFERENT model (or none) is live silently
    // returns the wrong brain (the bug that would haunt us). Crucially,
    // switching the served model is a *process relaunch* the
    // ServingDaemonModule owns (Contract A `inference::llama_server`); an
    // adapter must NEVER drive that load from inside a generate — relaunching
    // would kill the GPU-warm server out from under every other persona on the
    // shared gateway. So this guard is READ-ONLY: consult the daemon's
    // published serving snapshot (a `watch` borrow, no probe) and refuse to
    // generate unless OUR model is the READY, ACTIVE one. A mismatch is a loud
    // failure naming the cause, never a silent wrong-model answer
    // ([[fallbacks-are-illegal-fail-loud]]). Bringing the right model up — and
    // cross-persona residency arbitration on the shared gateway — is the
    // serving layer's job (#109), not this gate.
    // A dedicated lane (eval's EphemeralServingLane) is its OWN authority:
    // launched with exactly this model and confirmed HTTP-ready at spawn, so
    // the GLOBAL serving snapshot (which only knows the living persona lane) is
    // the wrong thing to consult. Skip the guard for a lane we own.
    if cfg.single_resident_model && !dedicated_lane {
        let snap = crate::inference::llama_server::current_serving();
        // The snapshot guarantees TWO residencies: the main lane's active
        // model, and (when published) the verified vision endpoint's model —
        // the #106 sidecar lane beside a text-only mind. A request for the
        // sidecar's model is exactly as guaranteed as one for the active
        // model (the daemon verified its `/props` before publishing), and
        // `endpoints_for_model` routes it to `vision_base_url`.
        let mut snap = snap;
        if !snapshot_guarantees(&snap, model) && !settled_on_another_model(&snap) {
            // A TRANSITION, not a fault — wait it out instead of failing the turn.
            // `await_ready_serving` is the daemon's own readiness signal (the same
            // `watch` it publishes): a push, not a poll, so this returns the instant the
            // relaunch lands and costs nothing while it waits. It is the mechanism the
            // boot gate, the eval lane, the embedder and the genome teacher all already
            // wait on; this seam was the one that refused instead.
            //
            // Budget is `DEFAULT_SERVING_WAIT` (= READY_TIMEOUT + 30s) on purpose: it is
            // DERIVED from the spawner's own load budget, so this can never declare a
            // failure before the daemon has exhausted its legitimate window to produce a
            // lane. Fail LOUD, not FAST. Bounded, so a lane that never returns still ends
            // as a named refusal rather than a hung generate.
            let started = std::time::Instant::now();
            let settled = crate::inference::llama_server::await_ready_serving(
                crate::inference::llama_server::DEFAULT_SERVING_WAIT,
            )
            .await;
            if let Some(s) = settled {
                snap = s;
            }
            // Carry the daemon's own stated degradation on the probe: an
            // unresolved wait with NO reason is "polling slop" a reader must go
            // spelunking to explain, while `degraded=` names the killer in the
            // stream itself (2026-08-15: every turn of a round waited here for
            // 120s each while serving/status knew the exact cause — a failed
            // decode smoke-probe — and nothing surfaced it).
            let degraded = snap.degraded_reason.as_deref().unwrap_or("");
            crate::probe!(
                class = "inference.awaiting_serving_transition",
                provider = cfg.provider_id.as_str(),
                wanted = model,
                waited_ms = started.elapsed().as_millis() as u64,
                served_before = crate::inference::llama_server::has_reconciled(),
                resolved = snapshot_guarantees(&snap, model),
                degraded = &degraded[..degraded.len().min(200)],
                "no lane was resident at pre-flight (serving transition) — waited on the \
                 daemon's readiness signal rather than failing the turn"
            );
        }
        if !snapshot_guarantees(&snap, model) {
            return Err(unguaranteed_model_refusal(
                &cfg.name,
                model,
                &snap,
                crate::inference::llama_server::has_reconciled(),
            ));
        }
        // #175 universal overflow backstop: REFUSE (never send) a prompt that alone
        // exceeds the served per-slot window. With context-shift OFF the server 500s
        // "Compute error" on overflow AND the fault POISONS the slot, so every LATER
        // request 500s too — one oversized prompt from ANY caller (a persona turn, a
        // dream distillation, an eval) takes the whole shared lane down until a
        // restart (the wedge storm this task chased). The persona deliberation path
        // already fits its prompt to the live window; this is the chokepoint backstop
        // for the ~10 OTHER callers that build their own prompts (dream_consolidation,
        // check_redundancy, validate_response, …), which the persona-scoped overshoot
        // WARN below never covered. A refused request fails LOUD naming the caller and
        // never reaches llama_decode, so the slot stays healthy. Threshold is PROMPT
        // ALONE ≥ window (unambiguous — no room for even the prompt, let alone a
        // reply), so a legitimately-budgeted request is never blocked. chars/4 is the
        // same conservative estimate the overshoot alarm uses.
        // [[fallbacks-are-illegal-fail-loud]] [[llama-compute-error-wedge-is-per-slot-context-overflow]]
        if let Some(prompt_tokens) =
            prompt_alone_overflows_served(&body, snap.served_context_window)
        {
            return Err(format!(
                "{}: refusing to generate — prompt ~{} tokens ≥ the served per-slot \
                 window of {} (caller: {}). Sending it would 500 and POISON the shared \
                 slot for every later request; fit the prompt to the served window (#175).",
                cfg.name,
                prompt_tokens,
                snap.served_context_window,
                caller,
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_only_when_prompt_alone_overflows_the_served_slot() {
        let body = |chars: usize| serde_json::json!({ "messages": [{ "role": "user", "content": "x".repeat(chars) }] });
        // ~12000 tokens (48000 chars / 4) vs a 8000-token slot → refuse, report the est.
        assert_eq!(
            prompt_alone_overflows_served(&body(48_000), 8_000),
            Some(12_000),
            "prompt alone over the window must be refused"
        );
        // ~4000 tokens vs an 8000 slot → fits (room for the prompt + a reply) → allow.
        assert_eq!(prompt_alone_overflows_served(&body(16_000), 8_000), None);
        // Window unknown (mid-relaunch) → never block, whatever the prompt size.
        assert_eq!(prompt_alone_overflows_served(&body(48_000), 0), None);
        // No messages array → nothing to overflow.
        assert_eq!(
            prompt_alone_overflows_served(&serde_json::json!({}), 8_000),
            None
        );
    }
}
