use super::*;

impl LlmDeliberationFaculty {
    pub(in super::super) async fn contribute_with_receipts(
        &self,
        ws: &Workspace,
        receipts: &mut Vec<crate::ai::provenance::GenerationReceipt>,
        rendered: &mut Vec<crate::cognition::render_receipt::RenderReceipt>,
    ) -> Option<Contribution> {
        // ONE atomic snapshot of the model binding for the whole turn — the
        // adapter we generate through, the model we request, and the served window
        // we size the prompt to all come from the same {adapter, model, window}
        // triple, so a re-home (`serving/pin` / grid failover) landing mid-turn can
        // never tear them apart. `load_full` (owned `Arc`) is held across the
        // `.await` below; a swap that happens after this load takes effect on the
        // NEXT turn. See [`ModelBinding`].
        let loaded = self.binding.load_full();
        // Repair a cold remote route before sampling its window or building the
        // prompt. The same act then acquires its selected home lane's admission;
        // no remote-sized request is replayed on a smaller local window.
        loaded.adapter.prepare_generation().await;
        // RESTORE-AHEAD (compression-ladder rung 1): tell the serving adapter
        // this page generates NEXT, so her KV page pages in DURING the
        // prompt assembly below instead of racing the turn at pin time. Fire-
        // and-forget; the pin-time path stays the backstop; no page (a nil id, or
        // a fork in no room) warms nothing.
        if let Some(page) = self.page(ws.room_id) {
            loaded.adapter.warm_ahead(page);
        }
        // Size this turn's {prompt + reply reserve} to the LIVE served per-slot window —
        // in BOTH directions. If the lane relaunched SMALLER than this persona's spawn-time
        // pin, a prompt over the slot overflows `llama_decode` → 500 "Compute error" that
        // POISONS the slot for every later request (#175, the wedge storm). If the lane grew
        // LARGER (a cold-boot 4096 slot that the daemon re-computed to 16128 once warm), she
        // must budget against what she actually has, not stay clamped at the stale cold value
        // (glass-boxed 2026-07-19: recall + tools squeezed into 25% of the real window; it was
        // even mis-triggering the tool-surface shrink). Both the prompt budget (`prompt_view`)
        // and the completion reserve (`build_request`) derive from THIS one
        // `binding.context_window`, so both estimates respect the observed capacity.
        // This is not a tokenizer count or a lease against a later lane resize.
        // A 0/not-ready snapshot (mid-relaunch) leaves the
        // provisioned window; the next ready tick re-reads. Model + adapter stay the same atomic
        // triple. [[fallbacks-are-illegal-fail-loud]] [[never-thrash-sticky-hysteresis-on-every-lane]]
        // Retain the capacity observation that precedes assembly. Sampling again
        // after an await could mistake newly grown capacity for the refused one.
        let capacity_readiness = Arc::new(crate::cognition::workspace::CapacityReadiness::new(
            self.binding.clone(),
            loaded.clone(),
        ));
        let binding = {
            // ONE live source, no clamp. The window this turn budgets to is the live
            // served window of the lane THIS persona is actually on — and the ADAPTER
            // that owns that lane reports it (`live_served_window`), so cognition never
            // reaches for a GLOBAL serving snapshot that might describe a DIFFERENT
            // server. A shared-gateway persona gets the gateway's current slot (tracked
            // up AND down through a relaunch); a dedicated eval fork keeps its own pinned
            // /props window (its adapter returns `None` → the binding window stands).
            // The prompt is BUILT to this at assembly — never generated large then
            // truncated. [[budget-at-assembly-never-clamp-the-prompt]]
            // [[no-hardcoded-context-numbers-derive-from-the-live-window]]
            let effective = capacity_readiness.observed_window();
            // Preserve KV stability through small GROWTH fluctuations. Every
            // reduction is authoritative immediately: reply reserve is requested
            // output, not spare capacity that can absorb a smaller served window.
            // The existing quantized history eviction still controls prefix churn.
            let deadband = loaded.context_window / 8;
            let adopt = effective < loaded.context_window
                || effective.saturating_sub(loaded.context_window) > deadband;
            if !adopt {
                if effective != loaded.context_window {
                    continuum_base::probe!(
                        class = "delib.window.held",
                        binding = loaded.context_window,
                        served = effective,
                        "small live capacity growth — retaining the smaller binding for KV stability"
                    );
                }
                loaded
            } else {
                tracing::info!(
                    persona = %self.persona_name,
                    binding = loaded.context_window,
                    served = effective,
                    probe_class = "delib.window.live",
                    "sized this turn to the lane's LIVE served window (adapter-reported, \
                     one source, no clamp)"
                );
                // PERSIST the reconciliation (2026-08-24, the 17% mind): this
                // adoption was turn-LOCAL, so `model_loadout()` — the source every
                // OTHER window consumer reads (working-memory ring/trail/archive
                // budgets in apply.rs) — kept serving the stale spawn pin forever.
                // Measured on round 706215f4: binding 28,672 vs served 166,400 —
                // her result tails clipped to 896 chars on a 166k lane, and the
                // re-read loops that shredded her memory were visible in her own
                // trail ("I've been going in circles").
                //
                // This observation belongs to the EXACT loaded binding. An RCU
                // retry against a newer route would attach the old adapter's
                // capacity to a different model, or replace a newer observation.
                // Publish once only if that snapshot still owns the handle. A
                // losing turn keeps its original route locally; the next turn
                // reads the winning binding and samples that adapter afresh.
                let reconciled = Arc::new(ModelBinding {
                    adapter: std::sync::Arc::clone(&loaded.adapter),
                    model: loaded.model.clone(),
                    context_window: effective,
                });
                let _ = self
                    .binding
                    .compare_and_swap(&loaded, Arc::clone(&reconciled));
                reconciled
            }
        };
        let (mut fit_window, mut calibration) = self.prompt_fit(&binding);
        let capacity_readiness = Arc::new(
            capacity_readiness.as_ref().clone().with_fit_window(fit_window),
        );
        let visual = match self.active_visual_feedback(&binding).await {
            Ok(visual) => visual,
            Err(error) => return Some(Contribution::deliberation_fault(error)),
        };
        let mut rejected_prompt_tokens = None;
        let mut rejected_measured_tokens = None;
        let mut request_template: Option<TextGenerationRequest> = None;
        // Her destination refused the tail her learned head promised: rebuilt once on the
        // activity's own surface, so she is never offered a verb she cannot call.
        let mut tail_refused = false;
        // Her newest result was refit once at a refusal's measured density
        // (`verdict/newest_refit.rs`): a second refusal parks, never resends.
        let mut newest_refit_sent = false;
        let mut view =
            self.prompt_view_with_visual_feedback(ws, fit_window, calibration, visual.as_ref());
        let mut gen_await_ms = 0u64;
        // The allowance the request actually carried (set where it is built, read at the seam).
        let mut sent_allowance: Option<u32> = None;
        // Correct only frozen admitted input, consuming a finite optional unit on
        // every retry. Exact measured progress is required too. No accepted cognition
        // is replayed, and no working-memory or RAG source is re-read or refilled.
        let (view, resp) = loop {
            if let Some(error) = &view.protocol_error {
                return Some(Contribution::deliberation_fault(error.clone()));
            }
            // The one budget broke (`prompt/budget.rs` probed each claim): never sent.
            if let Some(fault) = &view.budget_fault {
                return Some(Contribution::deliberation_fault(fault.clone()));
            }
            if let Some(error) = view.capacity_error {
                // Required input past the WORKING window is served from the whole slot, once,
                // never refused while the slot has room (`ContextBudget::working_window`).
                let (served_fit, served_calibration) = self.prompt_fit_served(&binding);
                if served_fit > fit_window {
                    continuum_base::probe!(
                        class = "delib.prompt.working_window_exceeded",
                        persona = %self.persona_name,
                        required_tokens = error.required_tokens,
                        working_window = fit_window,
                        served_window = served_fit,
                        "required input exceeds the working window: this act is fitted to the served window instead"
                    );
                    fit_window = served_fit;
                    calibration = served_calibration;
                    view = self.prompt_view_with_visual_feedback(ws, fit_window, calibration, visual.as_ref());
                    continue;
                }
                continuum_base::probe!(
                    class = "delib.prompt.capacity",
                    persona = %self.persona_name,
                    required_tokens = error.required_tokens,
                    budget_tokens = error.budget_tokens,
                    context_window = fit_window,
                    "required prompt input cannot fit; refusing an incomplete prompt"
                );
                return Some(
                    Contribution::prompt_capacity_refused(error)
                        .with_capacity_readiness(capacity_readiness.clone()),
                );
            }
            // FAIL LOUD, never a blank mind (companion to the `delib.prompt.empty`
            // probe in the fitter): a view with NO conversation from a room that HAS
            // turns means the budget arithmetic starved the prompt. Skipping the turn
            // is safe (the room's messages stay queued; next tick re-perceives) —
            // deliberating on a blank prompt is not: that is exactly how every persona
            // greeting-looped for an hour on 2026-07-30 while looking "alive".
            if view
                .messages
                .iter()
                .all(|m| m.content_text().trim().is_empty())
                && !ws.turns.is_empty()
            {
                tracing::error!(
                    persona = %self.persona_name,
                    window = fit_window,
                    room_turns = ws.turns.len(),
                    probe_class = "delib.prompt.starved",
                    "prompt fit produced an EMPTY conversation from a non-empty room — \
                     skipping this turn rather than deliberating blind"
                );
                return None;
            }
            // Introspection seam: emit EXACTLY what the model sees this tick. The RAG
            // is the load-bearing input — never opaque. Enable the `cognition` log
            // category for the persona to capture this per-turn (the existing
            // record/replay harness — recorder + RagCaptureSink + vdd::turn_replay —
            // is the durable path; this debug emit is the live tap).
            tracing::debug!(
                target: "cognition::deliberation",
                persona = %self.persona_name,
                system_prompt = %view.system,
                burst = %view.user_text(),
                "deliberation prompt — what the model sees this turn"
            );
            // SINGLE SHOT: one accepted generation → one verdict. A strictly
            // shrinking corrective admission replay follows a typed refusal; no
            // provider text or tool action has been accepted at that point.
            // This faculty does not run an internal agent loop. If the model chooses to act, that is a
            // `Decision::Act` verdict; the ORGANISM (the act→observe driver,
            // `super::act_observe`) executes the calls, admits the result as memory,
            // and re-perceives at the next tick. "Done" is the workspace SETTLING into
            // Speak/Pass across ticks — never a counter in here. See
            // docs/cognition/ACTING-ORGANISM.md §3.3.
            //
            // The thread is now ROLE-ATTRIBUTED (own posts → assistant, peers → user)
            // built from the workspace's structured turns, not one flat `user` blob —
            // the structured-turns refactor that fixes the echo loop.
            // A verb not on her native surface still DISPATCHES: `act_observe` resolves any
            // call by name, and the system prompt's menu plus `commands/help` reach it.
            // The view retains the tool level bought by the shared allocation for this epoch.
            // Send that same selection so accounting, wire bytes and prefix reuse agree.
            if request_template.is_none() {
                // What her view's allocation bought; a server that refuses `tail_tools` gets the
                // activity surface at the same grant, without her learned head.
                let mut selected = if tail_refused {
                    match self.surface.select_granted(&ws.room_affordances, view.tools_grant) {
                        Ok(activity) => activity.into_owned(),
                        Err(_) => view.tools.clone(), // unreachable in practice: the view ran this same activity selection at this grant and it succeeded
                    }
                } else {
                    view.tools.clone()
                };
                // Verbs she loaded into the tail ride beside the head, callable natively.
                let tail = std::mem::take(&mut selected.tail);
                if selected.reason == SurfaceReason::CoreHandsForWindow {
                    // The one truncating arm: the served window cannot hold her extended
                    // hands. Whoever later asks "why did she never call X" needs this row.
                    // Fixed per lane, so it repeats each turn until the window changes.
                    let served = binding.context_window;
                    continuum_base::probe!(
                        class = "delib.tool_surface.withheld",
                        persona = %self.persona_name,
                        offered = selected.specs.as_deref().map_or(0, <[_]>::len) as u64,
                        hands = self.surface.hands().len() as u64,
                        surface_tokens = selected.tokens as u64,
                        withheld_tokens = selected.demand_tokens().saturating_sub(selected.tokens) as u64,
                        budget_tokens = view.tools_grant as u64,
                        context_window = served,
                        "her one allocation granted her core hands, not her extended ones: \
                         the discovery pair still reaches the rest"
                    );
                }
                // Selection and budget accounting borrow the cached schemas. Only the
                // owned inference request needs a copy of the selected surface.
                let tools = selected.specs.map(std::borrow::Cow::into_owned);

                let request = self.build_request_within(
                    &binding,
                    view.completion_reserve,
                    view.messages.clone(),
                    tools,
                    view.system.clone(),
                    {
                        // Turn-boundary hygiene: peer-name stops (#150, don't speak AS
                        // teammates) + reserved-marker stops (#158, don't fabricate
                        // [action]/[recall] receipts). Combined into one stop list.
                        let mut stops = super::super::super::deliberation_budget::peer_stop_sequences(&ws.turns);
                        stops.extend(super::super::super::deliberation_budget::reserved_marker_stop_sequences());
                        (!stops.is_empty()).then_some(stops)
                    },
                    Some(ws.room_id),
                    self.turn_kind(ws),
                );

                let mut template = request;
                template.tail_tools = (!tail.is_empty()).then_some(tail);
                template.messages.clear();
                request_template = Some(template);
            }
            let mut request = request_template.as_ref().expect("initial request assembled").clone();
            request.messages = view.messages.clone();
            // Where her view ends in THIS view's messages: the engine checkpoints there and
            // her page keeps it, so the next act resumes past everything she already read.
            request.view_messages = u32::try_from(view.refit.grounding_at).ok().filter(|len| *len > 0);
            // The allowance belongs to THIS view, refreshed on every refit: the template
            // is built once, and a template's `max_tokens` from an earlier plan would
            // bound the reply to a window the prompt no longer leaves.
            request.max_tokens = Some(view.completion_reserve.max(1));
            let estimated_prompt_tokens = view.estimated_prompt_tokens;
            if rejected_prompt_tokens.is_some_and(|previous| estimated_prompt_tokens >= previous) {
                return Some(Contribution::deliberation_fault(
                    "provider rejected the prompt; required input cannot be reduced further within this capacity",
                ));
            }
            let started = std::time::Instant::now();
            // THE ALLOWANCE THE REQUEST ACTUALLY CARRIED (Cormac on #4409): the reasoning
            // budget was derived from this max_tokens, not from the reserve — they differ
            // whenever her need is measured below the reserve — so the classifier at the
            // seam must read the same number, or a real budget-hit reads as Landed.
            // A rebuilt request with no max_tokens keeps the last one sent (the read of the
            // previous value is also what keeps the initializer honest).
            sent_allowance = request.max_tokens.or(sent_allowance);
            let result = self
                .generate_for_workspace(ws, &binding, fit_window, request, view.allocation.clone(), receipts, rendered)
                .await?;
            gen_await_ms = gen_await_ms.saturating_add(started.elapsed().as_millis() as u64);
            match result {
                Err(crate::ai::inference_error::InferenceError::ContextExceeded {
                    requested,
                    available,
                }) => {
                    let Some(observed) = PromptCalibration::observed(
                        estimated_prompt_tokens,
                        requested,
                        Some(view.input_identity),
                    ) else {
                        return Some(Contribution::deliberation_fault(
                            "provider capacity refusal lacked a positive request/token measurement",
                        ));
                    };
                    calibration = observed;
                    fit_window = fit_window.min(available);
                    // Retain only measured density, not a fixed token surcharge.
                    // The next tick prices its own input before submitting. A
                    // different adapter/model/window cannot inherit this record.
                    self.prompt_feedback.store(Some(Arc::new(PromptFeedback {
                        binding: Arc::clone(&binding),
                        calibration,
                        available: fit_window,
                    })));
                    continuum_base::probe!(
                        class = "delib.prompt.refit",
                        persona = %self.persona_name,
                        requested_tokens = requested,
                        available_tokens = available,
                        estimated_prompt_tokens,
                        completion_reserve = view.completion_reserve,
                        corrective_replay_used = rejected_prompt_tokens.is_some(),
                        "provider capacity refusal: retained measured token density for prompt fitting"
                    );
                    if newest_refit_sent {
                        let error = self.newest_refit_refused(requested, available, view.completion_reserve);
                        return Some(
                            Contribution::prompt_capacity_refused(error).with_capacity_readiness(Arc::new(
                                capacity_readiness.as_ref().clone().with_fit_window(fit_window),
                            )),
                        );
                    }
                    if let Some(refit) = self.refit_newest_after_refusal(
                        ws, fit_window, calibration, visual.as_ref(), &view, requested, available,
                    ) {
                        newest_refit_sent = true;
                        rejected_prompt_tokens = Some(estimated_prompt_tokens);
                        rejected_measured_tokens = Some(requested);
                        view = refit;
                        continue;
                    }
                    if rejected_measured_tokens.is_some_and(|previous| requested >= previous) {
                        return Some(Contribution::deliberation_fault(
                            "exact prompt correction made no measured progress; refusing further admission replay",
                        ));
                    }
                    rejected_prompt_tokens = Some(estimated_prompt_tokens);
                    rejected_measured_tokens = Some(requested);
                    view = match self.refit_rejected_prompt(
                        view, requested, available, sent_allowance.unwrap_or(0),
                    ) {
                        Ok(view) => view,
                        Err(error) => {
                            return Some(
                                Contribution::prompt_capacity_refused(error)
                                    .with_capacity_readiness(Arc::new(
                                        capacity_readiness.as_ref().clone().with_fit_window(fit_window),
                                    )),
                            )
                        }
                    };
                }
                Err(crate::ai::inference_error::InferenceError::TailToolsUnreachable { model }) if !tail_refused => {
                    continuum_base::probe!(
                        class = "delib.tool_head.tail_refused",
                        persona = %self.persona_name,
                        model = %model,
                        "the engine her request reaches cannot take her tail verbs now: rebuilding \
                         on the activity's surface, never sending a head missing verbs she was offered"
                    );
                    tail_refused = true;
                    request_template = None;
                }
                Err(error) => {
                    return Some(Contribution::deliberation_fault(error.to_string()));
                }
                Ok(response) => {
                    if let Some(error) = response.generation_error() {
                        return Some(Contribution::deliberation_fault(error));
                    }
                    self.learn_prompt_density(
                        &binding,
                        &view,
                        response.usage.input_tokens,
                        // No timing on the reply = nothing measured as reused = nothing rides free.
                        response.timing.as_ref().map_or(0, |t| t.cached_tokens),
                        fit_window,
                    );
                    break (view, response);
                }
            }
        };
        // The channel split the NEXT turn's allowance is sized from: the server's
        // count apportioned by the bytes the adapter separated into `reasoning`
        // and `text` (a think-only turn is all reasoning; a native call all answer).
        // The answer share is the rest of the output count; whether she answered at all
        // is presence, never this split (`EmissionStop::classify`).
        let (reasoning_tokens, _) = reasoning_answer_split(
            resp.usage.output_tokens,
            resp.reasoning.as_ref().map_or(0, |r| r.len()),
            answer_payload_bytes(&resp.text, resp.content.as_deref()),
        );
        // WHERE the cap fell decides what the cut measures: inside the answer, the
        // answer channel grows (a write cut mid-payload gets its room); inside the
        // think, nothing was said and the need ring does not take it — a thinking
        // model fills any allowance, and doubling that walked her to the deadline
        // (`EmissionStop`). The cut itself is the fault paths' business below.
        // A deliberation's think that stopped AT its budget is censored, not measured
        // (Cormac on #4409): the classifier records it at the allowance so the need ring
        // never learns the budget and contracts the next allowance geometrically.
        let allowance = sent_allowance.unwrap_or(view.completion_reserve); // unwrap_or: a request built with no max_tokens was bounded by the reserve alone
        let reasoning_budget = match self.turn_kind(ws) {
            TurnKind::Pass => crate::inference::request_body::deliberation_reasoning_budget(
                u64::from(allowance),
            )
            .map(|b| b as u32),
            TurnKind::Act => None,
        };
        // The one classification, from what the response carries (an answer is text or a call;
        // a stream may not report usage), counted here before any await after the generation: a
        // cancellation at the read receipt below must not lose the stop of a generation that
        // already completed (Codex on #5616).
        let answered = answer_payload_bytes(resp.text.trim(), resp.content.as_deref()) > 0
            || resp.tool_calls.as_ref().is_some_and(|calls| !calls.is_empty());
        let reasoned = resp.reasoning.as_deref().is_some_and(|r| !r.trim().is_empty());
        let stop = super::super::super::working_set::EmissionStop::classify_with_budget(
            matches!(resp.finish_reason, FinishReason::Length),
            answered,
            reasoning_tokens,
            reasoning_budget,
            allowance,
        );
        crate::cognition::health_ledger::note_stop(stop.counted(answered, reasoned));
        // Sent and answered: she read what this prompt rendered (read_receipt.rs), never before.
        self.send_read_receipt(ws).await;
        let messages = &view.messages;
        let lifecycle_recorded = self
            .prompt_capture
            .as_ref()
            .is_some_and(|sink| sink.lifecycle_enabled());
        // #139 latency split: the model call's wall time. Compare to the forwarder's
        // `persona.turn.first_token` (whole-turn spawn→first token): first_token −
        // gen_await ≈ cognition-prep (recall/embeddings/context assembly BEFORE the
        // model); gen_await itself is lane-queue + prefill + decode. This is how we
        // find where the minutes actually go before optimizing anything.
        tracing::info!(
            persona = %self.persona_name,
            gen_await_ms = gen_await_ms,
            "delib.generate — model-call wall time (lane-queue + prefill + decode)"
        );

        // MEASURE THE REPLY — the observation the reply's claim ([`reply_claim`]) is
        // sized from. The server's own count (reasoning included), recorded
        // for every completed generation; a cut inside the answer records that channel
        // at double inside the registry (the growth path), a cut inside the think is not
        // a need sample.
        // This is the seam that turns the reserve from a `window/2` prior into a
        // measurement.
        //
        // Classified once, for every completed generation, before anything branches on it:
        // the hour line counts the stop here and nowhere else, so a cut that a fault path
        // handles below is still counted exactly once (Aris's three cuts inside thinking read
        // "think-only 0" while the count sat in one branch, the M5 2026-10-30).
        {
            if let super::super::super::working_set::EmissionStop::ThinkBudgetHit { allowance } = stop {
                continuum_base::probe!(
                    class = "delib.emission.think_budget_hit",
                    persona = %self.persona_name,
                    reasoning_tokens,
                    allowance,
                    "the think stopped at its budget — censored, recorded at the allowance, never taught to the need ring"
                );
            }
            if stop == super::super::super::working_set::EmissionStop::CutMidThought {
                continuum_base::probe!(
                    class = "delib.emission.cut_mid_thought",
                    persona = %self.persona_name,
                    output_tokens = resp.usage.output_tokens,
                    reasoning_tokens,
                    "think-only cut at the allowance — the peak holds, the need ring does not take it"
                );
            }
            let measured = self.working_set.as_ref().and_then(|reg| {
                reg.record_emission(
                    self.persona_id,
                    resp.usage.output_tokens,
                    reasoning_tokens,
                    stop,
                    ws.now_ms.unwrap_or(0), // JUSTIFIED unwrap_or: unstamped cycle still measures honestly (registry keeps peaks, not a time series)
                )
            });
            if stop == super::super::super::working_set::EmissionStop::CutMidAnswer {
                // The cost of claiming her measured need (ONE-BUDGET slice 6): a write larger than any
                // she has measured is cut once, then grows the next claim. Counted, so a live read sees it.
                continuum_base::probe!(
                    class = "delib.emission.cut_mid_answer",
                    persona = %self.persona_name,
                    allowance,
                    output_tokens = resp.usage.output_tokens,
                    peak = measured.as_ref().map_or(0, |m| m.peak_tokens), // map_or: an unrecorded emission has no peak to report
                    "an answer cut at its measured grant: recorded at double, the next claim grows"
                );
                if let (Some(wm), Some(measured)) = (&self.working_memory, measured) {
                    wm.retain_truncated_emission(ws.activity_room_id, measured.peak_tokens);
                }
            }
        }

        // SEGMENT ATTRIBUTION (the rent ledger, compression-ladder rung 2):
        // the server's reused-prefix length lands somewhere in the prompt's
        // segment map — the run containing that frontier NAMES what broke
        // reuse this turn, and the stream of these rows IS each segment's
        // rent. Only when the lane measured timings (same honesty rule as
        // delib.generate.cache: a fabricated attribution is a lying receipt).
        if let Some(t) = resp.timing.as_ref() {
            let cached = t.cached_tokens as u32;
            // Her last turn's shape — what the next work call's occupancy bound is read from.
            LAST_TURN_SHAPE.insert(
                self.persona_id,
                TurnShape {
                    input: resp.usage.input_tokens,
                    cached,
                    output: resp.usage.output_tokens,
                },
            );
            let frontier = view
                .segments
                .iter()
                .find(|(_, end)| *end > cached)
                .map(|(label, _)| *label)
                .unwrap_or("fully-warm"); // reuse extends past every mapped segment — nothing broke
            let mut map = String::with_capacity(64);
            for (label, end) in &view.segments {
                use std::fmt::Write as _;
                let _ = write!(map, "{label}:{end},");
            }
            continuum_base::probe!(
                class = "delib.segment.attribution",
                persona = %self.persona_name,
                cached_tokens = cached as u64,
                broke_in = %frontier,
                cold = cached == 0,
                segments = %map,
                "which prompt segment the KV reuse frontier died in — every miss becomes \
                 an attribution, and the stream is the rent ledger the dream curriculum reads",
            );
        }

        // Verbatim glass box: the EXACT request thread + the raw response. Iteration
        // is always 0 now (single shot); the act→observe driver re-enters this
        // faculty on the NEXT tick with the result folded into perception, and that
        // tick captures itself. Best-effort; never affects the turn.
        if let Some(cap) = self.prompt_capture.as_ref().filter(|_| !lifecycle_recorded) {
            // What was SENT, not the whole registry: the capture is the turn's glass box.
            let offered: Vec<String> = request_template
                .as_ref()
                .and_then(|template| template.tools.as_deref())
                .into_iter()
                .flatten()
                .map(|s| s.name.clone())
                .collect();
            cap.record(
                self.persona_id,
                ws.room_id,
                ws.cause.as_str(),
                0,
                &view.system,
                messages,
                &offered,
                &resp,
            );
        }

        // Record the chain-of-thought into working memory, so next tick the persona
        // resumes its train of thought instead of re-deriving it cold. `reasoning`
        // is `Some` only when thinking is enabled (a `<think>` block the adapter
        // separated); the room only ever saw `resp.text`. Recorded for EVERY verdict
        // shape (act or speak) — acting is thinking too.
        //
        // EXCEPT a silence turn (PASS): re-feeding its self-justification ("nothing
        // new, I should PASS") is what seeds the silence doom-loop the glass box
        // exposed — three stored rationalizations and the mind passes forever, even
        // when directly addressed. Instead record a STAMPED proprioceptive marker
        // (same mechanism `record_action` uses for repeated acts): the mind perceives
        // THAT it has been quiet, and — via the monotonic stamp — how many turns
        // running, which lets it notice the pattern and break out. The inverse of
        // reading fresh reasons to keep passing.
        // ([[no-hardcoded-heuristics-to-steer-cognition]])
        if let Some(wm) = &self.working_memory {
            if crate::cognition::prompt_assembly::looks_like_silence_token(&resp.text) {
                wm.record_fact("chose silence — said nothing to the room");
            } else if let Some(reasoning) = &resp.reasoning {
                wm.record(reasoning);
            }
        }

        // AN EMPTY COMPLETION IS NOT A CHOSEN SILENCE (the `Err` arm's missing twin).
        //
        // The `Err` arm above refuses to let a FAILED model collapse into a serene
        // `Pass` — [[fallbacks-are-illegal-fail-loud]]. But a lane can also answer
        // `Ok` with NOTHING, and that walked straight past the guard and settled as
        // an ordinary non-Act. Two live shapes, both measured 2026-08-16:
        //
        //  * the server generated tokens that never reached `content` — Qwen3.8 under
        //    `--jinja` opens `<think>`, and an unclosed block leaves `extract_reasoning`
        //    branch (3) with empty text and the whole tail as reasoning (#181). Direct
        //    probe against the live lane: 70-token prompt, `finish_reason: length`,
        //    `completion_tokens: 16`, `content: ""`.
        //  * the lane returned 0 tokens in AND out in 28ms (`finish_reason: stop`) —
        //    Solenne's capture on the turn her benchmark run died.
        //
        // Cost of laundering it: `agent/solve` reads "she chose not to act" → acts=0,
        // empty patch → the whole run voids as an INFRA VOID after three attempts; a
        // LIVE citizen reads it as a silent turn, which is indistinguishable from
        // withdrawal. Measured across every capture on disk: 47 of 862 responses
        // (5.5%) are empty-text, spread over ~19 citizens — and the all-empty column
        // is exactly the citizens whose "I've been repetitive, I'll remain silent"
        // turns are the standing round-killer (#390/#414). They were not withdrawing.
        // Nothing came back, and the substrate wrote it down as a choice.
        //
        // SCOPE: a native tool turn legitimately carries empty content, so ToolUse and
        // any present tool_calls are excluded — this fires only when the turn yields
        // no text, no tool call, and therefore nothing to act or speak with. The
        // reasoning tail rides on the fault so the receipt says WHICH shape it was:
        // thought-but-committed-nothing, or the lane returned void.
        // ONLY when there is nothing left to recover. This guard shipped (ce82f00ff)
        // gating on "empty text + no tool call" alone, which is the EXACT precondition
        // of the two recovery paths below — `persona.act.reasoning_lift` (a tool call
        // sitting in the reasoning tail gets lifted and executed) and
        // `persona.act.think_only` (#181's teacher sentinel, which hands her another
        // generation starting from her own conclusions). Faulting first made both
        // unreachable: measured live 2026-08-17, 40 `delib.empty_completion` faults
        // against 87 turn starts while three SWE runs sat at the same act count for
        // 1,357s. The recovery already existed; the guard was standing in front of it.
        //
        // So a REASONING-BEARING empty is not a fault — it is #181, and it has an
        // owner. Fault only for the genuinely unrecoverable shapes: the lane returned
        // void (no text, no reasoning, no call), or she has no hands for the sentinel
        // to teach through. Both are still surfaced rather than read as chosen silence,
        // which is what this guard is for ([[a-perception-FACT-is-honesty]]).
        let nothing_to_recover = resp
            .reasoning
            .as_deref()
            .is_none_or(|r| r.trim().is_empty())
            || self.tools.is_empty();
        if resp.text.trim().is_empty()
            && !matches!(resp.finish_reason, FinishReason::ToolUse)
            && resp.tool_calls.as_ref().is_none_or(|c| c.is_empty())
            && nothing_to_recover
        {
            let reasoning_tokens = resp.reasoning.as_ref().map_or(0, |r| r.len());
            let why = if reasoning_tokens > 0 {
                format!(
                    "the model produced {reasoning_tokens} chars of REASONING and committed \
                     no answer (finish_reason {:?}) — an unclosed think-block or a budget \
                     exhausted mid-thought, never a decision to stay silent",
                    resp.finish_reason
                )
            } else {
                format!(
                    "the lane returned an EMPTY completion with no reasoning and no tool call \
                     (finish_reason {:?}) — nothing was generated, never a decision to stay \
                     silent",
                    resp.finish_reason
                )
            };
            tracing::warn!(
                persona = %self.persona_name,
                finish_reason = ?resp.finish_reason,
                reasoning_chars = reasoning_tokens,
                gen_await_ms,
                "empty completion surfaced as a FAULT (not a silent Pass)"
            );
            continuum_base::probe!(
                class = "delib.empty_completion",
                persona = %self.persona_name,
                reasoning_chars = reasoning_tokens,
                gen_await_ms,
                "the lane answered with nothing — surfacing a fault so it can never be \
                 read as chosen silence"
            );
            return Some(Contribution::deliberation_fault(why));
        }
        // TRUNCATED-WITH-TEXT is NOT an utterance (2026-08-24, Joel: "why is
        // this amazing LLM not finishing things or quitting — something in
        // plumbing always"). A generation that ended at the OUTPUT LIMIT with
        // no tool call is a thought cut mid-stream — measured: 16,384-token
        // emissions (the ceiling, exactly) whose text then flowed through the
        // Speak path and SETTLED GRADED TURNS on a half-sentence. The empty
        // case above already faults; the with-text case fell through. Same
        // treatment: a fault (the #386 bounded in-place retry re-samples —
        // under lived sampling a fresh draw rarely spirals identically), never
        // a candidate answer. Her cut text rides the fault head so the glass
        // box shows what she was mid-way through.
        if matches!(resp.finish_reason, FinishReason::Length)
            && resp.tool_calls.as_ref().is_none_or(|c| c.is_empty())
            && !resp.text.trim().is_empty()
        {
            continuum_base::probe!(
                class = "delib.truncated_not_an_answer",
                persona = %self.persona_name,
                text_chars = resp.text.len(),
                "generation ended AT the output limit with no tool call — a cut                  thought, faulted for re-sample, never a gradeable utterance"
            );
            // A MIND THAT CAN ACT gets a wake-up record, not a silent re-sample (Kimi,
            // 2026-09-25: both her cuts landed mid-envelope on a large ledger note; the
            // re-sample began with no word of the cut, so she composed the same payload
            // again, and later turns replayed work that had already landed). Record the
            // fact of the cut and what she was composing into working memory, then route
            // the cut sentinel: reported, never executed, and `drive_to_settle` gives
            // her another generation that starts from that record. A speak-only mind
            // keeps the fault path below.
            if !self.tools.is_empty() {
                if let Some(wm) = &self.working_memory {
                    wm.record_fact(&cut_act_fact(&resp.text));
                }
                let call = crate::ai::types::ToolCall {
                    id: "tool-attempt-cut-at-limit".to_string(),
                    name: crate::cognition::tool_executor::command_executor::CUT_AT_LIMIT_SENTINEL
                        .to_string(),
                    input: serde_json::json!({}),
                };
                return Some(self.act_verdict(vec![call], &resp, ws.activity_room_id));
            }
            return Some(Contribution::deliberation_fault(format!(
                "the model hit the output limit mid-thought with no committed action \
                 (finish_reason Length, {} bytes, response {}) — a cut thought is not an answer",
                resp.text.len(), resp.request_id
            )));
        }

        // Did she choose to act? Two shapes, both → `Decision::Act`:
        //  (a) the adapter returned a native tool-use turn (FinishReason::ToolUse);
        //  (b) the model emitted a tool call as JSON in its prose (small models that
        //      ignore the native tool channel) — strictly better than dropping it.
        if !self.tools.is_empty() {
            if matches!(resp.finish_reason, FinishReason::ToolUse) {
                let mut calls = resp.tool_calls.clone().unwrap_or_default();
                // Wire dialect → canonical command names, BEFORE the
                // authorization gate and the executor ever see them (the
                // reverse half of the offer-side rename above).
                for c in &mut calls {
                    crate::ai::tool_dialect::normalize_call(c);
                }
                if let Some(v) = self.yield_verdict(&calls) {
                    return Some(v);
                }
                if !calls.is_empty() {
                    return Some(self.act_verdict(calls, &resp, ws.activity_room_id));
                }
            }
            if let Some(mut call) = crate::ai::json_in_prompt_tools::parse_tool_call(&resp.text) {
                if let Some(v) = self.yield_verdict(std::slice::from_ref(&call)) {
                    return Some(v);
                }
                // Same wire-dialect mapping as the native path above (#159): a model
                // that narrates `write_file(…)` / `list_files(…)` — its trained
                // OpenHands vocabulary — must resolve to `code/write` / `code/list`,
                // not silently no-op as an unknown name. The text-lift path skipped
                // this, so narrated snake_case verbs died while the SAME name in a
                // native tool_call worked. One mapping, both paths.
                crate::ai::tool_dialect::normalize_call(&mut call);
                return Some(self.act_verdict(vec![call], &resp, ws.activity_room_id));
            }
            // #159 fail-loud: she emitted the native `[TOOL_CALLS]` marker but named no
            // valid tool — `[recall]`/`[action]` are receipt vocabulary she MIMICS as a
            // call (#158), not something callable. Silently rendering the bogus attempt
            // as SPEECH gives her zero feedback → she never switches to `code/search` and
            // rambles to the deadline (`acts:0`, glass-boxed 2026-07-16). Route the
            // attempted name as an `Act` so the executor's unknown-command TEACHER fires
            // as an observation and `drive_to_settle` hands her another generation to do
            // it right. Bounded by `max_acts`; an identical repeat short-circuits via
            // `all_calls_already_satisfied`. [[unknown-tool-intent-must-fail-loud]]
            if let Some(attempted) =
                crate::ai::json_in_prompt_tools::attempted_tool_name(&resp.text)
            {
                let call = crate::ai::types::ToolCall {
                    id: "tool-attempt".to_string(),
                    name: crate::ai::tool_dialect::from_wire_name(&attempted).to_string(),
                    input: serde_json::json!({}),
                };
                return Some(self.act_verdict(vec![call], &resp, ws.activity_room_id));
            }
            // The sibling gap the block above cannot see (#159 follow-up): she fenced correct
            // ARGUMENTS and never named the tool, in any liftable position. `attempted_tool_name`
            // gates on the `[TOOL_CALLS]` marker as its first check, and there is no marker here,
            // so this emission produced no call AND no feedback — a silent drop. Measured live
            // 2026-08-07 (Sahar): "I will release the card 392bc54e" + a bare `{"card_id": …}`
            // fence, twice in one turn, then the same shape again next generation. Nothing lifted,
            // correctly — binding prose intent would also fire on peer coaching (#144) — but
            // nothing TAUGHT either, which is the whole reason #159 exists.
            //
            // REPORTED, never executed: we cannot know which tool she meant, and guessing is
            // exactly the false positive the coaching negatives guard. Routing the sentinel makes
            // the executor's teacher fire with the missing-name sentence, and `drive_to_settle`
            // hands her another generation — the same mechanism, extended to the case it missed.
            if let Some(snippet) = crate::ai::json_in_prompt_tools::nameless_args_fence(&resp.text)
            {
                let call = crate::ai::types::ToolCall {
                    id: "tool-attempt-nameless".to_string(),
                    name: crate::cognition::tool_executor::command_executor::NAMELESS_ARGS_SENTINEL
                        .to_string(),
                    input: serde_json::json!({ "emitted": snippet }),
                };
                return Some(self.act_verdict(vec![call], &resp, ws.activity_room_id));
            }
            // Reasoning-channel intent lift (#181 sibling, glass-boxed on the
            // deepseek4 eval battery 2026-08-03): a thinking model commits its act
            // into `reasoning_content` — "I'll read the file: {tool_call…}" — and
            // leaves `content` EMPTY (budget exhausted mid-think, or the act simply
            // landed in the wrong channel). Every check above reads only the content
            // channel, so the turn settled as Speak("") — a guaranteed empty fail
            // (2 of 3 real failures in that battery). When content is empty there is
            // no committed answer to respect, so the reasoning tail is the model's
            // only stated intention: run the SAME format stack over it and lift the
            // LAST parseable call (the final intention — earlier matches may be
            // considered-and-discarded exploration). Strictly gated on empty content:
            // a real Speak or an explicit PASS is a commitment in the answer channel
            // and private deliberation must never override it.
            if resp.text.trim().is_empty() {
                // A thought CUT at the output limit chose nothing: the last call in its tail
                // is what she was weighing when the cap landed, never a commitment. Lift
                // nothing; tell her it was cut and let her decide (BigMama's 5090 receipt,
                // 2026-10-06: a half-formed code/run lifted from a cut 8,504-token think).
                if matches!(resp.finish_reason, FinishReason::Length)
                    && !self.tools.is_empty()
                    && resp.reasoning.as_deref().is_some_and(|r| !r.trim().is_empty())
                {
                    continuum_base::probe!(
                        class = "persona.act.thought_cut",
                        persona = %self.persona_name,
                        reasoning_len = resp.reasoning.as_deref().map_or(0, str::len),
                        "the generation reached the output limit mid-thought: nothing lifted, the decision is hers"
                    );
                    let call = crate::ai::types::ToolCall {
                        id: "tool-attempt-thought-cut".to_string(),
                        name: crate::cognition::tool_executor::command_executor::THOUGHT_CUT_SENTINEL
                            .to_string(),
                        input: serde_json::json!({}),
                    };
                    return Some(self.act_verdict(vec![call], &resp, ws.activity_room_id));
                }
                if let Some(reasoning) = resp.reasoning.as_deref() {
                    if let Some(mut call) =
                        crate::ai::json_in_prompt_tools::parse_tool_calls(reasoning)
                            .into_iter()
                            .last()
                    {
                        crate::ai::tool_dialect::normalize_call(&mut call);
                        continuum_base::probe!(
                            class = "persona.act.reasoning_lift",
                            persona = %self.persona_name,
                            tool = %call.name,
                            "content channel empty — lifted the final tool intention from the reasoning tail"
                        );
                        return Some(self.act_verdict(vec![call], &resp, ws.activity_room_id));
                    }
                    // THINK-ONLY turn (#181 tail, glass-boxed on the 2026-08-15 bench
                    // round): she spent the ENTIRE generation inside the reasoning
                    // channel and stopped — no answer, no PASS, no liftable call
                    // anywhere in the thinking (the captured turn analyzed all 12
                    // tasks lucidly and then just ended). Settling that as an empty
                    // Speak is a silent dead turn: maximal thought, zero commitment,
                    // and the round dies of it. Route the SAME mechanism #159 built
                    // for the sibling cases above: a reported-never-executed sentinel
                    // whose executor teacher names what happened as an observation,
                    // and `drive_to_settle` hands her another generation — which
                    // starts from her own conclusions, because the reasoning was
                    // already recorded into working memory at the top of this fn.
                    // Model-agnostic by construction: `reasoning` is an adapter fact,
                    // never a model sniff. Bounded by `max_acts`; an identical repeat
                    // short-circuits via `all_calls_already_satisfied`.
                    if !reasoning.trim().is_empty() {
                        continuum_base::probe!(
                            class = "persona.act.think_only",
                            persona = %self.persona_name,
                            reasoning_len = reasoning.len(),
                            "generation ended inside the reasoning channel — no answer, no act; routing the think-only teacher"
                        );
                        let call = crate::ai::types::ToolCall {
                            id: "tool-attempt-think-only".to_string(),
                            name: crate::cognition::tool_executor::command_executor::THINK_ONLY_SENTINEL
                                .to_string(),
                            input: serde_json::json!({}),
                        };
                        return Some(self.act_verdict(vec![call], &resp, ws.activity_room_id));
                    }
                }
            }
        }

        // No action chosen → the prose IS the verdict (PASS token → silence, else
        // Speak). The organism settles here.
        Some(self.verdict(&resp, ws))
    }
}
