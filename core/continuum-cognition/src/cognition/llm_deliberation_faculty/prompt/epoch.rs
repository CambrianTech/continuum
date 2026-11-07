//! Her context view across acts ([`continuum_context::view::View`]): her conversation, her
//! actions, her standing blocks and her thoughts, as one epoch that only appends.
//!
//! Every act appends the items that changed, so the engine reuses the previous prompt whole
//! and prefills only what is new (replayed on the M5's captures, 2026-10-21: mean prefill
//! per act 70,296 -> 27,815 chars for Aiko, 51,505 -> 15,998 for Aris; continuum-context
//! `examples/replay.rs`). When the epoch outgrows what the window grants her history, it
//! turns over ([`LlmDeliberationFaculty::fit_view`]): the one flexbox grants each source a
//! budget between its pointers and its whole, and the view keeps each source's newest items
//! whole and collapses the older ones. That is the only place anything collapses; nothing is
//! cut in the middle of an epoch.
//!
//! The prompt assembly calls two things: [`LlmDeliberationFaculty::place_view`] to lead the
//! history with the view, and [`LlmDeliberationFaculty::fit_view`] once the window's
//! allocation has granted the history its budget.

use continuum_context::source::Source;
use continuum_context::view::{Claim, View};

mod allocation_estimate;
pub(in super::super) use allocation_estimate::AllocationEstimate;

use super::*;
use crate::cognition::context_streams::{ContextMessage, MemoryStreams};
use crate::cognition::working_memory::WorkingMemory;

/// The prefix of a view source's claim id in the window's one allocation.
const VIEW_CLAIM: &str = continuum_context::allocation::id::VIEW;

/// The source a sent prompt's tail joins her view under (`view_served`).
const SENT_TAIL: &str = "sent";

/// What fitting her view decided: whether a start funded her head's next epoch. What her view lacked
/// past its grant is collapsed at the next start, never her need (`allocation::need`).
#[derive(Clone, Copy, Debug)]
pub(in super::super) struct ViewFitted {
    pub(in super::super) head_funded: bool,
}

/// Where her view sits in this act's history: its leading `len` messages, and whether this
/// act started it.
#[derive(Clone, Copy, Debug)]
pub(in super::super) struct ViewPlacement {
    len: usize,
    started: bool,
}

impl ViewPlacement {
    /// How many leading messages of the history are her view.
    pub(in super::super) fn len(&self) -> usize {
        self.len
    }

}

impl LlmDeliberationFaculty {
    /// Lead `prompt.history` with her view, advanced by this act. What follows it (standing
    /// grounding, facts, the clock) is this act's fresh tail.
    pub(in super::super) fn place_view(&self, prompt: &mut PromptMessages) -> ViewPlacement {
        let (messages, started) = self.advance_view(&prompt.streams);
        let len = messages.len();
        prompt.history.splice(0..0, messages);
        prompt.grounding_at = len;
        ViewPlacement { len, started }
    }

    /// Her view's sources as claims in the window's one allocation, each `view/<source>`, priced by
    /// the view's own claims ([`continuum_context::view::claims`]) at `cost`, the allocation estimate her view
    /// starts by ([`AllocationEstimate`]): floor = the source folded or collapsed, want = the source whole
    /// (a stored body at its measured size), at the source's priority. There is no separate
    /// history claim for the view, so the allocation that weighs her reply also decides, once, how
    /// much of her context she reads, in the very prices the start keeps within.
    pub(in super::super) fn view_claims(&self, streams: &Streams, cost: AllocationEstimate) -> Vec<continuum_context::allocation::ViewClaim> {
        let memory = self.working_memory.as_deref();
        let memory_streams = memory.map(|memory| MemoryStreams::of(memory, self.persona_id));
        let sources = Self::view_sources(streams, memory_streams.as_ref());
        continuum_context::view::claims(&cost, &sources)
            .into_iter()
            .zip(&sources)
            .map(|(claim, source)| continuum_context::allocation::ViewClaim {
                claim: continuum_context::flex::FlexClaim {
                    source_id: format!("{VIEW_CLAIM}{}", claim.source),
                    priority: claim.priority,
                    floor_tokens: Self::clamp_u32(claim.floor),
                    min_tokens: Self::clamp_u32(claim.floor),
                    max_tokens: Self::clamp_u32(claim.full),
                    required: source.required(),
                },
                consumer: source.consumer(),
            })
            .collect()
    }

    /// Keep her view within what the window's one allocation granted its sources, plus `room`
    /// (what no claim took beyond the reply's runaway headroom). While the epoch fits that it
    /// only appends. When it outgrows them a new epoch starts, sized by [`start_grants`] to
    /// leave a growth reserve, so the next acts append instead of turning over again.
    ///
    /// Measured on the M5, 2026-10-21: sized to the 30 s latency target, Aiko's epoch started
    /// at 6,956 of 86,233 tokens and collapsed 40 items (18:20Z), the starvation the view
    /// exists to end. Measured again 2026-10-23: a source granted all it wants restarted
    /// whole, so the start filled its grant; in a room whose window slides, the next act
    /// kept the scrolled-out turns, appended the new ones and turned over again, every act
    /// (Atlas in 11c1a7ac: starts of ~259k chars, 1,902 tokens of room, ~9.4k chars appended
    /// per act; 3-11% of consecutive prompts shared).
    ///
    /// Returns whether a start in this act funded `head_growth` (what her head's next epoch adds, charged),
    /// taken from the start's room beside every required floor, so the next head can be adopted.
    pub(in super::super) fn fit_view(
        &self,
        prompt: &mut PromptMessages,
        view: ViewPlacement,
        act: &continuum_context::allocation::ActPlan,
        calibration: super::types::PromptCalibration,
        head_growth: usize,
    ) -> Result<ViewFitted, ViewSizing> {
        let kept = || Ok(ViewFitted { head_funded: false });
        if view.len == 0 {
            return kept();
        }
        let room = act.room() as usize;
        // A start the plan did not fund (`StartRefused`) is carried out typed: the view's closure
        // contract answers grants only, so the cause is held here and returned in place of the
        // mismatch the empty answer raises.
        let refused: std::cell::Cell<Option<continuum_context::allocation::StartRefused>> = std::cell::Cell::new(None);
        let typed = |mismatch: continuum_context::view::GrantMismatch| match refused.take() {
            Some(start) => ViewSizing::Start(start),
            None => ViewSizing::Mismatch(mismatch),
        };
        // In the allocation's unit: grants and room are charged tokens (`calibration.charge`),
        // so the view is charged too. Compared raw, a view near its grant passed here and the
        // final fit, which converts its budget back to raw, cut 96-99 of its oldest messages
        // from the front with no turnover (Benchy, the M5, 2026-10-23: a head clip, goal 7).
        // Her view's allocation estimate: each block as this prompt is priced, the unit of every grant.
        let cost = AllocationEstimate(calibration);
        let view_tokens = calibration.charge(calibration.estimate_all(&prompt.history[..view.len]));
        let granted = act.view_granted() as usize;
        // The epoch keeps appending into window no claim took: its rendering carries the
        // versions it superseded, which no claim counts, so measured against its grants alone
        // it read as over with 74k of the window unallocated (Atlas, the M5, 2026-10-22).
        // What the epoch being replaced appended per act: the growth the next one reserves.
        let (appended_per_act, view_chars) = self
            .context_view
            .lock()
            .as_ref()
            .map_or((None, 0), |view| (view.appended_per_act(), view.chars())); // map_or: no view yet, nothing appended
        // One act's measured appends, priced at this view's own tokens per character.
        let next_act_tokens = appended_per_act.map(|chars| (chars as u64 * view_tokens as u64).div_ceil(view_chars.max(1) as u64) as usize);
        // An allocator that answered for the wrong number of sources refuses the turn: the held
        // view stays as state, but a prompt built on a broken contract is never sent (Codex on #5564).
        // A start is sized from the same one plan as if the reply held its whole want
        // (`ActPlan::start_room`), so a pass and the act after it start her view alike; whether
        // this act keeps, restores or turns over is judged against what this act was granted.
        // A start funds her head's next epoch from its room only while every required floor still
        // fits beside it; else the head keeps its levels and the start keeps its room.
        let required_floors: usize = act.plan.allocations.iter().filter(|a| a.required && a.source_id.starts_with(VIEW_CLAIM)).map(|a| a.requested_floor as usize).sum();
        let room_all = act.start_room() as usize;
        let funds_head = room_all >= head_growth.saturating_add(required_floors);
        let start_room = if funds_head { room_all - head_growth } else { room_all };
        let sized = |claims: &[Claim]| match start_grants(claims, &act.plan, start_room, next_act_tokens) {
            Ok(grants) => grants,
            Err(start) => {
                refused.set(Some(start));
                Vec::new()
            }
        };
        let due = turnover_due(view_tokens, granted + room, next_act_tokens, || self.view_would_restore(cost, &prompt.streams, &sized))
            .map_err(&typed)?;
        if due == Turnover::Keep {
            return kept();
        }
        let fits = due == Turnover::Restore;
        // Admission before a turnover: redo this act's append within what the act granted her
        // view, an arrival too large for it reduced to its outline or pointer
        // (`View::readmit`). An act that started the view has nothing to redo.
        if !fits && !view.started {
            if let Some(admitted) = self.admit_view(cost, &prompt.streams, granted + room) {
                let admitted_tokens = calibration.charge(calibration.estimate_all(&admitted));
                if admitted_tokens <= granted + room {
                    continuum_base::probe!(
                        class = "delib.view.admitted",
                        persona = %self.persona_name,
                        view_tokens,
                        admitted_tokens,
                        granted,
                        room,
                        "an arrival too large for what the act granted her view came in reduced; \
                         the epoch keeps appending instead of turning over"
                    );
                    prompt.grounding_at = admitted.len();
                    prompt.history.splice(0..view.len, admitted);
                    return kept();
                }
            }
        }
        // A source the allocation could not grant its floor is shed from this epoch: its
        // floor is what it renders collapsed, so starting it anyway put the view over its
        // grant and left the fit to cut it (an atomic standing block, its minimum the whole
        // block, 21,857 tokens on an 8,192 window). Its items wait for a later start that funds it
        // (`View::withhold`), never arriving whole on the next act: Atlas's 287 shed thoughts did
        // (270,351 chars), turning her epoch over every other act (the M5, 2026-11-07).
        // A required source is never shed: it got its floor first, or it is under-provisioned
        // and kept anyway, so the view runs over its grant and the turn is refused
        // (`budget_fault` / `capacity_error`, view.rs), never sent without it.
        // (`escalation_needed` is only probed; it refuses nothing.)
        let shed = act.shed_view_sources();
        let started = self.turn_over_view_with(cost, &prompt.streams, &shed, &sized).map_err(&typed)?;
        prompt.grounding_at = started.len();
        prompt.history.splice(0..view.len, started);
        continuum_base::probe!(
            class = "delib.view.sized",
            persona = %self.persona_name,
            view_tokens,
            granted,
            room,
            appended_per_act = appended_per_act.unwrap_or(0), // unwrap_or: no act yet in the epoch it replaced
            restart = view.started,
            shed_sources = shed.len(),
            "her view outgrew what the window granted its sources and started a new epoch, \
             its growing sources leaving a reserve as large as their start"
        );
        Ok(ViewFitted { head_funded: funds_head })
    }

    /// Append this act's changes to her view and return its messages, and whether this act
    /// started the view (the first act after a start or a restart renders it whole, for the
    /// caller to size).
    fn advance_view(&self, streams: &Streams) -> (Vec<ChatMessage>, bool) {
        let memory = self.working_memory.as_deref();
        let memory_streams = memory.map(|memory| MemoryStreams::of(memory, self.persona_id));
        let sources = Self::view_sources(streams, memory_streams.as_ref());
        let mut held = self.context_view.lock();
        // A restart resumes the view her checkpoint kept, the same bytes the engine's saved
        // page holds, instead of starting a new epoch cold.
        if held.is_none() {
            *held = memory.and_then(WorkingMemory::take_view);
        }
        let started = held.is_none();
        if let Some(view) = held.as_mut() {
            let advance = view.advance(&sources);
            continuum_base::probe!(
                class = "delib.view.advance",
                persona = %self.persona_name,
                epoch = view.epoch_id(),
                appended_items = advance.items,
                appended_chars = advance.chars,
                view_chars = view.chars(),
                superseded_chars = view.superseded_chars(),
                by_source = %view
                    .last_advance_by_source()
                    .iter()
                    .map(|s| format!("{}:{}/{}", s.source, s.items, s.chars))
                    .collect::<Vec<_>>()
                    .join(","),
                "her view appended this act's changes; the prompt before them is unchanged"
            );
        } else {
            let whole = |claims: &[Claim]| claims.iter().map(|claim| claim.full).collect();
            match View::start(&AllocationEstimate::uncalibrated(), Vec::new(), &sources, whole) {
                Ok((view, _)) => {
                    *held = Some(view);
                    self.tool_head_turned_over();
                    if let Some(memory) = memory {
                        memory.reclaim_ended_notices();
                    }
                }
                Err(mismatch) => continuum_base::probe!(
                    class = "delib.view.refused",
                    persona = %self.persona_name,
                    mismatch = %mismatch,
                    "her view could not start"
                ),
            }
        }
        if let Some(memory) = memory {
            memory.keep_view(held.clone());
        }
        (Self::render(held.as_ref()), started)
    }

    /// Redo this act's append to her view within `budget_tokens`, every block priced by her view's
    /// allocation estimate (`cost`), the unit of the budget (the caller re-checks the result in tokens). `None`
    /// when there is no unsent act to redo.
    fn admit_view(&self, cost: AllocationEstimate, streams: &Streams, budget_tokens: usize) -> Option<Vec<ChatMessage>> {
        let memory = self.working_memory.as_deref();
        let memory_streams = memory.map(|memory| MemoryStreams::of(memory, self.persona_id));
        let sources = Self::view_sources(streams, memory_streams.as_ref());
        let mut held = self.context_view.lock();
        let view = held.as_mut()?;
        // What a start would grant one arrival at most: `start_grants` gives her growing
        // sources at most half of what the view may hold, so an item larger than half of it is
        // reduced by a turnover too. Only such an arrival is reduced here; any other stays
        // whole and the caller turns over, shrinking the oldest (goal 7).
        view.readmit(&cost, &sources, budget_tokens, budget_tokens / 2)?;
        if let Some(memory) = memory {
            memory.keep_view(held.clone());
        }
        Some(Self::render(held.as_ref()))
    }

    /// The prompt built on this act's view is final and about to be sent: nothing may rewind
    /// the act it appended any more ([`View::sent`]).
    pub(in super::super) fn view_sent(&self) {
        if let Some(view) = self.context_view.lock().as_mut() {
            view.sent();
        }
    }

    /// Whether her view already shows `source`'s item `item`, in the bytes a sent tail showed it.
    pub(in super::super) fn view_covers(&self, source: &str, item: &str) -> bool {
        self.context_view.lock().as_ref().is_some_and(|view| view.covers(source, item))
    }

    /// Whether a sent tail already showed her `message`, byte for byte: not sent again.
    pub(in super::super) fn view_holds_sent(&self, message: &ChatMessage) -> bool {
        let Ok(held) = ContextMessage::from_chat(message) else {
            return false; // a message her view cannot hold as sent was never held
        };
        self.context_view.lock().as_ref().is_some_and(|view| view.holds_sent(SENT_TAIL, &held))
    }

    /// Drop from this act's tail what a sent tail already showed her (the same ask, the same clock
    /// line, her same statement): it is in her view, byte for byte, where it was sent. Asked after
    /// her view is sized: a start covers nothing, so after a turnover everything is sent again.
    pub(in super::super) fn drop_what_she_was_sent(&self, prompt: &mut PromptMessages) {
        let asked = prompt.streams.conversation.stimulus.as_deref().is_some_and(|key| self.view_covers("conversation", key));
        if asked || prompt.stimulus.as_ref().is_some_and(|message| self.view_holds_sent(message)) {
            prompt.stimulus = None;
        }
        if prompt.action_statement.as_ref().is_some_and(|message| self.view_holds_sent(message)) {
            prompt.action_statement = None;
        }
        let view_len = prompt.grounding_at.min(prompt.history.len());
        let mut tail = prompt.history.split_off(view_len);
        tail.retain(|message| !self.view_holds_sent(message));
        prompt.history.append(&mut tail);
    }

    /// What this act's tail shows her that her sources also render: the ask (her conversation's
    /// held-out turn) and her newest act (held out of her actions while newest).
    pub(in super::super) fn tail_covers(prompt: &PromptMessages) -> Vec<(&'static str, String)> {
        let asked = prompt.stimulus.as_ref().and(prompt.streams.conversation.stimulus.clone()).map(|key| ("conversation", key));
        let acted = (!prompt.latest_result.is_empty()).then_some(prompt.streams.latest_act).flatten().map(|seq| ("actions", seq.to_string()));
        asked.into_iter().chain(acted).collect()
    }

    /// The prompt built on `view` was served: its tail (what follows her view: the clock, the ask,
    /// her call and its result) joins her view in the bytes it was sent ([`View::append_sent`]), so
    /// her next act's prompt is this one plus what is new, and the engine resumes at its end
    /// (`DeliberationPromptView::resumes_at`). A tail carrying media is not held as sent: it
    /// re-renders from its producers next act, as every tail did before.
    pub(in super::super) fn view_served(&self, view: &DeliberationPromptView) {
        let kept: Result<Vec<ContextMessage>, _> = view.tail().iter().map(ContextMessage::from_chat).collect();
        let tail = match kept {
            Ok(tail) => tail,
            Err(not_kept) => {
                continuum_base::probe!(
                    class = "delib.view.tail_not_kept",
                    persona = %self.persona_name,
                    reason = ?not_kept,
                    "her sent tail carries what her view cannot hold as sent: it re-renders next act"
                );
                return;
            }
        };
        let covers: Vec<(&str, &str)> = view.tail_covers.iter().map(|(source, key)| (*source, key.as_str())).collect();
        let memory = self.working_memory.as_deref();
        let mut held = self.context_view.lock();
        let Some(context) = held.as_mut() else {
            return; // no view (no working memory): nothing holds the tail
        };
        let appended = context.append_sent(SENT_TAIL, tail, &covers);
        continuum_base::probe!(
            class = "delib.view.tail_kept",
            persona = %self.persona_name,
            epoch = context.epoch_id(),
            messages = appended.items,
            chars = appended.chars,
            covers = covers.len(),
            "her sent tail joined her view as sent: her next prompt extends this one"
        );
        if let Some(memory) = memory {
            memory.keep_view(held.clone());
        }
    }

    /// Whether her view, started now under `allocate`'s grants in `cost`, would show her more than
    /// it holds.
    fn view_would_restore(&self, cost: AllocationEstimate, streams: &Streams, allocate: impl FnOnce(&[Claim]) -> Vec<usize>) -> Result<bool, continuum_context::view::GrantMismatch> {
        let memory = self.working_memory.as_deref();
        let memory_streams = memory.map(|memory| MemoryStreams::of(memory, self.persona_id));
        let sources = Self::view_sources(streams, memory_streams.as_ref());
        self.context_view.lock().as_ref().map_or(Ok(false), |view| view.start_would_restore(&cost, &sources, allocate)) // map_or: no view yet holds nothing to restore
    }

    /// Start a new epoch of her view under `allocate`'s grants, each kept as `cost` prices what it
    /// renders, without the `shed` sources, whose items wait for a later start that funds them
    /// ([`View::withhold`]).
    fn turn_over_view_with(
        &self,
        cost: AllocationEstimate,
        streams: &Streams,
        shed: &[&str],
        allocate: impl FnOnce(&[Claim]) -> Vec<usize>,
    ) -> Result<Vec<ChatMessage>, continuum_context::view::GrantMismatch> {
        let memory = self.working_memory.as_deref();
        let memory_streams = memory.map(|memory| MemoryStreams::of(memory, self.persona_id));
        let (withheld, sources): (Vec<_>, Vec<_>) =
            Self::view_sources(streams, memory_streams.as_ref()).into_iter().partition(|source| shed.contains(&source.name()));
        let mut held = self.context_view.lock();
        let started = if let Some(view) = held.as_mut() {
            view.turn_over(&cost, Vec::new(), &sources, allocate)
        } else {
            match View::start(&cost, Vec::new(), &sources, allocate) {
                Ok((view, start)) => {
                    *held = Some(view);
                    Ok(start)
                }
                Err(mismatch) => Err(mismatch),
            }
        };
        match started {
            Ok(start) => {
                if let Some(view) = held.as_mut() {
                    view.withhold(&withheld);
                }
                continuum_base::probe!(
                    class = "delib.view.turnover",
                    persona = %self.persona_name,
                    epoch = start.epoch,
                    start_chars = start.chars,
                    collapsed_items = start.collapsed,
                    folded_items = start.folded,
                    restored_items = start.restored_items,
                    restored_chars = start.restored_chars,
                    log_kept_chars = start.log_kept_chars,
                    log_folded = start.log_folded,
                    log_dropped_chars = start.log_dropped_chars,
                    "her view started a new epoch: the one re-prefill an epoch costs, collapsed \
                     by the flexbox at its start"
                );
                self.tool_head_turned_over();
                if let Some(memory) = memory {
                    memory.reclaim_ended_notices();
                }
            }
            // The held view stays as state; the turn built on it is refused (Codex on #5564).
            Err(mismatch) => {
                continuum_base::probe!(
                    class = "delib.view.refused",
                    persona = %self.persona_name,
                    mismatch = %mismatch,
                    "her view could not turn over"
                );
                return Err(mismatch);
            }
        }
        if let Some(memory) = memory {
            memory.keep_view(held.clone());
        }
        Ok(Self::render(held.as_ref()))
    }

    /// Every source whole, outside her view's state: what this turn's history would cost with
    /// no epoch and no collapse, the demand the serving plan sizes her window from.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub(in super::super) fn whole_view(&self, streams: &Streams) -> Vec<ChatMessage> {
        let memory = self.working_memory.as_deref();
        let memory_streams = memory.map(|memory| MemoryStreams::of(memory, self.persona_id));
        let sources = Self::view_sources(streams, memory_streams.as_ref());
        let whole = |claims: &[Claim]| claims.iter().map(|claim| claim.full).collect();
        match View::start(&AllocationEstimate::uncalibrated(), Vec::new(), &sources, whole) {
            Ok((view, _)) => Self::render(Some(&view)),
            Err(mismatch) => {
                continuum_base::probe!(
                    class = "delib.view.refused",
                    persona = %self.persona_name,
                    mismatch = %mismatch,
                    "her whole view could not be rendered for its demand"
                );
                Vec::new()
            }
        }
    }

    /// Her sources in the order an epoch start renders them, the front that survives a
    /// turnover first: the settled blocks (standing and board churn, in their canonical
    /// order), her actions, her conversation, the blocks that count or change within an
    /// epoch (the kanban's claims, recall, awareness, an unknown block), her thoughts, her open
    /// views, then the notices her substrate posted, whose investigation tally changes most acts. Within an epoch the order cannot matter (every act appends), so it is
    /// chosen for the one rewrite an epoch costs: a turnover keeps the prefix up to the
    /// first source whose FRONT changed.
    ///
    /// A growing source's front changes at nearly every turnover, because that is where it
    /// collapses: her conversation led, and a turnover broke the prompt at its first message,
    /// re-prefilling everything behind identity and tools (Benchy on the M5, 2026-10-24
    /// 00:32Z: turnover of 98 items, `delib.prompt.common_prefix` index 2, 20% shared; Delia
    /// on the IntelMac, 10-24 00:14Z: index 2 at `[actions]`, 41%). The settled blocks lead,
    /// then her actions, whose collapsed items keep their pointers so their front outlives a
    /// turnover, then her conversation, whose collapsed turns leave the view. Nothing ahead
    /// of her history counts or dates anything: the kanban's claimable count and claim
    /// column changed on 30% of turns (card 5b09111e), so it rides after her history with
    /// recall's "N ago" ages, awareness and her thoughts.
    fn view_sources<'a>(
        streams: &'a Streams,
        memory: Option<&'a MemoryStreams<'a>>,
    ) -> Vec<&'a dyn Source<ContextMessage>> {
        use crate::cognition::deliberation_prompt::{churn_of, PromptChurn};
        let settled = |block: &&crate::cognition::context_streams::blocks::Grounding| {
            churn_of(&block.faculty) < PromptChurn::Claims
        };
        let as_source = |block: &'a crate::cognition::context_streams::blocks::Grounding| {
            block as &dyn Source<ContextMessage>
        };
        let mut sources: Vec<&dyn Source<ContextMessage>> =
            streams.blocks.iter().filter(settled).map(as_source).collect();
        // Her held work changes only on a card's transition (board churn), so it rides with the
        // settled blocks, where the active-work block it replaced sorted.
        sources.push(&streams.held);
        sources.extend(memory.map(|m| &m.actions as &dyn Source<ContextMessage>));
        sources.push(&streams.conversation);
        sources.extend(streams.blocks.iter().filter(|block| !settled(block)).map(as_source));
        sources.extend(memory.map(|m| &m.thoughts as &dyn Source<ContextMessage>));
        // A task's state changes within an epoch (running, then its end), so it rides with the
        // sources that change, where her dispatched list rendered before.
        sources.extend(memory.map(|m| &m.tasks as &dyn Source<ContextMessage>));
        sources.extend(memory.map(|m| &m.open_views as &dyn Source<ContextMessage>));
        sources.extend(memory.map(|m| &m.notices as &dyn Source<ContextMessage>));
        // What her situation shows now: appended after everything else an act changed, so the
        // facts stand right before this act's tail (the clock, the ask), never ahead of what she
        // already read.
        sources.push(&streams.facts);
        sources
    }

    fn render(view: Option<&View<ContextMessage>>) -> Vec<ChatMessage> {
        view.map_or_else(Vec::new, |view| view.render().map(ContextMessage::to_chat).collect()) // map_or_else: no view yet renders nothing
    }

    fn clamp_u32(chars: usize) -> u32 {
        u32::try_from(chars).unwrap_or(u32::MAX) // unwrap_or: a source past 4G chars is granted the most a claim can say
    }
}

/// Why her view could not be sized this act: the turn is refused either way, typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in super::super) enum ViewSizing {
    /// The allocator answered for a different number of sources than it was asked (a broken
    /// contract: a budget fault).
    Mismatch(continuum_context::view::GrantMismatch),
    /// A start the plan did not fund: a required floor underfunded or past the room (a capacity
    /// refusal, distinct from the reply-only `IrreducibleShortfall`, with which an act proceeds).
    Start(continuum_context::allocation::StartRefused),
}

impl std::fmt::Display for ViewSizing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mismatch(mismatch) => write!(f, "{mismatch}"),
            Self::Start(start) => write!(f, "her view's start was not funded: {start:?}"),
        }
    }
}

/// What her view does this act.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in super::super) enum Turnover {
    /// It fits and keeps appending.
    Keep,
    /// It fits, and a start would show her more than it holds, on an act whose next append
    /// would turn the epoch over anyway.
    Restore,
    /// It outgrew what the window granted it.
    Overflow,
}

/// PURE: whether a view of `view_tokens` within `allowed` (its grants plus the room no claim
/// took) turns over this act. Over `allowed` it must. Within it, a turnover to give her back what
/// a squeezed start collapsed (`would_restore`) re-prefills every token of her view cold (Aris,
/// the M5, 2026-10-31 on 526bbe418: 101-111 s after a turnover vs 6-8 s warm; 32 of 91 turnovers
/// in 3 h were restores of a view that fit), so it is taken only when it is free: the room left
/// holds less than one act's measured appends (`next_act_tokens`), so the next act would turn
/// the epoch over anyway. Otherwise her collapsed items come back at the turnover the epoch's
/// growth forces, each reachable by its handle until then (goal 7). `would_restore` is asked
/// only then, since it prices a whole start.
pub(in super::super) fn turnover_due<E>(
    view_tokens: usize,
    allowed: usize,
    next_act_tokens: Option<usize>,
    would_restore: impl FnOnce() -> Result<bool, E>,
) -> Result<Turnover, E> {
    // NO GRANT, NO TURNOVER: a turn sized before her served window is known (the spawn's
    // placeholder MIN_SERVE_CTX, 2048) allows her view nothing, and turning over against that
    // started her view empty; the next act re-appended every item her sources hold (~2,000
    // items, 20-56M chars on the M5, 2026-11-02 22:29-22:31Z) and turned over again. Her held
    // view is kept; the turn itself is refused for capacity.
    if allowed == 0 {
        return Ok(Turnover::Keep);
    }
    if view_tokens > allowed {
        return Ok(Turnover::Overflow);
    }
    let room_left = allowed - view_tokens;
    if next_act_tokens.is_none_or(|tokens| tokens <= room_left) {
        return Ok(Turnover::Keep);
    }
    Ok(if would_restore()? { Turnover::Restore } else { Turnover::Keep })
}

#[cfg(test)]
thread_local! {
    /// Set by a test: [`start_grants`] answers for one fewer source than it was asked.
    pub(in super::super) static SHORT_GRANTS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Each view source's grant at a turnover, in the allocation's tokens (the unit her view's claims
/// are priced in, [`AllocationEstimate`]): the start the next epoch re-prefills, leaving a growth reserve for
/// the acts that follow to append into. The view keeps within each grant as her allocation estimate prices what
/// it renders; no grant is converted into characters.
///
/// `available` is what the window's one allocation leaves her view (its sources' grants plus
/// the room no claim took), in the allocation's tokens; `per_act_tokens` what one act of the epoch
/// being replaced appended, in the same tokens (`fit_view`'s `next_act_tokens`). The higher-priority sources (the
/// grounding blocks: identity, tools, policy) start whole at their grant, `grounding` in all;
/// they change least and only supersede. The lowest-priority sources (her conversation,
/// results and thoughts), which grow by appends, start at `growing` and the reserve is what
/// is left. The reserve is derived, not picked: an epoch whose growing sources start at
/// `growing` and reserve `reserve` lives `reserve / per_act` acts (`per_act` = the median an
/// act appended, [`View::appended_per_act`]), so their re-prefill amortizes to
/// `growing * per_act / reserve` per act. Holding that to at most one act's own appends gives
/// `reserve >= growing`: the measured rate cancels, and `growing` is at most half of
/// `available - grounding`. (The grounding's own re-prefill adds `grounding / life` per act
/// on top, the price of keeping it whole.) The reserve must also hold one act's appends, so
/// `growing` is at most `available - grounding - per_act` as well, which binds only when an
/// act appends more than half of what the growing sources may hold. Before the epoch being
/// replaced appended anything (`None`) the first bound stands alone. Both are the derivation's,
/// on every machine: no share learned on one box decides how much of what she read she keeps
/// (a quarter, replayed on M5 captures, collapsed every read but her newest on 2026-10-30,
/// ONE-BUDGET.md slice 6).
///
/// That half is the rule only until her re-reads are measured: then each growing source whose
/// items are acts starts at the depth `continuum_context::depth::choose` picks, keeping an item
/// whole exactly when collapsing it would cost more in expected re-reads than it saves in
/// turnover misses (the M5 captures, 2026-11-06: on misses alone, Aiko's best start was 23.9k
/// tokens to the half's 62.6k).
///
/// The one plan's grants are shared by arithmetic, never a second allocation
/// (`allocation::start_shares`): a higher-priority source starts at its whole grant, in priority
/// order, and the lowest priority shares what is left in proportion to what each lacks. The newest end is never cut: a source keeps its newest items whole
/// and collapses its oldest first, at the start only ([`View::turn_over`]).
pub(in super::super) fn start_grants(
    claims: &[Claim],
    plan: &continuum_context::flex::FlexAllocation,
    available: usize,
    per_act: Option<usize>,
) -> Result<Vec<usize>, continuum_context::allocation::StartRefused> {
    let grants: Vec<Option<&continuum_context::flex::SourceAllocation>> = claims
        .iter()
        .map(|claim| plan.allocations.iter().find(|a| a.source_id == format!("{VIEW_CLAIM}{}", claim.source)))
        .collect();
    let lowest = claims.iter().map(|claim| claim.priority).min().unwrap_or(0); // unwrap_or: no claims means nothing to grant
    let grounding: usize = claims
        .iter()
        .zip(&grants)
        .filter(|(claim, _)| claim.priority > lowest)
        .filter_map(|(_, grant)| grant.map(|a| a.allocated_tokens as usize))
        .sum::<usize>()
        .min(available);
    let growable = available - grounding;
    // An act that appended more than the growing sources can hold at all cannot be reserved
    // for at any start, and subtracting it started them at zero: every item collapsed, her
    // newest end with them. A re-home back into a 160-turn room measured 331,230 tokens
    // appended in one act against a 20,693 grant, and her view rendered empty
    // (`provider_context_feedback_bounds_replay_and_scales_next_turn`, 2026-10-23). Such an
    // epoch turns over on the next act whatever its start, so a reserve held for its growth is
    // never used: it starts with the whole growable room. Halved, a 5,204-token start held 1,887
    // chars and her history rendered empty after a scaled calibration
    // (a_scaled_calibration_after_turnovers).
    // `reserve >= growing` with `growing + reserve = growable`: half, from the amortization.
    let growing = match per_act {
        Some(tokens) if tokens >= growable => growable,
        Some(tokens) => (growable / 2).min(growable - tokens),
        // No act measured yet: the half bound alone.
        None => growable / 2,
    };
    // The newest end is never collapsed by the growth reserve (goal 7): halved, a 5,204-token
    // growable start converted to 1,885 chars, less than her one newest turn of 2,040, so her
    // history rendered empty after a scaled calibration (a_scaled_calibration_after_turnovers).
    // The start covers each growing source's newest item whole when the growable room can, at
    // its own price (`Claim::newest`).
    let newest: usize = claims
        .iter()
        .zip(&grants)
        .filter(|(claim, grant)| claim.priority == lowest && grant.is_some())
        .map(|(claim, _)| claim.newest)
        .sum();
    let growing = growing.max(newest.min(growable));
    // Each source's share of the start: arithmetic on the one plan's grants, never a second
    // allocation (`continuum_context::allocation::start_shares`). A REQUIRED floor (her view's
    // collapsed minimum on an act) is never shared away: the start holds every one of them.
    // Each growing source carries its newest item's cost, so the room raised above for it reaches
    // it whatever the shares (`StartSource::newest`).
    let mut sources: Vec<continuum_context::allocation::StartSource> = claims
        .iter()
        .zip(&grants)
        .map(|(claim, grant)| {
            let (floor, max, required, newest) = grant.map_or((0, 0, false, 0), |a| {
                let newest = if claim.priority == lowest { LlmDeliberationFaculty::clamp_u32(claim.newest) } else { 0 };
                (a.requested_floor, a.allocated_tokens, a.required, newest)
            }); // map_or: a source the window granted nothing starts collapsed
            continuum_context::allocation::StartSource { priority: claim.priority, fixed: claim.priority > lowest, required, floor, grant: max, newest }
        })
        .collect();
    // The start holds the required floors when the room the plan gave it can (raised toward them,
    // never past `available`: required means refuse if underfunded, never mint budget, Codex on
    // #5779). An underfunded floor or required floors past the room are the typed `StartRefused`,
    // carried to the turn's capacity refusal by the caller.
    let required_floors: usize = sources.iter().filter(|s| s.required).map(|s| s.floor as usize).sum();
    let share = |growing: usize, sources: &[continuum_context::allocation::StartSource]| {
        let start = (grounding + growing).max(required_floors).min(available);
        continuum_context::allocation::start_shares(LlmDeliberationFaculty::clamp_u32(start), sources).inspect_err(|refused| {
            let (needed, had) = refused.needed_and_had();
            continuum_base::probe!(
                class = "delib.view.start_refused",
                required_tokens = needed,
                start_tokens = had,
                "a view start whose required floors the plan did not fund: refused typed, never a floor dropped or budget minted"
            );
        })
    };
    let mut shares = share(growing, &sources)?;
    // How deep the start collapses, from her measured re-reads and growth
    // (`continuum_context::depth`): each growing source's share where the measurement decides it,
    // the half above where it does not. Every size is already in the allocation's tokens.
    let growing_at: Vec<usize> = (0..claims.len()).filter(|&index| claims[index].priority == lowest && grants[index].is_some()).collect();
    let weighed: Vec<continuum_context::depth::Growing<'_>> = growing_at.iter().map(|&index| claims[index].growing(shares[index] as usize)).collect();
    let room = continuum_context::depth::Room { growable, per_act };
    if let continuum_context::depth::Depth::Chosen(chosen) = continuum_context::depth::choose(room, &weighed) {
        // The growing start is what the lowered grants hold, never the depth's own sum: that sum
        // prices her newest item rounded, so it can fall a token under it.
        let mut start = 0usize;
        for (&index, &chosen_share) in growing_at.iter().zip(&chosen.shares) {
            let source = &mut sources[index];
            source.grant = depth_grant(chosen_share, source);
            start += source.grant as usize;
        }
        shares = share(start.min(growable), &sources)?;
        continuum_base::probe!(
            class = "delib.view.depth",
            start_tokens = start,
            chosen_tokens = chosen.start,
            half_tokens = growing,
            kept_items = chosen.kept,
            collapsed_items = chosen.collapsed,
            miss_per_act = chosen.miss_per_act,
            rereads_per_act = chosen.rereads_per_act,
            reread_tokens_per_act = chosen.reread_per_act,
            "her start collapses to the depth her measured re-reads and growth choose"
        );
    }
    let grants: Vec<usize> = claims
        .iter()
        .zip(&grants)
        .zip(&shares)
        .map(|((claim, grant), &share)| match grant {
            // Granted all it asked: the start renders it whole.
            Some(a) if share >= a.requested_max => claim.full,
            // The share itself: the view keeps within it as her allocation estimate prices each item.
            Some(_) => share as usize,
            None => 0,
        })
        .collect();
    // Test seam (Codex on #5564): an allocator answering for one fewer source than asked, so a
    // test drives the real prompt and contribute boundary with a mismatch.
    #[cfg(test)]
    let grants = {
        let mut grants = grants;
        if SHORT_GRANTS.with(std::cell::Cell::get) {
            grants.pop();
        }
        grants
    };
    Ok(grants)
}

/// How a measured depth lowers one growing source's grant: to the share it chose, never below its
/// floor or her newest item whole (the depth prices that item rounded, `StartSource::newest` ceiled;
/// goal 7), and never above the grant the plan gave it.
fn depth_grant(chosen_share: usize, source: &continuum_context::allocation::StartSource) -> u32 {
    LlmDeliberationFaculty::clamp_u32(chosen_share).max(source.floor).max(source.newest).min(source.grant)
}

#[cfg(test)]
mod tests;
