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

    /// The window and density this binding's next prompt is fitted with, priced behind the
    /// admission its adapter gives the request now: the engine's exact count of the finalized
    /// prompt, or the estimate alone (`PromptCalibration::charge`).
    fn fit_within(&self, binding: &ModelBinding, window: u32) -> (u32, PromptCalibration) {
        let admission = if binding.adapter.counts_prompts_exactly() {
            crate::ai::provenance::PromptAdmission::EngineCounted
        } else {
            crate::ai::provenance::PromptAdmission::Estimated
        };
        let feedback = self.prompt_feedback.load();
        let (window, calibration) = feedback.as_ref().filter(|f| f.applies_to(binding)).map_or(
            (window, PromptCalibration::default()),
            |f| (window.min(f.available), f.calibration),
        );
        // The template overhead of the engine this request reaches, measured there (never a constant).
        (window, calibration.behind(admission).with_template(super::template::TemplateEstimate::from_measurement(&binding.adapter.template_overhead(binding.requested_model()))))
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
        timing: Option<&crate::ai::types::GenerationTiming>,
        available: u32,
        served: &str,
    ) {
        // No timing on the reply = nothing measured as reused = nothing rides free.
        let engine_cached_tokens = timing.map_or(0, |t| t.cached_tokens); // map_or: no timing reused nothing
        // Her act's engine timing joins what holding context costs her on this engine and model.
        if let (Some(t), Some(memory)) = (timing, self.working_memory.as_deref()) {
            memory.observe_act(Self::served_by(binding), &continuum_context::price::ActSample {
                input: engine_prompt_tokens,
                cached: t.cached_tokens,
                fresh: t.prefill_tokens,
                prefill_ms: t.prefill_ms,
                output: t.decode_tokens,
                decode_ms: t.decode_ms,
            });
        }
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
            // The chat template's tokens around each message, as the estimate counted them:
            // measured from the engine, or unmeasured (her density then carries them).
            template = ?super::template::TemplateEstimate::from_measurement(&binding.adapter.template_overhead(binding.requested_model())),
            "the engine's count of this prompt, retained in her density; the next prompt is fitted at its bound"
        );
    }

    /// The engine and model `binding` reaches: the key her act time is measured under, whatever
    /// her window (seconds per held token are the engine's and the model's, never her window's).
    /// A need past the model's trained context (`what`: her irreducible window, or the priced
    /// point): no lane of this model can hold it, so it is a measurement defect, named, never
    /// recorded (card bf3b6d68).
    fn probe_need_beyond_trained(&self, what: &'static str, beyond: continuum_context::allocation::BeyondTrained) {
        continuum_base::probe!(
            class = "persona.need.beyond_trained",
            persona = %self.persona_name,
            what,
            needed = beyond.needed,
            trained = beyond.trained.get(),
            "a need past the model's trained context: a measurement defect, never recorded as her need (card bf3b6d68)"
        );
    }

    fn served_by(binding: &ModelBinding) -> crate::cognition::working_memory::act_times::ServedBy {
        crate::cognition::working_memory::act_times::ServedBy { provider: binding.adapter.provider_id().to_owned(), model: binding.requested_model().to_owned() }
    }

    /// Her acts' timing fit on the engine and model `binding` reaches, kept across window changes
    /// and restarts with her working memory; empty with no working memory (nothing measured).
    pub(in super::super) fn act_time_on(&self, binding: &ModelBinding) -> continuum_context::price::ActTimeFit {
        self.working_memory.as_deref().map_or_else(Default::default, |memory| memory.act_time(&Self::served_by(binding))) // map_or_else: a faculty with no working memory has measured nothing
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
        let need = self.working_set.as_ref().and_then(|reg| reg.need_or_pooled(self.persona_id));
        let act_room = crate::cognition::working_set::ActRoom::of(need);
        let reply = reply_claim(
            kind,
            need,
            pending_peak,
            Self::holds_live_work(ws),
            ws.directed_at_self(),
            time_term,
        );
        let desired_reply = reply.max_tokens as usize;

        // Her hands, her framing and her grounding are consumers in the one allocation below, each
        // answering how it reduces (`tools_epoch.rs`, `framing.rs`, `grounding.rs`): the native
        // tool schemas ride the served window ahead of every message (the gateway injects them via
        // the chat template), the framing is the system message, the grounding the assembled
        // context. Nothing about them is sized beside the allocation.
        let holds_live_work = Self::holds_live_work(ws);

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
        // Her view leads the history: this act appends to it, and everything after it (the clock,
        // the ask, her newest act) is this act's tail, judged by her view now (what the allocation
        // prices) and joining it once served.
        let view = self.place_view(&mut all_messages);
        self.judge_tail(&mut all_messages, false);

        let input_identity = all_messages.input_identity;
        let calibration = calibration.for_input(input_identity);
        // One price for every message of this act: her view's start, the fit and a refit.
        all_messages.pricing = calibration;
        // Her view already carries every block that rides the conversation. Measured BEFORE her
        // newest result is levelled: demand is the sum of wants (FOCUS-CONTRACT rule 5), so a
        // result shown at a smaller level still asks for its whole, or the window never grows for it.
        let unfitted_message_tokens = all_messages.tokens();

        // THE ACT'S ONE ALLOCATION (ONE-ACT stage 2, `continuum_context::allocation`). Owner's rule:
        // "Flexbox is the right way. It's like the governor: if you don't use it for literally
        // everything it doesn't work." Every part of her prompt is a claim in it, in the
        // allocation's unit (engine-calibrated tokens), and it runs ONCE: framing, the required
        // input, her newest result (irreducible at its whole; its first page only past what the
        // window holds beside every other hard limit), her
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
        use continuum_context::ladder::{Leveled, Rungs};
        // Her newest result whole and at its least page, in the allocation's unit (`newest_result.rs`).
        let newest = self.newest_result_claim(&all_messages);
        // Her hands and her framing (the head of her prompt), and her grounding: each a consumer
        // whose levels come from its items' shapes and whose hard limit is derived from them.
        let tools_offer = self.tools_offer(ws, &claim);
        let held_menu = self.tools_epoch.lock().as_ref().map(|held| held.menu().clone());
        let framing_offer = self.framing_offer(ws, holds_live_work, tools_offer.names(), held_menu.as_ref(), &claim);
        let grounding_offer = super::grounding::GroundingOffer::of(ws, &claim);
        // Her own last move and its result are a REQUIRED claim of their own (`last_move`, goal 7:
        // the newest complete interaction is whole): clipping them costs a WHOLE EXTRA TURN when
        // she re-issues the act. The rest of this act's fresh tail (older acts, facts, the clock,
        // demoted payloads) is the optional `history` claim and sheds first. Her view's sources
        // claim for themselves.
        let evidence = all_messages.recent_move_evidence();
        let view_tokens = calibration.estimate_all(&all_messages.history[..view.len()]);
        let optional_history = all_messages.tokens().saturating_sub(required.total()).saturating_sub(view_tokens);
        // A quiet self-tick (no required input) still sends her newest whole message.
        let quiet_minimum = if required.total() == 0 {
            all_messages.history.last().map_or(0, |message| calibration.estimate(message)) // map_or: no history = nothing to keep
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
        // Already in the allocation's tokens: her view's allocation estimate charges each block at this calibration.
        let view_claims: Vec<_> = self.view_claims(&all_messages.streams, super::epoch::AllocationEstimate(calibration)).into_iter().map(|mut source| {
            source.claim.max_tokens = source.claim.max_tokens.max(source.claim.floor_tokens);
            source.claim.required = source.claim.required || acting;
            source
        }).collect();
        let reply_need = reply.floor_tokens;
        // Her price and the model's trained context from the one binding snapshot this act is
        // priced on.
        let binding = self.binding.load();
        let trained = binding.trained_context();
        let started = std::time::Instant::now();
        let act = continuum_context::allocation::allocate(
            context_window,
            continuum_context::allocation::ActClaims {
                framing: continuum_context::allocation::Head { held: framing_offer.held.clone(), ..continuum_context::allocation::Head::free(framing_offer.leveled.clone()) },
                // Input she must read: its whole is its hard limit.
                required: Leveled::one(Rungs::whole(claim(required_tokens))),
                newest: newest.charged(&claim),
                tools: tools_offer.head(),
                grounding: grounding_offer.leveled.clone(),
                // The newest complete interaction is whole (goal 7): its whole is its hard limit.
                last_move: Leveled::one(Rungs::whole(claim(history_floor))),
                history: continuum_context::allocation::Span { floor: 0, min: 0, want: claim(optional_history.saturating_sub(history_floor)) },
                reply,
                view: view_claims,
                // What holding context costs her act on this binding, measured from her own acts:
                // the allocation holds the window where her value per second peaks.
                price: self.act_time_on(&binding).price(),
                // The most her need can be: the window the price chooses never passes it.
                trained,
            },
        );
        let allocation_us = started.elapsed().as_micros() as u64;
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
                    // A leveled part's floor is its hard limit, never the level this act chose.
                    floor: act.hard_limit(&claim.source_id).unwrap_or(claim.floor_tokens), // unwrap_or: a part that is not leveled floors at its claim
                    want: claim.max_tokens,
                    grant: act.granted(&claim.source_id),
                })
                .collect(),
            // Stamped per request, once its cap is known (`act_room.rs`).
            output_room: None,
        });

        // Her newest result as the allocation showed it: whole, or its first page (content with
        // reading on named, never a summary or a bare handle, goal 7), cut below once her view is
        // fitted, to the room its grant gives and the fit actually has beside it.
        let newest_page = match act.newest_shown() {
            continuum_context::allocation::NewestShown::Page { tokens } => Some(calibration.prompt_budget(tokens as usize)),
            continuum_context::allocation::NewestShown::Whole | continuum_context::allocation::NewestShown::Absent => None,
        };
        // The grounding at the levels the allocation chose: less of each under pressure, then
        // handles, never refusing the turn (`grounding.rs`).
        let grounding_rungs = act.chosen(claim_id::GROUNDING).map(|choice| choice.rungs.clone()).unwrap_or_default(); // unwrap_or_default: an unchosen grounding left the turn
        let grounding_grant = calibration.prompt_budget(act.granted(claim_id::GROUNDING) as usize);
        let ctx_floor = grounding_grant;

        // The smallest window this turn can be sent in: what cannot be shed (the required claims'
        // floors, her newest result at its chosen level) plus her reply's MEASURED need, uncapped
        // (an act's think plus answer; a pass's smallest reply; an unmeasured act adds nothing, it
        // is a state, not a number). Recorded held or refused, so a turn short by 123 tokens raises
        // her need by them. A record, never a grant: the governor sizes residency from it
        // (`requirements_for`) or she waits.
        // Past the model's trained context it is a measurement defect, never a floor.
        if let Some(reg) = &self.working_set {
            match continuum_context::allocation::NeedWindow::within(act.irreducible().saturating_add(reply_need), trained) {
                Ok(irreducible_window) => reg.record_floor(
                    self.persona_id,
                    irreducible_window.tokens(),
                    ws.now_ms.unwrap_or(0), // JUSTIFIED unwrap_or: unstamped cycle still measures honestly (registry keeps the newest, not a time series)
                ),
                Err(beyond) => self.probe_need_beyond_trained("irreducible", beyond),
            }
        }
        // IRREDUCIBLE growth, typed apart from a view that outgrew its grant (reducible, a turnover):
        // KEEP THE OVERFLOW, by part, so a turn over its window says which part and by how much.
        let shortfall = act.irreducible_shortfall();
        // An ACT is never sent with a cap below her measured need: a short cap spends itself on her
        // think and cuts her at its first sentence (the M5, 2026-11-04). A pass proceeds as before.
        let output_shortfall = shortfall.clone().filter(|_| kind == TurnKind::Act);
        if let Some(short) = shortfall {
            continuum_base::probe!(
                class = "delib.payload.over_window",
                persona = %self.persona_name,
                needed_tokens = short.needed,
                payload_overflow = short.needed.saturating_sub(short.window),
                binding = %short.binding,
                binding_limit = short.binding_limit,
                context_window,
                framing_limit = framing_offer.leveled.hard_limit(),
                tool_limit = tools_offer.head().leveled.hard_limit(),
                stimulus_tokens = required.stimulus,
                action_statement_tokens = required.action_statement,
                latest_result_tokens = required.latest_result,
                latest_result_least = newest.least(),
                latest_results = required.latest_result_count,
                "every part at its hard limit still does not fit the served window: growth no collapse can make, by this much, the binding part named"
            );
        }

        let tools_grant = calibration.prompt_budget(act.granted(claim_id::TOOLS) as usize);
        let tools_choice = self.granted_tools(ws, tools_offer, &framing_offer, &act, &claim);
        // Her framing at the menu levels the allocation chose: what the budget below charges.
        let framing_tokens = Self::framing_cost(
            &self.compose_system_split(&tools_choice.menu, ws.directed_at_self(), ws.self_initiated, ws.now_ms, holds_live_work).stable,
        );
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
            tools_limit = act.hard_limit(claim_id::TOOLS).unwrap_or(0), // unwrap_or: tools are always a leveled part
            searched_cells = act.searched.as_ref().map_or(0, |s| s.cells), // map_or: no search when every hard limit together did not fit
            search_refined = act.searched.as_ref().is_some_and(|s| s.refined),
            // The window the time price of context held (or why none: unmeasured, window binds).
            priced = ?act.priced,
            allocation_us,
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
        // have held: a reducible overflow, collapsed at the next start, never her need.
        // A start the plan did not fund (a required floor underfunded or past the room) is this
        // turn's CAPACITY refusal, typed, never a sent prompt and never budget minted; distinct
        // from the reply-only IrreducibleShortfall, with which an act proceeds at what is left.
        let head_growth = tools_choice.next.as_ref().map_or(0, |next| next.growth); // map_or: no next head adds nothing
        let (head_funded, view_fault, start_refusal) =
            match self.fit_view(&mut all_messages, view, &act, calibration, head_growth) {
                Ok(fitted) => (fitted.head_funded, None, None),
                Err(super::epoch::ViewSizing::Start(start)) => {
                    let (needed, had) = start.needed_and_had();
                    (false, None, Some(crate::cognition::workspace::PromptCapacityError { required_tokens: needed as usize, budget_tokens: had as usize }))
                }
                Err(mismatch) => (false, Some(format!("her view could not be sized: {mismatch}")), None),
            };
        // Her tail judged against her view as fitted: a turnover re-renders from her sources, so
        // what her tail showed before it arrives whole again, and a repeat needs its whole held now.
        self.judge_tail(&mut all_messages, false);
        // The surface this act sends: her head's held set and levels, or, when this act started an
        // epoch of her view that funded it, the next head (`tools_epoch.rs`).
        let (selected, menu) = self.retain_tools(tools_choice, start_refusal.is_some(), head_funded);
        // The system message at the menu levels her head holds, composed once.
        let system = self.compose_system_split(&menu, ws.directed_at_self(), ws.self_initiated, ws.now_ms, holds_live_work).stable;
        let framing_tokens = Self::framing_cost(&system);
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
        let tail_budget = msg_budget.saturating_sub(calibration.estimate_all(&view_messages));
        // The page is cut to the space actually available: its grant, and never more than the
        // tail's budget leaves beside the rest of the required input and her last move (what her
        // view holds is fitted already and may sit above its grant until its next start).
        let newest_result_reduced = match newest_page {
            Some(granted) => {
                let newest_now = calibration.estimate_all(&all_messages.latest_result);
                let beside = all_messages
                    .required_tokens()
                    .saturating_sub(newest_now)
                    .saturating_add(calibration.prompt_budget(act.granted(claim_id::LAST_MOVE) as usize));
                self.fit_newest_result(&mut all_messages, granted.min(tail_budget.saturating_sub(beside)))
            }
            None => false,
        };
        // Her tail as this prompt sends it: what joins her view once it is served.
        let mut judged_tail = std::mem::take(&mut all_messages.streams.tail.judged);
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
        // A view start the plan did not fund, or required input that cannot fit, refuses the turn.
        let capacity_error = start_refusal.or(capacity_error);
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
                .saturating_add(calibration.estimate_all(&fitted.messages))
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
            // HER NEED, the allocator's gate (card 70706a9e): the window her act's allocation CHOSE
            // (`continuum_context::allocation::need`), the priced optimum between what she must
            // hold and the model's trained context. Never every claim's floor (sizes each part
            // requested), never her post-fit size (the flexbox grows it to any window), and never the
            // window plus what her view lacked uncollapsed: Aiko recorded 902,158 on a 262,144-token
            // model (the M5, 2026-11-07 16:21Z), where her view's whole files and folded navigation
            // are collapsed, not needed. Learned only from a turn the fit held.
            let sent_tokens = sent.min(u32::MAX as usize) as u32;
            let framing = framing_tokens.min(u32::MAX as usize) as u32;
            let fit_ok = capacity_error.is_none();
            use crate::cognition::working_set::NeedSample;
            use continuum_context::allocation::ActNeed;
            let held = crate::cognition::working_set::need_sample(sent_tokens, framing, fit_ok).is_some();
            let chosen = match act.need {
                ActNeed::Chosen(window) => Some(NeedSample::Exact(window.tokens())),
                ActNeed::Censored(window) => {
                    continuum_base::probe!(
                        class = "persona.need.censored",
                        persona = %self.persona_name,
                        context_window,
                        at_least = window.tokens(),
                        priced = ?act.priced,
                        "the priced point lies past her grant: her need is recorded as at least the window her act would have chosen"
                    );
                    Some(NeedSample::AtLeast { at_least: window.tokens() })
                }
                ActNeed::Unpriced => {
                    continuum_base::probe!(
                        class = "persona.need.unpriced",
                        persona = %self.persona_name,
                        context_window,
                        priced = ?act.priced,
                        "no priced point this act (act time unmeasured, or no window inside its measured contexts): no need recorded; her irreducible window stands"
                    );
                    None
                }
                ActNeed::BeyondTrained(beyond) => {
                    self.probe_need_beyond_trained("priced", beyond);
                    None
                }
            };
            match (held, chosen) {
                (true, Some(need)) => reg.record_need(
                    self.persona_id,
                    need,
                    ws.now_ms.unwrap_or(0), // JUSTIFIED unwrap_or: unstamped cycle still measures honestly
                ),
                // Held, with no priced point: said above, by its cause.
                (true, None) => {}
                // THE UNHELD TURN IS SAID, not skipped (card 39822816): a mind the
                // allocator labels `wish:` after a deploy carrying #4325 must be
                // distinguishable, from the ledger alone, between "every turn was
                // starved", "every turn emptied to the framing", and "this seam was
                // never reached". One row per unheld turn, with the reason.
                (false, _) => continuum_base::probe!(
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
        fitted.refit.grounding_len = 0;
        let FittedMessages { mut refit, mut messages, grounding_at, .. } = fitted;

        // The assembled context at the levels the one allocation chose; its header and message
        // wrapper are priced in its most important item (`grounding.rs`).
        let used_msg_tokens = calibration.estimate_all(&messages);
        let context = self.render_grounding(
            ws,
            grounding_offer,
            &grounding_rungs,
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
        // refit keeps frozen; grounding sources themselves are her view's blocks. It is the first
        // item of her act's tail, named by what it says: a block she already holds is not sent again.
        if let Some(block) = deliberation_prompt::working_context_message(&context) {
            let judged = self.judge_context(block);
            for item in &judged {
                messages.insert(grounding_at + refit.grounding_len, item.text.to_chat());
                refit.grounding_len += 1;
            }
            judged_tail.splice(0..0, judged);
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
        // This is the native request estimate, not a backend tokenizer count.
        // Adapters may add model-specific framing; live acceptance still checks
        // their captured request/usage rather than assuming this estimate exact.
        // The system message and every message after it carry the template's share around them,
        // by kind, when the engine measured it: then the density this estimate is learned against
        // calibrates the rest, as her view's allocation estimate prices it (`epoch/allocation_estimate.rs`).
        let estimated_prompt_tokens = calibration
            .estimate_system(&system)
            .saturating_add(messages.iter().map(|message| calibration.estimate(message)).sum::<usize>())
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
            pricing: calibration,
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
            act_room,
            output_shortfall,
            judged_tail,
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
