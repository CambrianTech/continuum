//! The one rendering of every tool result she reads (HER-TOOLS, "One rendering").
//!
//! Joel, 2026-10-31: "Formalized and robust consistent results fed back to our agents are just
//! as important as the calls themselves." Before this, each path wrote its own shape: a refusal
//! as `code/shell-poll: [invalid] …`, a withheld verb as `` `work/review` failed: … ``, a success
//! as JSON that often opened `{"error":null,…}`, which reads like a failure to a model skimming
//! its first line. On the M5 (10-30 05:55Z to 10-31 04:01Z) 6.9% of her calls failed and 32% were
//! wasted (failed, identical repeats, bad polls). A model learns a format from the examples in
//! its own context; one shape, every time, is what makes the next call right the first time.
//!
//! Line 1 is always `<status> <tool>(<identifying args>)[ <cause>]`, in the tool name she called;
//! then the body; then a fixed footer key when one applies. The executor builds every result
//! through [`render`]; nothing else writes a tool result's frame.

use serde_json::Value;

/// How a call ended, one word from a closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// It ran and did what was asked.
    Ok,
    /// It became a task that is still running; its end is delivered to her.
    Running,
    /// It ran and reported failure (a nonzero exit, a failed check).
    Failed,
    /// It did not run: the substrate or the command refused the call.
    Refused,
}

impl Status {
    fn word(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Running => "running",
            Status::Failed => "failed",
            Status::Refused => "refused",
        }
    }
}

/// The arguments that say WHICH call this was (a path, a command line, an id), echoed whole in
/// line 1. Everything else she passed is in her own call, which sits right above the result.
const IDENTIFYING: &[&str] = &[
    "file_path", "path", "cmd", "command", "pattern", "query", "url", "execution_id", "outputId",
    "act", "card_id", "cardId", "submission_id", "room", "name", "branch", "title",
];

/// Line 1: what ended, how, and which call it was.
pub fn status_line(status: Status, tool: &str, input: &Value, cause: Option<&str>) -> String {
    let args: Vec<String> = IDENTIFYING
        .iter()
        .filter_map(|key| input.get(*key).filter(|v| !v.is_null()).map(|v| (key, v)))
        .map(|(key, value)| match value {
            Value::String(text) => format!("{key}={text}"),
            other => format!("{key}={other}"),
        })
        .collect();
    let cause = cause.map(|c| format!(" {c}")).unwrap_or_default(); // unwrap_or_default: no cause, nothing after the call
    format!("{} {tool}({}){cause}", status.word(), args.join(", "))
}

/// The whole result as she reads it: line 1, the body, and the footer line when the call became
/// a task (`wake:`), so she never polls for an end that is delivered to her.
pub fn render(status: Status, tool: &str, input: &Value, cause: Option<&str>, body: &str, task: Option<&str>) -> String {
    let mut out = status_line(status, tool, input, cause);
    if !body.is_empty() {
        out.push('\n');
        out.push_str(body);
    }
    if let Some(task) = task {
        out.push_str(&format!("\nwake: task {task} is running; its end is delivered to you. Do not poll."));
    }
    out
}

/// A command's own fields that say nothing (`"error": null`, `"stderr": ""` beside a stdout) are
/// dropped from the top level before the body is built: a success that opens with an `error`
/// key reads as a failure to a model skimming the first line.
pub fn without_empty_fields(value: &mut Value) {
    if let Value::Object(map) = value {
        map.retain(|_, v| !(v.is_null() || v.as_str().is_some_and(str::is_empty)));
    }
}

/// The cause a failed result names on line 1: its exit code when it reports one.
pub fn exit_cause(value: &Value) -> Option<String> {
    ["exit_code", "exitCode"].iter().find_map(|key| value.get(*key)?.as_i64()).map(|code| format!("exit {code}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // what this catches: two shapes for one outcome. A refusal, a failure and a success all lead
    // with one status word and the call in her own tool name, so the model reads every result the
    // same way (the M5, 10-31: three failure formats and `{"error":null` openings).
    #[test]
    fn every_outcome_leads_with_one_status_word_and_the_call() {
        let read = json!({"file_path": "src/a.rs", "start_line": 10});
        assert_eq!(status_line(Status::Ok, "read_file", &read, None), "ok read_file(file_path=src/a.rs)");
        let shell = json!({"cmd": "cargo test -p x"});
        assert_eq!(
            render(Status::Failed, "bash", &shell, Some("exit 101"), "error[E0432]", None),
            "failed bash(cmd=cargo test -p x) exit 101\nerror[E0432]"
        );
        assert_eq!(
            render(Status::Running, "bash", &shell, None, "", Some("ed17d2d1")),
            "running bash(cmd=cargo test -p x)\nwake: task ed17d2d1 is running; its end is delivered to you. Do not poll."
        );
        assert!(render(Status::Refused, "code_shell-poll", &json!({"execution_id": "3f2a"}), Some("invalid"), "no shell 3f2a", None)
            .starts_with("refused code_shell-poll(execution_id=3f2a) invalid\n"));
    }

    // what this catches: a success whose body opens with `"error":null` (code/search's answer did,
    // 91 times in a day), or a failure that does not name its exit code on line 1.
    #[test]
    fn empty_fields_leave_the_body_and_an_exit_code_is_the_cause() {
        let mut found = json!({"error": null, "stderr": "", "files_searched": 251, "matches": []});
        without_empty_fields(&mut found);
        assert_eq!(found, json!({"files_searched": 251, "matches": []}));
        assert_eq!(exit_cause(&json!({"exit_code": 101})).as_deref(), Some("exit 101"));
        assert_eq!(exit_cause(&json!({"exitCode": null})), None);
    }
}
