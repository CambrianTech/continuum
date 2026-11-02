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
//! The decision uses the same arithmetic as `delib.payload.over_window` (#5481): what the window
//! leaves after its reserves (`after_reserves`) less every OTHER irreducible part
//! (`RequiredParts`). A result within that is untouched, exactly as before. One over it takes
//! the next level of detail (`continuum_context::levels`, through `operation_level`, the one
//! owner of a result's summary), then, if even that cannot fit, its pointer, each with a line
//! saying it was too large to show whole and how to read the rest. Only a SAVED result is ever
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
    Pointer,
}

impl Level {
    fn name(self) -> &'static str {
        match self {
            Self::Whole => "whole",
            Self::Summary => "summary",
            Self::Pointer => "pointer",
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
    /// Fit her newest result to what the window leaves it, `after_reserves` being the window
    /// after framing, tools and the reply's floor (the same number the over-window probe reads).
    /// A result that fits, or one with no saved whole to reopen, is left exactly as it is.
    /// Answers whether it was shown smaller than whole (`DeliberationPromptView::newest_result_reduced`).
    pub(in super::super) fn fit_newest_result(&self, prompt: &mut PromptMessages, after_reserves: usize) -> bool {
        let parts = prompt.required_parts();
        let room = after_reserves.saturating_sub(parts.total() - parts.latest_result);
        if parts.latest_result <= room {
            return false;
        }
        let Some(memory) = self.working_memory.as_ref() else { return false };
        let Some((seq, _)) = memory.active_action_full() else { return false };
        let Ok(handle) = memory.saved_handle(seq) else {
            continuum_base::probe!(
                class = "delib.newest_result.unsaved_over_window",
                persona = %self.persona_name,
                seq,
                latest_result_tokens = parts.latest_result,
                room,
                "her newest result is over her window and was never saved: it stays whole, the turn faults"
            );
            return false;
        };
        let label = format!("result #{seq}");
        let observations = memory.active_action_observations(seq);
        let act_of = |id: &Option<String>| -> Option<&Observation> {
            id.as_ref().and_then(|id| observations.iter().find(|o| &o.output.result.tool_use_id == id))
        };
        // Level by level, largest slot first, until the group fits: whole -> summary -> pointer.
        // Each rewrite is in place: the native call/result pairing and its ids are untouched.
        let mut cost = Self::messages_cost(&prompt.latest_result);
        let mut group = slots(&mut prompt.latest_result);
        group.sort_by_key(|(_, text)| std::cmp::Reverse(text.chars().count()));
        let wholes: Vec<(usize, usize)> = group.iter().map(|(_, text)| (text.chars().count(), estimate_guard_tokens(text))).collect();
        let mut shown_as = Level::Whole;
        'levels: for level in [Level::Summary, Level::Pointer] {
            for ((id, text), &(chars, tokens)) in group.iter_mut().zip(&wholes) {
                if cost <= room {
                    break 'levels;
                }
                let note = too_large_note(&label, &handle, chars, tokens, room, level.name());
                let body = match level {
                    Level::Summary => match crate::cognition::working_memory::operation_level(text, act_of(id)) {
                        Some(summary) => format!("{note}\n{summary}"),
                        None => continue,
                    },
                    Level::Pointer | Level::Whole => note,
                };
                cost = cost.saturating_sub(estimate_guard_tokens(text)).saturating_add(estimate_guard_tokens(&body));
                **text = body;
                shown_as = level;
            }
        }
        continuum_base::probe!(
            class = "delib.newest_result.shown_smaller",
            persona = %self.persona_name,
            seq,
            handle = %handle,
            level = shown_as.name(),
            whole_tokens = parts.latest_result,
            shown_tokens = Self::messages_cost(&prompt.latest_result),
            room,
            "her newest result alone was over her window: shown at a smaller level with its handle"
        );
        shown_as != Level::Whole
    }
}
