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
        self.prompt_view_with_feedback(ws, window, calibration)
    }

    /// The window her prompt is fitted to: her served slot, which the serving plan computes on
    /// her node from that engine's measured KV capacity and memory, narrowed by the provider's
    /// last observation. No machine's number stands in for it: a knee replayed on one box's
    /// captures (49,152, the M5, 2026-10-28) was that box's constant on every other.
    pub fn prompt_fit(&self, binding: &ModelBinding) -> (u32, PromptCalibration) {
        let feedback = self.prompt_feedback.load();
        feedback.as_ref().filter(|f| f.applies_to(binding)).map_or(
            (binding.context_window, PromptCalibration::default()),
            |f| (binding.context_window.min(f.available), f.calibration),
        )
    }

    /// Fixture view against an explicit window, without a prior provider observation.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn prompt_view_within(&self, ws: &Workspace, context_window: u32) -> DeliberationPromptView {
        self.prompt_view_with_feedback(ws, context_window, PromptCalibration::default())
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
    pub(in super::super) fn learn_prompt_density(
        &self,
        binding: &Arc<ModelBinding>,
        view: &DeliberationPromptView,
        engine_prompt_tokens: u32,
        engine_cached_tokens: u32,
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
        continuum_base::probe!(
            class = "delib.prompt.density",
            persona = %self.persona_name,
            estimated_prompt_tokens = view.estimated_prompt_tokens,
            engine_prompt_tokens,
            // What the engine reused of it: her view's measure of success.
            engine_cached_tokens,
            "the engine's count of this prompt, retained as the next prompt's density"
        );
    }

    /// Apply observed token density to the same standing-grounding plan.
    fn prompt_view_with_feedback(
        &self,
        ws: &Workspace,
        context_window: u32,
        calibration: PromptCalibration,
    ) -> DeliberationPromptView {
        self.prompt_view_with_visual_feedback(ws, context_window, calibration, None)
    }

    pub fn prompt_view_with_visual_feedback(
        &self,
        ws: &Workspace,
        context_window: u32,
        calibration: PromptCalibration,
        visual: Option<&ActiveVisualFeedback>,
    ) -> DeliberationPromptView {
        // THE REPLY'S CLAIM ([`reply_claim`]): what she is expected to write, at the
        // priority this turn gives it. It competes in the window's one allocation below;
        // its grant travels with the view into `build_request_within` as `max_tokens`.
        let pending_peak = self.working_memory.as_ref()
            .map_or(0, |wm| wm.pending_emission_peak(ws.activity_room_id));
        let reply = reply_claim(
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
        // Her hands are a claim in the one allocation below (`tools_epoch.rs`): before it,
        // only their floor is irreducible; the surface sent is what the grant buys.
        let tools = self.tools_claim(ws);

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
                    &expanded,
                    ws.directed_at_self(),
                    ws.self_initiated,
                    ws.now_ms,
                    holds_live_work,
                )
                .stable,
        );

        // The contracts in force (own time, presence) are fixed text: each enters her view
        // as a standing block, once per epoch, and stays cached. Only the clock and the line
        // naming what is in force ride the fresh tail, so the ask stays last (the parrot fix)
        // without re-sending the bodies every act (context validator, 2026-10-25: 8.7% of an
        // act's prefill on the M5's captures).
        let contracts = deliberation_prompt::contracts_in_force(ws.self_initiated, ws.directed_at_self(), holds_live_work);
        let trailing_framing = deliberation_prompt::compose_live_framing(ws.now_ms, &contracts);
        let mut all_messages = self.messages_with_visual_feedback(
            ws,
            (!trailing_framing.is_empty()).then_some(trailing_framing.as_str()),
            visual,
        );
        all_messages.streams.blocks.extend(contracts.into_iter().map(|contract| {
            crate::cognition::context_streams::blocks::Grounding::atomic(self.persona_id, contract.name, contract.body)
        }));
        // Her view leads the history: this act appends to it, and everything after it (the
        // standing grounding, facts, the clock) is this act's fresh tail.
        let view = self.place_view(&mut all_messages);

        // Price the untruncated trailing sources separately: messages_unfitted
        // leaves them borrowed until the same plan has reserved their exact units.
        let input_identity = all_messages.input_identity;
        let calibration = calibration.for_input(input_identity);
        let after_reserves = calibration
            .prompt_budget(
                (context_window as usize).saturating_sub(Self::COMPLETION_FLOOR_TOKENS as usize),
            )
            .saturating_sub(framing_tokens)
            .saturating_sub(tools.floor);
        // Her view's REQUIRED sources (notices, the lines of files she has open) are never shed:
        // granted their floor first, or kept over their grant (`fit_view`). That floor is
        // therefore as irreducible as the turn's own input, and what is left for the rest is
        // what remains after it. Kimi on the windows 5090 (2026-10-28, refusal 1eef970d): her
        // newest result was priced against the window less framing and tools alone, fit there,
        // stayed whole, and the view's floor then took its room: required 33,719 > 30,243,
        // refused before sending with no reduction offered.
        let view_floor: usize = self
            .view_claims(&all_messages.streams)
            .iter()
            .filter(|claim| claim.required)
            .map(|claim| claim.floor_tokens as usize)
            .sum();
        let irreducible_room = after_reserves.saturating_sub(view_floor);
        // Her view already carries every block that rides the conversation. Measured BEFORE her
        // newest result is fitted: demand is the sum of wants (FOCUS-CONTRACT rule 5), so a result
        // shown at a smaller level still asks for its whole, or the window never grows for it.
        let unfitted_message_tokens = all_messages.tokens();
        // Her newest result alone over what the window leaves it is shown at a smaller level
        // with its handle, never dropped (goal 7); one that fits is untouched.
        let newest_result_reduced = self.fit_newest_result(&mut all_messages, irreducible_room);
        // KEEP THE OVERFLOW. This used to be one chained `saturating_sub` ending in
        // `- all_messages.required_tokens()`, and the saturation is where the diagnosis
        // died: "over by 31,000" and "exactly at the line" both arrive as `0`, so the one
        // number that says HOW BADLY the turn is over its window was destroyed at the
        // moment it was computed. Subtract in two steps and probe the remainder.
        let required = all_messages.required_parts();
        let newest_result_tokens = required.latest_result;
        let required_payload = required.total();
        let grounding_budget = irreducible_room.saturating_sub(required_payload);
        let payload_overflow = required_payload.saturating_sub(irreducible_room);
        if payload_overflow > 0 {
            continuum_base::probe!(
                class = "delib.payload.over_window",
                persona = %self.persona_name,
                required_payload,
                after_reserves,
                view_floor_tokens = view_floor,
                payload_overflow,
                context_window,
                framing_tokens,
                tool_tokens = tools.floor,
                stimulus_tokens = required.stimulus,
                action_statement_tokens = required.action_statement,
                latest_result_tokens = required.latest_result,
                latest_results = required.latest_result_count,
                grounding_tokens = required.grounding,
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
                continuum_base::probe!(
                    class = "delib.grounding.shed",
                    persona = %self.persona_name,
                    wanted_tokens = wanted,
                    budget_tokens = grounding_budget,
                    kept_tokens = plan.context_reserve() + plan.trailing_tokens(),
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
                .saturating_sub(tools.floor)
                .saturating_sub(framing_tokens);
            offered()
                .find(|c| c.faculty.as_str() == crate::cognition::faculty_ids::ACTIVE_WORK)
                .map(cost)
                .or_else(|| offered().map(cost).min())
                .map(|tokens| tokens + Self::working_context_header_cost())
                .unwrap_or(0) // No optional source bid means no enrichment reservation.
                .min(after_framing / 2)
        } else {
            grounding.context_reserve()
        };

        // ONE ALLOCATION FOR THE WHOLE WINDOW. Owner's rule: "Flexbox is the right way.
        // It's like the governor: if you don't use it for literally everything it doesn't
        // work." Every claimant on the served window bids in engine-calibrated tokens —
        // framing + the selected tools and the required input (required, never traded),
        // standing grounding, optional history, and the REPLY — and the one flexbox
        // algorithm decides ([`crate::cognition::rag_budget::flex_allocate`]). The reply was
        // carved off the top by its own rule and then yielded in two steps; on a writing
        // turn that cut Benchy's first write mid-call (2026-10-18 02:10Z). Now it trades
        // against history and grounding by priority and never takes from required input.
        //
        // Her own last move and its result are history's FLOOR, ahead of every optional
        // claim, the reply's desire included: clipping them costs a WHOLE EXTRA TURN when she
        // re-issues the act. `fit_messages` refuses to evict them; this floor is what makes
        // that refusal satisfiable.
        let evidence = all_messages.recent_move_evidence();
        let required_tokens = all_messages.required_tokens();
        // Her view's sources claim for themselves below; the history claim is this act's
        // fresh optional tail (facts, the clock, demoted payloads).
        let view_tokens = Self::messages_cost(&all_messages.history[..view.len()]);
        let optional_history = all_messages
            .tokens()
            .saturating_sub(required_tokens)
            .saturating_sub(view_tokens);
        // A quiet self-tick (no required input) still sends her newest whole message.
        let quiet_minimum = if required_tokens == 0 {
            all_messages.history.last().map_or(0, Self::message_cost) // map_or: no history = nothing to keep
        } else {
            0
        };
        let history_floor = evidence.tokens.max(quiet_minimum).min(optional_history);
        let claim = |tokens: usize| calibration.charge(tokens).min(u32::MAX as usize) as u32;
        let fixed_cost = claim(framing_tokens);
        let tools_floor = claim(tools.floor);
        let required_cost = claim(required_tokens);
        let mut reply = reply;
        // The reply never takes from required content: its floor is what the window leaves
        // after framing, tools and the required input. Its UNCLIPPED floor is her need.
        let reply_need = reply.floor_tokens;
        reply.floor_tokens = reply_need.min(context_window.saturating_sub(fixed_cost.saturating_add(tools_floor).saturating_add(required_cost)));
        reply.min_tokens = reply.floor_tokens;
        let fixed_claim = |id: &str, tokens: u32| crate::cognition::rag_budget::RagSourceBudget {
            source_id: id.to_owned(),
            priority: 10,
            floor_tokens: tokens,
            min_tokens: tokens,
            max_tokens: tokens,
            required: true,
        };
        let grounding_floor = claim(ctx_floor);
        let history_floor = claim(history_floor);
        let mut claims = vec![
            fixed_claim(FLEX_FRAMING, fixed_cost),
            fixed_claim(FLEX_REQUIRED, required_cost),
            crate::cognition::rag_budget::RagSourceBudget {
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
            crate::cognition::rag_budget::RagSourceBudget {
                source_id: FLEX_HISTORY.to_owned(),
                priority: 5,
                floor_tokens: history_floor,
                min_tokens: history_floor,
                max_tokens: claim(optional_history).max(history_floor),
                required: false,
            },
            reply,
            // Her hands, above grounding (`tools_epoch.rs`): floor = core hands, want = her whole
            // surface, and priority decides between them like every other claim. The level this
            // epoch chose is never a minimum: re-priced after a refusal scaled the calibration it
            // took two thirds of the window ahead of her whole conversation (22,167 of 29,440,
            // a_scaled_calibration_after_turnovers). A grant below it is a surface change the
            // epoch_level_lost probe reports.
            crate::cognition::rag_budget::RagSourceBudget {
                source_id: FLEX_TOOLS.to_owned(),
                priority: 7,
                floor_tokens: tools_floor,
                min_tokens: tools_floor,
                max_tokens: claim(tools.want).max(tools_floor),
                required: true,
            },
        ];
        // Her view's sources, each its own claim (`epoch.rs`): the blocks she works from at
        // the grounding claim's weight, her conversation, thoughts and results at history's.
        claims.extend(self.view_claims(&all_messages.streams).into_iter().map(|mut source| {
            source.floor_tokens = claim(source.floor_tokens as usize);
            source.min_tokens = source.floor_tokens;
            source.max_tokens = claim(source.max_tokens as usize).max(source.floor_tokens);
            source
        }));
        let plan = crate::cognition::rag_budget::flex_allocate(context_window, &claims);
        let allocation = Some(crate::ai::provenance::AllocationReceipt {
            window: context_window,
            revision: env!("CONTINUUM_BUILD_GIT_SHA").to_owned(),
            claims: claims
                .iter()
                .map(|claim| crate::ai::provenance::ClaimGrant {
                    source: claim.source_id.clone(),
                    priority: claim.priority,
                    floor: claim.floor_tokens,
                    want: claim.max_tokens,
                    grant: plan.granted(&claim.source_id),
                })
                .collect(),
        });
        let tools_grant = calibration.prompt_budget(plan.granted(FLEX_TOOLS) as usize);
        let (tools_choice, tools_refusal) = self.granted_tools(ws, tools, tools_grant);
        let budget_floor = crate::cognition::rag_budget::turn_need(&claims).saturating_sub(claims.iter().find(|c| c.source_id == FLEX_REPLY).map_or(0, |c| c.floor_tokens)).saturating_add(reply_need);
        // Her reply's grant is her measured need from the one allocation, carried as `max_tokens`.
        // What no claim took is her view's to grow into by appends (`fit_view`), never the
        // reply's: a cap claimed above her context starved her reads (ONE-BUDGET slice 6).
        let reply_grant = plan.granted(FLEX_REPLY);
        let completion_reserve = reply_grant.max(1) as usize;
        continuum_base::probe!(
            class = "delib.window.flex",
            persona = %self.persona_name,
            context_window,
            framing_tokens = fixed_cost,
            required_tokens = required_cost,
            grounding_granted = plan.granted(FLEX_GROUNDING),
            history_granted = plan.granted(FLEX_HISTORY),
            tools_granted = plan.granted(FLEX_TOOLS),
            tools_floor,
            reply_priority = claims[4].priority,
            reply_floor = claims[4].floor_tokens,
            reply_wanted = claims[4].max_tokens,
            reply_granted = reply_grant,
            unallocated = plan.unallocated,
            reserve = completion_reserve,
            escalation = plan.escalation_needed,
            "the served window's one allocation: framing, required input, tools, grounding, \
             her view and the reply by priority; the reply reserves its measured need and what \
             no claim took is her view's room"
        );
        // Grounding is optional: when its floor did not fit, it holds no room the
        // required input then has to do without.
        let ctx_floor = if plan.granted(FLEX_GROUNDING) < grounding_floor { 0 } else { ctx_floor };
        let after_framing = calibration
            .prompt_budget((context_window as usize).saturating_sub(completion_reserve))
            .saturating_sub(tools_choice.sent.tokens)
            .saturating_sub(framing_tokens);
        let window_msg_budget = after_framing.saturating_sub(ctx_floor);
        // History and her view get their grants; the required input is never part of the trade.
        let view_granted: u32 = plan
            .allocations
            .iter()
            .filter(|a| claims.iter().skip(6).any(|c| c.source_id == a.source_id))
            .map(|a| a.allocated_tokens)
            .sum();
        // What no claim took is room her view grows into by appends (`fit_view`), and the fit's
        // budget holds that same room, or it would drop what the epoch was allowed to keep.
        let view_room = plan.unallocated;
        let msg_budget = window_msg_budget.min(required_tokens.saturating_add(calibration.prompt_budget(
            plan.granted(FLEX_HISTORY).saturating_add(view_granted).saturating_add(view_room) as usize,
        )));
        // Her view within what the allocation granted its sources (`epoch.rs`): the only place
        // it is ever collapsed.
        // A broken allocator contract refuses the turn (`budget_fault`), never a prompt sent anyway.
        let view_fault = self
            .fit_view(&mut all_messages, view, &plan, view_room as usize, calibration)
            .err()
            .map(|mismatch| format!("her view could not be sized: {mismatch}"));
        self.view_sent();
        // The surface this act sends: the epoch's set, or this act's own when it turned the view over.
        let selected = self.retain_tools(tools_choice, tools_refusal.is_some());
        // Her view as the allocation sized it: the fit below never cuts it.
        let view_len = all_messages.grounding_at;

        // WHAT THIS TURN WOULD HAVE COST WITH NO BUDGET — recorded before either
        // fitting step throws the evidence away, and deliberately free to exceed
        // `context_window`. That excess IS the signal: it is the only way a mind held
        // at a too-small window can ever report that it needed a bigger one, and the
        // measurement `serving_plan` provisions from (see [`super::super::super::working_set`]).
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
                    // Demand includes her whole hands only when the window withheld the extended ones.
                    .saturating_add(selected.demand_tokens()),
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
        continuum_base::probe!(
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
        // HER VIEW IS NOT THE FIT'S TO CUT. The allocation sized it once (`fit_view`); the fit
        // sees only the fresh tail and the required input, within their own grants. Handed
        // the view too, it evicted from the front, where the view leads, to make room for a
        // tail whose floor the allocation had not granted: a head clip inside a prompt that
        // fit the window (capacity_and_wire fixtures: 1 and 3 view messages, 2026-10-23).
        let view_messages: Vec<ChatMessage> = all_messages.history.drain(..view_len).collect();
        all_messages.grounding_at = all_messages.grounding_at.saturating_sub(view_len);
        let (view_pointers, tail_pointers): (Vec<_>, Vec<_>) =
            std::mem::take(&mut all_messages.result_pointers).into_iter().partition(|(index, _)| *index < view_len);
        all_messages.result_pointers = tail_pointers.into_iter().map(|(index, pointer)| (index - view_len, pointer)).collect();
        let tail_budget = msg_budget.saturating_sub(Self::messages_cost(&view_messages));
        let (mut fitted, capacity_error) = match self.fit_messages(all_messages, tail_budget) {
            Ok(mut fitted) => {
                fitted.messages.splice(0..0, view_messages);
                fitted.grounding_at += view_len;
                fitted.refit.grounding_at += view_len;
                fitted.refit.history_len += view_len;
                for (index, _) in &mut fitted.refit.result_pointers {
                    *index += view_len;
                }
                fitted.refit.result_pointers.splice(0..0, view_pointers);
                (fitted, None)
            }
            Err(error) => (FittedMessages::default(), Some(error)),
        };
        // Her hands not granted even their floor refuse the turn as loudly as her input would.
        let capacity_error = tools_refusal.or(capacity_error);
        // THE FIT'S RECEIPT (card d33e928a, Kimi's finding): whenever the fit dropped
        // history, say what it dropped and from what budget, and say it loudly when
        // nothing optional survived — that turn composes with no room at all, operator
        // direction included, and before this there was no line to read it from.
        if capacity_error.is_none() && fitted.receipt.dropped_messages > 0 {
            let r = fitted.receipt;
            continuum_base::probe!(
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
            // HER NEED, the allocator's gate (card 70706a9e): the BUDGETER'S FLOOR on a held
            // turn, never its post-fit size, which the flexbox grows to any window (the M5,
            // 10-18: ~48.6k on a 102k lane refused a second lane; 4 residents on 1 slot).
            let sent_tokens = sent.min(u32::MAX as usize) as u32;
            let framing = framing_tokens.min(u32::MAX as usize) as u32;
            let fit_ok = capacity_error.is_none();
            match crate::cognition::working_set::need_sample(sent_tokens, framing, fit_ok).map(|_| budget_floor) {
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
                None => continuum_base::probe!(
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
        let FittedMessages { mut refit, mut messages, grounding_at, .. } = fitted;

        // Whatever remains after framing + conversation goes to enrichment
        // context. Rendered, it is one more message opened by a header — so the
        // moment any context renders, the prompt grows by that header and its
        // message wrapper too. Charge the context budget for both up front, or the
        // final prompt systematically exceeds the estimate by ~50 tokens (masked by
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
                surface_reason: selected.reason,
            },
        );
        // A ROOM'S CONTEXT NEVER ENTERS THE SYSTEM MESSAGE (2026-10-23). It rendered
        // there, after her identity and tools, so every room switch rewrote the end of
        // the system message and the engine re-prefilled her whole view behind it
        // (pages keyed per room restored ~7.4k tokens across a switch on the M5). It is
        // a room's, so it rides the fresh tail after her view, with the grounding the
        // refit keeps frozen; grounding sources themselves are her view's blocks.
        if let Some(block) = deliberation_prompt::working_context_message(&context) {
            messages.insert(grounding_at + refit.grounding_len, ChatMessage::text("user", block));
            refit.grounding_len += 1;
        }

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
        // Now: `stable` (tools + identity + turn contract, no room content) IS the
        // system message; `trailing` (clock + presence framing) rides as the NEWEST
        // user turn — same text, nearest generation, zero prefix invalidation.
        // Same delivery the `.trailing()` contributions already use and test.
        // The trailing half was already rendered into the FACTS phase of the
        // conversation by `messages_unfitted` (before the ask, parrot order
        // preserved); only the byte-stable half may touch the system message.
        let system = self
            .compose_system_split(
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
        // THE ONE BUDGET'S INVARIANT (`budget.rs`), at the one place the messages are final.
        // A required input that cannot fit is a capacity refusal, decided by the fit above.
        let budget_fault = if let Some(fault) = view_fault {
            Some(fault)
        } else if capacity_error.is_none() {
            self.budget_violation(&BudgetLedger {
                context_window,
                calibration,
                completion_reserve,
                system: &system,
                tool_tokens: selected.tokens,
                messages: &messages,
                view_len: grounding_at,
                grounding_len: refit.grounding_len,
                framing_grant: fixed_cost.saturating_add(plan.granted(FLEX_TOOLS)),
                required_grant: required_cost,
                history_grant: plan.granted(FLEX_HISTORY),
                grounding_grant: plan.granted(FLEX_GROUNDING),
                view_grant: view_granted.saturating_add(view_room),
            })
        } else {
            None
        };
        let segments = segment_map(&system, &messages);
        DeliberationPromptView {
            refit,
            system,
            input_identity,
            estimated_prompt_tokens,
            completion_reserve: completion_reserve as u32,
            capacity_error,
            budget_fault,
            protocol_error,
            messages,
            segments,
            newest_result_reduced,
            newest_result_tokens,
            allocation,
            tools: selected.into_owned(),
            tools_grant,
        }
    }

    /// Which tool categories open (list their verbs inline) this turn — now ALL of them.
    /// The full tool surface is always shown; relevance-scored progressive disclosure was
    /// a weak-model crutch that hid a capable persona's own hands (see the body). Empty
    /// tool set → empty (no menu at all).
    pub fn expanded_categories(&self, _ws: &Workspace) -> BTreeSet<String> {
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


    /// The conversation as it stands BEFORE any budget is applied — every turn, every
    /// perception fact, every trailing proprioception block, in final render order.
    ///
    /// Split out from the fitting step so a turn's TRUE conversational size is
    /// knowable. Fitting is lossy by design (newest-first, oldest dropped), and once
    /// it has run the question "how much did this turn actually want?" is
    /// unanswerable — which is precisely why the served window had to be sized by a
    /// constant instead of by demand. [`super::super::super::working_set`] measures this.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn messages_unfitted(&self, ws: &Workspace, trailing_framing: Option<&str>) -> PromptMessages {
        let mut prompt = self.messages_with_visual_feedback(ws, trailing_framing, None);
        let whole = self.whole_view(&prompt.streams);
        prompt.grounding_at = whole.len();
        prompt.history.splice(0..0, whole);
        prompt
    }
}
