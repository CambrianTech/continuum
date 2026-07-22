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
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
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

// ── The Continuum native arm ────────────────────────────────────────────────────────

/// Drives Continuum's OWN cognition on a prompt to a spoken answer. The persona cycle
/// lives at the command layer (it holds the `WorkspaceCycle`), so the native arm is
/// wrapped as a caller-provided async closure — competitor.rs never imports the persona
/// internals, and the native path still scores on the SAME runner + grader + events as
/// every external arm. The closure typically forwards to [`crate::cognition::eval`]'s
/// per-task drive (`drive_to_settle` → `settled.spoken`).
pub type ContinuumSolver =
    Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = Result<SolveOutcome, String>> + Send>> + Send + Sync>;

/// Continuum's native cognition as a competitor arm — the "home" arm the external
/// harnesses are measured against. Always available (it is us); its endpoint is
/// Continuum's own serving, so the runner's opponent endpoint is ignored here.
pub struct ContinuumArm {
    solver: ContinuumSolver,
}

impl ContinuumArm {
    /// Wrap a native-cognition solver (built at the command layer with the live cycle).
    pub fn new(solver: ContinuumSolver) -> Self {
        Self { solver }
    }
}

#[async_trait]
impl CompetitorAgent for ContinuumArm {
    fn name(&self) -> &'static str {
        "continuum"
    }

    fn available(&self) -> bool {
        true
    }

    async fn solve(&self, req: &SolveRequest) -> Result<SolveOutcome, String> {
        (self.solver)(req.prompt.clone()).await
    }
}

// ── The runner: loop arm × task, grade identically, classify, emit events ────────────

/// A failure with ≤ this many output tokens is a DECLINE/wedge (a bare "PASS" is ~2),
/// not a real wrong answer — harness noise the classifier flags, never a capability
/// verdict ([[hermes-agent-is-a-runnable-benchmark-opponent-arm]]).
const DECLINE_TOKEN_MAX: u32 = 4;

/// Self-diagnosing verdict for one arm's cell, so harness noise is FLAGGED, never
/// reported as a capability result.
#[derive(Debug, Clone, PartialEq)]
pub enum ArmClass {
    /// Every failure is a real wrong answer (> [`DECLINE_TOKEN_MAX`] tokens) — trustworthy.
    Clean,
    /// Some tasks were declines/wedges/errors — capability is not what this cell measured.
    Suspect { noisy: usize },
    /// Nothing was measured (no tasks, or every task was a decline/error).
    Void { reason: String },
}

impl ArmClass {
    /// Short uppercase label for events/scoreboard (`CLEAN` / `SUSPECT` / `VOID`).
    pub fn label(&self) -> &'static str {
        match self {
            ArmClass::Clean => "CLEAN",
            ArmClass::Suspect { .. } => "SUSPECT",
            ArmClass::Void { .. } => "VOID",
        }
    }
}

/// One (arm, task) outcome.
#[derive(Debug, Clone)]
pub struct ArmTaskResult {
    pub task_id: String,
    pub ok: bool,
    pub output_tokens: u32,
    pub latency_ms: u64,
    /// Grader message on a graded task, or the solve error when `errored`.
    pub grade: String,
    /// The solve itself failed (network/exec) — an infra miss, not a graded wrong answer.
    pub errored: bool,
}

/// One arm's cell on the scoreboard.
#[derive(Debug, Clone)]
pub struct ArmScore {
    pub arm: String,
    pub score: usize,
    pub total: usize,
    pub class: ArmClass,
    pub results: Vec<ArmTaskResult>,
}

/// The product-vs-product scoreboard: every arm, same tasks, same grader.
#[derive(Debug, Clone)]
pub struct Scoreboard {
    pub model: String,
    pub endpoint: String,
    pub arms: Vec<ArmScore>,
    /// Arms skipped because unavailable — surfaced, never silently dropped.
    pub skipped: Vec<String>,
}

/// Classify a cell from its per-task ledger — the CLEAN/SUSPECT/VOID triage that keeps
/// harness noise from masquerading as a capability number. A task is "noisy" if its
/// solve errored, or it failed with ≤ [`DECLINE_TOKEN_MAX`] tokens (a decline/wedge).
fn classify(results: &[ArmTaskResult]) -> ArmClass {
    if results.is_empty() {
        return ArmClass::Void {
            reason: "no tasks graded".to_string(),
        };
    }
    let noisy = results
        .iter()
        .filter(|r| r.errored || (!r.ok && r.output_tokens <= DECLINE_TOKEN_MAX))
        .count();
    if noisy == results.len() {
        ArmClass::Void {
            reason: format!("all {noisy} tasks were declines/errors — nothing measured"),
        }
    } else if noisy > 0 {
        ArmClass::Suspect { noisy }
    } else {
        ArmClass::Clean
    }
}

/// Run the competition: every AVAILABLE arm solves every task against the SAME endpoint,
/// each answer graded by the SAME [`grade_answer`], each cell self-classified. Unavailable
/// arms are skipped (logged + a `benchmark:arm:skipped` event), never faked. Emits the
/// widget-ready event stream (`benchmark:arm:*`, the sibling shape of `eval:*`) and
/// returns the full [`Scoreboard`].
///
/// `endpoint` follows the default-location contract: `None` → [`DEFAULT_ENDPOINT`]; the
/// live command layer passes a freshly-provisioned dedicated opponent lane's `base_url`
/// ([[benchmark-needs-its-own-serving-lane]]).
pub async fn run_competition(
    model: &str,
    endpoint: Option<&str>,
    tasks: &[EvalTask],
    arms: Vec<Box<dyn CompetitorAgent>>,
) -> Scoreboard {
    let resolved = resolve_endpoint(endpoint);
    let total = tasks.len();
    let mut board = Scoreboard {
        model: model.to_string(),
        endpoint: resolved.clone(),
        arms: Vec::new(),
        skipped: Vec::new(),
    };

    for arm in arms {
        let name = arm.name();
        if !arm.available() {
            crate::probe!(class = "benchmark.arm", arm = name, "skipped: arm unavailable in this environment");
            emit_arm(
                "benchmark:arm:skipped",
                serde_json::json!({ "arm": name, "reason": "unavailable (CLI/dep not present)", "atMs": now_ms() }),
            );
            board.skipped.push(name.to_string());
            continue;
        }

        emit_arm(
            "benchmark:arm:start",
            serde_json::json!({ "arm": name, "model": model, "endpoint": resolved, "total": total, "atMs": now_ms() }),
        );

        let mut results: Vec<ArmTaskResult> = Vec::with_capacity(total);
        let mut pass = 0usize;
        for (i, task) in tasks.iter().enumerate() {
            let req = SolveRequest::new(task.prompt.clone(), model, Some(&resolved));
            let result = match arm.solve(&req).await {
                Ok(out) => {
                    let (ok, grade) = grade_answer(task, &out.answer).await;
                    if ok {
                        pass += 1;
                    }
                    ArmTaskResult {
                        task_id: task.id.clone(),
                        ok,
                        output_tokens: out.output_tokens,
                        latency_ms: out.latency_ms,
                        grade,
                        errored: false,
                    }
                }
                Err(e) => ArmTaskResult {
                    task_id: task.id.clone(),
                    ok: false,
                    output_tokens: 0,
                    latency_ms: 0,
                    grade: e,
                    errored: true,
                },
            };
            emit_arm(
                "benchmark:arm:progress",
                serde_json::json!({
                    "arm": name,
                    "task": result.task_id,
                    "ok": result.ok,
                    "errored": result.errored,
                    "outputTokens": result.output_tokens,
                    "latencyMs": result.latency_ms,
                    "done": i + 1,
                    "total": total,
                    "pass": pass,
                    "atMs": now_ms(),
                }),
            );
            results.push(result);
        }

        let class = classify(&results);
        crate::probe!(
            class = "benchmark.arm",
            arm = name,
            score = pass,
            total = total,
            verdict = class.label(),
            "arm complete"
        );
        emit_arm(
            "benchmark:arm:complete",
            serde_json::json!({
                "arm": name,
                "model": model,
                "score": pass,
                "total": total,
                "class": class.label(),
                "atMs": now_ms(),
            }),
        );
        board.arms.push(ArmScore {
            arm: name.to_string(),
            score: pass,
            total,
            class,
            results,
        });
    }

    board
}

/// Publish a `benchmark:arm:*` event when a bus is present. No bus (a unit test, a
/// headless one-off) → a silent no-op; the runner never depends on the bus.
fn emit_arm(event: &str, payload: serde_json::Value) {
    if let Some(bus) = crate::runtime::MessageBus::global() {
        bus.publish_async_only(event, payload);
    }
}

/// Millisecond wall clock for event timestamps, shared with the eval harness.
fn now_ms() -> u64 {
    crate::persona::trace::now_ms()
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

    // A canned arm for driving the runner deterministically — maps a task prompt to the
    // outcome it should return, so run_competition is testable without a network, a
    // subprocess, or rustc.
    struct FakeArm {
        name: &'static str,
        avail: bool,
        responses: std::collections::HashMap<String, Result<SolveOutcome, String>>,
    }

    #[async_trait]
    impl CompetitorAgent for FakeArm {
        fn name(&self) -> &'static str {
            self.name
        }
        fn available(&self) -> bool {
            self.avail
        }
        async fn solve(&self, req: &SolveRequest) -> Result<SolveOutcome, String> {
            self.responses
                .get(&req.prompt)
                .cloned()
                .unwrap_or_else(|| Err("no canned response".to_string()))
        }
    }

    fn expect_task(id: &str, prompt: &str, expect: &str) -> EvalTask {
        EvalTask {
            id: id.into(),
            prompt: prompt.into(),
            expect: expect.into(),
            ..Default::default()
        }
    }

    fn outcome(answer: &str, tokens: u32) -> Result<SolveOutcome, String> {
        Ok(SolveOutcome {
            answer: answer.into(),
            output_tokens: tokens,
            latency_ms: 1,
        })
    }

    // what this catches: the whole runner shape in one pass — every arm solves every task
    // against the same tasks and is graded by the SAME grade_answer; the score is the pass
    // count; an arm whose only failure is a ≤4-token decline is flagged SUSPECT (not a
    // capability number); an unavailable arm is recorded in `skipped`, never faked or
    // silently dropped. Regressions here are exactly the failures that made the old
    // shell-matrix untrustworthy.
    #[tokio::test]
    async fn run_competition_scores_grades_and_classifies() {
        let tasks = vec![
            expect_task("t1", "Q1", "alpha"),
            expect_task("t2", "Q2", "beta"),
        ];
        let mut clean = std::collections::HashMap::new();
        clean.insert("Q1".to_string(), outcome("the answer is alpha here", 20));
        clean.insert("Q2".to_string(), outcome("clearly beta today", 15));

        let mut suspect = std::collections::HashMap::new();
        suspect.insert("Q1".to_string(), outcome("alpha indeed", 12));
        suspect.insert("Q2".to_string(), outcome("PASS", 2)); // decline: ≤4 tokens, wrong

        let arms: Vec<Box<dyn CompetitorAgent>> = vec![
            Box::new(FakeArm { name: "clean-arm", avail: true, responses: clean }),
            Box::new(FakeArm { name: "suspect-arm", avail: true, responses: suspect }),
            Box::new(FakeArm { name: "absent-arm", avail: false, responses: Default::default() }),
        ];

        let board = run_competition("m", Some("http://x:1/v1"), &tasks, arms).await;

        assert_eq!(board.endpoint, "http://x:1/v1", "endpoint threads through");
        assert_eq!(board.skipped, vec!["absent-arm"], "unavailable arm is skipped, not faked");
        assert_eq!(board.arms.len(), 2, "only available arms score");

        let clean = board.arms.iter().find(|a| a.arm == "clean-arm").unwrap();
        assert_eq!(clean.score, 2);
        assert_eq!(clean.class, ArmClass::Clean);

        let suspect = board.arms.iter().find(|a| a.arm == "suspect-arm").unwrap();
        assert_eq!(suspect.score, 1, "one real pass, one decline");
        assert_eq!(suspect.class, ArmClass::Suspect { noisy: 1 });
    }

    // what this catches: an arm whose every solve ERRORS (endpoint down / arm broken) is
    // VOID — its 0/N must never read as "the model scored zero", which is the difference
    // between a red harness and a false capability verdict.
    #[tokio::test]
    async fn all_errors_is_void_not_a_zero_score() {
        let tasks = vec![expect_task("t1", "Q1", "x")];
        let arm: Vec<Box<dyn CompetitorAgent>> = vec![Box::new(FakeArm {
            name: "broken",
            avail: true,
            responses: std::collections::HashMap::new(), // no canned → every solve Err
        })];
        let board = run_competition("m", None, &tasks, arm).await;
        assert_eq!(board.endpoint, DEFAULT_ENDPOINT, "None endpoint → default location");
        let cell = &board.arms[0];
        assert_eq!(cell.score, 0);
        assert!(matches!(cell.class, ArmClass::Void { .. }), "all-errored cell is VOID: {:?}", cell.class);
    }

    // what this catches: ContinuumArm threads a caller-provided native-cognition solver
    // through the same trait, so Continuum scores on the same runner + grader as the
    // external arms — without competitor.rs importing the persona cycle.
    #[tokio::test]
    async fn continuum_arm_wraps_a_native_solver() {
        let solver: ContinuumSolver = Arc::new(|prompt: String| {
            Box::pin(async move {
                Ok(SolveOutcome {
                    answer: format!("cognition says: {prompt}"),
                    output_tokens: 7,
                    latency_ms: 3,
                })
            })
        });
        let arm = ContinuumArm::new(solver);
        assert_eq!(arm.name(), "continuum");
        assert!(arm.available());
        let out = arm
            .solve(&SolveRequest::new("solve x", "m", None))
            .await
            .unwrap();
        assert!(out.answer.contains("cognition says: solve x"));
    }
}
