//! Her NEWEST result, when it alone cannot fit her window, is shown at its summary level with
//! the handle that reopens it whole, never dropped and never cut (plan goal 7: "keep the full
//! result reachable; shrink to a handle she can expand").
//!
//! The newest result is required input, so until now it was carried whole whatever its size.
//! Aris (the M5, 2026-10-26 22:22Z) received one `work/submission` answer of 340,832 chars, about
//! 183k tokens against her 187,136-token slot: every request overflowed (5 refused at 192k-234k
//! tokens) and she completed no act after it, because the newest result never ages out without
//! a next act.
//!
//! The decision is the act's one allocation's (`continuum_context::allocation`): her newest result
//! is a required claim whose floor is its coarsest level and whose min is its whole, so it is
//! whole whenever the window holds it beside every other floor. A result within its grant is
//! untouched. One over it takes
//! the next level of detail (`continuum_context::levels`, through `operation_level`, the one
//! owner of a result's summary), then, if even that cannot fit, each coarser level of that
//! summary, never a bare pointer, with a line saying it was too large to show whole and how to
//! read the rest (ONE-ACT stage 2: whole, or its summary plus handle). Only a SAVED result is ever
//! shown as less than whole: one whose save failed has no other copy, so it stays whole and the
//! turn's capacity fault names it.

use super::*;
use crate::ai::types::{ContentPart, MessageContent};
use crate::cognition::act_observe::Observation;

/// One text slot of the newest result group: a native tool result's content or a legacy
/// receipt's text.
fn slots(group: &mut [ChatMessage]) -> Vec<(Option<String>, &mut String)> {
    let mut out = Vec::new();
    for message in group {
        match &mut message.content {
            MessageContent::Text(text) => out.push((None, text)),
            MessageContent::Parts(parts) => {
                for part in parts {
                    if let ContentPart::ToolResult { tool_use_id, content, .. } = part {
                        out.push((Some(tool_use_id.clone()), content));
                    }
                }
            }
        }
    }
    out
}

/// The level of detail a result is shown at (`continuum_context::levels`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Whole,
    Summary,
}

impl Level {
    fn name(self) -> &'static str {
        match self {
            Self::Whole => "whole",
            Self::Summary => "summary",
        }
    }
}

/// What she reads in place of a result too large for her window: why, and how to read the rest.
fn too_large_note(label: &str, handle: &str, chars: usize, tokens: usize, room: usize, level: &str) -> String {
    format!(
        "[{label}: too large to show whole ({chars} chars, about {tokens} tokens; your window leaves \
         {room} for it), so this is its {level}. Nothing is lost: read the whole by line range or \
         grep with tool/output {handle}, or ask the tool a narrower question (a page or a filter)]"
    )
}

impl LlmDeliberationFaculty {
    /// Her newest result's FLOOR in the act's one allocation: what it costs at its coarsest level
    /// (its summary chain to the end, with its note and handle). An unsaved result has no smaller
    /// level, so its floor is its whole. Its claim's min is its whole, so it is shown whole whenever
    /// the window holds it (`continuum_context::allocation`).
    pub(in super::super) fn newest_result_floor(&self, prompt: &PromptMessages) -> usize {
        let mut group = prompt.latest_result.clone();
        self.level_newest(&mut group, 0, false);
        Self::messages_cost(&group)
    }

    /// Show her newest result within `room`, the grant the act's one allocation gave it: whole when
    /// it fits, else down its levels. Answers whether it was shown smaller than whole
    /// (`DeliberationPromptView::newest_result_reduced`).
    pub(in super::super) fn fit_newest_result(&self, prompt: &mut PromptMessages, room: usize) -> bool {
        self.level_newest(&mut prompt.latest_result, room, true)
    }

    /// `group` (her newest result's messages) down its levels until it costs at most `room`.
    /// Largest slot first, each down its mipmap chain: whole, its summary (`operation_level`),
    /// then each coarser level of that summary (`levels::summarize`, each the next rung of the
    /// ladder from the whole to its floor). Never a bare pointer (goal 7): her newest result is the step that ends the
    /// recursion, so what she sees of it always carries content. A slot whose chain ends still
    /// over the room keeps its coarsest level and the turn is refused for capacity, typed, never
    /// sent emptied. Each rewrite is in place: the native call/result pairing and its ids are
    /// untouched. Only a SAVED result is shown as less than whole. `report` probes what it did.
    fn level_newest(&self, group: &mut Vec<ChatMessage>, room: usize, report: bool) -> bool {
        let whole_tokens = Self::messages_cost(group);
        if whole_tokens <= room {
            return false;
        }
        let Some(memory) = self.working_memory.as_ref() else { return false };
        let Some((seq, _)) = memory.active_action_full() else { return false };
        let Ok(handle) = memory.saved_handle(seq) else {
            if report {
                continuum_base::probe!(
                    class = "delib.newest_result.unsaved_over_window",
                    persona = %self.persona_name,
                    seq,
                    latest_result_tokens = whole_tokens,
                    room,
                    "her newest result is over her window and was never saved: it stays whole, the turn faults"
                );
            }
            return false;
        };
        let label = format!("result #{seq}");
        let observations = memory.active_action_observations(seq);
        let act_of = |id: &Option<String>| -> Option<&Observation> {
            id.as_ref().and_then(|id| observations.iter().find(|o| &o.output.result.tool_use_id == id))
        };
        let mut cost = whole_tokens;
        let mut slots = slots(group);
        slots.sort_by_key(|(_, text)| std::cmp::Reverse(text.chars().count()));
        let wholes: Vec<(usize, usize)> = slots.iter().map(|(_, text)| (text.chars().count(), estimate_guard_tokens(text))).collect();
        let mut shown_as = Level::Whole;
        let mut depth = 0u32;
        for ((id, text), &(chars, tokens)) in slots.iter_mut().zip(&wholes) {
            let mut level = crate::cognition::working_memory::operation_level(text, act_of(id));
            while cost > room {
                let Some(summary) = level.take() else { break };
                let body = format!("{}\n{summary}", too_large_note(&label, &handle, chars, tokens, room, "summary"));
                cost = cost.saturating_sub(estimate_guard_tokens(text)).saturating_add(estimate_guard_tokens(&body));
                level = continuum_context::levels::summarize(&summary, continuum_context::levels::Shape::Output);
                **text = body;
                shown_as = Level::Summary;
                depth += 1;
            }
            if cost <= room {
                break;
            }
        }
        if report {
            continuum_base::probe!(
                class = "delib.newest_result.shown_smaller",
                persona = %self.persona_name,
                seq,
                handle = %handle,
                level = shown_as.name(),
                // How many mipmap levels below whole she was shown it at.
                depth,
                whole_tokens,
                shown_tokens = Self::messages_cost(group),
                room,
                "her newest result's grant was smaller than its whole: shown at a smaller level with its handle"
            );
        }
        shown_as != Level::Whole
    }
}

