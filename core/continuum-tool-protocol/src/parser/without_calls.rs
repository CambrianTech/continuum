//! Her words with any tool call written in them taken out: what she SAID, for a line others read.
//!
//! A model without native function calling writes its call as text (a fenced JSON block, or a bare
//! `{"name": .., "arguments": ..}` object) beside its words. The call is already reported as its own
//! act line; left in her words it reaches every roommate as a callable template (the IntelMac,
//! 2026-11-22: a 1.5B resident copied a peer's posted `tool/output` call verbatim, with the peer's
//! output id, which was not hers, and was refused). The syntax is this crate's, so the stripping is
//! too: a fenced block or a balanced object is removed only when [`parse_tool_calls`] reads a call in
//! it, never on its look.

use super::{parse_tool_calls, ResolveName};

/// `text` without the spans that hold a tool call: each fenced block, then each balanced `{..}`
/// outside the fences, that parses as one or more calls. Everything else is kept as written, then
/// trimmed; blank lines left by a removal collapse to one.
pub fn without_calls(text: &str, resolve_name: ResolveName) -> String {
    let unfenced = strip(text, fenced_blocks(text), resolve_name);
    let stripped = strip(&unfenced, objects(&unfenced), resolve_name);
    let mut out = String::with_capacity(stripped.len());
    let mut blank = 0;
    for line in stripped.lines() {
        if line.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_owned()
}

/// `text` with every span in `spans` that parses as a call removed.
fn strip(text: &str, spans: Vec<(usize, usize)>, resolve_name: ResolveName) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (start, end) in spans {
        if parse_tool_calls(&text[start..end], resolve_name).is_empty() {
            continue;
        }
        out.push_str(&text[at..start]);
        at = end;
    }
    out.push_str(&text[at..]);
    out
}

/// The byte spans of each ``` fenced block, fences included; an unclosed fence runs to the end.
fn fenced_blocks(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut from = 0;
    while let Some(open) = text[from..].find("```").map(|i| from + i) {
        let body = open + 3;
        let close = text[body..].find("```").map_or(text.len(), |i| body + i + 3); // map_or: an unclosed fence runs to the end
        spans.push((open, close));
        from = close;
    }
    spans
}

/// The byte spans of each top-level balanced `{..}`, strings and escapes respected.
fn objects(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let (mut depth, mut start, mut in_string, mut escaped) = (0usize, 0usize, false, false);
    for (i, c) in text.char_indices() {
        if in_string {
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' if depth > 0 => in_string = true,
            '{' => {
                if depth == 0 {
                    start = i;
                }
                depth += 1;
            }
            '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    spans.push((start, i + 1));
                }
            }
            _ => {}
        }
    }
    spans
}
