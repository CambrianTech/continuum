//! `benchmark/runs`: the RunProjection (exam-room surface) — per-run cards folded from the work board, workspace artifacts and verdicts, with stall detection.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};
use crate::cognition::swe_bench;

// ---------------------------------------------------------------------------
// benchmark/runs — the RunProjection (exam-room surface, slice 2)
// ---------------------------------------------------------------------------

/// How long a non-terminal run may go without ANY artifact activity before the
/// projection flags it `stalled`. Matches the harness cadence expectation (a
/// healthy Devstral act lands every ~4-6 min; 20 min of silence is 3-4 missed
/// beats), deliberately TIGHTER than the in-loop deadline (#2180's
/// `max_acts × 15 min` kills the run) — the projection warns first, the
/// deadline executes later. Glass-boxed origin: 2026-08-08, two runs sat
/// silent 2.5h and the operator found out by asking.
pub(super) const RUN_STALL_WINDOW_SECS: u64 = 20 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRunsParams.ts"
)]
pub struct BenchmarkRunsParams {
    /// Filter to one run. Omit → the newest `limit` runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
    /// Newest N runs to return (default 20).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub limit: Option<u32>,
}

/// One run's card — the projection every consumer renders: the positron
/// exam-room tab bar, a teacher persona's grounding, and the operator's
/// liveness Monitor all fold THIS, never bespoke file scraping
/// (docs/architecture/ACADEMY-EXAM-ROOM-POSITRONIC-SURFACE.md §5.2).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchRunCard.ts"
)]
pub struct BenchRunCard {
    pub run_id: String,
    /// Instance under test ("sympy__sympy-24066") — from the result ledger's
    /// staged-checkout name (#329: the board names WHAT, not just who).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub instance: Option<String>,
    /// Attempt N of `max_attempts` — the N-chances counter, live per ledger write.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub attempt: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub max_attempts: Option<u32>,
    /// Solver persona (from the result ledger; absent while attempt 1 is
    /// still in flight and nothing has been written yet).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub solver: Option<String>,
    /// `resolved` | `failed` (loud infra marker, incl. #2180 stalls the
    /// deadline caught) | `active` (artifact activity within the stall
    /// window) | `queued` (silent past the window BUT the solver is actively
    /// working another run — one pair of hands; waiting a turn is not a
    /// stall) | `quiet` (non-terminal AND silent past the window — the
    /// shape the projection exists to make visible) | `ungraded` (a staged
    /// workspace holds a real diff that no grade has ever seen — durable
    /// work awaiting a verdict, NOT a stall; see
    /// [`scan_workspace_artifact_cards`]).
    pub phase: String,
    /// True exactly when `phase == "quiet"`.
    pub stalled: bool,
    /// Epoch ms of the newest artifact write (result or grade ledger).
    #[ts(type = "number")]
    pub last_activity_ms: u64,
    #[ts(type = "number")]
    pub age_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub acts: Option<u32>,
    pub files_changed: Vec<String>,
    /// Investigation trail (#2177) — reads/searches/edit attempts, not git.
    pub files_examined: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resolved: Option<bool>,
    /// "passed/total", e.g. "1/1" — string so the sparkline renders directly.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fail_to_pass: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pass_to_pass: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub patch_bytes: Option<u32>,
    pub failed_tests: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub infra_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRunsResult.ts"
)]
pub struct BenchmarkRunsResult {
    pub runs: Vec<BenchRunCard>,
    /// How many cards MATCHED before `limit` truncated — so a page is never mistaken
    /// for the whole set.
    ///
    /// Measured cost of not having this (2026-08-21, on me): `benchmark/runs` returned
    /// 20 rows carrying 1 resolved instance, and I read that as the system's whole
    /// history and as evidence that grades were being lost on the way to the board.
    /// Both wrong. The projection reads grade siblings AND durable verdicts correctly;
    /// there were simply 37 graded instances and `limit.unwrap_or(20)` showed the 20
    /// most recent. The SECOND real pass (`sympy__sympy-13480`) was older than the
    /// window, so the board looked like a 1-pass history and nobody knew otherwise.
    ///
    /// Same convention `debug/probes/query` already uses ("MATCHED versus returned, so
    /// a page is never mistaken for the whole"). Silent truncation reads as "that's
    /// everything" — on the command an operator uses to ask how the benchmark is going,
    /// that is the worst possible failure shape.
    #[ts(type = "number")]
    pub matched: u32,
    /// Human-readable statement of the two numbers, so the truncation is visible in a
    /// glance at the receipt and not only to a caller who compares two fields.
    pub summary: String,
}

/// One run-ledger scan: the cards a caller asked for, plus how many there were BEFORE
/// truncation.
///
/// Returned as a struct rather than a bare `Vec` so the count cannot be dropped on the
/// way out — the positron board consumes `.cards` and ignores the total, while the
/// command reports both. A tuple would have let either caller silently discard it.
pub(crate) struct RunScan {
    pub cards: Vec<BenchRunCard>,
    /// Cards that matched the filter before `limit` was applied.
    pub matched: usize,
}

/// Fold one run's on-disk ledgers into a card. Pure over the two JSON values +
/// mtime so the derivation is unit-testable without a filesystem.
pub(super) fn fold_run_card(
    run_id: &str,
    result: Option<&serde_json::Value>,
    grade: Option<&serde_json::Value>,
    last_activity_ms: u64,
    now_ms: u64,
) -> BenchRunCard {
    let s = |v: Option<&serde_json::Value>, k: &str| {
        v.and_then(|v| v.get(k))
            .and_then(|x| x.as_str())
            .map(String::from)
    };
    let n = |v: Option<&serde_json::Value>, k: &str| {
        v.and_then(|v| v.get(k))
            .and_then(|x| x.as_u64())
            .map(|x| x as u32)
    };
    let arr = |v: Option<&serde_json::Value>, k: &str| -> Vec<String> {
        v.and_then(|v| v.get(k))
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|e| e.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };
    // ABSENCE vs ZERO, on the board. A grade carrying `error` is a harness/env
    // fault the grader PROVED — for the env class it re-runs the PRISTINE tree
    // first, so a genuinely broken patch is never mislabelled — and
    // `SweGradeResult.error` documents the contract in its own doc comment: "a
    // result with `error` is an ABSENCE, not a zero, and must never be tallied
    // as a failed attempt." This projection honoured that for the RESULT's error
    // and ignored the GRADE's, so an env fault folded as `resolved: false` +
    // phase `failed` — indistinguishable from a citizen who tried and lost.
    // Measured 2026-08-13: 8 of 36 instances (22%) grade UNGRADEABLE on this box.
    // `infra_error` already means "no valid verdict, and why", so it takes both
    // sources rather than growing a second field, and `resolved` returns to None
    // — the same "no verdict" the pre-grade card carries, because that is the truth.
    let grade_error = s(grade, "error");
    let ungradeable = grade_error.is_some();
    let resolved = if ungradeable {
        None
    } else {
        grade
            .and_then(|g| g.get("resolved"))
            .and_then(|x| x.as_bool())
    };
    let infra_error = s(result, "infra_error")
        .or_else(|| s(result, "error"))
        .or(grade_error);
    let failed_marker = result
        .and_then(|r| r.get("failed"))
        .and_then(|x| x.as_bool())
        .unwrap_or(false);
    let age_secs = now_ms.saturating_sub(last_activity_ms) / 1000;
    let phase = if resolved == Some(true) {
        "resolved"
    } else if ungradeable {
        // Ahead of `failed`: a run can carry both a failed marker and an
        // ungradeable grade, and the absence is the more truthful of the two.
        "ungradeable"
    } else if failed_marker {
        "failed"
    } else if age_secs < RUN_STALL_WINDOW_SECS {
        "active"
    } else {
        "quiet"
    };
    let ratio = |g: Option<&serde_json::Value>, passed: &str, total: &str| match (
        n(g, passed),
        n(g, total),
    ) {
        (Some(p), Some(t)) => Some(format!("{p}/{t}")),
        _ => None,
    };
    BenchRunCard {
        run_id: run_id.to_string(),
        instance: s(result, "instance"),
        attempt: n(result, "attempt"),
        max_attempts: n(result, "max_attempts"),
        solver: s(result, "persona_id"),
        stalled: phase == "quiet",
        phase: phase.to_string(),
        last_activity_ms,
        age_secs,
        acts: n(result, "acts"),
        files_changed: arr(result, "files_changed"),
        files_examined: arr(result, "files_examined"),
        resolved,
        fail_to_pass: ratio(grade, "failToPassPassed", "failToPassTotal"),
        pass_to_pass: ratio(grade, "passToPassPassed", "passToPassTotal"),
        // Graded diff size when a grade exists; before any grade, the RESULT's
        // own patch length — the live "a patch is forming" leading indicator
        // (#329). One field, grade-authoritative, never both shown.
        patch_bytes: n(grade, "patchBytes").or_else(|| {
            result
                .and_then(|r| r.get("patch"))
                .and_then(|p| p.as_str())
                .map(|p| p.len() as u32)
        }),
        failed_tests: arr(grade, "failedTests"),
        infra_error,
    }
}


#[derive(Default)]
pub struct BenchmarkRuns;

#[async_trait]
impl ActionCommand for BenchmarkRuns {
    const NAME: &'static str = "benchmark/runs";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "The benchmark RunProjection: every agent/solve run's live card — phase \
         (active/quiet/resolved/failed/ungradeable), last-activity age, stall flag, acts, \
         grade summary, \
         investigation trail — folded from the run ledgers. ONE projection for every consumer: \
         the exam-room tab bar, a teacher persona's grounding, and the operator's liveness \
         monitor all read THIS instead of scraping files. `quiet` (stalled=true) is the shape \
         it exists to expose: a non-terminal run with no artifact activity past the stall \
         window — silence must never be ambiguous with progress.";
    type Params = BenchmarkRunsParams;
    type Output = BenchmarkRunsResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: BenchmarkRunsParams,
    ) -> Result<BenchmarkRunsResult, CommandError> {
        // safe: `limit` is an OPTIONAL page size, so None means "caller didn't ask", not
        // "unknown quantity" — 20 is this command's documented default page and the value
        // the board already uses. `.max(1)` keeps an explicit 0 from returning nothing
        // silently. The count that must never be defaulted is `matched`, which comes from
        // the scan itself and is reported separately.
        let scan = scan_run_cards(p.run_id.as_deref(), p.limit.unwrap_or(20).max(1) as usize) // safe: see the 5 lines above
            .map_err(CommandError::Internal)?;
        let returned = scan.cards.len();
        let matched = scan.matched;
        // Say which of the two this is, in words. A caller comparing `runs.len()` to
        // `matched` would also learn it, but the receipt is what a human (or a citizen
        // reading the board) actually looks at, and the whole point is that truncation
        // must not be invisible there.
        let summary = if returned < matched {
            format!(
                "showing {returned} of {matched} run(s) — NEWEST first, older runs truncated \
                 by `limit`. Raise `--limit` to see the rest; this is a PAGE, not the whole \
                 history."
            )
        } else {
            format!("all {matched} run(s) — this is the complete set for these filters, not a page.")
        };
        Ok(BenchmarkRunsResult {
            runs: scan.cards,
            matched: matched as u32,
            summary,
        })
    }
}

/// The ONE run-ledger scan behind every consumer: the `benchmark/runs`
/// command AND the positron `kind="bench"` board emitter (#329) fold THIS —
/// never a parallel file scrape ([[the-compression-principle]]). Synchronous
/// fs I/O: async callers wrap it in `spawn_blocking`.
/// Cards for staged workspaces that hold REAL WORK no grade has ever seen.
///
/// Why this source exists (glass-boxed 2026-08-18, and it is the acceptance test from
/// docs/architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md failing): the board read ONLY
/// `progress/agent-solve-*.json`. Those files are written by a solve PROCESS. A process that
/// froze mid-flight never writes `files_changed`, and work done another way never writes a
/// file at all — so the board showed 20 runs with `files_changed: []` and phase `quiet`,
/// while three staged trees on the same disk held real in-place source edits. Two of them
/// were PASSES (astropy-14995, pytest-11143, both `resolved=true` once the grader could read
/// its own output). They were found by hand with `git -C … diff`, which is precisely the
/// "if answering needs a file read, it is disconnected and it failed" the doc names.
///
/// The workspace is DURABLE truth; a run file is an ephemeral progress marker. So the board
/// projects both, and an artifact nobody graded is a first-class row rather than an absence.
/// Instances already carrying a grade are skipped — a graded run is the authoritative card.
///
/// Bounded on purpose: at most [`WORKSPACE_ARTIFACT_SCAN_CAP`] trees per call, and the count
/// dropped is logged rather than silently truncated (no-silent-caps).
fn scan_workspace_artifact_cards(graded: &std::collections::HashSet<String>, now_ms: u64) -> Vec<BenchRunCard> {
    let Ok(home) = crate::paths::continuum_home() else {
        return Vec::new();
    };
    let peers = home.join("citizens").join("peers");
    let Ok(peer_entries) = std::fs::read_dir(&peers) else {
        return Vec::new();
    };
    let mut cards = Vec::new();
    let mut scanned = 0usize;
    let mut skipped_over_cap = 0usize;
    for peer in peer_entries.flatten() {
        let peer_id = peer.file_name().to_string_lossy().to_string();
        let swe = peer.path().join("workspace").join("swe");
        let Ok(instances) = std::fs::read_dir(&swe) else {
            continue;
        };
        for inst in instances.flatten() {
            if !inst.path().is_dir() {
                continue;
            }
            let instance = inst.file_name().to_string_lossy().to_string();
            if graded.contains(&instance) {
                continue;
            }
            if scanned >= WORKSPACE_ARTIFACT_SCAN_CAP {
                skipped_over_cap += 1;
                continue;
            }
            scanned += 1;
            // The diff comes from the event-driven registry (card f860e59c) — the SAME
            // `workspace_candidate_diff` reading, computed by the registry's owner when a
            // write named this tree. NEVER spawned from here: this scan runs on the board's
            // clock, and git-on-a-clock is the defect that card removed. Untracked (outer
            // None) = the watcher has not computed it yet; it lands on a later tick.
            // Unreadable (inner None) = skipped, as the old inline `Err` was.
            let Some(Some(diff)) =
                crate::commands::workspace_artifacts::candidate_diff(&inst.path())
            else {
                continue;
            };
            if diff.trim().is_empty() {
                continue;
            }
            // Touched paths straight off the diff header — no extra process spawn.
            let files_changed: Vec<String> = diff
                .lines()
                .filter_map(|l| l.strip_prefix("+++ b/"))
                .map(|p| p.to_string())
                .collect();
            let last_activity_ms = std::fs::metadata(inst.path())
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| crate::time::EpochMs::of(t).ok())
                .map(crate::time::EpochMs::millis)
                .unwrap_or(0);
            cards.push(BenchRunCard {
                run_id: format!("workspace:{}:{instance}", &peer_id[..8.min(peer_id.len())]),
                instance: Some(instance),
                attempt: None,
                max_attempts: None,
                solver: Some(peer_id.clone()),
                // NOT "quiet": nothing is stalled here — a finished artifact is waiting for a
                // verdict. Conflating the two is what hid two passes for ~22 hours.
                phase: "ungraded".to_string(),
                stalled: false,
                last_activity_ms,
                age_secs: now_ms.saturating_sub(last_activity_ms) / 1000,
                acts: None,
                files_changed,
                files_examined: Vec::new(),
                resolved: None,
                fail_to_pass: None,
                pass_to_pass: None,
                patch_bytes: Some(diff.len() as u32),
                failed_tests: Vec::new(),
                infra_error: None,
            });
        }
    }
    if skipped_over_cap > 0 {
        tracing::warn!(
            scanned,
            skipped_over_cap,
            cap = WORKSPACE_ARTIFACT_SCAN_CAP,
            "benchmark/runs: workspace-artifact scan hit its cap — some trees were NOT examined \
             for ungraded work (raise the cap or narrow the query; this is not 'nothing found')"
        );
    }
    cards
}

/// How many staged trees one `benchmark/runs` call will project. The diffs are read from
/// the event-driven registry (no spawn here — card f860e59c), but each row still costs a
/// metadata read and a diff-header walk on a polled board; this bounds that. Over-cap trees
/// are WARNED about, never silently dropped.
const WORKSPACE_ARTIFACT_SCAN_CAP: usize = 200;

/// Cards for instances that carry a DURABLE VERDICT — the third row source, and the one that
/// makes a score visible at all.
///
/// # Why (2026-08-18, the last link in the grade tail)
///
/// Verdict persistence landed and the board still could not show a pass. Measured minutes
/// after: `astropy__astropy-14995` graded `resolved=true, F2P 1/1, P2P 40/40`, the verdict
/// was on disk and readable — and `benchmark/runs` reported `resolved: 1`, still counting only
/// an old sympy row. Three rows for 14995 read `failed`, which was HONEST: those are the three
/// solve RUNS that died at the reboot. A run and a verdict are different objects. The board
/// projected runs and artifacts; a verdict had nowhere to appear.
///
/// Until this, `recorded_verdicts` only SUBTRACTED — it marked an artifact as graded so the
/// artifact row disappeared, which made a scored instance LESS visible than an unscored one.
/// A verdict must EMIT.
///
/// This is the acceptance test from
/// [BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER](../../../docs/architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md):
/// *can a citizen standing in the room perceive the run's state through the same ViewState
/// pipe the human's screen uses?* A score answerable only by reading
/// `benchmarks/swe/verdicts/*.json` is disconnected, and it failed.
///
/// Cheap by construction: no `git diff`, no process spawn — one small JSON read per scored
/// instance, so this source needs no cap.
fn scan_verdict_cards(now_ms: u64) -> Vec<BenchRunCard> {
    swe_bench::recorded_verdicts()
        .into_iter()
        .map(|(instance, v)| {
            let last_activity_ms = std::fs::metadata(swe_bench::verdict_path(&instance))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| crate::time::EpochMs::of(t).ok())
                .map(crate::time::EpochMs::millis)
                .unwrap_or(0);
            BenchRunCard {
                run_id: format!("verdict:{instance}"),
                instance: Some(instance),
                attempt: None,
                max_attempts: None,
                // WHO was graded. The verdict has carried this since gym grades became
                // durable; rendering None here would have thrown it away again one layer
                // later, and a capability row that cannot name the citizen it scored is
                // not a capability row. Empty (an operator grade of a bare patch) stays
                // None rather than becoming an empty-string solver the roster then fails
                // to resolve.
                solver: Some(v.solver.clone()).filter(|s| !s.is_empty()),
                // The verdict IS the phase. `record_verdict` refuses gold and errored
                // verdicts, so every row here is a real capability result — never a control
                // and never an env fault dressed as a score.
                phase: if v.resolved { "resolved" } else { "failed" }.to_string(),
                stalled: false,
                last_activity_ms,
                age_secs: now_ms.saturating_sub(last_activity_ms) / 1000,
                acts: None,
                files_changed: Vec::new(),
                files_examined: Vec::new(),
                resolved: Some(v.resolved),
                fail_to_pass: Some(format!("{}/{}", v.f2p_passed, v.f2p_total)),
                pass_to_pass: Some(format!("{}/{}", v.p2p_passed, v.p2p_total)),
                patch_bytes: None,
                failed_tests: v.failed_tests.clone(),
                infra_error: None,
            }
        })
        .collect()
}

pub(crate) fn scan_run_cards(
    run_id_filter: Option<&str>,
    limit: usize,
) -> Result<RunScan, String> {
    let base = std::env::var("CONTINUUM_HOME")
        .map(std::path::PathBuf::from)
        .ok()
        .or_else(|| dirs::home_dir().map(|h| h.join(".continuum")))
        .ok_or_else(|| "no home dir".to_string())?
        .join("progress");
    let now_ms = crate::time::EpochMs::now().millis();
    let mut cards: Vec<BenchRunCard> = Vec::new();
    let entries = std::fs::read_dir(&base).map_err(|e| format!("read {}: {e}", base.display()))?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // Grade files are read as SIBLINGS of their run below, never enumerated as runs
        // (live first use showed `X.grade` phantoms). That rule and the prefix now live in
        // ONE place with the boot reaper and the reboot guard, which is what stops the
        // board and the reaper disagreeing about what a run ledger is called.
        let Some(run_id) = crate::cognition::swe_bench::solve_run_id_from_file_name(&name) else {
            continue;
        };
        if let Some(want) = run_id_filter {
            if want != run_id {
                continue;
            }
        }
        let read_json = |p: &std::path::Path| -> Option<serde_json::Value> {
            std::fs::read_to_string(p)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
        };
        let mtime_ms = |p: &std::path::Path| -> Option<u64> {
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| crate::time::EpochMs::of(t).ok())
                .map(crate::time::EpochMs::millis)
        };
        let result_path = entry.path();
        let grade_path = base.join(format!("agent-solve-{run_id}.grade.json"));
        let result = read_json(&result_path);
        let grade = read_json(&grade_path);
        let last_activity_ms = mtime_ms(&result_path)
            .into_iter()
            .chain(mtime_ms(&grade_path))
            .max()
            .unwrap_or(0);
        cards.push(fold_run_card(
            run_id,
            result.as_ref(),
            grade.as_ref(),
            last_activity_ms,
            now_ms,
        ));
    }
    // QUEUED-BEHIND-HANDS, not stalled (glass-boxed 2026-08-30): a persona
    // runs ONE solve at a time (HandsLease), so her other claim-dispatched
    // runs sit legitimately idle while she works — and the per-card silence
    // window read them as "quiet", inflating the console's gone-quiet alarm
    // with runs that are just WAITING THEIR TURN (4 of the banner's 8, live).
    // Cross-card rule: a quiet card whose solver has an ACTIVE card is
    // re-phased `queued`. Same-solver-and-active is the whole test — honest
    // quiet (solver has nothing active anywhere) still alarms.
    let active_solvers: std::collections::HashSet<String> = cards
        .iter()
        .filter(|c| c.phase == "active")
        .filter_map(|c| c.solver.clone())
        .collect();
    for card in cards.iter_mut() {
        if card.phase == "quiet"
            && card
                .solver
                .as_ref()
                .is_some_and(|s| active_solvers.contains(s))
        {
            card.phase = "queued".to_string();
            card.stalled = false;
        }
    }
    // Second source: staged trees holding work no grade has seen. Only when the caller is
    // asking for the BOARD — a run-id query is asking about one run's ledger, and a workspace
    // artifact has no run id to match. See `scan_workspace_artifact_cards` for why the board
    // cannot be run-files-only.
    if run_id_filter.is_none() {
        // "Scored" is the union of two sources, and it MUST be: a run ledger's own grade
        // sibling, AND the durable per-instance verdict record. An operator or workspace
        // grade has no run id at all, so before verdicts were recorded (2026-08-18) a real
        // pass could not make an artifact stop reading `ungraded` — two of them didn't.
        let mut graded: std::collections::HashSet<String> = cards
            .iter()
            .filter(|c| c.resolved.is_some())
            .filter_map(|c| c.instance.clone())
            .collect();
        // A verdict EMITS its own row, and that row is also what marks the instance graded —
        // so a scored instance is MORE visible than an unscored one, not less. Before this,
        // verdicts only subtracted: the artifact row vanished and no score took its place.
        let verdict_cards = scan_verdict_cards(now_ms);
        graded.extend(verdict_cards.iter().filter_map(|c| c.instance.clone()));
        cards.extend(verdict_cards);
        cards.extend(scan_workspace_artifact_cards(&graded, now_ms));
    }
    cards.sort_by(|a, b| b.last_activity_ms.cmp(&a.last_activity_ms));
    // Counted BEFORE the truncate, and carried out with the cards. Every consumer that
    // shows a bounded page has to be able to say how much it bounded away, or the page
    // reads as the whole history — see `BenchmarkRunsResult::matched` for the hour that
    // cost.
    let matched = cards.len();
    cards.truncate(limit);
    // The ledger stores the solver as her PERSONA UUID; the board speaks NAMES.
    // Resolve against the live workspace roster here — the ONE scan — so every
    // consumer (command + positron emitter) gets the same display identity. A
    // uuid not in the roster (despawned persona, operator-fired run) stays as-is;
    // the client compacts unresolved ids to short form (#161 vocabulary).
    let names: std::collections::HashMap<String, String> =
        crate::cognition::persona_workspace::global()
            .roster()
            .into_iter()
            .filter_map(|(id, name)| name.map(|n| (id.to_string(), n)))
            .collect();
    for card in &mut cards {
        if let Some(solver) = &card.solver {
            if let Some(name) = names.get(solver) {
                card.solver = Some(name.clone());
            }
        }
    }
    Ok(RunScan { cards, matched })
}
