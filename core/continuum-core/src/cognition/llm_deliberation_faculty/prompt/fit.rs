use super::*;

impl LlmDeliberationFaculty {
    /// The per-message chat-template overhead every message pays, whether it is being
    /// measured for demand or fitted to a budget. ONE constant so the two cannot drift
    /// — an under-count here is an llama-server 400 at the window edge.
    ///
    /// Chat templates wrap each message in role markers
    /// (`<|im_start|>role\n…<|im_end|>`, ~4-5 tokens). The original +2 was optimistic:
    /// at the budget edge the fitted thread measured over the window by a token
    /// (glass-boxed 2026-07-13). Round UP like every other estimate here.
    // context-budget-exempt: fixed chat-template overhead per message (role tags + separators) — a property of the prompt FORMAT, not a budget that should scale with the window
    pub(in super::super::super) const PER_MESSAGE_TEMPLATE_TOKENS: usize = 5;

    pub(in super::super::super) fn context_piece_tokens(faculty: &str, body_bytes: usize) -> usize {
        // The actual rendered shape is "\n[faculty]\n<body>\n".
        (faculty.len() + body_bytes + "\n[]\n\n".len()).div_ceil(GUARD_CHARS_PER_TOKEN)
    }

    pub(in super::super::super) fn working_context_header_cost() -> usize {
        deliberation_prompt::WORKING_CONTEXT_HEADER
            .len()
            .div_ceil(GUARD_CHARS_PER_TOKEN)
    }

    pub(in super::super) fn framing_cost(system: &str) -> usize {
        // System is a message too; the template also opens the assistant's
        // generation turn. Charge both wrappers using the same format allowance.
        est_tokens(system) + 2 * Self::PER_MESSAGE_TEMPLATE_TOKENS
    }

    /// How many MESSAGES a cluster walk may cross — how far the front may advance past a
    /// broken cluster, and how deep the fill cap looks back for her own last move. A
    /// count of messages, not of tokens or seconds: there is nothing here to measure.
    pub(in super::super) const CLUSTER_SCAN_MESSAGES: usize = 6;

    /// HER OWN move: an assistant turn.
    fn is_own_move(message: &ChatMessage) -> bool {
        message.role == "assistant"
    }

    /// The OUTCOME of one of her moves: a tool-result / receipt block.
    fn is_move_result(message: &ChatMessage) -> bool {
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

    /// What this conversation costs whole, with no budget applied — the conversational
    /// half of a turn's demand ([`super::working_set`]).
    pub(in super::super) fn messages_cost(messages: &[ChatMessage]) -> usize {
        messages.iter().map(Self::message_cost).sum()
    }

    pub(in super::super) fn message_cost(message: &ChatMessage) -> usize {
        // Text-only lower bound, calibrated by the provider's measured prompt
        // count. Never count base64 characters as image tokens. Native images
        // require the finalized-body image quote before local generation.
        let tokens = super::super::super::token_budget::estimate_message_content_tokens(&message.content, est_tokens);
        tokens + Self::PER_MESSAGE_TEMPLATE_TOKENS
    }

    /// Fit an already-built conversation to `budget_tokens` with a whole-message front drop.
    /// Within an epoch nothing is dropped (`epoch_view`); this drop runs when the epoch turns
    /// over, and it frees `prompt.headroom` past the minimum: her measured per-act growth, the
    /// room the new epoch grows into before it turns over again (an exact fit turned over on
    /// every act). Whole turns only, never a mid-message shear
    /// ([[collapse-dont-clip-condense-the-past-never-erode-it]]).
    pub(in super::super) fn fit_messages(
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
                Some(message) => Self::message_cost(message),
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
        // 82416bb1: during an ongoing concern WM's ring only appends. Before
        // dropping a result AND its expansion handle, price the source's compact
        // alternative. No second store or mutation of WM: custody stays with the
        // existing saved result, and only the fitted view changes. Unsaved results
        // never supply an alternative. Ordinary eviction still bounds this view.
        // Collapsing and dropping both leave the measured growth free (`prompt.headroom`).
        let headroom = prompt.headroom;
        let mut history_tokens = Self::messages_cost(&prompt.history);
        // A turnover's view must leave the growth free even when compaction alone fit it.
        let over = history_tokens.saturating_add(headroom) > history_budget;
        for (index, pointer) in std::mem::take(&mut prompt.result_pointers) {
            let Some(message) = prompt.history.get_mut(index) else { continue };
            let old_cost = Self::message_cost(message);
            let pointer_cost = Self::message_cost(&pointer);
            if over && history_tokens > history_budget.saturating_sub(headroom) && pointer_cost < old_cost {
                history_tokens = history_tokens.saturating_sub(old_cost - pointer_cost);
                *message = pointer;
            } else {
                prompt.result_pointers.push((index, pointer));
            }
        }
        // Read BEFORE the history is borrowed for costing: what her own last move and its
        // outcome cost, so the drop below can refuse to reach them.
        let evidence = prompt.recent_move_evidence();
        let messages = &prompt.history;
        // Per-message template overhead: `Self::PER_MESSAGE_TEMPLATE_TOKENS`, shared
        // with `messages_cost` so measurement and fitting charge identically.
        let costs: Vec<usize> = messages.iter().map(Self::message_cost).collect();
        let total: usize = costs.iter().sum();
        let mut start = 0usize;
        if total.saturating_add(headroom) > history_budget {
            let drop_q = total.saturating_add(headroom) - history_budget;
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
                crate::probe!(
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
                crate::probe!(
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
            while start < drop_end && dropped < drop_q {
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
            headroom,
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
        // The steps ledger changes on every act (a new action at its tail, an old one aged
        // out at its head). In a multi-act turn it rides after everything append-only
        // across the acts and just before the pinned result, which stays last (card
        // 5b09111e: on the M5 it sat ahead of the framing, the ask and 18 room updates that
        // were byte-identical between acts, and forfeited all of them). With no pinned
        // result it keeps its place ahead of the ask: bracketed meta after the ask gets
        // answered and parroted in its place (the 2026-07-20 humaneval fix).
        if !prompt.latest_result.is_empty() {
            prompt.history.extend(prompt.stimulus);
            prompt.history.extend(prompt.room_updates);
            prompt.history.extend(prompt.ledger);
            prompt.history.extend(prompt.notices.into_iter().map(|(_, notice)| notice));
            prompt.history.extend(prompt.action_statement);
            prompt.history.extend(prompt.latest_result);
        } else {
            prompt.history.extend(prompt.action_statement);
            prompt.history.extend(prompt.ledger);
            prompt.history.extend(prompt.notices.into_iter().map(|(_, notice)| notice));
            prompt.history.extend(prompt.stimulus);
            prompt.history.extend(prompt.room_updates);
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
    pub(in super::super) fn refit_rejected_prompt(
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
        let fixed_tokens = old_estimate.saturating_sub(Self::messages_cost(&view.messages));
        let grounding: Vec<_> = view.messages.drain(
            view.refit.grounding_at..view.refit.grounding_at + view.refit.grounding_len
        ).collect();
        let required = view.messages.split_off(view.refit.history_len);
        let frozen_grounding_tokens = Self::messages_cost(&grounding);
        let prompt = PromptMessages {
            body: Vec::new(),
            headroom: 0,
            input_identity: view.input_identity,
            history: view.messages,
            result_pointers: view.refit.result_pointers,
            grounding_tokens: 0,
            grounding_at: view.refit.grounding_at,
            stimulus: None,
            latest_result: required,
            action_statement: None,
            room_updates: Vec::new(),
            ledger: None,
            notices: Vec::new(),
        };
        let mut fitted = self.fit_messages(prompt, input_budget
            .saturating_sub(fixed_tokens).saturating_sub(frozen_grounding_tokens))?;
        if fitted.refit.remaining_reductions() >= old_reductions {
            return Err(refusal());
        }
        fitted.refit.grounding_len = grounding.len();
        fitted.messages.splice(fitted.grounding_at..fitted.grounding_at, grounding);
        let estimate = fixed_tokens.saturating_add(Self::messages_cost(&fitted.messages));
        if estimate >= old_estimate { return Err(refusal()); }
        view.messages = fitted.messages;
        view.refit = fitted.refit;
        view.estimated_prompt_tokens = estimate;
        view.segments = segment_map(&view.system, &view.messages);
        view.capacity_error = None;
        view.epoch = None;
        Ok(view)
    }
}
