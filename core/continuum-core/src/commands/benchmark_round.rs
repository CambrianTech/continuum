//! `benchmark/round` — initiation and resume are the SAME verb.
//!
//! The one-command round (docs/planning/ONE-COMMAND-ROUND.md; Joel 2026-08-24:
//! *"It must work automatically entirely after initiation or resume (same
//! command I imagine)"*). The verb reads the round's last durable values — the
//! streamed `kind:"task"` grade rows in the progress ledger — and continues from
//! the first ungraded task; a completed round starts fresh. No flags for
//! resume: an interrupted round is a PAUSED round the same command continues
//! ([[continuity-is-the-default-reset-is-the-exception]]).
//!
//! v1 composes existing parts only: `gym::resolve_gym` (staging + freshness,
//! fail-loud), the live persona registry, and a detached `cognition/eval` in
//! learn mode. Gold-gating, the arena hold, and the round report are the next
//! slices of the spec — this slice makes initiate/resume/idempotence real.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cognition::eval::{
    detached_evals_in_flight, graded_task_ids_from, live_eval_run_id, progress_ledger_dir,
    CognitionEval, CognitionEvalParams, EvalTask,
};
use crate::cognition::learning_policy::LearningPolicy;
use crate::sdk_codegen::command::ActionCommand;
use crate::sdk_codegen::handler::Ctx;
use crate::sdk_codegen::{AccessLevel, CommandError};

#[derive(Debug, Clone, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRoundParams.ts"
)]
pub struct BenchmarkRoundParams {
    /// The gym to run — anything `gym::resolve_gym` accepts: a fetched benchmark
    /// name (`mirrorcode.jsonl`), a committed gym, or an on-disk JSONL path.
    pub benchmark: String,
    /// Who sits the round. Omitted → the sole online persona (fails loud naming
    /// the count when zero or several are online — pick one explicitly then).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub persona: Option<String>,
    /// Explicit reset: abandon any resumable run and start a new one. The rare
    /// exception — resume is the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fresh: Option<bool>,
    /// Cap the task count of a FRESH run (smoke rounds). Ignored on resume — a
    /// resumed round finishes the set it started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub limit: Option<u32>,
    /// HELP ARM: declare peer help legal for this round (see
    /// `CognitionEvalParams::help`). Scores report as OURS+help; solo stays the
    /// default arm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub help: Option<bool>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRoundResult.ts"
)]
pub struct BenchmarkRoundResult {
    /// The round's run handle — poll with `cognition/eval-status`.
    pub run_id: String,
    /// True when this invocation CONTINUED an interrupted run instead of
    /// starting a new one.
    pub resumed: bool,
    /// True when the referenced round was already fully graded — nothing was
    /// dispatched (run a `--fresh` round for a retake).
    pub complete: bool,
    /// True when this exact run is grading RIGHT NOW — nothing was dispatched
    /// (the verb is idempotent; calling it again is always safe).
    pub already_running: bool,
    /// The gym reference as given.
    pub benchmark: String,
    /// Where the tasks resolved from (cache path / `embedded:` origin).
    pub eval_set: String,
    /// Task counts: the whole set, already graded, dispatched this invocation.
    #[ts(type = "number")]
    pub total: u32,
    #[ts(type = "number")]
    pub graded: u32,
    #[ts(type = "number")]
    pub remaining: u32,
    /// Human-readable one-liner of what the verb decided and why.
    pub note: String,
}

/// The dispatch-time idempotence gate, pure for tests: given the detached evals in
/// flight, decide whether the verb may dispatch. `Some(Ok(run_id))` = THIS
/// benchmark's round is already in flight (report it, dispatch nothing);
/// `Some(Err(msg))` = a DIFFERENT round holds the arena (evals serialize — a second
/// dispatch would silently queue behind it for hours, so refuse loudly instead);
/// `None` = free to dispatch. Closes the measured gap where a round's first task
/// grades ~30min after dispatch and the ledger-based resume scan sees nothing.
fn gate_on_in_flight(
    in_flight: &[(String, String, String)],
    benchmark: &str,
) -> Option<Result<String, String>> {
    if let Some((rid, _, _)) = in_flight.iter().find(|(_, _, set)| set == benchmark) {
        return Some(Ok(rid.clone()));
    }
    in_flight.first().map(|(rid, persona, set)| {
        Err(format!(
            "another round is already grading (run {rid}, persona {persona}, gym {set}) — evals              serialize on the exam lease, so dispatching now would silently queue for hours.              Wait for it (cognition/eval-status --run-id {rid}) or reboot to clear it."
        ))
    })
}

/// The newest streamed task row's runId for `(persona ledger, benchmark ref)` —
/// the resume candidate. Pure over the ledger text so it is unit-testable.
fn latest_round_run_id(text: &str, benchmark: &str) -> Option<String> {
    text.lines()
        .rev()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v.get("kind").and_then(|k| k.as_str()) == Some("task"))
        .find(|v| v.get("evalSet").and_then(|e| e.as_str()) == Some(benchmark))
        .and_then(|v| {
            v.get("runId")
                .and_then(|r| r.as_str())
                .map(|r| r.to_string())
        })
}

/// Is `run_id`'s terminal row present in this ledger text — i.e. did the run
/// COMPLETE (or fail-loud) rather than get killed mid-flight? Task rows are
/// skipped by construction (they carry `kind:"task"`); a `failed:true` row
/// reads as NOT terminal so an infra-died round stays resumable.
fn run_completed(text: &str, run_id: &str) -> bool {
    text.lines()
        .rev()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v.get("kind").and_then(|k| k.as_str()) != Some("task"))
        .filter(|v| v.get("runId").and_then(|r| r.as_str()) == Some(run_id))
        .any(|v| {
            v.get("failed").and_then(|f| f.as_bool()) != Some(true)
                && v.get("cancelled").and_then(|c| c.as_bool()) != Some(true)
        })
}

#[derive(Default)]
pub struct BenchmarkRound;

#[async_trait]
impl ActionCommand for BenchmarkRound {
    const NAME: &'static str = "benchmark/round";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Run (or RESUME — same verb, no flags) a benchmark round: resolves the gym, picks the \
         online persona, reads the durable per-task grade rows, and dispatches a detached \
         learn-mode eval over the ungraded remainder. Idempotent: call it again after any \
         interruption and the round continues from the first ungraded task. Poll the returned \
         run_id with cognition/eval-status. `--fresh` is the explicit reset.";
    type Params = BenchmarkRoundParams;
    type Output = BenchmarkRoundResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: BenchmarkRoundParams,
    ) -> Result<BenchmarkRoundResult, CommandError> {
        // 1. Gym: staged + freshness-gated by the ONE resolver (fail-loud on a
        //    stale fingerprint, naming the re-materialize command).
        let (origin, text) =
            crate::cognition::gym::resolve_gym(&p.benchmark).map_err(CommandError::Invalid)?;
        let all_tasks: Vec<EvalTask> = text
            .lines()
            .enumerate()
            .map(|(i, l)| (i + 1, l.trim()))
            .filter(|(_, l)| !l.is_empty())
            .map(|(n, l)| {
                serde_json::from_str::<EvalTask>(l).map_err(|e| {
                    CommandError::Invalid(format!("{origin} line {n}: malformed EvalTask: {e}"))
                })
            })
            .collect::<Result<_, _>>()?;
        if all_tasks.is_empty() {
            return Err(CommandError::Invalid(format!(
                "gym '{}' resolved ({origin}) but holds zero tasks — nothing to round on",
                p.benchmark
            )));
        }

        // 1.5. Idempotence at DISPATCH scope: a detached round in flight for this
        //      benchmark is THE round — report it, never mint a sibling (live-measured
        //      hole: two fresh runs from two invocations 90s apart). A different
        //      benchmark's round holding the arena refuses loudly.
        match gate_on_in_flight(&detached_evals_in_flight(), &p.benchmark) {
            Some(Ok(rid)) => {
                let ledger_probe = progress_ledger_dir()
                    .map(|d| {
                        d.read_dir()
                            .ok()
                            .map(|entries| {
                                entries
                                    .flatten()
                                    .filter(|e| {
                                        e.path().extension().and_then(|x| x.to_str())
                                            == Some("jsonl")
                                    })
                                    .filter_map(|e| std::fs::read_to_string(e.path()).ok())
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            })
                            .unwrap_or_default() // unreadable ledger dir = 0 graded shown; the run handle is still correct
                    })
                    .unwrap_or_default(); // no HOME = no ledger = 0 graded shown, handle still correct
                let graded = graded_task_ids_from(&ledger_probe, &rid).len() as u32;
                return Ok(BenchmarkRoundResult {
                    run_id: rid,
                    resumed: false,
                    complete: false,
                    already_running: true,
                    benchmark: p.benchmark,
                    eval_set: origin,
                    total: all_tasks.len() as u32,
                    graded,
                    remaining: all_tasks.len() as u32 - graded,
                    note: "this round is already in flight — nothing dispatched; poll \
                           cognition/eval-status"
                        .into(),
                });
            }
            Some(Err(msg)) => return Err(CommandError::Invalid(msg)),
            None => {}
        }

        // 2. Persona: named, or the sole online citizen. Zero/several online →
        //    fail loud with the fix, never guess who sits the exam.
        let persona_ref: crate::identity::PersonaRef = match &p.persona {
            Some(name) => name.as_str().into(),
            None => {
                let ids = crate::cognition::persona_workspace::global().template_ids();
                match ids.as_slice() {
                    [only] => only.to_string().into(),
                    [] => {
                        return Err(CommandError::Invalid(
                            "no persona is online to sit the round — spawn one (persona/spawn) or \
                             pass --persona <name>"
                                .into(),
                        ))
                    }
                    many => {
                        return Err(CommandError::Invalid(format!(
                            "{} personas are online — pass --persona <name|uuid> to pick who sits \
                             the round (persona/instances/list shows them)",
                            many.len()
                        )))
                    }
                }
            }
        };
        let persona_uuid = crate::cognition::persona_workspace::global()
            .resolve_persona(&persona_ref)
            .map_err(CommandError::Invalid)?
            .as_uuid();
        // SELF-ESTABLISHMENT (spec step 5): post-reboot, a resolvable persona's
        // workspace TEMPLATE can still be assembling — the eval's own fork wait is
        // 10s, and dispatching into that window burned two runs today (each died
        // "no workspace template after 10s" and the round had to be re-invoked by
        // hand — the exact babysitting this verb exists to delete). Wait bounded
        // here (2 min covers a cold spawn), fail loud with the fix if it never
        // assembles.
        {
            let mut waited = 0u32;
            while !crate::cognition::persona_workspace::global()
                .template_ids()
                .contains(&persona_uuid)
            {
                if waited >= 120 {
                    return Err(CommandError::Invalid(format!(
                        "persona {persona_uuid} resolved but her workspace template never                          assembled within {waited}s — the core may still be booting her.                          Check persona/instances/list, then re-run this same command."
                    )));
                }
                tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                waited += 4;
            }
        }

        // 3. Resume state: the streamed `kind:"task"` rows are the round's durable
        //    memory. Missing ledger = first round ever = fresh.
        let ledger_text = progress_ledger_dir()
            .map(|d| d.join(format!("{persona_uuid}.jsonl")))
            .and_then(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_default(); // no ledger yet = first round ever = fresh, by design
        let resumable = if p.fresh.unwrap_or(false) { // omitted flag = the default: resume
            None
        } else {
            latest_round_run_id(&ledger_text, &p.benchmark)
                .filter(|rid| !run_completed(&ledger_text, rid))
        };

        let (run_id, tasks, graded, resumed) = match resumable {
            Some(rid) => {
                // Idempotence: this exact run grading right now → report, dispatch nothing.
                if live_eval_run_id().as_deref() == Some(rid.as_str()) {
                    let graded = graded_task_ids_from(&ledger_text, &rid).len() as u32;
                    return Ok(BenchmarkRoundResult {
                        run_id: rid,
                        resumed: false,
                        complete: false,
                        already_running: true,
                        benchmark: p.benchmark,
                        eval_set: origin,
                        total: all_tasks.len() as u32,
                        graded,
                        remaining: all_tasks.len() as u32 - graded,
                        note: "this round is grading right now — nothing dispatched; poll \
                               cognition/eval-status"
                            .into(),
                    });
                }
                let graded_ids = graded_task_ids_from(&ledger_text, &rid);
                let remaining: Vec<EvalTask> = all_tasks
                    .iter()
                    .filter(|t| !graded_ids.contains(&t.id))
                    .cloned()
                    .collect();
                if remaining.is_empty() {
                    // Every task graded but the summary row never landed (killed after
                    // the last grade): the round is DONE — say so, dispatch nothing.
                    return Ok(BenchmarkRoundResult {
                        run_id: rid,
                        resumed: false,
                        complete: true,
                        already_running: false,
                        benchmark: p.benchmark,
                        eval_set: origin,
                        total: all_tasks.len() as u32,
                        graded: graded_ids.len() as u32,
                        remaining: 0,
                        note: "every task already graded — round complete; pass --fresh for a \
                               retake"
                            .into(),
                    });
                }
                (rid, remaining, graded_ids.len() as u32, true)
            }
            None => {
                let mut tasks = all_tasks.clone();
                if let Some(cap) = p.limit {
                    tasks.truncate(cap as usize);
                }
                (uuid::Uuid::new_v4().to_string(), tasks, 0, false)
            }
        };

        // 4. Dispatch: detached learn-mode eval over the remainder, SAME run_id on
        //    resume so every grade row and lesson lands in one round's thread.
        //    `eval_set` rides along even with inline tasks — it is the identity the
        //    per-task rows key resume on (inline wins as the task SOURCE).
        let dispatched = tasks.len() as u32;
        let note_text = format!(
            "benchmark/round {} — {} of {} tasks{}",
            p.benchmark,
            dispatched,
            all_tasks.len(),
            if resumed { " (resumed)" } else { "" }
        );
        let eval_params = CognitionEvalParams {
            persona_id: persona_ref,
            gene: None,
            room_id: None,
            tasks: Some(tasks),
            eval_set: Some(p.benchmark.clone()),
            base_model_id: None,
            reviewers: None,
            max_acts: None,
            max_retries: None,
            temperature: None, // lived sampling — the engineerable default
            note: Some(note_text.clone()),
            detach: Some(true),
            run_id: Some(run_id.clone()),
            workspace_root: None,
            capture_dir: None,
            learn: LearningPolicy::LearnFromThisWork,
            suppress_recall: None,
            help: p.help, // arm rides the round declaration through to the eval
        };
        let ctx = Ctx {
            handle: None,
            session_id: None,
            user_id: None,
            context_id: None,
            caller: None,
        };
        let ack = CognitionEval.run(&ctx, eval_params).await?;
        crate::probe!(
            class = "bench.round.dispatched",
            benchmark = %p.benchmark,
            run_id = %run_id,
            resumed = resumed,
            graded = graded as u64,
            dispatched = dispatched as u64,
            "one-command round dispatched — same verb resumes it after any interruption"
        );
        Ok(BenchmarkRoundResult {
            run_id: ack.run_id.unwrap_or(run_id), // ack echoes the id we minted; either is the same handle
            resumed,
            complete: false,
            already_running: false,
            benchmark: p.benchmark,
            eval_set: origin,
            total: all_tasks.len() as u32,
            graded,
            remaining: dispatched,
            note: note_text,
        })
    }
}

crate::register_stateless_command!(BenchmarkRound);

#[derive(Debug, Clone, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRoundStopParams.ts"
)]
pub struct BenchmarkRoundStopParams {
    /// The run to stop. Omit to stop whatever round is in flight (there is at
    /// most one — evals serialize on the exam lease).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRoundStopResult.ts"
)]
pub struct BenchmarkRoundStopResult {
    pub run_id: String,
    /// Cancellation lands at the next task boundary; grades already streamed
    /// stay, and the SAME `benchmark/round` command resumes from the first
    /// ungraded task later.
    pub note: String,
}

/// `benchmark/round-stop` — stop a round WITHOUT a reboot (2026-08-24, Joel:
/// "can't iterate to put the new one in place"). The iteration primitive:
/// stop → fix → deploy → same round command resumes.
#[derive(Default)]
pub struct BenchmarkRoundStop;

#[async_trait]
impl ActionCommand for BenchmarkRoundStop {
    const NAME: &'static str = "benchmark/round-stop";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Stop the in-flight benchmark round cleanly (no reboot): cancellation lands at the next          task boundary, every streamed grade is kept, and the same benchmark/round command          resumes from the first ungraded task. Pass run_id, or omit to stop the one in flight.";
    type Params = BenchmarkRoundStopParams;
    type Output = BenchmarkRoundStopResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: BenchmarkRoundStopParams,
    ) -> Result<BenchmarkRoundStopResult, CommandError> {
        let run_id = match p.run_id {
            Some(rid) => rid,
            None => {
                let in_flight = detached_evals_in_flight();
                match in_flight.as_slice() {
                    [(rid, _, _)] => rid.clone(),
                    [] => {
                        return Err(CommandError::Invalid(
                            "no round is in flight — nothing to stop".into(),
                        ))
                    }
                    many => {
                        return Err(CommandError::Invalid(format!(
                            "{} runs in flight — pass --run-id to pick one",
                            many.len()
                        )))
                    }
                }
            }
        };
        crate::cognition::eval::cancel_eval_run(&run_id);
        Ok(BenchmarkRoundStopResult {
            run_id,
            note: "cancellation lands at the next task boundary; grades kept; the same                    benchmark/round command resumes later"
                .into(),
        })
    }
}

crate::register_stateless_command!(BenchmarkRoundStop);

#[cfg(test)]
mod tests {
    use super::*;

    fn task_row(run: &str, set: &str, task: &str) -> String {
        serde_json::json!({
            "kind": "task", "runId": run, "evalSet": set, "taskId": task, "ok": true, "acts": 1
        })
        .to_string()
    }
    fn run_row(run: &str, failed: bool) -> String {
        serde_json::json!({ "runId": run, "score": 1, "total": 2, "failed": failed }).to_string()
    }

    // what this catches: the resume seam's whole contract — the verb must find the
    // NEWEST unfinished run for THIS benchmark, treat a completed run as done, and
    // treat a failed-row run as resumable (an infra death is a pause, not an end).
    #[test]
    fn resume_picks_the_newest_unfinished_run_for_the_benchmark() {
        let text = [
            task_row("run-a", "mirrorcode.jsonl", "t1"),
            run_row("run-a", false),
            task_row("run-b", "ds-1000.jsonl", "t1"),
            task_row("run-c", "mirrorcode.jsonl", "t1"),
            task_row("run-c", "mirrorcode.jsonl", "t2"),
        ]
        .join("\n");
        // Newest mirrorcode run is run-c (run-b is another gym), and it has no
        // terminal row → resumable.
        assert_eq!(
            latest_round_run_id(&text, "mirrorcode.jsonl").as_deref(),
            Some("run-c")
        );
        assert!(!run_completed(&text, "run-c"), "no terminal row = paused");
        // run-a completed → NOT resumable.
        assert!(run_completed(&text, "run-a"));
        // A failed terminal row is a pause, not completion.
        let failed = [task_row("run-d", "g.jsonl", "t1"), run_row("run-d", true)].join("\n");
        assert!(
            !run_completed(&failed, "run-d"),
            "failed:true must stay resumable"
        );
        // Unknown gym → nothing to resume.
        assert_eq!(latest_round_run_id(&text, "nope.jsonl"), None);
    }

    // what this catches (live, 2026-08-24, runs 4ae8e14c + 912de04f): a round's first
    // task grades ~30min after dispatch; until then the ledger has no rows and the
    // resume scan sees nothing, so a re-invoked verb minted a DUPLICATE fresh run.
    // The gate must answer from DISPATCH-time state: same benchmark in flight →
    // report that run; a different one → refuse loudly (evals serialize, a second
    // dispatch silently queues for hours); nothing in flight → dispatch.
    #[test]
    fn gate_reports_same_benchmark_refuses_other_dispatches_when_free() {
        let in_flight = vec![(
            "run-live".to_string(),
            "atlas".to_string(),
            "mirrorcode.jsonl".to_string(),
        )];
        assert_eq!(
            gate_on_in_flight(&in_flight, "mirrorcode.jsonl"),
            Some(Ok("run-live".to_string())),
            "same benchmark in flight = THE round — never a sibling"
        );
        match gate_on_in_flight(&in_flight, "ds-1000.jsonl") {
            Some(Err(msg)) => assert!(
                msg.contains("run-live"),
                "refusal must name the holding run: {msg}"
            ),
            other => panic!("a different round holding the arena must refuse, got {other:?}"),
        }
        assert_eq!(
            gate_on_in_flight(&[], "mirrorcode.jsonl"),
            None,
            "nothing in flight = free to dispatch"
        );
    }
}
