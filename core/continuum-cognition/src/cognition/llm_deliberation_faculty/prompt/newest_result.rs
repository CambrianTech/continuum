//! Her NEWEST result as the act's one allocation shows it: whole, or its FIRST PAGE (plan goal 7:
//! "Keep the newest interaction whole and the full result reachable").
//!
//! The decision is the allocation's (`continuum_context::allocation::newest`): her newest result is
//! irreducible at its whole, chosen before her tools, grounding, view and older history collapse,
//! so it is whole whenever the window holds it beside every other hard limit. This file is its
//! producer and renderer only: it measures what the result costs whole and at its least page, and
//! renders the page the allocation granted.
//!
//! A page is the result's own content from its start, cut to the room: at a line end when a line
//! fits, else inside the first line. It opens with a note naming how much there is and the call
//! that reads on from where the page ends, against the saved output that result IS (her call's own
//! saved whole, or for a legacy receipt her act's). Before it was a ladder of summaries with a
//! handle: on the M5 (2026-11-07) her newest result came in that way 159 times in a day, about 105
//! follow-on acts re-read it, and 52 of those reads were themselves too large and came back as
//! another summary and handle (card 8a12dcd1). A page always carries content, and reading on
//! always makes progress.
//!
//! Every result has a page: a slot with no saved whole (a reported note, a result restored with
//! no active action) is saved when it must page, so a newest result larger than her window never
//! refuses her act. Only a store that refuses the write leaves a slot whole (probed).

use super::*;
use crate::ai::types::{ContentPart, MessageContent};
use crate::cognition::tool_executor::tool_output::Reopen;
use continuum_context::allocation::Newest;

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

/// What one slot's page shows of it, and so where reading on starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cut {
    /// Its first `shown` of `total` lines.
    Lines { shown: usize, total: usize },
    /// Its first `bytes` bytes, inside its first line.
    Bytes { bytes: usize },
}

/// The saved output one slot's page continues in, and how she knows it.
struct Saved<'a> {
    label: &'a str,
    handle: &'a str,
}

impl Saved<'_> {
    /// What she reads before the page: how much there is, what the page is, and the call that
    /// reads on, written as she makes it.
    fn note(&self, chars: usize, tokens: usize, cut: Cut) -> String {
        let (shown, next) = match cut {
            Cut::Lines { shown, total } => (
                format!("lines 1-{shown} of {total}"),
                // To the output's own end: a range with no end reads through its last line.
                Reopen::Lines { output: self.handle, range: crate::cognition::tool_executor::spill::LineRange { start: shown + 1, end: None } }.to_string(),
            ),
            Cut::Bytes { bytes } => (
                format!("its first {bytes} bytes (its first line alone is longer than the room)"),
                Reopen::Bytes { output: self.handle, offset: bytes as u64 }.to_string(),
            ),
        };
        format!(
            "[{}: {chars} chars, about {tokens} tokens, more than your window holds beside the rest, \
             so this is its first page: {shown}. Nothing is lost: read on with {next}]",
            self.label
        )
    }

    /// `text`'s page holding its first `upto` bytes: whole lines when at least one fits, else
    /// the first line cut at a character boundary.
    fn page_at(&self, text: &str, upto: usize, chars: usize, tokens: usize) -> String {
        let upto = text.floor_char_boundary(upto.min(text.len()));
        let first_line = text.find('\n').map_or(text.len(), |end| end + 1); // map_or: a one-line text is its first line
        if upto >= first_line {
            let end = text[..upto].rfind('\n').map_or(first_line, |at| at + 1); // map_or: no line end before upto keeps the first line
            let total = text.lines().count();
            let shown = text[..end].lines().count();
            return format!("{}\n{}", self.note(chars, tokens, Cut::Lines { shown, total }), &text[..end]);
        }
        format!("{}\n{}", self.note(chars, tokens, Cut::Bytes { bytes: upto }), &text[..upto])
    }

    /// `text`'s first page within `target` tokens: the most content whose page fits, and never
    /// less than its least page (as much content as its note, so a page is never a bare handle).
    fn first_page(&self, text: &str, target: usize) -> String {
        let chars = text.chars().count();
        let tokens = estimate_guard_tokens(text);
        // As much content as its note at its longest (either form, with this text's own counts).
        let total = text.lines().count();
        let note = self.note(chars, tokens, Cut::Lines { shown: total, total }).len().max(self.note(chars, tokens, Cut::Bytes { bytes: text.len() }).len());
        let least = note.min(text.len());
        let (mut fits, mut over) = (least, text.len());
        while over - fits > 1 {
            let mid = fits + (over - fits) / 2;
            if estimate_guard_tokens(&self.page_at(text, mid, chars, tokens)) <= target {
                fits = mid;
            } else {
                over = mid;
            }
        }
        self.page_at(text, fits, chars, tokens)
    }
}

impl LlmDeliberationFaculty {
    /// Her newest result for the act's one allocation (`continuum_context::allocation::Newest`), in
    /// this prompt's estimate: what it costs whole and, when it is saved, at its least page.
    pub(in super::super) fn newest_result_claim(&self, prompt: &PromptMessages) -> Newest {
        if prompt.latest_result.is_empty() {
            return Newest::Absent;
        }
        let whole = prompt.pricing.estimate_all(&prompt.latest_result);
        let mut least = prompt.latest_result.clone();
        if !self.page_newest(prompt.pricing, &mut least, 0, false) {
            return Newest::Whole(whole as u32);
        }
        Newest::Pageable { whole: whole as u32, least: prompt.pricing.estimate_all(&least) as u32 }
    }

    /// Show her newest result as the allocation granted it: its first page within `room` (this
    /// prompt's estimate), carried as her tail's result item's summary and judged at `Level::Page`
    /// (`judge_tail`), so the page is what is sent and what joins her view. Answers whether it was
    /// shown short of whole (`DeliberationPromptView::newest_result_reduced`).
    pub(in super::super) fn fit_newest_result(&self, prompt: &mut PromptMessages, room: usize) -> bool {
        // Only a result her view sends whole is paged: a repeat block is its whole, and a result she
        // already holds is not sent.
        let tail = &prompt.streams.tail;
        let whole = tail.judged.iter().any(|item| Some(item.element()) == tail.result() && item.level() == continuum_context::level::Level::Full);
        if !whole {
            return false;
        }
        // Cut from her result as her tail renders it whole; its attachments ride beside the page.
        let mut group = tail.result_group();
        let attached = prompt.pricing.estimate_all(&prompt.latest_result).saturating_sub(prompt.pricing.estimate_all(&group));
        if !self.page_newest(prompt.pricing, &mut group, room.saturating_sub(attached), true) {
            return false;
        }
        let Some(Ok(page)) = group.last().map(crate::cognition::context_streams::ContextMessage::from_chat) else {
            return false; // her result renders as text or protocol, so its page does too
        };
        prompt.streams.tail.page_result(page);
        self.judge_tail(prompt, true);
        true
    }

    /// `group` (her newest result's messages) cut to `room`: its largest slot first, each to its
    /// first page, until it fits or every saved slot is at its least page. A slot with no saved
    /// whole stays whole. Each rewrite is in place: the native call/result pairing and its ids are
    /// untouched. Whether any slot was paged.
    fn page_newest(&self, pricing: PromptCalibration, group: &mut [ChatMessage], room: usize, report: bool) -> bool {
        let whole_tokens = pricing.estimate_all(group);
        if whole_tokens <= room {
            return false;
        }
        // Every slot of her newest result pages: through its own saved whole when it has one, else
        // saved now (content-addressed, so a save of what is already saved is the same handle).
        // A result with no page was a refusal on every act while it stayed newest (the IntelMac,
        // 2026-11-11: Iris and Eitan, 38 and 37 refusals, a 29,618-token result restored with no
        // active action, so this returned before any probe). Joel: "These refusals are supposed
        // to be impossible."
        let active = self.working_memory.as_ref().and_then(|memory| memory.active_action_full().map(|(seq, _)| (memory, seq)));
        let observations = active.map(|(memory, seq)| memory.active_action_observations(seq)).unwrap_or_default(); // unwrap_or_default: no active action has no observations; its slots are saved below
        let act_handle = active.and_then(|(memory, seq)| memory.saved_handle(seq).ok());
        let label = active.map_or_else(|| "result".to_owned(), |(_, seq)| format!("result #{seq}")); // map_or_else: a result with no active action is labelled by what it is
        let handle_of = |id: &Option<String>| -> Option<String> {
            match id {
                Some(id) => observations
                    .iter()
                    .find(|o| &o.output.result.tool_use_id == id)
                    .and_then(|o| o.output.result.custody.handle().map(str::to_owned)),
                None => act_handle.clone(),
            }
        };
        let pin = crate::cognition::tool_executor::spill::LivePin::new();
        let mut saved_now = 0usize;
        let mut slots: Vec<(Option<String>, &mut String)> = slots(group)
            .into_iter()
            .map(|(id, text)| {
                let handle = handle_of(&id).or_else(|| {
                    let handle = pin.retain(self.persona_id, text).handle().map(str::to_owned);
                    saved_now += usize::from(handle.is_some());
                    handle
                });
                (handle, text)
            })
            .collect();
        if report && saved_now > 0 {
            continuum_base::probe!(
                class = "delib.newest_result.saved_to_page",
                persona = %self.persona_name,
                saved_now,
                active_action = active.is_some(),
                latest_result_tokens = whole_tokens,
                room,
                "her newest result had no saved whole: saved now so it pages instead of refusing her act"
            );
        }
        slots.sort_by_key(|(_, text)| std::cmp::Reverse(text.len()));
        let mut cost = whole_tokens;
        for (handle, text) in slots.iter_mut() {
            let over = cost.saturating_sub(room);
            if over == 0 {
                break;
            }
            let Some(handle) = handle.as_deref() else { continue };
            let saved = Saved { label: &label, handle };
            let tokens = estimate_guard_tokens(text);
            let page = saved.first_page(text, tokens.saturating_sub(over));
            cost = cost.saturating_sub(tokens).saturating_add(estimate_guard_tokens(&page));
            **text = page;
        }
        if report {
            continuum_base::probe!(
                class = "delib.newest_result.paged",
                persona = %self.persona_name,
                seq = active.map_or(0, |(_, seq)| seq), // map_or: a result with no active action has no act number
                whole_tokens,
                shown_tokens = pricing.estimate_all(group),
                room,
                "her newest result is larger than her window holds beside every other hard limit: shown as its first page, reading on named"
            );
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the 09:37:24 Windows Sahar abort when a multiline page cut split U+FFFD.
    #[test]
    fn multiline_page_cut_inside_replacement_character_preserves_first_line() {
        let text = format!("ok tool/output(outputId=f68053e3b2bad9c9)\n{}\u{fffd}tail", "x".repeat(1456));
        let Some(at) = text.find('\u{fffd}') else { panic!("fixture must contain the replacement character") };
        let saved = Saved { label: "result", handle: "f68053e3b2bad9c9" };
        for split in [at + 1, at + 2] {
            let page = saved.page_at(&text, split, text.chars().count(), estimate_guard_tokens(&text));
            assert!(page.ends_with("ok tool/output(outputId=f68053e3b2bad9c9)\n"));
            assert!(page.contains("lines 1-1 of 2"));
        }
    }
    // what this catches: the same Sahar abort across every UTF-8 byte split and page-search budget.
    #[test]
    fn every_byte_cut_keeps_a_prefix_and_exact_continuation() {
        let saved = Saved { label: "result", handle: "f68053e3b2bad9c9" };
        for text in ["", "ASCII", "é中😀tail", "header\né中😀tail", "é\n中\n😀tail"] {
            for upto in 0..=text.len() + 2 {
                let page = saved.page_at(text, upto, text.chars().count(), estimate_guard_tokens(text));
                let Some((_, content)) = page.split_once('\n') else { panic!("page must separate note and content") };
                assert!(text.starts_with(content));
                let boundary = text.floor_char_boundary(upto.min(text.len()));
                let first_line = text.find('\n').map_or(text.len(), |at| at + 1);
                if boundary < first_line {
                    assert_eq!(content, &text[..boundary]);
                    assert!(page.contains(&format!("its first {boundary} bytes")));
                } else if !text.is_empty() {
                    let end = text[..boundary].rfind('\n').map_or(first_line, |at| at + 1);
                    assert_eq!(content, &text[..end]);
                }
            }
            for target in 0..=estimate_guard_tokens(text) + 5 {
                let page = saved.first_page(text, target);
                let Some((_, content)) = page.split_once('\n') else { panic!("page must separate note and content") };
                assert!(text.starts_with(content));
            }
        }
    }
}
