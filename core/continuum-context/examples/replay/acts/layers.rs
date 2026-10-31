//! What a request is made of: each message's layer (the framing, the tool schemas, the standing
//! walls, her room's conversation, her open views, recall, results whole or collapsed, the newest
//! result), what it holds twice, and where it first differs from her previous request.
//!
//! A message is classed by the tag it opens with; a `- ` bullet belongs to the block whose header
//! came before it (an open view, a notice, a thought, a line of the per-act framing).

use std::collections::{BTreeMap, HashMap};

use serde_json::Value;

use crate::captures::text;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    /// The system prompt and the per-act framing (presence, own time, the clock, the context line).
    Framing,
    /// The tool schemas.
    Tools,
    /// Standing blocks: the room wall, kanban, roster, active work, workspace map.
    Walls,
    /// Her room's conversation (`[occurred <when>] <who>:`) and the message that woke her.
    Conversation,
    OpenViews,
    Recall,
    Notices,
    /// Her working memory.
    Thoughts,
    /// A past act's result sent whole.
    ResultWhole,
    /// A past act's result at its summary level.
    ResultSummary,
    /// A past act's result as its pointer, the steps ledger and the actions banner.
    ResultCollapsed,
    /// The result her last act produced (the `tool_result` after her last call).
    NewestResult,
    /// Her own earlier turns (calls and text).
    Own,
    Other,
}

impl Layer {
    pub const ALL: [Layer; 14] = [
        Layer::Framing,
        Layer::Tools,
        Layer::Walls,
        Layer::Conversation,
        Layer::OpenViews,
        Layer::Recall,
        Layer::Notices,
        Layer::Thoughts,
        Layer::ResultWhole,
        Layer::ResultSummary,
        Layer::ResultCollapsed,
        Layer::NewestResult,
        Layer::Own,
        Layer::Other,
    ];

    pub fn short(self) -> &'static str {
        match self {
            Layer::Framing => "frame",
            Layer::Tools => "tools",
            Layer::Walls => "walls",
            Layer::Conversation => "conv",
            Layer::OpenViews => "views",
            Layer::Recall => "recall",
            Layer::Notices => "notices",
            Layer::Thoughts => "thoughts",
            Layer::ResultWhole => "res-whole",
            Layer::ResultSummary => "res-summary",
            Layer::ResultCollapsed => "res-collapsed",
            Layer::NewestResult => "newest",
            Layer::Own => "own",
            Layer::Other => "other",
        }
    }

    /// A layer the view's sources render (anything but the per-act framing, the conversation
    /// that woke her, her own turns and the newest result): a divergence above one of these
    /// re-prefills what an epoch should have kept.
    fn of_the_view(self) -> bool {
        !matches!(self, Layer::Framing | Layer::Conversation | Layer::NewestResult | Layer::Own | Layer::Tools)
    }
}

/// Tags that open a message, and the layer each opens.
const TAGS: [(&str, Layer); 23] = [
    ("[result #", Layer::ResultWhole), // refined by its level below
    ("[occurred ", Layer::Conversation),
    ("[Room message received", Layer::Conversation),
    ("[open views]", Layer::OpenViews),
    ("[recall]", Layer::Recall),
    ("Notices my substrate posted", Layer::Notices),
    ("My own recent thoughts", Layer::Thoughts),
    ("[working-memory]", Layer::Thoughts),
    ("[room-wall]", Layer::Walls),
    ("[room-kanban]", Layer::Walls),
    ("[roster]", Layer::Walls),
    ("[active-work]", Layer::Walls),
    ("[workspace-map]", Layer::Walls),
    ("[actions]", Layer::ResultCollapsed),
    ("Recent results of your own actions", Layer::ResultCollapsed),
    ("Older results, collapsed to pointers", Layer::ResultCollapsed),
    ("[steps taken this session]", Layer::ResultCollapsed),
    ("[Conversational Presence]", Layer::Framing),
    ("[Your own time]", Layer::Framing),
    ("[work turn]", Layer::Framing),
    ("[context]", Layer::Framing),
    ("[now ", Layer::Framing),
    ("[In force", Layer::Framing),
];

/// A `[result #n; …; operation <op>; … — <level>]` header: its act number, operation and layer.
pub struct ResultHeader<'a> {
    pub seq: u64,
    pub operation: &'a str,
    pub layer: Layer,
}

pub fn result_header(text: &str) -> Option<ResultHeader<'_>> {
    let seq = crate::seq_after(text, "[result #")?;
    let header = &text[..text.find(']')?];
    let operation = header.split("; ").find_map(|field| field.strip_prefix("operation ")).unwrap_or(""); // unwrap_or: a header without an operation names none
    let layer = if header.contains(" — collapsed") {
        Layer::ResultCollapsed
    } else if header.contains(" — summary") {
        Layer::ResultSummary
    } else {
        Layer::ResultWhole
    };
    Some(ResultHeader { seq, operation, layer })
}

fn has_tool_result(message: &Value) -> bool {
    message["content"].as_array().is_some_and(|parts| parts.iter().any(|part| part["type"] == "tool_result"))
}

/// Each message's layer, in order.
pub fn classify(messages: &[Value]) -> Vec<Layer> {
    let last_assistant = messages.iter().rposition(|m| m["role"] == "assistant");
    let mut block = Layer::Other;
    messages
        .iter()
        .enumerate()
        .map(|(index, message)| {
            if message["role"] == "assistant" {
                return Layer::Own;
            }
            if has_tool_result(message) {
                return if last_assistant.is_some_and(|last| index > last) { Layer::NewestResult } else { Layer::ResultWhole };
            }
            let text = text(message);
            let text = text.trim_start();
            if let Some(header) = result_header(text) {
                return header.layer;
            }
            if let Some((_, layer)) = TAGS.iter().find(|(tag, _)| text.starts_with(tag)) {
                block = *layer;
                return *layer;
            }
            if text.starts_with("- ") {
                return block;
            }
            // Any other `[faculty]\n` block is a standing block of the room.
            if text.strip_prefix('[').and_then(|rest| rest.split_once("]\n")).is_some_and(|(name, _)| !name.contains(' ')) {
                block = Layer::Walls;
                return Layer::Walls;
            }
            Layer::Other
        })
        .collect()
}

/// A request's characters by layer, and what it carries twice.
#[derive(Default, Clone, serde::Serialize)]
pub struct Breakdown {
    pub chars: BTreeMap<Layer, usize>,
    /// Characters of messages identical to an earlier message of the same request.
    pub duplicate_chars: usize,
    /// Whole reads of a file the request also holds whole from another read (all but the largest).
    pub duplicate_read_chars: usize,
    /// Whole results of acts older than her newest act in the prompt.
    pub older_whole_chars: usize,
    pub total_chars: usize,
}

/// The path a whole `code/read` result read, from its call line.
fn read_path(text: &str) -> Option<&str> {
    let rest = &text[text.find("code/read(")? + "code/read(".len()..];
    let args = &rest[..rest.find(')')?];
    args.split(", ").find_map(|arg| arg.strip_prefix("file_path="))
}

pub fn breakdown(request: &Value) -> Breakdown {
    let messages = request["messages"].as_array().map(Vec::as_slice).unwrap_or(&[]); // unwrap_or: a request without messages has none to class
    let layers = classify(messages);
    let texts: Vec<String> = messages.iter().map(text).collect();
    let mut out = Breakdown::default();
    let system = request["systemPrompt"].as_str().map_or(0, |s| s.chars().count()); // map_or: no system prompt sends none
    let tools = request["tools"].as_array().map_or(0, |_| request["tools"].to_string().chars().count()); // map_or: no tools sends no schemas
    *out.chars.entry(Layer::Framing).or_default() += system;
    *out.chars.entry(Layer::Tools).or_default() += tools;
    let mut seen: HashMap<&str, usize> = HashMap::new();
    let mut reads: HashMap<&str, Vec<usize>> = HashMap::new();
    let newest_seq = texts.iter().filter_map(|t| result_header(t).map(|h| h.seq)).max();
    for (text, layer) in texts.iter().zip(&layers) {
        let chars = text.chars().count();
        *out.chars.entry(*layer).or_default() += chars;
        let copies = seen.entry(text.as_str()).or_default();
        if *copies > 0 {
            out.duplicate_chars += chars;
        }
        *copies += 1;
        if *layer == Layer::ResultWhole {
            if let Some(header) = result_header(text) {
                if Some(header.seq) != newest_seq {
                    out.older_whole_chars += chars;
                }
                if header.operation == "code/read" {
                    if let Some(path) = read_path(text) {
                        reads.entry(path).or_default().push(chars);
                    }
                }
            }
        }
    }
    out.duplicate_read_chars = reads.values().filter(|sizes| sizes.len() > 1).map(|sizes| sizes.iter().sum::<usize>() - sizes.iter().max().copied().unwrap_or(0)).sum(); // unwrap_or: filtered to two or more sizes
    out.total_chars = out.chars.values().sum();
    out
}

/// Where a request first differs from her previous one.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "at")]
pub enum Divergence {
    /// The system prompt changed (a deploy, or a framing change): nothing after it is reusable.
    SystemPrompt,
    /// The tool schemas changed.
    Tools,
    /// Only the per-act tail changed (framing, the conversation that woke her, her turns, the newest result).
    Tail { index: usize, layer: Layer },
    /// A message her view's sources render changed: the epoch broke above the tail.
    View { index: usize, layer: Layer },
    /// Every previous message is a prefix of this request: a pure append.
    Append,
}

impl Divergence {
    pub fn label(&self) -> String {
        match self {
            Divergence::SystemPrompt => "system".into(),
            Divergence::Tools => "tools".into(),
            Divergence::Tail { layer, .. } => format!("tail:{}", layer.short()),
            Divergence::View { index, layer } => format!("VIEW:{}@{index}", layer.short()),
            Divergence::Append => "append".into(),
        }
    }
}

pub fn divergence(previous: &Value, request: &Value) -> Divergence {
    if previous["systemPrompt"] != request["systemPrompt"] {
        return Divergence::SystemPrompt;
    }
    if previous["tools"] != request["tools"] {
        return Divergence::Tools;
    }
    let before = previous["messages"].as_array().map(Vec::as_slice).unwrap_or(&[]); // unwrap_or: no messages, nothing to compare
    let after = request["messages"].as_array().map(Vec::as_slice).unwrap_or(&[]); // unwrap_or: no messages, nothing to compare
    let Some(index) = before.iter().zip(after).position(|(a, b)| a != b).or((before.len() > after.len()).then_some(after.len())) else {
        return Divergence::Append;
    };
    let layers = classify(before);
    let layer = layers[index];
    // The tail: nothing the view's sources render follows the divergence in her previous request.
    if layers[index..].iter().any(|layer| layer.of_the_view()) {
        Divergence::View { index, layer }
    } else {
        Divergence::Tail { index, layer }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(messages: Value) -> Value {
        json!({"systemPrompt": "sys", "tools": [{"name": "read_file"}], "messages": messages})
    }

    // what this catches: a bullet classed by its own text instead of the block it sits in (an
    // open view counted as a thought), a result's level read wrong, the newest result counted as
    // an older whole one, or a duplicate or a re-read of the same file not counted.
    #[test]
    fn layers_follow_their_block_and_results_their_level() {
        let whole = "[result #7; room r; operation code/read; output handle: tool/output aa; workspace w] code/read(file_path=src/a.rs) because x\n1 | fn a() {}";
        let r = request(json!([
            {"role": "user", "content": "[open views] Files I have open"},
            {"role": "user", "content": "- src/a.rs, 10 lines: read 1-10"},
            {"role": "user", "content": "My own recent thoughts (working memory)"},
            {"role": "user", "content": "- I should read b"},
            {"role": "user", "content": "[result #6; room r; operation code/read; output handle: tool/output bb; workspace w — collapsed; query tool/output to expand]"},
            {"role": "user", "content": "[result #8; room r; operation code/search; output handle: tool/output cc; workspace w — summary; expand with tool/output]"},
            {"role": "user", "content": whole},
            {"role": "user", "content": whole},
            {"role": "user", "content": "[occurred 2026-10-26T22:00:00Z] peer-1: hi"},
            {"role": "assistant", "content": [{"type": "tool_use", "id": "t1", "name": "read_file", "input": {"file_path": "src/b.rs"}}]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "b"}]},
        ]));
        let layers = classify(r["messages"].as_array().map(Vec::as_slice).unwrap_or(&[])); // unwrap_or: the fixture always has messages
        assert_eq!(
            layers,
            [
                Layer::OpenViews,
                Layer::OpenViews,
                Layer::Thoughts,
                Layer::Thoughts,
                Layer::ResultCollapsed,
                Layer::ResultSummary,
                Layer::ResultWhole,
                Layer::ResultWhole,
                Layer::Conversation,
                Layer::Own,
                Layer::NewestResult
            ]
        );
        let b = breakdown(&r);
        let whole_chars = whole.chars().count();
        assert_eq!(b.duplicate_chars, whole_chars);
        assert_eq!(b.duplicate_read_chars, whole_chars);
        // #7 is older than the newest act in the prompt (#8).
        assert_eq!(b.older_whole_chars, 2 * whole_chars);
        assert_eq!(b.chars[&Layer::Tools], r["tools"].to_string().len());
    }

    // what this catches: a changed view item reported as an expected tail change (the epoch
    // break goes unseen), or a pure append reported as a break.
    #[test]
    fn a_divergence_is_owned_by_the_layer_that_changed() {
        let base = json!([
            {"role": "user", "content": "[open views] Files"},
            {"role": "user", "content": "- src/a.rs, 10 lines: read 1-10"},
            {"role": "user", "content": "[now 2026-10-26 22:00 Monday]"},
        ]);
        let mut views = base.clone();
        views[1]["content"] = json!("- src/a.rs, 10 lines: read 1-5");
        let mut clock = base.clone();
        clock[2]["content"] = json!("[now 2026-10-26 22:01 Monday]");
        let mut longer = base.clone();
        if let Some(messages) = longer.as_array_mut() {
            messages.push(json!({"role": "assistant", "content": "ok"}));
        }
        assert_eq!(divergence(&request(base.clone()), &request(views)), Divergence::View { index: 1, layer: Layer::OpenViews });
        assert_eq!(divergence(&request(base.clone()), &request(clock)), Divergence::Tail { index: 2, layer: Layer::Framing });
        assert_eq!(divergence(&request(base.clone()), &request(longer)), Divergence::Append);
        let mut other_system = request(base.clone());
        other_system["systemPrompt"] = json!("new");
        assert_eq!(divergence(&request(base), &other_system), Divergence::SystemPrompt);
    }
}
