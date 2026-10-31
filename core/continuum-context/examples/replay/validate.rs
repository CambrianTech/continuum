//! The context validator: what a context policy costs and what it keeps, measured on real history.
//!
//! The captures hold what was SENT, so a result canary collapsed has no body left, and a
//! policy that would have kept it could not be judged. The validator restores each result's whole
//! text from the request where it was the pinned newest result, gives it a summary level with
//! `continuum_context::levels`, and counts her real re-reads: a `read_file` of a path she had
//! read before. A re-read is HELD under a policy when that policy's previous prompt still had the
//! earlier read, whole or as its outline: a retention proxy, not proof the re-read was avoidable. Speed is the fresh prefill the
//! policy's prompts cost, priced at a measured prefill rate.
//!
//! Joel, 2026-10-25: "Can we set up some kind of validation scenario to demonstrate our
//! algorithm works?"

use std::collections::{BTreeMap, HashSet};

use continuum_context::cause::{ContentHash, Fingerprint};
use continuum_context::levels::{summarize, Shape};
use continuum_context::source::Item;

/// What one request adds beyond its prompt: the act it answers (its LAST assistant turn), as
/// that turn's pinned result and the files that turn read.
#[derive(Default)]
pub struct Act {
    pub result: Option<String>,
    pub reads: Vec<String>,
    /// Where this capture sits: its session and its act cycle, which counts up by one per act
    /// within a session (every act capture on the M5 steps by exactly 1). The next capture
    /// continues this one only when it is the very next cycle of the same session, so a skipped
    /// capture is a broken chain, never a result filed under the wrong action (Cormac on #5432).
    pub cycle: Option<(String, u64)>,
    /// What the act asked for (MIND-CONTEXT invariant 4): its calls' fingerprint, through the
    /// core's one canonical encoding. `None` for an act with no calls.
    pub fingerprint: Option<Fingerprint>,
    /// Every call is a read tool: the only acts whose unchanged outcome can be a repeat. Until the
    /// commands declare their effect this is the validator's read list (`READ_TOOLS`).
    pub observes: bool,
}

impl Act {
    /// Stamp the act with its capture row's session and cycle.
    pub fn at(mut self, row: &serde_json::Value) -> Self {
        self.cycle = row["session_id"].as_str().zip(row["cycle_id"].as_u64()).map(|(session, cycle)| (session.to_owned(), cycle));
        self
    }

    /// Whether this act's capture is the one right after `previous`'s.
    fn follows(&self, previous: &Option<(String, u64)>) -> bool {
        matches!((&self.cycle, previous), (Some((session, cycle)), Some((was, then))) if session == was && *cycle == then + 1)
    }
}

/// The read tool's names as the captures record them: the native name and the command path.
/// Exact names only: a tool merely containing "read" (a thread, a ready check) is not a read
/// (Cormac on #5432).
pub const READ_TOOLS: [&str; 2] = ["read_file", "code/read"];

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
    let calls_of = |m: &serde_json::Value| -> Vec<serde_json::Value> { parts(m).into_iter().filter(|p| p["type"] == "tool_use").collect() };
    let calls = calls_of(messages[last_assistant]);
    let mut act = Act::default();
    if !calls.is_empty() {
        let encoded: Vec<(String, Vec<u8>)> = calls
            .iter()
            .map(|call| {
                let name = call["name"].as_str().unwrap_or_default().to_owned(); // unwrap_or_default: a nameless call fingerprints as its arguments alone
                let canonical = continuum_tool_protocol::ToolCall { id: String::new(), name: name.clone(), input: call["input"].clone() }.canonical_input();
                (name, canonical)
            })
            .collect();
        act.fingerprint = Some(Fingerprint::of(encoded.iter().map(|(name, input)| (name.as_str(), input.as_slice()))));
        act.observes = calls.iter().all(|call| call["name"].as_str().is_some_and(|name| READ_TOOLS.contains(&name)));
    }
    let mut answered = Vec::new();
    for message in &messages[last_assistant + 1..] {
        for part in parts(message) {
            if part["type"] != "tool_result" {
                continue;
            }
            // Pair only on a real, non-empty id: two missing ids compare equal as null (Codex).
            let Some(id) = part["tool_use_id"].as_str().filter(|id| !id.is_empty()) else { continue };
            let Some(call) = calls.iter().find(|c| c["id"].as_str() == Some(id)) else { continue };
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
pub fn gutter(line: &str) -> Option<(u32, &str)> {
    let (number, code) = line.split_once(" | ")?;
    Some((number.trim().parse().ok()?, code))
}

/// Results made whole again, and which result each read produced.
#[derive(Default)]
pub struct Restored {
    by_seq: BTreeMap<String, String>,
    seen: HashSet<String>,
    /// The previous capture's act: its result, its reads and where it sits.
    pending: Option<(String, Vec<String>, Option<(String, u64)>)>,
    read_seq: BTreeMap<String, String>,
    /// Acts whose result could not be tied to exactly one action: more than one new action
    /// key, or a capture missing between two (Codex on #5432). Counted, never restored.
    pub ambiguous: usize,
    /// Each earlier observing act: its fingerprint, its result's hash and its result, for
    /// invariant 4's repeat identity.
    observed: Vec<(Fingerprint, ContentHash, String)>,
}

/// One act's invariant-4 identity against the acts before it.
#[derive(Default, Clone, Copy)]
pub struct Repeats {
    /// It asked an earlier observing act's exact request again (equal fingerprint).
    pub requests: usize,
    /// ...and got the same outcome (equal result hash): an unchanged observation.
    pub unchanged: usize,
    /// ...and that earlier whole was in the prompt she decided this act on: the view renders
    /// this act as a repeat. An unchanged one whose earlier whole was not in her prompt arrives
    /// whole, because she needs it again (Codex and Fable on the design).
    pub rendered: usize,
}

/// One act's re-reads under a policy.
#[derive(Default, Clone, Copy)]
pub struct Rereads {
    pub total: usize,
    /// Re-reads where the earlier read was still in her prompt, whole or as its outline: a
    /// RETENTION proxy. Whether the re-read was avoidable also needs the file's revision and the
    /// range she needed, which a capture does not prove (Codex on #5432).
    pub held: usize,
}

impl Restored {
    /// Feed one request's action keys. The previous capture's result is restored only when the
    /// chain is proven: this capture is the very next cycle of the same session (no capture
    /// skipped between), and exactly one action is new, so that action is the act. A
    /// gap, two new actions, or none is AMBIGUOUS: counted and never restored, so no body is
    /// filed under an unrelated action (Codex and Cormac on #5432).
    pub fn learn(&mut self, action_keys: &[String], act: &Act) {
        let new: Vec<&String> = action_keys.iter().filter(|key| !self.seen.contains(*key) && key.parse::<u64>().is_ok()).collect();
        if let Some((result, reads, cycle)) = self.pending.take() {
            let chained = act.follows(&cycle);
            match new.as_slice() {
                [only] if chained => {
                    for path in reads {
                        self.read_seq.insert(path, (*only).clone());
                    }
                    self.by_seq.insert((*only).clone(), result);
                }
                _ => self.ambiguous += 1,
            }
        }
        self.seen.extend(action_keys.iter().cloned());
        if let Some(result) = &act.result {
            self.pending = Some((result.clone(), act.reads.clone(), act.cycle.clone()));
        }
    }

    /// Give each action item its whole result again, its summary level, and what was sent as
    /// its pointer. An item sent whole already is left as it was.
    pub fn enrich(&self, items: &mut [Item<String>]) {
        for item in items {
            let Some(whole) = self.by_seq.get(item.key()) else { continue };
            if item.full().chars().count() >= whole.chars().count() {
                continue;
            }
            let sent = item.full().clone();
            let header = sent.lines().next().unwrap_or_default().to_owned(); // unwrap_or_default: an empty item has no header line
            let summary = summarize(whole, Shape::Code(&gutter))
                .or_else(|| summarize(whole, Shape::Output))
                .map(|summary| format!("{header}\n{summary}"));
            // Rebuilt, never edited in place: its content hash is the whole it now holds.
            *item = Item::whole(item.id(), item.cause(), item.key(), format!("{header}\n{whole}"))
                .with_summary(summary)
                .with_pointer(sent);
        }
    }

    /// `act`'s invariant-4 identity against every earlier observing act, judged against `prompt`,
    /// the prompt the act was decided on; then `act` joins the earlier acts. Counts only: no
    /// text leaves this function (THE-PROVING-GROUND P9).
    pub fn repeats(&mut self, act: &Act, prompt: &str) -> Repeats {
        let mut counted = Repeats::default();
        let (Some(fingerprint), Some(result), true) = (act.fingerprint, act.result.as_deref(), act.observes) else {
            return counted;
        };
        let hash = ContentHash::of(result);
        let asked: Vec<&(Fingerprint, ContentHash, String)> = self.observed.iter().filter(|(earlier, _, _)| *earlier == fingerprint).collect();
        if !asked.is_empty() {
            counted.requests = 1;
        }
        let unchanged: Vec<&&(Fingerprint, ContentHash, String)> = asked.iter().filter(|(_, earlier, _)| *earlier == hash).collect();
        if !unchanged.is_empty() {
            counted.unchanged = 1;
        }
        if unchanged.iter().any(|(_, _, whole)| prompt.contains(whole.as_str())) {
            counted.rendered = 1;
        }
        self.observed.push((fingerprint, hash, result.to_owned()));
        counted
    }

    /// The re-reads among `act`'s reads, judged against the prompt the act was decided on: held
    /// when that prompt still had the earlier read whole or as its outline.
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
                counted.held += 1;
            }
        }
        counted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(messages: serde_json::Value) -> serde_json::Value {
        serde_json::json!({ "messages": messages })
    }

    // what this catches: reads from earlier turns, results paired by position, and missing ids
    // pairing as null == null. Only the last assistant turn's calls count, each with its own result.
    #[test]
    fn the_act_is_the_last_turn_paired_by_id() {
        let r = request(serde_json::json!([
            {"role":"assistant","content":[{"type":"tool_use","id":"old","name":"read_file","input":{"file_path":"old.rs"}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"old","content":"old body"}]},
            {"role":"assistant","content":[
                {"type":"tool_use","id":"a","name":"read_file","input":{"file_path":"a.rs"}},
                {"type":"tool_use","id":"b","name":"thread_list","input":{"path":"x"}},
                {"type":"tool_use","name":"read_file","input":{"file_path":"no-id.rs"}}]},
            {"role":"user","content":[
                {"type":"tool_result","tool_use_id":"a","content":"a body"},
                {"type":"tool_result","content":"orphan"}]}
        ]));
        let act = act_of(&r);
        assert_eq!(act.reads, ["a.rs"]);
        assert_eq!(act.result.as_deref(), Some("a body"));
    }

    // what this catches: a body filed under the wrong action. Two new actions in one request, or
    // none because a capture is missing, are ambiguous: counted, never restored.
    #[test]
    fn ambiguous_batches_and_gaps_are_counted_not_restored() {
        let mut restored = Restored::default();
        let act = |path: &str, cycle: u64| Act { result: Some(format!("{path} body")), reads: vec![path.to_owned()], cycle: Some(("s".into(), cycle)), ..Act::default() };
        let keys = |n: u32| -> Vec<String> { (1..=n).map(|k| k.to_string()).collect() };
        restored.learn(&keys(1), &act("a.rs", 1));
        restored.learn(&keys(2), &act("b.rs", 2));
        assert_eq!(restored.by_seq.get("2").map(String::as_str), Some("a.rs body"), "a proven chain restores");
        restored.learn(&keys(4), &act("c.rs", 3));
        assert_eq!(restored.ambiguous, 1, "two new actions are ambiguous");
        // Cormac's case: one new key after a skipped capture. Cycle 4 was never captured, so
        // c's body (cycle 3) must not land on key 5, which cycle 5 shows.
        restored.learn(&keys(5), &act("d.rs", 5));
        assert_eq!(restored.ambiguous, 2, "a single new key after a gap is ambiguous");
        restored.learn(&keys(5), &act("e.rs", 6));
        assert_eq!(restored.ambiguous, 3, "no new action is ambiguous");
        assert!(["3", "4", "5"].iter().all(|k| !restored.by_seq.contains_key(*k)));
    }

    fn read(path: &str, result: &str) -> Act {
        act_of(&request(serde_json::json!([
            {"role":"assistant","content":[{"type":"tool_use","id":"r","name":"read_file","input":{"file_path": path}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"r","content": result}]}
        ])))
    }

    // what this catches: invariant 4's validator counting a repeat it would not render (the
    // earlier whole was not in her prompt, so she needed it again), missing one it would (the
    // 2026-10-30 read loop on the M5), or counting a changed outcome or a write as unchanged.
    #[test]
    fn a_repeat_is_counted_only_for_an_unchanged_read_whose_earlier_whole_she_had() {
        let mut restored = Restored::default();
        let first = read("a.rs", "fn a() {}");
        assert_eq!(restored.repeats(&first, "").requests, 0);
        let again = restored.repeats(&read("a.rs", "fn a() {}"), "earlier: fn a() {} still here");
        assert_eq!((again.requests, again.unchanged, again.rendered), (1, 1, 1));
        let folded = restored.repeats(&read("a.rs", "fn a() {}"), "[#1 collapsed]");
        assert_eq!((folded.requests, folded.unchanged, folded.rendered), (1, 1, 0), "not in her prompt: arrives whole");
        let changed = restored.repeats(&read("a.rs", "fn a() { 1 }"), "fn a() {}");
        assert_eq!((changed.requests, changed.unchanged, changed.rendered), (1, 0, 0));
        let write = act_of(&request(serde_json::json!([
            {"role":"assistant","content":[{"type":"tool_use","id":"w","name":"write_file","input":{"file_path":"a.rs"}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"w","content":"fn a() {}"}]}
        ])));
        assert_eq!(restored.repeats(&write, "fn a() {}").requests, 0, "a write is never a repeat");
        let keys_reordered = act_of(&request(serde_json::json!([
            {"role":"assistant","content":[{"type":"tool_use","id":"r","name":"read_file","input":{"path": "b.rs", "file_path": "a.rs"}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"r","content":"x"}]}
        ])));
        assert_ne!(keys_reordered.fingerprint, read("a.rs", "x").fingerprint, "every argument is part of the request");
    }
}
