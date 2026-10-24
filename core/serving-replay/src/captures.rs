//! Act sequences from real prompt captures: the schema-4 JSONL each mind's calls are
//! recorded into (continuum-mind `cognition/prompt_capture.rs`, under
//! `~/.continuum/fixtures/prompt-captures/<persona>.jsonl` and its `.prev.jsonl`).
//! Only `submitted` rows are read: each carries the exact request she was served.

use std::collections::BTreeMap;
use std::path::Path;

use crate::acts::{newest_chars, Act, ActSequence};
use crate::target::{ActRequest, Message, MindId};

#[derive(Debug, Clone)]
pub struct CaptureSelection {
    /// Replay the minds with the most matching acts, this many of them.
    pub minds: usize,
    /// Only requests with this `purpose` (`cognition/act` is a mind acting).
    pub purpose: Option<String>,
    /// Per mind, the newest this-many acts.
    pub acts_per_mind: usize,
    /// Scale down: keep the newest this-many chars of every message and system prompt.
    pub message_tail_chars: Option<usize>,
    pub n_predict: u32,
    pub order: CaptureOrder,
}

/// How the chosen minds' acts interleave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureOrder {
    /// As captured. Minds captured on different days replay one after another.
    Time,
    /// Each mind keeps her own order and the minds take turns: N minds contending for the
    /// seats at once, from captures that were not taken at once.
    RoundRobin,
}

struct Captured {
    at_ms: u64,
    act: Act,
}

pub fn load(dir: &Path, sel: &CaptureSelection) -> Result<ActSequence, String> {
    let mut by_mind: BTreeMap<MindId, Vec<Captured>> = BTreeMap::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("cannot read captures at {}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| format!("cannot list {}: {e}", dir.display()))?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        for line in text.lines() {
            if let Some(c) = submitted(line, sel)? {
                by_mind.entry(c.act.mind.clone()).or_default().push(c);
            }
        }
    }
    let mut ranked: Vec<(MindId, Vec<Captured>)> = by_mind.into_iter().collect();
    ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    let mut per_mind: Vec<std::vec::IntoIter<Captured>> = Vec::new();
    for (_, mut acts) in ranked.into_iter().take(sel.minds) {
        acts.sort_by_key(|c| c.at_ms);
        let skip = acts.len().saturating_sub(sel.acts_per_mind);
        per_mind.push(acts.into_iter().skip(skip).collect::<Vec<_>>().into_iter());
    }
    let acts: Vec<Act> = match sel.order {
        CaptureOrder::Time => {
            let mut chosen: Vec<Captured> = per_mind.into_iter().flatten().collect();
            chosen.sort_by(|a, b| a.at_ms.cmp(&b.at_ms).then_with(|| a.act.source.cmp(&b.act.source)));
            chosen.into_iter().map(|c| c.act).collect()
        }
        CaptureOrder::RoundRobin => {
            let mut out = Vec::new();
            while per_mind.iter().any(|m| m.len() > 0) {
                out.extend(per_mind.iter_mut().filter_map(Iterator::next).map(|c| c.act));
            }
            out
        }
    };
    if acts.is_empty() {
        return Err(format!("no submitted captures match purpose {:?} under {}", sel.purpose, dir.display()));
    }
    let label = format!(
        "captures: {} acts of {} minds (purpose {}, newest {} per mind, {:?} order{}) from {}",
        acts.len(),
        acts.iter().map(|a| &a.mind).collect::<std::collections::BTreeSet<_>>().len(),
        sel.purpose.as_deref().unwrap_or("any"), // unwrap_or: no filter reads as "any"
        sel.acts_per_mind,
        sel.order,
        sel.message_tail_chars.map(|n| format!(", messages scaled to their newest {n} chars")).unwrap_or_default(), // unwrap_or_default: unscaled adds nothing
        dir.display()
    );
    Ok(ActSequence { label, acts })
}

/// One capture line as an act, when it is a `submitted` row the selection wants.
fn submitted(line: &str, sel: &CaptureSelection) -> Result<Option<Captured>, String> {
    // Terminal rows carry the whole raw stream; skip them without decoding.
    if !line.contains("\"event\":\"submitted\"") {
        return Ok(None);
    }
    let row: serde_json::Value = serde_json::from_str(line) // BOUNDARY: one persisted capture row.
        .map_err(|e| format!("capture row is not JSON: {e}"))?;
    if row["schema_version"].as_u64() != Some(4) || row["event"] != "submitted" {
        return Ok(None);
    }
    let request = &row["request"];
    if let Some(purpose) = &sel.purpose {
        if request["purpose"].as_str() != Some(purpose.as_str()) {
            return Ok(None);
        }
    }
    let (Some(mind), Some(at_ms), Some(messages)) =
        (row["persona_id"].as_str(), row["captured_at_ms"].as_u64(), request["messages"].as_array())
    else {
        return Err(format!("submitted row lacks persona_id, captured_at_ms or messages: request {}", row["request_id"]));
    };
    let scale = |text: String| match sel.message_tail_chars {
        Some(n) => newest_chars(&text, n),
        None => text,
    };
    let messages = messages
        .iter()
        .map(|m| Message { role: m["role"].as_str().unwrap_or("user").to_owned(), content: scale(content_text(&m["content"])) }) // unwrap_or: a role-less message is the user's
        .collect();
    let tools = request["tools"]
        .as_array()
        .map(|tools| tools.iter().map(openai_tool).collect())
        .unwrap_or_default(); // unwrap_or_default: a request without tools offers none
    let system = request["systemPrompt"].as_str().filter(|s| !s.is_empty()).map(|s| scale(s.to_owned()));
    Ok(Some(Captured {
        at_ms,
        act: Act {
            mind: MindId(mind.to_owned()),
            source: row["request_id"].as_str().unwrap_or_default().to_owned(), // unwrap_or_default: provenance only
            request: ActRequest { system, messages, tools, n_predict: sel.n_predict },
        },
    }))
}

/// A message's text: a plain string, or the `text` of each part of a parts array.
fn content_text(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(parts) => parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

/// The captured tool shape (`name`, `description`, `input_schema`) as the engine's
/// OpenAI-shape definition.
fn openai_tool(tool: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": tool["name"],
            "description": tool["description"],
            "parameters": tool["input_schema"],
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: replaying the wrong acts. Minds must be ranked by how many
    // matching acts they have, keep their NEWEST acts, interleave in capture order, take
    // parts content and tool schemas in the engine's shape, and skip terminal rows.
    #[test]
    fn captures_become_a_time_ordered_multi_mind_sequence() {
        let dir = std::env::temp_dir().join(format!("serving-replay-captures-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir"); // expect: a test without a temp dir cannot run
        let row = |mind: &str, at: u64, id: &str, purpose: &str| {
            serde_json::json!({"schema_version": 4, "event": "submitted", "captured_at_ms": at,
                "persona_id": mind, "request_id": id, "request": {"purpose": purpose, "systemPrompt": "S",
                "messages": [{"role": "user", "content": [{"type": "text", "text": "a"}, {"type": "text", "text": "b"}]}],
                "tools": [{"name": "t", "description": "d", "input_schema": {"type": "object"}}]}})
            .to_string()
        };
        let a = [row("A", 1, "a1", "cognition/act"), row("A", 3, "a2", "cognition/act"), row("A", 5, "a3", "cognition/act"),
            r#"{"schema_version":4,"event":"completed","request_id":"a3"}"#.to_owned()];
        let b = [row("B", 2, "b1", "cognition/act"), row("B", 4, "b2", "cognition/deliberation")];
        std::fs::write(dir.join("A.jsonl"), a.join("\n")).expect("fixture"); // expect: fixture write in the test's own temp dir
        std::fs::write(dir.join("B.prev.jsonl"), b.join("\n")).expect("fixture"); // expect: fixture write in the test's own temp dir
        let sel = CaptureSelection { minds: 2, purpose: Some("cognition/act".into()), acts_per_mind: 2, message_tail_chars: None, n_predict: 3, order: CaptureOrder::Time };
        let seq = load(&dir, &sel);
        let _ = std::fs::remove_dir_all(&dir);
        let seq = seq.expect("sequence"); // expect: the fixture holds matching acts
        let order: Vec<&str> = seq.acts.iter().map(|a| a.source.as_str()).collect();
        assert_eq!(order, ["b1", "a2", "a3"]);
        let first = &seq.acts[0].request;
        assert_eq!(first.messages[0].content, "a\nb");
        assert_eq!(first.tools[0]["function"]["parameters"], serde_json::json!({"type": "object"}));
        assert_eq!((first.system.as_deref(), first.n_predict), (Some("S"), 3));
        let rr = CaptureSelection { order: CaptureOrder::RoundRobin, ..sel.clone() };
        let rr_dir = std::env::temp_dir().join(format!("serving-replay-captures-rr-{}", std::process::id()));
        std::fs::create_dir_all(&rr_dir).expect("temp dir"); // expect: a test without a temp dir cannot run
        std::fs::write(rr_dir.join("A.jsonl"), a.join("\n")).expect("fixture"); // expect: fixture write in the test's own temp dir
        std::fs::write(rr_dir.join("B.jsonl"), [row("B", 0, "b0", "cognition/act"), row("B", 2, "b1", "cognition/act")].join("\n")).expect("fixture"); // expect: fixture write in the test's own temp dir
        let rr_seq = load(&rr_dir, &rr);
        let _ = std::fs::remove_dir_all(&rr_dir);
        let rr_order: Vec<String> = rr_seq.expect("sequence").acts.into_iter().map(|a| a.source).collect(); // expect: the fixture holds matching acts
        assert_eq!(rr_order, ["a2", "b0", "a3", "b1"]);
        let one = CaptureSelection { minds: 1, ..sel };
        let dir2 = std::env::temp_dir().join(format!("serving-replay-captures-none-{}", std::process::id()));
        assert!(load(&dir2, &one).is_err());
    }
}
