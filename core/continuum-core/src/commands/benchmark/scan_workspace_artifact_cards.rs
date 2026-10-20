use super::*;

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
pub(super) fn scan_workspace_artifact_cards(graded: &std::collections::HashSet<String>, now_ms: u64) -> Vec<BenchRunCard> {
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
pub(super) const WORKSPACE_ARTIFACT_SCAN_CAP: usize = 200;

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
pub(super) fn scan_verdict_cards(now_ms: u64) -> Vec<BenchRunCard> {
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
    let base = crate::paths::continuum_home()?.join("progress");
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

/// No parameters. Rounds in flight are few (usually one) and each is a handful of
/// fields, so paging and filtering would be ceremony over a list you always want whole.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRoundsParams.ts"
)]
pub struct BenchmarkRoundsParams {}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRoundsResult.ts"
)]
pub struct BenchmarkRoundsResult {
    /// Rounds currently in flight. EMPTY is a real, unambiguous answer — "no round is
    /// running" — and never "the question could not be reached". A round is removed the
    /// instant its last card settles, so a round that finished is absent by design and
    /// its END is on the `bench.round.done` probe.
    pub rounds: Vec<crate::cognition::bench_round::RoundSnapshot>,
    /// How many are in flight, so a reader that only needs the yes/no does not have to
    /// interpret an array's length.
    pub in_flight: usize,
}

#[derive(Default)]
pub struct BenchmarkRounds;

#[async_trait]
impl ActionCommand for BenchmarkRounds {
    const NAME: &'static str = "benchmark/rounds";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Every benchmark ROUND in flight, with its stage — the lifecycle question answered \
         by a QUERY instead of by probe archaeology (#371). A round is the card set one \
         `benchmark/dispatch` posted; its id IS its run room's id. Each row carries stage \
         (working|done), dispatched/settled/remaining, and the work DRIVER (citizen — works \
         in the room and feeds the curriculum — vs detached_solve). An EMPTY list is a real \
         answer meaning no round is running, never a failure to reach the question; a round \
         is dropped the moment its last card settles, and that END is the `bench.round.done` \
         probe. This is what a fresh driver reads to answer 'has it started, is it stuck, is \
         it done' with zero log reads.";
    type Params = BenchmarkRoundsParams;
    type Output = BenchmarkRoundsResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        _p: BenchmarkRoundsParams,
    ) -> Result<BenchmarkRoundsResult, CommandError> {
        let mut rounds = crate::cognition::bench_round::live_rounds();
        // Merge run-ledger facts so the row answers "is it stuck" itself
        // (2026-09-01: `working 0/8` rendered identically for three hours of
        // thrash and for a healthy grind — the description above promised
        // "is it stuck" and the row could not say). Blocking fs scan off the
        // async worker; a scan failure degrades to tracker-only rows, which
        // is the honest board on a node with no ledger yet.
        let runs = tokio::task::spawn_blocking(|| {
            scan_run_cards(None, ROUNDS_ENRICH_SCAN_LIMIT)
                .map(|s| s.cards)
                .unwrap_or_default()
        })
        .await
        .unwrap_or_default();  // unwrap_or: unreadable = empty, the report shows the tracker's view
        let facts: Vec<crate::cognition::bench_round::CardRunFacts> =
            runs.iter().map(card_run_facts).collect();
        crate::cognition::bench_round::enrich_rounds(
            &mut rounds,
            &facts,
            crate::time::EpochMs::now().millis(),
        );
        enrich_rounds_from_board_and_verdicts(&mut rounds).await;
        Ok(BenchmarkRoundsResult {
            in_flight: rounds.len(),
            rounds,
        })
    }
}

/// BOARD TRUTH + VERDICT FILES onto the round report — the durable records, read at
/// report time (the round tracker knows the deck; the board knows who holds what and
/// when; the verdict file knows the grade and when). This is what makes a round's
/// time-to-claim, time-to-settle and resolve rate readable by a stranger after any
/// number of reboots. Best-effort: an unreadable board leaves the tracker's view.
pub(crate) async fn enrich_rounds_from_board_and_verdicts(
    rounds: &mut [crate::cognition::bench_round::RoundSnapshot],
) {
    let registry = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global();
    let reader = registry.as_ref().and_then(|reg| reg.any_live_citizen());
    for round in rounds.iter_mut() {
        // Verdicts: keyed by instance, independent of the board.
        for card in round.cards.iter_mut() {
            if card.instance.is_empty() {
                continue;
            }
            if let Some(v) = crate::cognition::swe_bench::read_verdict(&card.instance) {
                card.resolved = Some(v.resolved);
                card.graded_at_ms = (v.graded_at_ms > 0).then_some(v.graded_at_ms);
            }
        }
        let Some(rt) = reader.as_ref() else {
            continue;
        };
        let Ok(room_id) = round.round_id.parse::<uuid::Uuid>() else {
            continue;
        };
        let Ok(set) = rt.airc().subscription_set().await else {
            continue;
        };
        let Some(room) = set
            .all()
            .map(|sub| sub.as_room())
            .find(|r| r.channel.as_uuid() == room_id)
        else {
            continue; // the reader is not resident in this run room — tracker view stands
        };
        let Ok(board) = rt.airc().work_board_in(&room).await else {
            continue;
        };
        let board = board.snapshot();
        for card in round.cards.iter_mut() {
            let Some(bc) = board
                .cards
                .iter()
                .find(|c| c.card_id.as_uuid().to_string() == card.card_id)
            else {
                continue;
            };
            card.board_state = serde_json::to_value(bc.state)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default();  // unwrap_or: unreadable = empty, the report shows the tracker's view
            card.owner = bc
                .owner
                .map(|o| {
                    registry
                        .as_ref()
                        .and_then(|reg| reg.get(o.as_uuid()))
                        .map(|rt| rt.agent_name().to_string())
                        .unwrap_or_else(|| o.as_uuid().to_string()[..8].to_string())  // unwrap_or: no live runtime for the owner = her short id, still addressable
                })
                .unwrap_or_default();  // unwrap_or: unreadable = empty, the report shows the tracker's view
            // An owner is a CLAIM attribute: the board keeps the last holder's id on a
            // card set back to open (2026-09-05: seven reopened cards read as "owned" and
            // the academy board — and the operator — counted 12 in hands with 5 held).
            if matches!(card.board_state.as_str(), "open" | "closed" | "merged") {
                card.owner.clear();
            }
            card.created_at_ms = Some(bc.created_at_ms);
            card.updated_at_ms = Some(bc.updated_at_ms);
        }
    }
}

/// How deep the rounds projection scans the run ledger for enrichment. Bounded
/// so a node with hundreds of banked verdicts pays a bounded scan; deep enough
/// that every in-flight round's instances are covered (a round is ≤ dozens of
/// cards and its runs are by definition recent).
pub(super) const ROUNDS_ENRICH_SCAN_LIMIT: usize = 200;

/// `BenchRunCard` → the minimal facts `bench_round::enrich_rounds` folds.
/// ONE mapping, shared by the command above and the positron bench emitter —
/// two hand-rolled copies would drift ([[the-same-bug-at-two-sites-is-a-missing-constraint]]).
pub(crate) fn card_run_facts(card: &BenchRunCard) -> crate::cognition::bench_round::CardRunFacts {
    crate::cognition::bench_round::CardRunFacts {
        card_id: card
            .run_id
            .strip_prefix("claim-")
            .and_then(|rest| rest.get(..36))
            .filter(|id| uuid::Uuid::parse_str(id).is_ok())
            .map(str::to_string),
        instance: card.instance.clone().unwrap_or_default(),
        solver: card.solver.clone(),
        phase: card.phase.clone(),
        acts: card.acts,
        patch_bytes: card.patch_bytes,
        last_activity_ms: card.last_activity_ms,
        resolved: card.resolved,
    }
}

/// Where a catalogued suite's rows actually live, and whether anything in this tree can read
/// them. Four states, because the four have four different fixes and collapsing any two of
/// them produces a refusal the operator has to go do archaeology on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceReach {
    /// Servable by the HuggingFace rows API at these exact coordinates.
    Rows {
        dataset: &'static str,
        config: &'static str,
        split: &'static str,
    },
    /// HuggingFace-hosted, but a LOADING-SCRIPT dataset: the rows API refuses it outright
    /// ("runs arbitrary Python code"). No config or split makes it work, so a refusal that
    /// merely says "not found" invites an infinite guessing loop. Measured live 2026-08-19
    /// against `datasets-server.huggingface.co/splits` for both rows carrying this.
    HuggingFaceScriptDataset { dataset: &'static str },
    /// A real source, just not one the HF path can read (GitHub raw files, a repo to clone).
    /// Needs its own fetcher; naming that is the honest answer.
    ForeignSource { url: &'static str },
    /// Ships with the binary — there is nothing to pull.
    InTree,
}

impl BenchmarkSpec {
    /// The suite's fetch coordinates, as DATA rather than as an operator's memory.
    ///
    /// `config`/`split` are NOT derivable from the URL and are not uniformly `default`/`test`
    /// — bigcodebench versions its splits (`v0.1.4`), and a wrong guess returns an in-band HF
    /// error that a caller reads as "the suite is empty". So the exceptions live here, in the
    /// ONE place that knows, exactly as [`BenchmarkSpec::swe_dataset`] already does for row
    /// shape. The dataset id is still read back off `source_url` so it is never duplicated.
    pub fn reach(&self) -> SourceReach {
        let Some(url) = self.source_url else {
            return SourceReach::InTree;
        };
        let Some(dataset) = url
            .strip_prefix("https://huggingface.co/datasets/")
            .filter(|id| id.contains('/'))
        else {
            return SourceReach::ForeignSource { url };
        };
        // Loading-script datasets: the rows API cannot serve these at ANY coordinates.
        if matches!(dataset, "codeparrot/apps" | "livecodebench/code_generation_lite") {
            return SourceReach::HuggingFaceScriptDataset { dataset };
        }
        let (config, split) = match dataset {
            // bigcodebench publishes revisions as SPLITS; `test` does not exist. v0.1.4 is
            // the newest as of 2026-08-19 — bump it here when they publish, so the version
            // scored against is a recorded catalog fact and not whatever the default was.
            "bigcode/bigcodebench" => ("default", "v0.1.4"),
            _ => ("default", "test"),
        };
        SourceReach::Rows {
            dataset,
            config,
            split,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkFetchParams.ts"
)]
pub struct BenchmarkFetchParams {
    /// Which catalogued benchmark to stage, as it appears in `benchmark/list`.
    pub benchmark: String,
    /// Dataset config. Defaults to `default` — the HF convention.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub config: Option<String>,
    /// Split to pull. Defaults to `test`, which is what a benchmark is scored on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub split: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkFetchResult.ts"
)]
pub struct BenchmarkFetchResult {
    pub benchmark: String,
    pub dataset: String,
    pub config: String,
    pub split: String,
    /// Rows actually staged. This is the suite's REAL denominator — compare it against the
    /// catalog's `tasks` before trusting any rate computed from it.
    pub rows: usize,
    /// The catalog's declared task count, so a mismatch is visible at fetch time rather than
    /// discovered when a published number turns out to be over the wrong denominator.
    pub declared_tasks: u32,
    /// True when `rows` and `declared_tasks` agree. False is not fatal — datasets are revised
    /// upstream — but a rate published over a disagreeing denominator is not comparable.
    pub denominator_matches: bool,
    /// How many rows actually PROJECT into posable tasks through this suite's `SuiteAdapter`.
    /// `None` = the suite has no adapter yet: its rows are staged but cannot be posed to a
    /// citizen. Staged-but-unposable is the honest middle state, and reporting it as a distinct
    /// value is what keeps a fetched suite from LOOKING runnable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tasks: Option<usize>,
    /// Present only when projection is unavailable or failed, saying which of those it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub adapter_note: Option<String>,
}

#[derive(Default)]
pub struct BenchmarkFetch;
