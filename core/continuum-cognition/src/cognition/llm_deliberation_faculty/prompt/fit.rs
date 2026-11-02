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
    pub const PER_MESSAGE_TEMPLATE_TOKENS: usize = 5;

    pub(in super::super) fn context_piece_tokens(faculty: &str, body_bytes: usize) -> usize {
        // The actual rendered shape is "\n[faculty]\n<body>\n".
        (faculty.len() + body_bytes + "\n[]\n\n".len()).div_ceil(GUARD_CHARS_PER_TOKEN)
    }

    /// What wrapping a non-empty assembled context costs: its header, and the chat-template
    /// wrapper of the message it now is (it rides the tail after her view, never the system
    /// message; `deliberation_prompt::working_context_message`).
    pub(in super::super) fn working_context_header_cost() -> usize {
        deliberation_prompt::WORKING_CONTEXT_HEADER
            .len()
            .div_ceil(GUARD_CHARS_PER_TOKEN)
            + Self::PER_MESSAGE_TEMPLATE_TOKENS
    }

    pub fn framing_cost(system: &str) -> usize {
        // System is a message too; the template also opens the assistant's
        // generation turn. Charge both wrappers using the same format allowance.
        estimate_guard_tokens(system) + 2 * Self::PER_MESSAGE_TEMPLATE_TOKENS
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

    /// What this conversation costs whole, with no budget applied — the conversational
    /// half of a turn's demand ([`super::super::super::working_set`]).
    pub fn messages_cost(messages: &[ChatMessage]) -> usize {
        messages.iter().map(Self::message_cost).sum()
    }

    pub fn message_cost(message: &ChatMessage) -> usize {
        // Text-only lower bound, calibrated by the provider's measured prompt
        // count. Never count base64 characters as image tokens. Native images
        // require the finalized-body image quote before local generation.
        let tokens = super::super::super::token_budget::estimate_message_content_tokens(&message.content, estimate_guard_tokens);
        tokens + Self::PER_MESSAGE_TEMPLATE_TOKENS
    }

    /// The window-start advance QUANTUM divisor: when the conversation outgrows
    /// its budget, the front is dropped in chunks of `budget/8` (floored at 512
    /// tokens) rather than message-by-message. See [`Self::fit_messages`].
    // context-budget-exempt: a stability/latency trade ratio, not a token budget — the actual chunk derives from the live budget at the call
    pub(in super::super) const FRONT_DROP_QUANTUM_DIVISOR: usize = 8;

    /// Fit an already-built conversation to `budget_tokens` — with a
    /// QUANTIZED, whole-message front drop, because the fit's cut point is the
    /// byte-stability of the entire conversation prefix.
    ///
    /// The old shape walked NEWEST-first, spending the budget backward: correct
    /// per-act, and a KV catastrophe across acts — every act appends new tokens
    /// at the tail, so the exact-fit cut point advanced the window START by a
    /// few messages per act, and the straddling message was tail-trimmed to a
    /// different byte offset each time. Measured 2026-09-01 (wire-capture
    /// diffs): consecutive prompts diverging at ~1% depth on exactly this seam,
    /// re-prefilling the other 99%.
    ///
    /// Now the drop is quantized: the minimum front mass that must go is
    /// rounded UP to a chunk (`budget/8`, ≥512 tokens), whole messages only, so
    /// the window start stays BYTE-IDENTICAL for ~a chunk's worth of new
    /// conversation and then advances once, in one jump. The cost is a window
    /// that runs up to one chunk under-full — a deliberate stability/capacity
    /// trade, same law as the scratch slot: reuse of a 20-80k prefix is worth
    /// vastly more than the last few k of oldest history
    /// ([[collapse-dont-clip-condense-the-past-never-erode-it]] — and when the
    /// jump does land, it drops whole turns, never a mid-message shear).
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
        // Nothing collapses here: her view collapses once, at an epoch start (`epoch.rs`),
        // and arrives within the history budget. This drop is the guard behind it.
        // Read BEFORE the history is borrowed for costing: what her own last move and its
        // outcome cost, so the drop below can refuse to reach them.
        let evidence = prompt.recent_move_evidence();
        let messages = &prompt.history;
        // Per-message template overhead: `Self::PER_MESSAGE_TEMPLATE_TOKENS`, shared
        // with `messages_cost` so measurement and fitting charge identically.
        let costs: Vec<usize> = messages.iter().map(Self::message_cost).collect();
        let total: usize = costs.iter().sum();
        let mut start = 0usize;
        let quantum = (budget_tokens / Self::FRONT_DROP_QUANTUM_DIVISOR).max(512);
        if total > history_budget {
            let min_drop = total - history_budget;
            let drop_q = min_drop.div_ceil(quantum).saturating_mul(quantum);
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
            quantum,
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
        let fixed_tokens = old_estimate.saturating_sub(Self::messages_cost(&view.messages));
        let grounding: Vec<_> = view.messages.drain(
            view.refit.grounding_at..view.refit.grounding_at + view.refit.grounding_len
        ).collect();
        let required = view.messages.split_off(view.refit.history_len);
        let frozen_grounding_tokens = Self::messages_cost(&grounding);
        let prompt = PromptMessages {
            streams: Default::default(),
            input_identity: view.input_identity,
            history: view.messages,
            result_pointers: view.refit.result_pointers,
            grounding_tokens: 0,
            grounding_at: view.refit.grounding_at,
            stimulus: None,
            latest_result: required,
            action_statement: None,
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

/// A borrowed selection of whole source content or its declared atomic prefix.
/// Reserving and expanding it never copies the source payload.
pub struct GroundingPiece<'a> {
    pub(in super::super) contribution: &'a Contribution,
    /// None keeps the source's original rendering, including its separators.
    pub(in super::super) units: Option<usize>,
    pub tokens: usize,
}

impl<'a> GroundingPiece<'a> {
    pub(in super::super) fn cost(c: &Contribution, body_bytes: usize) -> usize {
        if c.trailing {
            (c.faculty.as_str().len() + body_bytes + "[]\n".len()).div_ceil(GUARD_CHARS_PER_TOKEN)
                + LlmDeliberationFaculty::PER_MESSAGE_TEMPLATE_TOKENS
        } else {
            LlmDeliberationFaculty::context_piece_tokens(c.faculty.as_str(), body_bytes)
        }
    }

    pub fn whole(c: &'a Contribution) -> Self {
        // A block whose SOURCE cut it short still owes its reader the notice, even
        // when every delivered part fits: reserve it here so the budget stays exact.
        let notice = if c.source_truncated {
            usize::from(!c.content.is_empty()) + Self::notice_bytes(c, 0)
        } else {
            0
        };
        Self {
            contribution: c,
            units: None,
            tokens: Self::cost(c, c.content.len() + notice),
        }
    }

    pub(in super::super) const NOTICE_SUFFIX: &'static str = " more not shown (context budget)";

    pub(in super::super) const EXPAND_PREFIX: &'static str = " — run `";

    pub(in super::super) const EXPAND_SUFFIX: &'static str = "` to see all ";

    /// The omission notice, exactly as `append_notice` writes it. `omitted` is what
    /// THIS packing left out of `parts`. A source that cut its own list short adds
    /// an uncounted remainder: the count becomes `N+` (or no number when the packing
    /// omitted nothing), and the verb says "see all" with no total, because the
    /// true total is unknown.
    pub(in super::super) fn notice_text(c: &Contribution, omitted: usize) -> String {
        let count = match (omitted, c.source_truncated) {
            (0, true) => String::new(),
            (n, true) => format!("{n}+"),
            (n, false) => n.to_string(),
        };
        // With no number to show, the suffix's leading space would dangle ("… more").
        let suffix = if count.is_empty() { Self::NOTICE_SUFFIX.trim_start() } else { Self::NOTICE_SUFFIX };
        let mut text = format!("…{count}{suffix}");
        if let Some(cmd) = c.expand_command {
            text.push_str(Self::EXPAND_PREFIX);
            text.push_str(cmd);
            if c.source_truncated {
                text.push_str(Self::EXPAND_SUFFIX.trim_end());
            } else {
                text.push_str(Self::EXPAND_SUFFIX);
                text.push_str(&c.parts.len().to_string());
            }
        }
        text
    }

    pub(in super::super) fn notice_bytes(c: &Contribution, omitted: usize) -> usize {
        Self::notice_text(c, omitted).len()
    }

    pub(in super::super) fn append_notice(c: &Contribution, omitted: usize, out: &mut String) {
        out.push_str(&Self::notice_text(c, omitted));
    }

    pub fn minimum(c: &'a Contribution) -> Self {
        let whole = Self::whole(c);
        if c.parts.len() < 2 {
            return whole;
        }
        let tokens = Self::cost(
            c,
            c.parts[0].len() + 1 + Self::notice_bytes(c, c.parts.len() - 1),
        );
        if tokens >= whole.tokens {
            return whole;
        }
        Self {
            contribution: c,
            units: Some(1),
            tokens,
        }
    }

    pub(in super::super) fn within(c: &'a Contribution, budget: usize) -> Option<Self> {
        let whole = Self::whole(c);
        if whole.tokens <= budget {
            return Some(whole);
        }
        // Reserve the actual omission notice before accepting complete units.
        let mut bytes = 0usize;
        let mut selected = None;
        for (index, unit) in c
            .parts
            .iter()
            .take(c.parts.len().saturating_sub(1))
            .enumerate()
        {
            bytes += unit.len() + usize::from(index > 0);
            let units = index + 1;
            let tokens = Self::cost(c, bytes + 1 + Self::notice_bytes(c, c.parts.len() - units));
            if tokens > budget {
                break;
            }
            selected = Some(Self {
                contribution: c,
                units: Some(units),
                tokens,
            });
        }
        selected
    }

    pub(in super::super) fn append_body(&self, out: &mut String) {
        let c = self.contribution;
        match self.units {
            None => {
                out.push_str(&c.content);
                if c.source_truncated {
                    if !c.content.is_empty() {
                        out.push('\n');
                    }
                    Self::append_notice(c, 0, out);
                }
            }
            Some(units) => {
                for (index, unit) in c.parts[..units].iter().enumerate() {
                    if index > 0 {
                        out.push('\n');
                    }
                    out.push_str(unit);
                }
                out.push('\n');
                Self::append_notice(c, c.parts.len() - units, out);
            }
        }
    }
}

/// The reservation retains the exact source units rendering will emit. An
/// equal-salience list cannot spend a peer's reservation; KV placement never
/// determines whether standing activity context may be evicted.
#[derive(Default)]
pub struct GroundingPlan<'a> {
    pub(in super::super) pieces: Vec<GroundingPiece<'a>>,
}

/// A plan that fit as asked, or one that had to give something up — with the size it
/// WANTED carried out, so the caller can say by how much rather than only that it did.
/// A reduction that cannot report its own magnitude is the saturating-subtract mistake
/// one layer up.
pub(in super::super) enum Fitted<T> {
    Whole(T),
    Reduced { plan: T, wanted: usize },
}

impl<'a> GroundingPlan<'a> {
    /// The standing sources' truthful minimums, whatever the budget — the plan the
    /// caller WANTS. Fitting it is [`GroundingPlan::within_budget`]'s job. Only the
    /// standing sources no producer marked as a view block: they render in the
    /// assembled-context message after her view (never the system message, which is hers
    /// in every room); a block that rides the conversation is a source of her view
    /// (`prompt/epoch.rs`), budgeted by its epoch.
    pub fn minimum(ws: &'a Workspace) -> Self {
        Self {
            pieces: ws
                .broadcast
                .iter()
                .filter(|c| c.decision.is_none() && c.standing_grounding && !c.trailing)
                .map(GroundingPiece::minimum)
                .collect(),
        }
    }

    /// Fit the standing minimums into `budget`, SHEDDING rather than failing.
    ///
    /// This returned `Err` until 2026-09-09, and that error muted citizens: it was
    /// promoted ahead of every other capacity check into a hard deliberation fault, so a
    /// persona whose enrichment overflowed refused the turn entirely — and kept her held
    /// card while refusing. Grounding is enrichment; its shortfall is not a reason to
    /// stop thinking.
    ///
    /// What sheds first, and why: the CONTEXT half (the assembled-context message) goes
    /// before the TRAILING half. Trailing pieces sit nearest the generation and carry the
    /// act-shaped facts (the pinned result, the live board); the context half is standing
    /// background, and dropping it also frees `working_context_header_cost` in one go.
    /// Pieces are shed newest-kept / oldest-first within each half, so what survives is
    /// the freshest world she has.
    pub(in super::super) fn within_budget(ws: &'a Workspace, budget: usize) -> Fitted<Self> {
        let whole = Self::minimum(ws);
        let wanted = whole.context_reserve() + whole.trailing_tokens();
        if wanted <= budget {
            return Fitted::Whole(whole);
        }
        // Trailing-only: keep the half nearest generation, drop the standing background
        // and its header.
        let mut plan = whole;
        plan.pieces.retain(|p| p.contribution.trailing);
        // Still over: shed oldest-first until it fits. A single piece larger than the
        // whole budget leaves an empty plan — honestly nothing, never a truncated
        // fragment that reads as complete.
        while plan.trailing_tokens() > budget && !plan.pieces.is_empty() {
            plan.pieces.remove(0);
        }
        Fitted::Reduced { plan, wanted }
    }

    pub(in super::super) fn expand_within(&mut self, budget: usize) {
        let mut used = self.context_reserve() + self.trailing_tokens();
        // Every minimum remains owned before optional details can spend spare
        // room. Preserve broadcast order for standing sources' trailing KV tier.
        for piece in &mut self.pieces {
            let available = budget.saturating_sub(used) + piece.tokens;
            if let Some(expanded) = GroundingPiece::within(piece.contribution, available) {
                used = used - piece.tokens + expanded.tokens;
                *piece = expanded;
            }
        }
    }

    pub fn trailing_tokens(&self) -> usize {
        self.pieces
            .iter()
            .filter(|p| p.contribution.trailing)
            .map(|p| p.tokens)
            .sum()
    }

    pub(in super::super) fn context_tokens(&self) -> usize {
        self.pieces
            .iter()
            .filter(|p| !p.contribution.trailing)
            .map(|p| p.tokens)
            .sum()
    }

    /// What the context half costs rendered: its pieces, plus the header and message
    /// wrapper of the assembled-context message when any piece renders.
    pub fn context_reserve(&self) -> usize {
        let tokens = self.context_tokens();
        tokens
            + if tokens > 0 {
                LlmDeliberationFaculty::working_context_header_cost()
            } else {
                0
            }
    }

    pub(in super::super) fn trailing_messages(&self) -> Vec<ChatMessage> {
        // STABLE PREFIX FIRST (card c119ace7): these blocks ride the conversation tail
        // in ONE canonical order — churn class, then name — never the broadcast order
        // they arrived in. Broadcast order is the arbiter's per-turn salience, and the
        // citizens' own captures (M5, 2026-09-20) showed `[workspace-map]` and
        // `[active-work]` trading places turn to turn, re-prefilling every byte behind
        // the swap with nothing in it changed. The plan's PIECE order is untouched (it
        // is the shedding + expansion priority, `within_budget` / `expand_within`);
        // only the wire order is canonical. See `deliberation_prompt::stable_prefix_order`.
        let mut pieces: Vec<&GroundingPiece<'a>> =
            self.pieces.iter().filter(|p| p.contribution.trailing).collect();
        pieces.sort_by(|a, b| {
            deliberation_prompt::stable_prefix_order(
                a.contribution.faculty.as_str(),
                b.contribution.faculty.as_str(),
            )
        });
        pieces
            .into_iter()
            .map(|piece| {
                let c = piece.contribution;
                let mut body = String::with_capacity(piece.tokens * GUARD_CHARS_PER_TOKEN);
                body.push('[');
                body.push_str(c.faculty.as_str());
                body.push_str("]\n");
                piece.append_body(&mut body);
                ChatMessage::text("user", body)
            })
            .collect()
    }
}
