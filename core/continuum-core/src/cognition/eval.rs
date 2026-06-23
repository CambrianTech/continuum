//! `cognition/eval` — the test-graded coder gym as a first-class command.
//!
//! Drives a held-out CODER eval through a persona's LIVE cognition — the SAME
//! spawned [`WorkspaceCycle`](super::persona_workspace), the same model, faculties,
//! tools, and `GridTrustAuthPolicy` gate as a real room turn. There is no alternate
//! "eval mind": she really thinks. The only special power the grader holds is the
//! stopwatch — a synthetic eval room has no heartbeat to pace re-perception, so the
//! OBSERVER bounds how many act→observe cycles a task may take (`max_acts`) before
//! it counts as unfinished ([[live-prompt-comes-from-workspacecycle-not-airc-source]],
//! ACTING-ORGANISM.md §4).
//!
//! This is how we DETECT whether a change (a trained LoRA, a prompt, a better base
//! model) actually made her a better coder — the number, not a vibe (SELF-EVOLVING-
//! GENOME §6 slice 1: until lift is real, every later slice is a hypothesis).
//!
//! Grading is OBJECTIVE when a task carries a `test`: take her code, append the
//! test, RUN it, pass = exit 0 (the P1 keystone of ROADMAP-TO-CODING-ITSELF — "did
//! her change make the tests pass?", not substring-on-prose). Descriptive tasks
//! (no `test`) fall back to a case-insensitive substring match on `expect`.
//!
//! Access: `Privileged` → `Trusted`, same tier as [`cognition/trace`] and
//! [`cognition/prompt`]. Driving another mind through a gym (and executing the code
//! it writes) is for trusted local citizens and the owner, never an arbitrary
//! remote `Provisional` peer.
//!
//! [`cognition/trace`]: super::introspect_commands::CognitionTrace
//! [`cognition/prompt`]: super::introspect_commands::CognitionPrompt

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// The committed, discriminating coder set used when the caller passes neither
/// inline `tasks` nor an explicit `eval_set` path. Authoring a harder/specialized
/// eval = add lines to the JSONL, no recompile.
const DEFAULT_EVAL_SET: &str = "docs/genome/coder-eval.jsonl";

/// How many act→observe cycles a single task may take before it counts as
/// unfinished, when the caller doesn't set `max_acts`.
const DEFAULT_MAX_ACTS: u32 = 8;

/// Wall-clock for executing model-generated code under test, in seconds.
const TEST_GRADE_TIMEOUT_SECS: u64 = 10;

/// One eval task. Both the JSONL rows and inline `tasks` deserialize into this;
/// every field is optional so an authoring typo degrades to a benign empty rather
/// than failing the whole run. A task is TEST-GRADED when it carries `test`, else
/// substring-graded against `expect`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
pub struct EvalTask {
    /// Stable id for the task (echoed in results so a regression is identifiable).
    #[serde(default)]
    pub id: String,
    /// The prompt posed to the persona, framed as a room message.
    #[serde(default)]
    pub prompt: String,
    /// Substring the answer must contain (case-insensitive) for descriptive tasks.
    /// Ignored when `test` is present.
    #[serde(default)]
    pub expect: String,
    /// A test program appended to her extracted code and RUN; pass = exit 0. When
    /// present, this objective grade supersedes `expect`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub test: Option<String>,
    /// Language of `test` (slice 1 supports `python` only). Defaults to `python`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lang: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CognitionEvalParams {
    /// The persona (UUID) to put through the gym. Must be spawned (have a live
    /// `WorkspaceCycle`) — the eval drives her real cognition, not a stand-in.
    pub persona_id: String,
    /// Room context the eval turns are scoped to. Omit for the nil room.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub room_id: Option<String>,
    /// Inline tasks. When set, takes precedence over `eval_set`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tasks: Option<Vec<EvalTask>>,
    /// Path to a JSONL eval set (one `EvalTask` per line). Defaults to the committed
    /// `docs/genome/coder-eval.jsonl` when neither this nor `tasks` is given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub eval_set: Option<String>,
    /// Max act→observe cycles per task before it counts as unfinished. Default 8.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub max_acts: Option<u32>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct EvalTaskResult {
    pub id: String,
    /// Did the task pass (test exit 0, or substring matched)?
    pub ok: bool,
    /// Human-readable verdict: "tests passed" / a trimmed traceback / "timeout
    /// (10s)" for test tasks; "substring match" / "no match" for descriptive ones.
    pub grade: String,
    /// How many times she acted (ran code / read / searched) before settling.
    #[ts(type = "number")]
    pub acts: u32,
    /// The first 200 chars of what she SPOKE once settled (empty if she ran out of
    /// the act budget mid-action — an honest "did not finish", never fabricated).
    pub answer: String,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CognitionEvalResult {
    pub persona_id: String,
    /// Tasks passed.
    #[ts(type = "number")]
    pub score: u32,
    /// Tasks attempted.
    #[ts(type = "number")]
    pub total: u32,
    /// `score / total` — THE number a change is measured against.
    pub pass_rate: f64,
    pub results: Vec<EvalTaskResult>,
}

/// The gym command. Stateless: it reaches the persona's live cognition through the
/// global [`WorkspaceCycle`](super::persona_workspace) registry, so it needs no host
/// module state.
#[derive(Default)]
pub struct CognitionEval;

#[async_trait]
impl ActionCommand for CognitionEval {
    const NAME: &'static str = "cognition/eval";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Put a persona through a test-graded coder gym using her LIVE cognition (same model, \
         faculties, tools). Pass persona_id (must be spawned); tasks come from inline `tasks`, an \
         `eval_set` JSONL path, or the default coder-eval set. Returns a pass-rate — the objective \
         number for whether a change made her a better coder.";
    type Params = CognitionEvalParams;
    type Output = CognitionEvalResult;

    async fn run(&self, _ctx: &Ctx, p: CognitionEvalParams) -> Result<CognitionEvalResult, CommandError> {
        let persona_uuid = Uuid::parse_str(&p.persona_id).map_err(|_| {
            CommandError::Invalid(format!("persona_id '{}' is not a valid UUID", p.persona_id))
        })?;
        let room = match p.room_id.as_deref() {
            Some(s) => Uuid::parse_str(s)
                .map_err(|_| CommandError::Invalid(format!("room_id '{s}' is not a valid UUID")))?,
            None => Uuid::nil(),
        };

        // Task source: inline → eval_set JSONL → committed default. A missing file
        // is a loud error (don't silently grade an empty set) UNLESS it's the
        // default path run from a non-repo cwd, where a one-task smoke set keeps the
        // command usable.
        let tasks: Vec<EvalTask> = if let Some(inline) = p.tasks {
            inline
        } else {
            let path = p.eval_set.as_deref().unwrap_or(DEFAULT_EVAL_SET);
            match std::fs::read_to_string(path) {
                Ok(text) => text
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .filter_map(|l| serde_json::from_str::<EvalTask>(l).ok())
                    .collect(),
                Err(_) if p.eval_set.is_none() => vec![EvalTask {
                    id: "render_ai_help".into(),
                    prompt: "Which file defines `fn render_ai_help`? Reply with just the path."
                        .into(),
                    expect: "help.rs".into(),
                    ..Default::default()
                }],
                Err(e) => {
                    return Err(CommandError::Invalid(format!(
                        "eval_set '{path}' could not be read: {e}"
                    )))
                }
            }
        };

        let Some(cycle) = crate::cognition::persona_workspace::global().get(&persona_uuid) else {
            return Err(CommandError::NotFound(format!(
                "no live WorkspaceCycle for persona {persona_uuid} — is it spawned?"
            )));
        };

        let max_acts = p.max_acts.unwrap_or(DEFAULT_MAX_ACTS) as usize;
        let mut pass = 0u32;
        let mut results = Vec::with_capacity(tasks.len());
        for t in &tasks {
            // Frame as a room message so her ORDINARY cognition handles it, then
            // DRIVE her to settlement: she may act (run code, read a file, search),
            // observe the result as memory, and re-perceive — the live act→observe
            // motion, paced by the grader because the eval room has no metronome.
            let burst = format!("[eval]\npeer: {}", t.prompt);
            let settled =
                crate::cognition::act_observe::drive_to_settle(&cycle, burst, room, max_acts).await;
            let answer = settled.spoken.unwrap_or_default();
            let (ok, grade) = if let Some(test) = &t.test {
                let lang = t.lang.as_deref().unwrap_or("python");
                test_grade(&answer, lang, test).await
            } else {
                let m = !t.expect.is_empty()
                    && answer.to_lowercase().contains(&t.expect.to_lowercase());
                (m, if m { "substring match".into() } else { "no match".into() })
            };
            if ok {
                pass += 1;
            }
            results.push(EvalTaskResult {
                id: t.id.clone(),
                ok,
                grade,
                acts: settled.acts as u32,
                answer: answer.chars().take(200).collect(),
            });
        }

        let total = tasks.len() as u32;
        Ok(CognitionEvalResult {
            persona_id: persona_uuid.to_string(),
            score: pass,
            total,
            pass_rate: if total > 0 { pass as f64 / total as f64 } else { 0.0 },
            results,
        })
    }
}

// Stateless → self-register onto the ONE registry (descriptor + runtime object).
crate::register_stateless_command!(CognitionEval);

/// Extract a code block from a model response for test-grading. Prefers the first
/// ```fenced``` block (stripping the language tag line); falls back to the whole
/// text. Small models wrap code in fences inconsistently — this is forgiving.
fn extract_code_block(answer: &str) -> String {
    if let Some(start) = answer.find("```") {
        let after = &answer[start + 3..];
        let body = match after.find('\n') {
            Some(i) => &after[i + 1..], // skip the ```lang tag line
            None => after,
        };
        if let Some(end) = body.find("```") {
            return body[..end].trim().to_string();
        }
    }
    answer.trim().to_string()
}

/// TEST-GRADE a coder task: take the model's code, append the task's test, RUN it,
/// pass = exit 0. The gym's objective grade — not substring-on-prose.
///
/// SAFETY: runs model-generated code in a temp dir with a 10s timeout. That is the
/// pragmatic floor for an OWNER's local dev machine (what coding agents do); it is
/// NOT a sandbox. Before public/untrusted tasks, this MUST run in a real sandbox
/// (container/seccomp). Slice 1 = prove the grading mechanism; sandbox is a P1 req.
async fn test_grade(answer: &str, lang: &str, test: &str) -> (bool, String) {
    let code = extract_code_block(answer);
    let (ext, cmd) = match lang {
        "python" | "py" => ("py", "python3"),
        other => {
            return (
                false,
                format!("unsupported lang '{other}' (slice 1 = python only)"),
            )
        }
    };
    let dir = std::env::temp_dir().join(format!("cu-gym-{}", Uuid::new_v4()));
    if std::fs::create_dir_all(&dir).is_err() {
        return (false, "temp dir create failed".to_string());
    }
    let file = dir.join(format!("sol.{ext}"));
    let full = format!("{code}\n\n{test}\n");
    let write_ok = std::fs::write(&file, full).is_ok();
    let verdict = if !write_ok {
        (false, "temp write failed".to_string())
    } else {
        match tokio::time::timeout(
            std::time::Duration::from_secs(TEST_GRADE_TIMEOUT_SECS),
            tokio::process::Command::new(cmd).arg(&file).output(),
        )
        .await
        {
            Ok(Ok(out)) if out.status.success() => (true, "tests passed".to_string()),
            Ok(Ok(out)) => (
                false,
                String::from_utf8_lossy(&out.stderr)
                    .trim()
                    .chars()
                    .take(180)
                    .collect(),
            ),
            Ok(Err(e)) => (false, format!("spawn failed: {e}")),
            Err(_) => (false, format!("timeout ({TEST_GRADE_TIMEOUT_SECS}s)")),
        }
    };
    let _ = std::fs::remove_dir_all(&dir);
    verdict
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the first fenced block is extracted and the ```lang tag
    // line is stripped, so a model that wraps its answer in ```python … ``` is
    // graded on the CODE, not the fences.
    #[test]
    fn extracts_fenced_code_block_stripping_lang_tag() {
        let answer = "Sure!\n```python\ndef add(a, b):\n    return a + b\n```\nHope that helps.";
        assert_eq!(extract_code_block(answer), "def add(a, b):\n    return a + b");
    }

    // what this catches: un-fenced answers fall back to the whole (trimmed) text,
    // so a model that emits bare code still gets graded rather than scoring 0.
    #[test]
    fn unfenced_answer_falls_back_to_whole_text() {
        assert_eq!(extract_code_block("  def f():\n    return 1  "), "def f():\n    return 1");
    }

    // what this catches: a passing test → exit 0 → PASS with "tests passed".
    #[tokio::test]
    async fn passing_test_grades_pass() {
        let answer = "```python\ndef add(a, b):\n    return a + b\n```";
        let (ok, grade) = test_grade(answer, "python", "assert add(2, 3) == 5").await;
        assert!(ok, "grade was: {grade}");
        assert_eq!(grade, "tests passed");
    }

    // what this catches: a failing assertion → FAIL, and the traceback (not a vibe)
    // is surfaced as the grade so the failure is diagnosable.
    #[tokio::test]
    async fn failing_test_grades_fail_with_traceback() {
        let answer = "```python\ndef add(a, b):\n    return a - b\n```";
        let (ok, grade) = test_grade(answer, "python", "assert add(2, 3) == 5").await;
        assert!(!ok);
        assert!(grade.contains("Error") || grade.contains("assert"), "grade was: {grade}");
    }

    // what this catches: an unsupported language fails LOUD (named reason), never
    // silently passes — the fail-loud contract for the gym.
    #[tokio::test]
    async fn unsupported_lang_fails_loud() {
        let (ok, grade) = test_grade("fn main() {}", "rust", "// test").await;
        assert!(!ok);
        assert!(grade.contains("unsupported lang 'rust'"), "grade was: {grade}");
    }
}
