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
use crate::cognition::context_streams::actions::Actions;
use crate::cognition::context_streams::thoughts::Thoughts;
use crate::cognition::context_streams::ContextMessage;

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
    /// included): floor = the source collapsed, want = the source whole, at the source's
    /// priority. There is no separate history claim for the view, so the allocation that
    /// weighs her reply also decides, once, how much of her context she reads.
    pub(in super::super) fn view_claims(&self, streams: &Streams) -> Vec<crate::persona::rag_budget::RagSourceBudget> {
        let memory = self.working_memory.as_deref();
        let actions = memory.map(|memory| Actions { memory });
        let thoughts = memory.map(|memory| Thoughts { memory });
        Self::view_sources(streams, actions.as_ref(), thoughts.as_ref())
            .into_iter()
            .map(|source| {
                let items = source.items();
                let cost = |message: &ContextMessage| Self::message_cost(&message.to_chat());
                let floor: usize = items.iter().map(|item| item.pointer.as_ref().map_or(0, cost)).sum(); // map_or: an item that leaves when collapsed costs nothing
                let want: usize = items.iter().map(|item| cost(&item.full)).sum();
                crate::persona::rag_budget::RagSourceBudget {
                    source_id: format!("{VIEW_CLAIM}{}", source.name()),
                    priority: source.priority(),
                    floor_tokens: Self::clamp_u32(floor),
                    min_tokens: Self::clamp_u32(floor),
                    max_tokens: Self::clamp_u32(want),
                    required: false,
                }
            })
            .collect()
    }

    /// Keep her view within what the window's one allocation granted its sources, plus `room`
    /// (what no claim took beyond the reply's runaway headroom). While the epoch fits that it
    /// only appends. When it outgrows them a new epoch starts: a
    /// source granted all it wants whole, a source the allocation cut at half its grant, so
    /// her reads stay whole until her window is actually full and a cut source has room to
    /// grow by appends.
    ///
    /// Measured on the M5, 2026-10-21: sized to the 30 s latency target, Aiko's epoch started
    /// at 6,956 of 86,233 tokens and collapsed 40 items (18:20Z), the starvation the view
    /// exists to end. The replay's half-window start halved its turnovers (continuum-context
    /// `examples/replay.rs`).
    pub(in super::super) fn fit_view(
        &self,
        prompt: &mut PromptMessages,
        view: ViewPlacement,
        plan: &crate::persona::rag_budget::FlexAllocation,
        room: usize,
    ) {
        if view.len == 0 {
            return;
        }
        let view_tokens = Self::messages_cost(&prompt.history[..view.len]);
        let grant = |source: &str| plan.allocations.iter().find(|a| a.source_id == format!("{VIEW_CLAIM}{source}"));
        let granted: usize = plan
            .allocations
            .iter()
            .filter(|a| a.source_id.starts_with(VIEW_CLAIM))
            .map(|a| a.allocated_tokens as usize)
            .sum();
        // The epoch keeps appending into window no claim took: its rendering carries the
        // versions it superseded, which no claim counts, so measured against its grants alone
        // it read as over with 74k of the window unallocated (Atlas, the M5, 2026-10-22).
        if view_tokens <= granted + room {
            return;
        }
        // A source granted all it wants starts whole; one the allocation cut starts at half
        // its grant (its characters in proportion), the other half room to grow by appends.
        let allocate = |claims: &[Claim]| -> Vec<usize> {
            claims
                .iter()
                .map(|claim| match grant(&claim.source) {
                    Some(a) if a.allocated_tokens >= a.requested_max => claim.full_chars,
                    Some(a) => {
                        let chars_per_token = claim.full_chars as f64 / f64::from(a.requested_max.max(1));
                        (f64::from(a.allocated_tokens) * chars_per_token / 2.0) as usize
                    }
                    None => 0,
                })
                .collect()
        };
        let started = self.turn_over_view_with(&prompt.streams, allocate);
        prompt.grounding_at = started.len();
        prompt.history.splice(0..view.len, started);
        continuum_base::probe!(
            class = "delib.view.sized",
            persona = %self.persona_name,
            view_tokens,
            granted,
            restart = view.started,
            "her view outgrew what the window granted its sources and started a new epoch: a \
             source granted all it wants whole, a cut one at half its grant"
        );
    }

    /// Append this act's changes to her view and return its messages, and whether this act
    /// started the view (the first act after a start or a restart renders it whole, for the
    /// caller to size).
    fn advance_view(&self, streams: &Streams) -> (Vec<ChatMessage>, bool) {
        let memory = self.working_memory.as_deref();
        let actions = memory.map(|memory| Actions { memory });
        let thoughts = memory.map(|memory| Thoughts { memory });
        let sources = Self::view_sources(streams, actions.as_ref(), thoughts.as_ref());
        let mut held = self.context_view.lock();
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
                Ok((view, _)) => *held = Some(view),
                Err(mismatch) => continuum_base::probe!(
                    class = "delib.view.refused",
                    persona = %self.persona_name,
                    mismatch = %mismatch,
                    "her view could not start"
                ),
            }
        }
        (Self::render(held.as_ref()), started)
    }

    /// Start a new epoch of her view within `budget_chars`, the flexbox granting each
    /// source its share by priority.
    fn turn_over_view_with(&self, streams: &Streams, allocate: impl FnOnce(&[Claim]) -> Vec<usize>) -> Vec<ChatMessage> {
        let memory = self.working_memory.as_deref();
        let actions = memory.map(|memory| Actions { memory });
        let thoughts = memory.map(|memory| Thoughts { memory });
        let sources = Self::view_sources(streams, actions.as_ref(), thoughts.as_ref());
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
            Ok(start) => continuum_base::probe!(
                class = "delib.view.turnover",
                persona = %self.persona_name,
                epoch = start.epoch,
                start_chars = start.chars,
                collapsed_items = start.collapsed,
                "her view started a new epoch: the one re-prefill an epoch costs, collapsed \
                 by the flexbox at its start"
            ),
            Err(mismatch) => continuum_base::probe!(
                class = "delib.view.refused",
                persona = %self.persona_name,
                mismatch = %mismatch,
                "her view could not turn over"
            ),
        }
        Self::render(held.as_ref())
    }

    /// Every source whole, outside her view's state: what this turn's history would cost with
    /// no epoch and no collapse, the demand the serving plan sizes her window from.
    #[cfg(test)]
    pub(in super::super) fn whole_view(&self, streams: &Streams) -> Vec<ChatMessage> {
        let memory = self.working_memory.as_deref();
        let actions = memory.map(|memory| Actions { memory });
        let thoughts = memory.map(|memory| Thoughts { memory });
        let sources = Self::view_sources(streams, actions.as_ref(), thoughts.as_ref());
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

    /// Her sources in the order they first render: the conversation, her actions, each
    /// standing block, her thoughts (the order the prompt had before the view).
    fn view_sources<'a>(
        streams: &'a Streams,
        actions: Option<&'a Actions<'a>>,
        thoughts: Option<&'a Thoughts<'a>>,
    ) -> Vec<&'a dyn Source<ContextMessage>> {
        let mut sources: Vec<&dyn Source<ContextMessage>> = vec![&streams.conversation];
        sources.extend(actions.map(|source| source as &dyn Source<ContextMessage>));
        sources.extend(streams.blocks.iter().map(|source| source as &dyn Source<ContextMessage>));
        sources.extend(thoughts.map(|source| source as &dyn Source<ContextMessage>));
        sources
    }

    fn render(view: Option<&View<ContextMessage>>) -> Vec<ChatMessage> {
        view.map_or_else(Vec::new, |view| view.render().map(ContextMessage::to_chat).collect()) // map_or_else: no view yet renders nothing
    }

    fn clamp_u32(chars: usize) -> u32 {
        u32::try_from(chars).unwrap_or(u32::MAX) // unwrap_or: a source past 4G chars is granted the most a claim can say
    }
}
