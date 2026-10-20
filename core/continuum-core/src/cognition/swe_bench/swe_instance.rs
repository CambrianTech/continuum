use super::*;

/// One instance — the fields the protocol actually needs.
///
/// **Deliberately provenance-agnostic.** These fields describe a repo, a commit, a fix, and the
/// tests that define the bug — none of that is specific to a HuggingFace row. A teacher
/// synthesizing work (revert a known-good commit in a real repo, keep its tests as the
/// FAIL_TO_PASS set — that is `gym/mine`, #133) produces the SAME type, and `grade()` scores it
/// with zero new code.
///
/// That matters because benchmarks are becoming curriculum, not just measurement: the corpus
/// compounds as instances accumulate, and a teacher extends it via simulated work. The
/// expensive mistake would be a second, divergent "generated instance" struct — so `load_dataset`
/// is the only HF-shaped thing here, and everything downstream takes `&SweInstance`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweInstance {
    pub instance_id: String,
    pub repo: String,
    pub base_commit: String,
    /// The gold patch. The spine check grades THIS; it must resolve or the environment is wrong.
    pub patch: String,
    /// The tests that define the bug. Applied to the tree after the candidate patch.
    pub test_patch: String,
    pub problem_statement: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(
        rename = "FAIL_TO_PASS",
        alias = "fail_to_pass",
        default,
        deserialize_with = "test_list_string"
    )]
    pub fail_to_pass: String,
    #[serde(
        rename = "PASS_TO_PASS",
        alias = "pass_to_pass",
        default,
        deserialize_with = "test_list_string"
    )]
    pub pass_to_pass: String,
    /// SWE-bench Multilingual ships its harness AS DATA per instance: an eval script
    /// (test files at base, test patch applied, ONE marked test command, restore)
    /// and the name of the log parser that reads that command's output. Absent on
    /// the Python family, which runs through our own venv + pytest runner.
    #[serde(default)]
    pub eval_script: Option<String>,
    #[serde(default)]
    pub log_parser: Option<String>,
}

/// The SWE-instance family speaks THREE dialects for the same two fields
/// (2026-08-23 landscape import): SWE-bench serves `FAIL_TO_PASS` as a
/// JSON-ENCODED STRING, SWE-rebench serves it as a REAL LIST, and SWE-bench
/// Pro lowercases the name (string-encoded). One tolerant deserializer
/// normalizes all three to the JSON-encoded-string form `f2p()`/`p2p()`
/// already parse — the constraint fix, so no second instance struct ever
/// exists per dialect ([[fix-the-constraint-not-the-instance-interfaces-first]]).
pub(super) fn test_list_string<'de, D>(d: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::String(s) => Ok(s),
        serde_json::Value::Array(_) => Ok(v.to_string()),
        serde_json::Value::Null => Ok(String::new()),
        other => Err(serde::de::Error::custom(format!(
            "FAIL_TO_PASS/PASS_TO_PASS must be a JSON-encoded string or a list, got {other}"
        ))),
    }
}

/// Is this required "test id" something that can never name a test?
///
/// UPSTREAM DATA DEFECT, measured 2026-08-27 across the local sets: SWE-bench's
/// own log-parser leaked non-test tokens into the required lists. Lite's
/// `pytest-dev__pytest-5227` requires `"["`, `"[100%]"` and
/// `"[100%]------------------------------"` — fragments of pytest's progress
/// bar; Verified carries six more (`"#15789"` on django-15863/16485, a bare
/// `"\""` on django-16145/16612, `"[100%]"` on pytest-5262/7521).
///
/// A required id that cannot name a test is unsatisfiable for EVERY solver,
/// including the instance's own gold patch — so counting it as a failure does
/// not measure the citizen, it just makes the instance unwinnable. It cost a
/// real resolve: pytest-5227 passed 3/3 fail-to-pass and every one of its 31
/// genuine pass-to-pass tests, and was scored a MISS on three progress-bar
/// fragments. (Upstream's own harness scores these only because its log parser
/// is loose enough to re-emit the same garbage tokens; ours requires a real
/// node id, so they never match.)
///
/// The predicate is deliberately conservative — it drops a token only when it
/// contains NO identifier character at all. Every real test id (node id, bare
/// function, django's `test_x (module.Class)`) has letters, so nothing
/// legitimate can be filtered by this.
pub(super) fn is_unsatisfiable_test_id(id: &str) -> bool {
    !id.chars().any(|c| c.is_ascii_alphabetic() || c == '_')
}

impl SweInstance {
    /// Test-id lists arrive JSON-encoded inside a string field.
    ///
    /// Non-test tokens are dropped here — the ONE seam every consumer (gate,
    /// grade, scoreboard) reads through, so no caller can re-introduce them.
    pub fn f2p(&self) -> Vec<String> {
        Self::satisfiable(
            serde_json::from_str(&self.fail_to_pass).unwrap_or_default(),
            &self.instance_id,
            "FAIL_TO_PASS",
        )
    }
    pub fn p2p(&self) -> Vec<String> {
        Self::satisfiable(
            serde_json::from_str(&self.pass_to_pass).unwrap_or_default(),
            &self.instance_id,
            "PASS_TO_PASS",
        )
    }

    /// Drop unsatisfiable ids and SAY SO — a silently shrunk denominator is the
    /// shape that turns a benchmark into a number nobody can audit.
    pub(super) fn satisfiable(ids: Vec<String>, instance_id: &str, field: &str) -> Vec<String> {
        let (keep, dropped): (Vec<String>, Vec<String>) = ids
            .into_iter()
            .partition(|id| !is_unsatisfiable_test_id(id));
        if !dropped.is_empty() {
            crate::probe!(
                class = "benchmark.dataset.unsatisfiable_ids",
                instance = %instance_id,
                field = %field,
                dropped = %dropped.len(),
                ids = ?dropped,
                "upstream required-test list carries tokens that cannot name a test \
                 (its log parser leaked progress output); dropped so the instance is \
                 winnable by the gold patch — never counted as citizen failures"
            );
        }
        keep
    }
    /// The era to resolve dependencies and the interpreter against.
    pub fn year(&self) -> u32 {
        self.created_at
            .get(..4)
            .and_then(|y| y.parse().ok())
            .unwrap_or(2023)
    }
}

/// What a graded run concluded, with enough detail that a zero is never ambiguous.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SweVerdict {
    pub instance_id: String,
    pub resolved: bool,
    pub f2p_passed: usize,
    pub f2p_total: usize,
    pub p2p_passed: usize,
    pub p2p_total: usize,
    /// False when FAIL_TO_PASS already passed on the pristine tree — the task carries no bug
    /// here, so no score from it distinguishes a fix from a no-op.
    pub gate_ok: bool,
    /// FAIL_TO_PASS tests that were ALREADY green on the pristine tree while others still
    /// failed. The gate holds (there is a bug left to fix) but the receipt must name them:
    /// a citizen cannot earn credit for those, and an env whose backend makes one of them
    /// pass (django-13346's `test_key_iregex` on this box) is a parity note, not a void.
    #[serde(default)]
    pub pristine_green: Vec<String>,
    /// PASS_TO_PASS tests that failed after the patch AND on the pristine tree in this same
    /// environment, so they are excluded from the score: an env fault named, not a
    /// regression charged. `resolved` is decided on the rest.
    #[serde(default)]
    pub env_void_p2p: Vec<String>,
    /// Set when the run could not produce a verdict at all (clone, patch, or env failure).
    /// A verdict with `error` set is NOT a zero — it is an absence, and must never be
    /// tallied as a failed attempt.
    pub error: Option<String>,
    /// The NAMES of the tests that failed, sorted, so a verdict can teach.
    /// "PASS_TO_PASS 6/11" is a count with nothing to act on; "your patch broke
    /// test_arguments and test_unit" is what a human reviewer would say (Joel,
    /// 2026-08-08: "you or any human could tell him what's wrong — or the grader").
    /// This is what the experience stream and the room verdict carry forward.
    /// Epoch-ms when this verdict was recorded — the experiment's settle clock.
    #[serde(default)]
    pub graded_at_ms: u64,
    #[serde(default)]
    pub failed_tests: Vec<String>,
    /// The failing FAIL_TO_PASS run's OUTPUT tail (capped) — the assertion diff a
    /// human reviewer would paste. Glass-boxed on atlas-sympy-24066-n4 (2026-08-08):
    /// she synthesized ~90% of the gold patch and missed on one predicate
    /// (`== Dimension(1)` vs `is_dimensionless`); the retry verdict named the
    /// failing TEST but not what it PRINTED — the leftover
    /// `Dimension(impedance*capacitance/time)` in the assertion output is the fact
    /// that teaches equality isn't enough. Format-agnostic (a report TAIL, not a
    /// parsed section) so sympy's own runner and pytest both carry it.
    #[serde(default)]
    pub failure_excerpt: Option<String>,
    /// The harness build that PRODUCED this verdict.
    ///
    /// A verdict is only as current as the harness that scored it. Measured
    /// 2026-08-28: 19 of 32 verdicts on this box had been written across ten
    /// days by three different harness builds, and regrading one from its
    /// IDENTICAL banked patch moved pass-to-pass from 0/40 to 40/40 — the old
    /// verdict said she had destroyed 40 working tests; she had broken nothing.
    /// Nothing in the file said which harness said it, so staleness was an mtime
    /// an operator had to notice rather than a fact a reader could check.
    ///
    /// This matters most for the thing we publish: an improvement curve plotted
    /// across mixed harness eras is not a curve, it is an average over a moving
    /// instrument. With this stamped, a chart can require one era — or say
    /// plainly that it blended several.
    ///
    /// `#[serde(default)]` so the verdicts already on disk load unstamped; an
    /// EMPTY sha reads as "written before provenance existed", which is exactly
    /// what those files are.
    #[serde(default)]
    pub harness_build: String,
    /// The MODEL that produced the graded patch — the serving snapshot's
    /// active model at verdict time (2026-08-29: cards do not carry a model;
    /// solves run on whatever serves, so "X resolved what Y couldn't" is
    /// unpublishable without this stamp). `#[serde(default)]`: pre-stamp
    /// verdicts read empty = "written before model provenance existed".
    #[serde(default)]
    pub served_model: String,
    /// WHICH ADAPTER produced this verdict — `"swe-bench-lite"`, `"games-rs"`,
    /// `"hard-rs"`. Benchmarks are adapters into one activity
    /// (docs/architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md), so the verdict
    /// store is shared and a reader must be able to ASK which adapter scored a row
    /// rather than infer it from the SHAPE of `instance_id` — inferring a category
    /// from a string's shape is exactly the mixing this repo forbids.
    ///
    /// `#[serde(default)]`: verdicts written before gym grades were durable read
    /// empty, which honestly means "recorded when SWE was the only producer".
    #[serde(default)]
    pub benchmark: String,
    /// WHO was graded — the owner's peer uuid, as a string because that is what the
    /// board's roster lookup keys on. Empty when the grade had no owner (an operator
    /// grade of a bare patch). Without it a verdict row can say a task resolved and
    /// not say who resolved it, which is unusable as a capability record for a
    /// citizen and unpublishable as a claim about a model.
    #[serde(default)]
    pub solver: String,
}

/// Where a detached benchmark run journals its state. One file per run, rewritten in place:
/// `state: "running"` at dispatch, then the verdict — or a killed-by-reboot marker.
///
/// Honours `CONTINUUM_HOME` the same way every other progress-ledger reader does
/// (`paths::continuum_home`). It did not until 2026-08-18, which made this the SECOND
/// way to spell the ledger root — and a reader that resolves a directory differently from
/// the writer reports a self-consistent lie about an empty dir, the same failure shape the
/// `swe_cache_dir` doc records for env coverage.
pub fn solve_ledger_dir() -> PathBuf {
    crate::paths::continuum_home()
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())).join(".continuum")
        })
        .join("progress")
}

/// The ONE filename prefix a detached solve run's ledger carries: `agent-solve-<run_id>.json`,
/// with its verdict alongside as `agent-solve-<run_id>.grade.json`.
///
/// # Why this is a constant and not spelled out per call site (2026-08-18)
///
/// It was spelled out per call site, and the two spellings did not match. `agent/solve`
/// wrote `agent-solve-*` (`agent_solve_ledger_path`, the only production writer); the reaper
/// and the reboot guard below read `swe-solve-*`, a name NOTHING in the tree has ever
/// written. Measured on this box the day it was found: **506 `agent-solve-*` ledgers, 19 of
/// them still marked `running` — the oldest 162 hours old — against 32 legacy `swe-solve-*`
/// files, 0 running.** So neither half of the mechanism had ever engaged on a real run:
///
/// - [`reap_orphaned_solve_runs_in`] had never reaped a production run, so 19 orphans sat
///   frozen as `running` instead of becoming honest FAILED records — the exact silent-death
///   hole its own doc says it exists to close.
/// - [`in_flight_solve_runs_in`] answered "nothing is in flight" to the reboot guard while
///   19 runs were marked otherwise, so `continuum reboot` would have destroyed live work
///   without naming it. A guard that cannot see is worse than no guard: it reports safety.
///
/// A unit test even pinned the mismatch as intent — it wrote `agent-solve-other.json` and
/// asserted it stayed untouched as "another subsystem's ledger". There is no other
/// subsystem; that WAS the production ledger.
/// [[the-same-bug-at-two-sites-is-a-missing-constraint-not-two-bugs]]
pub const SOLVE_LEDGER_PREFIX: &str = "agent-solve-";

/// Marker carried by the one error a CACHED-env re-point failure produces, so a
/// consumer can tell "my cache is stale for THIS tree" apart from "this instance
/// has no buildable environment". ONE definition, written at the error site and
/// read by the close classifier — never the same prose typed in two places.
///
/// Why it must be distinguishable (glass-boxed 2026-08-27, django-11734): the
/// per-instance env is cached, and each grader re-points its editable install at
/// ITS OWN tree — `agent/solve` at the citizen's dirty workspace, `benchmark_grade`
/// at a pristine clone + patch. A patch that breaks the build backend therefore
/// fails the solve's re-point while the fresh clone grades fine seconds later. The
/// close path used to read ANY grade error as an environment absence, which both
/// lied in the log and armed the per-boot parking brake against an instance whose
/// environment was provably healthy. A cache is an optimization; its failure must
/// never become a conclusion about the world.
///
/// The deeper fix — a typed `error_kind` on the verdict rather than a marker in
/// prose — is tracked in #2531; this constant is the seam that makes the current
/// taxonomy honest without a wire-shape change mid-round.
pub const ENV_CACHE_REPOINT_FAILED: &str = "cached-env re-point failed";

/// The run id a ledger file name carries, or `None` if it is not a run ledger.
///
/// Rejects `agent-solve-<id>.grade.json`: a grade is read as a SIBLING of its run, never
/// enumerated as one. Without that check the prefix/suffix strip yields a phantom run whose
/// id ends in `.grade` — the live first-use bug `scan_run_cards` already carried a guard for,
/// now shared rather than re-derived.
pub fn solve_run_id_from_file_name(name: &str) -> Option<&str> {
    let run_id = name
        .strip_prefix(SOLVE_LEDGER_PREFIX)?
        .strip_suffix(".json")?;
    if run_id.is_empty() || run_id.ends_with(".grade") {
        return None;
    }
    Some(run_id)
}

/// This run's ledger file. The one path builder, so writer and reader cannot drift again.
pub fn solve_ledger_path(dir: &Path, run_id: &str) -> PathBuf {
    dir.join(format!("{SOLVE_LEDGER_PREFIX}{run_id}.json"))
}

/// This run's verdict file, alongside its ledger.
pub fn solve_grade_path(dir: &Path, run_id: &str) -> PathBuf {
    dir.join(format!("{SOLVE_LEDGER_PREFIX}{run_id}.grade.json"))
}

/// Runs this ledger dir believes are IN FLIGHT — written at dispatch, not yet resolved.
///
/// Two callers, one truth: the reboot guard asks before killing the core, and the boot reaper
/// asks after. Returning `(run_id, instance)` rather than a bool is what lets the guard NAME
/// what it would destroy, which is the difference between a policy and a nag.
pub fn in_flight_solve_runs_in(dir: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut live = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(run_id) = solve_run_id_from_file_name(name) else {
            continue;
        };
        let run_id = run_id.to_string();
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if v.get("state").and_then(|s| s.as_str()) != Some("running") {
            continue;
        }
        let instance = v
            .get("instance")
            .and_then(|i| i.as_str())
            .unwrap_or("unknown")
            .to_string();
        live.push((run_id, instance));
    }
    live.sort();
    live
}

/// Convenience over the real ledger dir.
pub fn in_flight_solve_runs() -> Vec<(String, String)> {
    in_flight_solve_runs_in(&solve_ledger_dir())
}

/// A `running` ledger that nothing has touched for this long is not a solve in flight — it
/// is a solve that died without a reboot (a SIGKILL, a panicked task, the power event of
/// 2026-09-16). The boot reaper only sees deaths a restart caused; between boots a dead
/// ledger would otherwise hold its instance out of grading forever (Cormac, #4117 review).
/// A live solve rewrites its ledger on every act, so an hour of silence is decisive.
pub const SOLVE_LEDGER_STALE: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// In-flight runs whose ledger is FRESH: `(run_id, instance)` for every `running` ledger
/// touched within [`SOLVE_LEDGER_STALE`]; stale ones are returned separately so the caller
/// can name them instead of silently ignoring them.
pub fn in_flight_solve_runs_fresh_in(
    dir: &Path,
    now: std::time::SystemTime,
) -> (Vec<(String, String)>, Vec<(String, String)>) {
    let mut fresh = Vec::new();
    let mut stale = Vec::new();
    for (run_id, instance) in in_flight_solve_runs_in(dir) {
        let path = solve_ledger_path(dir, &run_id);
        let age = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| now.duration_since(m).ok());
        match age {
            Some(a) if a > SOLVE_LEDGER_STALE => stale.push((run_id, instance)),
            _ => fresh.push((run_id, instance)), // an unreadable mtime is treated as fresh: never grade over a solve that might be live
        }
    }
    (fresh, stale)
}

pub fn in_flight_solve_runs_fresh() -> (Vec<(String, String)>, Vec<(String, String)>) {
    in_flight_solve_runs_fresh_in(&solve_ledger_dir(), std::time::SystemTime::now())
}

/// At boot, any run still marked `running` was owned by a core that no longer exists — a
/// reboot, a crash, a SIGKILL. Rewrite it as a FAILED run naming the cause.
///
/// Glass-boxed the day `swe-solve` shipped: two reboots silently killed a detached run, and
/// its ledger simply never appeared. A poller cannot distinguish "still working" from "died an
/// hour ago" when the evidence for both is an absent file — the same shape as #137's 41 train
/// jobs submitted with zero outcomes recorded. The marker at dispatch is what makes the death
/// observable; this reap is what makes it honest.
/// The reap MERGES the death into the existing ledger rather than replacing it, because the
/// ledger is the only pointer to what the dead run LEFT BEHIND.
///
/// A live `running` record carries `workspace` — the absolute path of the checkout her hands
/// edited — alongside `instance`, `persona_id` and `acts`. Overwriting it with a bare marker
/// (which this did until 2026-08-18) turns a gradeable orphan into an unlocatable one: the
/// patch is still on disk, and nothing on the board can say where. That is the same class of
/// harm as reaping a finished verdict, and the reason the existing keys are preserved here
/// and only the failure fields are added.
/// Returns `(run_id, instance)` for every run it reaped — the SAME pair
/// [`in_flight_solve_runs_in`] returns, and for the same reason: the caller has to be able
/// to NAME what died. Returning bare ids was a measured loss of the only fact that makes a
/// reap actionable. On 2026-08-21, 8 of 20 runs on this box carried
/// `killed by a core restart`, and the boot probe could say only that A run died — not
/// WHICH INSTANCE — so recovering the round meant hand-opening ledger files. The instance
/// is written into the record three lines below; dropping it on the way out was pure
/// discard.
pub fn reap_orphaned_solve_runs_in(dir: &Path) -> Vec<(String, String)> {
    let mut reaped = Vec::new();
    for (run_id, instance) in in_flight_solve_runs_in(dir) {
        let path = solve_ledger_path(dir, &run_id);
        // Start from what the run itself journaled; the death is an ANNOTATION on that
        // record, never a replacement for it.
        let mut record = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .filter(|v| v.is_object())
            .unwrap_or_else(|| serde_json::json!({}));
        let obj = record.as_object_mut().expect("filtered to an object above");
        obj.insert("failed".into(), serde_json::Value::Bool(true));
        // `state` is what `in_flight_solve_runs_in` keys on. Clearing it off `running` is
        // what makes the reap idempotent — a second boot must not re-reap.
        obj.insert("state".into(), serde_json::Value::String("failed".into()));
        // `run_id`, NOT `runId`: this ledger family is written by `agent/solve`, which spells
        // it snake_case throughout. The first cut of this reap inserted `runId` and shipped
        // records carrying BOTH — two names for one field, in the same change that collapsed
        // two names for one file. Caught on the live reap of 19 orphans. `entry` so a record
        // that already carries its id keeps it untouched. (`cognition/eval` uses `runId` for
        // its OWN ledger family; that is a different family and stays as it is.)
        obj.entry("run_id")
            .or_insert_with(|| serde_json::Value::String(run_id.clone()));
        obj.insert(
            "instance".into(),
            serde_json::Value::String(instance.clone()),
        );
        obj.insert(
            "error".into(),
            serde_json::Value::String(
                "killed by a core restart — the run was in flight when the core that owned it \
                 went away. Nothing was scored; re-dispatch to measure this instance."
                    .into(),
            ),
        );
        if std::fs::write(&path, record.to_string()).is_ok() {
            reaped.push((run_id, instance));
        }
    }
    reaped
}

/// Convenience over the real ledger dir.
pub fn reap_orphaned_solve_runs() -> Vec<(String, String)> {
    reap_orphaned_solve_runs_in(&solve_ledger_dir())
}

/// Where cached datasets and per-instance environments live. A governed cache class, not a
/// scratch dir — see the disk-eviction contract.
///
/// # THIS IS THE ONLY SWE ENV ROOT. `swe_cache_dir()/envs`, nowhere else.
///
/// A second root used to exist — `~/.continuum/cache/swe-envs`, the default of the retired
/// `legacy/benchmarks/swe/grade_local.py`. Both directories held real venvs. Neither named
/// the other. Nothing failed loudly, because each was internally consistent.
///
/// On 2026-08-17 that cost a full misdiagnosis with a decision attached: `ls` on the retired
/// root showed 14 envs across 3 repos, which became the reported finding *"77% of staged
/// instances have no environment — the env builder only works for sympy/flask/requests"*.
/// A design was approved on it. The live root held **46 envs across 8 repos** — 95% coverage,
/// every repo present. Same question, two directories, opposite answers. The retired root is
/// now deleted and the legacy script's default points here.
///
/// The general defect, worth recognising before it regrows elsewhere: a cache with two roots
/// cannot report its own coverage, because every reader picks one and gets a self-consistent
/// lie. If you add a second location for anything cached here — a mirror, a per-node copy, a
/// migration staging dir — it needs to be derived from THIS function, not spelled out again.
/// A path literal repeated in a second file is the whole failure mode
/// ([[the-same-bug-at-two-sites-is-a-missing-constraint-not-two-bugs]]).
///
/// Corollary for anyone measuring env coverage: read the root from here, never from a path
/// you remember or a directory you found by name.
/// ONE MORE ROOT-DERIVATION NOTE, added 2026-08-18 because this function became the very
/// thing its doc warns about. It resolved `HOME` directly while `solve_ledger_dir` and every
/// other progress reader resolve `CONTINUUM_HOME` — so the benchmarks root and the ledger
/// root were TWO roots again, exactly the shape described above.
///
/// It surfaced as test pollution, which is the cheap way to find it: a unit test set
/// `CONTINUUM_HOME` to a tempdir to isolate its writes, and `record_verdict` wrote into the
/// OPERATOR'S REAL `~/.continuum/benchmarks/swe/verdicts` anyway. The isolation was vacuous
/// and the test was quietly seeding the live verdict record with fixture data — a fixture
/// that would later read as a genuine measurement.
pub fn swe_cache_dir() -> PathBuf {
    crate::paths::continuum_home()
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())).join(".continuum")
        })
        .join("benchmarks")
        .join("swe")
}

/// Where a scored instance's verdict lives — the DURABLE record that a grade happened.
/// One file per instance under the governed benchmarks root, beside `work/` and `envs/`.
///
/// # Why this exists (2026-08-18, and it cost the day's only two passes)
///
/// `benchmark/swe-grade` computed a verdict, appended it to the citizen's experience
/// stream, and RETURNED it. Nothing durable was written. So two real SWE-bench Lite
/// resolutions — astropy-14995 and pytest-11143, each FAIL_TO_PASS 1/1 and PASS_TO_PASS
/// 40/40, watched passing live — left no trace, and `benchmark/runs` kept rendering both
/// artifacts as `ungraded`. Measured the same afternoon: **37 grade artifacts on disk from
/// the detached-solve path, and 0 from any operator or workspace grade**, because that arm
/// had no persistence at all.
///
/// A measurement the system cannot remember is not a measurement. This is the results-log
/// half of [[run-ledgers-are-typed-artifacts]]: the run LEDGER is current state, keyed by
/// run; the verdict is HISTORY, keyed by INSTANCE — because grading is per-instance and a
/// workspace grade legitimately has no run id at all.
pub fn verdict_dir() -> PathBuf {
    swe_cache_dir().join("verdicts")
}

/// This instance's verdict file. One naming, so writer and board reader cannot drift
/// (the failure mode that cost the boot reaper weeks — see [`SOLVE_LEDGER_PREFIX`]).
pub fn verdict_path(instance_id: &str) -> PathBuf {
    verdict_dir().join(format!("{instance_id}.json"))
}

/// Persist a REAL verdict. Returns the path written.
///
/// Refuses two things by construction, because both would launder the board:
/// - an **errored** verdict — that is an ABSENCE (clone/env/patch fault), never a score,
///   and recording it would tally a broken harness as a citizen's failure (the #384 class);
/// - a **gold-gate** verdict — the positive control proves the ENV, not the citizen. A gold
///   pass recorded as an instance verdict would read on the board as our result.
/// What a new verdict does to the one already on disk for the same instance.
#[derive(Debug, PartialEq, Eq)]
pub enum VerdictWrite {
    /// Nothing recorded yet, or the new verdict is at least as good: it becomes the record.
    Record,
    /// A RESOLVE already stands and the new verdict does not resolve. The resolve stays;
    /// the new one is kept as history.
    KeepTheResolve,
}

/// ONE CITIZEN'S OUTCOME IS NOT ANOTHER CITIZEN'S TO OVERWRITE.
///
/// sympy__sympy-22456, 2026-09-07: Mathis worked her card in her room and it came back
/// resolved at 14:13Z (f2p 1/1). At 16:15Z a DIFFERENT citizen, holding a different card
/// for the same upstream instance in a later round, was graded from her own checkout — a
/// diff that broke 29 previously-passing tests — and the write replaced Mathis's outcome
/// with `resolved: false`. Work one citizen actually did was erased by another's attempt.
///
/// The cause is that this file is keyed by INSTANCE while the work is done by a CARD in a
/// room: two activities share one filename, so the newest write wins whoever earned it.
/// The outcome belongs to the card (`bench_round`'s per-card `resolved` / `graded_at_ms`
/// already hold it there); this per-instance file is a derived index for reporting, and
/// treating it as the truth is what let one room's result overwrite another's. Making the
/// card's outcome the sole record is the structural fix and its own card.
///
/// Until then this guard keeps the index honest: an outcome that RESOLVED is never
/// replaced by one that did not, because a later attempt failing says something about the
/// later attempt, not about the patch that passed. The superseding verdict is appended to
/// the instance's regrade history rather than discarded, so both attempts stay readable.
///
/// A new RESOLVE always records (it can only be an improvement), and failure-over-failure
/// records too — the freshest failure detail is what the citizen holding it needs.
pub fn verdict_write_for(existing: Option<&SweVerdict>, new: &SweVerdict) -> VerdictWrite {
    match existing {
        Some(prior) if prior.resolved && !new.resolved => VerdictWrite::KeepTheResolve,
        _ => VerdictWrite::Record,
    }
}

/// Where an instance's superseded regrades are kept — one JSON object per line, appended.
pub fn regrade_history_path(instance_id: &str) -> PathBuf {
    verdict_dir().join(format!("{instance_id}.regrades.jsonl"))
}

pub fn record_verdict(verdict: &SweVerdict, is_gold: bool) -> Result<Option<PathBuf>, String> {
    let recorded = persist_verdict(verdict, is_gold)?;
    if recorded.is_some() {
        crate::persona::training_producer::settle_instance_credit(&verdict.instance_id, verdict.resolved);
    }
    Ok(recorded)
}

/// A card grader already owns the exact identity. Do not rediscover it through
/// the live round: the terminal event may have retired that round before grading.
pub(crate) async fn record_card_verdict(
    verdict: &SweVerdict,
    card: uuid::Uuid,
) -> Result<Option<PathBuf>, String> {
    record_card_verdict_with(verdict, card, crate::persona::training_producer::settle_card_credit).await
}

pub(crate) async fn record_card_verdict_with<F, Fut>(
    verdict: &SweVerdict,
    card: uuid::Uuid,
    settle: F,
) -> Result<Option<PathBuf>, String>
where
    F: FnOnce(uuid::Uuid, bool) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let recorded = persist_verdict(verdict, false)?;
    if recorded.is_some() {
        settle(card, verdict.resolved).await;
    }
    Ok(recorded)
}

/// The shared durable verdict boundary. Settlement is admitted only after the
/// exact verdict is written; gold, errors and refused downgrades never settle.
pub(super) fn persist_verdict(
    verdict: &SweVerdict,
    is_gold: bool,
) -> Result<Option<PathBuf>, String> {
    if is_gold || verdict.error.is_some() || verdict.instance_id.is_empty() {
        return Ok(None);
    }
    let dir = verdict_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let path = verdict_path(&verdict.instance_id);
    // STAMP THE HARNESS at the ONE seam every durable verdict passes through, so
    // provenance cannot be forgotten by a caller. A verdict that does not say
    // which build scored it makes staleness an mtime someone has to notice —
    // and staleness is not cosmetic here: regrading one stale verdict from its
    // identical banked patch moved p2p 0/40 -> 40/40 (2026-08-28).
    let mut verdict = verdict.clone();
    verdict.harness_build = env!("CONTINUUM_BUILD_GIT_SHA").to_string();
    verdict.graded_at_ms = EpochMs::now().millis();
    verdict.served_model = crate::inference::llama_server::current_serving()
        .active_model
        .unwrap_or_default(); // unwrap_or: nothing serving at grade time = honest empty, never a guess
    let verdict = &verdict;
    let body = serde_json::to_string_pretty(verdict).map_err(|e| e.to_string())?;
    // A resolve on disk is never replaced by a later failure — the losing verdict goes to
    // the instance's regrade history instead, where it stays readable.
    if verdict_write_for(read_verdict(&verdict.instance_id).as_ref(), verdict)
        == VerdictWrite::KeepTheResolve
    {
        let history = regrade_history_path(&verdict.instance_id);
        let line = serde_json::to_string(verdict).map_err(|e| e.to_string())?; // file format on disk: one JSON object per line in the instance's regrade history, read back by the score projection and by hand
        use std::io::Write as _;
        let appended = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&history)
            .and_then(|mut f| writeln!(f, "{line}"));
        crate::probe!(
            class = "benchmark.verdict.downgrade_refused",
            instance = verdict.instance_id.as_str(),
            f2p = format!("{}/{}", verdict.f2p_passed, verdict.f2p_total),
            history_written = appended.is_ok(),
            "another citizen's card already resolved this instance — her outcome stands and this later attempt is recorded as a regrade beside it"
        );
        return Ok(None);
    }
    std::fs::write(&path, body).map_err(|e| format!("write {}: {e}", path.display()))?;
    // The verdict is the OUTCOME every staged work turn on this instance's cards
    // was waiting for: PASS lifts them into the makers' curricula, FAIL discards.
    Ok(Some(path))
}

/// Read this instance's recorded verdict, or `None` if it has never been scored.
pub fn read_verdict(instance_id: &str) -> Option<SweVerdict> {
    let text = std::fs::read_to_string(verdict_path(instance_id)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Every instance that carries a durable verdict, as `(instance_id, verdict)`.
/// The board's source of "has this been scored", so an artifact stops reading `ungraded`
/// the moment a real grade lands — and keeps reading scored across reboots.
pub fn recorded_verdicts() -> Vec<(String, SweVerdict)> {
    let Ok(entries) = std::fs::read_dir(verdict_dir()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(v) = serde_json::from_str::<SweVerdict>(&text) {
            if !v.instance_id.is_empty() {
                out.push((v.instance_id.clone(), v));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}
