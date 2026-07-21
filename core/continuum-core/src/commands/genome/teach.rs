//! `genome/teach` — the teacher-episode generator: grow the corpus that teaches
//! the self-verify-and-correct ENGINEERING REFLEX, not raw coding skill.
//!
//! ## Why this exists (cold-start)
//!
//! The genome loop closes mechanically (recorder → dataset → `mlx_lm.lora` → GGUF
//! → `:58057` page-in → `cognition/eval` A/B) but produced **inert** genes: the
//! lever was never raw skill, it's the reflex of *writing code → reading the real
//! compiler/test error → fixing → re-running → answering*. You cannot distill a
//! reflex that isn't in the data — of one persona's 1761 captured turns only 5 ever
//! used a tool. So we BOOTSTRAP it: a teacher model writes a solution, the gym
//! grader (the SAME `test_grade` the A/B evaluator uses) actually compiles and runs
//! it, the REAL error feeds back, and the teacher fixes — looping to green. The full
//! validated write→error→fix→pass trajectory becomes a multi-turn ShareGPT example.
//! Once the reflex exists in the genome, the persona's own successful corrections
//! start appearing in captures and the loop goes self-feeding.
//!
//! ## The grader guarantees corpus quality; the teacher only affects YIELD
//!
//! Every example that ships is test-validated: it COMPILES and its asserts PASS
//! (exit 0). A weak teacher solves fewer tasks (lower yield) but never injects an
//! incorrect "lesson" — `test_grade` is the gate. This is the sanctioned way the
//! genome loop fixes behavior: curate the LEARNING corpus by an objective scorer
//! ([[no-hardcoded-heuristics-to-steer-cognition]] forbids puppeting LIVE output;
//! curating training data by a validated scorer is explicitly allowed). The teacher
//! defaults to the locally-served model so this RUNS with no external dep; point
//! `teacher_model` at a stronger peer/gateway model for higher yield.
//!
//! ## Non-disruptive
//!
//! This generates a dataset on disk. It does NOT touch the living `:58057` lane's
//! served genome, fork a persona, or train anything — it produces the corpus a
//! later `genome/job-create` / native `mlx_lm.lora` run consumes. Procedure is never
//! the artifact: the reflex is LEARNED from these trajectories, never hardcoded as a
//! run-N-times loop in a class or prompt.
//!
//! Access: `Privileged` — it spends real inference compute and executes
//! model-generated code through the grader, same tier as `cognition/eval`.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;

use crate::ai::adapter::InferenceDevice;
use crate::ai::types::TextGenerationResponse;
use crate::ai::{ChatMessage, MessageContent, TextGenerationRequest};
use crate::cognition::eval::EvalTask;
use crate::cognition::gym_grader::test_grade;
use crate::cognition::inference_session::resolve_model;
use crate::inference::llama_server::{await_ready_serving, DEFAULT_SERVING_WAIT, PROVIDER_ID};
use crate::modules::ai_provider::global_registry;
use crate::modules::dataset::DatasetService;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// Default write→fix→pass task set (one `EvalTask` JSONL row each — needs `test`).
/// Authoring a harder battery = add lines, no recompile.
const DEFAULT_TEACH_SET: &str = "docs/genome/coder-write-eval.jsonl";

/// Default dataset name (subdirectory under the datasets root).
const DEFAULT_DATASET_NAME: &str = "coder-reflex-teacher";

/// How many fix attempts a single task gets before it's dropped as unsolved, when
/// the caller doesn't set `max_fix_iters`. The first generation + this many fixes.
/// `pub(crate)` so the [`curriculum`](super::curriculum) synthesizer reuses the same
/// default — one place decides how many fixes a lesson gets, command or loop.
pub(crate) const DEFAULT_MAX_FIX_ITERS: u32 = 4;

/// Teacher decoding temperature default — low, because we want correct code, and a
/// fix turn should converge on the error, not wander. `pub(crate)` for the same
/// single-source reason as [`DEFAULT_MAX_FIX_ITERS`].
pub(crate) const DEFAULT_TEMPERATURE: f32 = 0.2;

/// The teacher's standing instruction — frames it as an engineer who reads errors
/// and returns a complete corrected solution. This is the BEHAVIOR the trajectory
/// teaches, captured once as the system turn of every example.
const TEACHER_SYSTEM: &str = "You are an expert Rust engineer. Write correct, idiomatic Rust that \
    COMPILES and passes its tests. Return ONLY the item(s) the task asks for (functions/types) in a \
    single ```rust code block — a separate harness calls them, so do not write `fn main`. When you \
    are shown a compiler or test error, read it carefully and return the COMPLETE corrected \
    solution in a ```rust block.";

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/GenomeTeachParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct GenomeTeachParams {
    /// Inline tasks. When set, takes precedence over `teach_set`. Each task SHOULD
    /// carry a `test` — only test-validated trajectories become corpus.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tasks: Option<Vec<EvalTask>>,
    /// Path to a JSONL task set (one `EvalTask` per line). Defaults to the committed
    /// `docs/genome/coder-write-eval.jsonl` when neither this nor `tasks` is given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub teach_set: Option<String>,
    /// Model that writes + fixes. Omit to use the locally-served model (runs with no
    /// external dep); point at a stronger peer/gateway model for higher yield.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub teacher_model: Option<String>,
    /// Max fix attempts per task before it's dropped as unsolved. Default 4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub max_fix_iters: Option<u32>,
    /// Teacher decoding temperature. Default 0.2 (we want correct, convergent code).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub temperature: Option<f32>,
    /// Dataset name (subdirectory under the datasets root). Default
    /// `coder-reflex-teacher`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    /// Override the datasets root (default `~/.continuum/datasets`). The dataset is
    /// written to `<root>/<name>/{train,eval}.jsonl` + `manifest.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub output_dir: Option<String>,
    /// Fraction of validated examples placed in the train split. Default 0.8.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub split_ratio: Option<f64>,
    /// Fire-and-stream (#86): true runs the corpus-gen in the CORE and returns a `run_id`
    /// HANDLE immediately — the client never blocks for the many-minute run, the work
    /// survives disconnect, progress streams as events, and the terminal result lands in
    /// a run ledger polled by `genome/teach-status --run_id`. Default false (inline).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detach: Option<bool>,
    /// The run handle. Minted by the command when omitted; the detached ack AND the ledger
    /// row carry it, so the two halves of fire-and-poll join on one id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
}

/// Per-task outcome — so a low yield is diagnosable (which tasks the teacher never
/// got to green, and why), not a silent shortfall.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/GenomeTeachTaskOutcome.ts")]
#[serde(rename_all = "camelCase")]
pub struct GenomeTeachTaskOutcome {
    /// The task id (echoed for traceability).
    pub id: String,
    /// True iff the teacher reached a test-passing solution within the fix budget.
    pub solved: bool,
    /// How many generations it took (1 = first-try; >1 = needed self-correction).
    #[ts(type = "number")]
    pub attempts: u32,
    /// On failure, the last real grader message (compile error / panic / timeout).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema, Default)]
#[ts(export, export_to = "../../../protocol/typescript/genome/GenomeTeachResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct GenomeTeachResult {
    /// True = this is a fire-and-stream JOB HANDLE (#86), NOT a completed run: teach was
    /// spawned detached and its real result is in the run ledger (poll `genome/teach-status
    /// --run_id`), not in the fields below (which are defaulted on the ack).
    #[serde(default)]
    pub detached: bool,
    /// The run handle — present on a detached ack AND on the ledger row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
    /// The dataset name written.
    pub dataset: String,
    /// The teacher model used (resolved, so the trend row is attributable).
    pub teacher_model: String,
    /// Absolute path to the dataset directory.
    pub dataset_dir: String,
    /// Total tasks attempted.
    #[ts(type = "number")]
    pub tasks_total: usize,
    /// Tasks the teacher drove to a test-passing solution (became corpus).
    #[ts(type = "number")]
    pub tasks_solved: usize,
    /// Tasks dropped (no `test` to validate, or never reached green in budget).
    #[ts(type = "number")]
    pub tasks_dropped: usize,
    /// Of the solved tasks, how many needed at least one self-correction — the
    /// reflex examples (the whole point), distinct from first-try passes.
    #[ts(type = "number")]
    pub trajectories_with_correction: usize,
    /// Validated examples written (== tasks_solved).
    #[ts(type = "number")]
    pub examples: usize,
    /// Examples in the train split.
    #[ts(type = "number")]
    pub train_examples: usize,
    /// Examples in the eval split.
    #[ts(type = "number")]
    pub eval_examples: usize,
    /// Per-task outcomes, so a low yield names which tasks failed and why.
    pub outcomes: Vec<GenomeTeachTaskOutcome>,
}

/// Flatten a `ChatMessage`'s content to plain text for the ShareGPT row. We only
/// ever build `Text` content here; `Parts` are joined defensively (text parts only).
fn message_text(m: &ChatMessage) -> String {
    match &m.content {
        MessageContent::Text(s) => s.clone(),
        MessageContent::Parts(parts) => parts
            .iter()
            .filter_map(|p| match p {
                crate::ai::types::ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// Live teach progress — the queryable snapshot `genome/teach-status` returns, so a
/// long corpus-gen run is OBSERVABLE (poll `done/total/currentTask`) instead of a
/// black-box wait or an autopsy of CPU%. Mirrors eval's `EvalPassProgress` watch cell —
/// bus events are fire-and-forget (a UI subscribes), but a STATUS you can query is what
/// lets an operator or a poller see where a run actually is. Clean harness = you know
/// what's going on. [[self-test-via-command-feedback-surface-never-blind]]
#[derive(Debug, Clone, Serialize, TS, JsonSchema, Default)]
#[ts(export, export_to = "../../../protocol/typescript/genome/TeachProgress.ts")]
#[serde(rename_all = "camelCase")]
pub struct TeachProgress {
    /// `started` (denominator set, no work yet) | `task` (one graded) | `completed`.
    pub phase: String,
    /// Tasks graded so far.
    #[ts(type = "number")]
    pub done: usize,
    /// Total tasks in the set (the progress-bar denominator, known at `started`).
    #[ts(type = "number")]
    pub total: usize,
    /// Validated (test-passing) trajectories so far — the corpus yield.
    #[ts(type = "number")]
    pub solved: usize,
    /// The task just graded (None at `started`/`completed`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub current_task: Option<String>,
    /// Wall-clock stamp — a poller checks this for staleness on a long-dead run.
    #[ts(type = "number")]
    pub updated_at_ms: u64,
}

static TEACH_PROGRESS: std::sync::OnceLock<tokio::sync::watch::Sender<Option<TeachProgress>>> =
    std::sync::OnceLock::new();

fn teach_progress_tx() -> &'static tokio::sync::watch::Sender<Option<TeachProgress>> {
    TEACH_PROGRESS.get_or_init(|| tokio::sync::watch::channel(None).0)
}

/// Subscribe to live teach progress — the watch `genome/teach-status` reads.
pub fn subscribe_teach_progress() -> tokio::sync::watch::Receiver<Option<TeachProgress>> {
    teach_progress_tx().subscribe()
}

fn set_teach_progress(p: TeachProgress) {
    let _ = teach_progress_tx().send(Some(p));
}

/// The run ledger for a detached teach job — one JSON row per `run_id`, the DURABLE half of
/// fire-and-stream (#86). The watch cell + bus carry PROGRESS (push, live); this file carries
/// the terminal RESULT, cross-process and surviving the run, so `genome/teach-status --run_id`
/// resolves to complete+result (or a failed row) regardless of who's watching or when.
fn teach_ledger_path(run_id: &str) -> Option<std::path::PathBuf> {
    std::env::var("HOME").ok().map(|h| {
        std::path::PathBuf::from(h)
            .join(".continuum")
            .join("progress")
            .join("teach")
            .join(format!("{run_id}.json"))
    })
}

/// Write the terminal row for a detached run — success carries the full result, failure
/// carries the loud error keyed on the SAME run_id (a poller sees the failure, never waits
/// on a corpse — the eval `append_failed_ledger` lesson). [[fallbacks-are-illegal-fail-loud]]
fn write_teach_ledger(run_id: &str, result: Result<&GenomeTeachResult, String>) {
    let Some(path) = teach_ledger_path(run_id) else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let row = match result {
        Ok(r) => serde_json::json!({ "runId": run_id, "complete": true, "ok": true, "result": r }),
        Err(e) => serde_json::json!({ "runId": run_id, "complete": true, "ok": false, "error": e }),
    };
    let _ = std::fs::write(&path, serde_json::to_string_pretty(&row).unwrap_or_default());
}

fn read_teach_ledger(run_id: &str) -> Option<serde_json::Value> {
    let path = teach_ledger_path(run_id)?;
    let txt = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&txt).ok()
}

/// Emit ONE teach progress event to the message bus, so a UI / the operator can WATCH
/// corpus generation live instead of black-box-waiting for the final result (the whole
/// point of long jobs being observable — the same `MessageBus` seam `emit_eval_phase`
/// uses for `eval:phase`). Feedback is a first-class cross-modality dimension: this
/// long job streams its own progress. [[feedback-is-a-first-class-cross-modality-dimension-jtag-cu]]
fn emit_teach_progress(
    done: usize,
    total: usize,
    task_id: &str,
    solved: bool,
    solved_count: usize,
    with_correction: usize,
) {
    set_teach_progress(TeachProgress {
        phase: "task".to_string(),
        done,
        total,
        solved: solved_count,
        current_task: Some(task_id.to_string()),
        updated_at_ms: crate::persona::trace::now_ms(),
    });
    if let Some(bus) = crate::runtime::MessageBus::global() {
        bus.publish_async_only(
            "genome:teach:progress",
            serde_json::json!({
                "phase": "task",
                "done": done,
                "total": total,
                "task": task_id,
                "solved": solved,
                "withCorrection": with_correction,
                "atMs": crate::persona::trace::now_ms(),
            }),
        );
    }
    tracing::info!(
        target: "genome::teach",
        done, total, task = task_id, solved, with_correction,
        "teach task graded"
    );
}

/// A lifecycle MILESTONE event — `started` (carries `total`, the progress bar's
/// denominator, before any work) and `completed` (the terminal fill). Every milestone
/// emits an event so a UI can render a real progress bar: `started` sizes it, the
/// per-task `emit_teach_progress` increments it, `completed` closes it. Same seam as
/// the per-task events. [[feedback-is-a-first-class-cross-modality-dimension-jtag-cu]]
fn emit_teach_milestone(phase: &str, done: usize, total: usize, solved: usize) {
    set_teach_progress(TeachProgress {
        phase: phase.to_string(),
        done,
        total,
        solved,
        current_task: None,
        updated_at_ms: crate::persona::trace::now_ms(),
    });
    if let Some(bus) = crate::runtime::MessageBus::global() {
        bus.publish_async_only(
            "genome:teach:progress",
            serde_json::json!({
                "phase": phase,
                "done": done,
                "total": total,
                "solved": solved,
                "atMs": crate::persona::trace::now_ms(),
            }),
        );
    }
    tracing::info!(target: "genome::teach", phase, done, total, solved, "teach milestone");
}

/// Convert a validated trajectory (the full write→error→fix→pass turn sequence) into
/// the ShareGPT `{"messages":[{role,content},...]}` shape `dataset/*` + `mlx_lm.lora`
/// consume. Order is preserved — that ordering IS the lesson (task → attempt →
/// real error → correction → passing answer).
fn build_sharegpt(messages: &[ChatMessage]) -> Value {
    let msgs: Vec<Value> = messages
        .iter()
        .map(|m| json!({ "role": m.role, "content": message_text(m) }))
        .collect();
    json!({ "messages": msgs })
}

/// One teacher generation. Resolves the adapter the canonical way (no injected
/// state — mirrors `cognition/generate_response`), acquiring + dropping the registry
/// read guard within the call so it's never held across the multi-task loop.
async fn teacher_generate(
    model: &str,
    messages: Vec<ChatMessage>,
    temperature: f32,
) -> Result<String, CommandError> {
    let request = TextGenerationRequest {
        messages,
        system_prompt: None,
        model: Some(model.to_string()),
        provider: Some(PROVIDER_ID.to_string()),
        temperature: Some(temperature),
        // The model owns its length — no ceiling. A hard cap truncates code mid-fn.
        max_tokens: None,
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        frequency_penalty: None,
        repeat_last_n: None,
        stop_sequences: None,
        tools: None,
        tool_choice: None,
        response_format: None,
        active_adapters: None,
        request_id: None,
        user_id: None,
        room_id: None,
        purpose: Some("genome/teach".to_string()),
        persona_id: None,
    };

    let registry_arc = global_registry();
    let registry = registry_arc.read().await;
    let (_provider_id, adapter) = registry
        .select(Some(PROVIDER_ID), Some(model), InferenceDevice::Auto)
        .ok_or_else(|| {
            CommandError::Internal(format!("no adapter serves teacher model '{model}'"))
        })?;
    let response: TextGenerationResponse = adapter
        .generate_text(request)
        .await
        .map_err(CommandError::Internal)?;
    Ok(response.text)
}

/// The validated corpus a remediation pass produces: the ShareGPT examples (only
/// test-PASSING trajectories) plus a per-task outcome trail and how many needed a
/// correction. Returned by [`synthesize_remediation`] to whatever drives it — the
/// `genome/teach` command over a static set, or the self-improvement orchestrator
/// over a persona's own measured failures ([[attention-salience-selects-what-becomes-curriculum]]).
pub struct RemediationCorpus {
    /// ShareGPT `{"messages":[...]}` examples — one per solved task, the full
    /// write→error→fix→pass trajectory.
    pub examples: Vec<Value>,
    /// Per-task outcome (solved?, attempts, last real error) — the trail.
    pub outcomes: Vec<GenomeTeachTaskOutcome>,
    /// How many solved trajectories needed ≥1 correction (the self-verify reflex
    /// actually firing, not first-try luck).
    pub with_correction: usize,
}

/// **The curriculum synthesizer** (remediation mode). For each test-graded task, a
/// teacher model writes a solution, the gym grader compiles+runs it, the REAL error
/// feeds back, and it loops to green within `max_fix_iters` — only test-PASSING
/// trajectories become corpus. The write→error→fix→pass ordering IS the lesson (the
/// self-verify-and-correct reflex being taught).
///
/// This is the reusable core `genome/teach` runs over a static set AND the
/// self-improvement loop runs over a persona's own salience-selected failures
/// ([`crate::cognition::experience::salient_teach_set`]). It does NOT resolve the
/// teacher or write the dataset — the caller owns model resolution and packaging, so
/// this stays a pure task-set → validated-corpus transform.
///
/// Mirror-and-challenge: the teacher solves HER failed tasks (mirror — her real
/// fitness gap) and the fix-loop stretches past the first wrong attempt (challenge —
/// the corrected trajectory she has not yet lived). Measurement stays elsewhere
/// (`cognition/eval`, isolated) — this only PRODUCES curriculum, never grades her.
pub async fn synthesize_remediation(
    tasks: &[EvalTask],
    teacher_model: &str,
    temperature: f32,
    max_fix_iters: u32,
) -> Result<RemediationCorpus, CommandError> {
    let mut examples: Vec<Value> = Vec::new();
    let mut outcomes: Vec<GenomeTeachTaskOutcome> = Vec::new();
    let mut with_correction = 0usize;

    // WAIT for the served teacher model to be READY before the first generation. The
    // teacher runs on the local serving lane; if teach launches while serving is
    // relaunching that lane (a genome page-in, a window grow-back), the first
    // `generate_text` hits a not-ready lane and — with no readiness gate and no
    // timeout — HANGS FOREVER, parking the whole job at 0% with no dataset (glass-boxed
    // 2026-07-21). This is the same race the eval lane fixed; the teacher path needs
    // the same discipline. A timeout (below) still recovers if the lane wedges mid-run.
    if await_ready_serving(DEFAULT_SERVING_WAIT).await.is_none() {
        return Err(CommandError::Internal(
            "no served model became ready within the serving-wait budget — cannot run the teacher. \
             Bring up serving (ai/inference/serve) before genome/teach."
                .to_string(),
        ));
    }

    // MILESTONE: started — carries the denominator so a progress bar can size itself
    // before the first (slow) generation.
    emit_teach_milestone("started", 0, tasks.len(), 0);

    for task in tasks {
        // Only test-graded tasks can be validated → become corpus. A task with no
        // `test` is dropped with a named reason, never silently passed.
        let Some(test) = task.test.as_deref() else {
            outcomes.push(GenomeTeachTaskOutcome {
                id: task.id.clone(),
                solved: false,
                attempts: 0,
                last_error: Some("task has no `test` — cannot validate, dropped".into()),
            });
            continue;
        };
        let lang = task.lang.as_deref().unwrap_or("rust");

        // The trajectory we build turn-by-turn; on green it becomes the example.
        let mut trajectory = vec![
            ChatMessage::text("system", TEACHER_SYSTEM),
            ChatMessage::text("user", &task.prompt),
        ];

        let mut attempts = 0u32;
        let mut last_error: Option<String> = None;
        let mut solved = false;

        // First generation + up to `max_fix_iters` corrections. Readiness is an EVENT
        // (await_ready_serving above waits on the serving watch — a push, not a poll), so
        // the generation runs against a lane that IS up; a dead lane surfaces as an adapter
        // error (a push from the transport), not an indefinite hang. No wall-clock timeout
        // — a timeout is a guess about health, and guessing is the smell we're removing.
        // [[command-async-shape-prefer-stream-never-block]]
        for _ in 0..=max_fix_iters {
            let answer = match teacher_generate(teacher_model, trajectory.clone(), temperature).await {
                Ok(a) => a,
                Err(e) => {
                    last_error = Some(format!("teacher generation failed: {e}"));
                    break;
                }
            };
            attempts += 1;
            trajectory.push(ChatMessage::text("assistant", &answer));

            let (passed, grade) = test_grade(&answer, lang, test).await;
            if passed {
                solved = true;
                break;
            }
            last_error = Some(grade.clone());
            // Feed the REAL error back as the next turn — this is the reflex being
            // taught: read the actual compiler/test output, then correct.
            trajectory.push(ChatMessage::text(
                "user",
                format!(
                    "Your solution failed:\n{grade}\n\nRead the error and return the COMPLETE \
                     corrected solution in a ```rust block."
                ),
            ));
        }

        if solved {
            if attempts > 1 {
                with_correction += 1;
            }
            examples.push(build_sharegpt(&trajectory));
        }
        outcomes.push(GenomeTeachTaskOutcome {
            id: task.id.clone(),
            solved,
            attempts,
            last_error: if solved { None } else { last_error },
        });
        // Stream progress so the run is watchable live (events, not black-box wait).
        emit_teach_progress(outcomes.len(), tasks.len(), &task.id, solved, examples.len(), with_correction);
    }

    // MILESTONE: completed — the terminal fill, so the bar closes even on a 0-yield run.
    emit_teach_milestone("completed", outcomes.len(), tasks.len(), examples.len());

    Ok(RemediationCorpus {
        examples,
        outcomes,
        with_correction,
    })
}

/// Stateless — self-registers onto the ONE registry. Holds no module state; resolves
/// inference + dataset packaging through their global/associated seams.
#[derive(Default)]
pub struct GenomeTeach;

#[async_trait]
impl ActionCommand for GenomeTeach {
    const NAME: &'static str = "genome/teach";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Generate a test-VALIDATED write→error→fix→pass training corpus that teaches the \
         self-verify-and-correct engineering reflex. A teacher model writes Rust, the gym grader \
         compiles+runs it, the REAL error feeds back, and it loops to green — only test-passing \
         trajectories become multi-turn ShareGPT examples. Non-disruptive: writes a dataset, never \
         touches the live serving lane. Feed the dataset to genome/job-create to forge the gene.";
    type Params = GenomeTeachParams;
    type Output = GenomeTeachResult;

    async fn run(&self, _ctx: &Ctx, p: GenomeTeachParams) -> Result<GenomeTeachResult, CommandError> {
        // Fire-and-stream (#86): `detach` runs the many-minute corpus-gen IN THE CORE and
        // returns a run_id HANDLE immediately — never blocking the client, surviving its
        // disconnect. Progress streams as events (genome:teach:progress); the terminal
        // result lands in the run ledger, polled by `genome/teach-status --run_id`. This is
        // the pattern for every long run — mirrors cognition/eval's detach exactly.
        // [[command-async-shape-prefer-stream-never-block]]
        if p.detach.unwrap_or(false) {
            let run_id = p
                .run_id
                .clone()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let mut inner = p.clone();
            inner.detach = Some(false);
            inner.run_id = Some(run_id.clone());
            let ledger_run = run_id.clone();
            tokio::spawn(async move {
                let res = GenomeTeach::run_teach(inner).await;
                write_teach_ledger(&ledger_run, res.as_ref().map_err(|e| e.to_string()));
                match res {
                    Ok(r) => tracing::info!(
                        run_id = %ledger_run, solved = r.tasks_solved, total = r.tasks_total,
                        "genome/teach detached run complete — result in run ledger"
                    ),
                    Err(e) => tracing::error!(run_id = %ledger_run, error = %e, "genome/teach detached run failed"),
                }
            });
            return Ok(GenomeTeachResult {
                detached: true,
                run_id: Some(run_id),
                ..Default::default()
            });
        }
        GenomeTeach::run_teach(p).await
    }
}

impl GenomeTeach {
    /// The corpus-gen body — ctx-free so it runs inline OR from a detached `tokio::spawn`
    /// (#86 fire-and-stream). Owns its params; reaches serving + dataset packaging via their
    /// global seams, needing neither `self` nor `ctx`.
    async fn run_teach(p: GenomeTeachParams) -> Result<GenomeTeachResult, CommandError> {
        // Task source: inline → teach_set JSONL → committed default. A missing
        // explicit path is a loud error (don't silently teach an empty set).
        let tasks: Vec<EvalTask> = if let Some(inline) = p.tasks {
            inline
        } else {
            let path = p.teach_set.as_deref().unwrap_or(DEFAULT_TEACH_SET);
            let text = std::fs::read_to_string(path).map_err(|e| {
                CommandError::Invalid(format!("teach_set '{path}' could not be read: {e}"))
            })?;
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .filter_map(|l| serde_json::from_str::<EvalTask>(l).ok())
                .collect()
        };
        if tasks.is_empty() {
            return Err(CommandError::Invalid(
                "no tasks to teach (inline `tasks` empty and/or teach_set had no valid rows)".into(),
            ));
        }

        // Resolve the teacher: explicit → the locally-served model. Fail loud if
        // nothing serves (no silent skip).
        let teacher_model = match p.teacher_model {
            Some(m) => m,
            None => resolve_model(None).await.map_err(|e| {
                CommandError::Internal(format!("teacher model resolve failed: {e:?}"))
            })?,
        };
        let temperature = p.temperature.unwrap_or(DEFAULT_TEMPERATURE);
        let max_fix_iters = p.max_fix_iters.unwrap_or(DEFAULT_MAX_FIX_ITERS);

        // The synthesis itself — shared with the self-improvement orchestrator.
        let RemediationCorpus {
            examples,
            outcomes,
            with_correction,
        } = synthesize_remediation(&tasks, &teacher_model, temperature, max_fix_iters).await?;

        if examples.is_empty() {
            return Err(CommandError::Internal(format!(
                "teacher solved 0 of {} tasks within {max_fix_iters} fixes — no validated corpus to \
                 write (try a stronger teacher_model or a higher max_fix_iters)",
                tasks.len()
            )));
        }

        // Package via the SAME writer the dataset/* verbs use — one train/eval/manifest
        // shape, never a parallel emitter. Dataset goes to `<root>/<name>/`.
        let name = p.name.unwrap_or_else(|| DEFAULT_DATASET_NAME.to_string());
        let split_ratio = p.split_ratio.unwrap_or(0.8);
        let root = match p.output_dir {
            Some(d) => std::path::PathBuf::from(d),
            None => {
                let home = std::env::var("HOME").map_err(|_| {
                    CommandError::Internal("HOME unset — cannot resolve datasets root".into())
                })?;
                std::path::PathBuf::from(home).join(".continuum").join("datasets")
            }
        };
        let dataset_dir = root.join(&name);
        let manifest =
            DatasetService::split_and_write(&name, &dataset_dir, &examples, split_ratio, None)
                .map_err(CommandError::Internal)?;

        let tasks_solved = examples.len();
        Ok(GenomeTeachResult {
            detached: false,
            run_id: p.run_id.clone(),
            dataset: name,
            teacher_model,
            dataset_dir: dataset_dir.display().to_string(),
            tasks_total: tasks.len(),
            tasks_solved,
            tasks_dropped: tasks.len() - tasks_solved,
            trajectories_with_correction: with_correction,
            examples: tasks_solved,
            train_examples: manifest.train_examples,
            eval_examples: manifest.eval_examples,
            outcomes,
        })
    }
}

// Stateless → self-register onto the ONE registry (descriptor + runtime object).
crate::register_stateless_command!(GenomeTeach);

/// `genome/teach-status` — the poll half of a long corpus-gen run. `genome/teach` runs
/// for many minutes (write→grade→fix over a whole task set); this returns the LIVE
/// `{done, total, currentTask, solved}` snapshot so an operator, a poller, or a UI
/// progress bar can SEE where it is — instead of guessing from CPU%. Read-only, ai-safe,
/// no params: the substrate serializes teach runs, so there's one live board to read.
/// The queryable companion to the fire-and-forget `genome:teach:progress` bus events —
/// a status you can ask beats a light you can only hope someone's watching.
/// [[self-test-via-command-feedback-surface-never-blind]]
#[derive(Default)]
pub struct GenomeTeachStatus;

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema, Default)]
#[ts(export, export_to = "../../../protocol/typescript/genome/GenomeTeachStatusParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct GenomeTeachStatusParams {
    /// The detached run's handle. With it, the terminal RESULT resolves from the run ledger
    /// (complete + result/error). Omit for LIVE progress only (the "how's it going" poll).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS, JsonSchema, Default)]
#[ts(export, export_to = "../../../protocol/typescript/genome/GenomeTeachStatusResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct GenomeTeachStatusResult {
    /// True once the run's ledger row exists (the detached run finished — check
    /// `result.ok`). Always false when polled without a run_id (live-progress only).
    pub complete: bool,
    /// The terminal ledger row `{ok, result|error}` when complete, else null.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub result: Option<serde_json::Value>,
    /// Live progress of the currently-running teach — the mid-run scoreboard a progress
    /// bar renders (done/total/currentTask/solved). Null until the first milestone fires;
    /// check `updatedAtMs` for staleness on a long-finished run.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub progress: Option<TeachProgress>,
}

#[async_trait]
impl ActionCommand for GenomeTeachStatus {
    const NAME: &'static str = "genome/teach-status";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Poll a detached genome/teach by run_id: {complete, result} from the run ledger + LIVE \
         {done, total, currentTask, solved} progress. The observable half of fire-and-stream — a \
         UI renders it as a progress bar, a poller knows alive-vs-done, without holding a \
         connection open across the many-minute run.";
    type Params = GenomeTeachStatusParams;
    type Output = GenomeTeachStatusResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: GenomeTeachStatusParams,
    ) -> Result<GenomeTeachStatusResult, CommandError> {
        // Live progress rides on EVERY poll (with or without a run_id).
        let progress = subscribe_teach_progress().borrow().clone();
        let row = p.run_id.as_deref().and_then(read_teach_ledger);
        Ok(GenomeTeachStatusResult {
            complete: row.is_some(),
            result: row,
            progress,
        })
    }
}

crate::register_stateless_command!(GenomeTeachStatus);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a validated trajectory flattens to the ShareGPT shape
    // mlx_lm.lora/dataset consume — role+content per turn, ORDER preserved (the
    // write→error→fix→pass ordering IS the lesson). Drift here silently corrupts
    // every example the teacher emits.
    #[test]
    fn build_sharegpt_preserves_role_content_and_order() {
        let traj = vec![
            ChatMessage::text("system", "be an engineer"),
            ChatMessage::text("user", "write add"),
            ChatMessage::text("assistant", "```rust\nfn add(){}\n```"),
            ChatMessage::text("user", "Your solution failed: compile error"),
            ChatMessage::text("assistant", "```rust\nfn add(a:i32,b:i32)->i32{a+b}\n```"),
        ];
        let v = build_sharegpt(&traj);
        let msgs = v["messages"].as_array().expect("messages array");
        assert_eq!(msgs.len(), 5);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[1]["content"], "write add");
        assert_eq!(msgs[3]["role"], "user");
        assert_eq!(msgs[4]["role"], "assistant");
        assert!(msgs[4]["content"].as_str().unwrap().contains("a+b"));
    }

    // what this catches: the command's wire name mirrors its file path
    // (commands/genome/teach.rs → "genome/teach"). The name keys cu, the persona
    // tool surface, and the grid; drift breaks "file tree IS the namespace".
    #[test]
    fn teach_command_name_mirrors_path() {
        assert_eq!(GenomeTeach::NAME, "genome/teach");
        assert_eq!(GenomeTeach::ACCESS, AccessLevel::Privileged);
    }
}
