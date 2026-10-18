//! The prompt view: the whole prompt fitted to the window, and the view callers read.

use super::*;

impl LlmDeliberationFaculty {
    /// The EXACT prompt this faculty sends the model this tick — the system
    /// prompt (identity + assembled RAG context + how-to-participate + silence
    /// affordance) and the user burst. Exposed so what the LLM sees is trivially
    /// introspectable at every turn (tests, replay, operator tooling). The RAG is
    /// the load-bearing input; it must never be opaque.
    ///
    /// Fits the estimated request to the served window, including selected tool
    /// schemas and the planned completion reserve. Identity framing, complete
    /// current inputs/results, and source-declared minimum standing context are
    /// retained first; insufficient capacity produces an explicit fault. Optional
    /// history then fits by whole, quantized front eviction. Additional source
    /// units and recall use the remaining room. Requiredness is independent of
    /// system/trailing cache placement; this estimate is not a backend token count.
    pub fn prompt_view(&self, ws: &Workspace) -> DeliberationPromptView {
        // Introspection / test entry: size against the CURRENT served window. The
        // production path (`contribute`) instead loads the binding ONCE and calls
        // `prompt_view_within` with that window, so prompt sizing and the request's
        // model come from the same atomic snapshot (no torn read across a re-home).
        let binding = self.binding.load();
        let (window, calibration) = self.prompt_fit(&binding);
        self.prompt_view_with_feedback(ws, window, calibration, binding.model.as_deref())
    }

    pub(super) fn prompt_fit(&self, binding: &ModelBinding) -> (u32, PromptCalibration) {
        let feedback = self.prompt_feedback.load();
        feedback.as_ref().filter(|f| f.applies_to(binding)).map_or(
            (binding.context_window, PromptCalibration::default()),
            |f| (binding.context_window.min(f.available), f.calibration),
        )
    }

    /// Fixture view against an explicit window, without a prior provider observation.
    #[cfg(test)]
    pub(super) fn prompt_view_within(&self, ws: &Workspace, context_window: u32) -> DeliberationPromptView {
        self.prompt_view_with_feedback(ws, context_window, PromptCalibration::default(), self.binding.load().model.as_deref())
    }

    /// Every reply carries the engine's own count of the prompt it was sent: retain that
    /// count over this turn's estimate as the density the NEXT prompt is fitted with, on
    /// this binding, for any input. Refusal-only calibration never learned the case that
    /// cost Kimi one turn in four on 2026-10-06: the bytes/3 estimate undercounted her
    /// prompt by 18-23% (engine 65,607 against 55,603 estimated), the prompt still FIT
    /// the slot, so nothing was refused, and the reply's reserve was eaten by the prompt
    /// itself: generation hit the slot after ~950 tokens and the turn ended mid-thought.
    /// A ratio below one changes nothing (`charge` never underprices), and each reply
    /// replaces the last, so one unusually dense turn shapes only the turn after it.
    pub(super) fn learn_prompt_density(
        &self,
        binding: &Arc<ModelBinding>,
        view: &DeliberationPromptView,
        engine_prompt_tokens: u32,
        available: u32,
    ) {
        let Some(calibration) =
            PromptCalibration::observed(view.estimated_prompt_tokens, engine_prompt_tokens, None)
        else {
            return; // the lane reported no count: nothing measured, nothing retained
        };
        self.prompt_feedback.store(Some(Arc::new(PromptFeedback {
            binding: Arc::clone(binding),
            calibration,
            available,
        })));
        crate::probe!(
            class = "delib.prompt.density",
            persona = %self.persona_name,
            estimated_prompt_tokens = view.estimated_prompt_tokens,
            engine_prompt_tokens,
            "the engine's count of this prompt, retained as the next prompt's density"
        );
    }

    /// Apply observed token density to the same standing-grounding plan.
    fn prompt_view_with_feedback(
        &self,
        ws: &Workspace,
        context_window: u32,
        calibration: PromptCalibration,
        model: Option<&str>,
    ) -> DeliberationPromptView {
        self.prompt_view_with_visual_feedback(ws, context_window, calibration, None, model)
    }

    pub(super) fn prompt_view_with_visual_feedback(
        &self,
        ws: &Workspace,
        context_window: u32,
        calibration: PromptCalibration,
        visual: Option<&ActiveVisualFeedback>,
        model: Option<&str>,
    ) -> DeliberationPromptView {
        // THE REPLY'S CLAIM ([`reply_claim`]): what she is expected to write, at the
        // priority this turn gives it. It competes in the window's one allocation below;
        // its grant travels with the view into `build_request_within` as `max_tokens`.
        let pending_peak = self.working_memory.as_ref()
            .map_or(0, |wm| wm.pending_emission_peak(ws.activity_room_id));
        let (reply, reply_runaway) = reply_claim(
            self.turn_kind(ws),
            self.working_set.as_ref().and_then(|reg| reg.need_of(self.persona_id)),
            pending_peak,
            Self::holds_live_work(ws),
            ws.directed_at_self(),
        );
        let desired_reply = reply.max_tokens as usize;

        // The NATIVE tool schemas ride the served window too: the gateway injects
        // each function spec (name + description + schema) via the chat template,
        // outside `system`/`user`. Without counting them the budget silently
        // overshoots `n_ctx` and llama-server 400s ("exceeds context size").
        //
        // This is NOT "a few dozen tokens" — that sentence described the two-tool
        // discovery pair and has been false since #206 restored the full native
        // surface (~a dozen tools, offered whole, deliberately: a native-tool model
        // can only call what it was offered, and the amputated surface stranded it
        // in a `commands/help` loop). Measured 2026-08-06 it is **~4,609 tokens** —
        // 28% of a 16,384 window, before framing or a single message. It is now on
        // the `delib.context.render` probe as `tool_tokens` rather than described,
        // because this comment being wrong is what cost three commits of chasing a
        // phantom accounting gap.
        //
        // Note the tool surface is paid TWICE in different forms: these schemas
        // here, AND the human-readable tool MENU inside `compose_system` (counted in
        // `framing_tokens`). Whether that duplication is intended is an open
        // question, not something this comment should assert either way.
        // The SELECTED surface, not the whole registry: `contribute` narrows to her
        // hands on a work turn, and subtracting the full registry here trimmed the
        // conversation to fit room that was never occupied (card dec1a7ff). One
        // decision, made once, consulted by the budget and by the request alike.
        let selected = self.select_tool_surface(ws, context_window);

        // Which tool categories the bookmarked menu OPENS this turn. Computed ONCE
        // here from `ws` (NOT from the budgeted `context`, which is not built yet) and
        // threaded IDENTICALLY into both `compose_system` calls below — so the
        // framing-token ESTIMATE (empty context) and the FINAL render (real context)
        // carry the exact same menu, and the budget math cannot under-count the tool
        // block and overshoot `n_ctx`. See [`Self::expanded_categories`].
        let expanded = self.expanded_categories(ws);

        // The framing wrapper alone (no assembled context) — essential + small.
        // Pass the SAME directedness + self-initiation + expansion as the final
        // compose below so the framing-token estimate matches the prompt actually sent
        // (both the silence block and the [Your own time] block are gated and add a
        // few dozen tokens each).
        let holds_live_work = Self::holds_live_work(ws);
        let framing_tokens = Self::framing_cost(
            &self
                .compose_system_split(
                    "",
                    &expanded,
                    ws.directed_at_self(),
                    ws.self_initiated,
                    ws.now_ms,
                    holds_live_work,
                )
                .stable,
        );

        // The volatile framing (clock + own-time/presence) rides the FACTS phase
        // of the conversation — flicker-class content, before the ask — never the
        // system message it invalidated on every act (see the split note below).
        let trailing_framing = deliberation_prompt::compose_trailing(
            ws.now_ms,
            ws.self_initiated,
            ws.directed_at_self(),
            holds_live_work,
        );
        let mut all_messages = self.messages_with_visual_feedback(
            ws,
            (!trailing_framing.is_empty()).then_some(trailing_framing.as_str()),
            visual,
        );

        // Price the untruncated trailing sources separately: messages_unfitted
        // leaves them borrowed until the same plan has reserved their exact units.
        let input_identity = all_messages.input_identity;
        let calibration = if calibration
            .input_identity
            .is_some_and(|key| key != input_identity)
        {
            PromptCalibration::default()
        } else {
            calibration
        };
        let unfitted_message_tokens = all_messages.tokens()
            + ws.broadcast
                .iter()
                .filter(|c| c.decision.is_none() && c.trailing && c.standing_grounding)
                .map(|c| GroundingPiece::whole(c).tokens)
                .sum::<usize>();
        // KEEP THE OVERFLOW. This used to be one chained `saturating_sub` ending in
        // `- all_messages.required_tokens()`, and the saturation is where the diagnosis
        // died: "over by 31,000" and "exactly at the line" both arrive as `0`, so the one
        // number that says HOW BADLY the turn is over its window was destroyed at the
        // moment it was computed. Subtract in two steps and probe the remainder.
        let required_payload = all_messages.required_tokens();
        let after_reserves = calibration
            .prompt_budget(
                (context_window as usize).saturating_sub(Self::COMPLETION_FLOOR_TOKENS as usize),
            )
            .saturating_sub(framing_tokens)
            .saturating_sub(selected.tokens);
        let grounding_budget = after_reserves.saturating_sub(required_payload);
        let payload_overflow = required_payload.saturating_sub(after_reserves);
        if payload_overflow > 0 {
            crate::probe!(
                class = "delib.payload.over_window",
                persona = %self.persona_name,
                required_payload,
                after_reserves,
                payload_overflow,
                context_window,
                framing_tokens,
                tool_tokens = selected.tokens,
                "the turn's IRREDUCIBLE payload does not fit the served window — grounding                  gets nothing, and this is the amount by which the window is short"
            );
        }
        // GROUNDING MAY NEVER MUTE A CITIZEN.
        //
        // `GroundingPlan::minimum` used to return `Err` here, and that error was promoted
        // ahead of every other capacity check (`grounding_error.or(capacity_error)`) into a
        // hard `deliberation_fault`. So a citizen whose ENRICHMENT would not fit refused the
        // whole turn — no model call, no act, zero `model_ms` — and, because she keeps her
        // held card while refusing, she can hold a benchmark card forever and never work it.
        //
        // Measured live 2026-09-09 on Kimi, holding `hard-rs/expr_eval`: served window
        // 28,160, conversation 39,532, and every 15 s a `work_turn` that produced
        // `act=0 model_ms=0` with `required 613 > budget 0`. Six hundred and thirteen tokens
        // of roster + workspace-map + working-memory, against a 28k window, decided she
        // could not think at all.
        //
        // Grounding is enrichment. When it does not fit, the honest move is to go without it
        // and think anyway, not to fall silent — a partial world beats a dead mind. Nothing
        // is deleted that the real guards would have kept: the conversation fitter still
        // refuses when the IRREDUCIBLE payload cannot fit (`fit_messages`), and the wire
        // check still refuses when the assembled request exceeds the window. Only the
        // enrichment's own shortfall stops being fatal.
        let mut grounding = match GroundingPlan::within_budget(ws, grounding_budget) {
            Fitted::Whole(plan) => plan,
            Fitted::Reduced { plan, wanted } => {
                crate::probe!(
                    class = "delib.grounding.shed",
                    persona = %self.persona_name,
                    wanted_tokens = wanted,
                    budget_tokens = grounding_budget,
                    kept_tokens = plan.system_reserve() + plan.trailing_tokens(),
                    "standing grounding did not fit its budget — thinking with less                      enrichment rather than refusing the turn"
                );
                plan
            }
        };
        all_messages.grounding_tokens = grounding.trailing_tokens();
        let ctx_floor = if grounding.pieces.is_empty() {
            // Preserve the existing optional-enrichment allowance when no standing
            // source bid. This soft floor confers no requiredness on recall or on
            // cache placement; standing sources above are never capped by a ratio.
            let offered = || {
                ws.broadcast
                    .iter()
                    .filter(|c| c.decision.is_none() && !c.trailing)
            };
            let cost =
                |c: &Contribution| Self::context_piece_tokens(c.faculty.as_str(), c.content.len());
            // Against the reply's FLOOR: its desire is negotiated below, in the same
            // allocation as this floor, never pre-charged here.
            let after_framing = calibration
                .prompt_budget((context_window as usize).saturating_sub(reply.floor_tokens as usize))
                .saturating_sub(selected.tokens)
                .saturating_sub(framing_tokens);
            offered()
                .find(|c| c.faculty.as_str() == crate::persona::active_work_source::SOURCE_ID)
                .map(cost)
                .or_else(|| offered().map(cost).min())
                .map(|tokens| tokens + Self::working_context_header_cost())
                .unwrap_or(0) // No optional source bid means no enrichment reservation.
                .min(after_framing / 2)
        } else {
            grounding.system_reserve()
        };

        // ONE ALLOCATION FOR THE WHOLE WINDOW. Owner's rule: "Flexbox is the right way.
        // It's like the governor: if you don't use it for literally everything it doesn't
        // work." Every claimant on the served window bids in engine-calibrated tokens —
        // framing + the selected tools and the required input (required, never traded),
        // standing grounding, optional history, and the REPLY — and the one flexbox
        // algorithm decides ([`crate::persona::rag_budget::flex_allocate`]). The reply was
        // carved off the top by its own rule and then yielded in two steps; on a writing
        // turn that cut Benchy's first write mid-call (2026-10-18 02:10Z). Now it trades
        // against history and grounding by priority and never takes from required input.
        //
        // LATENCY-BUDGETED FILL (act-latency law + compression ladder): a bigger served
        // window is only a gift if its content earns the prefill time. When the honest
        // kv-rate fix grew slots to ~134k, the demand-derived budgets happily filled them
        // — prompts ballooned 19-24k → 24-43k in an afternoon and five concurrent 35k
        // prefills serialized acts to 339-888s (measured 2026-09-01, the worst of the day,
        // WITH the best hit rates). So history's claim is capped by a time target — and
        // the tokens that target buys are THIS node's, never a constant's:
        // `PREFILL_TARGET_SECONDS × the measured prefill rate`
        // (`prefill_rate::latency_fill_cap`, the #4283 ladder: fresh → stale → the box's
        // most conservative curve → a named floor).
        let (derived_fill_cap, fill_rate) = Self::latency_fill_cap(
            model, crate::inference::prefill_rate::measured_rate_for,
        );
        // A LATENCY BUDGET MAY NOT BUY ITSELF A REPEATED ACT. The cap exists to bound
        // time-to-first-token; clipping away her own last move and its result costs a
        // WHOLE EXTRA TURN when she re-issues it, so history's FLOOR is what that evidence
        // costs — ahead of every optional claim, the reply's desire included. `fit_messages`
        // then refuses to evict it; this floor is what makes that refusal satisfiable.
        let evidence = all_messages.recent_move_evidence();
        let latency_fill_cap = derived_fill_cap.max(evidence.tokens);
        let required_tokens = all_messages.required_tokens();
        let optional_history = all_messages.tokens().saturating_sub(required_tokens);
        // A quiet self-tick (no required input) still sends her newest whole message.
        let quiet_minimum = if required_tokens == 0 {
            all_messages.history.last().map_or(0, Self::message_cost) // map_or: no history = nothing to keep
        } else {
            0
        };
        let history_floor = evidence.tokens.max(quiet_minimum).min(optional_history);
        let claim = |tokens: usize| calibration.charge(tokens).min(u32::MAX as usize) as u32;
        let fixed_cost = claim(framing_tokens.saturating_add(selected.tokens));
        let required_cost = claim(required_tokens);
        let mut reply = reply;
        // The reply never takes from required content: its floor is what the window
        // leaves after framing, tools and the required input.
        reply.floor_tokens = reply
            .floor_tokens
            .min(context_window.saturating_sub(fixed_cost.saturating_add(required_cost)));
        reply.min_tokens = reply.floor_tokens;
        let fixed_claim = |id: &str, tokens: u32| crate::persona::rag_budget::RagSourceBudget {
            source_id: id.to_owned(),
            priority: 10,
            floor_tokens: tokens,
            min_tokens: tokens,
            max_tokens: tokens,
            required: true,
        };
        let grounding_floor = claim(ctx_floor);
        let history_floor = claim(history_floor);
        let claims = [
            fixed_claim(FLEX_FRAMING, fixed_cost),
            fixed_claim(FLEX_REQUIRED, required_cost),
            crate::persona::rag_budget::RagSourceBudget {
                source_id: FLEX_GROUNDING.to_owned(),
                priority: 6,
                floor_tokens: grounding_floor,
                min_tokens: grounding_floor,
                // What grounding wants: every offered context piece and, when there is
                // one, its header. Nothing offered wants nothing.
                max_tokens: claim(
                    match Self::assembled_context_cost(ws) {
                        0 => 0,
                        offered => offered.saturating_add(Self::working_context_header_cost()),
                    }
                    .max(ctx_floor),
                ),
                required: false,
            },
            crate::persona::rag_budget::RagSourceBudget {
                source_id: FLEX_HISTORY.to_owned(),
                priority: 5,
                floor_tokens: history_floor,
                min_tokens: history_floor,
                max_tokens: claim(latency_fill_cap.min(optional_history)).max(history_floor),
                required: false,
            },
            reply,
        ];
        let plan = crate::persona::rag_budget::flex_allocate(context_window, &claims);
        // Whatever no claim took is hers to write with, up to the runaway bound.
        let reply_grant = plan.granted(FLEX_REPLY);
        let completion_reserve = reply_grant
            .saturating_add(plan.unallocated)
            .min(reply_runaway)
            .max(1) as usize;
        crate::probe!(
            class = "delib.window.flex",
            persona = %self.persona_name,
            context_window,
            framing_tokens = fixed_cost,
            required_tokens = required_cost,
            grounding_granted = plan.granted(FLEX_GROUNDING),
            history_granted = plan.granted(FLEX_HISTORY),
            reply_priority = claims[4].priority,
            reply_floor = claims[4].floor_tokens,
            reply_wanted = claims[4].max_tokens,
            reply_granted = reply_grant,
            unallocated = plan.unallocated,
            reply_runaway,
            allowance = completion_reserve,
            escalation = plan.escalation_needed,
            "the served window's one allocation — framing, required input, grounding, \
             history and the reply by priority; the allowance is the reply's grant plus \
             what no claim took, under its runaway bound"
        );
        // Grounding is optional: when its floor did not fit, it holds no room the
        // required input then has to do without.
        let ctx_floor = if plan.granted(FLEX_GROUNDING) < grounding_floor { 0 } else { ctx_floor };
        let after_framing = calibration
            .prompt_budget((context_window as usize).saturating_sub(completion_reserve))
            .saturating_sub(selected.tokens)
            .saturating_sub(framing_tokens);
        let window_msg_budget = after_framing.saturating_sub(ctx_floor);
        let fill_budget = required_tokens.saturating_add(latency_fill_cap);
        if window_msg_budget > fill_budget {
            let shed = all_messages.shed_preview(latency_fill_cap);
            crate::probe!(
                class = "delib.fill.latency_capped",
                persona = %self.persona_name,
                window_budget = window_msg_budget,
                cap = latency_fill_cap,
                // A clip is never again invisible: the rate that produced it and where
                // that rate came from ride the same row. `rate_source="none"` means the
                // named floor governed and nothing was measured on this box.
                rate_source = fill_rate.source.label(),
                measured_prefill_tps = fill_rate.tps.unwrap_or(0.0), // unwrap_or: 0.0 on the receipt means NO rate — rate_source says "none" and the floor governed
                target_seconds = PREFILL_TARGET_SECONDS,
                derived_cap = derived_fill_cap,
                // WHAT THE CLIP COSTS HER, in kind and amount — so "she repeated
                // herself" is traceable to "we cut the evidence she had already done
                // it" without anyone opening a capture file. At-least figures: the real
                // drop rounds up to a quantum.
                shed_messages = shed.messages,
                shed_tokens = shed.tokens,
                shed_own_moves = shed.own_moves,
                shed_move_results = shed.results,
                // …and what it is structurally forbidden to cost her.
                kept_recent_move_messages = evidence.messages,
                kept_recent_move_tokens = evidence.tokens,
                // 0 messages = her last move is further back than the scan bound and is
                // NOT protected; `truncated` = only a suffix of a longer chain is. The
                // protection's limits ride its own receipt.
                kept_recent_move_truncated = evidence.truncated,
                "conversation fill capped by the act-latency target at THIS node's \
                 measured prefill rate — window headroom is reserve for content that \
                 earns its prefill, never default fill; her own last move and its \
                 result are held out of the drop",
            );
        }
        // History gets its grant; the required input is never part of the trade.
        let msg_budget = window_msg_budget.min(
            required_tokens
                .saturating_add(calibration.prompt_budget(plan.granted(FLEX_HISTORY) as usize)),
        );

        // WHAT THIS TURN WOULD HAVE COST WITH NO BUDGET — recorded before either
        // fitting step throws the evidence away, and deliberately free to exceed
        // `context_window`. That excess IS the signal: it is the only way a mind held
        // at a too-small window can ever report that it needed a bigger one, and the
        // measurement `serving_plan` provisions from (see [`crate::cognition::working_set`]).
        //
        // Measuring the prompt we actually SEND instead would measure the clamp — a
        // citizen capped at 8192 fills 8192, so a p95 of what-was-sent re-derives the
        // cap that produced it and freezes it forever. Every term below is therefore
        // the UNTRUNCATED one.
        let demand_tokens = calibration
            .charge(
                framing_tokens
                    .saturating_add(unfitted_message_tokens)
                    .saturating_add(Self::assembled_context_cost(ws))
                    .saturating_add(Self::working_context_header_cost())
                    // Demand includes the full registry only when capacity withheld it.
                    .saturating_add(selected.demand_tokens(self.describe_tool_tokens())),
            )
            // Output is already measured in provider tokens; calibrate only input.
            .saturating_add(desired_reply);
        if let Some(reg) = &self.working_set {
            reg.record(
                self.persona_id,
                demand_tokens.min(u32::MAX as usize) as u32,
                // `now_ms` is the workspace's own clock when the cycle stamped one;
                // 0 when it didn't. The registry keeps peaks, not a time series, so an
                // unstamped cycle still contributes its measurement honestly.
                ws.now_ms.unwrap_or(0), // JUSTIFIED unwrap_or: unstamped cycle still measures honestly (registry keeps peaks, not a time series)
            );
        }
        crate::probe!(
            class = "delib.turn.demand",
            persona = %self.persona_name,
            demand_tokens,
            context_window,
            framing_tokens,
            conversation_tokens = unfitted_message_tokens,
            grounding_tokens = Self::assembled_context_cost(ws),
            completion_reserve,
            // The honest headline: >1.0 means this turn wanted more window than it had,
            // and by how much. This is the number that decides whether the served
            // window is provisioned for the work or for a constant.
            over_window = demand_tokens as f32 / (context_window.max(1) as f32),
            "turn demand vs served window"
        );

        let protocol_error = all_messages.active_protocol_error();
        let (mut fitted, capacity_error) = match self.fit_messages(all_messages, msg_budget) {
            Ok(fitted) => (fitted, None),
            Err(error) => (FittedMessages::default(), Some(error)),
        };
        // THE FIT'S RECEIPT (card d33e928a, Kimi's finding): whenever the fit dropped
        // history, say what it dropped and from what budget, and say it loudly when
        // nothing optional survived — that turn composes with no room at all, operator
        // direction included, and before this there was no line to read it from.
        if capacity_error.is_none() && fitted.receipt.dropped_messages > 0 {
            let r = fitted.receipt;
            crate::probe!(
                class = if r.emptied() { "delib.fill.history_emptied" } else { "delib.fill.history_dropped" },
                persona = %self.persona_name,
                context_window,
                completion_reserve,
                msg_budget,
                required_tokens = r.required_tokens,
                history_budget = r.history_budget,
                history_messages = r.history_messages,
                history_tokens = r.history_tokens,
                kept_messages = r.kept_messages,
                dropped_messages = r.dropped_messages,
                dropped_tokens = r.dropped_tokens,
                quantum = r.quantum,
                "the fit dropped optional history from the front; history_emptied means nothing \
                 optional survived and the turn composes with no room messages at all"
            );
        }
        // WHAT THIS TURN ACTUALLY SENDS — the post-fit size the served window must
        // hold. Recorded beside the untrimmed demand above: that one is the growth
        // signal (and the upper bound), this one is what a slot must FIT. The planner
        // sizes the window from this with headroom (`ServingDemand::window_target`),
        // so a 25k working set no longer provisions a 137k slot (2026-09-13).
        if let Some(reg) = &self.working_set {
            let sent = framing_tokens
                .saturating_add(Self::messages_cost(&fitted.messages))
                .saturating_add(desired_reply);
            // At a window that TRIMMED this turn the post-fit size is the window's
            // echo, not the turn's — the untrimmed demand is what the plan must hear
            // (`sent_sample_for_plan`; the 5090's self-sealed 2048, 2026-09-20).
            let sample = crate::cognition::working_set::WorkingSetRegistry::sent_sample_for_plan(
                demand_tokens.min(u32::MAX as usize) as u32,
                sent.min(u32::MAX as usize) as u32,
                context_window,
            );
            reg.record_sent(
                self.persona_id,
                sample,
                ws.now_ms.unwrap_or(0), // JUSTIFIED unwrap_or: unstamped cycle still measures honestly
            );
            // HER NEED — the allocator's gate (card 70706a9e). Only a turn the fit HELD
            // (the truthful minimum fitted, and something beyond the framing survived)
            // measures what a lane must hold for her; a starved or emptied turn says
            // nothing about her and is not recorded, so the need can never be a
            // window's echo. The wish above stays the target the allocator grows toward.
            let sent_tokens = sent.min(u32::MAX as usize) as u32;
            let framing = framing_tokens.min(u32::MAX as usize) as u32;
            let fit_ok = capacity_error.is_none();
            match crate::cognition::working_set::need_sample(sent_tokens, framing, fit_ok) {
                Some(need) => reg.record_need(
                    self.persona_id,
                    need,
                    ws.now_ms.unwrap_or(0), // JUSTIFIED unwrap_or: unstamped cycle still measures honestly
                ),
                // THE UNHELD TURN IS SAID, not skipped (card 39822816): a mind the
                // allocator labels `wish:` after a deploy carrying #4325 must be
                // distinguishable, from the ledger alone, between "every turn was
                // starved", "every turn emptied to the framing", and "this seam was
                // never reached". One row per unheld turn, with the reason.
                None => crate::probe!(
                    class = "persona.need.unheld",
                    persona = %self.persona_name,
                    sent_tokens,
                    framing_tokens = framing,
                    fit_ok,
                    reason = if fit_ok { "framing_only" } else { "starved" },
                    "this turn measured no need — not held, so the wish stays her gate"
                ),
            }
        }
        // Only the source's truthful minimum outranks optional conversation.
        // Additional declared list units use actual leftover room after fitting;
        // a large list cannot turn every detail into mandatory input.
        if capacity_error.is_none() {
            grounding
                .expand_within(after_framing.saturating_sub(Self::messages_cost(&fitted.messages)));
        }
        let trailing_grounding = grounding.trailing_messages();
        fitted.refit.grounding_len = trailing_grounding.len();
        fitted.messages.splice(
            fitted.grounding_at..fitted.grounding_at,
            trailing_grounding,
        );
        let messages = fitted.messages;

        // Whatever remains after framing + conversation goes to enrichment
        // context. The framing estimate above was taken with an EMPTY context,
        // where `working_context_block`'s wrapper header is absent — so the
        // moment any context renders, the system prompt grows by that header
        // too. Charge the context budget for it up front, or the final prompt
        // systematically exceeds the estimate by ~50 tokens (masked by
        // rounding slop until the tool-menu example grew the prompt to the
        // budget edge — glass-boxed 2026-07-13, llama-server 400 territory).
        let used_msg_tokens = Self::messages_cost(&messages);
        // Grounding gets everything the conversation did not actually use — never
        // less than the floor reserved above.
        let ctx_budget = after_framing
            .saturating_sub(used_msg_tokens)
            .saturating_sub(Self::working_context_header_cost())
            .max(ctx_floor.saturating_sub(Self::working_context_header_cost()));
        let context = self.render_assembled_context_within(
            ws,
            ctx_budget,
            &grounding,
            ContextComposition {
                context_window,
                framing_tokens,
                conversation_tokens: used_msg_tokens,
                ctx_floor,
                after_framing,
                tool_tokens: selected.tokens,
            },
        );

        // #266 KV-cache fix, THE SPLIT ACTUALLY APPLIED. The split composer
        // existed and its own docs called it "the live path" — but this view
        // was still built with `compose_system_holding` (stable ++ trailing in
        // ONE system message), so the [now] minute clock and the presence/
        // own-time flip kept churning the system prefix at char ~8.4k. Wire-
        // capture diff 2026-09-01: consecutive turns diverged exactly at
        // `[now …]` / `[Conversational Presence]`→`[Your own time]`, with the
        // server's reuse proven perfect the same hour (same prompt ×2 on one
        // slot → cache_n 397/401). A prefix that mutates every act caches
        // nothing; hit_rate was 0.0 fleet-wide with every persona on her OWN
        // slot ([[a-mutating-system-prompt-destroys-kv-reuse-for-everything-after-it]]).
        //
        // Now: `stable` (identity + tools + grounding context) IS the system
        // message; `trailing` (clock + presence framing) rides as the NEWEST
        // user turn — same text, nearest generation, zero prefix invalidation.
        // Same delivery the `.trailing()` contributions already use and test.
        // The trailing half was already rendered into the FACTS phase of the
        // conversation by `messages_unfitted` (before the ask, parrot order
        // preserved); only the byte-stable half may touch the system message.
        let system = self
            .compose_system_split(
                &context,
                &expanded,
                ws.directed_at_self(),
                ws.self_initiated,
                ws.now_ms,
                holds_live_work,
            )
            .stable;
        // This is the native request estimate, not a backend tokenizer count.
        // Adapters may add model-specific framing; live acceptance still checks
        // their captured request/usage rather than assuming this estimate exact.
        let estimated_prompt_tokens = Self::framing_cost(&system)
            .saturating_add(Self::messages_cost(&messages))
            .saturating_add(selected.tokens);
        let wire_tokens = calibration
            .charge(estimated_prompt_tokens)
            .saturating_add(completion_reserve);
        let capacity_error = capacity_error.or_else(|| {
            (wire_tokens > context_window as usize).then_some(PromptCapacityError {
                required_tokens: wire_tokens,
                budget_tokens: context_window as usize,
            })
        });
        let segments = segment_map(&system, &messages);
        DeliberationPromptView {
            refit: fitted.refit,
            system,
            input_identity,
            estimated_prompt_tokens,
            completion_reserve: completion_reserve as u32,
            capacity_error,
            protocol_error,
            messages,
            segments,
        }
    }

    /// Which tool categories open (list their verbs inline) this turn — now ALL of them.
    /// The full tool surface is always shown; relevance-scored progressive disclosure was
    /// a weak-model crutch that hid a capable persona's own hands (see the body). Empty
    /// tool set → empty (no menu at all).
    pub(super) fn expanded_categories(&self, _ws: &Workspace) -> BTreeSet<String> {
        if self.tools.is_empty() {
            return BTreeSet::new();
        }
        // Show her the FULL tool surface — EVERY category's verbs inline, not a collapsed
        // bookmark she has to guess she can open. Hiding tools behind relevance-scored
        // progressive disclosure was a crutch for a weak model, and it CRIPPLES a capable
        // one: she can't reach for what she can't see, so she says "I can't execute that"
        // about a tool she was authorized for all along. A competent peer sees all her
        // hands. Category names + arg names are cheap tokens (not the typed schemas —
        // those still load on demand via commands/help). [[write-cognition-as-a-parent-above-lowered-expectations]]
        persona_tools::group_categories(&self.tools)
            .into_iter()
            .map(|(cat, _)| cat.to_string())
            .collect()
    }
}

/// The required prompt input cannot be represented in this window.
/// No partial payload is submitted: the existing fault and demand paths expose
/// the capacity problem without recording it as a citizen's decision to pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptCapacityError {
    pub required_tokens: usize,
    pub budget_tokens: usize,
}

impl std::fmt::Display for PromptCapacityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "complete prompt input requires {} tokens, but only {} are available",
            self.required_tokens, self.budget_tokens
        )
    }
}

/// A snapshot of exactly what the deliberation faculty sends the model — the
/// glass box over the RAG/prompt. Print it, capture it, diff it across turns.
#[derive(Debug, Clone)]
pub struct DeliberationPromptView {
    // Source-owned boundaries over this exact assembled snapshot, never re-inferred
    // from prose or refreshed from working memory during corrective admission.
    pub(super) refit: PromptRefit,
    pub system: String,
    pub(super) input_identity: [u8; 32],
    /// Cached pre-calibration cost, including selected schemas and framing.
    pub estimated_prompt_tokens: usize,
    /// The reply's allowance this plan granted (its flex grant plus what no other claim
    /// took, under its runaway bound) — charged against the window here, and carried
    /// verbatim as the wire request's `max_tokens`.
    pub completion_reserve: u32,
    pub capacity_error: Option<PromptCapacityError>,
    pub protocol_error: Option<String>,
    /// The role-attributed conversation thread sent to the model — the persona's
    /// OWN posts as `assistant`, peers' as `user` (PERSONA-COGNITION-PIPELINE §7.5).
    /// Replaces the single flat `user` string that collapsed her own turns into the
    /// conversation and caused the identity bleed / transcript replay / echo loop.
    pub messages: Vec<ChatMessage>,
    /// THE RENT LEDGER'S SPINE (compression-ladder rung 2): the prompt's segment
    /// runs in send order, each `(label, cumulative estimated-token END)`. After
    /// generation, the server's reused-prefix length (`cached_tokens`) lands
    /// somewhere in this map, and the run containing that frontier NAMES the
    /// segment that broke reuse — turning every cache miss from a number into an
    /// attribution, and accumulating each segment's true daily cost (its rent).
    /// Estimated tokens (chars/4 class), so attribution is segment-scale
    /// (thousands of tokens), never byte-exact — which is all promotion needs.
    pub segments: Vec<(&'static str, u32)>,
}

impl DeliberationPromptView {
    /// The conversation rendered as flat text — the role-tagged thread joined for
    /// human-readable introspection (glass-box `eprintln!`, debug logs, assertions).
    /// The canonical form the model receives is [`messages`](Self::messages); this
    /// is a projection for eyes, not the wire payload.
    pub fn user_text(&self) -> String {
        self.messages
            .iter()
            .map(|m| format!("[{}] {}", m.role, m.content_text()))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
