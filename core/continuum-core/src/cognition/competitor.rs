//! CompetitorAgent — pluggable external coding-agent arms for the benchmark.
//!
//! The product question is "is Continuum better than the Hermes AGENT at coding?" —
//! not "does our persona extract more from these weights?" ([[hermes-agent-is-a-runnable-benchmark-opponent-arm]]).
//! Answering it honestly means running each rival harness AS PEOPLE RUN IT, against a
//! model endpoint we control, and grading every arm's answer with the SAME grader as
//! Continuum's own cognition ([`crate::cognition::gym_grader`]). Product vs product,
//! one grader, one scoreboard.
//!
//! Every arm is an OPTIONAL adapter behind ONE interface ([`CompetitorAgent`]):
//!   - it self-reports [`available`](CompetitorAgent::available) — an arm whose CLI is
//!     not installed is SKIPPED with a logged reason, never silently faked
//!     ([[fallbacks-are-illegal-fail-loud]]);
//!   - it works with a DEFAULT endpoint location ([`DEFAULT_ENDPOINT`]) when a caller
//!     gives none, so a manual run needs no config, while the benchmark runner passes a
//!     freshly-provisioned dedicated opponent lane's `base_url`
//!     ([[benchmark-needs-its-own-serving-lane]]) — never the live core's shared
//!     serving, which restarts on deploys and contends with personas.
//!
//! Outlier-validated interface ([[joel-boundary-design-values]] — build the two most
//! different adapters, prove the interface fits both without forcing, then stop):
//!   - [`RawOneshotArm`] — outlier A, the simplest possible arm: one `/v1/chat/completions`
//!     POST. The floor every harness must beat to justify its overhead.
//!   - [`HermesArm`] — outlier B, maximally different: an external agent SUBPROCESS
//!     (`hermes -z`) with its own tools/memory/skills loop, reached over the same
//!     OpenAI-compatible endpoint via its `lmstudio` provider.
//! Continuum's native arm is the existing [`crate::cognition::eval`] cognition path;
//! it is wrapped into the same runner in the follow-up slice.

use crate::cognition::eval::EvalTask;
use async_trait::async_trait;
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

/// OpenAI-compatible endpoint an arm targets when a caller gives none. This is the
/// convenience default for a MANUAL run — the benchmark runner always provisions a
/// dedicated opponent lane and passes its real `base_url`. Never the live core's
/// shared serving (`:58057`): that restarts on every deploy and contends with the
/// living personas ([[benchmark-needs-its-own-serving-lane]]).
pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:58300/v1";

/// Placeholder bearer token for a local, keyless llama-server. A local `/v1` ignores
/// it; Hermes's `lmstudio` provider still requires SOME `LM_API_KEY` to be set.
const DEFAULT_API_KEY: &str = "sk-local";

/// Wall-clock ceiling for a single task solve. An external agent (Hermes) runs a full
/// tool loop, so this is generous; a hung arm is reaped (`kill_on_drop`) at the bound,
/// never left orphaned.
const SOLVE_TIMEOUT: Duration = Duration::from_secs(300);

/// Hermes rejects at STARTUP any auxiliary compression model whose served context is
/// below this (proven 2026-07-21: "Auxiliary compression model … below the minimum
/// 64,000"). The opponent lane the runner provisions for a Hermes arm MUST therefore
/// serve ≥64K (for a 32K-native GGUF: `--ctx-size 65536 --rope-scaling yarn
/// --yarn-orig-ctx 32768`). Surfaced here so the runner sizes the lane correctly.
pub const HERMES_MIN_SERVED_CONTEXT: u32 = 64_000;

/// Resolve a caller's optional endpoint to a concrete location — the "works with a
/// default location" contract. `Some(url)` passes through (trailing slash trimmed);
/// `None` falls back to [`DEFAULT_ENDPOINT`].
pub fn resolve_endpoint(endpoint: Option<&str>) -> String {
    endpoint
        .map(|e| e.trim_end_matches('/').to_string())
        .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string())
}

/// One coding task posed to a competitor arm, with the endpoint already resolved.
#[derive(Debug, Clone)]
pub struct SolveRequest {
    /// The task prompt, verbatim (the arm frames it however it likes).
    pub prompt: String,
    /// The served model id the endpoint exposes (e.g. `qwen-coder-1.5b`).
    pub model: String,
    /// Resolved OpenAI-compatible base URL (default already applied).
    pub endpoint: String,
    /// Bearer token for the endpoint (local llama-server ignores it).
    pub api_key: String,
}

impl SolveRequest {
    /// Build a request, applying the default endpoint location when `endpoint` is None.
    pub fn new(prompt: impl Into<String>, model: impl Into<String>, endpoint: Option<&str>) -> Self {
        Self {
            prompt: prompt.into(),
            model: model.into(),
            endpoint: resolve_endpoint(endpoint),
            api_key: DEFAULT_API_KEY.to_string(),
        }
    }
}

/// The result of an arm solving one task — the raw answer plus the two signals the
/// self-diagnosing classifier needs (`output_tokens` ≤4 = a decline/wedge = harness
/// noise, not a capability miss) and latency for the scoreboard.
#[derive(Debug, Clone)]
pub struct SolveOutcome {
    /// The arm's final answer text, verbatim (graded by [`grade_answer`]).
    pub answer: String,
    /// Completion tokens the arm produced (exact from `/v1` usage where available, else
    /// a whitespace-word estimate for a subprocess arm that only yields stdout).
    pub output_tokens: u32,
    /// Wall-clock milliseconds this solve took.
    pub latency_ms: u64,
}

/// A pluggable external coding agent. Implementors are OPTIONAL: the runner filters on
/// [`available`](Self::available) and skips (logs) the rest — never silently faked.
#[async_trait]
pub trait CompetitorAgent: Send + Sync {
    /// Stable arm name for the scoreboard/events (e.g. `"hermes"`, `"raw-oneshot"`).
    fn name(&self) -> &'static str;

    /// Is this arm usable in THIS environment right now (its CLI/deps present)? An
    /// unavailable arm is SKIPPED with a logged reason, never substituted.
    fn available(&self) -> bool;

    /// Solve one task against the endpoint, returning the answer to grade.
    async fn solve(&self, req: &SolveRequest) -> Result<SolveOutcome, String>;
}

// ── Outlier A: the raw one-shot arm (simplest possible) ─────────────────────────────

/// The floor arm: a single greedy `/v1/chat/completions` POST, no agent loop. Every
/// harness with a tool/memory/skills loop must beat THIS to justify its overhead.
/// Always available (it is just HTTP; endpoint readiness is the runner's concern).
pub struct RawOneshotArm;

#[async_trait]
impl CompetitorAgent for RawOneshotArm {
    fn name(&self) -> &'static str {
        "raw-oneshot"
    }

    fn available(&self) -> bool {
        true
    }

    async fn solve(&self, req: &SolveRequest) -> Result<SolveOutcome, String> {
        let client = reqwest::Client::builder()
            .timeout(SOLVE_TIMEOUT)
            .build()
            .map_err(|e| format!("raw-oneshot: client build failed: {e}"))?;
        let url = format!("{}/chat/completions", req.endpoint);
        // Greedy (temperature 0) for a deterministic grade, matching the eval harness's
        // `isolate_for_eval`. No hardcoded `max_tokens`: the server owns generation
        // length ([[audit-for-clamps-whenever-patching]], #45).
        let body = serde_json::json!({
            "model": req.model,
            "messages": [{ "role": "user", "content": req.prompt }],
            "temperature": 0
        });
        let started = Instant::now();
        let resp = client
            .post(&url)
            .bearer_auth(&req.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("raw-oneshot: POST {url} failed: {e}"))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| format!("raw-oneshot: read body failed: {e}"))?;
        if !status.is_success() {
            return Err(format!("raw-oneshot: {url} returned {status}: {text}"));
        }
        let parsed: ChatResponse = serde_json::from_str(&text)
            .map_err(|e| format!("raw-oneshot: malformed /v1 response ({e}): {text}"))?;
        let answer = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .unwrap_or_default();
        let output_tokens = parsed
            .usage
            .map(|u| u.completion_tokens)
            .unwrap_or_else(|| estimate_tokens(&answer));
        Ok(SolveOutcome {
            answer,
            output_tokens,
            latency_ms: started.elapsed().as_millis() as u64,
        })
    }
}

// ── Outlier B: the Hermes agent subprocess (maximally different) ─────────────────────

/// The Hermes agent harness, run one-shot as people run it: `hermes -z "<prompt>" -m
/// <model> --provider lmstudio` with `LM_BASE_URL` pointed at our endpoint. Hermes runs
/// its FULL agent loop (tools/memory/skills) and prints the final answer to stdout.
/// Available iff the `hermes` CLI is installed. The opponent lane MUST serve
/// ≥[`HERMES_MIN_SERVED_CONTEXT`] or Hermes refuses at startup.
pub struct HermesArm;

#[async_trait]
impl CompetitorAgent for HermesArm {
    fn name(&self) -> &'static str {
        "hermes"
    }

    fn available(&self) -> bool {
        find_hermes().is_some()
    }

    async fn solve(&self, req: &SolveRequest) -> Result<SolveOutcome, String> {
        let bin = find_hermes()
            .ok_or_else(|| "hermes: CLI not found on PATH or ~/.local/bin".to_string())?;
        let mut cmd = tokio::process::Command::new(bin);
        cmd.args(hermes_args(&req.model, &req.prompt))
            // Point the `lmstudio` provider at OUR endpoint. `HERMES_YOLO_MODE=1`
            // auto-approves so `-z` runs fully non-interactive (no TTY).
            .env("LM_BASE_URL", &req.endpoint)
            .env("LM_API_KEY", &req.api_key)
            .env("HERMES_YOLO_MODE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let started = Instant::now();
        let output = match tokio::time::timeout(SOLVE_TIMEOUT, cmd.output()).await {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return Err(format!("hermes: spawn/exec failed: {e}")),
            Err(_) => {
                return Err(format!(
                    "hermes: no answer within {}s (agent loop hung)",
                    SOLVE_TIMEOUT.as_secs()
                ))
            }
        };
        let answer = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if answer.is_empty() {
            // Fail LOUD with the harness's own stderr — an empty answer is a real
            // failure to surface (e.g. the <64K-context startup rejection), never a
            // silent skip ([[fallbacks-are-illegal-fail-loud]]).
            let stderr = String::from_utf8_lossy(&output.stderr);
            let tail: String = stderr.lines().rev().take(3).collect::<Vec<_>>().join(" | ");
            return Err(format!("hermes: empty answer (stderr: {tail})"));
        }
        Ok(SolveOutcome {
            output_tokens: estimate_tokens(&answer),
            answer,
            latency_ms: started.elapsed().as_millis() as u64,
        })
    }
}

/// The one-shot, non-interactive Hermes invocation. Factored out so the wiring is
/// unit-testable: `-z` is the TTY-free one-shot mode; `--provider lmstudio` routes to
/// the LOCAL OpenAI-compatible endpoint (`LM_BASE_URL`) instead of Hermes's default
/// Nous cloud. A regression here silently sends every task to the cloud (connection
/// error) or to an interactive prompt that never returns.
fn hermes_args(model: &str, prompt: &str) -> Vec<String> {
    vec![
        "-z".to_string(),
        prompt.to_string(),
        "-m".to_string(),
        model.to_string(),
        "--provider".to_string(),
        "lmstudio".to_string(),
    ]
}

/// Locate the `hermes` CLI: the known install path first, then PATH. `None` = the arm
/// is unavailable (skipped, never faked).
fn find_hermes() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("HOME") {
        let p = PathBuf::from(home).join(".local/bin/hermes");
        if p.is_file() {
            return Some(p);
        }
    }
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join("hermes"))
        .find(|p| p.is_file())
}

// ── Shared: registry + grading ──────────────────────────────────────────────────────

/// Every OPTIONAL external arm, each self-reporting availability. The runner filters on
/// [`CompetitorAgent::available`] and skips (logs) the unavailable ones. Continuum's
/// native cognition arm is not here — it is the [`crate::cognition::eval`] path the
/// runner drives directly.
pub fn optional_arms() -> Vec<Box<dyn CompetitorAgent>> {
    vec![Box::new(RawOneshotArm), Box::new(HermesArm)]
}

/// Grade an arm's answer with the SAME bar as Continuum's own cognition, so the
/// scoreboard is product-vs-product on identical criteria. A `test`-graded task
/// compiles + runs the extracted code through [`crate::cognition::gym_grader`] (the
/// language's own verdict); an `expect`-graded task checks the substring. A task with
/// neither is not gradeable as a mouth answer — fail LOUD, never a silent pass.
pub async fn grade_answer(task: &EvalTask, answer: &str) -> (bool, String) {
    if let Some(test) = task.test.as_deref() {
        let lang = task.lang.as_deref().unwrap_or("rust");
        crate::cognition::gym_grader::test_grade(answer, lang, test).await
    } else if !task.expect.is_empty() {
        let ok = answer.to_lowercase().contains(&task.expect.to_lowercase());
        let msg = if ok {
            format!("expected substring '{}' present", task.expect)
        } else {
            format!("missing expected substring '{}'", task.expect)
        };
        (ok, msg)
    } else {
        (
            false,
            "task has neither `test` nor `expect` — not gradeable as a mouth answer".to_string(),
        )
    }
}

/// Cheap completion-token estimate for an arm that only yields text (a subprocess whose
/// stdout carries no usage). Whitespace words ≈ tokens to within the classifier's needs
/// (its only threshold is "≤4 = a decline/wedge, not a real answer").
fn estimate_tokens(text: &str) -> u32 {
    text.split_whitespace().count() as u32
}

/// Minimal shape of an OpenAI-compatible `/v1/chat/completions` response — only the
/// fields the arm reads. Extra fields the server sends are ignored.
#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    #[serde(default)]
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatUsage {
    #[serde(default)]
    completion_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the "works with a default location" contract — a caller that
    // gives no endpoint gets the default; an explicit one passes through with its
    // trailing slash normalized (so `format!("{endpoint}/chat/completions")` is clean).
    #[test]
    fn resolve_endpoint_applies_default_and_normalizes() {
        assert_eq!(resolve_endpoint(None), DEFAULT_ENDPOINT);
        assert_eq!(
            resolve_endpoint(Some("http://127.0.0.1:9000/v1/")),
            "http://127.0.0.1:9000/v1"
        );
        assert_eq!(
            resolve_endpoint(Some("http://host:1/v1")),
            "http://host:1/v1"
        );
    }

    // what this catches: SolveRequest::new threads the default-location contract through
    // construction — no endpoint given → the default is baked into the request the arm
    // will actually hit.
    #[test]
    fn solve_request_defaults_endpoint() {
        let r = SolveRequest::new("do it", "qwen-coder-1.5b", None);
        assert_eq!(r.endpoint, DEFAULT_ENDPOINT);
        assert_eq!(r.model, "qwen-coder-1.5b");
        let r2 = SolveRequest::new("do it", "m", Some("http://x:2/v1/"));
        assert_eq!(r2.endpoint, "http://x:2/v1");
    }

    // what this catches: the Hermes one-shot wiring — `-z` immediately followed by the
    // prompt (TTY-free), and `--provider lmstudio` present so it routes to our LOCAL
    // endpoint, not Hermes's default Nous cloud. A regression here bricks every Hermes
    // task with a connection error or an interactive hang.
    #[test]
    fn hermes_args_are_oneshot_local_provider() {
        let args = hermes_args("qwen-coder-1.5b", "write two_sum");
        assert_eq!(args[0], "-z");
        assert_eq!(args[1], "write two_sum", "prompt must directly follow -z");
        let joined = args.join(" ");
        assert!(joined.contains("--provider lmstudio"), "must use the local provider: {joined}");
        assert!(joined.contains("-m qwen-coder-1.5b"), "must pass the served model: {joined}");
    }

    // what this catches: grade_answer REUSES the shared bar rather than a parallel one —
    // an expect-graded task passes iff the substring is present (case-insensitive), and a
    // task with neither test nor expect fails LOUD rather than silently passing.
    #[tokio::test]
    async fn grade_answer_expect_path_and_ungradeable_fail_loud() {
        let mut task = EvalTask {
            id: "t".into(),
            prompt: "p".into(),
            expect: "ANSWER-42".into(),
            ..Default::default()
        };
        let (ok, _) = grade_answer(&task, "the answer-42 is here").await;
        assert!(ok, "case-insensitive substring must match");
        let (ok, _) = grade_answer(&task, "nope").await;
        assert!(!ok, "missing substring must miss");
        task.expect = String::new();
        let (ok, msg) = grade_answer(&task, "anything").await;
        assert!(!ok && msg.contains("neither"), "ungradeable must fail loud: {msg}");
    }

    // what this catches: the raw one-shot arm is always available (it is just HTTP), so
    // it is the floor arm that never gets skipped for a missing dependency.
    #[test]
    fn raw_oneshot_is_always_available() {
        assert!(RawOneshotArm.available());
        assert_eq!(RawOneshotArm.name(), "raw-oneshot");
    }
}
