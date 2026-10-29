//! Event-driven SWE grade-on-done — closes the kanban benchmark loop.
//!
//! Subscribes to `work.card.state_changed` (emitted by `work/state`, see
//! [`crate::persona::work_board::events::WORK_CARD_STATE_CHANGED`]) and, when a benchmark SWE card
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

use crate::commands::benchmark::{grade_swe, SweGradeParams};
use crate::recipe::benchmark_catalog::known_benchmarks;
use crate::persona::work_board::events::WORK_CARD_STATE_CHANGED;
use crate::persona::PersonaAircRuntimeRegistry;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};

/// States that mean "she's done — grade it". `work/state` maps done|closed → Closed and
/// accepts merged; a bench card reaching any of these is ready for the oracle. An
/// in_progress/review transition must NOT fire a grade. ONE definition of "terminal",
/// shared with the round tracker (`cognition::bench_round`) — the grader and the round
/// lifecycle must never disagree about doneness.
fn is_terminal(state: &str) -> bool {
    crate::cognition::bench_round::is_terminal_card_state(state)
}

use crate::recipe::bench_title::parse_card_title as parse_bench_title;

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
            // EVENT-DRIVEN for the grade itself: react to the card-transition event.
            event_subscriptions: &[WORK_CARD_STATE_CHANGED],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: Some(std::time::Duration::from_secs(180)),
        }
    }

    async fn tick(&self) -> Result<(), String> {
        // No desk sweep: a hand-in is graded when it is submitted (`grade_submission`,
        // fired by work/submit), never by scanning citizens' workspaces on a clock.
        // THE VERDICT LAW on the tick (card 52842311): verdicts on file settle the
        // cards the board left open — review-parked parents, unpulled review cards.
        crate::modules::verdict_board::reconcile_verdicted().await;
        Ok(())
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // No boot reconciliation of desks: grading is of hand-ins (`grade_submission`,
        // fired by work/submit). The desk sweep graded every staged copy in every
        // citizen's workspace, including citizens who chose no benchmark (2026-10-05).
        // THE live wiring. `config().event_subscriptions` installs a SYNCHRONOUS-tier
        // subscription the registry marks `synchronous: false` — which `publish()`
        // filters OUT, and runtime.rs:99 says so out loud: "event_subscriptions are
        // dispatched only by the (currently unused) synchronous publish path — if
        // this module expects live bus events, spawn a bus-receiver task from
        // initialize() instead". Grade-on-done shipped on the dead tier and NEVER
        // fired in production (proven live 2026-08-15: two bridged publishes, zero
        // handle_event probes). This is the prescribed receiver task, same shape as
        // chat::spawn_persist_listener.
        let mut rx = ctx.bus.receiver();
        let registry = self.registry.clone();
        ctx.runtime.spawn(async move {
            loop {
                let event = match rx.recv().await {
                    Ok(e) => e,
                    // Lagged (slow consumer): a missed transition re-fires on the
                    // next state change; the board is the durable truth, not the bus.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                };
                if event.name != WORK_CARD_STATE_CHANGED {
                    continue;
                }
                // ONE subscription, two reactions (#371): advance the round lifecycle
                // (bench.round.* transition probes), then grade the card. The round
                // tracker is pure sync state — safe to call inline before the grade
                // spawns.
                crate::cognition::bench_round::observe_card_event(&event.payload);
                on_card_state_changed(&registry, &event.payload);
            }
        });
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        Err(format!("benchmark_grade has no commands: {command}"))
    }

    async fn handle_event(&self, event_name: &str, payload: Value) -> Result<(), String> {
        // Kept for the synchronous tier should it ever dispatch — the LIVE path is
        // the bus-receiver task spawned in `initialize` (see the comment there).
        if event_name == WORK_CARD_STATE_CHANGED {
            crate::cognition::bench_round::observe_card_event(&payload);
            on_card_state_changed(&self.registry, &payload);
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// React to one card-state transition: terminal state → spawn the grade against the
/// card's OWN room. Non-terminal / malformed payloads are silently not-ours. Spawns
/// because grading is minutes long (clone + venv + pytest) and the caller is a bus
/// consumer loop that must keep draining.
fn on_card_state_changed(registry: &PersonaAircRuntimeRegistry, payload: &Value) {
    let state = payload
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !is_terminal(state) {
        return;
    }
    let card_id = payload
        .get("card_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if card_id.is_empty() {
        return;
    }
    // The card's room, from the wire event (bridge payload contract). Boards are
    // per-room: without this the grade read whatever room the grading citizen
    // happened to be in — the #345 wrong-room trap, hit live 2026-08-15.
    let room_id = payload
        .get("room_id")
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<uuid::Uuid>().ok());

    let registry = registry.clone();
    tokio::spawn(async move {
        continuum_base::probe!(
            class = "benchmark_grade.start",
            card_id = %card_id,
            room_id = %room_id.map(|r| r.to_string()).unwrap_or_default(),
            "grade-on-done fired — reading the card's own room board"
        );
        if let Err(e) = grade_card(&registry, &card_id, room_id).await {
            continuum_base::probe!(
                class = "benchmark_grade.error",
                card_id = %card_id,
                error = %e,
                "grade could not run"
            );
            tracing::warn!(card = %card_id, "benchmark_grade: {e}");
        }
    });
}

/// Read the card, and — if it is a bench SWE card — grade her workspace against the
/// held-out oracle and post the verdict into the room.
///
/// `room_id` is the card's OWN room from the wire event. Boards are per-room, so the
/// read scopes there and the verdict posts there — never to `current_room()`, which
/// is whichever room the grading citizen happens to sit in (#345's wrong-room trap,
/// hit live 2026-08-15: the grader read academy's board, never found the bench card,
/// and parked forever with no receipt).
async fn grade_card(
    registry: &PersonaAircRuntimeRegistry,
    card_id: &str,
    room_id: Option<uuid::Uuid>,
) -> Result<(), String> {
    // Author/read through a live citizen — whoever this machine has online (never a
    // hardcoded name), the same deterministic pick curator_airc uses.
    let rt = registry
        .any_live_citizen()
        .ok_or("no live citizen to author the grade through")?;
    let airc = rt.airc().clone();

    // Resolve the card's room from the citizen's subscription set (same pattern as
    // work.rs::claim_following_card_room). A room we can't resolve is a LOUD error —
    // grading against a guessed board is exactly the silent-wrong-room failure this
    // parameter exists to kill.
    let room_id = room_id.ok_or("event carried no room_id — cannot scope the board read")?;
    let set = airc
        .subscription_set()
        .await
        .map_err(|e| format!("subscription set: {e}"))?;
    let room = set
        .all()
        .map(|sub| sub.as_room())
        .find(|r| r.channel.as_uuid() == room_id)
        .ok_or_else(|| {
            format!("grading citizen is not subscribed to the card's room {room_id}")
        })?;

    let board = airc
        .work_board_in(&room)
        .await
        .map_err(|e| format!("board read: {e}"))?
        .snapshot();

    // The event carries whatever id the citizen passed to work/state (short 8-char or full).
    // Normalize to simple hex; a short id is a prefix of the full simple form.
    let want = card_id.replace('-', "").to_ascii_lowercase();
    let (title, owner, graded_card) = {
        let card = board
            .cards
            .iter()
            .find(|c| c.card_id.as_uuid().simple().to_string().starts_with(&want))
            .ok_or_else(|| format!("card {card_id} not on the board"))?;
        (card.title.clone(), card.owner, card.card_id.as_uuid())
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
        // A gym suite, registered or a coursework lesson set: the ONE resolver turns the
        // title's suite name into the eval set the oracle is read from.
        if let Some(reference) = crate::cognition::eval_set::eval_set_reference(&bench) {
            return grade_gym_card(&airc, &reference, &instance, owner, &bench, room_id, graded_card).await;
        }
        return Ok(()); // catalogued-but-not-runnable, or a bench name we don't know
    };

    let owner =
        owner.ok_or_else(|| format!("bench card {card_id} has no owner — nobody worked it"))?;
    // The staged checkout: <home>/citizens/peers/<owner>/workspace/swe/<instance>, exactly
    // where benchmark/swe-setup put it. Graded in a FRESH clone, so this tree is READ only.
    let workspace = crate::paths::continuum_home()
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

    // Post the verdict into the CARD'S room as a participant — the run room the
    // citizens are standing in, not the grading citizen's current room.
    continuum_base::probe!(
        class = "benchmark_grade.verdict",
        card_id = %card_id,
        room_id = %room_id,
        resolved = verdict.resolved,
        "SWE grade complete — posting verdict into the card's room"
    );
    // The room line and the card move live beside `record_verdict` now
    // (`modules::verdict_board::follow`) so the sweep and the operator paths say the same
    // thing this path did; an errored verdict is an ABSENCE and posts nothing.
    if let Some(err) = &verdict.error {
        let msg = format!("🧪 [bench {bench}] {instance} — grade could not run (infra, not a score): {err}");
        crate::persona::airc_citizen::publish_text_in_room(&airc, room_id, &msg)
            .await
            .map_err(|e| format!("post verdict: {e}"))?;
    }
    Ok(())
}

/// Grade a HAND-IN: the patch a citizen submitted on a SWE bench card, graded the moment she
/// submits it. A grader grades what was handed in, never a desk (Joel and Cormac,
/// 2026-10-05: "read the nouns literally"). Before this, a 180 s sweep graded every staged
/// copy in every citizen's workspace, so it kept diffing Kimi's autopilot-staged copies
/// though she had chosen no benchmark (385 unreadable diffs in 50 minutes on her desk).
///
/// The patch is read back from the artifact store by the hash she published, so the grade
/// is exactly her submission even if her checkout moved on. `grade_swe` grades it in a
/// fresh clone at the base, records the verdict and follows it onto the board, as every
/// grade path does. Not a bench card, or not a SWE one: nothing to grade, returns quietly.
pub(crate) async fn grade_submission(
    airc: std::sync::Arc<airc_lib::Airc>,
    room_id: uuid::Uuid,
    card_title: String,
    instance: String,
    patch: airc_blobs::MediaRef,
) {
    let Some((bench, dataset)) = crate::recipe::benchmark_catalog::submission_oracle_dataset(&card_title) else {
        return;
    };
    let patch_hash = patch.hash.to_hex();
    let patch = match crate::media::artifact::read_text_blob(&patch) {
        Ok(p) => p,
        Err(why) => {
            continuum_base::probe!(
                class = "benchmark.grade.submission_unreadable",
                instance = %instance,
                hash = %patch_hash,
                why = %why,
                "the submitted patch could not be read back from the artifact store — not graded"
            );
            return;
        }
    };
    continuum_base::probe!(class = "benchmark.grade.submission", instance = %instance, room_id = %room_id, "grading a hand-in");
    match grade_swe(SweGradeParams {
        instance: instance.clone(),
        dataset: Some(dataset.to_string()),
        gold: None,
        patch: Some(patch),
        workspace: None,
    })
    .await
    {
        Ok(verdict) => {
            if let Some(err) = &verdict.error {
                // The refusal record the desk sweep used to write: an env fault stands so
                // card selection and import skip what this box cannot grade.
                crate::cognition::swe_verdict_sweep::record_refusal(&instance, err, None);
                let msg = format!("🧪 [bench {bench}] {instance} — grade could not run (infra, not a score): {err}");
                if let Err(e) = crate::persona::airc_citizen::publish_text_in_room(&airc, room_id, &msg).await {
                    continuum_base::probe!(
                        class = "benchmark.grade.submission_unposted",
                        instance = %instance,
                        error = %e,
                        "the infra-error line for a hand-in could not be posted to its room"
                    );
                }
            }
        }
        Err(e) => continuum_base::probe!(
            class = "benchmark.grade.submission_failed",
            instance = %instance,
            error = ?e,
            "grading the hand-in failed before a verdict"
        ),
    }
}

/// Resolve one gym task by id and normalize it with the SAME `require_hands_for_code`
/// rule dispatch applies before composing the card — so the `solution_file` this grader
/// reads is byte-for-byte the path the card told her to write. Pure over the embedded
/// gym text; the unit test below pins the agreement.
pub(crate) fn normalized_gym_task(
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

/// The gym arm's verdict, as a pure value — so what gets BANKED is testable without a
/// room, a citizen, or a compiler run. Extracted the day gym grades became durable: the
/// interesting half of that change is the record's CONTENT (a fraction the board can
/// render, a failing test it can name, the adapter and the citizen it belongs to), and
/// content built inline inside an async grader is content nothing ever asserts on.
fn gym_verdict(
    task_id: &str,
    bench: &str,
    owner: &str,
    passed: bool,
    detail: &str,
) -> crate::cognition::swe_bench::SweVerdict {
    crate::cognition::swe_bench::SweVerdict {
        instance_id: task_id.to_string(),
        resolved: passed,
        // A gym task is ONE held-out suite: it passed or it did not. 1/1 vs 0/1 keeps the
        // board's fraction meaningful instead of rendering 0/0 on every row.
        f2p_passed: usize::from(passed),
        f2p_total: 1,
        // No regression suite exists for a gym task — 0/0, honestly empty, never a
        // fabricated pass count.
        p2p_passed: 0,
        p2p_total: 0,
        // No pristine-tree gate to fail: the task ships with no solution, so a pass always
        // distinguishes real work from a no-op.
        gate_ok: true,
        // A verdict must TEACH, not just score: name the suite that failed and carry the
        // compiler/assertion output a reviewer would paste, capped so a page of rustc
        // cannot bloat the store.
        failed_tests: if passed {
            Vec::new()
        } else {
            vec![task_id.to_string()]
        },
        failure_excerpt: if passed {
            None
        } else {
            Some(detail.chars().take(2000).collect())
        },
        benchmark: bench.to_string(),
        solver: owner.to_string(),
        ..Default::default()
    }
}

/// Grade a GYM bench card (`[bench frontier-rs] calc_pow: …`) from the file the owner's
/// hands wrote. The task resolves from the SAME embedded gym `benchmark/dispatch` loaded,
/// normalized by the SAME `require_hands_for_code` rule — so the path graded here is
/// byte-for-byte the path the card told her to write. Verdict is posted into the room;
/// a missing/empty artifact is an ABSENCE message (never a silent skip or fake fail),
/// same honesty rule as the SWE arm's infra line. A workspace task (`dod_shell`) is graded
/// by running its definition of done, then its held-out check, in her workspace.
async fn grade_gym_card(
    airc: &std::sync::Arc<airc_lib::Airc>,
    reference: &str,
    task_id: &str,
    owner: Option<impl std::fmt::Display>,
    bench: &str,
    room_id: uuid::Uuid,
    graded_card: uuid::Uuid,
) -> Result<(), String> {
    let owner = owner
        .ok_or_else(|| format!("bench card {task_id} has no owner — nobody worked it"))?
        .to_string();
    let task = normalized_gym_task(reference, task_id)?;
    // Her workspace root — the same layout every other per-citizen path uses.
    let workspace = crate::persona::card_checkout::gym_workspace(
        &crate::paths::continuum_home().map_err(|e| format!("{e:?}"))?,
        owner.parse().map_err(|e| format!("invalid gym owner {owner}: {e}"))?,
    );
    // The graded OUTCOME is carried as a value, and the room line is rendered FROM it.
    // It used to be the other way round: the only typed thing was the prose, and the
    // probe recovered the score by reading its own emoji back out of the message. A fact
    // that exists only inside a sentence cannot be recorded, projected, or learned from —
    // which is exactly how four real gym passes on this box left no trace anywhere but
    // chat ([[stdout-is-never-a-transport]]).
    //
    // `None` is an ABSENCE, not a zero: an unwritten or empty artifact, or a grader that
    // could not run, means nobody was measured, and it must never be banked as a failed
    // attempt — the same rule `record_verdict` already enforces for errored SWE verdicts.
    let (outcome, msg): (Option<(bool, String)>, String) = if let Some(dod) = task.dod_shell.clone() {
        // A WORKSPACE task (the eval's order: a definition of done supersedes `test`): the
        // card told her the DoD, and the grade runs it, then the held-out check the card
        // never showed, through the same composition the eval grades with. Before this arm
        // a dod_shell card closed with no verdict at all, as if it were expect-graded.
        use crate::cognition::eval::DodVerdict;
        let verdict = crate::cognition::eval::run_workspace_grade(Some(&workspace), &task, &dod).await;
        // the DoD's output can run pages; the room gets the head, enough to act on
        let head: String = verdict.message().chars().take(600).collect();
        match verdict {
            DodVerdict::Pass(m) => (
                Some((true, m)),
                format!("\u{2705} [bench {bench}] {task_id} RESOLVED — the definition of done passed in your workspace. Nice work."),
            ),
            DodVerdict::Fail(m) => (
                Some((false, m)),
                format!("\u{274c} [bench {bench}] {task_id} not resolved — {head}"),
            ),
            DodVerdict::InfraError(_) => (
                None,
                format!("\u{1f9ea} [bench {bench}] {task_id} — the grader could not run here, so this is not scored: {head}"),
            ),
        }
    } else if let Some(test) = task.test.clone() {
        let lang = task.lang.clone().unwrap_or_else(|| "rust".to_string()); // unwrap_or_else: a row with no lang is rust, the gym's documented default
        let solution_file = task
            .solution_file
            .clone()
            .unwrap_or_else(|| format!("{}.rs", task.id)); // unwrap_or_else: require_hands_for_code's derived name, the path the card told her
        let path = workspace.join(&solution_file);
        let outcome: Option<(bool, String)> = match std::fs::read_to_string(&path) {
            Ok(code) if !code.trim().is_empty() => {
                Some(crate::genome::gym_grader::test_grade(&code, &lang, &test).await)
            }
            Ok(_) | Err(_) => None,
        };
        let msg = match &outcome {
            Some((true, _)) => format!(
                "\u{2705} [bench {bench}] {task_id} RESOLVED — `{solution_file}` compiled and \
                 passed the held-out tests. Nice work."
            ),
            Some((false, detail)) => {
                // rustc output can run pages; the room gets the head, enough to act on.
                let head: String = detail.chars().take(600).collect();
                format!("\u{274c} [bench {bench}] {task_id} not resolved — `{solution_file}`: {head}")
            }
            None if path.exists() => format!(
                "\u{1f9ea} [bench {bench}] {task_id} — `{solution_file}` exists but is EMPTY; \
                 nothing to grade."
            ),
            None => format!(
                "\u{1f9ea} [bench {bench}] {task_id} — no `{solution_file}` in the owner's workspace. \
                 The card is graded on the file her hands write (code/write); it was never written."
            ),
        };
        (outcome, msg)
    } else {
        return Ok(()); // expect-graded knowledge task — nothing file-shaped to grade
    };

    // DURABLE, through the ONE seam every other verdict passes: the record stamps harness
    // build, served model and grade time, refuses to let a later failure erase a recorded
    // resolve, and settles the card's staged training credit.
    if let Some((passed, detail)) = &outcome {
        let verdict = gym_verdict(task_id, bench, &owner, *passed, detail);
        // A record that cannot be written is LOUD. Dropping it quietly would restore the
        // exact invisibility this call exists to end.
        crate::cognition::swe_bench::record_card_verdict(&verdict, graded_card)
            .await
            .map_err(|e| format!("record gym verdict for {task_id}: {e}"))?;
    }

    continuum_base::probe!(
        class = "benchmark_grade.verdict",
        card_id = %task_id,
        room_id = %room_id,
        benchmark = %bench,
        solver = %owner,
        // TYPED, off the grade — never recovered from the message text.
        resolved = matches!(outcome, Some((true, _))),
        recorded = outcome.is_some(),
        "gym grade complete — verdict recorded, posting into the card's room"
    );
    crate::persona::airc_citizen::publish_text_in_room(airc, room_id, &msg)
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

    /// The half of grade-on-done that had no assertions until gym grades became durable:
    /// what actually gets BANKED. Nested per the one-mod rule.
    mod durable_gym_grades {
        use super::*;

        // what this catches: a gym PASS must bank a row the board can render as a score —
        // resolved, a real 1/1 fraction, and the two identity fields (which adapter, whose
        // hands). Regression for the three months of rounds that reached `done` with cards
        // genuinely resolved while `benchmark/runs` reported 0: the grade existed only as
        // an emoji in a chat line, so every projection over the verdict store saw nothing.
        #[test]
        fn a_gym_pass_banks_a_row_the_board_can_score() {
            let v = gym_verdict("conway_step", "games-rs", "7711fe60-dead-beef-cafe-000000000001", true, "");
            assert!(v.resolved, "a pass is banked as resolved");
            assert_eq!(
                (v.f2p_passed, v.f2p_total),
                (1, 1),
                "a gym task is one held-out suite — 1/1, never 0/0, or the board renders \
                 a scored row as an empty fraction"
            );
            assert!(v.gate_ok, "no pristine gate exists to fail for a gym task");
            assert!(v.failed_tests.is_empty(), "a pass names no failing test");
            assert!(v.failure_excerpt.is_none(), "a pass carries no failure output");
            assert_eq!(v.benchmark, "games-rs", "the ADAPTER is a field, not an inference from the id's shape");
            assert_eq!(
                v.solver, "7711fe60-dead-beef-cafe-000000000001",
                "a capability row that cannot name the citizen it scored is not a capability row"
            );
        }

        // what this catches: a FAIL must teach, not just score zero. The failing suite is
        // named and the compiler/assertion output rides along capped — a bare `resolved:
        // false` is indistinguishable downstream from an absence, which is the ambiguity
        // SweVerdict::error exists to prevent.
        #[test]
        fn a_gym_failure_names_the_suite_and_carries_its_output() {
            let detail = "e".repeat(5_000);
            let v = gym_verdict("sum_evens", "hard-rs", "peer-1", false, &detail);
            assert!(!v.resolved);
            assert_eq!((v.f2p_passed, v.f2p_total), (0, 1), "a failure is 0 of 1, not 0 of 0");
            assert_eq!(v.failed_tests, vec!["sum_evens".to_string()], "the verdict names what failed");
            let excerpt = v.failure_excerpt.expect("a failure carries its output");
            assert_eq!(excerpt.chars().count(), 2_000, "capped — a page of rustc never bloats the store");
            assert!(excerpt.starts_with("eee"), "the cap takes the HEAD, where the first error is");
        }
    }
}
