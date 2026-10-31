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

use super::*;
use crate::cognition::context_streams::{ContextMessage, MemoryStreams};
use crate::cognition::working_memory::WorkingMemory;

/// The prefix of a view source's claim id in the window's one allocation.
const VIEW_CLAIM: &str = "view/";

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

    /// Her view's sources as claims in the window's one allocation, each `view/<source>`,
    /// priced in the unit the final fit charges ([`Self::message_cost`], template overhead
    /// included): floor = the source folded or collapsed, want = the source whole, at the source's
    /// priority. There is no separate history claim for the view, so the allocation that
    /// weighs her reply also decides, once, how much of her context she reads.
    pub(in super::super) fn view_claims(&self, streams: &Streams) -> Vec<crate::cognition::rag_budget::RagSourceBudget> {
        let memory = self.working_memory.as_deref();
        let memory_streams = memory.map(|memory| MemoryStreams::of(memory, self.persona_id));
        Self::view_sources(streams, memory_streams.as_ref())
            .into_iter()
            .map(|source| {
                let items = source.items();
                let cost = |message: &ContextMessage| Self::message_cost(&message.to_chat());
                // Its floor from the view's one owner: one fold block when the source folds its
                // oldest run, else each item's pointer, so the allocation prices the true floor.
                let floor = continuum_context::view::floor(source, &items, cost);
                // An item's WHOLE level, stored or resident (`whole_chars`): a body evicted from RAM
                // is priced from its measured size, never as the pointer resident in its place,
                // or the allocation would never grant room to see it (FOCUS-CONTRACT rule 7).
                let whole = |item: &continuum_context::source::Item<ContextMessage>| {
                    let extra = continuum_context::source::whole_chars(item).saturating_sub(item.full().text.chars().count());
                    cost(item.full()) + extra / continuum_base::token_estimate::GUARD_CHARS_PER_TOKEN
                };
                let want: usize = items.iter().map(whole).sum();
                crate::cognition::rag_budget::RagSourceBudget {
                    source_id: format!("{VIEW_CLAIM}{}", source.name()),
                    priority: source.priority(),
                    floor_tokens: Self::clamp_u32(floor),
                    min_tokens: Self::clamp_u32(floor),
                    max_tokens: Self::clamp_u32(want),
                    required: source.required(),
                }
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
    pub(in super::super) fn fit_view(
        &self,
        prompt: &mut PromptMessages,
        view: ViewPlacement,
        plan: &crate::cognition::rag_budget::FlexAllocation,
        room: usize,
        calibration: super::types::PromptCalibration,
    ) -> Result<(), continuum_context::view::GrantMismatch> {
        if view.len == 0 {
            return Ok(());
        }
        // In the allocation's unit: grants and room are charged tokens (`calibration.charge`),
        // so the view is charged too. Compared raw, a view near its grant passed here and the
        // final fit, which converts its budget back to raw, cut 96-99 of its oldest messages
        // from the front with no turnover (Benchy, the M5, 2026-10-23: a head clip, goal 7).
        let view_tokens = calibration.charge(Self::messages_cost(&prompt.history[..view.len]));
        let granted: usize = plan
            .allocations
            .iter()
            .filter(|a| a.source_id.starts_with(VIEW_CLAIM))
            .map(|a| a.allocated_tokens as usize)
            .sum();
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
        let due = turnover_due(view_tokens, granted + room, next_act_tokens, || {
            self.view_would_restore(&prompt.streams, |claims: &[Claim]| start_grants(claims, plan, granted + room, appended_per_act))
        })?;
        if due == Turnover::Keep {
            return Ok(());
        }
        let fits = due == Turnover::Restore;
        // Admission before a turnover: redo this act's append within what the act granted her
        // view, an arrival too large for it reduced to its outline or pointer
        // (`View::readmit`). An act that started the view has nothing to redo.
        if !fits && !view.started {
            if let Some(admitted) = self.admit_view(&prompt.streams, view_tokens, granted + room) {
                let admitted_tokens = calibration.charge(Self::messages_cost(&admitted));
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
                    return Ok(());
                }
            }
        }
        // A source the allocation could not grant its floor is shed from this epoch: its
        // floor is what it renders collapsed, so starting it anyway put the view over its
        // grant and left the fit to cut it (an atomic standing block, its minimum the whole
        // block, 21,857 tokens on an 8,192 window). A later act's advance offers it again.
        // A required source is never shed: it got its floor first, or it is under-provisioned
        // and kept anyway, so the view runs over its grant and the turn is refused
        // (`budget_fault` / `capacity_error`, view.rs), never sent without it.
        // (`escalation_needed` is only probed; it refuses nothing.)
        let shed: Vec<&str> = plan
            .allocations
            .iter()
            .filter(|a| a.allocated_tokens < a.requested_floor)
            .filter(|a| a.state != crate::cognition::rag_budget::AllocationState::UnderProvisioned)
            .filter_map(|a| a.source_id.strip_prefix(VIEW_CLAIM))
            .collect();
        let started = self.turn_over_view_with(&prompt.streams, &shed, |claims: &[Claim]| {
            start_grants(claims, plan, granted + room, appended_per_act)
        })?;
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
        Ok(())
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
                "her view appended this act's changes; the prompt before them is unchanged"
            );
        } else {
            let whole = |claims: &[Claim]| claims.iter().map(|claim| claim.full_chars).collect();
            match View::start(Vec::new(), &sources, whole) {
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

    /// Redo this act's append to her view within `budget_tokens`, converted to characters at
    /// the view's own measured ratio (ballpark: an arrival is admitted or reduced whole, and
    /// the caller re-checks the result in tokens). `None` when there is no unsent act to redo.
    fn admit_view(&self, streams: &Streams, view_tokens: usize, budget_tokens: usize) -> Option<Vec<ChatMessage>> {
        let memory = self.working_memory.as_deref();
        let memory_streams = memory.map(|memory| MemoryStreams::of(memory, self.persona_id));
        let sources = Self::view_sources(streams, memory_streams.as_ref());
        let mut held = self.context_view.lock();
        let view = held.as_mut()?;
        let limit = (budget_tokens as u128 * view.chars() as u128 / view_tokens.max(1) as u128) as usize;
        // What a start would grant one arrival at most: `start_grants` gives her growing
        // sources at most half of what the view may hold, so an item larger than half of it is
        // reduced by a turnover too. Only such an arrival is reduced here; any other stays
        // whole and the caller turns over, shrinking the oldest (goal 7).
        view.readmit(&sources, limit, limit / 2)?;
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

    /// Start a new epoch of her view within `budget_chars`, the flexbox granting each
    /// source its share by priority, without the `shed` sources.
    /// Whether her view, started now under `allocate`'s grants, would show her more than it holds.
    fn view_would_restore(&self, streams: &Streams, allocate: impl FnOnce(&[Claim]) -> Vec<usize>) -> Result<bool, continuum_context::view::GrantMismatch> {
        let memory = self.working_memory.as_deref();
        let memory_streams = memory.map(|memory| MemoryStreams::of(memory, self.persona_id));
        let sources = Self::view_sources(streams, memory_streams.as_ref());
        self.context_view.lock().as_ref().map_or(Ok(false), |view| view.start_would_restore(&sources, allocate)) // map_or: no view yet holds nothing to restore
    }

    fn turn_over_view_with(
        &self,
        streams: &Streams,
        shed: &[&str],
        allocate: impl FnOnce(&[Claim]) -> Vec<usize>,
    ) -> Result<Vec<ChatMessage>, continuum_context::view::GrantMismatch> {
        let memory = self.working_memory.as_deref();
        let memory_streams = memory.map(|memory| MemoryStreams::of(memory, self.persona_id));
        let mut sources = Self::view_sources(streams, memory_streams.as_ref());
        sources.retain(|source| !shed.contains(&source.name()));
        let mut held = self.context_view.lock();
        let started = if let Some(view) = held.as_mut() {
            view.turn_over(Vec::new(), &sources, allocate)
        } else {
            match View::start(Vec::new(), &sources, allocate) {
                Ok((view, start)) => {
                    *held = Some(view);
                    Ok(start)
                }
                Err(mismatch) => Err(mismatch),
            }
        };
        match started {
            Ok(start) => {
                continuum_base::probe!(
                    class = "delib.view.turnover",
                    persona = %self.persona_name,
                    epoch = start.epoch,
                    start_chars = start.chars,
                    collapsed_items = start.collapsed,
                    folded_items = start.folded,
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
        let whole = |claims: &[Claim]| claims.iter().map(|claim| claim.full_chars).collect();
        match View::start(Vec::new(), &sources, whole) {
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
        sources.extend(memory.map(|m| &m.actions as &dyn Source<ContextMessage>));
        sources.push(&streams.conversation);
        sources.extend(streams.blocks.iter().filter(|block| !settled(block)).map(as_source));
        sources.extend(memory.map(|m| &m.thoughts as &dyn Source<ContextMessage>));
        // A task's state changes within an epoch (running, then its end), so it rides with the
        // sources that change, where her dispatched list rendered before.
        sources.extend(memory.map(|m| &m.tasks as &dyn Source<ContextMessage>));
        sources.extend(memory.map(|m| &m.open_views as &dyn Source<ContextMessage>));
        sources.extend(memory.map(|m| &m.notices as &dyn Source<ContextMessage>));
        sources
    }

    fn render(view: Option<&View<ContextMessage>>) -> Vec<ChatMessage> {
        view.map_or_else(Vec::new, |view| view.render().map(ContextMessage::to_chat).collect()) // map_or_else: no view yet renders nothing
    }

    fn clamp_u32(chars: usize) -> u32 {
        u32::try_from(chars).unwrap_or(u32::MAX) // unwrap_or: a source past 4G chars is granted the most a claim can say
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

/// Each view source's character grant at a turnover: the start the next epoch re-prefills,
/// leaving a growth reserve for the acts that follow to append into.
///
/// `available` is what the window's one allocation leaves her view (its sources' grants plus
/// the room no claim took), in the allocation's tokens. The higher-priority sources (the
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
/// The one allocator spends it: a higher-priority source asks for its whole grant as its
/// minimum, which `flex_allocate` serves first in priority order, and the lowest priority
/// shares what is left. The newest end is never cut: a source keeps its newest items whole
/// and collapses its oldest first, at the start only ([`View::turn_over`]).
pub(in super::super) fn start_grants(
    claims: &[Claim],
    plan: &crate::cognition::rag_budget::FlexAllocation,
    available: usize,
    appended_per_act: Option<usize>,
) -> Vec<usize> {
    let grants: Vec<Option<&crate::cognition::rag_budget::SourceAllocation>> = claims
        .iter()
        .map(|claim| plan.allocations.iter().find(|a| a.source_id == format!("{VIEW_CLAIM}{}", claim.source)))
        .collect();
    // Characters per allocation token, the ratio each claim was priced at (`view_claims`).
    let wanted_tokens: u64 = grants.iter().flatten().map(|a| u64::from(a.requested_max)).sum();
    let wanted_chars: u64 = claims.iter().zip(&grants).filter(|(_, a)| a.is_some()).map(|(c, _)| c.full_chars as u64).sum();
    let per_act_tokens = appended_per_act.map_or(0, |chars| (chars as u64 * wanted_tokens).div_ceil(wanted_chars.max(1)) as usize); // map_or: no act measured yet: the half bound alone
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
    let growing = if per_act_tokens < growable {
        (growable / 2).min(growable - per_act_tokens)
    } else {
        growable
    };
    // The newest end is never collapsed by the growth reserve (goal 7): halved, a 5,204-token
    // growable start converted to 1,885 chars, less than her one newest turn of 2,040, so her
    // history rendered empty after a scaled calibration (a_scaled_calibration_after_turnovers).
    // The start covers each growing source's newest item whole when the growable room can.
    let newest: usize = claims
        .iter()
        .zip(&grants)
        .filter(|(claim, _)| claim.priority == lowest)
        .filter_map(|(claim, grant)| {
            grant.map(|a| (claim.newest_chars as u64 * u64::from(a.requested_max)).div_ceil(claim.full_chars.max(1) as u64) as usize)
        })
        .sum();
    let growing = growing.max(newest.min(growable));
    let start = grounding + growing;
    let budgets: Vec<crate::cognition::rag_budget::RagSourceBudget> = claims
        .iter()
        .zip(&grants)
        .map(|(claim, grant)| {
            let (floor, max) = grant.map_or((0, 0), |a| (a.requested_floor, a.allocated_tokens)); // map_or: a source the window granted nothing starts collapsed
            crate::cognition::rag_budget::RagSourceBudget {
                source_id: claim.source.clone(),
                priority: claim.priority,
                floor_tokens: floor.min(max),
                min_tokens: if claim.priority > lowest { max } else { floor.min(max) },
                max_tokens: max,
                required: false,
            }
        })
        .collect();
    let sized = crate::cognition::rag_budget::flex_allocate(LlmDeliberationFaculty::clamp_u32(start), &budgets);
    let grants: Vec<usize> = claims
        .iter()
        .zip(&grants)
        .zip(&sized.allocations)
        .map(|((claim, grant), sized)| match grant {
            Some(a) if sized.allocated_tokens >= a.requested_max => claim.full_chars,
            Some(a) => (u64::from(sized.allocated_tokens) * claim.full_chars as u64 / u64::from(a.requested_max.max(1))) as usize,
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
    grants
}

#[cfg(test)]
mod tests;
