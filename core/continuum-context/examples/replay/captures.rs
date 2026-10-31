//! The one reader of the prompt captures (schema 4, `~/.continuum/fixtures/prompt-captures/<persona>.jsonl`)
//! for this example: every mode (the view replay, `--acts`) loads rows through it.
//!
//! A capture file holds one `submitted` row per request, carrying the exact request she was
//! served, and one terminal row per request: `completed` (the response, its usage and the
//! engine's timing), `failed` (the error) or `cancelled`. Rows submitted outside the window are
//! skipped before they are decoded: the stamp is read from the line's text, so a night of
//! captures loads in seconds.

use std::collections::HashMap;

use serde_json::Value;

/// A UTC window in Unix milliseconds: `since` inclusive, `until` exclusive; `None` is open.
#[derive(Clone, Copy, Default, Debug)]
pub struct Window {
    pub since_ms: Option<u64>,
    pub until_ms: Option<u64>,
}

impl Window {
    pub fn holds(&self, at_ms: u64) -> bool {
        self.since_ms.is_none_or(|since| at_ms >= since) && self.until_ms.is_none_or(|until| at_ms < until)
    }
}

/// The rows of a set of capture files.
#[derive(Default)]
pub struct Captures {
    /// `submitted` rows inside the window, in file order.
    pub submitted: Vec<Value>,
    /// The terminal row of each request, by request id.
    pub terminal: HashMap<String, Value>,
}

/// `captured_at_ms` read from the line's text, without decoding the row.
fn stamp(line: &str) -> Option<u64> {
    let key = "\"captured_at_ms\":";
    let rest = line[line.find(key)? + key.len()..].trim_start();
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len()); // unwrap_or: the number runs to the end of the text
    rest[..end].parse().ok()
}

/// Read capture text into `into`: submitted rows inside `window`, terminal rows from its start on
/// (a request submitted inside the window may end after it).
pub fn read(text: &str, window: Window, into: &mut Captures) {
    for line in text.lines() {
        let Some(at_ms) = stamp(line) else { continue };
        if window.since_ms.is_some_and(|since| at_ms < since) {
            continue;
        }
        let Ok(row) = serde_json::from_str::<Value>(line) else { continue }; // BOUNDARY: one persisted capture row; a torn last line is skipped
        match row["event"].as_str() {
            Some("submitted") if window.holds(at_ms) => into.submitted.push(row),
            Some("completed" | "failed" | "cancelled") => {
                if let Some(id) = row["request_id"].as_str() {
                    into.terminal.insert(id.to_owned(), row);
                }
            }
            _ => {}
        }
    }
}

/// Load every file in `paths`.
pub fn load(paths: &[String], window: Window) -> Result<Captures, Box<dyn std::error::Error>> {
    let mut captures = Captures::default();
    for path in paths {
        read(&std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?, window, &mut captures);
    }
    Ok(captures)
}

/// A message's text as the prompt measures it: a string as is, a parts array as its JSON.
pub fn text(message: &Value) -> String {
    match &message["content"] {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a row submitted before the window is decoded (or kept) anyway, or a
    // terminal row for a request inside the window is lost because it landed after `until`.
    #[test]
    fn the_window_keeps_its_submissions_and_their_late_terminals() {
        let text = [
            r#"{"event":"submitted","captured_at_ms":100,"request_id":"a"}"#,
            r#"{"event":"submitted","captured_at_ms":200,"request_id":"b"}"#,
            r#"{"event":"completed","captured_at_ms":350,"request_id":"b"}"#,
            r#"{"event":"submitted","captured_at_ms":400,"request_id":"c"}"#,
            "{\"event\":\"comp", // a torn last line
        ]
        .join("\n");
        let mut captures = Captures::default();
        read(&text, Window { since_ms: Some(150), until_ms: Some(300) }, &mut captures);
        let ids: Vec<&str> = captures.submitted.iter().filter_map(|row| row["request_id"].as_str()).collect();
        assert_eq!(ids, ["b"]);
        assert!(captures.terminal.contains_key("b"));
    }
}
