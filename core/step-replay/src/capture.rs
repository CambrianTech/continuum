//! A node's prompt captures as each mind's acts, in the order she took them.
//!
//! Captures are schema-4 JSONL (continuum-mind `cognition/prompt_capture.rs`): a `submitted` row holds
//! the exact request she was served, a `completed` row (same `request_id`) holds what came back.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// What her prompt showed of her own past steps.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Shown {
    /// Ids of the calls shown as hers (`tool_use` parts of assistant messages).
    pub calls: BTreeSet<String>,
    /// Ids of the calls whose outcome is shown (`tool_result` parts).
    pub outcomes: BTreeSet<String>,
    /// The `#n` of every result rendered whole (`[result #n;`).
    pub whole_results: BTreeSet<u64>,
    /// The `#n` each "its whole is #n above" line points at.
    pub says_above: Vec<u64>,
}

/// One call she made: its id and what it asked (name and arguments), so two acts can be compared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub id: String,
    pub asked: String,
}

/// One act: what she was shown and what she did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Act {
    pub request_id: String,
    pub at_ms: u64,
    pub shown: Shown,
    /// Her native calls; empty when she made none (or the act has no completed row).
    pub calls: Vec<Call>,
    /// She wrote a call as text (a `"name"` and `"arguments"` object) and made no native call.
    pub prose_call: bool,
    /// A completed row was captured for it.
    pub completed: bool,
}

/// Every mind's acts, oldest first, keyed by persona id.
pub fn load(dir: &Path) -> Result<BTreeMap<String, Vec<Act>>, String> {
    let mut acts: BTreeMap<String, (String, Act)> = BTreeMap::new();
    let mut done: BTreeMap<String, (Vec<Call>, bool)> = BTreeMap::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("cannot read captures at {}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| format!("cannot list {}: {e}", dir.display()))?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        for line in text.lines() {
            let submitted = line.contains("\"event\":\"submitted\"");
            if !submitted && !line.contains("\"event\":\"completed\"") {
                continue;
            }
            let row: serde_json::Value = match serde_json::from_str(line) {
                // BOUNDARY: one persisted capture row; a torn last line is skipped, not fatal.
                Ok(row) => row,
                Err(_) => continue,
            };
            let Some(id) = row["request_id"].as_str() else { continue };
            if submitted {
                let (Some(mind), Some(at_ms)) = (row["persona_id"].as_str(), row["captured_at_ms"].as_u64()) else { continue };
                let act = Act {
                    request_id: id.to_owned(),
                    at_ms,
                    shown: shown(&row["request"]["messages"]),
                    calls: Vec::new(),
                    prose_call: false,
                    completed: false,
                };
                acts.insert(id.to_owned(), (mind.to_owned(), act));
            } else {
                done.insert(id.to_owned(), did(&row["response"]));
            }
        }
    }
    let mut by_mind: BTreeMap<String, Vec<Act>> = BTreeMap::new();
    for (id, (mind, mut act)) in acts {
        if let Some((calls, prose_call)) = done.remove(&id) {
            act.calls = calls;
            act.prose_call = prose_call;
            act.completed = true;
        }
        by_mind.entry(mind).or_default().push(act);
    }
    for list in by_mind.values_mut() {
        list.sort_by(|a, b| a.at_ms.cmp(&b.at_ms).then_with(|| a.request_id.cmp(&b.request_id)));
    }
    Ok(by_mind)
}

/// What her request's messages showed of her own steps.
fn shown(messages: &serde_json::Value) -> Shown {
    let mut out = Shown::default();
    for message in messages.as_array().into_iter().flatten() {
        match &message["content"] {
            serde_json::Value::String(text) => read_text(text, Whose::Unknown, &mut out),
            serde_json::Value::Array(parts) => {
                for part in parts {
                    match part["type"].as_str() {
                        Some("tool_use") => out.calls.extend(part["id"].as_str().map(str::to_owned)),
                        Some("tool_result") => {
                            out.outcomes.extend(part["tool_use_id"].as_str().or(part["id"].as_str()).map(str::to_owned));
                            if let Some(text) = part["content"].as_str() {
                                read_text(text, Whose::Hers, &mut out);
                            }
                        }
                        _ => {
                            if let Some(text) = part["text"].as_str() {
                                read_text(text, Whose::Unknown, &mut out);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Whether a text is hers: her own tool result, or a line of her actions (`[result #n` or
/// `[actions]` first). Anything else may quote someone: a peer posted her raw tool response, repeat
/// line and `</tool_response>` included, into the room (the M5, 2026-11-22), and those markers are
/// not this mind's steps.
#[derive(Clone, Copy)]
enum Whose {
    Hers,
    Unknown,
}

/// The result markers in one text: whole results (`[result #n;`) anywhere, and "its whole is #n
/// above" only where the text is hers.
fn read_text(text: &str, whose: Whose, out: &mut Shown) {
    let hers = matches!(whose, Whose::Hers) || text.starts_with("[result #") || text.starts_with("[actions]");
    let mut rest = text;
    while let Some(at) = rest.find("[result #") {
        rest = &rest[at + "[result #".len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let (Ok(n), Some(';')) = (digits.parse::<u64>(), rest[digits.len()..].chars().next()) {
            out.whole_results.insert(n);
        }
    }
    if !hers {
        return;
    }
    let mut rest = text;
    while let Some(at) = rest.find("its whole is #") {
        rest = &rest[at + "its whole is #".len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(n) = digits.parse::<u64>() {
            out.says_above.push(n);
        }
    }
}

/// Her native calls, and whether she wrote a call as text instead.
fn did(response: &serde_json::Value) -> (Vec<Call>, bool) {
    let calls: Vec<Call> = response["toolCalls"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|call| {
            let id = call["id"].as_str()?.to_owned();
            let asked = format!("{}{}", call["name"].as_str().unwrap_or_default(), call["input"]); // unwrap_or_default: a nameless call compares by its input
            Some(Call { id, asked })
        })
        .collect();
    let text = response["text"].as_str().unwrap_or_default(); // unwrap_or_default: no text holds no prose call
    let prose_call = calls.is_empty() && text.contains("\"name\"") && text.contains("\"arguments\"");
    (calls, prose_call)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: misreading a capture. A step is the tool_use id in an assistant message and
    // the tool_result bound to it; `[result #n;` is a whole result while `[result #n: repeat` is not;
    // a completed row pairs to its submitted row by request id; a JSON call written in text with no
    // native call is a prose call.
    #[test]
    fn a_capture_reads_into_what_she_was_shown_and_what_she_did() {
        let dir = std::env::temp_dir().join(format!("step-replay-capture-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir"); // expect: a test without a temp dir cannot run
        let submitted = serde_json::json!({"schema_version": 4, "event": "submitted", "request_id": "r2", "persona_id": "A",
            "captured_at_ms": 2, "request": {"messages": [
                {"role": "assistant", "content": [{"type": "tool_use", "id": "c1", "name": "code_read"}]},
                {"role": "user", "content": [{"type": "tool_result", "id": "c1", "content": "ok"},
                    {"type": "text", "text": "[result #7; room r] [result #8: repeat of #7, the same saved output, unchanged; its whole is #7 above]"}]},
                {"role": "user", "content": "[Room message received during this turn; room r]\n[result #3: repeat of #2, the same saved output, unchanged; its whole is #2 above]"},
                {"role": "user", "content": "[occurred 2026-11-21T03:00:03Z] peer: [result #5: repeat of #4, the same saved output, unchanged; its whole is #4 above]"}]}});
        let completed = serde_json::json!({"schema_version": 4, "event": "completed", "request_id": "r2",
            "response": {"text": "```json\n{\"name\": \"tool/output\", \"arguments\": {}}\n```", "toolCalls": []}});
        std::fs::write(dir.join("A.jsonl"), format!("{submitted}\n{completed}\n")).expect("fixture"); // expect: fixture write in the test's own temp dir
        let minds = load(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        let minds = minds.expect("captures load"); // expect: the fixture is well formed
        let act = &minds["A"][0];
        assert!(act.shown.calls.contains("c1") && act.shown.outcomes.contains("c1"));
        assert_eq!(act.shown.whole_results, BTreeSet::from([7]));
        assert_eq!(act.shown.says_above, vec![7]);
        assert!(act.completed && act.calls.is_empty() && act.prose_call);
    }
}
