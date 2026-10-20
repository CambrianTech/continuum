use super::*;

/// Calibration from one authoritative provider observation. This scales the
/// CURRENT prompt estimate; a large request's absolute excess is never charged
/// to every later small request. It remains an estimate, not a tokenizer proof.
#[derive(Clone, Copy)]
pub(in super::super) struct PromptCalibration {
    pub(in super::super) estimated: std::num::NonZeroUsize,
    pub(in super::super) measured: std::num::NonZeroU32,
    pub(in super::super) input_identity: Option<[u8; 32]>,
}

impl Default for PromptCalibration {
    fn default() -> Self {
        Self {
            estimated: std::num::NonZeroUsize::MIN,
            measured: std::num::NonZeroU32::MIN,
            input_identity: None,
        }
    }
}

impl PromptCalibration {
    pub(in super::super) fn observed(estimated: usize, measured: u32, input_identity: Option<[u8; 32]>) -> Option<Self> {
        Some(Self {
            estimated: std::num::NonZeroUsize::new(estimated)?,
            measured: std::num::NonZeroU32::new(measured)?,
            input_identity,
        })
    }

    pub(in super::super) fn charge(self, estimate: usize) -> usize {
        let tokens =
            (estimate as u128 * self.measured.get() as u128).div_ceil(self.estimated.get() as u128);
        // Never underprice the existing estimate. u128 keeps this portable even
        // for a full usize-sized prompt estimate multiplied by a u32 count.
        estimate.max(tokens.min(usize::MAX as u128) as usize)
    }

    pub(in super::super) fn prompt_budget(self, measured_budget: usize) -> usize {
        let tokens =
            measured_budget as u128 * self.estimated.get() as u128 / self.measured.get() as u128;
        measured_budget.min(tokens.min(usize::MAX as u128) as usize)
    }
}

/// One observation in the existing persona-owned faculty. Route/window changes
/// invalidate it; no global cache, room map, or retained request payload.
pub(in super::super) struct PromptFeedback {
    pub(in super::super) binding: Arc<ModelBinding>,
    pub(in super::super) calibration: PromptCalibration,
    pub(in super::super) available: u32,
}

impl PromptFeedback {
    pub(in super::super) fn applies_to(&self, binding: &ModelBinding) -> bool {
        Arc::ptr_eq(&self.binding.adapter, &binding.adapter)
            && self.binding.model == binding.model
            && self.binding.context_window == binding.context_window
    }
}

/// Prompt roles are provider-facing; these slots retain the substrate's meaning
/// until fitting is complete. They confer no authority on a message's author.
pub(in super::super) struct PromptMessages {
    pub(in super::super) input_identity: [u8; 32],
    pub(in super::super) history: Vec<ChatMessage>,
    /// The keyed sources `history[..grounding_at]` renders; her epoch appends their news.
    pub(in super::super) body: Vec<epoch_view::Block>,
    /// Tokens a front drop frees past the minimum: the room her next epoch grows into.
    pub(in super::super) headroom: usize,
    /// Source-owned compact alternatives, indexed into history; never inferred from prose.
    pub(in super::super) result_pointers: Vec<(usize, ChatMessage)>,
    /// Kept before the append-only result ring even when older history yields.
    pub(in super::super) grounding_tokens: usize,
    pub(in super::super) grounding_at: usize,
    pub(in super::super) stimulus: Option<ChatMessage>,
    /// Atomic required native call/result group (one prose message for legacy receipts).
    pub(in super::super) latest_result: Vec<ChatMessage>,
    /// Public action text retains assistant provenance and required-input pricing.
    pub(in super::super) action_statement: Option<ChatMessage>,
    pub(in super::super) room_updates: Vec<ChatMessage>,
    /// The steps ledger, which changes on every act: it rides after the room updates and
    /// before the pinned result, so nothing stable sits behind it (card 5b09111e).
    pub(in super::super) ledger: Option<ChatMessage>,
    /// Substrate notices cannot disappear with the optional thought bid. Their
    /// existing store owns retention; oversized payloads use the explicit fit refusal.
    pub(in super::super) notices: Vec<(&'static str, ChatMessage)>,
}

/// Her OWN most recent move and its outcome, priced — the SUFFIX of the optional history
/// that a binding fill budget must not reach into.
///
/// A suffix, not a run in the middle: you cannot drop from the middle of a conversation,
/// so protecting her act means keeping everything from its opener to the end — including
/// the follow-up question that arrived after it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in super::super) struct RecentMove {
    pub(in super::super) messages: usize,
    pub(in super::super) tokens: usize,
    /// The cluster extends FURTHER BACK than the scan bound: this is a suffix of her
    /// causal chain, not the chain. Rides the receipt so a partial protection never
    /// reads as a whole one.
    pub(in super::super) truncated: bool,
    /// The largest single message in the unit. Separates "this unit does not fit, but
    /// every piece of it COULD have been kept" from "one piece of it cannot be carried
    /// whole under any policy" — see [`EvidenceFit`].
    pub(in super::super) largest_message: usize,
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
    pub(in super::super) fn as_str(self) -> &'static str {
        match self {
            Self::NoMove => "no_move",
            Self::Protected => "protected",
            Self::CannotFit => "cannot_fit",
            Self::TooLargeToCarry => "too_large_to_carry",
        }
    }
}

impl RecentMove {
    /// Can this unit be held out of a drop against `history_budget`? See [`EvidenceFit`]
    /// — a `CannotFit` is declared and named, never a silent fallback.
    pub(in super::super) fn fit_within(self, history_budget: usize) -> EvidenceFit {
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
    pub(in super::super) fn recent_move_evidence(&self) -> RecentMove {
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

    /// Native active batches must preserve exact one-to-one correlation. Legacy
    /// prose has no native calls; it is not reconstructed or silently substituted.
    pub(in super::super) fn active_protocol_error(&self) -> Option<String> {
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

    /// Her epoch turned over: the history's epoch part becomes the compacted view (with the result
    /// pointers the fit may substitute), the growth room is freed, the standing pieces count again.
    /// Returns its budget: all fresh, so `start` (net of framing and tools), never below her required input.
    pub(in super::super) fn restart_epoch(&mut self, epoch: &mut epoch_view::EpochView, cause: epoch_view::TurnoverCause, standing: &[epoch_view::Block], standing_unsaid: usize, headroom: usize, start: usize, persona: &str) -> usize {
        let (len, pointers) = epoch.turn_over(&mut self.history, self.grounding_at, &self.body, cause, persona);
        self.grounding_at = len;
        self.result_pointers = pointers;
        self.headroom = headroom;
        self.grounding_tokens = self.grounding_tokens - standing_unsaid
            + epoch.unsaid_tokens(standing, LlmDeliberationFaculty::message_cost);
        start.max(self.required_tokens())
    }

    pub(in super::super) fn required_tokens(&self) -> usize {
        self.stimulus
            .iter()
            .chain(self.room_updates.iter())
            .chain(self.ledger.iter())
            .chain(self.notices.iter().map(|(_, notice)| notice))
            .chain(self.action_statement.iter())
            .chain(self.latest_result.iter())
            .map(|message| LlmDeliberationFaculty::messages_cost(std::slice::from_ref(message)))
            .sum::<usize>()
            + self.grounding_tokens
    }

    pub(in super::super) fn tokens(&self) -> usize {
        LlmDeliberationFaculty::messages_cost(&self.history) + self.required_tokens()
    }
}

/// Finite optional representations remaining in the admitted prompt. Grounding
/// stays frozen at its original position; required messages remain one intact tail.
#[derive(Debug, Clone, Default)]
pub(in super::super) struct PromptRefit {
    pub(in super::super) history_len: usize,
    pub(in super::super) grounding_at: usize,
    pub(in super::super) grounding_len: usize,
    pub(in super::super) result_pointers: Vec<(usize, ChatMessage)>,
}

impl PromptRefit {
    pub(in super::super) fn remaining_reductions(&self) -> usize {
        self.history_len.saturating_add(self.result_pointers.len())
    }
}

#[derive(Default)]
pub(in super::super) struct FittedMessages {
    pub(in super::super) refit: PromptRefit,
    pub(in super::super) messages: Vec<ChatMessage>,
    pub(in super::super) grounding_at: usize,
    /// What the fit did to her optional history (card d33e928a, Kimi's finding): the
    /// front-drop was silent, so a turn whose reduction emptied her history — every room
    /// message, operator direction included, gone before compose — left no receipt.
    pub(in super::super) receipt: FitReceipt,
}

/// The fit's own account of the optional history it was handed and what survived.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(in super::super) struct FitReceipt {
    pub(in super::super) history_messages: usize,
    pub(in super::super) history_tokens: usize,
    pub(in super::super) kept_messages: usize,
    pub(in super::super) dropped_messages: usize,
    pub(in super::super) dropped_tokens: usize,
    pub(in super::super) required_tokens: usize,
    pub(in super::super) history_budget: usize,
    pub(in super::super) headroom: usize,
}

impl FitReceipt {
    /// Optional history existed and none of it survived the fit.
    pub(in super::super) fn emptied(&self) -> bool {
        self.history_messages > 0 && self.kept_messages == 0
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
    pub(in super::super) refit: PromptRefit,
    pub system: String,
    pub(in super::super) input_identity: [u8; 32],
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
    /// Her epoch with this view's body, committed only if the model answers this view.
    pub(in super::super) epoch: Option<epoch_view::EpochView>,
}

impl DeliberationPromptView {
    /// Index of the first message after her epoch body: the body, then the standing pieces
    /// and grounding appended after the fit, end here; the per-act tail starts here.
    pub(crate) fn stable_prefix_messages(&self) -> usize {
        self.refit.grounding_at + self.refit.grounding_len
    }

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
