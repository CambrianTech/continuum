use super::*;

/// What a run's OWN grade says should happen to its claim card. One decision,
/// one place, testable without a disk or a board — the five outcomes were
/// previously five interleaved booleans computed inline in an async fn with
/// side effects, which is why the cached-env conflation (#2531) survived
/// unnoticed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CardCloseDecision {
    /// She produced no diff. The env was fine; nothing to score.
    NoPatch,
    /// The cached env could not be re-pointed at THIS grader's tree. Nothing
    /// was measured here, but the instance's environment is NOT absent.
    CacheRepointFailed,
    /// No grade on disk at all — infra died before grading.
    NoVerdict,
    /// The environment is genuinely absent, or the control is broken
    /// (`gate_ok == false`: the fail-to-pass tests already passed pristine).
    EnvAbsent,
    /// A real verdict. This run closes its card.
    Close,
}

/// Classify a run's `grade.json` for the terminal-close decision.
///
/// Order matters: the two "the env was fine, we just measured nothing" cases
/// (`NoPatch`, `CacheRepointFailed`) are checked BEFORE the generic
/// any-error-means-absent rule, because that rule is a catch-all and would
/// otherwise swallow them — which is exactly the bug #2531 records.
pub(super) fn classify_grade_for_close(grade: Option<&serde_json::Value>) -> CardCloseDecision {
    let Some(g) = grade else {
        return CardCloseDecision::NoVerdict;
    };
    let error = g.get("error").and_then(|e| e.as_str());
    // "No candidate patch" is NOT an env absence — the env was fine and she
    // simply produced no diff (measured 2026-08-27: the env label sent these
    // through the env-retake path with a message promising "fires when the env
    // heals", which was false and confusing in the log).
    if error.is_some_and(|e| e.contains("no candidate patch")) {
        return CardCloseDecision::NoPatch;
    }
    // A CACHED-env re-point failure is not an environment absence. The env is
    // cached per instance and each grader re-points its editable install at ITS
    // OWN tree (solve → the citizen's dirty workspace; benchmark_grade → a
    // pristine clone + patch), so a patch that breaks the build backend fails
    // the solve's re-point while the fresh clone grades fine seconds later —
    // measured on django-11734, where this path declared an environment absent
    // that had just run 40/40 pass-to-pass. A cache is an optimization; its
    // failure must never become a conclusion about the world (#2531).
    if error.is_some_and(|e| e.contains(crate::cognition::swe_bench::ENV_CACHE_REPOINT_FAILED)) {
        return CardCloseDecision::CacheRepointFailed;
    }
    // A failed GATE is env-class: the fail-to-pass tests already passed on the
    // pristine tree, so the control is broken and nothing was measured — the
    // durable layer already refuses such verdicts (record_verdict), and the
    // card must stay open for the retake the same as any env absence.
    let gate_failed = g.get("gate_ok").is_some_and(|k| k == false);
    if gate_failed || g.get("error").is_some_and(|e| !e.is_null()) {
        return CardCloseDecision::EnvAbsent;
    }
    CardCloseDecision::Close
}

/// Fire the round's next card the MOMENT a run ends without settling its own.
///
/// The settle edge (`benchmark_grade`'s follow-on) only fires on a card-state
/// change, so an honest non-settling outcome — no diff, env absent, a cached env
/// that would not re-point — left the round motionless until the 5-minute
/// becalmed watchdog happened to notice. That is advancement by TIMEOUT; the
/// fact is known right here, so act on it here.
///
/// The just-finished card is excluded by the decision itself
/// (`next_unworked_excluding`), which is what keeps this from re-firing the card
/// that just failed in a tight loop. Best-effort by design: no live citizen
/// simply means the watchdog picks it up on its next pass, exactly as before.
pub(super) async fn advance_round_after_non_settling(run_id: &str) {
    let Some(card) = run_id
        .strip_prefix("claim-")
        .and_then(|s| Uuid::parse_str(s).ok())
    else {
        return; // not a claim-dispatched run — it owns no card to advance from
    };
    let Some(next) = crate::cognition::bench_round::next_unworked_excluding(card) else {
        return; // round finished, or every remaining card is already in flight
    };
    let Some(reg) = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
    else {
        return;
    };
    let Some(airc) = reg
        .get(next.assignee)
        .or_else(|| reg.any_live_citizen())
        .map(|rt| rt.airc().clone())
    else {
        crate::probe!(
            class = "bench.round.advance_blocked",
            card_id = %next.card,
            "the next card is due after a non-settling run, but no live citizen can \
             carry the dispatch — the becalmed watchdog remains the backstop"
        );
        return;
    };
    crate::probe!(
        class = "bench.round.advanced",
        finished = %card,
        next_card = %next.card,
        assignee = %next.assignee,
        "a run ended WITHOUT settling its card — firing the round's next card now \
         instead of waiting out the becalmed watchdog"
    );
    crate::modules::work::dispatch_staged_swe_solve(
        &Default::default(),
        &airc,
        crate::modules::work::StagedSolveDispatch {
            claimer: crate::identity::PeerId::from_uuid(next.assignee),
            card: airc_work::WorkCardId::from_uuid(next.card),
            room: airc_core::RoomId::from_u128(next.run_room.as_u128()),
            // The card's RECORDED team rides every driver edge — dropping it here
            // silently converted a team round into a solo round on re-dispatch.
            teammates: next
                .teammates
                .iter()
                .map(|t| crate::identity::PeerId::from_uuid(*t))
                .collect(),
        },
    )
    .await;
}

/// The A3 terminal close: a claim-dispatched run (`run_id == claim-<card uuid>`)
/// whose final attempt produced a REAL verdict closes its card, authored through
/// the run's own persona (subscribed to the run room by dispatch). Non-claim runs
/// and verdict-less runs (infra/ungradeable) leave the card as-is.
pub(super) async fn close_claim_card_if_graded(run_id: &str) {
    let Some(card_uuid) = run_id
        .strip_prefix("claim-")
        .and_then(|s| Uuid::parse_str(s).ok())
    else {
        return; // not a claim-dispatched run — no card to close
    };
    let Some(ledger) = agent_solve_ledger_path(run_id) else {
        return;
    };
    let grade_path = ledger.with_extension("grade.json");
    let grade = std::fs::read_to_string(&grade_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok()); // disk boundary: reading the grade.json verdict file the grader wrote
    let decision = classify_grade_for_close(grade.as_ref());
    let env_absent = decision == CardCloseDecision::EnvAbsent;
    let verdict_is_real = decision == CardCloseDecision::Close;
    let no_patch = decision == CardCloseDecision::NoPatch;
    let cache_repoint_failed = decision == CardCloseDecision::CacheRepointFailed;
    if no_patch {
        crate::probe!(
            class = "benchmark.card.close_skipped",
            run_id = %run_id,
            reason = "no_patch",
            "she produced no diff — card stays open for a retake; not an env absence"
        );
        advance_round_after_non_settling(run_id).await;
        return;
    }
    if cache_repoint_failed {
        crate::probe!(
            class = "benchmark.card.close_skipped",
            run_id = %run_id,
            reason = "cache_repoint_failed",
            "the cached env could not be re-pointed at THIS tree, so this grader \
             measured nothing — NOT an env absence (the instance's env is healthy; \
             a sibling grader on a pristine clone may already have scored it). No \
             parking brake: this run simply does not close the card"
        );
        advance_round_after_non_settling(run_id).await;
        return;
    }
    if !verdict_is_real && !env_absent {
        crate::probe!(
            class = "benchmark.card.close_skipped",
            run_id = %run_id,
            "no verdict on disk at all (infra died before grading) — this run does \
             not close the card, so a resume re-fires it (the owed retake)"
        );
        advance_round_after_non_settling(run_id).await;
        return;
    }
    if env_absent {
        // Defer this instance for the rest of the boot: a broken control or
        // absent env yields the SAME result on every retake, and each retake
        // burns a full solve of her time (measured: sphinx-10614, two complete
        // 30-minute attempts against a gate that cannot pass). The per-boot
        // defer set the dispatch gate already consults is exactly the right
        // parking brake; the next boot's prewarm re-tests honestly.
        if let Some(inst) = grade
            .as_ref()
            .and_then(|g| g.get("instance"))
            .and_then(|i| i.as_str())
        {
            crate::modules::work::env_broken_this_boot().insert(inst.to_string());
        }
        // The harness measured the environment and it is ABSENT — the card
        // stays OPEN so the retake is AUTOMATIC the boot the env heals (Joel
        // 2026-08-27: "you'd want to get it possible to run the broken ones
        // after a restart"). The infinite-loop hazard this used to carry is
        // killed at the dispatch seam instead: dispatch skips instances whose
        // env failed THIS boot's prewarm, so a still-broken card idles quietly
        // and a healed one re-fires with zero operator action.
        crate::probe!(
            class = "benchmark.card.close_skipped",
            run_id = %run_id,
            reason = "env_absent",
            "env-absent verdict — this run does not close the card; the retake \
             fires itself on the first boot whose env pre-warm succeeds (if the \
             card is still open — a lapse sweeper may already have closed it)"
        );
        advance_round_after_non_settling(run_id).await;
        return;
    }
    // Author through the run's persona (her runtime is subscribed to the run
    // room); fall back to any live citizen — same authoring rule as the lapse
    // sweeper. No runtime at all → probe and leave it; the sweeper picks the
    // card up on its next tick because it is Claimed with an artifact.
    let persona = std::fs::read_to_string(&ledger)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok()) // disk boundary: reading the grade.json verdict file the grader wrote
        .and_then(|v| {
            v.get("persona_id")
                .and_then(|p| p.as_str())
                .and_then(|p| Uuid::parse_str(p).ok())
        });
    let Some(reg) = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
    else {
        return;
    };
    let airc = persona
        .and_then(|p| reg.get(p))
        .or_else(|| reg.any_live_citizen())
        .map(|rt| rt.airc().clone());
    let Some(airc) = airc else {
        crate::probe!(
            class = "benchmark.card.close_skipped",
            run_id = %run_id,
            "no live citizen to author the close — the lapse sweeper will close it"
        );
        return;
    };
    let card_id = airc_work::WorkCardId::from_uuid(card_uuid);
    let Some(room) = crate::modules::work::room_holding_card(&airc, card_id).await else {
        crate::probe!(
            class = "benchmark.card.close_skipped",
            run_id = %run_id,
            "no subscribed room's board holds the card — cannot place the close"
        );
        return;
    };
    match airc
        .change_work_card_state_in(
            &room,
            airc_lib::ChangeWorkCardState {
                card_id,
                state: airc_work::CardState::Closed,
            },
        )
        .await
    {
        Ok(_) => crate::probe!(
            class = "benchmark.card.closed_by_harness",
            run_id = %run_id,
            card_id = %card_uuid,
            "final verdict on disk — harness closed the card; the round settles on              this event (last card → Done)"
        ),
        Err(e) => crate::probe!(
            class = "benchmark.card.close_skipped",
            run_id = %run_id,
            error = %e.to_string(),
            "close refused — card stays as-is; the lapse sweeper retries"
        ),
    }
}

/// Why a watchdog halted an attempt: the ledger stopped advancing, or the far ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttemptHalt {
    /// No new act settled for the allowance — the wedge the watchdog exists to catch.
    Stalled { acts: u64, allowance: Duration },
    /// The attempt advanced the whole way and still outlived its ceiling.
    Ceiling { acts: u64, ceiling: Duration },
    /// The operator asked for it to stop (`benchmark/round-stop`): the flag reached the
    /// act loop at the next watchdog poll — not "the next task boundary" a solve never has.
    Cancelled { acts: u64 },
}

impl AttemptHalt {
    pub(crate) fn kind(self) -> &'static str {
        match self {
            AttemptHalt::Stalled { .. } => "stalled",
            AttemptHalt::Ceiling { .. } => "ceiling",
            AttemptHalt::Cancelled { .. } => "cancelled",
        }
    }
    pub(crate) fn acts(self) -> u64 {
        match self {
            AttemptHalt::Stalled { acts, .. }
            | AttemptHalt::Ceiling { acts, .. }
            | AttemptHalt::Cancelled { acts } => acts,
        }
    }
}

impl std::fmt::Display for AttemptHalt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttemptHalt::Stalled { acts, allowance } => write!(
                f,
                "stalled: its ledger did not advance past {acts} act(s) for {}s — fork/lane wedge",
                allowance.as_secs()
            ),
            AttemptHalt::Ceiling { acts, ceiling } => write!(
                f,
                "outlived its ceiling ({}s) still advancing at {acts} act(s) — bounded, not judged",
                ceiling.as_secs()
            ),
            AttemptHalt::Cancelled { acts } => write!(
                f,
                "cancelled by round-stop at {acts} act(s) — stopped, not judged"
            ),
        }
    }
}

/// The acts a run's ledger records right now, or `None` when there is no ledger or it
/// does not decode — an absence the watchdog treats as "no advancement observed", never
/// as a count.
pub(super) fn ledger_acts(path: Option<&std::path::Path>) -> Option<u64> {
    let text = std::fs::read_to_string(path?).ok()?;
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()?
        .get("acts")?
        .as_u64()
}

/// Run one attempt under an ADVANCEMENT watchdog: the attempt is halted only when its
/// ledger's `acts` has not moved for `stall_allowance` (the pulse writes it every minute
/// while she works), or when the far `ceiling` passes. A run that advances at any pace
/// is never judged stalled — the flat `max_acts × 8 min` timeout this replaces ended
/// every run on a box whose acts cost 21 min (2026-09-16, the M5's 16-mind roster) and
/// the resume pass re-fired each from act 1, forever.
pub(super) async fn attempt_with_advancement_watchdog<F>(
    ledger: Option<&std::path::Path>,
    stall_allowance: Duration,
    ceiling: Duration,
    cancelled: impl Fn() -> bool,
    attempt: F,
) -> Result<F::Output, AttemptHalt>
where
    F: std::future::Future,
{
    // Poll the ledger on a cadence well under the allowance; a missing ledger (an
    // attached call has none) means the ceiling is the only bound.
    const POLL: Duration = Duration::from_secs(60);
    let started = tokio::time::Instant::now();
    let mut last_acts = ledger_acts(ledger);
    let mut last_advance = started;
    tokio::pin!(attempt);
    loop {
        tokio::select! {
            out = &mut attempt => return Ok(out),
            _ = tokio::time::sleep(POLL) => {
                let now = tokio::time::Instant::now();
                let acts = ledger_acts(ledger);
                if acts.is_some() && acts != last_acts {
                    last_acts = acts;
                    last_advance = now;
                }
                let acts_n = last_acts.unwrap_or(0);
                // A stop request lands HERE (card 4558eef9): `benchmark/round-stop` used to
                // promise "the next task boundary", which a detached solve never reaches —
                // sklearn-25102 kept acting 22 → 26 for 25 minutes after the stop and its
                // steady hold kept the lane pinned. Dropping the attempt future releases the
                // hold and the lane with it.
                if cancelled() {
                    return Err(AttemptHalt::Cancelled { acts: acts_n });
                }
                if ledger.is_some() && now.duration_since(last_advance) >= stall_allowance {
                    return Err(AttemptHalt::Stalled { acts: acts_n, allowance: stall_allowance });
                }
                if now.duration_since(started) >= ceiling {
                    return Err(AttemptHalt::Ceiling { acts: acts_n, ceiling });
                }
            }
        }
    }
}

/// Result file for a detached solve run, polled after the ack (mirrors the eval/competition
/// progress-ledger convention: `~/.continuum/progress/agent-solve-<run_id>.json`).
///
/// Both the directory and the file name come from `cognition::swe_bench`, which is also where
/// the boot reaper and the reboot guard READ them. They were spelled out independently here
/// until 2026-08-18 and the names did not match, so neither reader ever saw a run this writer
/// produced ([`crate::cognition::swe_bench::SOLVE_LEDGER_PREFIX`] carries the measurement).
pub(super) fn agent_solve_ledger_path(run_id: &str) -> Option<std::path::PathBuf> {
    let dir = crate::cognition::swe_bench::solve_ledger_dir();
    let _ = std::fs::create_dir_all(&dir);
    Some(crate::cognition::swe_bench::solve_ledger_path(&dir, run_id))
}

/// Where THIS run's artifacts live — the patch above all. One definition, so a run's
/// evidence never depends on how it happened to be launched.
///
/// `capture_dir` is an OPTIONAL caller courtesy (a hand-launched run naming its own
/// folder). It was also, until 2026-08-18, the ONLY thing standing between an attempt
/// and total evidence loss: the patch write sat behind `if let Some(dir) =
/// capture_dir` with no else, so every run that did not pass one silently discarded
/// the diff. Measured that day: all 25 patches on this box live under
/// `benchmarks/swe/captures/run-*` — hand-launched runs. Every CITIZEN-dispatched run
/// (`claim-*`, i.e. the entire path the benchmark actually runs on) kept none. A
/// citizen wrote 41,166 bytes against sympy-13480, broke 40 previously-passing tests,
/// and the one artifact that could say whether that was a surgical edit or a clobber
/// was gone before anyone could read it.
///
/// So custody stops being a parameter. Absent an explicit dir, it derives from
/// [`swe_cache_dir`] — the benchmarks root, whose own doc says to read it from there and
/// never from a remembered path.
///
/// It lands in `benchmarks/swe/captures/run-<id>/`, which is EXACTLY where the 25
/// hand-launched patches already live. That is deliberate on two counts, and my first cut
/// got both wrong by inventing `progress/run-<id>/` instead (caught by Joel the same
/// hour):
///
/// 1. **One home per artifact class.** A second location for "her patch" is the parallel
///    allocator this codebase keeps paying for — the exact sin I had written down that
///    morning and then committed.
/// 2. **It must be a GOVERNED directory.** `benchmarks` is a registered `TrackedDir` with
///    a decided eviction story; `progress` is neither tracked nor decided, so patches
///    there would have been unbounded growth in an unmanaged dir — precisely what
///    CLAUDE.md's "no new cache dir without an eviction decision" rule exists to stop
///    (the 460 GB incident).
pub(super) fn run_artifact_dir(run_id: &str, capture_dir: Option<&str>) -> Option<std::path::PathBuf> {
    if let Some(d) = capture_dir {
        return Some(std::path::PathBuf::from(d));
    }
    Some(
        crate::cognition::swe_bench::swe_cache_dir()
            .join("captures")
            .join(format!("run-{run_id}")),
    )
}

/// Global admission gate for scored solve DRIVES — the fix for the lane-thrash death
/// (glass-boxed 2026-08-11, build 4627): with solves finally firing (dispatch auto-fire +
/// claim + durable-restore all launch `dispatch_staged_swe_solve`), MORE solves than the
/// box has serving lanes ran their generation phase at once and thrashed one llama-server
/// lane to a Connection-refused death mid-solve ("6 mid-relaunch retries exhausted — lane
/// never came back"). This caps concurrent solve drives at the live serving-lane budget:
/// the (lanes+1)th solve WAITS for a permit instead of oversubscribing and killing the
/// lane. The permit is held for the whole drive and released on drop — panic-safe, so a
/// stalled or panicking solve can never leak a slot. Sized ONCE at first use from the live
/// lane count (min 1); a later lane shrink is a v1 mismatch (rare, and lease_measured_work still
/// protects the KV prefix). One place, both triggers respect it, because every solve —
/// inline or detached, dispatch or claim — flows through `solve_body`.
/// [[measured-work-gets-an-exclusive-warm-slot-quiesce-others]]
pub(super) fn solve_admission() -> &'static tokio::sync::Semaphore {
    static SLOTS: std::sync::OnceLock<tokio::sync::Semaphore> = std::sync::OnceLock::new();
    SLOTS.get_or_init(|| {
        let lanes = crate::inference::llama_server::current_serving()
            .lanes
            .max(1) as usize;
        tokio::sync::Semaphore::new(lanes)
    })
}

impl AgentSolve {
    /// The solve body — deliberately ctx-free (reaches the persona via the global workspace
    /// registry), so it runs inline OR spawned detached with the same code path.
    /// A detached solve has nobody waiting on its first token: it renders the prompt an
    /// unattended turn can afford (card 7496ed9d) — every budget read inside sees it.
    pub(crate) async fn solve_body(p: AgentSolveParams) -> Result<AgentSolveResult, CommandError> {
        crate::cognition::audience::with(
            crate::inference::prefill_rate::Audience::Unattended,
            Self::solve_body_attended_by_nobody(p),
        )
        .await
    }
}
