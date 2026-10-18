//! The fitting types: prompt messages, recent-move evidence, grounding plans, fit receipts.

use super::*;

/// Prompt roles are provider-facing; these slots retain the substrate's meaning
/// until fitting is complete. They confer no authority on a message's author.
pub(super) struct PromptMessages {
    pub(super) input_identity: [u8; 32],
    pub(super) history: Vec<ChatMessage>,
    /// Source-owned compact alternatives, indexed into history; never inferred from prose.
    pub(super) result_pointers: Vec<(usize, ChatMessage)>,
    /// Kept before the append-only result ring even when older history yields.
    pub(super) grounding_tokens: usize,
    pub(super) grounding_at: usize,
    pub(super) stimulus: Option<ChatMessage>,
    /// Atomic required native call/result group (one prose message for legacy receipts).
    pub(super) latest_result: Vec<ChatMessage>,
    /// Public action text retains assistant provenance and required-input pricing.
    pub(super) action_statement: Option<ChatMessage>,
    pub(super) room_updates: Vec<ChatMessage>,
    /// The steps ledger, which changes on every act: it rides after the room updates and
    /// before the pinned result, so nothing stable sits behind it (card 5b09111e).
    pub(super) ledger: Option<ChatMessage>,
    /// Substrate notices cannot disappear with the optional thought bid. Their
    /// existing store owns retention; oversized payloads use the explicit fit refusal.
    pub(super) notices: Vec<ChatMessage>,
}

/// Her OWN most recent move and its outcome, priced — the SUFFIX of the optional history
/// that a binding fill budget must not reach into.
///
/// A suffix, not a run in the middle: you cannot drop from the middle of a conversation,
/// so protecting her act means keeping everything from its opener to the end — including
/// the follow-up question that arrived after it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct RecentMove {
    pub(super) messages: usize,
    pub(super) tokens: usize,
    /// The cluster extends FURTHER BACK than the scan bound: this is a suffix of her
    /// causal chain, not the chain. Rides the receipt so a partial protection never
    /// reads as a whole one.
    pub(super) truncated: bool,
    /// The largest single message in the unit. Separates "this unit does not fit, but
    /// every piece of it COULD have been kept" from "one piece of it cannot be carried
    /// whole under any policy" — see [`EvidenceFit`].
    pub(super) largest_message: usize,
}

/// Whether her most recent causal unit can be held out of a binding drop.
///
/// The seam other machinery composes with — a `cannot-fit` here is an OUTCOME the
/// substrate declares and names on `delib.fill.evidence_cannot_fit`, never a quiet
/// fallback to ordinary dropping. Something too large to fit must never disappear with
/// nobody told; that is the disease this whole change exists to end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EvidenceFit {
    /// No move of hers within the scan bound — there is nothing to protect.
    NoMove,
    /// The unit fits the budget and is held out of the drop.
    Protected,
    /// The unit is larger than the budget, but every message in it would individually
    /// fit. Dropping it forgets a completed act that COULD have been kept, so the turn
    /// refuses instead (Astra's review of #4290): required action/result continuity does
    /// not become optional when it exceeds capacity.
    CannotFit,
    /// One message in the unit is on its own larger than the whole conversation budget.
    ///
    /// No policy can carry that whole, and refusing here does not preserve it — it just
    /// mutes the citizen. That exact refusal is a MEASURED outage: an unbounded required
    /// result produced 836 refusals and zero acts on 2026-09-10, and the substrate's
    /// answer was to cap the result at its share of the window and demote the remainder
    /// to history (the demoted remainder is what lands back here). So this is loud but
    /// not fatal: the oversized member yields like any other optional turn, and the real
    /// fix is condensing it, never a capacity fault
    /// ([[collapse-dont-clip-condense-the-past-never-erode-it]]).
    TooLargeToCarry,
}

impl EvidenceFit {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::NoMove => "no_move",
            Self::Protected => "protected",
            Self::CannotFit => "cannot_fit",
            Self::TooLargeToCarry => "too_large_to_carry",
        }
    }
}

/// What a binding fill budget costs her, in KIND and amount. A clip that takes her own
/// last moves away has to say so on its own receipt: "she repeated herself" must be
/// traceable to "we cut the evidence she had already done it" without reading a capture.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ShedPreview {
    pub(super) messages: usize,
    pub(super) tokens: usize,
    pub(super) own_moves: usize,
    pub(super) results: usize,
}

impl RecentMove {
    /// Can this unit be held out of a drop against `history_budget`? See [`EvidenceFit`]
    /// — a `CannotFit` is declared and named, never a silent fallback.
    pub(super) fn fit_within(self, history_budget: usize) -> EvidenceFit {
        if self.messages == 0 || self.tokens == 0 {
            EvidenceFit::NoMove
        } else if self.tokens <= history_budget {
            EvidenceFit::Protected
        } else if self.largest_message > history_budget {
            EvidenceFit::TooLargeToCarry
        } else {
            EvidenceFit::CannotFit
        }
    }
}

impl PromptMessages {
    /// The suffix that carries her OWN most recent move and its outcome: from the turn
    /// that OPENED the cluster through the end of the history.
    ///
    /// Her move need not be the LAST message — a follow-up question, a room turn, a peer
    /// asking about the thing she just did all arrive after it, and those are exactly the
    /// turns whose answer depends on the act being still visible. So the scan walks back
    /// past trailing ordinary turns to find her move, then back through its cluster, then
    /// one more for the opener (an act without the ask that produced it is the
    /// broken-cluster shape the evictor already refuses to leave at the front).
    ///
    /// # What this does NOT do, stated plainly
    ///
    /// This is an ASSEMBLER HEURISTIC over message roles and prose prefixes, and it is
    /// bounded by [`LlmDeliberationFaculty::CLUSTER_SCAN_MESSAGES`] in both directions:
    ///
    /// - a move further back than the bound (more than N ordinary turns have happened
    ///   since) is NOT protected — `RecentMove::default()`, and the receipt says
    ///   `kept_recent_move_messages=0`;
    /// - a tool chain LONGER than the bound is protected only as a SUFFIX, with
    ///   [`RecentMove::truncated`] set, and no opener is added because the message before
    ///   the cut is another link, not the real opener;
    /// - it does not protect an act referenced by an arbitrarily later message, and it
    ///   does not close recent-action loss.
    ///
    /// The real answer is SOURCE-OWNED CAUSAL GROUPING — the source that emits an act and
    /// its result declares them one unit, and the assembler protects declared units
    /// instead of sniffing prefixes. That is out of reach in this change (Astra's review
    /// of #4290); this is the smaller honest move, with its limits on the receipt.
    pub(super) fn recent_move_evidence(&self) -> RecentMove {
        let history = &self.history;
        let scan = LlmDeliberationFaculty::CLUSTER_SCAN_MESSAGES;
        // Walk back over turns that arrived AFTER her move.
        let mut end = history.len();
        let mut skipped = 0usize;
        while end > 0
            && skipped < scan
            && !LlmDeliberationFaculty::continues_cluster(&history[end - 1])
        {
            end -= 1;
            skipped += 1;
        }
        if end == 0 || !LlmDeliberationFaculty::continues_cluster(&history[end - 1]) {
            return RecentMove::default();
        }
        // …then back through the cluster itself.
        let mut idx = end;
        let mut span = 0usize;
        while idx > 0 && span < scan && LlmDeliberationFaculty::continues_cluster(&history[idx - 1])
        {
            idx -= 1;
            span += 1;
        }
        let truncated = idx > 0 && LlmDeliberationFaculty::continues_cluster(&history[idx - 1]);
        if !truncated && idx > 0 {
            idx -= 1;
        }
        RecentMove {
            messages: history.len() - idx,
            tokens: history[idx..]
                .iter()
                .map(LlmDeliberationFaculty::message_cost)
                .sum(),
            truncated,
            largest_message: history[idx..]
                .iter()
                .map(LlmDeliberationFaculty::message_cost)
                .max()
                .unwrap_or(0), // unwrap_or: the span is non-empty by construction here; 0 is unreachable
        }
    }

    /// What a `history_budget`-sized fill sheds, in kind and amount — oldest first, whole
    /// messages, the same ORDER the evictor drops in.
    ///
    /// AT LEAST this much: the real drop rounds up to a quantum and may advance past a
    /// broken cluster opener, so this under-reports rather than over-reports. It is a
    /// receipt, not a second evictor — the decision stays in one place
    /// ([`LlmDeliberationFaculty::fit_messages`]).
    pub(super) fn shed_preview(&self, history_budget: usize) -> ShedPreview {
        let mut out = ShedPreview::default();
        let total: usize = self
            .history
            .iter()
            .map(LlmDeliberationFaculty::message_cost)
            .sum();
        let mut must_drop = total.saturating_sub(history_budget);
        for message in &self.history {
            if must_drop == 0 {
                break;
            }
            let cost = LlmDeliberationFaculty::message_cost(message);
            must_drop = must_drop.saturating_sub(cost);
            out.messages += 1;
            out.tokens += cost;
            if LlmDeliberationFaculty::is_move_result(message) {
                out.results += 1;
            } else if LlmDeliberationFaculty::is_own_move(message) {
                out.own_moves += 1;
            }
        }
        out
    }

    /// Native active batches must preserve exact one-to-one correlation. Legacy
    /// prose has no native calls; it is not reconstructed or silently substituted.
    pub(super) fn active_protocol_error(&self) -> Option<String> {
        use crate::ai::types::{ContentPart, MessageContent};
        let mut calls = Vec::new();
        let mut results = Vec::new();
        for message in &self.latest_result {
            if let MessageContent::Parts(parts) = &message.content {
                for part in parts {
                    match part {
                        ContentPart::ToolUse { id, .. } => calls.push(id.as_str()),
                        ContentPart::ToolResult { tool_use_id, .. } => results.push(tool_use_id.as_str()),
                        _ => {}
                    }
                }
            }
        }
        let unique: std::collections::HashSet<_> = calls.iter().copied().collect();
        (calls != results || unique.len() != calls.len() || calls.iter().any(|id| id.is_empty()))
            .then(|| "active tool receipt has missing, duplicate, or mismatched native call/result IDs; refusing an invalid continuation".to_string())
    }

    pub(super) fn required_tokens(&self) -> usize {
        self.stimulus
            .iter()
            .chain(self.room_updates.iter())
            .chain(self.ledger.iter())
            .chain(self.notices.iter())
            .chain(self.action_statement.iter())
            .chain(self.latest_result.iter())
            .map(|message| LlmDeliberationFaculty::messages_cost(std::slice::from_ref(message)))
            .sum::<usize>()
            + self.grounding_tokens
    }

    pub(super) fn tokens(&self) -> usize {
        LlmDeliberationFaculty::messages_cost(&self.history) + self.required_tokens()
    }
}

/// Finite optional representations remaining in the admitted prompt. Grounding
/// stays frozen at its original position; required messages remain one intact tail.
#[derive(Debug, Clone, Default)]
pub(super) struct PromptRefit {
    pub(super) history_len: usize,
    pub(super) grounding_at: usize,
    pub(super) grounding_len: usize,
    pub(super) result_pointers: Vec<(usize, ChatMessage)>,
}

impl PromptRefit {
    pub(super) fn remaining_reductions(&self) -> usize {
        self.history_len.saturating_add(self.result_pointers.len())
    }
}

#[derive(Default)]
pub(super) struct FittedMessages {
    pub(super) refit: PromptRefit,
    pub(super) messages: Vec<ChatMessage>,
    pub(super) grounding_at: usize,
    /// What the fit did to her optional history (card d33e928a, Kimi's finding): the
    /// front-drop was silent, so a turn whose reduction emptied her history — every room
    /// message, operator direction included, gone before compose — left no receipt.
    pub(super) receipt: FitReceipt,
}

/// The fit's own account of the optional history it was handed and what survived.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FitReceipt {
    pub(super) history_messages: usize,
    pub(super) history_tokens: usize,
    pub(super) kept_messages: usize,
    pub(super) dropped_messages: usize,
    pub(super) dropped_tokens: usize,
    pub(super) required_tokens: usize,
    pub(super) history_budget: usize,
    pub(super) quantum: usize,
}

impl FitReceipt {
    /// Optional history existed and none of it survived the fit.
    pub(super) fn emptied(&self) -> bool {
        self.history_messages > 0 && self.kept_messages == 0
    }
}

#[derive(Default)]
pub(super) struct ContextComposition {
    pub(super) context_window: u32,
    pub(super) framing_tokens: usize,
    pub(super) conversation_tokens: usize,
    pub(super) ctx_floor: usize,
    pub(super) after_framing: usize,
    pub(super) tool_tokens: usize,
}

/// A borrowed selection of whole source content or its declared atomic prefix.
/// Reserving and expanding it never copies the source payload.
pub(super) struct GroundingPiece<'a> {
    pub(super) contribution: &'a Contribution,
    /// None keeps the source's original rendering, including its separators.
    pub(super) units: Option<usize>,
    pub(super) tokens: usize,
}

impl<'a> GroundingPiece<'a> {
    fn cost(c: &Contribution, body_bytes: usize) -> usize {
        if c.trailing {
            (c.faculty.as_str().len() + body_bytes + "[]\n".len()).div_ceil(GUARD_CHARS_PER_TOKEN)
                + LlmDeliberationFaculty::PER_MESSAGE_TEMPLATE_TOKENS
        } else {
            LlmDeliberationFaculty::context_piece_tokens(c.faculty.as_str(), body_bytes)
        }
    }

    pub(super) fn whole(c: &'a Contribution) -> Self {
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

    const NOTICE_SUFFIX: &'static str = " more not shown (context budget)";
    const EXPAND_PREFIX: &'static str = " — run `";
    const EXPAND_SUFFIX: &'static str = "` to see all ";

    /// The omission notice, exactly as `append_notice` writes it. `omitted` is what
    /// THIS packing left out of `parts`. A source that cut its own list short adds
    /// an uncounted remainder: the count becomes `N+` (or no number when the packing
    /// omitted nothing), and the verb says "see all" with no total, because the
    /// true total is unknown.
    fn notice_text(c: &Contribution, omitted: usize) -> String {
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

    fn notice_bytes(c: &Contribution, omitted: usize) -> usize {
        Self::notice_text(c, omitted).len()
    }

    fn append_notice(c: &Contribution, omitted: usize, out: &mut String) {
        out.push_str(&Self::notice_text(c, omitted));
    }

    pub(super) fn minimum(c: &'a Contribution) -> Self {
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

    pub(super) fn within(c: &'a Contribution, budget: usize) -> Option<Self> {
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

    pub(super) fn append_body(&self, out: &mut String) {
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
pub(super) struct GroundingPlan<'a> {
    pub(super) pieces: Vec<GroundingPiece<'a>>,
}

/// A plan that fit as asked, or one that had to give something up — with the size it
/// WANTED carried out, so the caller can say by how much rather than only that it did.
/// A reduction that cannot report its own magnitude is the saturating-subtract mistake
/// one layer up.
pub(super) enum Fitted<T> {
    Whole(T),
    Reduced { plan: T, wanted: usize },
}

impl<'a> GroundingPlan<'a> {
    /// The standing sources' truthful minimums, whatever the budget — the plan the
    /// caller WANTS. Fitting it is [`GroundingPlan::within_budget`]'s job.
    pub(super) fn minimum(ws: &'a Workspace) -> Self {
        Self {
            pieces: ws
                .broadcast
                .iter()
                .filter(|c| c.decision.is_none() && c.standing_grounding)
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
    /// What sheds first, and why: the SYSTEM half goes before the TRAILING half. Trailing
    /// pieces sit nearest the generation and carry the act-shaped facts (the pinned
    /// result, the live board); the system half is standing background, and dropping it
    /// also frees `working_context_header_cost` in one go. Pieces are shed newest-kept /
    /// oldest-first within each half, so what survives is the freshest world she has.
    pub(super) fn within_budget(ws: &'a Workspace, budget: usize) -> Fitted<Self> {
        let whole = Self::minimum(ws);
        let wanted = whole.system_reserve() + whole.trailing_tokens();
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

    pub(super) fn expand_within(&mut self, budget: usize) {
        let mut used = self.system_reserve() + self.trailing_tokens();
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

    pub(super) fn trailing_tokens(&self) -> usize {
        self.pieces
            .iter()
            .filter(|p| p.contribution.trailing)
            .map(|p| p.tokens)
            .sum()
    }

    pub(super) fn system_tokens(&self) -> usize {
        self.pieces
            .iter()
            .filter(|p| !p.contribution.trailing)
            .map(|p| p.tokens)
            .sum()
    }

    pub(super) fn system_reserve(&self) -> usize {
        let tokens = self.system_tokens();
        tokens
            + if tokens > 0 {
                LlmDeliberationFaculty::working_context_header_cost()
            } else {
                0
            }
    }

    pub(super) fn trailing_messages(&self) -> Vec<ChatMessage> {
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
