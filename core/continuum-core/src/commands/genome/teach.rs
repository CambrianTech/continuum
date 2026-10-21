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
//! ## Default dataset-only behavior
//!
//! By default this generates a dataset on disk. Explicit `training` opt-in
//! persists a candidate before submitting through the existing training trigger;
//! completed jobs follow its automatic evaluation/adoption policy. Procedure is never
//! the artifact: the reflex is LEARNED from these trajectories, never hardcoded as a
//! run-N-times loop in a class or prompt.
//!
//! Access: `Privileged` — it spends real inference compute and executes
//! model-generated code through the grader, same tier as `cognition/eval`.

use continuum_base::time::EpochMs;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;

use crate::ai::types::TextGenerationResponse;
use crate::ai::{ChatMessage, MessageContent, TextGenerationRequest};
use crate::cognition::eval::EvalTask;
use crate::cognition::gym_grader::test_grade;
use crate::cognition::inference_session::resolve_model;
use crate::cognition::provenance::{GenerationOutcome, GenerationReceipt};
use crate::inference::llama_server::{await_ready_serving, DEFAULT_SERVING_WAIT, PROVIDER_ID};
use crate::modules::dataset::DatasetService;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

mod bridge;
pub use bridge::{TeachTrainingAction, TeachTrainingResult};

/// Default write→fix→pass task set (one `EvalTask` JSONL row each — needs `test`), by its
/// gym name: it resolves through `cognition::gym::resolve_gym`, which reads a file on disk
/// first (authoring a harder battery = add lines and pass its path, no recompile), then the
/// fetched cache, then the copy embedded in the binary. It was a repo-relative path, which a
/// running core (whose cwd is not the repo) could not read: the coursework smoke on IntelMac,
/// 2026-10-07, failed at its first step with "No such file or directory".
const DEFAULT_TEACH_SET: &str = "coder-write-eval.jsonl";

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
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/GenomeTeachParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GenomeTeachParams {
    /// Explicit training opt-in. Omit to write a dataset only. Completed training
    /// follows the existing automatic evaluation/adoption policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub training: Option<TeachTrainingAction>,
    /// Explicitly allow the serving owner to checkpoint and temporarily retire
    /// its current local model when this teacher cannot coexist. Default false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub exclusive_teacher: Option<bool>,
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
    /// A citizen's peer id (full UUID or ≥4-char hex prefix): teach from HER LIVED
    /// FAILURES instead of a static set (#319 — the curriculum drain). Loads her
    /// durable experience stream (`citizens/peers/<id>/experience.jsonl`, written by
    /// `benchmark/grade`/`swe-grade`), keeps only the LATEST record per task (a later
    /// PASS retires the failure — the lesson was learned), and selects the salient,
    /// test-graded failures as the teach set. Precedence: `tasks` > this > `teach_set`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from_experience: Option<String>,
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
    #[ts(optional)]
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/GenomeTeachTaskOutcome.ts"
)]
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/GenomeTeachResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GenomeTeachResult {
    /// Candidate identity and destination acceptance/dispatch, never a learning claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub training: Option<TeachTrainingResult>,
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
    /// Resolved model requested for teaching. Actual served model/provider/request
    /// identities are retained in each dataset row's teacherGenerations metadata.
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


/// Convert a validated trajectory (the full write→error→fix→pass turn sequence) into
/// the ShareGPT `{"messages":[{role,content},...]}` shape `dataset/*` + `mlx_lm.lora`
/// consume. Order is preserved — that ordering IS the lesson (task → attempt →
/// real error → correction → passing answer).
fn build_sharegpt(messages: &[ChatMessage], receipts: &[GenerationReceipt]) -> Value {
    let msgs: Vec<Value> = messages
        .iter()
        .map(|m| json!({ "role": m.role, "content": message_text(m) }))
        .collect();
    // The existing dataset writer preserves row metadata. Receipts describe the
    // actual teacher calls, not an inferred student outcome or training acceptance.
    json!({ "messages": msgs, "metadata": { "teacherGenerations": receipts } })
}

struct TeacherGeneration {
    text: String,
    receipt: GenerationReceipt,
}

/// Generate only through the acquired eval-lane adapter. The eval owner may share
/// resident weights; an acquisition refusal must never select a different live adapter.
async fn teacher_generate(
    adapter: &std::sync::Arc<dyn crate::ai::adapter::AIProviderAdapter>,
    model: &str,
    messages: Vec<ChatMessage>,
    temperature: f32,
) -> Result<TeacherGeneration, CommandError> {
    let request_id = uuid::Uuid::new_v4().to_string();
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
        request_id: Some(request_id.clone()),
        user_id: None,
        room_id: None,
        purpose: Some("genome/teach".to_string()),
        persona_id: None,
        turn_bound: None,
    };

    let response: TextGenerationResponse = adapter
        .generate_text(request)
        .await
        .map_err(CommandError::Internal)?;
    let receipt = GenerationReceipt::from_response(request_id, &response);
    if let GenerationOutcome::Faulted { detail, .. } = &receipt.outcome {
        // An Ok transport response can still report failed generation. Its partial
        // text must not reach grading or become an apparently valid example.
        return Err(CommandError::Internal(format!(
            "teacher generation faulted: {detail}"
        )));
    }
    Ok(TeacherGeneration {
        text: response.text,
        receipt,
    })
}

/// Preserve the serving-readiness prerequisite and the eval owner's lane policy.
/// Successful acquisition may share resident weights; refusal ends synthesis.
async fn acquire_teacher_lane(
    model: &str,
) -> Result<crate::cognition::eval::EvalLane, CommandError> {
    if await_ready_serving(DEFAULT_SERVING_WAIT).await.is_none() {
        return Err(CommandError::Internal(
            "no served model became ready within the serving-wait budget -- cannot run the teacher. \
             Bring up serving (ai/inference/serve) before genome/teach."
                .to_string(),
        ));
    }
    crate::cognition::eval::spawn_base_eval_lane(model).await
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
    synthesize_remediation_with_lane(
        tasks,
        teacher_model,
        temperature,
        max_fix_iters,
        acquire_teacher_lane(teacher_model),
    )
    .await
}

// The lane future keeps acquisition and the generation loop on the same path in tests.
async fn synthesize_remediation_with_lane(
    tasks: &[EvalTask],
    teacher_model: &str,
    temperature: f32,
    max_fix_iters: u32,
    lane: impl std::future::Future<Output = Result<crate::cognition::eval::EvalLane, CommandError>>,
) -> Result<RemediationCorpus, CommandError> {
    // Retain the owner for the entire batch, including grading between generations.
    let teacher_lane = lane.await?;
    synthesize_remediation_with_adapter(
        tasks,
        teacher_model,
        temperature,
        max_fix_iters,
        &teacher_lane.adapter,
        None,
    )
    .await
}

/// The existing generation/grading loop borrows its adapter for the entire batch.
/// The serving owner can keep a private borrower and its cleanup obligation outside
/// this future; only the validated corpus leaves that owner.
pub(crate) async fn synthesize_remediation_with_adapter(
    tasks: &[EvalTask],
    teacher_model: &str,
    temperature: f32,
    max_fix_iters: u32,
    adapter: &std::sync::Arc<dyn crate::ai::adapter::AIProviderAdapter>,
    cancellation: Option<&tokio::sync::watch::Receiver<bool>>,
) -> Result<RemediationCorpus, CommandError> {
    let mut examples: Vec<Value> = Vec::new();
    let mut outcomes: Vec<GenomeTeachTaskOutcome> = Vec::new();
    let mut with_correction = 0usize;

    // MILESTONE: started — carries the denominator so a progress bar can size itself
    // before the first (slow) generation.
    emit_teach_milestone("started", 0, tasks.len(), 0);

    for task in tasks {
        if cancellation.is_some_and(|cancel| *cancel.borrow()) {
            return Err(CommandError::Internal(
                "teacher batch cancelled between tasks".into(),
            ));
        }
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

        let mut receipts = Vec::new();
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
            if cancellation.is_some_and(|cancel| *cancel.borrow()) {
                return Err(CommandError::Internal(
                    "teacher batch cancelled between attempts".into(),
                ));
            }
            let answer =
                match teacher_generate(adapter, teacher_model, trajectory.clone(), temperature)
                    .await
                {
                    Ok(a) => a,
                    Err(e) => {
                        last_error = Some(format!("teacher generation failed: {e}"));
                        break;
                    }
                };
            receipts.push(answer.receipt);
            let answer = answer.text;
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
            examples.push(build_sharegpt(&trajectory, &receipts));
        }
        outcomes.push(GenomeTeachTaskOutcome {
            id: task.id.clone(),
            solved,
            attempts,
            last_error: if solved { None } else { last_error },
        });
        // Stream progress so the run is watchable live (events, not black-box wait).
        emit_teach_progress(
            outcomes.len(),
            tasks.len(),
            &task.id,
            solved,
            examples.len(),
            with_correction,
        );
    }

    // MILESTONE: completed — the terminal fill, so the bar closes even on a 0-yield run.
    emit_teach_milestone("completed", outcomes.len(), tasks.len(), examples.len());

    Ok(RemediationCorpus {
        examples,
        outcomes,
        with_correction,
    })
}

/// The teacher's standing instruction for LIVED expansion — produce a strong, complete
/// answer to a real question the persona faced but stalled on. Unlike [`TEACHER_SYSTEM`]
/// (correct CODE, grader-gated), this is a general expert answer with NO objective grader;
/// the whole-being benchmark-lift gate (#59) validates the CONSOLIDATION, never a single
/// trajectory. The system turn shapes the teacher's GENERATION but is deliberately NOT
/// trained in (see [`synthesize_lived_expansion`]) — the being learns to answer the class
/// of question without needing this scaffold.
const LIVED_TEACHER_SYSTEM: &str = "You are a thoughtful expert. A teammate was asked the \
    following in a live conversation and their attempt stalled before they finished. Give a single, \
    complete, self-contained answer — clear and correct, with no meta-commentary about the attempt.";

/// **Lived-axis expansion — the LLM-teacher path** that [`super::curriculum::expansion_examples`]
/// deliberately deferred. A salient LIVED turn is untestable (no grader) AND a failure
/// (non-convergence / infra fault), so it can be neither remediated (no `test` to validate
/// a correction) nor echoed (training on her stalled answer would teach the failure). The
/// honest resolution: a teacher RE-ANSWERS the stimulus she faced, and that strong answer —
/// not her stalled one — becomes the SFT pair. Safe on untestable material for the same
/// reason the received axis is: validation is per-CONSOLIDATION by whole-being benchmark
/// lift (#59), never per-trajectory ([[lived-and-eval-experience-are-one-stream-one-being]]).
///
/// Uses the same acquired eval lane as [`synthesize_remediation`], including its
/// refusal semantics, but has NO grader loop: ONE teacher
/// generation per stimulus. Two honesty guards: an empty teacher answer is dropped (never
/// ship a blank lesson), and the teacher's system turn shapes generation but is NOT in the
/// example — the SFT pair is the bare `{question → answer}`, so the being learns the class
/// of question, not a scaffolded reply. Empty stimuli in → empty out (no lane spin-up).
pub async fn synthesize_lived_expansion(
    stimuli: &[String],
    teacher_model: &str,
    temperature: f32,
) -> Result<Vec<Value>, CommandError> {
    synthesize_lived_expansion_with_lane(
        stimuli,
        teacher_model,
        temperature,
        acquire_teacher_lane(teacher_model),
    )
    .await
}

async fn synthesize_lived_expansion_with_lane(
    stimuli: &[String],
    teacher_model: &str,
    temperature: f32,
    lane: impl std::future::Future<Output = Result<crate::cognition::eval::EvalLane, CommandError>>,
) -> Result<Vec<Value>, CommandError> {
    // Trim + drop blanks up front: nothing to answer, and it decides whether we even need
    // a lane. Empty in → empty out is a legitimate outcome (no fitness gap), never a fault.
    let stimuli: Vec<&str> = stimuli
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if stimuli.is_empty() {
        return Ok(Vec::new());
    }

    let teacher_lane = lane.await?;

    let mut examples: Vec<Value> = Vec::new();
    for stimulus in stimuli {
        // The system turn steers the GENERATION only; it is not part of the training pair.
        let messages = vec![
            ChatMessage::text("system", LIVED_TEACHER_SYSTEM),
            ChatMessage::text("user", stimulus),
        ];
        let answer =
            match teacher_generate(&teacher_lane.adapter, teacher_model, messages, temperature)
                .await
            {
                Ok(a) => a,
                Err(e) => {
                    // Fail-loud on the ITEM, resilient on the BATCH — one stimulus failing
                    // must not abort the whole consolidation (same spirit as remediation
                    // breaking one task without killing the run).
                    tracing::warn!(
                        target: "genome::teach",
                        error = %e,
                        "lived-expansion teacher generation failed for one stimulus — skipped"
                    );
                    continue;
                }
            };
        if answer.text.trim().is_empty() {
            continue; // never ship a blank lesson
        }
        // The bare {stimulus → answer} pair — the teacher's scaffold system turn is dropped.
        examples.push(json!({
            "messages": [
                { "role": "user", "content": stimulus },
                { "role": "assistant", "content": answer.text.trim() },
            ],
            "metadata": { "teacherGenerations": [answer.receipt] }
        }));
    }
    Ok(examples)
}

/// GenomeModule owns the executor needed by explicit training submission.
/// Dataset-only teaching does not require that executor.
#[derive(Clone)]
pub struct GenomeTeach {
    pub(crate) serving: std::sync::Arc<
        crate::runtime::LateBound<crate::modules::serving_daemon::ServingDaemonModule>,
    >,
    pub(crate) executor: std::sync::Arc<crate::runtime::LateBound<crate::runtime::CommandExecutor>>,
}

#[async_trait]
impl ActionCommand for GenomeTeach {
    const NAME: &'static str = "genome/teach";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Generate a test-VALIDATED write→error→fix→pass training corpus that teaches the \
         self-verify-and-correct engineering reflex. A teacher model writes Rust, the gym grader \
         compiles+runs it, the REAL error feeds back, and it loops to green — only test-passing \
         trajectories become multi-turn ShareGPT examples. By default writes a dataset only. \
         Explicit training prepares or resumes an immutable candidate and submits it through \
         the existing training/evaluation/adoption pipeline.";
    type Params = GenomeTeachParams;
    type Output = GenomeTeachResult;

    async fn run(
        &self,
        ctx: &Ctx,
        p: GenomeTeachParams,
    ) -> Result<GenomeTeachResult, CommandError> {
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
            let command = self.clone();
            let ctx = ctx.clone();
            tokio::spawn(async move {
                let res = command.run_owned(&ctx, inner).await;
                write_teach_ledger(&ledger_run, res.as_ref().map_err(|e| e.to_string()));
                match res {
                    Ok(r) => tracing::info!(
                        run_id = %ledger_run, solved = r.tasks_solved, total = r.tasks_total,
                        "genome/teach detached run complete — result in run ledger"
                    ),
                    Err(e) => {
                        tracing::error!(run_id = %ledger_run, error = %e, "genome/teach detached run failed")
                    }
                }
            });
            return Ok(GenomeTeachResult {
                detached: true,
                run_id: Some(run_id),
                ..Default::default()
            });
        }
        self.run_owned(ctx, p).await
    }
}

impl GenomeTeach {
    /// Shared corpus generation. Optional preparation owns the reserved output and
    /// provenance; the default path only writes the requested dataset.
    async fn run_teach(
        &self,
        p: GenomeTeachParams,
        mut candidate: Option<bridge::Preparation>,
    ) -> Result<GenomeTeachResult, CommandError> {
        // Bind this pass's run_id so the emit helpers write LIVE progress to its ledger
        // (cross-process, so `teach-status --run_id` and any widget see the bar advance).
        set_current_teach_run(p.run_id.clone());
        let source_params = p.clone();
        let tasks = tokio::task::spawn_blocking(move || select_teach_tasks(source_params))
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))??;

        if let Some(preparation) = candidate.take() {
            candidate = Some(preparation.bind_source(tasks.clone()).await?);
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
        } = if p.exclusive_teacher.unwrap_or(false) {
            let serving = self.serving.cloned().ok_or_else(|| {
                CommandError::Internal("teacher serving owner is not installed".into())
            })?;
            serving
                .run_teacher_batch(
                    crate::modules::serving_daemon::academy_batch::TeacherBatchRequest {
                        tasks: tasks.clone(),
                        teacher_model: teacher_model.clone(),
                        temperature,
                        max_fix_iters,
                    },
                )
                .await?
        } else {
            synthesize_remediation(&tasks, &teacher_model, temperature, max_fix_iters).await?
        };

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
            None => crate::paths::continuum_home()
                .map_err(|e| CommandError::Internal(format!("cannot resolve the datasets root: {e}")))?
                .join("datasets"),
        };
        let dataset_dir = match candidate.as_ref() {
            Some(preparation) => preparation.dataset_dir()?,
            None => root.join(&name),
        };
        let tasks_solved = examples.len();
        let manifest = if candidate.is_some() {
            let name = name.clone();
            let directory = dataset_dir.clone();
            tokio::task::spawn_blocking(move || {
                DatasetService::split_and_write(&name, &directory, &examples, split_ratio, None)
            })
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))?
            .map_err(CommandError::Internal)?
        } else {
            DatasetService::split_and_write(&name, &dataset_dir, &examples, split_ratio, None)
                .map_err(CommandError::Internal)?
        };
        let result = GenomeTeachResult {
            training: None,
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
        };
        match candidate {
            Some(preparation) => preparation.publish(result).await,
            None => Ok(result),
        }
    }
}

fn select_teach_tasks(p: GenomeTeachParams) -> Result<Vec<EvalTask>, CommandError> {
    let strict = p.training.is_some();
    lesson_tasks(p.tasks, p.from_experience.as_deref(), p.teach_set.as_deref(), strict)
}

/// Where lessons come from, in order: inline → `from_experience` (the #319 curriculum drain,
/// her own objectively graded failures) → a `teach_set` JSONL → the committed default. The ONE
/// selector `genome/teach` and `coursework/import` share. `strict` parses a teach set with the
/// gym's fail-loud parser (a training run must not silently drop a malformed row). A missing
/// explicit path is a loud error (don't silently teach an empty set).
pub(crate) fn lesson_tasks(
    inline: Option<Vec<EvalTask>>,
    from_experience: Option<&str>,
    teach_set: Option<&str>,
    strict: bool,
) -> Result<Vec<EvalTask>, CommandError> {
    let tasks: Vec<EvalTask> = if let Some(inline) = inline {
        inline
    } else if let Some(solver) = from_experience {
        // Her lived, objectively graded failures become her curriculum. Same
        // citizen layout as the grader that wrote the stream (one resolver),
        // latest-per-task dedup so a later PASS retires the failure, then the
        // SAME salience selection every other experience consumer uses.
        let home = crate::paths::continuum_home()?;
        let (solver_full, solver_dir) =
            crate::commands::benchmark::resolve_solver_dir(&home, solver)?;
        let records = crate::cognition::experience::load_experiences(&solver_dir);
        let latest = crate::cognition::experience::latest_per_task(&records);
        let teach = crate::cognition::experience::salient_teach_set(
            &latest,
            &crate::cognition::experience::ErrorSalience,
        );
        // Salient failures the remediation teacher CANNOT consume: objectively
        // graded but testless (a swe-grade carries no rust `EvalTask.test`), so
        // `salient_teach_set` filters them out. They reach no learning organ today
        // (card 6cdaf59f) — count them so an empty remediation drain is not mistaken
        // for a clean citizen, and so the dead link is loud on the probe stream.
        let unteachable_salient = {
            use crate::cognition::experience::SalienceDetector;
            let d = crate::cognition::experience::ErrorSalience;
            latest
                .iter()
                .filter(|r| d.assess(r).is_some())
                .filter(|r| r.task.test.is_none())
                .count()
        };
        continuum_base::probe!(
            class = "genome.teach.from_experience",
            solver = solver_full.as_str(),
            stream_records = records.len() as u64,
            after_dedup = latest.len() as u64,
            teachable_failures = teach.len() as u64,
            unteachable_salient = unteachable_salient as u64,
            "curriculum drained from the citizen's lived experience stream (#319)",
        );
        if teach.is_empty() {
            // Distinguish a genuinely-clean citizen from a SILENT DEAD LINK. If she
            // has salient failures the remediation teacher can't consume (testless —
            // swe-grades), zero teachable is NOT "healthy": her real coding failures
            // are reaching no learning organ (card 6cdaf59f). Fail LOUD and name it,
            // never the reassuring all-clear that hid this.
            if unteachable_salient > 0 {
                return Err(CommandError::Invalid(format!(
                    "{unteachable_salient} salient failure(s) in {solver_full}'s stream \
                         that the remediation teacher CANNOT consume — objectively graded but \
                         testless (e.g. swe-grade), so no learning organ reaches them today \
                         (card 6cdaf59f). This is a DEAD LINK, not a healthy state: her real \
                         coding failures are not becoming curriculum ({} records, {} after dedup)",
                    records.len(),
                    latest.len()
                )));
            }
            // An empty drain with no unteachable remainder is a genuinely CLEAN state
            // (no salient failures pending), distinct from a misconfigured teach_set.
            return Err(CommandError::Invalid(format!(
                "no salient failures pending in {solver_full}'s experience stream \
                     ({} records, {} after latest-per-task dedup) — nothing to learn \
                     right now, which is a healthy state, not a fault",
                records.len(),
                latest.len()
            )));
        }
        teach
    } else {
        let path = teach_set.unwrap_or(DEFAULT_TEACH_SET); // unwrap_or: no teach set named = the committed default set, the selector's documented last source
        // The ONE gym resolver: a file on disk, the fetched cache, or the embedded copy.
        let (origin, text) = crate::cognition::gym::resolve_gym(path)
            .map_err(|e| CommandError::Invalid(format!("teach_set '{path}' could not be read: {e}")))?;
        if strict {
            crate::cognition::gym::parse_tasks(&text, &origin).map_err(CommandError::Invalid)?
        } else {
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .filter_map(|l| serde_json::from_str::<EvalTask>(l).ok())
                .collect()
        }
    };
    if tasks.is_empty() {
        return Err(CommandError::Invalid(
            "no tasks to teach (inline `tasks` empty and/or teach_set had no valid rows)".into(),
        ));
    }

    Ok(tasks)
}

// Runtime object is owned by GenomeModule; register its command descriptor once.
crate::register_command!(GenomeTeach);

mod progress;
mod status;
#[cfg(test)]
mod tests;
pub use progress::{subscribe_teach_progress, TeachProgress};
pub use status::{GenomeTeachStatus, GenomeTeachStatusParams, GenomeTeachStatusResult};
use progress::{emit_teach_milestone, emit_teach_progress, set_current_teach_run, write_teach_ledger};
