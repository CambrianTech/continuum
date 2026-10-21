//! Her context view across acts ([`continuum_context::view::View`]): her conversation, her
//! actions, her standing blocks and her thoughts, as one epoch that only appends.
//!
//! Every act appends the items that changed, so the engine reuses the previous prompt whole
//! and prefills only what is new (replayed on the M5's captures, 2026-10-21: mean prefill
//! per act 70,296 -> 27,815 chars for Aiko, 51,505 -> 15,998 for Aris; continuum-context
//! `examples/replay.rs`). When the epoch outgrows what the window grants her history, it
//! turns over: the one flexbox grants each source a budget between its pointers and its
//! whole, the view keeps each source's newest items whole and collapses the older ones,
//! and the new epoch starts no larger than this node prefills in its latency target. That
//! is the only place anything collapses; nothing is cut in the middle of an epoch.

use continuum_context::source::Source;
use continuum_context::view::{Claim, View};

use super::*;
use crate::cognition::context_streams::actions::Actions;
use crate::cognition::context_streams::thoughts::Thoughts;
use crate::cognition::context_streams::ContextMessage;

impl LlmDeliberationFaculty {
    /// Append this act's changes to her view (the first act starts it whole) and return its
    /// messages.
    pub(in super::super) fn advance_view(&self, streams: &Streams) -> Vec<ChatMessage> {
        let memory = self.working_memory.as_deref();
        let actions = memory.map(|memory| Actions { memory });
        let thoughts = memory.map(|memory| Thoughts { memory });
        let sources = Self::view_sources(streams, actions.as_ref(), thoughts.as_ref());
        let mut held = self.context_view.lock();
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
        Self::render(held.as_ref())
    }

    /// Start a new epoch of her view within `budget_chars`, the flexbox granting each
    /// source its share by priority.
    pub(in super::super) fn turn_over_view(&self, streams: &Streams, budget_chars: usize) -> Vec<ChatMessage> {
        let memory = self.working_memory.as_deref();
        let actions = memory.map(|memory| Actions { memory });
        let thoughts = memory.map(|memory| Thoughts { memory });
        let sources = Self::view_sources(streams, actions.as_ref(), thoughts.as_ref());
        let allocate = |claims: &[Claim]| -> Vec<usize> {
            let budgets: Vec<crate::persona::rag_budget::RagSourceBudget> = claims
                .iter()
                .map(|claim| crate::persona::rag_budget::RagSourceBudget {
                    source_id: claim.source.clone(),
                    priority: claim.priority,
                    floor_tokens: Self::clamp_u32(claim.pointer_chars),
                    min_tokens: Self::clamp_u32(claim.pointer_chars),
                    max_tokens: Self::clamp_u32(claim.full_chars),
                    required: false,
                })
                .collect();
            let plan = crate::persona::rag_budget::flex_allocate(Self::clamp_u32(budget_chars), &budgets);
            claims.iter().map(|claim| plan.granted(&claim.source) as usize).collect()
        };
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
                budget_chars,
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
