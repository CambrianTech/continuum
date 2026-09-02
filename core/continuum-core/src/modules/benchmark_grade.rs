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

use airc_lib::CardState;

use crate::commands::benchmark::{grade_swe, known_benchmarks, SweGradeParams};
use crate::modules::work::WORK_CARD_STATE_CHANGED;
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
            // EVENT-DRIVEN for the grade itself: react to the card-transition event.
            event_subscriptions: &[WORK_CARD_STATE_CHANGED],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            // The one periodic ACTUATOR (doctrine: actuators may tick; condition-polls
            // may not): lease expiry is a TIME fact with no wire event, so a bench card
            // whose owner wrote the artifact but whose work session died before `done`
            // would rot as Claimed forever. The sweep detects exactly that state and
            // CLOSES the card — the close's wire echo then drives the normal
            // bridge→grade tail. One grade path; the sweeper is only a detector.
            tick_interval: Some(std::time::Duration::from_secs(180)),
        }
    }

    async fn tick(&self) -> Result<(), String> {
        sweep_lapsed_bench_cards(&self.registry).await
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // RECONCILE the artifacts already on disk, once, as this module comes up.
        //
        // Grading belongs to the benchmark recipe — it IS the activity's outcome score
        // (docs/architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md), so it is owned HERE,
        // by the module that owns every other grade path, and never by some unrelated daemon's
        // boot sequence. (Written after doing exactly that and being corrected: a sweep hung
        // off `serving_daemon` start is the parallel-runner shape this repo has a whole
        // document forbidding. Joel: "Grading is supposed to be part of the regular benchmark
        // recipe".)
        //
        // Why a reconciliation exists at all, next to two event paths that are both correct:
        // the grade-on-done subscriber fires on a card TRANSITION, and the tick sweep detects
        // a LAPSED lease. Neither can see an artifact with no card — detached `agent/solve`
        // runs (#425) produce exactly that, and 17 unscored citizen patches were sitting on
        // this box the night it was written, two of them PASSES over a day old. This is the
        // same reap-or-adopt boot owns for every other resource (#452): an orphaned ARTIFACT
        // is an orphaned run.
        //
        // Deterministic and idempotent (see the sweep's module doc), so it is a reconciliation
        // and not the forbidden condition-poll: it enumerates ALL staged instances in sorted
        // order with no cap and no recency sort, skips anything already carrying a verdict, and
        // refuses ambiguity rather than guessing. Same disk, same outcome, every time.
        //
        // DETACHED because each grade is a fresh clone plus a real test suite — minutes apiece.
        // Module init must not block on it, and the citizens' first turn must not queue behind
        // it.
        tokio::spawn(async {
            let report = crate::cognition::swe_verdict_sweep::sweep().await;
            if report.graded > 0 {
                tracing::info!(
                    graded = report.graded,
                    resolved = report.resolved,
                    "benchmark artifact reconciliation scored citizen work that had no verdict"
                );
            }
        });
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
        // A5 BOOT RESUME: rejoin surviving Working rounds once serving + residency
        // are back — the benchmark side owns re-firing (the serving daemon's reap
        // refuses to, correctly). One-shot; the card-settled edge chains onward.
        crate::modules::benchmark_resume::spawn_boot_resume(self.registry.clone());

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
    // A4 FOLLOW-ON — the card-settled edge of the ONE driver decision
    // (bench_round::next_unworked_after; dispatch and boot-resume are the other
    // two edges). A settled card frees serving capacity; the round's next
    // unworked card fires NOW instead of starving behind the initial
    // solve_cap = lanes (dispatched − solves_fired stops being permanent).
    // The in-flight guard inside dispatch_staged_swe_solve makes double-fire
    // impossible even if two settle events race.
    if let Ok(settled_uuid) = card_id.parse::<uuid::Uuid>() {
        if let Some(next) = crate::cognition::bench_round::next_unworked_after(settled_uuid) {
            let reg = registry.clone();
            tokio::spawn(async move {
                let airc = reg
                    .get(next.assignee)
                    .or_else(|| reg.any_live_citizen())
                    .map(|rt| rt.airc().clone());
                let Some(airc) = airc else {
                    crate::probe!(
                        class = "bench.round.follow_on_blocked",
                        card_id = %next.card,
                        "next card is due but no live citizen can carry the dispatch —                          the boot-resume edge retries"
                    );
                    return;
                };
                crate::probe!(
                    class = "bench.round.follow_on",
                    settled = %settled_uuid,
                    next_card = %next.card,
                    assignee = %next.assignee,
                    "card settled — firing the round's next unworked card"
                );
                crate::modules::work::dispatch_staged_swe_solve(
                    &Default::default(),
                    &airc,
                    crate::modules::work::StagedSolveDispatch {
                        claimer: crate::identity::PeerId::from_uuid(next.assignee),
                        card: airc_work::WorkCardId::from_uuid(next.card),
                        room: airc_core::RoomId::from_u128(next.run_room.as_u128()),
                        // The card's RECORDED team rides every driver edge (gap 3).
                        teammates: next
                            .teammates
                            .iter()
                            .map(|t| crate::identity::PeerId::from_uuid(*t))
                            .collect(),
                    },
                )
                .await;
            });
        }
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
        crate::probe!(
            class = "benchmark_grade.start",
            card_id = %card_id,
            room_id = %room_id.map(|r| r.to_string()).unwrap_or_default(),
            "grade-on-done fired — reading the card's own room board"
        );
        if let Err(e) = grade_card(&registry, &card_id, room_id).await {
            crate::probe!(
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
            return grade_gym_card(&airc, spec, &instance, owner, &bench, room_id).await;
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

    // Post the verdict into the CARD'S room as a participant — the run room the
    // citizens are standing in, not the grading citizen's current room.
    crate::probe!(
        class = "benchmark_grade.verdict",
        card_id = %card_id,
        room_id = %room_id,
        resolved = verdict.resolved,
        "SWE grade complete — posting verdict into the card's room"
    );
    crate::persona::airc_citizen::publish_text_in_room(&airc, room_id, &msg)
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
    room_id: uuid::Uuid,
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
    crate::probe!(
        class = "benchmark_grade.verdict",
        card_id = %task_id,
        room_id = %room_id,
        resolved = msg.starts_with('✅'),
        "gym grade complete — posting verdict into the card's room"
    );
    crate::persona::airc_citizen::publish_text_in_room(airc, room_id, &msg)
        .await
        .map_err(|e| format!("post verdict: {e}"))?;
    Ok(())
}

/// Most closes a single sweep will perform. A burst of lapsed cards (e.g. a whole round's
/// worth after a long outage) grades a few per tick instead of storming the room; the rest
/// are still lapsed next tick.
const SWEEP_MAX_CLOSES_PER_TICK: usize = 3;

/// How long after a claim LAPSES a still-resident owner is presumed to be
/// mid-resume rather than done-and-gone. A reboot lapses every lease at once and
/// the resume machinery renews them on the claim cadence (TTL/3); within this
/// window a resident owner may still be catching up, so her artifact is left to
/// her. Beyond it, a lapsed claim on a resident owner means she is NOT renewing =
/// NOT actively working the card (an actively-worked card stays `Held`), so her
/// done-but-unclosed artifact must be graded, not stranded forever. Sized well
/// above a worst-case reboot+resume (the 35B lane's build+load alone is ~10min)
/// and far below the multi-hour wedge this un-strands (measured 2026-09-02:
/// rounds stalled ~20h with artifacts present, owner resident, never graded).
const RESIDENT_OWNER_GRACE_MS: u64 = 30 * 60 * 1000;

/// A resident owner with a LAPSED claim is presumed "still working" only inside
/// the post-lapse resume window. Pure so the boundary is unit-testable.
fn resident_owner_still_working(lapsed_for_ms: u64) -> bool {
    lapsed_for_ms < RESIDENT_OWNER_GRACE_MS
}

/// The sweep decision, pure so the truth table is unit-testable: a bench card is
/// auto-closeable exactly when a person CLAIMED it, their lease LAPSED (work session
/// died — `card_holder::hold_of`, the one lease predicate, #357), and their hands left
/// a real artifact behind. A LIVE claim is never preempted; an unclaimed card was never
/// worked; no artifact means there is nothing to grade — dispatch re-offer (#419) is
/// that card's path, not a grade.
fn sweep_ready(
    state: &CardState,
    hold: crate::persona::card_holder::Hold,
    artifact_present: bool,
) -> bool {
    matches!(state, CardState::Claimed | CardState::InProgress)
        && matches!(hold, crate::persona::card_holder::Hold::Lapsed)
        && artifact_present
}

/// Did the owner's hands leave something gradeable? Mirrors the grade arms' own path
/// derivations exactly — gym: the task's `solution_file` exists non-empty under her
/// workspace root; SWE: the staged checkout exists AND the tree is dirty (an untouched
/// clone graded would burn a fake capability-zero for a dead session, the #384 class).
fn bench_artifact_present(bench: &str, instance: &str, owner: &str) -> bool {
    let Some(spec) = known_benchmarks().iter().find(|b| b.name == bench) else {
        return false;
    };
    let Ok(home) = crate::commands::benchmark::continuum_home() else {
        return false;
    };
    let workspace = home
        .join("citizens")
        .join("peers")
        .join(owner)
        .join("workspace");
    if spec.swe_dataset().is_some() {
        let tree = workspace.join("swe").join(instance);
        if !tree.is_dir() {
            return false;
        }
        return std::process::Command::new("git")
            .arg("-C")
            .arg(&tree)
            .args(["status", "--porcelain"])
            .output()
            .map(|o| o.status.success() && !o.stdout.is_empty())
            .unwrap_or(false);
    }
    let Some(reference) = spec.eval_set else {
        return false;
    };
    let Ok(task) = normalized_gym_task(reference, instance) else {
        return false;
    };
    if task.test.is_none() {
        return false; // expect-graded knowledge task — nothing file-shaped to sweep
    }
    let solution_file = task
        .solution_file
        .clone()
        .unwrap_or_else(|| format!("{}.rs", task.id));
    std::fs::read_to_string(workspace.join(solution_file))
        .map(|code| !code.trim().is_empty())
        .unwrap_or(false)
}

/// The algorithm of the operator's manual flip (2026-08-15, the first graded pass):
/// a citizen writes the artifact but her work session dies before she says `done`,
/// so the card rots as a lapsed claim and the grade never runs. The sweep detects
/// exactly that state and CLOSES the card as her would-have-been `done` — the close's
/// wire echo then drives the normal bridge→grade tail. ONE grade path; this is only
/// a detector. A citizen who says `done` herself beats the sweeper (card goes
/// terminal, sweep skips it); a citizen still working is never preempted (Held).
async fn sweep_lapsed_bench_cards(
    registry: &PersonaAircRuntimeRegistry,
) -> Result<(), String> {
    // Boot / no citizens online yet: quietly nothing to do — same posture as grade_card,
    // but a tick must not error every 3 minutes of a citizen-less core.
    let Some(rt) = registry.any_live_citizen() else {
        return Ok(());
    };
    let airc = rt.airc().clone();
    let set = airc
        .subscription_set()
        .await
        .map_err(|e| format!("subscription set: {e}"))?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis() as u64;

    let mut closed = 0usize;
    for room in set.all().map(|sub| sub.as_room()) {
        // Rooms without a board (or with a transiently unreadable one) are not sweep
        // targets this tick; the board is durable, next tick sees it.
        let Ok(board) = airc.work_board_in(&room).await else {
            continue;
        };
        for card in &board.snapshot().cards {
            if closed >= SWEEP_MAX_CLOSES_PER_TICK {
                return Ok(());
            }
            let Some((bench, instance)) = parse_bench_title(&card.title) else {
                continue; // normal work cards are NEVER the sweeper's business
            };
            let Some(owner) = card.owner else { continue };
            let hold = crate::persona::card_holder::hold_of(card, now_ms);
            if !sweep_ready(
                &card.state,
                hold,
                bench_artifact_present(&bench, &instance, &owner.to_string()),
            ) {
                continue;
            }
            // A RESIDENT owner is not a dead session — but only for a BOUNDED
            // window after the claim lapses. A reboot lapses every lease while the
            // resume machinery brings the owner back, and confiscating her
            // half-done work in that window grades a MISS she was still earning
            // (measured 2026-08-31: both of the day's misses were exactly this —
            // reboot → lease lapsed → swept mid-resume, "a few attempts" never
            // happened). BUT deferring FOREVER strands done-but-unclosed work: a
            // resident owner who wrote the patch and let the claim lapse without
            // saying `done` never returns to it, so the card rots and the round
            // never finishes (measured 2026-09-02: rounds stalled ~20h with
            // artifacts present, owner resident, ungraded — the pileup of
            // unstarted standing rounds traced straight here). An actively-worked
            // card keeps its claim `Held` via the TTL/3 renewal cadence, so a
            // LAPSED claim past the resume window means she is NOT working it.
            // Within the grace: hers to finish. Past it: grade her artifact.
            let lapsed_for_ms = card
                .claim_expires_at_ms
                .map(|e| now_ms.saturating_sub(e))
                .unwrap_or(u64::MAX); // no expiry recorded = not renewing = gradeable
            if registry.get(owner.as_uuid()).is_some()
                && resident_owner_still_working(lapsed_for_ms)
            {
                crate::probe!(
                    class = "benchmark_grade.sweep_deferred_owner_online",
                    card_id = %card.card_id.as_uuid(),
                    owner = %owner.as_uuid(),
                    bench = %bench,
                    instance = %instance,
                    lapsed_ms = lapsed_for_ms,
                    "lapsed claim with artifact, owner RESIDENT and within the resume \
                     grace — left to her, not confiscated"
                );
                continue;
            }
            let room_id = room.channel.as_uuid();
            crate::probe!(
                class = "benchmark_grade.sweep_close",
                card_id = %card.card_id.as_uuid(),
                room_id = %room_id,
                bench = %bench,
                instance = %instance,
                "lapsed claim with a written artifact — auto-closing so the grade can run"
            );
            // Close FIRST, provenance note only on success: a persistently-failing close
            // must not post a note into the room every tick (the first live tick failed
            // ALL 21 closes silently — 21 probes in 0.6s straight past the 3-close cap,
            // which only counts successes; without an error probe the whole failure mode
            // was invisible). The note still lands before the verdict — grading takes
            // seconds, the note posts immediately after the close.
            // Room-SCOPED mutate (airc #1363): the close targets the room the card
            // actually lives in. The current-room verb refused all 21 of the first
            // tick's closes — every target card was in a room the authoring citizen
            // wasn't standing in (the WRITE half of the #345 class).
            match airc
                .change_work_card_state_in(
                    &room,
                    airc_lib::ChangeWorkCardState {
                        card_id: card.card_id,
                        state: CardState::Closed,
                    },
                )
                .await
            {
                Ok(_) => {
                    closed += 1;
                    let note = format!(
                        "⏱️ [bench {bench}] {instance} — the claim lapsed with a written \
                         artifact and no `done`; auto-closed so the grade can run. (A live \
                         claim is never preempted.)"
                    );
                    if let Err(e) =
                        crate::persona::airc_citizen::publish_text_in_room(&airc, room_id, &note)
                            .await
                    {
                        crate::probe!(
                            class = "benchmark_grade.sweep_note_failed",
                            card_id = %card.card_id.as_uuid(),
                            error = %e,
                            "card closed but the provenance note did not post"
                        );
                    }
                }
                Err(e) => {
                    crate::probe!(
                        class = "benchmark_grade.sweep_close_failed",
                        card_id = %card.card_id.as_uuid(),
                        room_id = %room_id,
                        error = %e,
                        "auto-close refused — card stays as-is, retried next tick"
                    );
                }
            }
        }
    }
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

    // what this catches: the sweep's whole safety envelope in one truth table — a LIVE
    // claim is never preempted, an unclaimed/terminal card is never touched, a lapsed
    // claim without an artifact is dispatch's re-offer problem (#419) not a grade, and
    // ONLY lapsed+claimed+artifact auto-closes. Drift here either preempts working
    // citizens or resurrects the manual operator flip.
    #[test]
    fn sweep_ready_truth_table() {
        use crate::persona::card_holder::Hold;
        // the one auto-close case (and its InProgress sibling)
        assert!(sweep_ready(&CardState::Claimed, Hold::Lapsed, true));
        assert!(sweep_ready(&CardState::InProgress, Hold::Lapsed, true));
        // a live claim is NEVER preempted
        assert!(!sweep_ready(&CardState::Claimed, Hold::Held, true));
        // no artifact → nothing to grade → not the sweeper's business
        assert!(!sweep_ready(&CardState::Claimed, Hold::Lapsed, false));
        // never-claimed and terminal cards are untouchable regardless
        assert!(!sweep_ready(&CardState::Open, Hold::Unclaimed, true));
        assert!(!sweep_ready(&CardState::Closed, Hold::Lapsed, true));
        assert!(!sweep_ready(&CardState::Merged, Hold::Lapsed, true));
    }

    // what this catches: the resident-owner defer is BOUNDED to the resume
    // window, so done-but-unclosed work stops stranding rounds forever. regression
    // for the 2026-09-02 stall: rounds wedged ~20h with an artifact present and a
    // resident owner whose claim had lapsed hours earlier — the sweep deferred
    // every tick because the owner was resident, so the round never finished and
    // standing rounds piled up unstarted. Inside the grace she is left alone;
    // just past it her artifact grades.
    #[test]
    fn resident_owner_defer_is_bounded_to_the_resume_window() {
        // fresh lapse (mid-reboot-resume) → still hers, not confiscated
        assert!(resident_owner_still_working(0));
        assert!(resident_owner_still_working(RESIDENT_OWNER_GRACE_MS - 1));
        // past the window → not renewing = not working → gradeable
        assert!(!resident_owner_still_working(RESIDENT_OWNER_GRACE_MS));
        // the measured 20h wedge is decisively past the window
        assert!(!resident_owner_still_working(20 * 60 * 60 * 1000));
        // the window must clear a worst-case 35B reboot+resume (~10min) with margin
        assert!(RESIDENT_OWNER_GRACE_MS >= 15 * 60 * 1000);
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
