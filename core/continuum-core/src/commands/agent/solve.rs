//! `agent/solve` — the headless-agent keystone. Drop a persona's WHOLE self into a working
//! directory, hand her one task, let her work it with her hands, and return the patch.
//!
//! This is the primitive every external benchmark harness composes on
//! ([[terminal-bench-is-the-meta-harness-build-one-agent-adapter]],
//! `docs/architecture/BENCHMARK-HARNESS-INTEGRATION.md`): Terminal-Bench, SWE-bench, Aider
//! Polyglot all reduce to "put an agent in a sandbox cwd with a task, collect the diff, grade
//! with our own tests." A benchmark being "LLM-shaped" is an ergonomics problem solved by the
//! RAG (the task is layered into her situation) — NEVER by stripping her
//! ([[benchmark-must-never-score-persona-against-a-soul-stripped-copy]]).
//!
//! She competes WHOLE: `with_tools: true` (hands on), `suppress_recall: false` (memory/RAG on),
//! her genome paged in on the measurement lane. Same drive machinery as `cognition/eval`
//! (`fork_eval_cycle_with_adapter` → `drive_to_settle`), minus the grader — the external harness
//! grades. One drive, two consumers (eval scores; agent/solve emits a patch).

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use ts_rs::TS;
use uuid::Uuid;

use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// Max act→observe cycles she may take on one task before it counts as unfinished. Agentic
/// SWE tasks (read → edit → compile → fix) need several; default generously.
const DEFAULT_MAX_ACTS: u32 = 12;
/// How long to wait for the forked cognition template (post-spawn `register_from_cfg` race).
const FORK_WAIT_TRIES: u32 = 20;

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/agent/AgentSolveParams.ts")]
pub struct AgentSolveParams {
    /// The persona (UUID, spawned) whose FULL cognition works the task.
    pub persona_id: String,
    /// The model to measure her on — forged into a dedicated measurement lane (her genome pages
    /// in on top). A loadable id from `ai/inference/models`.
    pub base_model_id: String,
    /// The task instruction, verbatim (layered into her situation/RAG).
    pub task: String,
    /// The working directory she acts in — her tools (`code/write`, `code/shell`, …) are rooted
    /// here. Ideally a git repo so the result is a clean diff; a bare dir also works.
    pub workspace: String,
    /// Max act→observe cycles (default 12).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub max_acts: Option<u32>,
    /// Fire-and-poll (#86): when true, the solve is spawned DETACHED — `run` returns a job
    /// handle NOW (arms empty, `detached: true`) and the REAL result (patch + acts) lands in
    /// `~/.continuum/progress/agent-solve-<run_id>.json`. A real agentic drive (N full-generation
    /// acts) outlives the IPC client timeout — a Devstral write→compile→fix loop took 12 min —
    /// so a Terminal-Bench/SWE-bench harness MUST fire-and-poll, never block on the socket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detach: Option<bool>,
    /// Correlation id for a detached run (echoed in the ack + the result file). Omit → minted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
    /// LEARN mode (#221 slice 3 — the loop-closer): after the solve, admit the EXPERIENCE
    /// of the work (task, act count, files touched — never the patch content, never any
    /// grader state) into the LIVING persona's memory, exactly as `cognition/eval`'s
    /// learn mode transfers redacted exam lessons. This is what lets work supersede
    /// stale beliefs: a day of python tasks consolidates into python facts, and the
    /// dream's supersession review demotes "you work with main.rs" — work IS training.
    /// The measurement fork itself stays #59-isolated either way; only the lesson
    /// crosses back. Default TRUE — a living being learns from her work (Joel
    /// 2026-07-23: "learn should be default anyway"); a harness wanting a
    /// memoryless measurement opts OUT with `learn:false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub learn: Option<bool>,
    /// GLASS-BOX (opt-in): directory for the JSONL turn-capture sink — every tick's bids +
    /// DECISION + timings append to `<dir>/<persona_id>.jsonl`, same sink `cognition/eval`
    /// wires (task #14). THE tool for diagnosing an acts=1 silent settle: the capture says
    /// whether she chose Act/Respond/Pass and why, where the bare ledger only shows the
    /// aftermath. Fork-only, never her live mind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub capture_dir: Option<String>,
    /// DIAGNOSTIC ONLY (default false — she competes WHOLE, memory ON). When true, her
    /// durable episodic/semantic recall is suppressed for this run — the same probe
    /// `cognition/eval` exposes ([[eval-measures-the-true-full-being-not-a-stripped-copy]]).
    /// Its ONLY legitimate use is glass-boxing session contamination: run a failing task
    /// with recall OFF — PASS ⇒ the failure is recall-mediated (stale/cross-task engrams
    /// surfacing), STILL FAIL ⇒ the failure is deeper (pipeline/model). NEVER a scoring
    /// mode — a benchmark number produced with recall off measures a stripped copy, not the
    /// being ([[benchmark-must-never-score-persona-against-a-soul-stripped-copy]]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub suppress_recall: Option<bool>,
    /// What the CALLER will grade — the turn's contract, declared by the harness that
    /// holds the answer key. Default [`Deliverable::Answer`]: her utterance is the
    /// result (#220's answer-graded tasks). A SWE-style harness that applies the DIFF
    /// and never reads the speech declares [`Deliverable::Workspace`], and the settle
    /// driver stops treating a zero-change explanation as a finished turn. Structural,
    /// caller-owned; it steers nothing about WHAT she does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub deliverable: Option<Deliverable>,
}

/// What the caller grades when the solve returns. Two genuinely different contracts,
/// so it is an enum on the wire, never a magic string ([[strings-to-enums]]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../protocol/typescript/agent/Deliverable.ts")]
pub enum Deliverable {
    /// Her spoken answer is the result (the default — every non-diff task).
    #[default]
    Answer,
    /// The state of the workspace is the result; the grader applies the diff.
    Workspace,
}

#[derive(Debug, Clone, Serialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/agent/AgentSolveResult.ts")]
pub struct AgentSolveResult {
    pub persona_id: String,
    pub model: String,
    /// How many times she acted (edited / ran / read) before settling.
    #[ts(type = "number")]
    pub acts: u32,
    /// Her final spoken answer (the "mouth" — some benchmarks grade this; SWE grades the patch).
    pub spoken: String,
    /// Unified `git diff` of everything she changed in the workspace — the "hands" artifact the
    /// SWE/Terminal-Bench harness applies + tests. Empty if she made no file changes (or the
    /// workspace is not a git repo).
    pub patch: String,
    /// Paths she touched (from `git diff --name-only`).
    pub files_changed: Vec<String>,
    /// True when this is the immediate ACK of a detached run (`acts`/`patch` empty — poll the
    /// result file `agent-solve-<run_id>.json` for the real outcome).
    pub detached: bool,
    /// The run's correlation id (set on a detached ack + the written result file).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
    /// INFRASTRUCTURE FAILURE, never a wrong answer. `Some(cause)` when the settle loop
    /// stopped because the deliberation model call FAILED (lane torn down mid-drive, a
    /// serving lane refusing a model it isn't hosting, a timeout) rather than because she
    /// finished. `SettleOutcome::inference_error` has carried this all along and its own
    /// doc says the grader MUST treat it as infra — but NOTHING READ IT, so a run
    /// truncated at act 7 of 30 reported `acts: 7, patchBytes: 0` and was indistinguishable
    /// from a persona who simply failed. Measured 2026-08-04 on sympy-21379: the lane went
    /// `serving: <none>, ready: false` mid-drive and the verdict said nothing at all.
    /// A number produced with this set is NOT a score
    /// ([[a-benchmark-zero-is-a-claim-about-the-harness-until-proven-otherwise]]).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub infra_error: Option<String>,
}

/// `agent/solve` — headless single-task agent run. Pure orchestration over the eval drive
/// machinery; never hand-spawns a llama-server (the serving system owns lifecycle) — it stands up
/// a measurement lane the same way `cognition/eval` does, holds it for the drive, drops it after.
#[derive(Default)]
pub struct AgentSolve;

#[async_trait]
impl ActionCommand for AgentSolve {
    const NAME: &'static str = "agent/solve";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Headless agent: drop a persona's WHOLE self (genome, memory, RAG, tools, act→verify loop) \
         into a working directory, hand her one task, and return the git patch she produced + her \
         answer + act count. The primitive every external benchmark harness (Terminal-Bench, \
         SWE-bench, Aider Polyglot) composes on. She is never stripped to fit a benchmark.";
    type Params = AgentSolveParams;
    type Output = AgentSolveResult;

    async fn run(&self, _ctx: &Ctx, p: AgentSolveParams) -> Result<AgentSolveResult, CommandError> {
        // Fire-and-poll (#86): a real agentic drive outlives the IPC client timeout, so `detach`
        // spawns the body on the runtime, writes the finished result to a poll file + emits a
        // terminal event, and returns a run_id NOW. The body is ctx-free (reaches the persona via
        // the global workspace registry), so it runs identically inline or detached.
        if p.detach.unwrap_or(false) {
            let run_id = p.run_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
            let run_id_ack = run_id.clone();
            let (persona_ack, model_ack) = (p.persona_id.clone(), p.base_model_id.clone());
            let mut inner = p;
            inner.detach = Some(false);
            inner.run_id = Some(run_id.clone());
            tokio::spawn(async move {
                let path = agent_solve_ledger_path(&run_id);
                match AgentSolve::solve_body(inner).await {
                    Ok(r) => {
                        if let (Some(path), Ok(json)) =
                            (path.as_ref(), serde_json::to_string_pretty(&r))
                        {
                            let _ = std::fs::write(path, json);
                        }
                        if let Some(bus) = crate::runtime::MessageBus::global() {
                            if let Ok(v) = serde_json::to_value(&r) {
                                bus.publish_async_only("agent:solve:complete", v);
                            }
                        }
                        tracing::info!(run_id = %run_id, acts = r.acts, "agent/solve detached run complete");
                    }
                    Err(e) => {
                        // Fail LOUD on the poll surface too — a detached run that dies must leave a
                        // diagnosable marker, never an empty file forever.
                        if let Some(path) = path {
                            let _ = std::fs::write(
                                &path,
                                serde_json::json!({"failed": true, "run_id": run_id, "error": e.to_string()})
                                    .to_string(),
                            );
                        }
                        tracing::error!(run_id = %run_id, error = %e, "agent/solve detached run failed");
                    }
                }
            });
            return Ok(AgentSolveResult {
                persona_id: persona_ack,
                model: model_ack,
                acts: 0,
                spoken: String::new(),
                patch: String::new(),
                files_changed: Vec::new(),
                detached: true,
                run_id: Some(run_id_ack),
                infra_error: None,
            });
        }
        Self::solve_body(p).await
    }
}

/// Result file for a detached solve run, polled after the ack (mirrors the eval/competition
/// progress-ledger convention: `~/.continuum/progress/agent-solve-<run_id>.json`).
fn agent_solve_ledger_path(run_id: &str) -> Option<std::path::PathBuf> {
    let base = std::env::var("CONTINUUM_HOME")
        .map(std::path::PathBuf::from)
        .ok()
        .or_else(|| dirs::home_dir().map(|h| h.join(".continuum")))?;
    let dir = base.join("progress");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join(format!("agent-solve-{run_id}.json")))
}

impl AgentSolve {
    /// The solve body — deliberately ctx-free (reaches the persona via the global workspace
    /// registry), so it runs inline OR spawned detached with the same code path.
    pub(crate) async fn solve_body(p: AgentSolveParams) -> Result<AgentSolveResult, CommandError> {
        let run_id = p.run_id.clone();
        // Short-form persona ids resolve too (#164): rosters/benchmark harnesses DISPLAY
        // 8-char short ids, so accept the id a caller was shown — a clean UUID passes
        // straight through, a short/mistyped form expands against the live persona registry
        // (the ONE shared id_resolve primitive), instead of failing "is not a UUID".
        let persona_uuid = crate::id_resolve::resolve(
            p.persona_id.trim(),
            &crate::persona::card::ids(),
            "persona",
        )
        .map_err(CommandError::Invalid)?;
        let workspace = p.workspace.trim().to_string();
        if !std::path::Path::new(&workspace).is_dir() {
            return Err(CommandError::Invalid(format!(
                "workspace '{workspace}' does not exist or is not a directory"
            )));
        }
        let max_acts = p.max_acts.unwrap_or(DEFAULT_MAX_ACTS).max(1) as usize;

        // 1) Stand up a dedicated measurement lane for the model (her genome pages in on top),
        //    exactly as cognition/eval does — held for the whole drive, dropped after.
        let lane = crate::cognition::eval::spawn_base_eval_lane(&p.base_model_id).await?;

        // 2) Fork her WHOLE cognition onto that lane, rooted at the workspace: tools ON, recall ON.
        //    A brief wait covers the post-spawn template race (same as the eval fork-waiter).
        let registry = crate::cognition::persona_workspace::global();
        let mut cycle = None;
        for attempt in 0..FORK_WAIT_TRIES {
            cycle = registry.fork_eval_cycle_with_adapter(
                &persona_uuid,
                lane.adapter.clone(),
                lane.served_ctx,
                true,             // with_tools — her hands are ON
                Some(&workspace), // roots the ToolExecutor at the sandbox cwd
                p.suppress_recall.unwrap_or(false), // memory/RAG ON by default; the diagnostic knob
            );
            if cycle.is_some() {
                break;
            }
            if attempt + 1 < FORK_WAIT_TRIES {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
        let cycle = cycle.ok_or_else(|| {
            CommandError::NotFound(format!(
                "no workspace template for persona {persona_uuid} — spawn her (register_from_cfg) \
                 before agent/solve can fork a measurement copy of her mind"
            ))
        })?;

        // 2b) Root her HANDS at the sandbox cwd. `fork_eval_cycle_with_adapter`'s `workspace_root`
        //     only repoints what she SEES (the workspace-map RAG block) — her `ToolExecutor` still
        //     writes to her durable per-persona workspace. Without this she writes reverse.py to
        //     `<home>/citizens/peers/<id>/workspace/` and our `git diff` on the sandbox scores a
        //     false ZERO (glass-boxed 2026-07-22: Devstral did 2 real acts, wrote the correct file,
        //     patch was empty). Same fail-loud mechanism cognition/eval uses to root a measurement
        //     persona at a target repo.
        crate::cognition::persona_workspace::root_acting_workspace(&cycle, &workspace).await?;

        // GLASS-BOX (same seam as cognition/eval, task #14): opt-in JSONL turn capture on
        // the fork — bids + DECISION + timings per tick, the instrument that turns an
        // acts=1 silent settle from a mystery into a mechanism.
        let cycle = match &p.capture_dir {
            Some(dir) => cycle.with_capture(std::sync::Arc::new(
                crate::cognition::workspace_capture::JsonlWorkspaceCaptureSink::open(
                    std::path::Path::new(dir),
                    persona_uuid,
                )
                .map_err(|e| {
                    CommandError::Internal(format!(
                        "failed to open agent/solve capture_dir '{dir}': {e}"
                    ))
                })?,
            )),
            None => cycle,
        };

        // 3) Layer the task into her situation as a directed, TOOL-FORCING request. The dominant
        //    misfit-coder failure (glass-boxed 2026-07-22): a 7B answers with the code in a message
        //    ("here's reverse.py: ```…```" / "I saved it to reverse.py") instead of CALLING the
        //    write tool → the graded artifact (the git patch) is empty. The old "Provide your
        //    complete solution" framing literally invited that own-goal. This is the standard SWE /
        //    Terminal-Bench harness contract: the deliverable is what her TOOLS put in the
        //    workspace; narrating it does not perform it. Meeting the misfit where it is — an
        //    ergonomic/adapter fix ([[use-adapters-dont-dumb-it-down]]), not a capability demand —
        //    and honest (it states the real I/O contract; it does not hand her the answer). Then
        //    DRIVE her to settlement (read → edit → run → fix, her real act→observe loop).
        let room = Uuid::nil();
        // The workspace-grounding sentence counters the observed "new project ritual"
        // (glass-boxed 2026-07-22 via turn capture: her first act on a seeded task was
        // code/create-workspace("my_stack_project") + a Rust hello-world + git/commit —
        // her habitual onboarding sequence replaying from memory — which re-roots her
        // hands OFF the graded tree, then she passes to a silent settle). Honest
        // contract language, same class as the tool-forcing framing: it states where
        // the work IS, it does not hand her the answer or gate her tools.
        let framed = format!(
            "This is a task you must COMPLETE NOW by USING YOUR TOOLS in your workspace — writing \
             files with code/write, running commands with code/shell, etc. Only what your tools \
             actually do takes effect: code shown in a message, or a claim that you saved a file, \
             does NOT create or change anything — the workspace is graded on the files your tools \
             write. You are ALREADY in the task's workspace: work on the files that are here. Do \
             not create a new workspace or start a new project — grading only sees this one. Do \
             the work with tool calls, then stop.\n\nTask:\n{}",
            p.task.trim()
        );
        let task_delivery = crate::persona::rag_budget::RagDelivery {
            source_id: "airc".to_string(),
            items: vec![crate::persona::rag_budget::RagItem {
                content: framed,
                tokens: 0,
                metadata: serde_json::json!({
                    "peer_id": "peer",
                    "occurred_at_ms": crate::persona::trace::now_ms(),
                }),
            }],
            tokens_used: 0,
            continuation: None,
            resolution_used: crate::persona::rag_budget::ResolutionPreference::Raw,
        };
        let burst = crate::cognition::workspace::Burst::from_turns(
            room,
            crate::persona::service_loop::build_workspace_turns(
                std::slice::from_ref(&task_delivery),
                "",
                "",
                None,
            ),
        );
        let settled = crate::cognition::act_observe::drive_to_settle(
            &cycle,
            burst,
            room,
            max_acts,
            {
                let f = crate::cognition::workspace::TurnFraming::directed();
                match p.deliverable.unwrap_or_default() {
                    Deliverable::Workspace => f.on_workspace(),
                    Deliverable::Answer => f,
                }
            },
        )
        .await;

        // 4) Collect the HANDS artifact: everything she changed in the workspace as a unified diff
        //    (new files included), plus the touched paths. This is what SWE/Terminal-Bench apply.
        let (patch, files_changed) = workspace_patch(&workspace).await;

        // 5) LEARN mode (#221 slice 3): carry the EXPERIENCE back to the living self —
        //    the same one-way bridge cognition/eval's learn mode uses. The lesson is
        //    experience-shaped (task + how she worked + which files), deliberately
        //    excluding the patch content and her final answer: the python-context
        //    signal that drives dream supersession rides the task text and file
        //    names; verbatim solutions would let a re-run score memorization instead
        //    of capability. Solve carries no held-out answer key in-band (the harness
        //    grades externally), so there is nothing to redact.
        if p.learn.unwrap_or(true) {
            let admitted = transfer_solve_experience(
                &persona_uuid,
                room,
                &p.task,
                settled.acts,
                &files_changed,
            );
            tracing::info!(
                persona = %persona_uuid,
                admitted,
                acts = settled.acts,
                "agent/solve learn mode: work experience admitted to the living self"
            );
        }

        // Lane drops here (end of scope) — measurement copy torn down, living personas untouched.
        drop(lane);

        Ok(AgentSolveResult {
            persona_id: p.persona_id,
            model: p.base_model_id,
            acts: settled.acts as u32,
            spoken: settled.spoken.unwrap_or_default(),
            patch,
            files_changed,
            detached: false,
            run_id,
            infra_error: settled.inference_error,
        })
    }
}

/// Build the durable EXPERIENCE string for one solve — what she worked on and how,
/// never what she produced (no patch content, no spoken answer: the lesson teaches
/// context, not solutions, so re-runs measure capability rather than memorization).
/// Pure; unit-testable.
fn format_solve_lesson(task: &str, acts: usize, files_changed: &[String]) -> String {
    let worked = if files_changed.is_empty() {
        "I changed no files".to_string()
    } else {
        format!("I changed: {}", files_changed.join(", "))
    };
    format!(
        "I worked a real coding task in my workspace: {} — I acted {} time(s); {}.",
        task.trim(),
        acts,
        worked
    )
}

/// Admit one solve's experience lesson into the LIVING persona (never the fork) —
/// `cognition/eval::transfer_redacted_lessons`' one-way bridge, solve-shaped.
/// Returns 1 if a fresh lesson was admitted (identical re-run lessons dedup
/// idempotently via `admit_reflection`'s content hash), else 0.
fn transfer_solve_experience(
    persona_uuid: &Uuid,
    room: Uuid,
    task: &str,
    acts: usize,
    files_changed: &[String],
) -> usize {
    let Some(admission) = crate::cognition::persona_workspace::global()
        .get(persona_uuid)
        .and_then(|cycle| cycle.acting().map(|a| a.admission.clone()))
    else {
        tracing::warn!(
            persona = %persona_uuid,
            "agent/solve learn mode: no live admission — experience not transferred \
             (she was measured, but the living self is not resident to learn)"
        );
        return 0;
    };
    let mut recall_keys = vec!["agent-solve".to_string()];
    recall_keys.extend(files_changed.iter().cloned());
    let engram = crate::persona::engram::Engram {
        id: Uuid::new_v4(),
        context_id: Some(room),
        kind: crate::persona::engram::EngramKind::Episodic,
        content: format_solve_lesson(task, acts, files_changed),
        origin: crate::persona::engram::EngramOrigin::SelfReflection {
            parent_engram_id: Uuid::nil(),
        },
        recall_keys,
        admitted_at_ms: crate::persona::trace::now_ms(),
        trust_state_at_admission: crate::persona::engram::TrustState::SelfTrust,
        admission_trace_id: None,
    };
    match admission.admit_reflection(engram) {
        Ok(crate::persona::engram::AdmissionDecision::Admit { .. }) => 1,
        _ => 0,
    }
}

/// Git pathspecs excluding the universal never-a-solution byproducts a verification run leaves
/// behind — Python bytecode/caches, tool caches, JS deps, OS cruft. Glass-boxed 2026-07-22: a
/// `python3 -c "from calc import ..."` verify step left `__pycache__/calc.cpython-314.pyc` in the
/// patch, polluting the graded artifact — real SWE-bench/aider patches are SOURCE-only. These are
/// never a solution, so they're excluded from both the diff and files_changed; anything a task
/// might legitimately produce (incl. `build`/`dist`/`target`) is kept.
const PATCH_EXCLUDES: &[&str] = &[
    ":(exclude,glob)**/__pycache__/**",
    ":(exclude,glob)**/*.pyc",
    ":(exclude,glob)**/*.pyo",
    ":(exclude,glob)**/.pytest_cache/**",
    ":(exclude,glob)**/.mypy_cache/**",
    ":(exclude,glob)**/.ruff_cache/**",
    ":(exclude,glob)**/node_modules/**",
    ":(exclude,glob)**/.DS_Store",
];

/// Unified diff of the SOLUTION changes in the workspace (tracked edits + new files), and the
/// touched paths — build/cache byproducts ([`PATCH_EXCLUDES`]) filtered out so the graded artifact
/// is source-only. `git add -N` stages new files as intent-to-add so `git diff` includes them
/// without committing content; the same excludes keep junk from being intent-added in the first
/// place. Non-repo or git-less environments return empty (honest — no hands artifact).
async fn workspace_patch(workspace: &str) -> (String, Vec<String>) {
    let git = |args: &[&str]| {
        let mut c = tokio::process::Command::new("git");
        c.arg("-C").arg(workspace).args(args);
        c
    };
    // Build the `-- . :(exclude…)` pathspec tail shared by add-N and both diffs.
    let mut pathspec: Vec<&str> = vec!["--", "."];
    pathspec.extend_from_slice(PATCH_EXCLUDES);
    let with_paths = |head: &[&str]| -> Vec<String> {
        head.iter().chain(pathspec.iter()).map(|s| s.to_string()).collect()
    };
    // Non-fatal: a bare (non-git) workspace just yields no patch.
    let _ = git(&with_paths(&["add", "-A", "-N"]).iter().map(String::as_str).collect::<Vec<_>>())
        .output()
        .await;
    let diff_args = with_paths(&["diff"]);
    let names_args = with_paths(&["diff", "--name-only"]);
    let diff = git(&diff_args.iter().map(String::as_str).collect::<Vec<_>>()).output().await.ok();
    let names = git(&names_args.iter().map(String::as_str).collect::<Vec<_>>()).output().await.ok();
    let patch = diff
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let files_changed = names
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.to_string())
                .collect()
        })
        .unwrap_or_default();
    (patch, files_changed)
}

crate::register_stateless_command!(AgentSolve);

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::process::Command;

    async fn git(dir: &std::path::Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .await
            .expect("git runs");
        assert!(out.status.success(), "git {:?} failed: {}", args, String::from_utf8_lossy(&out.stderr));
    }

    // what this catches: the patch is the benchmark's HANDS artifact — the SWE/Terminal-Bench
    // harness applies it and runs the repo's tests. If workspace_patch dropped NEW files (the
    // common case for a from-scratch task) it would silently score 0 on every such task while
    // looking like the agent "did nothing". This pins that `git add -A -N` + `git diff` includes
    // both an edit to a tracked file AND a brand-new untracked file.
    #[tokio::test]
    async fn workspace_patch_includes_edits_and_new_files() {
        let dir = std::env::temp_dir().join(format!("cu-agent-solve-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]).await;
        git(&dir, &["config", "user.email", "t@t"]).await;
        git(&dir, &["config", "user.name", "t"]).await;
        std::fs::write(dir.join("tracked.txt"), "one\n").unwrap();
        git(&dir, &["add", "-A"]).await;
        git(&dir, &["commit", "-qm", "base"]).await;

        // The agent's "hands": edit a tracked file + create a new one.
        std::fs::write(dir.join("tracked.txt"), "one\ntwo\n").unwrap();
        std::fs::write(dir.join("brand_new.rs"), "fn main() {}\n").unwrap();

        let (patch, files) = workspace_patch(dir.to_str().unwrap()).await;
        assert!(patch.contains("tracked.txt"), "edit missing from patch:\n{patch}");
        assert!(patch.contains("brand_new.rs"), "NEW file missing from patch:\n{patch}");
        assert!(patch.contains("+two"), "edit content missing:\n{patch}");
        assert!(patch.contains("fn main()"), "new-file content missing:\n{patch}");
        assert!(files.iter().any(|f| f == "tracked.txt"));
        assert!(files.iter().any(|f| f == "brand_new.rs"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: verification byproducts (Python bytecode, __pycache__) must NOT pollute
    // the graded patch — glass-boxed 2026-07-22 when a `python3 -c "from calc import ..."` verify
    // step left calc.cpython-314.pyc in the diff. Source is kept; the cache junk is filtered.
    #[tokio::test]
    async fn workspace_patch_excludes_build_byproducts() {
        let dir = std::env::temp_dir().join(format!("cu-agent-solve-junk-{}", Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("__pycache__")).unwrap();
        git(&dir, &["init", "-q"]).await;
        git(&dir, &["config", "user.email", "t@t"]).await;
        git(&dir, &["config", "user.name", "t"]).await;
        // her solution + the byproducts a verify run leaves behind
        std::fs::write(dir.join("calc.py"), "def add(a, b):\n    return a + b\n").unwrap();
        std::fs::write(dir.join("__pycache__/calc.cpython-314.pyc"), b"\x00\x01bytecode").unwrap();
        std::fs::create_dir_all(dir.join("node_modules/x")).unwrap();
        std::fs::write(dir.join("node_modules/x/index.js"), "module.exports={}").unwrap();

        let (patch, files) = workspace_patch(dir.to_str().unwrap()).await;
        assert!(patch.contains("calc.py"), "the solution source must be in the patch:\n{patch}");
        assert!(!patch.contains(".pyc"), "bytecode must be excluded:\n{patch}");
        assert!(!patch.contains("__pycache__"), "cache dir must be excluded:\n{patch}");
        assert!(!patch.contains("node_modules"), "deps must be excluded:\n{patch}");
        assert_eq!(files, vec!["calc.py".to_string()], "only source is a changed file: {files:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: a non-git workspace must degrade to an empty patch, never panic — a
    // bare sandbox dir (some benchmarks hand the agent a plain cwd) is a legitimate input.
    #[tokio::test]
    async fn workspace_patch_is_empty_on_non_git_dir() {
        let dir = std::env::temp_dir().join(format!("cu-agent-solve-bare-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("x.txt"), "hi\n").unwrap();
        let (patch, files) = workspace_patch(dir.to_str().unwrap()).await;
        assert!(patch.is_empty(), "bare dir should yield no patch, got:\n{patch}");
        assert!(files.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: the learn-mode lesson teaches CONTEXT, never solutions —
    // it must carry the task text + how she worked + which files (the language/
    // domain signal dream-supersession feeds on) and must NOT be empty-file
    // fragile. If patch content or answers ever leak into the lesson, battery
    // re-runs would measure memorization instead of capability.
    #[test]
    fn solve_lesson_is_experience_shaped() {
        let l = format_solve_lesson(
            "There is a bug in mathlib.py: multiply returns a+b. Fix it.",
            3,
            &["mathlib.py".to_string()],
        );
        assert!(l.contains("mathlib.py"), "domain signal rides the file name: {l}");
        assert!(l.contains("acted 3 time(s)"));
        assert!(l.contains("I changed: mathlib.py"));
        let none = format_solve_lesson("task", 0, &[]);
        assert!(none.contains("I changed no files"));
    }

    // what this catches: the wire name must mirror the file path (commands/agent/solve.rs ⟺
    // "agent/solve") and stay Privileged — it drives arbitrary shell + writes files in the cwd,
    // the same authority boundary as agent/start. Drift silently breaks routing or widens the ACL.
    #[test]
    fn name_and_access_hold_the_contract() {
        assert_eq!(AgentSolve::NAME, "agent/solve");
        assert!(matches!(AgentSolve::ACCESS, AccessLevel::Privileged));
    }
}
