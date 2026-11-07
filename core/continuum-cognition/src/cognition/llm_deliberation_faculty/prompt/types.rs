use super::*;

/// Calibration from one authoritative provider observation. This scales the
/// CURRENT prompt estimate; a large request's absolute excess is never charged
/// to every later small request. It remains an estimate, not a tokenizer proof.
#[derive(Clone, Copy)]
pub struct PromptCalibration {
    pub(in super::super) estimated: std::num::NonZeroUsize,
    pub(in super::super) measured: std::num::NonZeroU32,
    /// The input a refusal measured, when it came from one.
    pub(in super::super) input_identity: Option<[u8; 32]>,
    /// Whether the request this prices is admitted by the engine's exact count of its finalized
    /// prompt: only then may a price fall below the estimate ([`Self::charge`]).
    pub(in super::super) admission: crate::ai::provenance::PromptAdmission,
}

impl Default for PromptCalibration {
    fn default() -> Self {
        Self {
            estimated: std::num::NonZeroUsize::MIN,
            measured: std::num::NonZeroU32::MIN,
            input_identity: None,
            admission: crate::ai::provenance::PromptAdmission::Estimated,
        }
    }
}

impl PromptCalibration {
    /// A REFUSAL's price (or a refit's): the engine's count of a prompt it refused, over its
    /// estimate. Never below the estimate: a refusal proves the prompt was at least this dense,
    /// never that it was lighter, so it only raises the price.
    pub(in super::super) fn observed(
        estimated: usize,
        measured: u32,
        input_identity: Option<[u8; 32]>,
    ) -> Option<Self> {
        let estimated = std::num::NonZeroUsize::new(estimated)?;
        // An estimate past u32 is priced at the most a count can say.
        let at_least_estimate = estimated.get().min(u32::MAX as usize) as u32;
        Some(Self {
            estimated,
            measured: std::num::NonZeroU32::new(measured.max(at_least_estimate))?,
            input_identity,
            admission: crate::ai::provenance::PromptAdmission::Estimated,
        })
    }

    /// The engine's EXACT count of the finalized prompt it refused (the strict admission guard's
    /// `ContextExceeded`), over its estimate: the price of that very prompt, lighter or denser,
    /// behind the same count. A sparse-to-dense input (code or JSON after a ring of prose) is
    /// priced at the count, once, never at the ring's lighter bound again.
    pub(in super::super) fn counted(estimated: usize, counted: u32, input_identity: [u8; 32]) -> Option<Self> {
        Some(Self {
            estimated: std::num::NonZeroUsize::new(estimated)?,
            measured: std::num::NonZeroU32::new(counted)?,
            input_identity: Some(input_identity),
            admission: crate::ai::provenance::PromptAdmission::EngineCounted,
        })
    }

    /// This density as priced behind `admission`: the request it prices carries the engine's
    /// exact count ([`PromptAdmission::EngineCounted`]) or does not.
    ///
    /// [`PromptAdmission::EngineCounted`]: crate::ai::provenance::PromptAdmission::EngineCounted
    pub(in super::super) fn behind(self, admission: crate::ai::provenance::PromptAdmission) -> Self {
        Self { admission, ..self }
    }

    /// The calibration identity a receipt names: estimated tokens against the engine's count.
    pub(in super::super) fn density(self) -> (u64, u32) {
        (self.estimated.get() as u64, self.measured.get())
    }

    /// This calibration as it prices `input`.
    ///
    /// A ratio any text can have is the binding's density, for every input: the estimate is
    /// bytes / [`continuum_base::token_estimate::GUARD_CHARS_PER_TOKEN`] and a tokenizer
    /// emits at most one token per byte, so text measures at most that many times its
    /// estimate. Her framing, tools and view are the same bytes on the next turn whatever
    /// its input; priced at 1.0 there, the turn after a refusal (29,722 measured for 15,603
    /// estimated) was allocated at half its cost and its charged prompt plus reserve overran
    /// the window by 4,326 (2026-10-23). A larger ratio is no text density (an image the
    /// estimate does not count, or a count of something else), so it prices only the input
    /// it measured, and a changed input is not locked out by it.
    pub(in super::super) fn for_input(self, input: [u8; 32]) -> Self {
        let text_density = u128::from(self.measured.get())
            <= self.estimated.get() as u128 * continuum_base::token_estimate::GUARD_CHARS_PER_TOKEN as u128;
        match self.input_identity {
            Some(key) if key != input && !text_density => Self::default().behind(self.admission),
            _ => self,
        }
    }

    /// `estimate` in the engine's tokens, at this calibration's ratio: above the estimate for a
    /// refusal's price; below it for a measured density whose bound is lighter ([`Density`]) ONLY
    /// behind the engine's exact count ([`PromptAdmission::EngineCounted`]). The engine counted
    /// 0.83 of the estimate on the M5 (2026-11-04, a95afe3d1, 549 samples), and an act charged at
    /// the raw estimate lost ~2,174 tokens of room it never used (302 of 414 acts cut at their
    /// first sentence). Without that count nothing checks a lighter price, so a price never falls
    /// below the estimate. u128 keeps this portable for a full usize-sized estimate times a u32.
    ///
    /// [`PromptAdmission::EngineCounted`]: crate::ai::provenance::PromptAdmission::EngineCounted
    pub fn charge(self, estimate: usize) -> usize {
        let tokens = (estimate as u128 * self.measured.get() as u128)
            .div_ceil(self.estimated.get() as u128)
            .min(usize::MAX as u128) as usize;
        match self.admission {
            crate::ai::provenance::PromptAdmission::Estimated => tokens.max(estimate),
            crate::ai::provenance::PromptAdmission::EngineCounted => tokens,
        }
    }

    /// The estimate that fits `measured_budget` engine tokens: the inverse of [`Self::charge`],
    /// under the same admission rule.
    pub(in super::super) fn prompt_budget(self, measured_budget: usize) -> usize {
        let tokens = (measured_budget as u128 * self.estimated.get() as u128 / self.measured.get() as u128)
            .min(usize::MAX as u128) as usize;
        match self.admission {
            crate::ai::provenance::PromptAdmission::Estimated => tokens.min(measured_budget),
            crate::ai::provenance::PromptAdmission::EngineCounted => tokens,
        }
    }
}

/// One reply's measurement: the estimate a prompt was fitted with, and the engine's own count of it.
type DensitySample = (std::num::NonZeroUsize, std::num::NonZeroU32);

/// Her prompts' measured TEXT density on one binding: PROMPT bytes per prompt token, the unit her
/// text is sized in, and nothing else. It is never KV bytes per token: the physical rate a pool,
/// grant or footprint is sized by belongs to the engine's capacity layer (`capacity`,
/// `serving_plan::ModelFootprint`), and this type cannot reach it: it is private to the
/// deliberation faculty (`pub(in llm_deliberation_faculty)`), so no crate that plans memory can
/// name it. What the fit counts in prompt tokens may be recorded as her need IN TOKENS; it never
/// converts a token to a byte.
///
/// `Measured` is the engine's own count of each recent prompt against the estimate it was fitted
/// with (`delib.prompt.density`), newest last, at most one ring of recent turns
/// ([`crate::cognition::working_set::RECENT_TURNS`], the window every per-mind measurement is read
/// over), under the model the engine reported serving them (`served`). A reply from a different
/// served model starts a fresh ring: a ring is never carried across a model change.
/// (Limitation, named: no engine-reported tokenizer or chat-template revision reaches this seam
/// today; when the fork reports one, it joins `served` as the ring's key. A binding change, a new
/// adapter or window, already drops the ring with its feedback, `PromptFeedback::applies_to`.)
///
/// The fit is priced at the ring's BOUND, its densest ratio, never at a constant: on the M5
/// (2026-11-04, n=131) the bytes/3 guard read above the engine's count by 14% on average and by
/// up to 22% (Aris 13,449 estimated against 11,111 counted), and its densest ratio was 0.99. The
/// bound is an observed envelope, not a proof about the next prompt: the SAFETY authority is the
/// engine's exact count of the finalized request before generation (`/input_tokens`, checked as
/// `PromptCount::Measured { prompt, reply }`), whose `ContextExceeded` refits her at the refusal's
/// price before anything is generated. A denser reply enters the ring at once, so the next prompt
/// is priced at it. `Unmeasured` (no reply on this binding and model yet) prices at the estimate
/// itself, `PromptCalibration::default()` (follow-up: one `/tokenize` count, or the model's
/// measured density on this node, for the first act).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in super::super) enum Density {
    #[default]
    Unmeasured,
    Measured { served: String, samples: std::collections::VecDeque<DensitySample> },
}

/// What a density reads once a reply is measured: the bound the next prompt is priced at, and the
/// ring's mean beside it, each in engine tokens per estimated token, thousandths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct DensityReading {
    pub(in super::super) mean_milli: u64,
    pub(in super::super) bound_milli: u64,
}

impl Density {
    /// This density with one more reply measured under the model the engine reported serving it,
    /// the price its bound sets (the densest ratio in the ring, compared exactly in u128) and its
    /// reading. A reply from another served model starts a fresh ring. `None` when either count
    /// is zero: nothing was measured.
    pub(in super::super) fn observe(&self, estimated: usize, engine: u32, served: &str) -> Option<(Self, PromptCalibration, DensityReading)> {
        let sample = (std::num::NonZeroUsize::new(estimated)?, std::num::NonZeroU32::new(engine)?);
        let mut samples = match self {
            Self::Measured { served: ring_model, samples } if ring_model == served => samples.clone(),
            _ => std::collections::VecDeque::new(),
        };
        if samples.len() == crate::cognition::working_set::RECENT_TURNS {
            samples.pop_front();
        }
        samples.push_back(sample);
        let denser = |(ea, ma): DensitySample, (eb, mb): DensitySample| {
            u128::from(ma.get()) * eb.get() as u128 > u128::from(mb.get()) * ea.get() as u128
        };
        let (bound_estimated, bound_measured) = samples.iter().copied().fold(sample, |worst, s| if denser(s, worst) { s } else { worst });
        let estimated_sum: u128 = samples.iter().map(|(e, _)| e.get() as u128).sum();
        let engine_sum: u128 = samples.iter().map(|(_, m)| u128::from(m.get())).sum();
        let milli = |engine: u128, estimated: u128| (engine * 1_000 / estimated).min(u128::from(u64::MAX)) as u64;
        let reading = DensityReading {
            mean_milli: milli(engine_sum, estimated_sum),
            bound_milli: milli(u128::from(bound_measured.get()), bound_estimated.get() as u128),
        };
        // Priced behind the admission the next request has (`fit_within` sets it per binding).
        let price = PromptCalibration { estimated: bound_estimated, measured: bound_measured, input_identity: None, admission: crate::ai::provenance::PromptAdmission::Estimated };
        Some((Self::Measured { served: served.to_owned(), samples }, price, reading))
    }

    /// How many replies the ring holds (0 unmeasured). Only the ring's own tests read it.
    #[cfg(test)]
    pub(in super::super) fn samples(&self) -> usize {
        match self {
            Self::Unmeasured => 0,
            Self::Measured { samples, .. } => samples.len(),
        }
    }
}

/// One observation in the existing persona-owned faculty. Route/window changes
/// invalidate it; no global cache, room map, or retained request payload.
pub(in super::super) struct PromptFeedback {
    pub(in super::super) binding: Arc<ModelBinding>,
    pub(in super::super) calibration: PromptCalibration,
    pub(in super::super) available: u32,
    /// Her replies' measured density on this binding; a refusal carries it through unchanged.
    pub(in super::super) density: Density,
}

impl PromptFeedback {
    pub(in super::super) fn applies_to(&self, binding: &ModelBinding) -> bool {
        Arc::ptr_eq(&self.binding.adapter, &binding.adapter)
            && self.binding.model == binding.model
            && self.binding.context_window == binding.context_window
    }
}

#[derive(Default)]
/// This act's state of the sources her context view is fed by, besides her working memory
/// (her actions and thoughts are read from it directly).
pub(in super::super) struct Streams {
    pub(in super::super) conversation: crate::cognition::context_streams::conversation::Conversation,
    pub(in super::super) blocks: Vec<crate::cognition::context_streams::blocks::Grounding>,
    /// Her held work: one keyed item per card she holds (STATE-DRIVEN-SURFACE.md).
    pub(in super::super) held: crate::cognition::context_streams::held_work::HeldWorkStream,
    /// What her situation shows now: the perception facts, keyed (`context_streams::facts`).
    pub(in super::super) facts: crate::cognition::context_streams::facts::Facts,
    /// The act whose native call and result this act's tail carries (`latest_result`), held out
    /// of her actions while it is her newest: a sent tail covers it (`View::append_sent`).
    pub(in super::super) latest_act: Option<u64>,
}

/// Prompt roles are provider-facing; these slots retain the substrate's meaning
/// until fitting is complete. They confer no authority on a message's author.
pub struct PromptMessages {
    pub(in super::super) input_identity: [u8; 32],
    pub history: Vec<ChatMessage>,
    /// The sources of her context view; their messages lead `history` once the view has
    /// advanced (`LlmDeliberationFaculty::view_messages`).
    pub(in super::super) streams: Streams,
    /// Source-owned compact alternatives, indexed into history; never inferred from prose.
    pub(in super::super) result_pointers: Vec<(usize, ChatMessage)>,
    pub(in super::super) grounding_at: usize,
    pub(in super::super) stimulus: Option<ChatMessage>,
    /// Atomic required native call/result group (one prose message for legacy receipts).
    pub latest_result: Vec<ChatMessage>,
    /// Public action text retains assistant provenance and required-input pricing.
    pub(in super::super) action_statement: Option<ChatMessage>,
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
    pub fn active_protocol_error(&self) -> Option<String> {
        use crate::ai::types::{ContentPart, MessageContent};
        let mut calls = Vec::new();
        let mut results = Vec::new();
        for message in &self.latest_result {
            if let MessageContent::Parts(parts) = &message.content {
                for part in parts {
                    match part {
                        ContentPart::ToolUse { id, .. } => calls.push(id.as_str()),
                        ContentPart::ToolResult { tool_use_id, .. } => {
                            results.push(tool_use_id.as_str())
                        }
                        _ => {}
                    }
                }
            }
        }
        let unique: std::collections::HashSet<_> = calls.iter().copied().collect();
        (calls != results || unique.len() != calls.len() || calls.iter().any(|id| id.is_empty()))
            .then(|| "active tool receipt has missing, duplicate, or mismatched native call/result IDs; refusing an invalid continuation".to_string())
    }

    pub fn required_tokens(&self) -> usize {
        self.required_parts().total()
    }

    /// The irreducible payload by part, so a turn over its window can say WHICH part is too
    /// large (Aris on the M5, 2026-10-26: on resume after a restart her required payload was
    /// 105k tokens on a 58,880 window, and her loop spun 25 over-window refusals in one second
    /// before it cleared; nothing said what the 105k was).
    pub fn required_parts(&self) -> RequiredParts {
        let cost = |messages: &mut dyn Iterator<Item = &ChatMessage>| -> usize {
            messages.map(|message| LlmDeliberationFaculty::messages_cost(std::slice::from_ref(message))).sum()
        };
        RequiredParts {
            stimulus: cost(&mut self.stimulus.iter()),
            action_statement: cost(&mut self.action_statement.iter()),
            latest_result: cost(&mut self.latest_result.iter()),
            latest_result_count: self.latest_result.len(),
        }
    }

    pub fn tokens(&self) -> usize {
        LlmDeliberationFaculty::messages_cost(&self.history) + self.required_tokens()
    }
}

/// The irreducible payload's parts, in tokens.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RequiredParts {
    pub stimulus: usize,
    pub action_statement: usize,
    pub latest_result: usize,
    pub latest_result_count: usize,
}

impl RequiredParts {
    pub fn total(&self) -> usize {
        self.stimulus + self.action_statement + self.latest_result
    }
}

/// Finite optional representations remaining in the admitted prompt. Grounding
/// stays frozen at its original position; required messages remain one intact tail.
#[derive(Debug, Clone, Default)]
pub struct PromptRefit {
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
pub struct FittedMessages {
    pub refit: PromptRefit,
    pub messages: Vec<ChatMessage>,
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
    pub(in super::super) quantum: usize,
}

impl FitReceipt {
    /// Optional history existed and none of it survived the fit.
    pub(in super::super) fn emptied(&self) -> bool {
        self.history_messages > 0 && self.kept_messages == 0
    }
}

pub use crate::cognition::workspace::PromptCapacityError;

/// A snapshot of exactly what the deliberation faculty sends the model — the
/// glass box over the RAG/prompt. Print it, capture it, diff it across turns.
#[derive(Debug, Clone)]
pub struct DeliberationPromptView {
    // Source-owned boundaries over this exact assembled snapshot, never re-inferred
    // from prose or refreshed from working memory during corrective admission.
    pub refit: PromptRefit,
    pub system: String,
    pub input_identity: [u8; 32],
    /// Cached pre-calibration cost, including selected schemas and framing.
    pub estimated_prompt_tokens: usize,
    /// Her reply's grant in the one allocation: her measured need (no cap). Charged against the
    /// window here, so her view is sized beside it. A pass carries it verbatim as the wire
    /// request's `max_tokens`; an act carries the room its prompt leaves, never less than this
    /// (`request_allowance`). A turn cut at its cap records its cut channel as a lower bound,
    /// so the next claim grows from the evidence.
    pub completion_reserve: u32,
    pub capacity_error: Option<PromptCapacityError>,
    /// The assembled prompt broke the one budget (`prompt/budget.rs`): the act fails and
    /// this prompt is never sent.
    pub budget_fault: Option<String>,
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
    /// Her newest result is shown smaller than whole in this view, at its summary or pointer
    /// with its handle (`fit_newest_result`). A refusal of a view where it was whole may be
    /// the one her measured density shrinks (`verdict/newest_refit.rs`).
    pub newest_result_reduced: bool,
    /// Her newest result's estimated tokens as this view carries it (`RequiredParts::latest_result`):
    /// what a refusal's excess is charged to when it refits her (`verdict/newest_refit.rs`).
    pub newest_result_tokens: usize,
    /// The one allocation this view was fitted under, for the call's capture (FOCUS-CONTRACT
    /// rule 8). `None` when no allocation ran (a fault before the plan).
    pub allocation: Option<crate::ai::provenance::AllocationReceipt>,
    /// Her hands as the one allocation bought them (`tools_epoch.rs`): the request sends
    /// exactly this, never a second selection.
    pub tools: crate::cognition::tool_surface::SelectedSurface<'static>,
    /// The raw tokens the allocation granted her hands: a retry on a server that refuses
    /// `tail_tools` re-selects at this grant, without her learned head.
    pub tools_grant: usize,
    /// Her measured output need as this view's reply claim read it: an act's think budget and
    /// its receipt are sized from it (`act_room.rs`).
    pub act_room: crate::cognition::working_set::ActRoom,
    /// An ACT whose required floors and measured output need do not fit her window
    /// (`ActPlan::irreducible_shortfall`): refused, never sent with a cap below her need.
    pub output_shortfall: Option<continuum_context::allocation::IrreducibleShortfall>,
    /// What a sent tail of this view shows her that her sources also render (`(source, key)`): her
    /// newest act, the stimulus. They do not arrive again from their sources (`View::append_sent`).
    pub tail_covers: Vec<(&'static str, String)>,
}

impl DeliberationPromptView {
    /// Her tail: what follows her view in this prompt (the clock, the ask, her call and its result).
    pub(in super::super) fn tail(&self) -> &[ChatMessage] {
        &self.messages[self.refit.grounding_at.min(self.messages.len())..]
    }

    /// Whether her view will hold this tail exactly as sent once it is served
    /// (`LlmDeliberationFaculty::view_served`): every message is text or native protocol.
    pub(in super::super) fn tail_holdable(&self) -> bool {
        self.tail().iter().all(|message| crate::cognition::context_streams::ContextMessage::holdable(message).is_ok())
    }

    /// Where her next act's prompt stops matching this one, in messages: the engine checkpoints her
    /// state there and her saved page keeps it (`view_messages`). Her whole prompt when its tail
    /// joins her view as sent, so the next act resumes at its end; else where her view ends.
    pub fn resumes_at(&self) -> Option<u32> {
        let at = if self.tail_holdable() { self.messages.len() } else { self.refit.grounding_at };
        u32::try_from(at).ok().filter(|len| *len > 0)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The model the M5 served that day.
    const SERVED: &str = "Qwen3.6-35B-A3B";
    /// Aris on the M5 (2026-11-04, `delib.prompt.density`): 13,449 estimated, 11,111 counted.
    const ARIS: (usize, u32) = (13_449, 11_111);
    /// Two prompts at the fleet's measured mean density the same day (n=131: the guard read 14%
    /// above the engine's count on average): illustrative samples AT that aggregate, not receipts.
    const AT_THE_MEAN: [(usize, u32); 2] = [(12_000, 10_526), (9_000, 7_895)];
    /// The densest prompt that day: ratio 0.99, the guard barely above the engine's count.
    const DENSEST: (usize, u32) = (10_000, 9_900);

    /// The ring's price as a request behind the engine's exact count reads it (`fit_within`).
    fn ring_on(served: &str, from: Density, samples: &[(usize, u32)]) -> (Density, PromptCalibration) {
        samples.iter().fold((from, PromptCalibration::default()), |(density, _), &(estimated, engine)| {
            let (next, price, _) = density.observe(estimated, engine, served).expect("test: positive counts"); // JUSTIFIED: test fixture
            (next, price.behind(crate::ai::provenance::PromptAdmission::EngineCounted))
        })
    }

    fn ring(samples: &[(usize, u32)]) -> (Density, PromptCalibration) {
        ring_on(SERVED, Density::Unmeasured, samples)
    }

    // what this catches: the bytes/3 guard's estimate corrected only upward, so on the M5
    // (2026-11-04, n=131) where it read above the engine's count by 14% on average, the fit priced
    // every prompt as larger than the engine counted. At her measured density the same window
    // holds that much more ESTIMATE (an estimate correction in this sample; what it buys in useful
    // context or acts per hour is measured live, never claimed here).
    #[test]
    fn at_her_measured_density_the_window_holds_more_estimate_than_at_the_guard() {
        let (_, price) = ring(&[ARIS, AT_THE_MEAN[0], AT_THE_MEAN[1]]);
        let window = 100_000;
        assert_eq!(PromptCalibration::default().prompt_budget(window), window, "unmeasured: the estimate itself");
        let measured = price.prompt_budget(window);
        assert!(measured * 100 >= window * 113, "{measured}");
        // Without the engine's exact count nothing checks a lighter price: the estimate stands.
        let unguarded = price.behind(crate::ai::provenance::PromptAdmission::Estimated);
        assert_eq!((unguarded.prompt_budget(window), unguarded.charge(ARIS.0)), (window, ARIS.0), "never below the estimate unguarded");
        let (_, _, reading) = Density::Unmeasured.observe(ARIS.0, ARIS.1, SERVED).expect("test: positive counts"); // JUSTIFIED: test fixture
        assert_eq!(reading.bound_milli, 826, "Aris alone: 11,111 / 13,449");
    }

    // what this catches: a measured density that is not conservative. The densest prompt that day
    // measured 0.99 of its estimate; priced at a mean, it would be undercharged. At the ring's
    // bound every recorded prompt is charged at or above the engine's count.
    #[test]
    fn the_bound_covers_every_prompt_in_the_ring_including_the_densest() {
        let samples = [ARIS, AT_THE_MEAN[0], DENSEST, AT_THE_MEAN[1]];
        let (_, price) = ring(&samples);
        for (estimated, engine) in samples {
            assert!(price.charge(estimated) >= engine as usize, "{estimated} charged {} < engine {engine}", price.charge(estimated));
        }
        assert_eq!(price.charge(DENSEST.0), DENSEST.1 as usize, "the bound is the densest ratio");
    }

    // what this catches (Codex on #5779): drift upward left uncovered. After a ring of light
    // replies, ONE denser reply moves the bound to it, so the very next prompt is priced at it.
    #[test]
    fn a_denser_reply_is_covered_within_one_reply() {
        let (light, light_price) = ring(&[ARIS; 8]);
        assert!(light_price.charge(DENSEST.0) < DENSEST.1 as usize, "precondition: the light ring underprices the dense prompt");
        let (_, price) = ring_on(SERVED, light, &[DENSEST]);
        assert!(price.charge(DENSEST.0) >= DENSEST.1 as usize, "covered after one reply");
    }

    // what this catches (Codex on #5779): a ring carried across a model change. A reply from
    // another served model starts a fresh ring (its own first sample), never the old envelope.
    #[test]
    fn another_served_model_starts_a_fresh_ring() {
        let (dense, _) = ring(&[DENSEST, DENSEST]);
        let (fresh, price) = ring_on("another-model", dense, &[ARIS]);
        assert_eq!(fresh.samples(), 1, "a fresh ring");
        assert_eq!(price.charge(ARIS.0), ARIS.1 as usize, "priced by the new model's own count");
        assert_eq!(Density::Unmeasured.samples(), 0);
    }

    // what this catches: a dense prompt pinning her price forever (both ways): once a ring of
    // lighter replies follows it, it ages out and her bound falls with what she now sends.
    #[test]
    fn a_dense_prompt_ages_out_of_the_ring() {
        let mut samples = vec![DENSEST];
        samples.extend(std::iter::repeat(ARIS).take(crate::cognition::working_set::RECENT_TURNS));
        let (density, price) = ring(&samples);
        assert_eq!(density.samples(), crate::cognition::working_set::RECENT_TURNS);
        assert_eq!(price.charge(ARIS.0), ARIS.1 as usize, "the densest aged out: Aris's own ratio");
    }

    // what this catches: a refusal's price lowering what a prompt costs. A refusal proves the
    // prompt was at least as dense as counted, never lighter; and a zero count measures nothing.
    #[test]
    fn a_refusal_never_prices_below_the_estimate_and_a_zero_count_measures_nothing() {
        let refused = PromptCalibration::observed(10_000, 8_000, None).expect("test: positive counts"); // JUSTIFIED: test fixture
        assert_eq!(refused.charge(10_000), 10_000);
        assert!(Density::Unmeasured.observe(10_000, 0, SERVED).is_none() && Density::Unmeasured.observe(0, 10, SERVED).is_none());
    }
}

