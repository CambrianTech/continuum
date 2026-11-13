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

/// The saved output one slot's page continues in, and how she knows it. A page starts at
/// `first_line` (1-based), `first_byte` into the whole: line 1 for a first page, further on when she
/// reopens an output whose earlier page she already has (`continuation`).
struct Saved<'a> {
    label: &'a str,
    handle: &'a str,
    first_line: usize,
    first_byte: usize,
}

/// Where this mind's last page of a saved output ended, so reopening the SAME output (one
/// content-addressed handle) shows its next page instead of its first again. Eitan on the IntelMac,
/// 2026-11-11 to 11-13: every act reopened a 26,863-token output into a 19,022-token room and was
/// shown lines 1-K each time, for two days; a reopen is exempt from the repeat rule (it assumes the
/// whole is shown), so nothing stopped it. Owned by the mind's faculty, one entry (the output she
/// is reading), so it never grows; a different output, or the end of this one, starts at line 1.
#[derive(Default)]
pub(in crate::cognition::llm_deliberation_faculty) struct Reading(parking_lot::Mutex<Option<ReadPage>>);

/// The output she is reading, the act it was last paged for, where that page began, and the line
/// after its end.
#[derive(Clone)]
struct ReadPage {
    handle: String,
    act: u64,
    began: usize,
    next: usize,
}

impl Reading {
    /// The first line of act `act`'s page of `handle`. The same act renders the same page again (a
    /// retry or a grant change re-renders her act; it must not skip ahead); a LATER act that
    /// reopens the same output starts after the last page she was shown. Anything else: line 1.
    fn first_line(&self, handle: &str, act: u64) -> usize {
        match self.0.lock().as_ref() {
            Some(read) if read.handle == handle && read.act == act => read.began,
            Some(read) if read.handle == handle && read.act < act => read.next,
            _ => 1,
        }
    }

    /// Act `act` was shown `handle` from line `began` through line `last`.
    fn record(&self, handle: &str, act: u64, began: usize, last: usize) {
        *self.0.lock() = Some(ReadPage { handle: handle.to_owned(), act, began, next: last + 1 });
    }
}

/// The byte where 1-based `line` starts in `text`, or `None` past its end (read to the end: she
/// starts again at line 1).
fn byte_of(text: &str, line: usize) -> Option<usize> {
    let mut byte = 0;
    for (at, piece) in text.split_inclusive('\n').enumerate() {
        if at + 1 == line {
            return Some(byte);
        }
        byte += piece.len();
    }
    None
}

impl Saved<'_> {

    /// What she reads before the page: how much there is, what the page is, and the call that
    /// reads on, written as she makes it.
    fn note(&self, chars: usize, tokens: usize, cut: Cut) -> String {
        // Line numbers are the whole output's: `shown` and `total` count from this page's first line.
        let first = self.first_line;
        let (shown, next) = match cut {
            Cut::Lines { shown, total } => (
                format!("lines {first}-{} of {}", first + shown.saturating_sub(1), first - 1 + total),
                // To the output's own end: a range with no end reads through its last line.
                Reopen::Lines { output: self.handle, range: crate::cognition::tool_executor::spill::LineRange { start: first + shown, end: None } }.to_string(),
            ),
            Cut::Bytes { bytes } if first == 1 => (
                format!("its first {bytes} bytes (its first line alone is longer than the room)"),
                Reopen::Bytes { output: self.handle, offset: bytes as u64 }.to_string(),
            ),
            Cut::Bytes { bytes } => (
                format!("the first {bytes} bytes of line {first} (that line alone is longer than the room)"),
                Reopen::Bytes { output: self.handle, offset: (self.first_byte + bytes) as u64 }.to_string(),
            ),
        };
        let which = if first == 1 { "its first page".to_owned() } else { format!("its next page (you were shown lines 1-{} of it before)", first - 1) };
        format!(
            "[{}: {chars} chars, about {tokens} tokens, more than your window holds beside the rest, \
             so this is {which}: {shown}. Nothing is lost: read on with {next}]",
            self.label
        )
    }

    /// `text`'s page holding its first `upto` bytes: whole lines when at least one fits, else
    /// the first line cut at a character boundary.
    fn page_at(&self, text: &str, upto: usize, chars: usize, tokens: usize) -> String {
        let (end, cut) = Self::cut_at(text, upto);
        format!("{}\n{}", self.note(chars, tokens, cut), &text[..end])
    }

    /// Where a page holding `text`'s first `upto` bytes ends, and how it is cut: whole lines when
    /// at least one fits, else the first line cut at a character boundary. Decided once, here, so
    /// the page and the record of what she was shown never disagree.
    fn cut_at(text: &str, upto: usize) -> (usize, Cut) {
        let upto = text.floor_char_boundary(upto.min(text.len()));
        let first_line = text.find('\n').map_or(text.len(), |end| end + 1); // map_or: a one-line text is its first line
        if upto >= first_line {
            let end = text[..upto].rfind('\n').map_or(first_line, |at| at + 1); // map_or: no line end before upto keeps the first line
            return (end, Cut::Lines { shown: text[..end].lines().count(), total: text.lines().count() });
        }
        (upto, Cut::Bytes { bytes: upto })
    }

    /// The page of `text` (this page's part of the output, from its first line) within `target`
    /// tokens, its note sized by the WHOLE output (`chars`, `tokens`), and how it was cut.
    fn page(&self, text: &str, target: usize, chars: usize, tokens: usize) -> (String, Cut) {
        // The most content whose page fits, and never less than its least page (as much content as
        // its note, so a page is never a bare handle).
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
        let (_, cut) = Self::cut_at(text, fits);
        (self.page_at(text, fits, chars, tokens), cut)
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
            let tokens = estimate_guard_tokens(text);
            let chars = text.chars().count();
            // A LATER act reopening the same output continues where her last page of it ended; the
            // same act re-rendered shows the same page. Only the rendered pass reads or records it.
            let act = active.map(|(_, seq)| seq);
            let wanted = match (report, act) {
                (true, Some(act)) => self.reading.first_line(handle, act),
                _ => 1,
            };
            let (first_line, first_byte) = byte_of(text, wanted).map_or((1, 0), |byte| (wanted, byte)); // map_or: past the end she has read it all; she starts again at line 1
            let saved = Saved { label: &label, handle, first_line, first_byte };
            let (page, cut) = saved.page(&text[first_byte..], tokens.saturating_sub(over), chars, tokens);
            if let (true, Some(act), Cut::Lines { shown, .. }) = (report, act, cut) {
                self.reading.record(handle, act, first_line, first_line + shown.saturating_sub(1));
            }
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
        let saved = Saved { label: "result", handle: "f68053e3b2bad9c9", first_line: 1, first_byte: 0 };
        for split in [at + 1, at + 2] {
            let page = saved.page_at(&text, split, text.chars().count(), estimate_guard_tokens(&text));
            assert!(page.ends_with("ok tool/output(outputId=f68053e3b2bad9c9)\n"));
            assert!(page.contains("lines 1-1 of 2"));
        }
    }
    // what this catches: the same Sahar abort across every UTF-8 byte split and page-search budget.
    #[test]
    fn every_byte_cut_keeps_a_prefix_and_exact_continuation() {
        let saved = Saved { label: "result", handle: "f68053e3b2bad9c9", first_line: 1, first_byte: 0 };
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
                let (page, _) = saved.page(text, target, text.chars().count(), estimate_guard_tokens(text));
                let Some((_, content)) = page.split_once('\n') else { panic!("page must separate note and content") };
                assert!(text.starts_with(content));
            }
        }
    }

    // what this catches: Eitan on the IntelMac, 2026-11-11 to 11-13: every act reopened one output
    // too large for his room and was shown lines 1-K, for two days. A later act reopening the same
    // output gets its next page; the same act re-rendered keeps its page; another output, or the
    // end of this one, starts again at line 1.
    #[test]
    fn a_later_reopen_of_the_same_output_continues_and_a_rerender_does_not() {
        let reading = Reading::default();
        let text: String = (1..=30).map(|n| format!("line {n}\n")).collect();
        assert_eq!(reading.first_line("h", 7), 1, "nothing read yet");
        reading.record("h", 7, 1, 10);
        assert_eq!(reading.first_line("h", 7), 1, "the same act re-rendered keeps its page");
        assert_eq!(reading.first_line("h", 8), 11, "a later act continues after line 10");
        assert_eq!(reading.first_line("other", 8), 1, "another output starts at line 1");
        let byte = byte_of(&text, 11).expect("line 11 exists");
        let saved = Saved { label: "result", handle: "h", first_line: 11, first_byte: byte };
        let whole_tokens = estimate_guard_tokens(&text);
        let (page, cut) = saved.page(&text[byte..], whole_tokens / 3, text.chars().count(), whole_tokens);
        let Some((note, content)) = page.split_once('\n') else { panic!("a note, then the page") };
        assert!(content.starts_with("line 11\n"), "the page starts at line 11: {content}");
        assert!(note.contains("its next page (you were shown lines 1-10 of it before)") && note.contains("of 30"), "{note}");
        let Cut::Lines { shown, .. } = cut else { panic!("whole lines fit") };
        assert!(note.contains(&format!("lines 11-{}", 10 + shown)), "{note}");
        assert_eq!(byte_of(&text, 31), None, "past the end: she has read it all");
    }
}
