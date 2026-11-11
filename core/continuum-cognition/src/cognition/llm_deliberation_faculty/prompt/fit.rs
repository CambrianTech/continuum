use super::*;

impl LlmDeliberationFaculty {
    pub(in super::super) fn context_piece_tokens(faculty: &str, body_bytes: usize) -> usize {
        // The actual rendered shape is "\n[faculty]\n<body>\n".
        (faculty.len() + body_bytes + "\n[]\n\n".len()).div_ceil(GUARD_CHARS_PER_TOKEN)
    }

    /// What wrapping a non-empty assembled context costs: its header (it rides the tail after her
    /// view, never the system message; `deliberation_prompt::working_context_message`).
    pub fn working_context_header_cost() -> usize {
        deliberation_prompt::WORKING_CONTEXT_HEADER.len().div_ceil(GUARD_CHARS_PER_TOKEN)
    }

    /// The system message's estimate: its text. The chat template's own tokens around each message
    /// are no constant here: they are in the engine's count of every prompt, so her measured
    /// density prices them (`Density`, `delib.prompt.density`); unmeasured, the estimate stands
    /// alone and the engine's exact count before generation refits a prompt denser than priced.
    pub fn framing_cost(system: &str) -> usize {
        estimate_guard_tokens(system)
    }

    /// How many MESSAGES a cluster walk may cross — how far the front may advance past a
    /// broken cluster, and how deep the fill cap looks back for her own last move. A
    /// count of messages, not of tokens or seconds: there is nothing here to measure.
    pub(in super::super) const CLUSTER_SCAN_MESSAGES: usize = 6;

    /// HER OWN move: an assistant turn.
    pub(in super::super) fn is_own_move(message: &ChatMessage) -> bool {
        message.role == "assistant"
    }

    /// The OUTCOME of one of her moves: a tool-result / receipt block.
    pub(in super::super) fn is_move_result(message: &ChatMessage) -> bool {
        let body = message.content_text();
        body.starts_with("Full result of") || body.starts_with('⚙')
    }

    /// Does this message CONTINUE a cluster rather than open one? One owner for the
    /// predicate: the evictor uses it so a window never opens on an answer without its
    /// question, and [`PromptMessages::recent_move_evidence`] uses it to find the run at
    /// the END that must not be evicted at all.
    pub(in super::super) fn continues_cluster(message: &ChatMessage) -> bool {
        Self::is_own_move(message) || Self::is_move_result(message)
    }

    /// A message's TEXT estimate alone: the part [`PromptCalibration::estimate`] builds on (it adds the
    /// engine-measured template share of the message's kind). Never a message's price by itself: every
    /// prompt, fit and refit prices through [`PromptCalibration::estimate`].
    /// Never count base64 characters as image tokens. Native images require the finalized-body
    /// image quote before local generation.
    pub fn message_cost(message: &ChatMessage) -> usize {
        super::super::super::token_budget::estimate_message_content_tokens(&message.content, estimate_guard_tokens)
    }

    /// Fit an already-built conversation to `budget_tokens`: required input whole, then optional
    /// history kept newest-last, dropped whole message by whole message from its oldest end, never
    /// a mid-message shear, never past her own last move while it fits, and never opening on an
    /// answer without its question.
    pub fn fit_messages(
        &self,
        mut prompt: PromptMessages,
        budget_tokens: usize,
    ) -> Result<FittedMessages, PromptCapacityError> {
        let required_tokens = prompt.required_tokens();
        // A quiet self-tick still needs one complete situation message. It has
        // no external stimulus to protect, but returning zero messages is not a
        // valid provider request. Preserve its newest whole context or fault.
        let ambient_tokens = if required_tokens == 0 {
            match prompt.history.last() {
                Some(message) => prompt.pricing.estimate(message),
                None => 0, // no history and no required payload: assembly supplies the quiet world-state
            }
        } else {
            0
        };
        let minimum_tokens = required_tokens + ambient_tokens;
        if minimum_tokens > budget_tokens {
            return Err(PromptCapacityError {
                required_tokens: minimum_tokens,
                budget_tokens,
            });
        }
        let history_budget = budget_tokens - required_tokens;
        // Nothing collapses here: her view collapses once, at an epoch start (`epoch.rs`),
        // and arrives within the history budget. This drop is the guard behind it.
        // Read BEFORE the history is borrowed for costing: what her own last move and its
        // outcome cost, so the drop below can refuse to reach them.
        let evidence = prompt.recent_move_evidence();
        let messages = &prompt.history;
        // Each message at the one estimate her view and the refit price with, so they charge identically.
        let costs: Vec<usize> = messages.iter().map(|message| prompt.pricing.estimate(message)).collect();
        let total: usize = costs.iter().sum();
        let mut start = 0usize;
        if total > history_budget {
            // Whole messages, oldest first, exactly as many as the budget needs: each step is the
            // oldest message's own size. A quantized drop (chunks of a budget fraction) once kept
            // the conversation's front byte-stable across acts; her view's epoch owns that now,
            // and what this fits (this act's fresh tail, or this act's refused prompt) is never
            // the next act's prefix (her served tail joins her view as her view judged it).
            let min_drop = total - history_budget;
            let mut dropped = 0usize;
            // Only optional history yields. Stimulus and action payload are typed
            // slots outside this eviction range, even when both render as user.
            let drop_end = if required_tokens == 0 {
                messages.len().saturating_sub(1)
            } else {
                messages.len()
            };
            // HER OWN LAST MOVE IS THE LAST THING TO GO, never the first.
            //
            // Front-dropping is already recency-ordered, so the oldest turns yield first
            // — but HOW FAR the drop reaches is decided by cost alone, and a tight fill
            // budget walks it right up to the run at the end: her act, its result, and
            // the turn that asked for it. That run is the evidence that she ALREADY DID
            // THE THING. Evict it and she re-issues the same act — which the M5 records
            // as `persona.act.repeat_short_circuited` ("identical act already satisfied
            // this turn"), then `persona.act.no_deliverable_yet`, then an hour line with
            // zero writes that reads as a lazy persona.
            //
            // Protected whenever the budget can hold it at all. When it CANNOT, nothing
            // here can save it — stopping short would put the assembled prompt over the
            // window, which mutes her for the whole tick — but it is never silent: a
            // `CannotFit` is an outcome this code declares and names, with what was lost.
            //
            // `min`, not `saturating_sub`: the quiet-tick branch above already holds the
            // LAST message back, and that message is inside the evidence suffix —
            // subtracting would reserve one extra old turn and leave the fit over budget.
            let evidence_fit = evidence.fit_within(history_budget);
            if evidence_fit == EvidenceFit::CannotFit {
                // A REFUSAL, NOT A QUIET DROP (Astra's review of #4290, and her test
                // `continuity_insufficient_capacity_refuses_instead_of_dropping_evidence`).
                //
                // This branch is only reached when every other optional turn has already
                // yielded and her act and its result STILL do not fit. Continuing here
                // would forget a completed act and let her re-issue it, with a probe as
                // the only trace — "required action/result continuity must not become
                // optional when it exceeds capacity". So it is a capacity error, the same
                // outcome the irreducible stimulus and `latest_result` already produce,
                // on the same path the caller already handles.
                //
                // It is loud in both directions: the probe names what did not fit, and the
                // error carries the real requirement, which is the number the untruncated
                // demand receipt feeds to `serving_plan` — so a window too small for one
                // act gets PROVISIONED rather than silently trimmed forever.
                continuum_base::probe!(
                    class = "delib.fill.evidence_cannot_fit",
                    persona = %self.persona_name,
                    verdict = evidence_fit.as_str(),
                    evidence_messages = evidence.messages,
                    evidence_tokens = evidence.tokens,
                    largest_message_tokens = evidence.largest_message,
                    history_budget,
                    truncated_chain = evidence.truncated,
                    "her most recent act and its result do NOT fit the conversation \
                     budget — refusing the turn rather than forgetting a completed act. \
                     The window is too small for a turn of this shape"
                );
                return Err(PromptCapacityError {
                    required_tokens: required_tokens.saturating_add(evidence.tokens),
                    budget_tokens,
                });
            }
            if evidence_fit == EvidenceFit::TooLargeToCarry {
                // Loud, but NOT a refusal — see [`EvidenceFit::TooLargeToCarry`]. The
                // oversized member yields like any other optional turn and the rest of
                // the drop proceeds; refusing here is the 836-refusals outage.
                continuum_base::probe!(
                    class = "delib.fill.evidence_cannot_fit",
                    persona = %self.persona_name,
                    verdict = evidence_fit.as_str(),
                    evidence_messages = evidence.messages,
                    evidence_tokens = evidence.tokens,
                    largest_message_tokens = evidence.largest_message,
                    history_budget,
                    truncated_chain = evidence.truncated,
                    "one message of her last act is on its own larger than the whole \
                     conversation budget — it cannot be carried whole by any policy, so \
                     it yields rather than muting the turn. The answer is condensing it, \
                     never a capacity fault"
                );
            }
            let drop_end = if evidence_fit == EvidenceFit::Protected {
                drop_end.min(messages.len().saturating_sub(evidence.messages))
            } else {
                drop_end
            };
            while start < drop_end && dropped < min_drop {
                dropped += costs[start];
                start += 1;
            }
            // CLUSTERS ARE THE EVICTION GRANULARITY (Joel 2026-09-01:
            // "information flows in groups … a room, code we're looking at,
            // any context that flows semantically together"). A window that
            // opens mid-cluster — an assistant answer without its question, a
            // tool result without its act — costs tokens while carrying broken
            // meaning, worse than dropping the fragment too. Advance the start
            // past continuation-shaped messages to a clean opener. Structural
            // v1 (no model call): an assistant turn or a result/receipt block
            // continues a cluster; a plain user turn opens one. Bounded so a
            // pathological run can't eat the window; the causal-thread work
            // (ladder rung 4) replaces this with true causal subtrees.
            let mut opener_advance = 0usize;
            while start + 1 < messages.len()
                && start < drop_end
                && opener_advance < Self::CLUSTER_SCAN_MESSAGES
                && Self::continues_cluster(&messages[start])
            {
                start += 1;
                opener_advance += 1;
            }
        }
        let receipt = FitReceipt {
            history_messages: costs.len(),
            history_tokens: total,
            kept_messages: costs.len() - start,
            dropped_messages: start,
            dropped_tokens: costs[..start].iter().sum(),
            required_tokens,
            history_budget,
        };
        // Move the surviving messages; fitting never clones the entire prompt.
        prompt.history.drain(..start);
        let grounding_at = prompt.grounding_at.saturating_sub(start);
        let refit = PromptRefit {
            history_len: prompt.history.len(),
            grounding_at,
            grounding_len: 0,
            result_pointers: prompt.result_pointers.into_iter()
                .filter_map(|(index, pointer)| index.checked_sub(start).map(|i| (i, pointer)))
                .collect(),
        };
        // The pinned result stays last. With none, the ask follows her own statement:
        // bracketed meta after the ask gets answered and parroted in its place (the
        // 2026-07-20 humaneval fix); her notices ride her view, ahead of all of this.
        if !prompt.latest_result.is_empty() {
            prompt.history.extend(prompt.stimulus);
            prompt.history.extend(prompt.action_statement);
            prompt.history.extend(prompt.latest_result);
        } else {
            prompt.history.extend(prompt.action_statement);
            prompt.history.extend(prompt.stimulus);
        }
        Ok(FittedMessages {
            messages: prompt.history,
            grounding_at,
            receipt,
            refit,
        })
    }

    /// Reduce only the already admitted optional snapshot after an exact quote.
    /// The conversion proposes a reduction, not a tokenizer guarantee; the adapter
    /// must quote the resulting request again. Each successful correction consumes
    /// a whole optional message or its one saved-result representation.
    pub fn refit_rejected_prompt(
        &self,
        mut view: DeliberationPromptView,
        measured: u32,
        available: u32,
        reply: u32,
    ) -> Result<DeliberationPromptView, PromptCapacityError> {
        let old_estimate = view.estimated_prompt_tokens;
        let old_reductions = view.refit.remaining_reductions();
        let refusal = || PromptCapacityError {
            required_tokens: (measured as usize).saturating_add(reply as usize),
            budget_tokens: available as usize,
        };
        if measured == 0 || old_estimate == 0 || old_reductions == 0 {
            return Err(refusal());
        }
        // Local admission requires prompt + reply < window (serving_guard).
        // The additional token represents that strict boundary, not a safety fudge.
        let deficit = (measured as usize).saturating_add(reply as usize)
            .saturating_sub((available as usize).saturating_sub(1));
        let reduction = ((deficit as u128 * old_estimate as u128)
            .div_ceil(measured as u128)).min(usize::MAX as u128) as usize;
        if reduction == 0 { return Err(refusal()); }
        let input_budget = old_estimate.saturating_sub(reduction);
        let fixed_tokens = old_estimate.saturating_sub(view.pricing.estimate_all(&view.messages));
        let grounding: Vec<_> = view.messages.drain(
            view.refit.grounding_at..view.refit.grounding_at + view.refit.grounding_len
        ).collect();
        let required = view.messages.split_off(view.refit.history_len);
        let frozen_grounding_tokens = view.pricing.estimate_all(&grounding);
        let prompt = PromptMessages {
            streams: Default::default(),
            input_identity: view.input_identity,
            history: view.messages,
            result_pointers: view.refit.result_pointers,
            grounding_at: view.refit.grounding_at,
            stimulus: None,
            latest_result: required,
            action_statement: None,
            pricing: view.pricing,
        };
        let mut fitted = self.fit_messages(prompt, input_budget
            .saturating_sub(fixed_tokens).saturating_sub(frozen_grounding_tokens))?;
        if fitted.refit.remaining_reductions() >= old_reductions {
            return Err(refusal());
        }
        fitted.refit.grounding_len = grounding.len();
        fitted.messages.splice(fitted.grounding_at..fitted.grounding_at, grounding);
        let estimate = fixed_tokens.saturating_add(view.pricing.estimate_all(&fitted.messages));
        if estimate >= old_estimate { return Err(refusal()); }
        view.messages = fitted.messages;
        view.refit = fitted.refit;
        view.estimated_prompt_tokens = estimate;
        view.segments = segment_map(&view.system, &view.messages);
        view.capacity_error = None;
        Ok(view)
    }
}

#[derive(Default)]
pub(in super::super) struct ContextComposition {
    pub(in super::super) context_window: u32,
    pub(in super::super) framing_tokens: usize,
    pub(in super::super) conversation_tokens: usize,
    pub(in super::super) ctx_floor: usize,
    pub(in super::super) after_framing: usize,
    pub(in super::super) tool_tokens: usize,
    /// WHY the surface priced in `tool_tokens` is the one sent: the receipt that says which
    /// arm put a 38-tool surface on a CPU lane (card 3026b83e).
    pub(in super::super) surface_reason: SurfaceReason,
}
