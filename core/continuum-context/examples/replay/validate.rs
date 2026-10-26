//! The context validator: what a context policy costs and what it keeps, measured on real history.
//!
//! The captures hold what was SENT, so a result canary collapsed has no body left, and a
//! policy that would have kept it could not be judged. The validator restores each result's whole
//! text from the request where it was the pinned newest result, gives it a summary level with
//! `continuum_context::levels`, and counts her real re-reads: a `read_file` of a path she had
//! read before. A re-read is PREVENTABLE under a policy when that policy's previous prompt
//! still held the earlier read, whole or as its outline. Speed is the fresh prefill the
//! policy's prompts cost, priced at a measured prefill rate.
//!
//! Joel, 2026-10-25: "Can we set up some kind of validation scenario to demonstrate our
//! algorithm works?"

use std::collections::{BTreeMap, HashSet};

use continuum_context::levels::{summarize, Shape};
use continuum_context::source::Item;

/// What one request adds beyond its prompt: the act it answers (its LAST assistant turn), as
/// that turn's pinned result and the files that turn read.
#[derive(Default)]
pub struct Act {
    pub result: Option<String>,
    pub reads: Vec<String>,
}

/// The read tool's names as the captures record them: the native name and the command path.
/// Exact names only: a tool merely containing "read" (a thread, a ready check) is not a read
/// (Cormac on #5432).
const READ_TOOLS: [&str; 2] = ["read_file", "code/read"];

/// The act a request carries: the `tool_use` calls of its last assistant turn, each paired by
/// `tool_use_id` with the `tool_result` that answered it (Codex and Cormac on #5432: reads had
/// accumulated across all history, and the result was simply the last one). Its result is the
/// text answering those calls; its reads are the paths of those calls that were reads.
pub fn act_of(request: &serde_json::Value) -> Act {
    let messages: Vec<&serde_json::Value> = request["messages"].as_array().into_iter().flatten().collect();
    let Some(last_assistant) = messages.iter().rposition(|m| m["role"] == "assistant") else {
        return Act::default();
    };
    let parts = |m: &serde_json::Value| m["content"].as_array().cloned().unwrap_or_default(); // unwrap_or_default: a text message carries no tool parts
    let calls: Vec<serde_json::Value> = parts(messages[last_assistant]).into_iter().filter(|p| p["type"] == "tool_use").collect();
    let mut act = Act::default();
    let mut answered = Vec::new();
    for message in &messages[last_assistant + 1..] {
        for part in parts(message) {
            if part["type"] != "tool_result" {
                continue;
            }
            let Some(call) = calls.iter().find(|c| c["id"] == part["tool_use_id"]) else { continue };
            let text = match &part["content"] {
                serde_json::Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            answered.push(text);
            if call["name"].as_str().is_some_and(|name| READ_TOOLS.contains(&name)) {
                if let Some(path) = call["input"]["file_path"].as_str().or(call["input"]["path"].as_str()) {
                    act.reads.push(path.to_owned());
                }
            }
        }
    }
    if !answered.is_empty() {
        act.result = Some(answered.join("\n"));
    }
    act
}

/// The hands gutter, `  N | code`: what a numbered read renders.
fn gutter(line: &str) -> Option<(u32, &str)> {
    let (number, code) = line.split_once(" | ")?;
    Some((number.trim().parse().ok()?, code))
}

/// Results made whole again, and which result each read produced.
#[derive(Default)]
pub struct Restored {
    by_seq: BTreeMap<String, String>,
    seen: HashSet<String>,
    pending: Option<(String, Vec<String>)>,
    read_seq: BTreeMap<String, String>,
}

/// One act's re-reads under a policy.
#[derive(Default, Clone, Copy)]
pub struct Rereads {
    pub total: usize,
    pub preventable: usize,
}

impl Restored {
    /// Feed one request's action keys: an action first seen now is the act the previous
    /// request pinned, so it owns that whole result and the paths that act read.
    pub fn learn(&mut self, action_keys: &[String], act: &Act) {
        let new: Vec<&String> = action_keys.iter().filter(|key| !self.seen.contains(*key) && key.parse::<u64>().is_ok()).collect();
        if let (Some(newest), Some((result, reads))) = (new.iter().max_by_key(|key| key.parse::<u64>().unwrap_or(0)), self.pending.take()) { // unwrap_or: keys were filtered to numbers
            for path in reads {
                self.read_seq.insert(path, (*newest).clone());
            }
            self.by_seq.insert((*newest).clone(), result);
        }
        self.seen.extend(action_keys.iter().cloned());
        if let Some(result) = &act.result {
            self.pending = Some((result.clone(), act.reads.clone()));
        }
    }

    /// Give each action item its whole result again, its summary level, and what was sent as
    /// its pointer. An item sent whole already is left as it was.
    pub fn enrich(&self, items: &mut [Item<String>]) {
        for item in items {
            let Some(whole) = self.by_seq.get(&item.key) else { continue };
            if item.full.chars().count() >= whole.chars().count() {
                continue;
            }
            let sent = item.full.clone();
            let header = sent.lines().next().unwrap_or_default().to_owned(); // unwrap_or_default: an empty item has no header line
            item.summary = summarize(whole, Shape::Code(&gutter))
                .or_else(|| summarize(whole, Shape::Output))
                .map(|summary| format!("{header}\n{summary}"));
            item.full = format!("{header}\n{whole}");
            item.pointer = Some(sent);
        }
    }

    /// The re-reads among `act`'s reads, judged against the prompt the act was decided on:
    /// preventable when that prompt still held the earlier read whole or as its outline.
    pub fn rereads(&self, act: &Act, prompt: &str) -> Rereads {
        let mut counted = Rereads::default();
        for path in &act.reads {
            let Some(seq) = self.read_seq.get(path) else { continue };
            counted.total += 1;
            let Some(whole) = self.by_seq.get(seq) else { continue };
            let held_whole = prompt.contains(whole.as_str());
            let held_outline = summarize(whole, Shape::Code(&gutter))
                .or_else(|| summarize(whole, Shape::Output))
                .is_some_and(|summary| prompt.contains(summary.as_str()));
            if held_whole || held_outline {
                counted.preventable += 1;
            }
        }
        counted
    }
}
