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

    /// The window her prompt is fitted to: her served window, narrowed only by the provider's
    /// last observation. No fixed cap below the slot: a 49k "working window" (2026-10-28) cut
    /// prefill but left her re-reading what fell out of view (2026-11-02, today's reads: Aiko
    /// 92% re-reads, Aris 75%, Benchy 69%, Atlas 54%).
    pub fn prompt_fit(&self, binding: &ModelBinding) -> (u32, PromptCalibration) {
        self.fit_within(binding, binding.context_window)
    }

    fn fit_within(&self, binding: &ModelBinding, window: u32) -> (u32, PromptCalibration) {
        let feedback = self.prompt_feedback.load();
        feedback.as_ref().filter(|f| f.applies_to(binding)).map_or(
            (window, PromptCalibration::default()),
            |f| (window.min(f.available), f.calibration),
        )
    }

    /// The window this faculty's turn is fitted to (its binding's served window): the shares
    /// sized from the window (results, room updates, the tool surface) scale with it.
    pub(in super::super) fn turn_window(&self) -> u32 {
        self.binding.load().context_window
    }

    /// Fixture view against an explicit window, without a prior provider observation.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn prompt_view_within(&self, ws: &Workspace, context_window: u32) -> DeliberationPromptView {
        self.prompt_view_with_feedback(ws, context_window, PromptCalibration::default())
    }

    /// Every reply carries the engine's own count of the prompt it was sent: retain that
    /// count over this turn's estimate in her measured [`Density`] on this binding, whose bound
    /// (the densest recent ratio) the NEXT prompt is fitted at, for any input. Both ways:
    /// refusal-only calibration never learned the case that cost Kimi one turn in four on
    /// 2026-10-06 (the bytes/3 estimate undercounted her prompt by 18-23%, engine 65,607 against
    /// 55,603 estimated, so the prompt ate the reply's reserve), and a ratio below one was
    /// ignored before (the M5, 2026-11-04, n=131: the guard read above the engine's count by 14% on
    /// average, up to 22%; an estimate correction, not a measured gain). `served` is the model the
    /// engine reported serving this reply: a ring never crosses a model change.
    pub(in super::super) fn learn_prompt_density(
        &self,
        binding: &Arc<ModelBinding>,
        view: &DeliberationPromptView,
        engine_prompt_tokens: u32,
        engine_cached_tokens: u32,
        available: u32,
        served: &str,
    ) {
        let Some((density, calibration, reading)) =
            self.density_on(binding).observe(view.estimated_prompt_tokens, engine_prompt_tokens, served)
        else {
            return; // the lane reported no count: nothing measured, nothing retained
        };
        self.prompt_feedback.store(Some(Arc::new(PromptFeedback {
            binding: Arc::clone(binding),
            calibration,
            available,
            density,
        })));
        continuum_base::probe!(
            class = "delib.prompt.density",
            persona = %self.persona_name,
            estimated_prompt_tokens = view.estimated_prompt_tokens,
            engine_prompt_tokens,
            // What the engine reused of it: her view's measure of success.
            engine_cached_tokens,
            // Engine tokens per estimated token, thousandths: the ring's mean, and the bound the
            // next prompt is priced at.
            density_mean_milli = reading.mean_milli,
            density_bound_milli = reading.bound_milli,
            "the engine's count of this prompt, retained in her density; the next prompt is fitted at its bound"
        );
    }

    /// Her measured density on `binding`: the ring her last feedback carried when it applies to
    /// this binding, else none (a new model, adapter or window measures afresh).
    pub(in super::super) fn density_on(&self, binding: &ModelBinding) -> Density {
        let feedback = self.prompt_feedback.load();
        match feedback.as_ref() {
            Some(feedback) if feedback.applies_to(binding) => feedback.density.clone(),
            _ => Density::default(),
        }
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
        let kind = self.turn_kind(ws);
        let time_term = kind.time_term(crate::inference::decode_knee::rate_for(self.binding.load().requested_model()).tps);
        let reply = reply_claim(
            kind,
            self.working_set.as_ref().and_then(|reg| reg.need_of(self.persona_id)),
            pending_peak,
            Self::holds_live_work(ws),
            ws.directed_at_self(),
            time_term,
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

        let input_identity = all_messages.input_identity;
        let calibration = calibration.for_input(input_identity);
        // Her view already carries every block that rides the conversation. Measured BEFORE her
        // newest result is levelled: demand is the sum of wants (FOCUS-CONTRACT rule 5), so a
        // result shown at a smaller level still asks for its whole, or the window never grows for it.
        let unfitted_message_tokens = all_messages.tokens();

        // THE ACT'S ONE ALLOCATION (ONE-ACT stage 2, `continuum_context::allocation`). Owner's rule:
        // "Flexbox is the right way. It's like the governor: if you don't use it for literally
        // everything it doesn't work." Every part of her prompt is a claim in it, in the
        // allocation's unit (engine-calibrated tokens), and it runs ONCE: framing, the required
        // input, her newest result (the largest of its levels that fits beside every other floor), her
        // hands, the standing grounding, the fresh history, the reply and each source of her view.
        // Nothing is sized beside it: the room her newest result gets, the grounding's budget and
        // her view's start are all its grants or arithmetic on them. (Before: three side budgets
        // and three allocator runs an act; Kimi on the windows 5090, 2026-10-28, refusal 1eef970d,
        // was one of them disagreeing with another: her result priced against the window less
        // framing and tools, her view's floor then taking its room.)
        let claim = |tokens: usize| calibration.charge(tokens).min(u32::MAX as usize) as u32;
        let required = all_messages.required_parts();
        let newest_result_tokens = required.latest_result;
        // The required input other than her newest result (which claims for itself) and other
        // than the standing grounding (enrichment, its own optional claim): the stimulus and her
        // action statement.
        let required_tokens = required.stimulus.saturating_add(required.action_statement);
        let newest_levels = self.newest_result_levels(&all_messages);
        // GROUNDING MAY NEVER MUTE A CITIZEN (Kimi, 2026-09-09: 613 tokens of roster and map on a
        // 28k window refused every turn). It is an optional claim with no floor: its min is the
        // standing sources' truthful minimums, its want everything offered, and its grant is the
        // budget it is fitted to below, shedding rather than failing.
        let standing = GroundingPlan::minimum(ws);
        let grounding_min = if standing.pieces.is_empty() {
            // No standing source bid: the optional enrichment's own smallest piece (the active
            // work's, else the smallest offered) and its header. No reservation without a bid.
            let offered = || ws.broadcast.iter().filter(|c| c.decision.is_none() && !c.trailing);
            let cost = |c: &Contribution| Self::context_piece_tokens(c.faculty.as_str(), c.content.len());
            offered()
                .find(|c| c.faculty.as_str() == crate::cognition::faculty_ids::ACTIVE_WORK)
                .map(cost)
                .or_else(|| offered().map(cost).min())
                .map_or(0, |tokens| tokens + Self::working_context_header_cost()) // map_or: no optional source bid, no enrichment reservation
        } else {
            standing.context_reserve() + standing.trailing_tokens()
        };
        let grounding_want = match Self::assembled_context_cost(ws) {
            0 => 0,
            offered => offered.saturating_add(Self::working_context_header_cost()),
        };
        // Her own last move and its result are a REQUIRED claim of their own (`last_move`, goal 7:
        // the newest complete interaction is whole): clipping them costs a WHOLE EXTRA TURN when
        // she re-issues the act. The rest of this act's fresh tail (older acts, facts, the clock,
        // demoted payloads) is the optional `history` claim and sheds first. Her view's sources
        // claim for themselves.
        let evidence = all_messages.recent_move_evidence();
        let view_tokens = Self::messages_cost(&all_messages.history[..view.len()]);
        let optional_history = all_messages.tokens().saturating_sub(required.total()).saturating_sub(view_tokens);
        // A quiet self-tick (no required input) still sends her newest whole message.
        let quiet_minimum = if required.total() == 0 {
            all_messages.history.last().map_or(0, Self::message_cost) // map_or: no history = nothing to keep
        } else {
            0
        };
        let history_floor = evidence.tokens.max(quiet_minimum).min(optional_history);
        // Her view's sources, each its own claim (`epoch.rs`), priced in the allocation's unit.
        // On an act her view's FLOOR (each source at its collapsed minimum: handles and fold blocks)
        // is required: she acts on what she sees, and her view only appends (FIFO), so a view shed
        // to make room for her reply's want would be re-prefilled whole the act after, worst on a
        // tight window. The reply gets what the required floors leave; past the window that is the
        // typed IrreducibleShortfall (Cormac on #5779). On a pass her view's optional sources shed
        // first as before.
        let acting = kind == TurnKind::Act;
        let view_claims: Vec<_> = self.view_claims(&all_messages.streams).into_iter().map(|mut source| {
            source.floor_tokens = claim(source.floor_tokens as usize);
            source.min_tokens = source.floor_tokens;
            source.max_tokens = claim(source.max_tokens as usize).max(source.floor_tokens);
            source.required = source.required || acting;
            source
        }).collect();
        let reply_need = reply.floor_tokens;
        let act = continuum_context::allocation::allocate(
            context_window,
            continuum_context::allocation::ActClaims {
                framing: claim(framing_tokens),
                required: claim(required_tokens),
                newest: continuum_context::allocation::Newest { levels: newest_levels.iter().map(|&cost| claim(cost)).collect() },
                tools: continuum_context::allocation::Span { floor: claim(tools.floor), min: claim(tools.floor), want: claim(tools.want) },
                grounding: continuum_context::allocation::Span { floor: 0, min: claim(grounding_min), want: claim(grounding_want.max(grounding_min)) },
                last_move: claim(history_floor),
                history: continuum_context::allocation::Span { floor: 0, min: 0, want: claim(optional_history.saturating_sub(history_floor)) },
                reply,
                view: view_claims,
            },
        );
        use continuum_context::allocation::id as claim_id;
        let allocation = Some(crate::ai::provenance::AllocationReceipt {
            window: context_window,
            revision: env!("CONTINUUM_BUILD_GIT_SHA").to_owned(),
            claims: act
                .claims()
                .iter()
                .map(|claim| crate::ai::provenance::ClaimGrant {
                    source: claim.source_id.clone(),
                    priority: claim.priority,
                    floor: claim.floor_tokens,
                    want: claim.max_tokens,
                    grant: act.granted(&claim.source_id),
                })
                .collect(),
        });

        // Her newest result at the level the allocation chose: whole when the window holds it,
        // else the largest of its levels that fits, never a bare pointer (goal 7, `newest_result.rs`).
        let newest_result_reduced = match newest_levels.get(act.newest_level()) {
            Some(&level_cost) => self.fit_newest_result(&mut all_messages, level_cost),
            None => false, // no result this act
        };
        // The standing grounding within its grant: shedding, never refusing the turn.
        let grounding_grant = calibration.prompt_budget(act.granted(claim_id::GROUNDING) as usize);
        let mut grounding = match GroundingPlan::within_budget(ws, grounding_grant) {
            Fitted::Whole(plan) => plan,
            Fitted::Reduced { plan, wanted } => {
                continuum_base::probe!(
                    class = "delib.grounding.shed",
                    persona = %self.persona_name,
                    wanted_tokens = wanted,
                    budget_tokens = grounding_grant,
                    kept_tokens = plan.context_reserve() + plan.trailing_tokens(),
                    "standing grounding did not fit its grant: thinking with less enrichment rather than refusing the turn"
                );
                plan
            }
        };
        all_messages.grounding_tokens = grounding.trailing_tokens();
        let ctx_floor = if grounding.pieces.is_empty() { grounding_min.min(grounding_grant) } else { grounding.context_reserve() };

        // The smallest window this turn can be sent in: the reply's floor plus what cannot be shed
        // (the required claims' floors, her newest result at its chosen level). Recorded held or
        // refused, so a turn short by 123 tokens raises her need by them.
        if let Some(reg) = &self.working_set {
            let irreducible_window = Self::COMPLETION_FLOOR_TOKENS.saturating_add(act.irreducible());
            reg.record_floor(
                self.persona_id,
                irreducible_window,
                ws.now_ms.unwrap_or(0), // JUSTIFIED unwrap_or: unstamped cycle still measures honestly (registry keeps the newest, not a time series)
            );
        }
        // IRREDUCIBLE growth, typed apart from a view that outgrew its grant (reducible, a turnover):
        // KEEP THE OVERFLOW, by part, so a turn over its window says which part and by how much.
        if let Some(short) = act.irreducible_shortfall() {
            continuum_base::probe!(
                class = "delib.payload.over_window",
                persona = %self.persona_name,
                needed_tokens = short.needed,
                payload_overflow = short.needed.saturating_sub(short.window),
                context_window,
                framing_tokens,
                tool_tokens = tools.floor,
                stimulus_tokens = required.stimulus,
                action_statement_tokens = required.action_statement,
                latest_result_tokens = required.latest_result,
                latest_result_levels = newest_levels.len(),
                latest_results = required.latest_result_count,
                "the turn's IRREDUCIBLE floors do not fit the served window: growth no collapse can make, by this much"
            );
        }

        let tools_grant = calibration.prompt_budget(act.granted(claim_id::TOOLS) as usize);
        let (tools_choice, tools_refusal) = self.granted_tools(ws, tools, tools_grant);
        let budget_floor = act.held_need();
        // Her reply's grant is her measured need from the one allocation, the reserve her view is
        // sized beside (a pass's `max_tokens`; an act's cap is its room, `request_allowance`).
        // What no claim took is her view's to grow into by appends (`fit_view`), never the
        // reply's: a cap claimed above her context starved her reads (ONE-BUDGET slice 6).
        let reply_grant = act.granted(claim_id::REPLY);
        let completion_reserve = reply_grant.max(1) as usize;
        let reply_claimed = act.reply();
        continuum_base::probe!(
            class = "delib.window.flex",
            persona = %self.persona_name,
            context_window,
            framing_tokens = act.granted(claim_id::FRAMING),
            required_tokens = act.granted(claim_id::REQUIRED),
            newest_granted = act.granted(claim_id::NEWEST),
            grounding_granted = act.granted(claim_id::GROUNDING),
            last_move_granted = act.granted(claim_id::LAST_MOVE),
            history_granted = act.granted(claim_id::HISTORY),
            tools_granted = act.granted(claim_id::TOOLS),
            reply_priority = reply_claimed.map_or(0, |r| r.priority), // map_or: the reply is always claimed; 0 reads as absent
            reply_floor = reply_claimed.map_or(0, |r| r.floor_tokens), // map_or: as above
            reply_wanted = reply_claimed.map_or(0, |r| r.max_tokens), // map_or: as above
            reply_need,
            reply_granted = reply_grant,
            unallocated = act.room(),
            reserve = completion_reserve,
            escalation = act.plan.escalation_needed,
            "the act's one allocation: framing, required input, her newest result, tools, grounding, \
             her view and the reply by priority; what no claim took is her view's room"
        );
        // What the window holds for messages once the reply's grant, the tools sent and the framing
        // are set: arithmetic on the plan, never a budget of its own.
        let after_framing = calibration
            .prompt_budget((context_window as usize).saturating_sub(completion_reserve))
            .saturating_sub(tools_choice.sent.tokens)
            .saturating_sub(framing_tokens);
        let window_msg_budget = after_framing.saturating_sub(ctx_floor);
        let view_granted = act.view_granted();
        let view_room = act.room();
        // History and her view get their grants; the required input is never part of the trade.
        let msg_budget = window_msg_budget.min(all_messages.required_tokens().saturating_add(calibration.prompt_budget(
            act.granted(claim_id::LAST_MOVE)
                .saturating_add(act.granted(claim_id::HISTORY))
                .saturating_add(view_granted)
                .saturating_add(view_room) as usize,
        )));
        // Her view within what the allocation granted its sources (`epoch.rs`): the only place
        // it is ever collapsed. A broken allocator contract refuses the turn (`budget_fault`),
        // never a prompt sent anyway. An overflow also says how far beyond its grant her view would
        // have held: a reducible overflow, recorded as her censored need, never a grant.
        // A start the plan did not fund (a required floor underfunded or past the room) is this
        // turn's CAPACITY refusal, typed, never a sent prompt and never budget minted; distinct
        // from the reply-only IrreducibleShortfall, with which an act proceeds at what is left.
        let (view_beyond, view_fault, start_refusal) =
            match self.fit_view(&mut all_messages, view, &act, calibration) {
                Ok(beyond) => (beyond, None, None),
                Err(super::epoch::ViewSizing::Start(start)) => {
                    let (needed, had) = start.needed_and_had();
                    (None, None, Some(crate::cognition::workspace::PromptCapacityError { required_tokens: needed as usize, budget_tokens: had as usize }))
                }
                Err(mismatch) => (None, Some(format!("her view could not be sized: {mismatch}")), None),
            };
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
        let capacity_error = start_refusal.or(tools_refusal).or(capacity_error);
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
            // HER VIEW OVERFLOWED ITS GRANT: this turn's need is censored by the window, so what she
            // would have held (the window plus what her view lacked, `overflow_beyond`) is recorded
            // as a lower bound in place of the size the fit held her at. Measured only from held
            // sizes, her need could never exceed her grant (Aris on the M5, 2026-11-04: 55,672
            // granted of a 654,336 pool, a turnover on most acts, the same files re-read).
            use crate::cognition::working_set::NeedSample;
            let censored = view_beyond.map(|beyond| {
                let at_least = (context_window as usize).saturating_add(beyond).min(u32::MAX as usize) as u32;
                continuum_base::probe!(
                    class = "persona.need.censored",
                    persona = %self.persona_name,
                    context_window,
                    beyond_grant = beyond,
                    at_least,
                    "her view overflowed its grant: her need is recorded as at least the window plus what her view lacked to hold this act and the next one's appends"
                );
                NeedSample::AtLeast { at_least }
            });
            let held = crate::cognition::working_set::need_sample(sent_tokens, framing, fit_ok).map(|_| NeedSample::Exact(budget_floor));
            match censored.or(held) {
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
        // Only the source's truthful minimum outranks optional conversation. Additional
        // declared list units fill the grounding's own grant, never leftover room sized here: a
        // large list cannot turn every detail into mandatory input.
        if capacity_error.is_none() {
            grounding.expand_within(grounding_grant);
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
        // The context half renders within what the grounding's grant leaves after its trailing
        // half, never less than its standing minimum: the one allocation sized it.
        let ctx_budget = grounding_grant
            .saturating_sub(grounding.trailing_tokens())
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
                framing_grant: act.granted(claim_id::FRAMING).saturating_add(act.granted(claim_id::TOOLS)),
                required_grant: act.granted(claim_id::REQUIRED).saturating_add(act.granted(claim_id::NEWEST)),
                history_grant: act.granted(claim_id::LAST_MOVE).saturating_add(act.granted(claim_id::HISTORY)),
                grounding_grant: act.granted(claim_id::GROUNDING),
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
