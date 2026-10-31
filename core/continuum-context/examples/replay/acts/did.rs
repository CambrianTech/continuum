//! What she did with an act: the calls her response made (name and target), what kind of act
//! that was (write, shell, board, read, talk), and a signature to tell whether she repeated
//! her previous act exactly (Benchy, 125 identical acts in an hour on 2026-10-20).

use serde_json::Value;

use crate::validate::READ_TOOLS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Nothing came back (a failed or cancelled request), or an empty response.
    Silent,
    /// Text only: she spoke.
    Talk,
    /// A tool this table does not know.
    Other,
    Read,
    /// The work board: claim, note, submit, release.
    Board,
    Shell,
    /// Changed a file or the repository: edit, write, commit, push, open a PR.
    Write,
}

/// Tools that change her workspace or the repository. The capture names a tool by its native
/// name or its command path.
const WRITES: [&str; 9] = ["edit_file", "write_file", "code/edit", "code/write", "git_commit", "git_push", "code/git/commit", "code/git/push", "code_github_pr-create"];
const SHELLS: [&str; 4] = ["bash", "run_code", "code_shell-poll", "code/shell"];
const BOARD: [&str; 8] = ["claim_task", "work_create", "heartbeat_task", "ledger", "release_task", "update_task", "work_submit", "work/note"];
const READS: [&str; 12] = ["grep", "list_files", "file_tree", "git_status", "git_diff", "web_fetch", "web_search", "help", "list_commands", "get_task", "list_tasks", "code/search"];

fn kind_of(name: &str) -> Kind {
    if WRITES.contains(&name) {
        Kind::Write
    } else if SHELLS.contains(&name) {
        Kind::Shell
    } else if BOARD.contains(&name) {
        Kind::Board
    } else if READ_TOOLS.contains(&name) || READS.contains(&name) {
        Kind::Read
    } else {
        Kind::Other
    }
}

/// The argument that says what a call acted on.
const TARGETS: [&str; 9] = ["file_path", "path", "cmd", "command", "pattern", "card_id", "url", "query", "code"];

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Call {
    pub name: String,
    pub target: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Did {
    pub kind: Kind,
    pub calls: Vec<Call>,
    /// Characters of text she said alongside (or instead of) the calls.
    pub said_chars: usize,
    /// The calls with their whole inputs, or the text when she only spoke: equal signatures are
    /// the same act.
    #[serde(skip)]
    pub signature: String,
}

impl Default for Did {
    fn default() -> Self {
        Did { kind: Kind::Silent, calls: Vec::new(), said_chars: 0, signature: String::new() }
    }
}

/// What a `completed` response did.
pub fn did(response: &Value) -> Did {
    let calls: Vec<&Value> = response["toolCalls"].as_array().map(|calls| calls.iter().collect()).unwrap_or_default(); // unwrap_or_default: a text-only response made no calls
    let text = response["text"].as_str().unwrap_or(""); // unwrap_or: a response without text said nothing
    let kind = calls.iter().filter_map(|call| call["name"].as_str()).map(kind_of).max().unwrap_or(if text.trim().is_empty() { Kind::Silent } else { Kind::Talk }); // unwrap_or: no calls, so it is what she said
    let signature = if calls.is_empty() { text.to_owned() } else { calls.iter().map(|call| format!("{}{}", call["name"], call["input"])).collect::<Vec<_>>().join("\n") };
    Did {
        kind,
        calls: calls
            .iter()
            .map(|call| Call {
                name: call["name"].as_str().unwrap_or("?").to_owned(), // unwrap_or: a nameless call shows as `?`
                target: TARGETS.iter().find_map(|key| call["input"][key].as_str()).map(str::to_owned),
            })
            .collect(),
        said_chars: text.chars().count(),
        signature,
    }
}

impl Did {
    /// The same act as `previous`: identical calls, or identical words, and not silence.
    pub fn repeats(&self, previous: &Did) -> bool {
        self.kind != Kind::Silent && !self.signature.is_empty() && self.signature == previous.signature
    }

    /// `read_file(src/a.rs) grep(fn x)` for the table, each target cut to `width` characters.
    pub fn summary(&self, width: usize) -> String {
        if self.calls.is_empty() {
            return if self.said_chars > 0 { format!("said {} chars", self.said_chars) } else { "-".to_owned() };
        }
        self.calls
            .iter()
            .map(|call| match &call.target {
                Some(target) => {
                    let line = target.lines().next().unwrap_or(""); // unwrap_or: an empty target shows empty
                    let cut: String = line.chars().take(width).collect();
                    format!("{}({cut}{})", call.name, if cut.len() < target.len() { "…" } else { "" })
                }
                None => call.name.clone(),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // what this catches: a write hidden behind a read in the same batch (the act counted as a
    // read), a re-issued identical call not seen as a repeat, or two empty responses counted as
    // repeating each other.
    #[test]
    fn the_strongest_call_names_the_act_and_identical_calls_repeat() {
        let batch = json!({"text": "", "toolCalls": [
            {"name": "read_file", "input": {"file_path": "src/a.rs"}},
            {"name": "edit_file", "input": {"file_path": "src/a.rs", "old": "x", "new": "y"}},
        ]});
        let first = did(&batch);
        assert_eq!(first.kind, Kind::Write);
        assert_eq!(first.summary(20), "read_file(src/a.rs) edit_file(src/a.rs)");
        assert!(did(&batch).repeats(&first));
        let other = did(&json!({"text": "", "toolCalls": [{"name": "read_file", "input": {"file_path": "src/b.rs"}}]}));
        assert_eq!(other.kind, Kind::Read);
        assert!(!other.repeats(&first));
        assert_eq!(did(&json!({"text": "Done."})).kind, Kind::Talk);
        assert!(!Did::default().repeats(&Did::default()));
    }
}
