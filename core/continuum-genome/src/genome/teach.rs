//! CURRICULUM SYNTHESIS: a teacher model writes, the gym grader runs, the real error goes
//! back, and only test-passing trajectories become training examples (remediation), plus
//! the lived-expansion pass over a teammate's real asks. Genome learning, so it lives with
//! the genome; `genome/teach` (the command), the gene eval lane and the academy batch all
//! drive it. The teacher runs on an adapter its caller hands it: acquiring a lane for the
//! teacher is the caller's (the eval lane owner above the genome), never the genome's.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use schemars::JsonSchema;
use ts_rs::TS;

use crate::ai::types::TextGenerationResponse;
use crate::ai::{ChatMessage, MessageContent, TextGenerationRequest};
use crate::genome::gym_grader::test_grade;
use crate::ai::provenance::{GenerationOutcome, GenerationReceipt};
use crate::inference::llama_server::PROVIDER_ID;
use crate::sdk_codegen::CommandError;

pub mod progress;
pub use progress::{subscribe_teach_progress, TeachProgress};
use progress::{emit_teach_milestone, emit_teach_progress};

/// How many fix attempts a single task gets before it's dropped as unsolved, when
/// the caller doesn't set `max_fix_iters`. The first generation + this many fixes.
/// One place decides the default — one place decides how many fixes a lesson gets, command or loop.
pub const DEFAULT_MAX_FIX_ITERS: u32 = 4;

/// Teacher decoding temperature default — low, because we want correct code, and a
/// fix turn should converge on the error, not wander. `pub(crate)` for the same
/// single-source reason as [`DEFAULT_MAX_FIX_ITERS`].
pub const DEFAULT_TEMPERATURE: f32 = 0.2;

/// One lesson the teacher works to green: what is asked, and the test that proves an
/// answer. Narrower than an eval task on purpose: the teacher reads only these facts, so any
/// task source (a gym row, a lived failure, an academy section) projects into it at its seam.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TeachTask {
    /// The task id, echoed in its outcome for traceability.
    pub id: String,
    /// The prompt the teacher answers.
    pub prompt: String,
    /// The test appended to the answer and RUN; a task without one cannot be validated and is
    /// dropped with a named reason.
    pub test: Option<String>,
    /// Language of `test`; `rust` when unset.
    pub lang: Option<String>,
}

/// The teacher's standing instruction — frames it as an engineer who reads errors
/// and returns a complete corrected solution. This is the BEHAVIOR the trajectory
/// teaches, captured once as the system turn of every example.
const TEACHER_SYSTEM: &str = "You are an expert Rust engineer. Write correct, idiomatic Rust that \
    COMPILES and passes its tests. Return ONLY the item(s) the task asks for (functions/types) in a \
    single ```rust code block — a separate harness calls them, so do not write `fn main`. When you \
    are shown a compiler or test error, read it carefully and return the COMPLETE corrected \
    solution in a ```rust block.";

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
pub fn build_sharegpt(messages: &[ChatMessage], receipts: &[GenerationReceipt]) -> Value {
    let msgs: Vec<Value> = messages
        .iter()
        .map(|m| json!({ "role": m.role, "content": message_text(m) }))
        .collect();
    // The existing dataset writer preserves row metadata. Receipts describe the
    // actual teacher calls, not an inferred student outcome or training acceptance.
    json!({ "messages": msgs, "metadata": { "teacherGenerations": receipts } })
}

pub struct TeacherGeneration {
    pub text: String,
    pub receipt: GenerationReceipt,
}

/// Generate only through the acquired eval-lane adapter. The eval owner may share
/// resident weights; an acquisition refusal must never select a different live adapter.
pub async fn teacher_generate(
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
        reasoning_budget: None,
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        frequency_penalty: None,
        repeat_last_n: None,
        stop_sequences: None,
        tools: None,
        tail_tools: None,
        view_messages: None,
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

/// The validated corpus a remediation pass produces: the ShareGPT examples (only
/// test-PASSING trajectories) plus a per-task outcome trail and how many needed a
/// correction. Returned by [`synthesize_remediation_with_adapter`] to whatever drives it — the
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
/// self-improvement loop runs over a persona's own salience-selected failures (the
/// experience stream's salient teach set). It does NOT resolve the teacher, acquire its
/// lane or write the dataset: the caller owns model resolution, the lane and packaging, so
/// this stays a pure task-set → validated-corpus transform.
///
/// Mirror-and-challenge: the teacher solves HER failed tasks (mirror — her real
/// fitness gap) and the fix-loop stretches past the first wrong attempt (challenge —
/// the corrected trajectory she has not yet lived). Measurement stays elsewhere
/// (`cognition/eval`, isolated) — this only PRODUCES curriculum, never grades her.
///
/// The generation/grading loop borrows its adapter for the entire batch. The serving
/// owner can keep a private borrower and its cleanup obligation outside this future;
/// only the validated corpus leaves that owner.
pub async fn synthesize_remediation_with_adapter(
    tasks: &[TeachTask],
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
        // (the caller's lane acquisition waits on the serving watch — a push, not a poll), so
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
/// trained in (see [`synthesize_lived_expansion_with_adapter`]) — the being learns to answer the class
/// of question without needing this scaffold.
const LIVED_TEACHER_SYSTEM: &str = "You are a thoughtful expert. A teammate was asked the \
    following in a live conversation and their attempt stalled before they finished. Give a single, \
    complete, self-contained answer — clear and correct, with no meta-commentary about the attempt.";

/// The stimuli worth a teacher answer: trimmed, blanks dropped. Nothing to answer means no
/// fitness gap, a legitimate empty outcome, and the caller then needs no lane at all.
pub fn teachable_stimuli(stimuli: &[String]) -> Vec<&str> {
    stimuli
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect()
}

/// **Lived-axis expansion — the LLM-teacher path** that the curriculum's `expansion_examples`
/// deliberately deferred. A salient LIVED turn is untestable (no grader) AND a failure
/// (non-convergence / infra fault), so it can be neither remediated (no `test` to validate
/// a correction) nor echoed (training on her stalled answer would teach the failure). The
/// honest resolution: a teacher RE-ANSWERS the stimulus she faced, and that strong answer —
/// not her stalled one — becomes the SFT pair. Safe on untestable material for the same
/// reason the received axis is: validation is per-CONSOLIDATION by whole-being benchmark
/// lift (#59), never per-trajectory ([[lived-and-eval-experience-are-one-stream-one-being]]).
///
/// Runs on the adapter its caller acquired, like [`synthesize_remediation_with_adapter`],
/// but has NO grader loop: ONE teacher
/// generation per stimulus. Two honesty guards: an empty teacher answer is dropped (never
/// ship a blank lesson), and the teacher's system turn shapes generation but is NOT in the
/// example — the SFT pair is the bare `{question → answer}`, so the being learns the class
/// of question, not a scaffolded reply. The caller passes [`teachable_stimuli`], so an
/// empty set never spins up a lane.
pub async fn synthesize_lived_expansion_with_adapter(
    stimuli: &[&str],
    teacher_model: &str,
    temperature: f32,
    adapter: &std::sync::Arc<dyn crate::ai::adapter::AIProviderAdapter>,
) -> Result<Vec<Value>, CommandError> {
    let mut examples: Vec<Value> = Vec::new();
    for &stimulus in stimuli {
        // The system turn steers the GENERATION only; it is not part of the training pair.
        let messages = vec![
            ChatMessage::text("system", LIVED_TEACHER_SYSTEM),
            ChatMessage::text("user", stimulus),
        ];
        let answer =
            match teacher_generate(adapter, teacher_model, messages, temperature)
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

#[cfg(test)]
mod tests;
