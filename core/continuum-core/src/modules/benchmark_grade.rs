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
/// through a live citizen (prefer "Benchy") — the same handle `benchmark/dispatch` uses.
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
    // Author/read through a live citizen (prefer Benchy), same fallback as curator_airc.
    let rt = registry
        .get_by_agent_name("Benchy")
        .or_else(|| registry.iter().next())
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

    // Only SWE-INSTANCE benchmarks grade through this path (repo + held-out tests). A gym
    // card ([bench frontier-rs] …) has a different grader and swe_dataset() is None.
    let Some(dataset) = known_benchmarks()
        .iter()
        .find(|b| b.name == bench)
        .and_then(|s| s.swe_dataset())
    else {
        return Ok(());
    };

    let owner = owner.ok_or_else(|| format!("bench card {card_id} has no owner — nobody worked it"))?;
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
    airc.say(&msg).await.map_err(|e| format!("post verdict: {e}"))?;
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
}
