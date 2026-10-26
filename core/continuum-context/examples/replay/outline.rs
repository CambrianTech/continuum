//! Outline results (`--outline-results`): an oversized result enters her view at its outline
//! (`continuum_context::levels`), with a pointer line naming the output handle that holds it
//! whole and how to page it, instead of filling the view in a few acts (plan line S1).
//!
//! The rule, with no constant of its own: an act's results enter whole while together they fit
//! the view's measured per-act headroom, the median characters one act of this epoch appended
//! (`View::appended_per_act`, the growth reserve `start_grants` already keeps for one act).
//! Past it, the largest results enter at their outline first, until the rest fits. A result
//! with no outline smaller than a quarter of it (`levels::LEVEL_RATIO`), or with no output handle
//! to page it from, enters whole: a level she cannot expand is a defect, not a level.
//!
//! Two modes, because the plan forbids clipping her newest end:
//! - [`Mode::NextAct`]: her newest result is read in full on the act it arrives (it is in the
//!   tail), and is held at its outline from the next act on.
//! - [`Mode::Strict`]: the outline only, even on its own act: the trade the rule refuses, measured.
//!
//! The paging proxy: an outlined result would have been paged back when a later act of hers, in
//! the epoch the outline stands in, quotes a line the outline elided (a line of at least
//! [`QUOTE_CHARS`] characters, gutter stripped and trimmed: shorter lines such as `}` or
//! `let x = 1;` repeat everywhere and would match by chance). A proxy: a quote may come from a
//! re-read she made anyway, and a page she needed without quoting is not counted.

use std::collections::HashSet;

use continuum_context::levels::{summarize, Shape};
use serde_json::Value;

use super::validate::gutter;

/// The paging proxy's matching rule: the shortest line it counts as a quote. A chosen floor, not
/// a measured separator: on Benchy's 67 distinct tool results (M5 captures, 2026-10-26) about 22%
/// of lines recur across results at every length from 10 to 60 characters, because she re-reads
/// the same files, so length does not tell a chance match from a real one. The floor only keeps
/// one-token lines (`}`) out; the proxy is a lower bound either way (a page she needed without
/// quoting is not counted), and its counts (0-2 per resident) are reported as such.
pub const QUOTE_CHARS: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Every result whole (append-tail alone).
    Whole,
    /// Whole on its own act, outline from the next act on.
    NextAct,
    /// Outline past headroom even on its own act.
    Strict,
}

/// One act's results with the oversized ones at their outline.
pub struct Outlined {
    pub piece: String,
    /// The distinctive lines the outlines left out: what a later quote would page back.
    pub elided: HashSet<String>,
    pub results: usize,
}

/// The output handle an act's result header names (`output handle: tool/output <id>`).
fn handle_of(text: &str) -> Option<String> {
    let (_, rest) = text.split_once("output handle: tool/output ")?;
    let id: String = rest.chars().take_while(char::is_ascii_alphanumeric).collect();
    (!id.is_empty()).then_some(id)
}

pub fn quotable(line: &str) -> Option<String> {
    let code = gutter(line).map_or(line, |(_, code)| code).trim(); // map_or: a line without the gutter is its own code
    (code.chars().count() >= QUOTE_CHARS).then(|| code.to_owned())
}

/// `piece` (one tail message, as sent) with its results past `headroom` at their outline; `None`
/// when it carries no result, they fit, or none has an outline and a handle.
pub fn outline(piece: &str, headroom: usize) -> Option<Outlined> {
    let mut parts: Value = serde_json::from_str(piece).ok()?;
    let array = parts.as_array_mut()?;
    let handle = array.iter().filter_map(|part| part["text"].as_str()).find_map(handle_of)?;
    let mut sizes: Vec<(usize, usize)> = array
        .iter()
        .enumerate()
        .filter(|(_, part)| part["type"] == "tool_result")
        .filter_map(|(index, part)| Some((index, part["content"].as_str()?.chars().count())))
        .collect();
    let mut total: usize = sizes.iter().map(|(_, chars)| chars).sum();
    if total <= headroom {
        return None;
    }
    sizes.sort_by(|a, b| b.1.cmp(&a.1));
    let (mut elided, mut results) = (HashSet::new(), 0usize);
    for (index, chars) in sizes {
        if total <= headroom {
            break;
        }
        let Some(whole) = array[index]["content"].as_str().map(str::to_owned) else { continue };
        let Some(summary) = summarize(&whole, Shape::Code(&gutter)).or_else(|| summarize(&whole, Shape::Output)) else { continue };
        let path = whole
            .lines()
            .next()
            .and_then(|header| serde_json::from_str::<Value>(header).ok())
            .and_then(|header| header["file_path"].as_str().map(str::to_owned));
        let page = match path {
            Some(path) => format!("tool/output {handle} with a range, or code/read {path} with a line range"),
            None => format!("tool/output {handle} with a range"),
        };
        let shown = format!("[outline of a {chars}-char result; the whole is at output handle tool/output {handle}; page it with {page}]\n{summary}");
        let kept: HashSet<&str> = summary.lines().collect();
        elided.extend(whole.lines().filter(|line| !kept.contains(line)).filter_map(quotable));
        total = total - chars + shown.chars().count();
        array[index]["content"] = Value::String(shown);
        results += 1;
    }
    (results > 0).then(|| Outlined { piece: parts.to_string(), elided, results })
}

fn leaves(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(text) => out.push(text.clone()),
        Value::Array(values) => values.iter().for_each(|value| leaves(value, out)),
        Value::Object(map) => map.values().for_each(|value| leaves(value, out)),
        _ => {}
    }
}

/// The quotable lines of the act a request carries: its last assistant turn, text and tool input.
pub fn said(request: &Value) -> Vec<String> {
    let Some(last) = request["messages"].as_array().and_then(|messages| messages.iter().rfind(|m| m["role"] == "assistant")) else {
        return Vec::new();
    };
    let mut strings = Vec::new();
    leaves(&last["content"], &mut strings);
    strings.iter().flat_map(|text| text.lines()).filter_map(quotable).collect()
}

/// Outlines standing in her view, and how many a later act of hers quoted past.
#[derive(Default)]
pub struct Paging {
    /// The first request index whose act could need the page, and the lines it would.
    pending: Vec<(usize, HashSet<String>)>,
    pub outlined: usize,
    pub paged: usize,
    /// The previous act's lines: a request repeating its last assistant turn is no new act.
    last: Vec<String>,
}

impl Paging {
    pub fn outlined(&mut self, from: usize, outlined: &Outlined) {
        self.outlined += outlined.results;
        if !outlined.elided.is_empty() {
            self.pending.push((from, outlined.elided.clone()));
        }
    }

    /// The act request `at` carries said `lines`.
    pub fn said(&mut self, at: usize, lines: &[String]) {
        if lines == self.last.as_slice() {
            return;
        }
        let mut paged = 0;
        self.pending.retain(|(from, elided)| {
            let quoted = at >= *from && lines.iter().any(|line| elided.contains(line));
            paged += usize::from(quoted);
            !quoted
        });
        self.paged += paged;
        self.last = lines.to_vec();
    }

    /// A turnover renders her results from the action log again: no outline of this mode stands.
    pub fn turned_over(&mut self) {
        self.pending.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn act(content: &str) -> String {
        serde_json::json!([
            {"type": "tool_result", "tool_use_id": "a", "content": content},
            {"type": "text", "text": "[result #7; operation code/read; output handle: tool/output d6977925]"}
        ])
        .to_string()
    }

    fn read(lines: usize) -> String {
        let body: String = (1..=lines).map(|n| format!("{n:>6} |         let value_number_{n} = compute_the_thing_for(index, {n});\n")).collect();
        format!("{{\"file_path\":\"a.rs\"}}\n{:>6} | pub fn f() {{\n{body}", 0)
    }

    // what this catches: an oversized result kept whole, or outlined without the handle that
    // pages it back (a level she cannot expand), or a result that fits the headroom outlined.
    #[test]
    fn a_result_past_headroom_enters_at_its_outline_with_its_handle() {
        let piece = act(&read(200));
        assert!(outline(&piece, piece.len()).is_none(), "a result that fits enters whole");
        let Some(outlined) = outline(&piece, 1_000) else { panic!("an oversized read has an outline") };
        assert!(outlined.piece.len() * 3 < piece.len());
        assert!(outlined.piece.contains("tool/output d6977925") && outlined.piece.contains("code/read a.rs"));
        assert!(outlined.elided.contains("let value_number_7 = compute_the_thing_for(index, 7);"));
        assert!(outline(&act("short"), 0).is_none(), "no outline smaller than a quarter: whole");
        let no_handle = serde_json::json!([{"type": "tool_result", "content": read(200)}]).to_string();
        assert!(outline(&no_handle, 0).is_none(), "no handle to page from: whole");
    }

    // what this catches: a page counted before the act could need it, after a turnover took the
    // outline away, or twice for a request that repeats an old act.
    #[test]
    fn paging_counts_a_later_quote_of_an_elided_line_once() {
        let Some(outlined) = outline(&act(&read(200)), 1_000) else { panic!("outlined") };
        let quote = vec!["let value_number_9 = compute_the_thing_for(index, 9);".to_owned()];
        let mut paging = Paging::default();
        paging.outlined(3, &outlined);
        let other = vec!["an act of hers that quotes nothing she was shown".to_owned()];
        paging.said(2, &quote);
        assert_eq!(paging.paged, 0, "the act at 2 saw it whole");
        paging.said(3, &other);
        paging.said(4, &quote);
        paging.said(5, &other);
        paging.said(6, &quote);
        assert_eq!((paging.outlined, paging.paged), (1, 1), "one page per outline");
        paging.outlined(7, &outlined);
        paging.said(7, &other);
        paging.said(8, &quote);
        paging.outlined(8, &outlined);
        paging.said(9, &quote);
        assert_eq!(paging.paged, 2, "a request repeating the act before it is no new act");
        paging.turned_over();
        paging.said(10, &other);
        paging.said(11, &quote);
        assert_eq!(paging.paged, 2, "a turnover took the outline away");
    }
}
