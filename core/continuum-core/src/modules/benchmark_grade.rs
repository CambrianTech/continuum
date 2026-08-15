//! Event-driven SWE grade-on-done — closes the kanban benchmark loop.
//!
//! Subscribes to `work.card.state_changed` (emitted by `work/state`, see
//! [`crate::modules::work::WORK_CARD_STATE_CHANGED`]) and, when a benchmark SWE card
//! (`[bench swe-*]`) reaches a terminal state, grades the citizen's workspace diff against
//! the HELD-OUT oracle and posts the verdict into the room. The whole system is
//! event-based, never polling ([[the-whole-system-is-event-based-not-polling]]): nothing
//! scans the board on a clock — the transition event fires the grade.
//!
//! This is the REACT half of the benchmark adapter (stage → she works her full loop →
//! done → grade her artifact). The grade runs `grade_swe` in a FRESH clone at
//! `base_commit`, so a dirtied workspace can never launder a pass — which is what makes
//! full grid-backed capacity + teams + free communication all FAIR: she may use everything
//! she is, but the oracle stays held out
//! ([[exams-are-taken-with-full-grid-backed-capacity-disclosed]]).

use std::any::Any;

use async_trait::async_trait;
use serde_json::Value;

use crate::commands::benchmark::{grade_swe, known_benchmarks, SweGradeParams};
use crate::modules::work::WORK_CARD_STATE_CHANGED;
use crate::persona::PersonaAircRuntimeRegistry;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};

/// States that mean "she's done — grade it". `work/state` maps done|closed → Closed and
/// accepts merged; a bench card reaching any of these is ready for the oracle. An
/// in_progress/review transition must NOT fire a grade.
fn is_terminal(state: &str) -> bool {
    matches!(
        state.to_ascii_lowercase().as_str(),
        "closed" | "done" | "merged"
    )
}

/// Parse `[bench <name>] <instance>: <gist>` — the exact shape `dispatch_card_title`
/// writes — into `(bench_name, instance_id)`. `None` for any non-bench title, so a normal
/// work card silently isn't graded.
fn parse_bench_title(title: &str) -> Option<(String, String)> {
    let rest = title.strip_prefix("[bench ")?;
    let (bench, after) = rest.split_once("] ")?;
    let instance = after.split(':').next()?.trim();
    if bench.trim().is_empty() || instance.is_empty() {
        return None;
    }
    Some((bench.trim().to_string(), instance.to_string()))
}

/// The grade-on-done subscriber. Holds a persona-airc registry so it can author the grade
/// through a live citizen — whoever this machine has online (never a hardcoded name), the
/// same `any_live_citizen` pick `benchmark/dispatch`'s curator uses.
pub struct BenchmarkGradeModule {
    registry: PersonaAircRuntimeRegistry,
}

impl BenchmarkGradeModule {
    pub fn new(registry: PersonaAircRuntimeRegistry) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl ServiceModule for BenchmarkGradeModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "benchmark_grade",
            priority: ModulePriority::Normal,
            command_prefixes: &[],
            // EVENT-DRIVEN: react to the card-transition event. No tick, no board poll.
            event_subscriptions: &[WORK_CARD_STATE_CHANGED],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        Err(format!("benchmark_grade has no commands: {command}"))
    }

    async fn handle_event(&self, event_name: &str, payload: Value) -> Result<(), String> {
        if event_name != WORK_CARD_STATE_CHANGED {
            return Ok(());
        }
        let state = payload
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !is_terminal(state) {
            return Ok(());
        }
        let card_id = payload
            .get("card_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if card_id.is_empty() {
            return Ok(());
        }

        // Grading is minutes long (clone + venv + pytest). The bus calls handle_event
        // INLINE during publish, so blocking here would hang the citizen's work/state
        // verb. Spawn it — fire-and-observe, never block-on-client (the long-jobs rule).
        let registry = self.registry.clone();
        tokio::spawn(async move {
            if let Err(e) = grade_card(&registry, &card_id).await {
                tracing::warn!(card = %card_id, "benchmark_grade: {e}");
            }
        });
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Read the card, and — if it is a bench SWE card — grade her workspace against the
/// held-out oracle and post the verdict into the room.
async fn grade_card(registry: &PersonaAircRuntimeRegistry, card_id: &str) -> Result<(), String> {
    // Author/read through a live citizen — whoever this machine has online (never a
    // hardcoded name), the same deterministic pick curator_airc uses.
    let rt = registry
        .any_live_citizen()
        .ok_or("no live citizen to author the grade through")?;
    let airc = rt.airc().clone();

    let board = airc
        .work_board_complete(airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
        .await
        .map_err(|e| format!("board read: {e}"))?
        .snapshot();

    // The event carries whatever id the citizen passed to work/state (short 8-char or full).
    // Normalize to simple hex; a short id is a prefix of the full simple form.
    let want = card_id.replace('-', "").to_ascii_lowercase();
    let (title, owner) = {
        let card = board
            .cards
            .iter()
            .find(|c| c.card_id.as_uuid().simple().to_string().starts_with(&want))
            .ok_or_else(|| format!("card {card_id} not on the board"))?;
        (card.title.clone(), card.owner)
    };

    let Some((bench, instance)) = parse_bench_title(&title) else {
        return Ok(()); // not a bench card — nothing to grade
    };

    // SWE-INSTANCE benchmarks grade via a fresh clone + the repo's held-out tests; every
    // other RUNNABLE benchmark is a GYM (embedded task collection) whose grade reads the
    // FILE her hands wrote. Both fire on the same done-transition. Until the gym arm below
    // existed, gym cards had NO grader at all — a perfect artifact could never resolve,
    // and the board sat at 0-resolved while kickoffs promised a grade (2026-08-15).
    let spec = known_benchmarks().iter().find(|b| b.name == bench);
    let Some(dataset) = spec.and_then(|s| s.swe_dataset()) else {
        if let Some(spec) = spec.filter(|s| s.eval_set.is_some()) {
            return grade_gym_card(&airc, spec, &instance, owner, &bench).await;
        }
        return Ok(()); // catalogued-but-not-runnable, or a bench name we don't know
    };

    let owner =
        owner.ok_or_else(|| format!("bench card {card_id} has no owner — nobody worked it"))?;
    // The staged checkout: <home>/citizens/peers/<owner>/workspace/swe/<instance>, exactly
    // where benchmark/swe-setup put it. Graded in a FRESH clone, so this tree is READ only.
    let workspace = crate::commands::benchmark::continuum_home()
        .map_err(|e| format!("{e:?}"))?
        .join("citizens")
        .join("peers")
        .join(owner.to_string())
        .join("workspace")
        .join("swe")
        .join(&instance);

    let verdict = grade_swe(SweGradeParams {
        instance: instance.clone(),
        dataset: Some(dataset.to_string()),
        gold: None,
        patch: None,
        workspace: Some(workspace.display().to_string()),
    })
    .await
    .map_err(|e| format!("grade_swe: {e:?}"))?;

    let msg = if let Some(err) = &verdict.error {
        // An errored verdict is an ABSENCE, not a zero — say so, never a fake fail.
        format!("🧪 [bench {bench}] {instance} — grade could not run (infra, not a score): {err}")
    } else if verdict.resolved {
        format!(
            "✅ [bench {bench}] {instance} RESOLVED — {}/{} FAIL_TO_PASS + {}/{} PASS_TO_PASS passed. Nice work.",
            verdict.fail_to_pass_passed,
            verdict.fail_to_pass_total,
            verdict.pass_to_pass_passed,
            verdict.pass_to_pass_total,
        )
    } else {
        let failing = if verdict.failed_tests.is_empty() {
            String::new()
        } else {
            format!(" — still failing: {}", verdict.failed_tests.join(", "))
        };
        format!(
            "❌ [bench {bench}] {instance} not resolved — {}/{} FAIL_TO_PASS passed{}",
            verdict.fail_to_pass_passed, verdict.fail_to_pass_total, failing
        )
    };

    // Post the verdict into the room as a participant (slice 2 posts to the authoring
    // citizen's room; per-run bench-room targeting is #329/#346 slice 3).
    airc.say(&msg)
        .await
        .map_err(|e| format!("post verdict: {e}"))?;
    Ok(())
}

/// Resolve one gym task by id and normalize it with the SAME `require_hands_for_code`
/// rule dispatch applies before composing the card — so the `solution_file` this grader
/// reads is byte-for-byte the path the card told her to write. Pure over the embedded
/// gym text; the unit test below pins the agreement.
fn normalized_gym_task(
    reference: &str,
    task_id: &str,
) -> Result<crate::cognition::eval::EvalTask, String> {
    let (origin, text) = crate::cognition::gym::resolve_gym(reference)?;
    let mut task = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str::<crate::cognition::eval::EvalTask>(l).ok())
        .find(|t| t.id == task_id)
        .ok_or_else(|| format!("task '{task_id}' not found in {origin}"))?;
    task.require_hands_for_code();
    Ok(task)
}

/// Grade a GYM bench card (`[bench frontier-rs] calc_pow: …`) from the file the owner's
/// hands wrote. The task resolves from the SAME embedded gym `benchmark/dispatch` loaded,
/// normalized by the SAME `require_hands_for_code` rule — so the path graded here is
/// byte-for-byte the path the card told her to write. Verdict is posted into the room;
/// a missing/empty artifact is an ABSENCE message (never a silent skip or fake fail),
/// same honesty rule as the SWE arm's infra line.
async fn grade_gym_card(
    airc: &std::sync::Arc<airc_lib::Airc>,
    spec: &crate::commands::benchmark::BenchmarkSpec,
    task_id: &str,
    owner: Option<impl std::fmt::Display>,
    bench: &str,
) -> Result<(), String> {
    let owner = owner
        .ok_or_else(|| format!("bench card {task_id} has no owner — nobody worked it"))?
        .to_string();
    let reference = spec
        .eval_set
        .ok_or_else(|| format!("gym benchmark '{bench}' has no eval_set"))?;
    let task = normalized_gym_task(reference, task_id)?;
    let Some(test) = task.test.clone() else {
        return Ok(()); // expect-graded knowledge task — nothing file-shaped to grade
    };
    let lang = task.lang.clone().unwrap_or_else(|| "rust".to_string());
    let solution_file = task
        .solution_file
        .clone()
        .unwrap_or_else(|| format!("{}.rs", task.id));
    // Her workspace root — the same layout every other per-citizen path uses.
    let path = crate::commands::benchmark::continuum_home()
        .map_err(|e| format!("{e:?}"))?
        .join("citizens")
        .join("peers")
        .join(&owner)
        .join("workspace")
        .join(&solution_file);
    let msg = match std::fs::read_to_string(&path) {
        Ok(code) if !code.trim().is_empty() => {
            let (passed, detail) =
                crate::cognition::gym_grader::test_grade(&code, &lang, &test).await;
            if passed {
                format!(
                    "✅ [bench {bench}] {task_id} RESOLVED — `{solution_file}` compiled and \
                     passed the held-out tests. Nice work."
                )
            } else {
                // rustc output can run pages; the room gets the head, enough to act on.
                let head: String = detail.chars().take(600).collect();
                format!("❌ [bench {bench}] {task_id} not resolved — `{solution_file}`: {head}")
            }
        }
        Ok(_) => format!(
            "🧪 [bench {bench}] {task_id} — `{solution_file}` exists but is EMPTY; \
             nothing to grade."
        ),
        Err(_) => format!(
            "🧪 [bench {bench}] {task_id} — no `{solution_file}` in the owner's workspace. \
             The card is graded on the file her hands write (code/write); it was never written."
        ),
    };
    airc.say(&msg)
        .await
        .map_err(|e| format!("post verdict: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the title parse must extract (bench, instance) from the EXACT
    // shape dispatch_card_title writes, and reject non-bench titles. Drift here silently
    // stops grading every dispatched card.
    #[test]
    fn parses_bench_title_and_rejects_non_bench() {
        let (b, i) =
            parse_bench_title("[bench swe-bench-lite] sympy__sympy-24152: fix the thing").unwrap();
        assert_eq!(b, "swe-bench-lite");
        assert_eq!(i, "sympy__sympy-24152");
        assert!(parse_bench_title("a normal work card").is_none());
        assert!(parse_bench_title("[bench frontier-rs] task-1: gist").is_some());
    }

    // what this catches: only terminal states fire the grade. An in_progress/review
    // transition must NOT trigger a grade — the citizen isn't done.
    #[test]
    fn only_terminal_states_grade() {
        assert!(is_terminal("done") && is_terminal("closed") && is_terminal("merged"));
        assert!(!is_terminal("in_progress") && !is_terminal("claimed") && !is_terminal("review"));
    }

    // what this catches: the grader and the dispatched card must agree on the artifact
    // path — BOTH must run the task through require_hands_for_code. Regression for the
    // 2026-08-15 0-resolved incident: gym rows carry no solution_file, dispatch composed
    // cards from the raw task (no file named) while nothing graded on done at all, so a
    // citizen was graded against a path she was never told (task #439). This resolves a
    // REAL embedded gym task the way grade_gym_card does and pins the derived name and
    // harness presence; if a gym row ever names solution_file explicitly, that name wins
    // in both places by the same rule, so the agreement holds either way.
    #[test]
    fn gym_grade_reads_the_same_artifact_path_the_card_names() {
        let task = normalized_gym_task("frontier-rs.jsonl", "edit_distance")
            .expect("frontier-rs edit_distance must resolve from the embedded gym");
        let file = task
            .solution_file
            .as_deref()
            .expect("a test-graded task must have a derived solution_file after normalization");
        assert_eq!(file, "sol_edit_distance.rs");
        assert!(
            task.test.is_some(),
            "frontier-rs tasks are compile-and-run graded — the harness must survive load"
        );
        assert!(
            task.prompt.contains(file),
            "the normalized prompt must ASK for the file the grade reads"
        );
    }
}
