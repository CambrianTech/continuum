//! SWE-bench: the repo-test grader, in Rust.
//!
//! A SWE-bench instance is a real GitHub issue plus the repo at the commit before its fix. A
//! solution is a patch. The protocol is fixed and small:
//!
//! ```text
//! clone @ base_commit -> apply model_patch -> apply test_patch -> run tests
//! RESOLVED  iff  every FAIL_TO_PASS passes AND every PASS_TO_PASS passes
//! ```
//!
//! **Why this is Rust and not a script.** Benchmarks are the curriculum spine — they feed
//! training signal, eval integrity, and the paper claim — so every benchmark operation is a
//! command with a handle and events, exactly like a model download
//! ([[benchmark-infra-is-substrate-commands-handles-events-never-bash]]). The Python harness
//! this replaces cost, in one morning: a zsh word-splitting bug that silently ran the whole
//! instance list as a single bogus id, a 60-minute blind poll loop whose output was buffered
//! so a dead dispatch looked identical to a working one, and no liveness signal at all. None
//! of those are Python's fault in principle and all of them are what hand-rolled harnesses
//! actually do in practice.
//!
//! **The one thing that stays Python is the SUBJECT.** flask's and sympy's test suites are
//! pytest; running them is what the benchmark IS, no more a dependency of ours than `rustc`
//! is for `humaneval-rs`. We never write Python — we invoke the artifact's own tests and read
//! the verdicts. `uv` (itself a Rust binary) builds the per-instance environment.
//!
//! Three hazards are load-bearing here, each learned the expensive way today:
//!
//! 1. **The gate.** FAIL_TO_PASS must FAIL on the pristine tree. Eight runs were scored
//!    against a clone left at HEAD where the fix already existed, so 0 and 1 meant the same
//!    thing ([[the-swe-workspace-was-never-at-the-base-commit-the-bug-was-not-there]]).
//! 2. **Id shape.** sympy's FAIL_TO_PASS entries are bare function names, not pytest node
//!    ids. Feeding those to pytest as paths yields "file or directory not found", which scores
//!    as a failure and mis-scored GOLD on three instances whose environments were fine.
//! 3. **Era.** Both the dependency graph AND the interpreter have one. 2014 code does
//!    `from collections import Mapping`, deleted in 3.10; no pin rescues that.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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
    #[serde(rename = "FAIL_TO_PASS", default)]
    pub fail_to_pass: String,
    #[serde(rename = "PASS_TO_PASS", default)]
    pub pass_to_pass: String,
}

impl SweInstance {
    /// Test-id lists arrive JSON-encoded inside a string field.
    pub fn f2p(&self) -> Vec<String> {
        serde_json::from_str(&self.fail_to_pass).unwrap_or_default()
    }
    pub fn p2p(&self) -> Vec<String> {
        serde_json::from_str(&self.pass_to_pass).unwrap_or_default()
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
    /// Set when the run could not produce a verdict at all (clone, patch, or env failure).
    /// A verdict with `error` set is NOT a zero — it is an absence, and must never be
    /// tallied as a failed attempt.
    pub error: Option<String>,
    /// The NAMES of the tests that failed, sorted, so a verdict can teach.
    /// "PASS_TO_PASS 6/11" is a count with nothing to act on; "your patch broke
    /// test_arguments and test_unit" is what a human reviewer would say (Joel,
    /// 2026-08-08: "you or any human could tell him what's wrong — or the grader").
    /// This is what the experience stream and the room verdict carry forward.
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
}

/// Where a detached benchmark run journals its state. One file per run, rewritten in place:
/// `state: "running"` at dispatch, then the verdict — or a killed-by-reboot marker.
///
/// Honours `CONTINUUM_HOME` the same way every other progress-ledger reader does
/// (`benchmark::continuum_home`). It did not until 2026-08-18, which made this the SECOND
/// way to spell the ledger root — and a reader that resolves a directory differently from
/// the writer reports a self-consistent lie about an empty dir, the same failure shape the
/// `swe_cache_dir` doc records for env coverage.
pub fn solve_ledger_dir() -> PathBuf {
    crate::commands::benchmark::continuum_home()
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
pub fn reap_orphaned_solve_runs_in(dir: &Path) -> Vec<String> {
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
        obj.insert("instance".into(), serde_json::Value::String(instance));
        obj.insert(
            "error".into(),
            serde_json::Value::String(
                "killed by a core restart — the run was in flight when the core that owned it \
                 went away. Nothing was scored; re-dispatch to measure this instance."
                    .into(),
            ),
        );
        if std::fs::write(&path, record.to_string()).is_ok() {
            reaped.push(run_id);
        }
    }
    reaped
}

/// Convenience over the real ledger dir.
pub fn reap_orphaned_solve_runs() -> Vec<String> {
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
    crate::commands::benchmark::continuum_home()
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
pub fn record_verdict(verdict: &SweVerdict, is_gold: bool) -> Result<Option<PathBuf>, String> {
    if is_gold || verdict.error.is_some() || verdict.instance_id.is_empty() {
        return Ok(None);
    }
    let dir = verdict_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let path = verdict_path(&verdict.instance_id);
    let body = serde_json::to_string_pretty(verdict).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| format!("write {}: {e}", path.display()))?;
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

/// Fetch a dataset split, cached on first use. On-demand, never a gated install step.
pub async fn load_dataset(dataset: &str) -> Result<Vec<SweInstance>, String> {
    let cache = swe_cache_dir().join(format!("{}.json", dataset.replace('/', "__")));
    if let Ok(bytes) = std::fs::read(&cache) {
        if let Ok(rows) = serde_json::from_slice::<Vec<SweInstance>>(&bytes) {
            if !rows.is_empty() {
                return Ok(rows);
            }
        }
    }
    let mut rows: Vec<SweInstance> = Vec::new();
    // The datasets-server caps a page at 100; SWE-bench Lite is 300.
    for offset in (0..2000).step_by(100) {
        let url = format!(
            "https://datasets-server.huggingface.co/rows?dataset={}&config=default&split=test&offset={}&length=100",
            urlencoding_encode(dataset),
            offset
        );
        let resp = reqwest::get(&url)
            .await
            .map_err(|e| format!("dataset fetch failed at offset {offset}: {e}"))?;
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("dataset decode failed at offset {offset}: {e}"))?;
        let page = body
            .get("rows")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();
        if page.is_empty() {
            break;
        }
        for entry in page {
            if let Some(row) = entry.get("row") {
                if let Ok(inst) = serde_json::from_value::<SweInstance>(row.clone()) {
                    rows.push(inst);
                }
            }
        }
    }
    if rows.is_empty() {
        return Err(format!("{dataset} returned no usable rows"));
    }
    let _ = std::fs::create_dir_all(swe_cache_dir());
    let _ = std::fs::write(&cache, serde_json::to_vec(&rows).unwrap_or_default());
    Ok(rows)
}

/// Minimal percent-encoding for the one query param we send (the dataset name's `/`).
fn urlencoding_encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            other => format!("%{:02X}", other as u32),
        })
        .collect()
}

/// Ceiling on any single grader subprocess. Nothing legitimate here — a git
/// mirror fetch, a uv venv build, one instance's full pytest files — takes
/// this long; the class this kills is INFINITE (task #381: sympy
/// symbolic-computation hangs orphaned to launchd at 100% CPU for ELEVEN
/// HOURS, detected by the operator's cooling fan, not by any instrument).
const SUBPROCESS_CEILING: std::time::Duration = std::time::Duration::from_secs(15 * 60);

pub(crate) async fn run(
    program: &str,
    args: &[&str],
    cwd: Option<&Path>,
) -> Result<std::process::Output, String> {
    run_env(program, args, cwd, &[]).await
}

/// `run` with extra environment variables — the seam era C-extension builds need
/// (see `ERA_CFLAGS`). Everything else (process group, ceiling, kill semantics)
/// is identical; `run` delegates here so there is exactly one subprocess path.
pub(crate) async fn run_env(
    program: &str,
    args: &[&str],
    cwd: Option<&Path>,
    envs: &[(&str, &str)],
) -> Result<std::process::Output, String> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    for (k, v) in envs {
        cmd.env(k, v);
    }
    // #381: the child leads its own PROCESS GROUP so a kill reaches the whole
    // tree — pytest spawns grandchildren (sympy's own runner, plugins) that
    // survive a parent-only kill and orphan to launchd. kill_on_drop covers
    // the drop path (a core reboot cancels this future mid-await); the
    // explicit killpg below covers the grandchildren the drop-kill misses.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(cmd.as_std_mut(), 0);
    cmd.kill_on_drop(true);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let child = cmd
        .spawn()
        .map_err(|e| format!("could not run `{program}`: {e}"))?;
    let child_pid = child.id();
    match tokio::time::timeout(SUBPROCESS_CEILING, child.wait_with_output()).await {
        Ok(out) => out.map_err(|e| format!("could not run `{program}`: {e}")),
        Err(_elapsed) => {
            // The timed-out future was just dropped, killing the DIRECT child;
            // now kill its group (pgid == child pid, it was made group leader
            // at spawn) so grandchildren die with it. Windows has no process
            // groups in this shape — kill_on_drop alone is the story there.
            #[cfg(unix)]
            if let Some(pid) = child_pid {
                unsafe {
                    libc::killpg(pid as i32, libc::SIGKILL);
                }
            }
            crate::probe!(
                class = "benchmark.subprocess.ceiling",
                program = %program,
                ceiling_s = SUBPROCESS_CEILING.as_secs(),
                "grader subprocess exceeded the ceiling — whole process group killed \
                 (a hung test run is an environment fault, never a verdict)"
            );
            Err(format!(
                "`{program}` exceeded the {}s subprocess ceiling and was killed \
                 (whole process group) — a hung run is an environment fault, \
                 never a verdict (task #381)",
                SUBPROCESS_CEILING.as_secs()
            ))
        }
    }
}

/// Clone the repo at `base_commit`. The commit is the whole point — a clone left at HEAD is
/// how eight runs got scored against a tree with the fix already in it.
pub async fn clone_at(instance: &SweInstance, repo_dir: &Path) -> Result<(), String> {
    // A stale tree here is not "probably fine" — it is the tree the score comes from. Removal
    // failing used to be SWALLOWED (`let _ =`), and the clone below then died on git's own
    // "destination path already exists and is not an empty directory", which reads like a
    // harness bug rather than a filesystem one. Say which it is.
    if repo_dir.exists() {
        std::fs::remove_dir_all(repo_dir).map_err(|e| {
            format!("could not clear the stale grade tree {}: {e}", repo_dir.display())
        })?;
    }
    // The PARENT must exist first. `git clone` creates its target directory but not the chain
    // above it, and the failure surfaces late and cryptically — as a mid-fetch "unable to write
    // .git/objects/pack/*.pack: No such file or directory", which reads like a disk or network
    // fault rather than a missing mkdir.
    if let Some(parent) = repo_dir.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    // ONE network fetch per REPO, not per clone.
    //
    // The protocol needs two trees per instance — the one she works in and a pristine one to
    // score in — and a sweep re-grades the same repo across many instances. Cloning from
    // GitHub each time was ~240 MB of network AND disk per tree: a 300-instance Lite sweep
    // would have been ~140 GB and hours of fetch. A local mirror plus `--shared` clones makes
    // every tree after the first nearly free (objects are borrowed, not copied) and costs one
    // fetch per repo for the whole sweep.
    //
    // `--shared` is safe precisely because these trees are DISPOSABLE: nothing here is ever
    // pushed, and the mirror is only ever fast-forwarded. The usual caveat — pruning the parent
    // can corrupt a borrower — does not apply to a cache we recreate from scratch.
    let mirror = swe_cache_dir()
        .join("mirrors")
        .join(instance.repo.replace('/', "__"));
    let url = format!("https://github.com/{}.git", instance.repo);
    if !mirror.exists() {
        if let Some(parent) = mirror.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let out = run(
            "git",
            &[
                "clone",
                "--quiet",
                "--bare",
                &url,
                &mirror.to_string_lossy(),
            ],
            None,
        )
        .await?;
        if !out.status.success() {
            return Err(format!(
                "mirror clone of {} failed: {}",
                instance.repo,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }

    // Clone into a STAGING path and move it into place, never straight into `repo_dir`.
    //
    // `git clone` refuses any destination that is not empty, and on macOS `repo_dir` does not
    // stay empty on its own: Finder / Spotlight drop a `.DS_Store` into a directory the instant
    // they notice it. Measured 2026-08-17 — a re-grade of astropy-14995 failed with
    // "destination path already exists and is not an empty directory" against a directory whose
    // ONLY content was a `.DS_Store` stamped AFTER the remove above. The window between
    // remove and clone was enough. A grade voided by an OS indexer is indistinguishable, from
    // the receipt, from a grade voided by the harness — so close the window instead of
    // widening the diagnosis. Staging + rename also means a half-fetched tree is never
    // visible at `repo_dir`: the move is the commit point.
    let staging = repo_dir.with_extension(format!(
        "cloning-{}",
        std::process::id()
    ));
    if staging.exists() {
        std::fs::remove_dir_all(&staging).map_err(|e| {
            format!("could not clear the stale staging tree {}: {e}", staging.display())
        })?;
    }
    let out = run(
        "git",
        &[
            "clone",
            "--quiet",
            "--shared",
            &mirror.to_string_lossy(),
            &staging.to_string_lossy(),
        ],
        None,
    )
    .await?;
    if !out.status.success() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!(
            "clone of {} from its local mirror failed: {}",
            instance.repo,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    // Anything that reappeared at the destination while we fetched (see above) loses to the
    // tree we just built.
    if repo_dir.exists() {
        std::fs::remove_dir_all(repo_dir).map_err(|e| {
            format!("could not clear {} before staging the fresh clone: {e}", repo_dir.display())
        })?;
    }
    std::fs::rename(&staging, repo_dir).map_err(|e| {
        format!(
            "could not move the fresh clone {} into place at {}: {e}",
            staging.display(),
            repo_dir.display()
        )
    })?;
    let out = run(
        "git",
        &["checkout", "--quiet", &instance.base_commit],
        Some(repo_dir),
    )
    .await?;
    if !out.status.success() {
        // A mirror created earlier can predate this instance's base_commit. Refresh it once and
        // retry rather than failing — the alternative is a cache that silently rots into
        // "instance not gradeable" as the dataset grows.
        let _ = run("git", &["fetch", "--quiet", "--all"], Some(&mirror)).await;
        let _ = run("git", &["fetch", "--quiet", "origin"], Some(repo_dir)).await;
        let retry = run(
            "git",
            &["checkout", "--quiet", &instance.base_commit],
            Some(repo_dir),
        )
        .await?;
        if !retry.status.success() {
            return Err(format!(
                "checkout {} failed even after refreshing the local mirror: {}",
                &instance.base_commit[..12.min(instance.base_commit.len())],
                String::from_utf8_lossy(&retry.stderr).trim()
            ));
        }
    }
    Ok(())
}

/// Apply a patch, tolerating the whitespace drift that trips a strict apply.
pub async fn apply_patch(repo_dir: &Path, text: &str, what: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Ok(());
    }
    let path = repo_dir.join(format!(".{what}.patch"));
    std::fs::write(&path, text).map_err(|e| format!("could not stage {what} patch: {e}"))?;
    let p = path.to_string_lossy().to_string();
    // Last arm is --3way: when the candidate patch already edited lines the test_patch's
    // context touches, exact/relaxed apply fails ("tree is not what the patch expects" —
    // flask-4992 + sphinx-10451 graded INFRA on this, Round A). 3-way merges against the
    // blob ids in the patch header instead, succeeding wherever the edits don't genuinely
    // overlap — and a genuine overlap still fails loudly, which is the honest outcome.
    for extra in [
        vec![],
        vec!["--ignore-whitespace"],
        vec!["--ignore-whitespace", "-C1"],
        vec!["--3way"],
    ] {
        let mut args = vec!["apply"];
        args.extend(extra);
        args.push(&p);
        if let Ok(out) = run("git", &args, Some(repo_dir)).await {
            if out.status.success() {
                let _ = std::fs::remove_file(&path);
                return Ok(());
            }
        }
    }
    Err(format!(
        "could not apply {what} patch — the tree is not what the patch expects"
    ))
}

/// The interpreter an instance's own code could actually have run on.
///
/// 3.9 is the last release carrying the `collections.Mapping` aliases pre-2020 code reaches
/// for; uv's own floor is 3.8, so there is no lower rung to offer. Coarse on purpose — the
/// gold gate is the arbiter, and a wrong guess fails loudly rather than producing a
/// plausible number.
///
/// The 2020..=2022 rung is 3.10, learned live (pylint-5859, 2026-08-11): Python 3.11
/// removed `inspect.formatargspec`, which era sdist BUILDS still import (wrapt 1.13.3,
/// pulled by 2022 astroid) — every 2022-era env with such a dep died at build on 3.11.
/// 3.11 released 2022-10; a March-2022 dependency graph never targeted it. The same
/// mismatch is the leading suspect for the p2p-0/N broken baselines (flask-5063,
/// pytest-5103) whose envs were built on 3.11 before this rung existed.
pub fn interpreter_for_year(year: u32) -> &'static str {
    if year < 2020 {
        "3.9"
    } else if year <= 2022 {
        "3.10"
    } else {
        "3.11"
    }
}

/// Build (or reuse) the per-instance environment. Per-instance rather than per-repo because
/// instances span years of a repo's history and their dependency graphs genuinely differ.
/// The repo's declared build-time dependencies, from `[build-system].requires` in its
/// `pyproject.toml`. Empty when there is no pyproject, no `[build-system]` table, or no
/// `requires` array. These must be pre-installed into the venv when building with
/// `--no-build-isolation` (see the call site) — pip won't fetch them for us in that mode.
/// The name of the repo's own TEST extra, from `[project.optional-dependencies]` — the
/// group whose contents the suite needs to so much as COLLECT.
///
/// Why this exists (measured 2026-08-17, and it is the whole of astropy's 0-of-9): the env
/// installs `-e .`, which resolves RUNTIME dependencies only. astropy's `conftest.py` opens
/// with `import hypothesis`, a TEST dependency, so pytest dies loading conftest before it
/// reaches a single test:
///
/// ```text
/// ImportError while loading conftest '…/repo/conftest.py'.
/// conftest.py:9: in <module>
///     import hypothesis
/// E   ModuleNotFoundError: No module named 'hypothesis'
/// ```
///
/// PASS_TO_PASS then reads 0/9 on the PRISTINE tree, the gold gate correctly refuses to
/// score, and EVERY astropy instance is ungradeable — a ceiling that has nothing to do with
/// the citizen's patch. The repo already declares exactly what its suite needs; we simply
/// never asked for it.
///
/// Names are not standardised, so prefer in the order the ecosystem actually uses them and
/// take the first that the repo declares. Returns `None` when there is no pyproject, no
/// optional-dependencies table, or no test-shaped group — those repos install as before.
/// The `--exclude-newer` cutoff for an instance: its own creation date, or `None` when the
/// dataset carries no date (which disables pinning AND healing — see `era_pinned_uv_install`).
/// Extracted because BOTH the fresh-build path and the cached-env heal need the same answer,
/// and two copies of "what era is this instance" is exactly how the two drift apart.
fn era_cutoff(instance: &SweInstance) -> Option<String> {
    if instance.created_at.is_empty() {
        None
    } else {
        Some(instance.created_at.clone())
    }
}

fn test_extra_name(repo_dir: &Path) -> Option<String> {
    const PREFERRED: [&str; 4] = ["test", "tests", "testing", "dev"];
    let declared = declared_extras(repo_dir);
    PREFERRED
        .iter()
        .find(|name| declared.iter().any(|d| d == *name))
        .map(|name| (*name).to_string())
}

/// Every extra group the repo declares, from BOTH places Python puts them.
///
/// The first version of this read `pyproject.toml` only and shipped believing it worked —
/// the live run said otherwise (2026-08-17): astropy 5.3 declares its suite deps in
/// `setup.cfg` under `[options.extras_require]`, has no `[project.optional-dependencies]`
/// at all, and the pyproject-only parser returned None for the exact repo the fix was
/// written for. Both files are ordinary in the ecosystem; reading one is reading half.
fn declared_extras(repo_dir: &Path) -> Vec<String> {
    let mut found = Vec::new();

    // PEP 621.
    if let Ok(text) = std::fs::read_to_string(repo_dir.join("pyproject.toml")) {
        if let Ok(parsed) = toml::from_str::<toml::Value>(&text) {
            if let Some(table) = parsed
                .get("project")
                .and_then(|p| p.get("optional-dependencies"))
                .and_then(|o| o.as_table())
            {
                found.extend(table.keys().cloned());
            }
        }
    }

    // setuptools' declarative config. Hand-scanned rather than pulled in as a dependency:
    // we need section keys, not values, and an INI parser for that is more surface than
    // the six lines below.
    if let Ok(text) = std::fs::read_to_string(repo_dir.join("setup.cfg")) {
        let mut in_extras = false;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                in_extras = trimmed == "[options.extras_require]";
                continue;
            }
            // A key is flush-left; anything indented is a continuation of the previous
            // key's requirement list (`test =\n    pytest>=7.0`), never a group name.
            if !in_extras || line.starts_with([' ', '\t']) {
                continue;
            }
            if let Some((key, _)) = trimmed.split_once('=') {
                let key = key.trim();
                if !key.is_empty() {
                    found.push(key.to_string());
                }
            }
        }
    }
    found
}

/// Install the repo's test extra into an existing env, once. Non-fatal BY DESIGN: a repo
/// whose suite already collects does not need it, and a resolve failure here must not
/// delete an env that grades fine today. The marker file is what makes it once-only, and
/// what lets envs built BEFORE this existed heal themselves on their next use instead of
/// staying silently ungradeable forever.
async fn ensure_test_extra(
    uv: &str,
    py_s: &str,
    env_dir: &Path,
    repo_dir: &Path,
    as_of: Option<&str>,
) {
    let marker = env_dir.join(".test-extra");
    // A marker is authoritative only when it NAMES the extra it installed. Markers written
    // by the first version recorded "none-declared" from a half-blind parser; treating
    // those as settled would make this fix unreachable on exactly the envs that need it,
    // and would need a human to know which files to delete. They self-heal instead.
    if std::fs::read_to_string(&marker)
        .map(|m| !m.trim().is_empty() && m.trim() != "none-declared")
        .unwrap_or(false)
    {
        return;
    }
    let Some(extra) = test_extra_name(repo_dir) else {
        // NO NEGATIVE MARKER. The first version wrote "none-declared" here to avoid
        // re-parsing, and then a parser that read only pyproject.toml recorded that answer
        // PERMANENTLY for astropy — whose extras live in setup.cfg — so the heal could
        // never run again even after the parser was fixed. Caching a negative is caching
        // the limits of today's code. Re-reading two small files per grade costs nothing
        // measurable next to cloning a repo and running its suite.
        return;
    };
    let spec = format!(".[{extra}]");
    let outcome = era_pinned_uv_install(
        uv,
        py_s,
        as_of,
        &["--no-build-isolation", "-e", &spec],
        Some(repo_dir),
        &[("CFLAGS", ERA_CFLAGS)],
    )
    .await;
    let ok = matches!(&outcome, Ok(o) if o.status.success());
    crate::probe!(
        class = "swe.env.test_extra",
        extra = %extra,
        installed = ok,
        env = %env_dir.display(),
        "the repo's own test extra — without it a conftest that imports a test-only \
         dependency makes every instance in the repo ungradeable"
    );
    if ok {
        let _ = std::fs::write(&marker, format!("{extra}\n"));
    }
}

fn build_requires(repo_dir: &Path) -> Vec<String> {
    let text = match std::fs::read_to_string(repo_dir.join("pyproject.toml")) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let parsed: toml::Value = match toml::from_str(&text) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    parsed
        .get("build-system")
        .and_then(|bs| bs.get("requires"))
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Era C code vs MODERN clang (#383 cause 2's second head, measured live 2026-08-16 on
/// astropy__astropy-12907 right after the jinja2 fix moved the failure downstream):
/// Xcode 15+ clang promotes `-Wincompatible-function-pointer-types`,
/// `-Wimplicit-function-declaration`, and `-Wint-conversion` to hard ERRORS. astropy 2022's
/// wcslib wrappers (`wcslib_wtbarr_wrap.c`: `tp_traverse` assigned an `int (PyWtbarr *, …)`
/// where `traverseproc` wants `int (PyObject *, …)`) compiled as WARNINGS on the era's own
/// compilers — the code under test genuinely ran this way. The compiler is HARNESS (it runs
/// on this machine), the C is SUBJECT: demote exactly those three diagnostics back to the
/// warnings they were. distutils APPENDS `CFLAGS` to its sysconfig baseline, so nothing else
/// about the build changes, and modern code that doesn't trip them is untouched.
///
/// The FOURTH head (#383, measured live 2026-08-17 on astropy__astropy-14182, and the
/// reason two dispatched rounds died at env-build after the jinja2 + build-requires fixes
/// both landed): astropy vendors cfitsio, which vendors a 1990s zlib, whose
/// `cextern/cfitsio/zlib/zutil.h:140` reads
///
/// ```c
/// #if defined(MACOS) || defined(TARGET_OS_MAC)
/// #  define OS_CODE  7
/// #    ifndef fdopen
/// #      define fdopen(fd,mode) NULL /* No fdopen() */
/// ```
///
/// `TARGET_OS_MAC` is 1 on EVERY modern Apple SDK — it means "some Apple platform", not
/// "classic Mac OS" as it did when this zlib was written. So the branch fires, `fdopen` is
/// macro-replaced by `NULL`, and the system header's own declaration
/// `FILE *fdopen(int, const char *)` becomes `FILE *NULL(int, const char *)` →
/// `error: expected identifier or '('` in `<stdio.h>`, thousands of lines from anything
/// astropy wrote. (The adjacent `'OS_CODE' macro redefined` warning is the same branch.)
///
/// The guard is `#ifndef fdopen`, so pre-defining it is the whole fix: `-Dfdopen=fdopen`
/// makes the guard FALSE — the NULL stub is never emitted — and the macro itself is the
/// identity, so every real `fdopen` call compiles to `fdopen`. Nothing is stubbed, nothing
/// is renamed, no source is patched, and a repo that does not vendor this zlib never
/// notices the flag.
///
/// It lives here rather than in a per-repo table because it is not an astropy fact — it is
/// an ERA fact (old vendored zlib vs a modern Apple SDK), identical in shape to the three
/// above: the compiler is HARNESS, the C is SUBJECT, and the subject built fine on the
/// compilers of its own day.
const ERA_CFLAGS: &str = "-Wno-error=incompatible-function-pointer-types \
     -Wno-error=implicit-function-declaration -Wno-error=int-conversion \
     -Dfdopen=fdopen";

/// Build deps that a repo's DEPENDENCY sdists import at build time but that nothing installs
/// under `--no-build-isolation` (we honor the top repo's `[build-system].requires`; a
/// dependency sdist's declaration is honored by nobody). Keyed by repo because which sdists
/// get built is a property of the repo's dependency graph on this platform. DATA, not logic —
/// grow it one measured failure at a time, never speculatively.
fn era_sdist_build_deps(repo: &str) -> &'static [&'static str] {
    match repo {
        // pyerfa's sdist build runs `erfa_generator`, which imports jinja2
        // (astropy__astropy-12907 live at dispatch, 2026-08-16).
        "astropy/astropy" => &["jinja2"],
        _ => &[],
    }
}

pub async fn ensure_env(instance: &SweInstance, repo_dir: &Path) -> Result<PathBuf, String> {
    let env_dir = swe_cache_dir().join("envs").join(&instance.instance_id);
    let py = env_dir.join("bin").join("python");
    if py.exists() {
        // THE EDITABLE POINTS SOMEWHERE (root cause of the "2019-pytest era" + "flask-2.2
        // era" env-void classes, glass-boxed 2026-08-12): the env is cached per INSTANCE,
        // but its `-e .` install pins the ABSOLUTE PATH of whichever tree built it first —
        // the solver's workspace. A flat-layout repo (sympy, pylint) survives because the
        // grade clone's cwd shadows the import; a src/-layout repo (pytest 4.x, flask 2.2)
        // does not, so the grade imported the persona's DIRTY WORKSPACE code and pristine
        // p2p read 0/N (six instances voided). Re-point the editable at THIS caller's tree:
        // `--no-deps` skips dependency resolution (the graph is already in the venv), so
        // this is a seconds-cheap metadata rebuild, and solve/grade run serially per
        // instance so there is no cross-tree race.
        if let Some(uv) = which("uv") {
            let py_s = py.to_string_lossy().to_string();
            let out = run_env(
                &uv,
                &[
                    "pip",
                    "install",
                    "-q",
                    "--python",
                    &py_s,
                    "--no-build-isolation",
                    "--no-deps",
                    "-e",
                    ".",
                ],
                Some(repo_dir),
                &[("CFLAGS", ERA_CFLAGS)],
            )
            .await?;
            if !out.status.success() {
                return Err(format!(
                    "could not re-point {}'s cached env at {}: {}",
                    instance.instance_id,
                    repo_dir.display(),
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
            // An env cached BEFORE the test extra existed is silently ungradeable (its
            // conftest cannot import). Heal it here, once, guarded by the marker — the
            // operator should never have to know which envs predate a fix.
            ensure_test_extra(&uv, &py_s, &env_dir, repo_dir, era_cutoff(instance).as_deref())
                .await;
        }
        return Ok(py);
    }
    let _ = std::fs::create_dir_all(env_dir.parent().unwrap_or(&env_dir));
    let uv = which("uv").ok_or_else(|| {
        "uv is not installed — it builds the per-instance environment (a Rust binary, \
         install from https://astral.sh/uv)"
            .to_string()
    })?;
    let interpreter = interpreter_for_year(instance.year());
    let env_s = env_dir.to_string_lossy().to_string();
    let py_s = py.to_string_lossy().to_string();

    let out = run(&uv, &["venv", "--python", interpreter, "-q", &env_s], None).await?;
    if !out.status.success() {
        return Err(format!(
            "could not create a Python {interpreter} venv: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    // THE SPLIT: SUBJECT vs HARNESS.
    //
    // Subject — the repo's own runtime dependency graph — is pinned to the instance's date,
    // because those versions define the behavior under test. A plain install resolves 2026
    // packages against 2021 code: flask 2.0 asks for `Werkzeug>=2.0`, gets 3.x, which deleted
    // `url_quote`, and the repo cannot even import.
    //
    // Harness — pytest, setuptools, wheel — is deliberately MODERN, because it has to run on
    // this machine's interpreter. Date-pinning it breaks in ways unrelated to the instance:
    // 2021 `py` raises `AttributeError: __spec__` under 3.11's import machinery, and 2021
    // setuptools predates PEP 660 so `build_meta:__legacy__` has no `build_editable`.
    //
    // `--no-build-isolation` is what lets the two coexist: the build runs against the modern
    // setuptools already in the venv instead of pip fetching a date-pinned one.
    //
    // setuptools is pinned `<70`, NOT bare-latest. Two era-2020..2022 build requirements fight:
    // an editable install (`-e .`) needs PEP 660 `build_editable`, which landed in setuptools 64;
    // and those repos' C-extension setup code imports legacy APIs like `setuptools.dep_util`
    // (astropy's `wcs/setup_package.py`), which setuptools REMOVED in 70.0. setuptools 69.x is the
    // only version that has BOTH — bare-latest (>=70) builds the pure-Python repos but dies on
    // every C-extension instance with `ModuleNotFoundError: setuptools.dep_util`. This is the whole
    // era class, not one repo (#380 "pin era deps"). pytest/wheel stay latest.
    let _ = run(
        &uv,
        &[
            "pip",
            "install",
            "-q",
            "--python",
            &py_s,
            "pytest",
            "setuptools<70",
            "wheel",
        ],
        None,
    )
    .await?;

    // BUILD REQUIRES: `--no-build-isolation` means pip will NOT fetch the repo's declared
    // build-time dependencies — it builds against whatever is already in the venv. A repo whose
    // setup.py imports a build helper (astropy: `import extension_helpers`; any C-extension repo:
    // cython, numpy) therefore fails with `ModuleNotFoundError` at the metadata step unless we
    // pre-install what its own `[build-system].requires` declares. We honor that declaration
    // rather than hardcoding per-repo build deps — the pyproject IS the source of truth. Installed
    // MODERN (no date-pin) like pytest/setuptools/wheel above: these are build harness, run on
    // THIS interpreter, and carry their own version pins where they matter (e.g. cython==0.29.22).
    let build_reqs = build_requires(repo_dir);
    if !build_reqs.is_empty() {
        let mut breq_args = vec!["pip", "install", "-q", "--python", &py_s];
        breq_args.extend(build_reqs.iter().map(String::as_str));
        let out = run(&uv, &breq_args, None).await?;
        if !out.status.success() {
            // Non-fatal: some declared build deps (e.g. `oldest-supported-numpy` on a fresh
            // interpreter) may not resolve, yet the build can still succeed against the modern
            // setuptools already present. Let the `-e .` step below be the real gate; surface this
            // only as a breadcrumb if that step then fails.
            tracing::warn!(
                instance = %instance.instance_id,
                requires = ?build_reqs,
                stderr = %String::from_utf8_lossy(&out.stderr).trim(),
                "build-system.requires install had non-zero exit — proceeding to -e . anyway"
            );
        }
        // RE-ASSERT THE HARNESS FLOOR (pylint-7114, live 2026-08-12): a repo whose own
        // `[build-system].requires` pins an OLDER setuptools (pylint 2022 pins ~62.6)
        // just CLOBBERED the 69.x we installed above — and setuptools <64 predates PEP
        // 660, so the editable build dies with "build_meta has no attribute
        // build_editable". Honoring the repo's declaration is right for ITS build
        // helpers (cython, extension-helpers); setuptools itself is HARNESS, and the
        // window [64, 70) is the only range with BOTH build_editable (>=64) and
        // dep_util (<70) — the same two-sided constraint documented above.
        let _ = run(
            &uv,
            &[
                "pip",
                "install",
                "-q",
                "--python",
                &py_s,
                "setuptools>=64,<70",
            ],
            None,
        )
        .await?;
    }

    let as_of = era_cutoff(instance);
    let repo_s = repo_dir.to_string_lossy().to_string();

    // DEPENDENCY-SDIST BUILD DEPS (#383 cause 2, reproduced live 2026-08-16): the
    // `build_requires` step above honors the TOP repo's `[build-system].requires`, but
    // `--no-build-isolation` extends to every DEPENDENCY built from sdist too — and their
    // declared build deps are honored by NOBODY. astropy pulls `pyerfa`, whose sdist runs an
    // `erfa_generator` step importing jinja2 at build time; with no wheel for this
    // interpreter/arch the sdist build is mandatory, and the whole env died with
    // `ModuleNotFoundError: No module named 'jinja2'` — every astropy instance ungradeable.
    // The table is DATA (repo → the build deps its dependency sdists need), the same shape as
    // the official harness's per-repo spec tables. Era-pinned with the instance's own cutoff
    // so a build tool never smuggles a modern package into a date-pinned subject graph
    // (jinja2 IS a runtime dep of some subjects, e.g. flask).
    let sdist_deps = era_sdist_build_deps(&instance.repo);
    if !sdist_deps.is_empty() {
        // Shares `era_pinned_uv_install` with the `-e .` install below. It did NOT before,
        // and that asymmetry is what made every astropy instance ungradeable: the pin caps
        // setuptools at the 2017 cutoff while markupsafe's build requires >=40.8.0, and the
        // heal that reads uv's own `exclude-newer-package` hint lived only at the other site.
        let mut out = era_pinned_uv_install(
            &uv,
            &py_s,
            as_of.as_deref(),
            sdist_deps,
            None,
            &[("CFLAGS", ERA_CFLAGS)],
        )
        .await?;
        // BUILD TOOLS ARE NOT SUBJECT CODE — reproduced in isolation 2026-08-17: the era
        // pin on this pre-install is UNSATISFIABLE BY CONSTRUCTION on a modern box, at
        // every rung the heal can reach. Era markupsafe 1.0 (no wheel for this
        // interpreter/arch, sdist build mandatory) needs setuptools>=40.8.0 which the pin
        // excludes; LIFT setuptools (the heal's correct first move, verified firing) and
        // the 2017 sdist dies on `ImportError: cannot import name 'Feature'` (removed in
        // setuptools 46); lift markupsafe instead and era jinja2 2.10 dies at import on
        // `soft_unicode` (removed in markupsafe 2.1). The only installable combination is
        // MODERN jinja2 + MODERN markupsafe — verified importing clean.
        //
        // So when the healed era-pinned install still fails, retry ONCE unpinned, loudly.
        // Scope-safe by construction: this table lists BUILD TOOLS for dependency-sdist
        // code generators (astropy→pyerfa→jinja2), never subject requirements — a repo
        // where the package IS subject code (flask) has no entry here, and the subject
        // graph is resolved by the still-date-pinned `-e .` step below.
        if !out.status.success() && as_of.is_some() {
            tracing::warn!(
                instance = %instance.instance_id,
                deps = ?sdist_deps,
                "era-pinned sdist build-dep install unsatisfiable at every heal rung — \
                 retrying UNPINNED (build tools only; the subject graph stays date-pinned)"
            );
            out = era_pinned_uv_install(&uv, &py_s, None, sdist_deps, None, &[("CFLAGS", ERA_CFLAGS)])
                .await?;
        }
        if !out.status.success() {
            // Fail LOUD and leave no half-built env behind — same doctrine as the `-e .` gate.
            let _ = std::fs::remove_dir_all(&env_dir);
            return Err(format!(
                "could not pre-install {}'s dependency-sdist build deps {:?} — env removed \
                 rather than cached broken: {}",
                instance.instance_id,
                sdist_deps,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }

    // DELETED-HISTORY packages: `--exclude-newer <date>` assumes PyPI still carries the
    // uploads that existed at that date. It does not always — `atomicwrites` had its whole
    // pre-2022 history deleted by its author, so for any pre-2022 instance the date pin
    // leaves ZERO candidates and resolution fails outright (pytest-dev__pytest-5103, live
    // 2026-08-11). uv's error names both the package and the earliest surviving upload, and
    // its own suggested remedy is a per-package `--exclude-newer-package` cutoff. We heal
    // exactly as instructed: parse the hint, override ONLY that package to just past its
    // earliest surviving upload, and retry. The subject stays date-pinned; the override is
    // the minimum needed for a resolvable graph, discovered from the resolver's own evidence
    // rather than a hand-maintained package list. Bounded: each round must surface a NEW
    // package or we stop, and history-holes per graph are few.
    // ERA_CFLAGS rides on this invocation because it is where every C build happens — the
    // repo's own extensions AND its dependency sdists (pyerfa et al) compile inside this
    // resolve. The heal loop lives in `era_pinned_uv_install`, shared with the sdist
    // build-dep pre-install above.
    let out = era_pinned_uv_install(
        &uv,
        &py_s,
        as_of.as_deref(),
        &["--no-build-isolation", "-e", "."],
        Some(Path::new(&repo_s)),
        &[("CFLAGS", ERA_CFLAGS)],
    )
    .await?;
    if !out.status.success() {
        // DELETE the half-built env rather than cache it. Keying on "the directory exists"
        // made a failed install sticky: every later run reused a venv with no repo in it and
        // reported a gold failure whose real cause was three steps upstream.
        let _ = std::fs::remove_dir_all(&env_dir);
        return Err(format!(
            "could not install {}'s repo into a venv — a cached broken env would poison every \
             later run, so it was removed: {}",
            instance.instance_id,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    // PYTEST IS SUBJECT, NOT HARNESS, for date-pinned instances (#380, glass-boxed
    // 2026-08-12): era test suites import pytest INTERNALS — flask 2.2's test_cli.py does
    // `from _pytest.monkeypatch import notset`, deleted by modern pytest — so the
    // latest-pytest installed above voids every such baseline. The era interpreter rungs
    // (#2253) removed the original reason pytest had to be modern, so downgrade it to the
    // instance's own date. Skipped when the repo IS pytest: the editable install already
    // provides the (exactly-era) subject and a PyPI pytest would stomp it. Skipped too when
    // the repo's runner isn't pytest AT ALL (#383: django grades through its own
    // runtests.py) — era-pinning a harness the grade never invokes is dead weight, and the
    // pytest smoke gate below would DELETE a perfectly good env whenever the era pytest
    // can't execute on this interpreter, voiding instances pytest has nothing to do with.
    if as_of.is_some()
        && instance.repo != "pytest-dev/pytest"
        && runner_for_repo(&instance.repo) == TestRunner::Pytest
    {
        let date = instance.created_at.clone();
        // --reinstall is load-bearing: the modern pytest above already satisfies the bare
        // requirement, so without it this resolve is a no-op (hand-verified: flask-5063
        // stayed on 9.1.1 until --reinstall brought it to era 7.3.0, tests then green).
        let out = run(
            &uv,
            &[
                "pip",
                "install",
                "-q",
                "--python",
                &py_s,
                "--exclude-newer",
                &date,
                "--reinstall",
                "pytest",
            ],
            None,
        )
        .await?;
        if !out.status.success() {
            let _ = std::fs::remove_dir_all(&env_dir);
            return Err(format!(
                "could not era-pin pytest for {} (cutoff {date}) — env removed rather than \
                 cached broken: {}",
                instance.instance_id,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }

        // GOLD-GATE THE HARNESS (#380's other half). The era pin above assumes the era
        // INTERPRETER rung (#2253) found a matching interpreter. When it can't — no
        // Python 3.5 on a modern macOS — the venv falls back to a modern interpreter
        // while pytest is still pinned to the instance's year, and that PAIR can be
        // structurally unable to run. Glass-boxed 2026-08-13 on sympy__sympy-11400
        // (2016 → pytest 2.9.2 on Python 3.9.6): pytest dies in `pytest_configure`
        // with INTERNALERROR before collecting anything, on a two-line trivial test
        // with no conftest and no sympy involved. Every test then "fails", the
        // pristine p2p reads 0/29, and the whole tree grades UNGRADEABLE — 8 of 36
        // instances on this box.
        //
        // A version pin is a GUESS about compatibility; running it is the evidence. So
        // prove the harness executes before handing the env to a citizen.
        //
        // This REFUSES rather than self-heals, and that is deliberate — measured, not
        // assumed. The obvious repair (reinstall a modern pytest) was tried against this
        // exact instance and does NOT work: pytest 8.4.2 loads, then dies in sympy 1.0's
        // own 2016 conftest on the `py.path` hook API that pytest 7 removed; pytest 6.2.5
        // dies on `py.test.mark.slow`, removed in pytest 4. The band of pytest versions
        // that both RUN on Python 3.9 and LOAD a 2016 conftest is EMPTY, so no version
        // choice rescues this class. What does work — verified on this tree, 30/30 passing
        // — is sympy's OWN runner (`sympy.test(...)`) on the same interpreter. That is
        // #383's shape ("django needs its OWN test runner") generalised: the runner is a
        // property of the repo era, not a pytest version to search for. `run_tests` is
        // pytest-only today, so until it grows a runner seam, an env in this state cannot
        // produce a verdict and must say so loudly instead of caching a tree where every
        // test errors ([[brittleness-is-the-highest-priority-work-there-is]] — heal what is
        // known-safe, REPORT what needs a human decision; a wrong auto-repair here would
        // silently trade one void tree for another).
        if let Err(why) = smoke_test_pytest(&py, &env_dir).await {
            let _ = std::fs::remove_dir_all(&env_dir);
            return Err(format!(
                "{}'s env has no runnable test harness: era-pinned pytest (cutoff {}) \
                 cannot execute even a trivial test on this venv's interpreter, which \
                 means the era INTERPRETER rung fell back to a modern one and left an \
                 incompatible pair. Env removed rather than cached — a cached copy grades \
                 every attempt UNGRADEABLE. This repo era likely needs its own native \
                 runner rather than any pytest version (#383). Detail: {why}",
                instance.instance_id, instance.created_at
            ));
        }
    }
    // LAST, because it is additive and must not be able to fail the build: the repo's own
    // test extra. Everything above decides whether the env can BUILD and RUN a harness;
    // this decides whether the suite can COLLECT. Non-fatal by design — see
    // `ensure_test_extra`.
    ensure_test_extra(&uv, &py_s, &env_dir, repo_dir, as_of.as_deref()).await;
    Ok(py)
}

/// Can this venv's pytest actually RUN? Not "is it installed", not "does `--version`
/// answer" — both stay true for a pytest that dies in `pytest_configure` (sympy-11400:
/// `pytest --version` prints 2.9.2 happily, then INTERNALERRORs on any real run).
///
/// So: write a trivial passing test to a scratch dir OUTSIDE the repo (no conftest, no
/// subject imports — a failure here is the harness, never the code under test) and
/// require a clean exit. This is the smallest honest question, and it is the one the
/// grader's whole verdict rests on.
async fn smoke_test_pytest(py: &std::path::Path, env_dir: &std::path::Path) -> Result<(), String> {
    let smoke_dir = env_dir.join(".harness-smoke");
    std::fs::create_dir_all(&smoke_dir)
        .map_err(|e| format!("could not create the harness smoke dir: {e}"))?;
    std::fs::write(
        smoke_dir.join("test_harness_smoke.py"),
        "def test_the_harness_can_run():\n    assert True\n",
    )
    .map_err(|e| format!("could not write the harness smoke test: {e}"))?;
    let out = run(
        &py.to_string_lossy(),
        &["-m", "pytest", "-q", "test_harness_smoke.py"],
        Some(&smoke_dir),
    )
    .await?;
    if out.status.success() {
        return Ok(());
    }
    // stderr carries the INTERNALERROR trace; stdout carries collection errors.
    let detail = {
        let e = String::from_utf8_lossy(&out.stderr);
        let s = if e.trim().is_empty() {
            String::from_utf8_lossy(&out.stdout).to_string()
        } else {
            e.to_string()
        };
        s.lines().rev().take(3).collect::<Vec<_>>().join(" | ")
    };
    Err(format!(
        "a trivial one-assert test did not pass under this venv's pytest: {detail}"
    ))
}

/// Parse uv's deleted-history hint into an `--exclude-newer-package` value (`pkg=cutoff`).
///
/// The hint shape (uv 0.11):
/// ```text
/// hint: `atomicwrites` was filtered by `exclude-newer` to only include packages uploaded
/// before 2019-04-13T16:17:45Z. The latest version satisfying the requirement is v1.4.1,
/// published at 2022-07-08T18:31:40.459Z. Consider using `exclude-newer-package` to
/// override the cutoff for this package.
/// ```
/// The cutoff is that surviving upload's timestamp plus one second — `--exclude-newer-package`
/// is exclusive ("prior to"), so the publish instant itself must land inside the window, and
/// one second past it admits nothing else.
fn deleted_history_override(stderr: &str) -> Option<String> {
    let hint_at = stderr.find("Consider using `exclude-newer-package`")?;
    let region = &stderr[..hint_at];
    let pkg = {
        let rest = &region[region.rfind("hint: `")? + "hint: `".len()..];
        rest.split('`').next()?
    };
    let published = {
        let rest = &region[region.rfind("published at ")? + "published at ".len()..];
        &rest[..=rest.find('Z')?]
    };
    let ts = chrono::DateTime::parse_from_rfc3339(published).ok()?;
    let cutoff =
        (ts + chrono::Duration::seconds(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    if pkg.is_empty() || pkg.contains(char::is_whitespace) {
        return None;
    }
    Some(format!("{pkg}={cutoff}"))
}

/// Parse uv's metadata-version-mismatch failure into an `--exclude-newer-package` value
/// that LIFTS the era cutoff for the one unbuildable package.
///
/// The failure shape (uv 0.11, live pylint-5859 2026-08-11):
/// ```text
/// ╰─▶ Package metadata version `0.0.0` does not match given version `1.7.1`
/// hint: `lazy-object-proxy` (v1.7.1) was included because `pylint` (v2.13.0.dev0) ...
/// ```
/// The `0.0.0` is an sdist built without git metadata (setuptools_scm fallback) — it happens
/// exactly when the era release ships no wheel for this platform, so no date-pinned retry can
/// ever succeed. Lifting that ONE package's cutoff (far-future bound) lets uv take a modern
/// wheel-shipping release; the rest of the graph stays era-pinned. Verified live: pylint
/// 2.13.0-dev0 + astroid 2.9.3 (era-correct) with only the shim floated.
fn metadata_mismatch_override(stderr: &str) -> Option<String> {
    if !stderr.contains("Package metadata version `0.0.0` does not match given version") {
        return None;
    }
    let rest = &stderr[stderr.find("hint: `")? + "hint: `".len()..];
    let pkg = rest.split('`').next()?;
    if pkg.is_empty() || pkg.contains(char::is_whitespace) {
        return None;
    }
    Some(format!("{pkg}=9999-01-01T00:00:00Z"))
}

/// Third heal arm: an era `importlib-metadata` 0.x in the graph (2019 pluggy 0.12 pulls it
/// unconditionally) crashes the MODERN setuptools that `--no-build-isolation` builds with —
/// setuptools' own banner names the clash and its own remedy is "install an updated
/// version" (setuptools/importlib_metadata#396). Lift that one package's cutoff, exactly
/// the metadata-mismatch shape (live: pytest-5413 fresh env, 2026-08-12; sibling 5221 is
/// one pluggy release older and never pulls it).
fn setuptools_importlib_clash_override(stderr: &str) -> Option<String> {
    if stderr.contains("`importlib-metadata` version is incompatible with `setuptools`") {
        Some("importlib-metadata=9999-01-01T00:00:00Z".to_string())
    } else {
        None
    }
}

/// How many per-package cutoff overrides one install may discover before we stop.
///
/// Each round must surface a NEW package (see the loop), so this bounds a pathological
/// graph rather than a healthy one — history-holes per graph are few, and the live maximum
/// observed across Lite is two.
const MAX_ERA_OVERRIDES: usize = 8;

/// ONE era-pinned `uv pip install`, healing from uv's own evidence.
///
/// # Why this is a function and not two copies of a loop
///
/// "Install under the instance's date pin, and when the resolver hits a hole that the pin
/// itself created, read uv's hint and retry with the minimum per-package override" is ONE
/// decision. It had TWO call sites in `ensure_env` — the editable `-e .` install, which had
/// the heal loop, and the dependency-sdist build-dep pre-install, which did not — so the
/// same class of failure was survivable at one site and fatal at the other.
///
/// Measured 2026-08-17: every astropy instance in swe-bench-lite's head died at the
/// unhealed site with `could not pre-install …'s dependency-sdist build deps ["jinja2"]`.
/// The cause is the pin working exactly as designed: jinja2 2.10 needs markupsafe 1.0,
/// whose `setup.py` build requires `setuptools>=40.8.0`, while the 2017 cutoff caps
/// setuptools at 38.2.4 — unsatisfiable by construction. uv's stderr named the package and
/// suggested `exclude-newer-package`, which is precisely what the loop 30 lines below knew
/// how to parse. Extracting it heals both sites and makes a third site inherit the
/// behaviour by construction.
///
/// `tail` is the call-site-specific argv after the shared pin flags (`["jinja2"]`, or
/// `["--no-build-isolation", "-e", "."]`). `as_of: None` disables pinning AND healing —
/// with no pin there is no pin-induced hole to heal.
async fn era_pinned_uv_install(
    uv: &str,
    py: &str,
    as_of: Option<&str>,
    tail: &[&str],
    cwd: Option<&Path>,
    envs: &[(&str, &str)],
) -> Result<std::process::Output, String> {
    let mut overrides: Vec<String> = Vec::new();
    loop {
        let mut args = vec!["pip", "install", "-q", "--python", py];
        if let Some(date) = as_of {
            args.push("--exclude-newer");
            args.push(date);
        }
        for pin in &overrides {
            args.push("--exclude-newer-package");
            args.push(pin);
        }
        args.extend(tail.iter().copied());
        let out = run_env(uv, &args, cwd, envs).await?;
        if out.status.success() || as_of.is_none() {
            return Ok(out);
        }
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        // Three heal arms, same bounded loop: (1) deleted-history — the date pin leaves zero
        // candidates and uv's hint names the earliest surviving upload; (2) metadata-mismatch
        // — an era sdist with no wheel for this platform builds as version 0.0.0
        // (setuptools_scm without git metadata), so the ONE unbuildable package's cutoff is
        // lifted entirely; (3) the setuptools/importlib-metadata clash. All three parse uv's
        // OWN evidence; no hand-maintained package list.
        match deleted_history_override(&stderr)
            .or_else(|| metadata_mismatch_override(&stderr))
            .or_else(|| setuptools_importlib_clash_override(&stderr))
        {
            Some(pin) if !overrides.contains(&pin) && overrides.len() < MAX_ERA_OVERRIDES => {
                tracing::warn!(
                    r#override = %pin,
                    tail = ?tail,
                    "date-pinned resolution hit an unresolvable era package — retrying with \
                     a per-package cutoff derived from uv's own error"
                );
                // The setuptools/importlib clash needs MORE than a lifted cutoff: the broken
                // importlib-metadata 0.x is ALREADY INSTALLED in the venv (2019 pluggy pulled
                // it in the requirements step), and an install won't touch an already-satisfied
                // package — so the cutoff pin alone retries into the exact same crash (live:
                // pytest-5413/5495 kickoff, 2026-08-12, the first run after this arm shipped).
                // Apply the banner's own remedy directly: upgrade the installed copy, then
                // retry. General to any `--no-build-isolation` build, not just `-e .`.
                if pin.starts_with("importlib-metadata=") {
                    let up = run(
                        uv,
                        &[
                            "pip",
                            "install",
                            "-q",
                            "--python",
                            py,
                            "--upgrade",
                            "importlib-metadata",
                        ],
                        None,
                    )
                    .await?;
                    if !up.status.success() {
                        // The heal itself failed — no point looping on the same wall.
                        return Ok(out);
                    }
                }
                overrides.push(pin);
            }
            _ => return Ok(out),
        }
    }
}

fn which(bin: &str) -> Option<String> {
    let path = std::env::var("PATH").ok()?;
    for dir in path.split(':') {
        let candidate = Path::new(dir).join(bin);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    None
}

/// The test files an instance's own `test_patch` touches — the scope to run.
pub fn patched_test_files(test_patch: &str) -> Vec<String> {
    let mut files: Vec<String> = test_patch
        .lines()
        .filter_map(|l| l.strip_prefix("+++ b/"))
        .map(|p| p.split_whitespace().next().unwrap_or(p).to_string())
        .collect();
    files.sort();
    files.dedup();
    files
}

/// Which harness executes an instance's tests — a property of the REPO, not a pytest
/// version to search for (#383). django (114 of Lite's 300 instances) does not use pytest
/// at all: its harness is `./tests/runtests.py <module directives>`, and running pytest
/// against it grades every django instance UNGRADEABLE. The official SWE-bench per-repo
/// spec tables are the reference semantics for each arm's command shape.
///
/// This is the seam the sympy-11400 refusal in `ensure_env` predicted: "the runner is a
/// property of the repo era, not a pytest version to search for."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestRunner {
    /// The default: `python -m pytest <files> -v` and pytest's verbose report.
    Pytest,
    /// django's own runner: `python tests/runtests.py --verbosity 2 --settings=test_sqlite
    /// --parallel 1 <module directives>` from the repo root, unittest-style report.
    DjangoRuntests,
}

/// Select the runner from the instance's `repo` field. Unknown repos default to pytest —
/// the wrong default fails LOUDLY (the pristine gates in `grade` void the tree) rather
/// than silently, so a new repo class surfaces as a named gap, not a fake zero.
pub fn runner_for_repo(repo: &str) -> TestRunner {
    match repo {
        "django/django" => TestRunner::DjangoRuntests,
        _ => TestRunner::Pytest,
    }
}

/// The exact argv (after the venv's `python`) that runs an instance's test scope.
/// Pure so the per-repo command shape is table-testable without a venv.
/// The module name of the JSON runner dropped into django's `tests/` directory.
/// `tests/` is on `sys.path` when runtests.py runs, so a bare module name resolves.
const DJANGO_JSON_RUNNER_MODULE: &str = "continuum_json_runner";

/// The settings module that SELECTS the JSON runner — the only seam django offers.
///
/// `runtests.py` has **no `--testrunner` flag in any era**; it reads
/// `settings.TEST_RUNNER` and defaults it only when unset (verified in-tree at 1.11,
/// 2.2, 3.2, 4.2, 5.2 and main — same three lines throughout). Passing a flag it does
/// not know is not a no-op: argparse rejects the whole invocation before a single test
/// runs, which is exactly what happened live on django-10914 (`unrecognized arguments:
/// --testrunner=…`, suite 0/40 in 4s). So the runner is selected by a settings module
/// that re-exports django's own `test_sqlite` and overrides that one key.
const DJANGO_JSON_SETTINGS_MODULE: &str = "continuum_json_settings";

/// Settings for [`DJANGO_JSON_SETTINGS_MODULE`]. A pure ADDITION to the clone: django's
/// own `test_sqlite` stays untouched and supplies every database/hasher setting, so this
/// shim cannot drift from whatever the era's suite settings happen to be.
const DJANGO_JSON_SETTINGS_SRC: &str = r#"# Written by continuum's SWE-bench grader. Not part of django.
# runtests.py honours settings.TEST_RUNNER; it has no --testrunner flag. Inherit
# django's own suite settings verbatim and override only the runner.
from test_sqlite import *  # noqa: F401,F403
TEST_RUNNER = "continuum_json_runner.JsonRunner"
"#;

/// A `DiscoverRunner` that reports each test by its CANONICAL id instead of by prose.
///
/// WHY THIS EXISTS RATHER THAN A BETTER REGEX. unittest's verbose output is a rendering,
/// not a data format: the outcome sits on the id line when a test has no docstring and on
/// the DOCSTRING line when it does, django ≥4.1 changed the class-path shape, and a
/// docstring can contain any characters including " ... " and " (". Every one of those is a
/// way for a line-shaped parser to mis-attribute silently — and mis-attribution here does
/// not look like a bug, it looks like a citizen who failed. `test.id()` is the id unittest
/// itself uses; asking the runner for it removes the entire class of guess.
///
/// Skips and expected failures count as PASSES, unexpected successes as FAILURES — the same
/// rule [`django_outcome`] applies, kept identical on purpose.
const DJANGO_JSON_RUNNER_SRC: &str = r#"# Written by continuum's SWE-bench grader. Not part of django.
import json, sys, unittest
from django.test.runner import DiscoverRunner

class _ContinuumResult(unittest.TextTestResult):
    def __init__(self, *a, **kw):
        super().__init__(*a, **kw)
        self.continuum_rows = {}
    def _record(self, test, ok):
        # shortDescription() is the docstring's first line — the SAME string unittest
        # renders on its own output line, and therefore the id SWE-bench's own log parser
        # captured for docstringed tests. Emitting it is not redundant: the dataset spells
        # those tests BY THE DOCSTRING and by nothing else.
        try:
            desc = test.shortDescription() or ""
        except Exception:
            desc = ""
        self.continuum_rows[test.id()] = (ok, desc)
    def addSuccess(self, test):
        super().addSuccess(test); self._record(test, True)
    def addError(self, test, err):
        super().addError(test, err); self._record(test, False)
    def addFailure(self, test, err):
        super().addFailure(test, err); self._record(test, False)
    def addSkip(self, test, reason):
        super().addSkip(test, reason); self._record(test, True)
    def addExpectedFailure(self, test, err):
        super().addExpectedFailure(test, err); self._record(test, True)
    def addUnexpectedSuccess(self, test):
        super().addUnexpectedSuccess(test); self._record(test, False)

class JsonRunner(DiscoverRunner):
    def get_resultclass(self):
        return _ContinuumResult
    def run_suite(self, suite, **kwargs):
        result = super().run_suite(suite, **kwargs)
        for tid, (ok, desc) in getattr(result, "continuum_rows", {}).items():
            row = {"id": tid, "ok": ok}
            if desc:
                row["desc"] = desc
            sys.stderr.write("CONTINUUM_TEST " + json.dumps(row) + "\n")
        sys.stderr.flush()
        return result
"#;

/// Drop the JSON runner AND the settings module that selects it into the clone's `tests/`
/// dir. Returns whether both are usable — either alone does nothing, so this is one unit.
/// Idempotent — grading re-runs over the same clone just overwrite them.
async fn install_django_json_runner(repo_dir: &Path) -> bool {
    let dir = repo_dir.join("tests");
    if !dir.is_dir() {
        return false;
    }
    for (module, src) in [
        (DJANGO_JSON_RUNNER_MODULE, DJANGO_JSON_RUNNER_SRC),
        (DJANGO_JSON_SETTINGS_MODULE, DJANGO_JSON_SETTINGS_SRC),
    ] {
        let path = dir.join(format!("{module}.py"));
        if let Err(e) = std::fs::write(&path, src) {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "could not install the django JSON test runner — falling back to parsing the \
                 verbose report, which mis-attributes docstringed tests (#383)"
            );
            return false;
        }
    }
    true
}

/// One machine-readable row per test: `CONTINUUM_TEST {"id": …, "ok": …, "desc": …}`.
///
/// THREE spellings are registered for one outcome, because the dataset uses all three and a
/// canonical id ALONE cannot resolve a django instance (measured on django-10914, gold gate):
///
/// | spelling | where it comes from | example |
/// |---|---|---|
/// | canonical id | `test.id()` | `test_utils.tests.AssertRaisesMsgTest.test_special_re_chars` |
/// | unittest rendering | id, re-spelled | `test_special_re_chars (test_utils.tests.AssertRaisesMsgTest)` |
/// | **docstring** | `test.shortDescription()` | `assertRaisesMessage shouldn't interpret RE special chars.` |
///
/// The third row is the one that is easy to miss and impossible to work around downstream.
/// SWE-bench's own django ids were harvested from unittest's verbose log, and unittest prints
/// the DOCSTRING in place of the id when a test has one — so for those tests the dataset's
/// PASS_TO_PASS entry *is* the docstring, with no id anywhere in it. Verified in the dataset:
/// 2 of django-10914's 98 p2p ids are docstring prose. Only the test itself knows its own
/// docstring, which is why the runner emits it rather than the grader guessing.
///
/// Ids and renderings are unique, so they are inserted directly. Docstrings are NOT unique —
/// two tests may share one — so they are AND-folded (pass only if every test carrying that
/// docstring passed) and they never overwrite a real id.
pub fn parse_django_json(report: &str) -> (HashMap<String, bool>, HashMap<String, bool>) {
    let mut by_node = HashMap::new();
    let mut by_func: HashMap<String, bool> = HashMap::new();
    let mut by_desc: HashMap<String, bool> = HashMap::new();
    for line in report.lines() {
        let Some(payload) = line.trim().strip_prefix("CONTINUUM_TEST ") else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else {
            continue;
        };
        let (Some(id), Some(ok)) = (v.get("id").and_then(|x| x.as_str()), v.get("ok").and_then(|x| x.as_bool()))
        else {
            continue;
        };
        by_node.insert(id.to_string(), ok);
        if let Some((class_path, method)) = id.rsplit_once('.') {
            by_node.insert(format!("{method} ({class_path})"), ok);
            let entry = by_func.entry(method.to_string()).or_insert(true);
            *entry = *entry && ok;
        }
        if let Some(desc) = v.get("desc").and_then(|x| x.as_str()) {
            let desc = desc.trim();
            if !desc.is_empty() {
                let entry = by_desc.entry(desc.to_string()).or_insert(true);
                *entry = *entry && ok;
            }
        }
    }
    // Docstrings fill gaps; they never shadow a canonical id or its rendering.
    for (desc, ok) in by_desc {
        by_node.entry(desc).or_insert(ok);
    }
    (by_node, by_func)
}

pub fn test_invocation(runner: TestRunner, test_files: &[String]) -> Vec<String> {
    test_invocation_with(runner, test_files, false)
}

/// [`test_invocation`], plus whether django should be told to use the JSON runner.
pub fn test_invocation_with(
    runner: TestRunner,
    test_files: &[String],
    django_json: bool,
) -> Vec<String> {
    match runner {
        TestRunner::Pytest => {
            let mut args: Vec<String> = vec!["-m".into(), "pytest".into()];
            args.extend(test_files.iter().cloned());
            // Flags must be era-portable — see the comment at the call site (`run_tests`).
            args.extend(["-v".into(), "-p".into(), "no:cacheprovider".into()]);
            // Belt to [`strip_ansi`]'s braces: ask for no color at the SOURCE. A repo can
            // force `--color=yes` from its own `setup.cfg` addopts (astropy does), and CLI
            // args are appended after addopts so the later `--color=no` wins. This is the
            // nicety — the parser's strip is what actually guarantees correctness, which is
            // why an ancient pytest that rejected this flag still could not break grading.
            args.push("--color=no".into());
            args
        }
        TestRunner::DjangoRuntests => {
            // Mirrors the official harness: verbosity 2 prints one line per test (the
            // report we parse when there are no JSON rows), test_sqlite is the settings
            // module django's own suite ships for exactly this, and --parallel 1 keeps the
            // per-test lines from interleaving across workers.
            //
            // The JSON runner is selected THROUGH settings (`TEST_RUNNER`), because that is
            // the seam runtests.py actually reads — see [`DJANGO_JSON_SETTINGS_MODULE`]. Our
            // shim re-exports test_sqlite, so this stays one settings argument either way.
            let settings = if django_json {
                DJANGO_JSON_SETTINGS_MODULE
            } else {
                "test_sqlite"
            };
            let mut args: Vec<String> = vec![
                "tests/runtests.py".into(),
                "--verbosity".into(),
                "2".into(),
                format!("--settings={settings}"),
                "--parallel".into(),
                "1".into(),
            ];
            args.extend(test_files.iter().map(|f| django_directive(f)));
            args
        }
    }
}

/// A patched test FILE, as django's runner wants it: a dotted module directive.
/// `tests/migrations/test_operations.py` → `migrations.test_operations` — the same
/// transform the official harness applies for django/django.
fn django_directive(file: &str) -> String {
    let f = file.strip_suffix(".py").unwrap_or(file);
    let f = f.strip_prefix("tests/").unwrap_or(f);
    f.replace('/', ".")
}

/// Resolve django's `runtests.py --verbosity 2` (unittest) report into the same two maps
/// the pytest parser produces, keyed in the DATASET's own id shape.
///
/// django FAIL_TO_PASS ids look like `test_combine (expressions.tests.CombinedExprTests)`.
/// The report line is `test_combine (expressions.tests.CombinedExprTests) ... ok` on
/// django ≤4.0, and on ≥4.1 the class path grows a trailing method repeat
/// (`...CombinedExprTests.test_combine) ... ok`) — normalized back so dataset ids hit.
/// unittest's outcome vocabulary → pass/fail, or `None` for a line that is not an outcome.
///
/// ONE place, because the report has TWO line shapes (id-and-outcome on one line, or
/// id-then-docstring-and-outcome across two) and both must classify identically. Two copies
/// of this match is how a `skipped` counts as a pass in one shape and a non-outcome in the
/// other — silent, and invisible in aggregate scores.
///
/// `skipped` and `expected failure` are PASSES: SWE-bench's own harness treats a test that
/// declines to run as satisfied, and a test the suite knows is broken as behaving correctly.
/// `unexpected success` is a FAILURE for the same reason — the suite's expectation was wrong.
fn django_outcome(outcome: &str) -> Option<bool> {
    if outcome.starts_with("ok")
        || outcome.starts_with("skipped")
        || outcome.starts_with("expected failure")
    {
        Some(true)
    } else if outcome.starts_with("FAIL")
        || outcome.starts_with("ERROR")
        || outcome.starts_with("unexpected success")
    {
        Some(false)
    } else {
        None
    }
}

/// THE TWO-LINE DOCSTRING FORM (fixed 2026-08-17, found by the gold gate).
///
/// unittest's verbose output puts the outcome on the test-id line ONLY when the test has no
/// docstring. With a docstring it prints TWO lines and the `... ok` lands on the SECOND:
///
/// ```text
/// test_skip_if_db_feature (test_utils.tests.SkippingTestCase)
/// Testing the django.test.skipIfDBFeature decorator. ... ok
/// ```
///
/// This parser required `" ... "` AND `" ("` on ONE line, so BOTH lines fell through the
/// `continue`s: the id line has no `" ... "`, the docstring line has no `" ("`. Every
/// docstringed django test was therefore recorded NOWHERE, and absent-from-map reads
/// downstream as not-passed.
///
/// Measured consequence, which is why this is not a cosmetic parse bug: django-10914's own
/// GOLD patch graded `FAIL_TO_PASS 0/1, PASS_TO_PASS 35/40` and was reported as a
/// "REGRESSION — your changes BROKE 5 test(s)", while the very output being quoted showed
/// all five passing `... ok`. The 5 broken were the 5 docstringed ones. So django scores
/// were never measuring django — and django is 114/300 of Lite (#383). Every django zero we
/// have recorded is retro-actively uninterpretable.
///
/// Dataset ids for these tests may be EITHER spelling — SWE-bench's own id lists were
/// harvested from this same verbose output, so some entries are the docstring text rather
/// than the node id (e.g. "An exception is setUp() is reraised after disable() is called.").
/// Both are therefore registered as keys for the same outcome; whichever spelling the
/// dataset carries resolves. Registering both is not a fallback — it is the honest statement
/// that one test has two names in this format.
pub fn parse_django_report(report: &str) -> (HashMap<String, bool>, HashMap<String, bool>) {
    let mut by_node = HashMap::new();
    let mut by_func: HashMap<String, bool> = HashMap::new();
    // A test-id line awaiting its outcome on a following docstring line: (func, class_norm).
    let mut pending: Option<(String, String)> = None;
    // Same reason as the pytest parser — a colorized runner must not become "0 of N".
    let report = strip_ansi(report);
    for line in report.lines() {
        let line = line.trim();
        let Some((head, tail)) = line.split_once(" ... ") else {
            // No outcome here. A bare `func (class.path)` line is the FIRST half of the
            // two-line form — remember it. Anything else clears the pending slot so an
            // outcome can never be attributed across unrelated output.
            pending = match line
                .split_once(" (")
                .and_then(|(f, c)| c.strip_suffix(')').map(|c| (f, c)))
            {
                Some((func, class_path)) if !func.is_empty() && !class_path.is_empty() => {
                    let class_norm = class_path
                        .strip_suffix(&format!(".{func}"))
                        .unwrap_or(class_path);
                    Some((func.to_string(), class_norm.to_string()))
                }
                _ => None,
            };
            continue;
        };
        // An outcome line WITHOUT `" ("` is the docstring half. Attribute it to the id we
        // remembered, and register the docstring text as a second key for the same test.
        if !head.contains(" (") {
            let outcome = tail.trim();
            let ok = match django_outcome(outcome) {
                Some(ok) => ok,
                None => {
                    pending = None;
                    continue;
                }
            };
            if let Some((func, class_norm)) = pending.take() {
                by_node.insert(format!("{func} ({class_norm})"), ok);
                let doc = head.trim();
                if !doc.is_empty() {
                    by_node.insert(doc.to_string(), ok);
                }
                let entry = by_func.entry(func).or_insert(true);
                *entry = *entry && ok;
            }
            continue;
        }
        pending = None;
        let Some((func, class_part)) = head.split_once(" (") else {
            continue;
        };
        let Some(class_path) = class_part.strip_suffix(')') else {
            continue;
        };
        let Some(ok) = django_outcome(tail.trim()) else {
            continue;
        };
        // django ≥4.1 prints `module.Class.test_name`; the dataset uses `module.Class`.
        let class_norm = class_path
            .strip_suffix(&format!(".{func}"))
            .unwrap_or(class_path);
        by_node.insert(format!("{func} ({class_norm})"), ok);
        // Same bare name in two classes: passing only if every one passed (pytest rule).
        let entry = by_func.entry(func.to_string()).or_insert(true);
        *entry = *entry && ok;
    }
    (by_node, by_func)
}

/// Remove ANSI escape sequences (CSI/SGR and OSC) from captured process output.
///
/// Borrowed when there is no `ESC` at all — the common case, since pytest suppresses color
/// on a pipe — so the usual path allocates nothing.
///
/// Why a grader needs this at all (glass-boxed 2026-08-18): astropy's own `setup.cfg` carries
/// `addopts = --color=yes`, which forces color even though our output is a pipe. Every result
/// line then reads `…test_x.py::\x1b[1mtest_y\x1b[0m \x1b[32mPASSED\x1b[0m`, the escape lands
/// INSIDE the node id and before the verdict, and a parser matching `starts_with("PASSED")`
/// skips every line. The grader reported `PASS_TO_PASS 0 of 40` and declared the environment
/// broken — while that same run printed `1 failed, 179 passed`. The suite was fine; the
/// grader could not read it. A repo can force color from its own config, so stripping at the
/// PARSER is the durable fix: it needs no flag, works on already-captured reports, and does
/// not depend on any pytest version.
pub fn strip_ansi(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains('\u{1b}') {
        return std::borrow::Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: params/intermediates, then a final byte in @..~ ends the sequence.
            Some('[') => {
                for f in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&f) {
                        break;
                    }
                }
            }
            // OSC: runs until BEL or the ST pair (ESC \).
            Some(']') => {
                while let Some(f) = chars.next() {
                    if f == '\u{7}' {
                        break;
                    }
                    if f == '\u{1b}' {
                        // ST is ESC \ — consume the backslash and stop.
                        let _ = chars.next();
                        break;
                    }
                }
            }
            // Any other two-byte escape: drop both bytes.
            Some(_) => {}
            None => break,
        }
    }
    std::borrow::Cow::Owned(out)
}

/// Resolve pytest's `-v` report into a verdict per node id AND per bare function name.
///
/// The dataset does not use one id shape. pytest and flask instances give node ids
/// (`tests/test_x.py::test_y`); sympy gives BARE function names because sympy ships its own
/// runner. Looking up both is what makes one grader serve every repo.
///
/// Color-forcing repos are handled by [`strip_ansi`] before matching — see its doc for the
/// astropy incident this prevents.
pub fn parse_pytest_report(report: &str) -> (HashMap<String, bool>, HashMap<String, bool>) {
    let mut by_node = HashMap::new();
    let mut by_func: HashMap<String, bool> = HashMap::new();
    let report = strip_ansi(report);
    for line in report.lines() {
        let line = line.trim();
        let Some((node, rest)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        if !node.contains("::") {
            continue;
        }
        let verdict = rest.trim_start();
        let ok = if verdict.starts_with("PASSED")
            || verdict.starts_with("XFAIL")
            || verdict.starts_with("SKIPPED")
        {
            true
        } else if verdict.starts_with("FAILED")
            || verdict.starts_with("ERROR")
            || verdict.starts_with("XPASS")
        {
            false
        } else {
            continue;
        };
        by_node.insert(node.to_string(), ok);
        let func = node
            .rsplit("::")
            .next()
            .unwrap_or(node)
            .split('[')
            .next()
            .unwrap_or(node);
        // Same bare name in two files: passing only if every one passed.
        let entry = by_func.entry(func.to_string()).or_insert(true);
        *entry = *entry && ok;
    }
    (by_node, by_func)
}

/// Look one required id up in a parsed report, by node id or by bare function name.
pub fn verdict_for(
    id: &str,
    by_node: &HashMap<String, bool>,
    by_func: &HashMap<String, bool>,
) -> Option<bool> {
    if let Some(v) = by_node.get(id) {
        return Some(*v);
    }
    // django ids are `test_name (module.Class)` — the bare name is the LEADING token, and
    // the rsplit('.') chain below would extract `Class)` instead. Must come first.
    let key = if let Some((func, _)) = id.split_once(" (") {
        func
    } else {
        let key = id.rsplit("::").next().unwrap_or(id);
        let key = key.rsplit('.').next().unwrap_or(key);
        key.split('[').next().unwrap_or(key)
    };
    by_func.get(key).copied()
}

/// Run the instance's test files once and resolve every required id against that report.
/// A test absent from the report counts as failed — but the absence is knowable, not silent.
/// Restore a grade worktree to its checked-out state: revert tracked edits AND remove
/// files a candidate patch CREATED (checkout alone leaves those untracked in place, and a
/// leftover broken module poisons later runs — live: pytest-11143 attempt 3 misread a real
/// capability regression as an env void, 2026-08-12). No `-x`: gitignored editable-install
/// artifacts (*.egg-info) must survive.
pub async fn reset_worktree(repo_dir: &Path) {
    let _ = run("git", &["checkout", "--quiet", "."], Some(repo_dir)).await;
    let _ = run("git", &["clean", "-fdq"], Some(repo_dir)).await;
}

pub async fn run_tests(
    repo_dir: &Path,
    venv_py: &Path,
    ids: &[String],
    test_files: &[String],
    runner: TestRunner,
) -> (HashMap<String, bool>, String) {
    if test_files.is_empty() || ids.is_empty() {
        return (
            ids.iter().map(|i| (i.clone(), false)).collect(),
            String::new(),
        );
    }
    // Pytest flags must be era-portable: the interpreter under `-m pytest` can be the repo's
    // OWN pytest (pytest-dev instances: the editable install IS the subject) or an era-pinned
    // one. `--no-header` (6.1+) and `-rN` (5.1+) made pytest 4.4 exit 4 with "unrecognized
    // arguments" before running a single test — every id read as failed, and the whole
    // 2019-pytest class graded p2p 0/N (live: pytest-5221 retry, 2026-08-12). Cosmetic
    // flags are not worth a version gate; `-v` and no:cacheprovider go back to 2.x.
    // django is graded from CANONICAL ids, not from prose. `install_django_json_runner`
    // drops a DiscoverRunner subclass into the clone that emits one machine-readable row
    // per test keyed by `test.id()`; `test_invocation` then asks runtests.py to use it.
    // If the drop fails we fall back to the verbose-report parser — reported, never silent,
    // because a fallback that scores is indistinguishable from a fallback that lies.
    let django_json = match runner {
        TestRunner::DjangoRuntests => install_django_json_runner(repo_dir).await,
        TestRunner::Pytest => false,
    };
    let owned_args = test_invocation_with(runner, test_files, django_json);
    let args: Vec<&str> = owned_args.iter().map(String::as_str).collect();
    let Ok(out) = run(&venv_py.to_string_lossy(), &args, Some(repo_dir)).await else {
        return (
            ids.iter().map(|i| (i.clone(), false)).collect(),
            String::new(),
        );
    };
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let (by_node, by_func) = match runner {
        TestRunner::Pytest => parse_pytest_report(&report),
        // Machine-readable rows when the JSON runner is installed. If it emitted NOTHING
        // (an import error in the runner or settings shim, a runtests.py that rejected the
        // invocation, a crash before any test ran) fall back to the verbose report rather
        // than scoring every id as failed — and say so, because a silent fallback here
        // reads as a citizen's zero. This fallback is what kept django-10914's broken
        // `--testrunner` invocation from grading as 40 capability failures.
        TestRunner::DjangoRuntests if django_json => {
            let (by_node, by_func) = parse_django_json(&report);
            if by_node.is_empty() {
                tracing::warn!(
                    "the django JSON runner emitted no rows — falling back to the verbose \
                     report. Grades from this run are less trustworthy (#383)."
                );
                parse_django_report(&report)
            } else {
                (by_node, by_func)
            }
        }
        TestRunner::DjangoRuntests => parse_django_report(&report),
    };
    let verdicts = ids
        .iter()
        .map(|id| {
            (
                id.clone(),
                verdict_for(id, &by_node, &by_func).unwrap_or(false),
            )
        })
        .collect();
    // The report rides back so the grader can excerpt the FAILURE OUTPUT into the
    // verdict — the assertion diff is the teaching half a bare test name lacks.
    (verdicts, report)
}

/// Cap on the failure-output excerpt a verdict carries. Failures and the short
/// summary sit at the END of a test run's output (pytest and sympy's own runner
/// alike), so the TAIL is the format-agnostic excerpt. Bounded so a pathological
/// run (a runaway traceback, a print loop) can't flood the retry prompt.
const FAILURE_EXCERPT_MAX: usize = 2000;

/// Tail of a test report, char-capped on a char boundary.
fn report_tail(report: &str) -> String {
    let trimmed = report.trim_end();
    if trimmed.len() <= FAILURE_EXCERPT_MAX {
        return trimmed.to_string();
    }
    let mut start = trimmed.len() - FAILURE_EXCERPT_MAX;
    while !trimmed.is_char_boundary(start) {
        start += 1;
    }
    format!("…{}", &trimmed[start..])
}

/// How many PASS_TO_PASS tests to sample. The full set runs to hundreds on some instances and
/// its job is regression detection; a broad sample catches breakage without paying for the
/// tail. Named rather than inlined so a number in a report is traceable to a decision.
pub const P2P_SAMPLE: usize = 40;

/// Grade one candidate patch against one instance, running the full protocol.
///
/// `model_patch` empty means "grade the tree as the solver left it" — the caller has already
/// edited `repo_dir` in place and the diff is implicit.
/// The retry loop's feedback text, composed from BOTH grading reports. The
/// REGRESSION section leads and is unmissable — glass-boxed 2026-08-08
/// (atlas-sympy-24066-n7): her attempt-1 edit broke all 30 pass-to-pass tests,
/// but the old excerpt was built from the fail-to-pass report alone, so the
/// retries never showed her the breakage — she resubmitted a byte-identical
/// broken patch twice. A verdict that hides the collateral damage teaches
/// "my patch just doesn't fix it yet" when the truth is "my patch destroyed
/// the tree". Pure so the composition is unit-testable without a venv.
fn compose_failure_excerpt(
    p2p_broken: &[String],
    p2p_report: &str,
    f2p_still_failing: bool,
    f2p_report: &str,
) -> Option<String> {
    let mut sections: Vec<String> = Vec::new();
    if !p2p_broken.is_empty() {
        const NAME_CAP: usize = 10;
        let shown: Vec<&str> = p2p_broken
            .iter()
            .take(NAME_CAP)
            .map(|s| s.as_str())
            .collect();
        let more = p2p_broken.len().saturating_sub(NAME_CAP);
        let more_note = if more > 0 {
            format!(" (+{more} more)")
        } else {
            String::new()
        };
        let tail = if p2p_report.trim().is_empty() {
            String::new()
        } else {
            format!(" Broken-test output tail:\n{}", report_tail(p2p_report))
        };
        sections.push(format!(
            "REGRESSION — your changes BROKE {} test(s) that PASSED before you touched the \
             tree: {}{}. Fix or revert that breakage FIRST: a patch that destroys working \
             behavior grades as a failure no matter what else it does. A wrong symbol name or \
             a deleted line in code you didn't mean to change is the usual cause — re-read \
             your own diff.{}",
            p2p_broken.len(),
            shown.join(", "),
            more_note,
            tail
        ));
    }
    // The target-test tail, where every runner puts its failures + summary.
    // This is what turns "test_issue_24062 failed" into
    // "AssertionError: Dimension(impedance*capacitance/time) != 1".
    if f2p_still_failing && !f2p_report.trim().is_empty() {
        sections.push(report_tail(f2p_report));
    }
    if sections.is_empty() {
        None
    } else {
        Some(sections.join("\n\n"))
    }
}

/// THE GOLD GATE: grade the instance's OWN gold patch and require it to resolve.
///
/// This is the spine check [`SweInstance::patch`]'s own doc has promised since that field
/// was written — "the spine check grades THIS; it must resolve or the environment is
/// wrong" — and which did not exist. `grade(.., None)` means "grade the tree as the solver
/// left it", not "grade gold", so nothing in the tree ever validated an env against a
/// known-correct patch.
///
/// WHY THIS IS THE KEYSTONE FOR EVERY NUMBER WE REPORT. Without it a `resolved: false` has
/// two indistinguishable causes: the citizen's patch was wrong, or the environment cannot
/// score a correct patch at all. Measured 2026-08-17 on this box, the second is live and
/// unquantified: a 2019-era django env carries pytest 8.4.2, and the module's own notes
/// record era suites importing pytest internals that modern pytest deleted (flask 2.2's
/// `from _pytest.monkeypatch import notset`). So today an unknown fraction of our zeros are
/// harness artifacts being tallied as capability. That is not a measurement — it is noise
/// with a number attached, and it is why 114/300 (#383) and #380 cannot be told apart from
/// model failure by looking at scores.
///
/// The gate makes the distinction mechanical: gold resolves → the env can score, so a
/// citizen's zero is HERS. Gold fails → the env is disqualified and no result from it may
/// be reported as capability ([[an-absence-is-an-unfinished-measurement]]).
///
/// Deliberately a thin caller over [`grade`], not a parallel scorer: it must exercise the
/// EXACT clone → apply → test path a real attempt takes, or it proves nothing about that
/// path. A second implementation that agreed with itself would be the classic dead
/// instrument.
///
/// `gate_ok == false` in the returned verdict is a DIFFERENT fact and is not a gate
/// failure: it means FAIL_TO_PASS already passed on the pristine tree, so the instance
/// carries no bug here. Callers must not conflate "this task cannot distinguish a fix"
/// with "this environment is broken".
pub async fn gold_gate(instance: &SweInstance, repo_dir: &Path) -> SweVerdict {
    let mut verdict = grade(instance, repo_dir, Some(&instance.patch)).await;
    // An env that cannot score its own gold patch is disqualified, and the reason has to
    // survive into the receipt — a bare `resolved: false` here would read downstream as a
    // capability zero, which is the exact confusion this gate exists to end.
    if verdict.error.is_none() && !verdict.resolved {
        verdict.error = Some(format!(
            "GOLD GATE FAILED for {}: the instance's own gold patch did not resolve \
             (FAIL_TO_PASS {}/{}, PASS_TO_PASS {}/{}). The environment cannot score a \
             known-correct patch, so NO result from it is a capability measurement — \
             not a zero, an absence. Era deps are the leading suspect (#380): check the \
             interpreter rung against `interpreter_for_year` and the harness pytest \
             version against what this era's suite can import.",
            instance.instance_id,
            verdict.f2p_passed,
            verdict.f2p_total,
            verdict.p2p_passed,
            verdict.p2p_total,
        ));
    }
    verdict
}

pub async fn grade(
    instance: &SweInstance,
    repo_dir: &Path,
    model_patch: Option<&str>,
) -> SweVerdict {
    let mut verdict = SweVerdict {
        instance_id: instance.instance_id.clone(),
        ..Default::default()
    };
    let f2p = instance.f2p();
    let p2p: Vec<String> = instance.p2p().into_iter().take(P2P_SAMPLE).collect();
    verdict.f2p_total = f2p.len();
    verdict.p2p_total = p2p.len();
    let test_files = patched_test_files(&instance.test_patch);
    let runner = runner_for_repo(&instance.repo);

    let venv_py = match ensure_env(instance, repo_dir).await {
        Ok(p) => p,
        Err(e) => {
            verdict.error = Some(e);
            return verdict;
        }
    };

    // THE GATE, on a pristine tree: FAIL_TO_PASS must FAIL. That failure IS the bug. If it
    // passes here the checkout does not contain the bug, and nothing measured against it can
    // distinguish a fix from a no-op.
    if let Err(e) = apply_patch(repo_dir, &instance.test_patch, "gate-test").await {
        verdict.error = Some(e);
        return verdict;
    }
    let (pre, _) = run_tests(repo_dir, &venv_py, &f2p, &test_files, runner).await;
    let already: Vec<&String> = pre
        .iter()
        .filter(|(_, ok)| **ok)
        .map(|(id, _)| id)
        .collect();
    verdict.gate_ok = already.is_empty();
    if !verdict.gate_ok {
        verdict.error = Some(format!(
            "UNGRADEABLE — FAIL_TO_PASS already passes on the pristine tree ({already:?}). The \
             bug is not in this checkout; every score from this tree is void."
        ));
        return verdict;
    }

    // Reset, then run the real protocol: model patch first, tests second.
    reset_worktree(repo_dir).await;
    if let Some(patch) = model_patch {
        if let Err(e) = apply_patch(repo_dir, patch, "model").await {
            verdict.error = Some(format!("candidate patch did not apply: {e}"));
            return verdict;
        }
    }
    if let Err(e) = apply_patch(repo_dir, &instance.test_patch, "test").await {
        verdict.error = Some(e);
        return verdict;
    }

    let (f2p_res, f2p_report) = run_tests(repo_dir, &venv_py, &f2p, &test_files, runner).await;
    let (p2p_res, p2p_report) = run_tests(repo_dir, &venv_py, &p2p, &test_files, runner).await;
    verdict.f2p_passed = f2p_res.values().filter(|ok| **ok).count();
    verdict.p2p_passed = p2p_res.values().filter(|ok| **ok).count();

    // THE GATE'S OTHER HALF (#383 family, live 2026-08-11): PASS_TO_PASS is defined as
    // "passes before AND after the fix" — so a tree where p2p passes ZERO of N is not a
    // graded failure, it is a suite that does not run in this environment at all
    // (pytest-dev__pytest-5103 graded p2p 0/40 with an EMPTY patch: the era env cannot run
    // pytest's own suite, and that env fault was recorded as a capability verdict). The
    // f2p half of the gate cannot catch this: f2p "fails on pristine" is exactly what a
    // broken suite also produces. Distinguish the two the only honest way — re-run p2p on
    // the PRISTINE tree, paid only when the suspicious all-fail shape appears: pristine
    // ALSO passes zero → the env is broken, void the tree; pristine passes any → the
    // candidate patch genuinely broke the suite and the graded numbers stand.
    if verdict.p2p_total > 0 && verdict.p2p_passed == 0 {
        reset_worktree(repo_dir).await;
        if let Err(e) = apply_patch(repo_dir, &instance.test_patch, "p2p-gate").await {
            verdict.error = Some(e);
            return verdict;
        }
        let (pristine_p2p, pristine_report) =
            run_tests(repo_dir, &venv_py, &p2p, &test_files, runner).await;
        if pristine_p2p.values().filter(|ok| **ok).count() == 0 {
            verdict.gate_ok = false;
            // CARRY THE REPORT. This verdict is the ONLY artifact of the pristine run, and
            // without the run's own output "the suite does not run in this environment" is
            // a conclusion with its evidence deleted — the reader is left to reproduce it
            // by hand and guess at the difference.
            //
            // Measured 2026-08-17, and it cost hours: astropy-14365 graded 0-of-8 here
            // while the SAME invocation (`-m pytest <file> -v -p no:cacheprovider`, same
            // venv, same tree) run by hand gave 8 passed + the instance's own bug failing —
            // exactly the gate condition. Three hypotheses got proposed and none could be
            // settled, because the one thing that would have named the divergence was
            // discarded into `_` on this line. An instrument that knows the answer and
            // drops it is worse than one that never looked.
            let tail = report_tail(&pristine_report);
            verdict.error = Some(format!(
                "UNGRADEABLE — PASS_TO_PASS passes 0 of {} on the PRISTINE tree: the \
                 suite does not run in this environment, so every score from this tree \
                 is an env fault, never a capability verdict.{}",
                verdict.p2p_total,
                if tail.is_empty() {
                    " The pristine run produced NO output at all — the harness never \
                     executed (a missing interpreter, a refused invocation, a run that \
                     died before writing a byte), which is a different fault from a suite \
                     that ran and failed.".to_string()
                } else {
                    format!("\n\nPRISTINE RUN OUTPUT (tail):\n{tail}")
                }
            ));
            return verdict;
        }
    }

    verdict.failed_tests = f2p_res
        .iter()
        .chain(p2p_res.iter())
        .filter(|(_, ok)| !**ok)
        .map(|(id, _)| id.clone())
        .collect();
    verdict.failed_tests.sort();
    let mut p2p_broken: Vec<String> = p2p_res
        .iter()
        .filter(|(_, ok)| !**ok)
        .map(|(id, _)| id.clone())
        .collect();
    p2p_broken.sort();
    verdict.failure_excerpt = compose_failure_excerpt(
        &p2p_broken,
        &p2p_report,
        verdict.f2p_passed < verdict.f2p_total,
        &f2p_report,
    );
    verdict.resolved = verdict.f2p_passed == verdict.f2p_total
        && verdict.p2p_passed == verdict.p2p_total
        && verdict.f2p_total > 0;
    verdict
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the env installs `-e .` — RUNTIME deps only — and astropy's
    // conftest.py opens with `import hypothesis`, a TEST dep. pytest then dies loading
    // conftest, PASS_TO_PASS reads 0/9 on the PRISTINE tree, the gold gate refuses to score,
    // and every astropy instance is ungradeable no matter what a citizen writes (measured
    // 2026-08-17). This picks the group whose contents the suite needs. The preference ORDER
    // is the load-bearing part: a repo declaring both `dev` and `test` must get `test`, or we
    // install a kitchen-sink group and inherit its resolution failures.
    #[test]
    fn the_test_extra_is_the_narrowest_group_the_repo_declares() {
        let dir = std::env::temp_dir().join(format!("swe-extra-{}", std::process::id()));
        let write = |body: &str| {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("pyproject.toml"), body).unwrap();
        };

        write("[project.optional-dependencies]\ndev = []\ntest = []\n");
        assert_eq!(test_extra_name(&dir).as_deref(), Some("test"), "test beats dev");

        write("[project.optional-dependencies]\ndocs = []\ntesting = []\n");
        assert_eq!(
            test_extra_name(&dir).as_deref(),
            Some("testing"),
            "a repo that spells it `testing` still gets its suite deps"
        );

        // No test-shaped group, and no pyproject at all, both mean "install as before" —
        // NOT "install something plausible". A wrong extra is a resolve failure on a repo
        // that grades fine today.
        write("[project.optional-dependencies]\ndocs = []\n");
        assert_eq!(test_extra_name(&dir), None, "docs-only declares no suite deps");

        // THE CASE THAT SHIPPED BROKEN (2026-08-17): astropy 5.3 has no
        // [project.optional-dependencies] at all — its suite deps are setuptools
        // declarative config. A pyproject-only parser returns None for the exact repo this
        // fix exists for, which is what the live gold gate caught after I had already
        // claimed the fix worked.
        std::fs::remove_file(dir.join("pyproject.toml")).unwrap();
        std::fs::write(
            dir.join("setup.cfg"),
            "[options]\npackages = find:\n\n[options.extras_require]\ntest =  # Required to run the suite.\n    pytest>=7.0\n    pytest-astropy>=0.10\ntest_all =\n    objgraph\n[options.package_data]\n* = data/*\n",
        )
        .unwrap();
        assert_eq!(
            test_extra_name(&dir).as_deref(),
            Some("test"),
            "setup.cfg extras are declarations too — and `test_all`, which sorts first \
             alphabetically and is a superset, must NOT win over `test`"
        );
        let _ = std::fs::remove_file(dir.join("setup.cfg"));
        write("[project]\nname = \"x\"\n");
        assert_eq!(test_extra_name(&dir), None, "no optional-dependencies table");
        std::fs::remove_file(dir.join("pyproject.toml")).unwrap();
        assert_eq!(test_extra_name(&dir), None, "no pyproject at all");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: a SECOND swe env root. On 2026-08-17 two roots existed —
    // `swe_cache_dir()/envs` (live, 46 envs / 8 repos) and `~/.continuum/cache/swe-envs`
    // (retired, 14 envs / 3 repos, the legacy python default). Nothing failed: each root was
    // internally consistent, so whichever you `ls`ed answered confidently. Reading the retired
    // one produced "77% of staged instances have no environment", which was reported as the
    // benchmark's root cause and had a design approved on it. Truth was 95% coverage.
    //
    // A cache with two roots cannot report its own coverage. So: the env root is derived from
    // swe_cache_dir() and appears as a path literal NOWHERE else in the crate. If you need the
    // envs dir, call the function. See swe_cache_dir's doc for the full incident.
    #[test]
    fn the_swe_env_root_has_exactly_one_spelling() {
        fn walk(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    if let Ok(t) = std::fs::read_to_string(&p) {
                        out.push((p.to_string_lossy().to_string(), t));
                    }
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        walk(&root, &mut files);

        // The retired root, and any hand-spelled sibling of the live one. ASSEMBLED at
        // runtime, never written whole: a literal needle would match its own declaration and
        // the guard would fail on itself (it did, first run). Comments are stripped too, so
        // this test's prose and swe_cache_dir's doc can name the paths freely.
        let retired = format!("cache/{}-envs", "swe");
        let live_spelled_out = format!("benchmarks/{}/envs", "swe");
        let banned = [retired.as_str(), live_spelled_out.as_str()];
        let mut hits = Vec::new();
        for (path, text) in &files {
            for (n, raw) in text.lines().enumerate() {
                let code = match raw.find("//") {
                    Some(i) => &raw[..i],
                    None => raw,
                };
                for b in banned {
                    if code.contains(b) {
                        hits.push(format!("{path}:{} → {}", n + 1, code.trim()));
                    }
                }
            }
        }
        assert!(
            hits.is_empty(),
            "a SECOND spelling of the swe env root appeared — this is exactly how the \
             2026-08-17 coverage misdiagnosis happened (two roots, both real, neither naming \
             the other, opposite answers to the same question). Derive it from \
             `swe_cache_dir()` instead of writing the path:\n  {}",
            hits.join("\n  ")
        );
    }

    // what this catches: the false-env-void misgrade (pytest-11143 attempt 3, live
    // 2026-08-12) — a candidate patch that CREATED a file survived `git checkout .`, the
    // leftover module broke the "pristine" p2p re-run, and a REAL capability regression was
    // voided as an env fault. reset_worktree must revert tracked edits AND remove untracked
    // files, while leaving gitignored artifacts (editable-install *.egg-info) untouched.
    #[tokio::test]
    async fn reset_worktree_removes_created_files_but_keeps_ignored_artifacts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "t@t"],
            vec!["config", "user.name", "t"],
        ] {
            assert!(run("git", &args, Some(repo))
                .await
                .unwrap()
                .status
                .success());
        }
        std::fs::write(repo.join("tracked.py"), "original").unwrap();
        std::fs::write(repo.join(".gitignore"), "*.egg-info\n").unwrap();
        run("git", &["add", "."], Some(repo)).await.unwrap();
        run("git", &["commit", "-qm", "base"], Some(repo))
            .await
            .unwrap();

        // The three states a candidate patch leaves behind:
        std::fs::write(repo.join("tracked.py"), "edited").unwrap(); // tracked edit
        std::fs::write(repo.join("conftest.py"), "boom").unwrap(); // CREATED file
        std::fs::write(repo.join("pkg.egg-info"), "install artifact").unwrap(); // ignored

        reset_worktree(repo).await;

        assert_eq!(
            std::fs::read_to_string(repo.join("tracked.py")).unwrap(),
            "original"
        );
        assert!(
            !repo.join("conftest.py").exists(),
            "created file must not survive the reset"
        );
        assert!(
            repo.join("pkg.egg-info").exists(),
            "ignored install artifacts must survive"
        );
    }

    // what this catches: the deleted-history env-build failure (pytest-dev__pytest-5103,
    // live 2026-08-11) — `atomicwrites`' pre-2022 uploads were deleted from PyPI, so the
    // date-pinned resolve has ZERO candidates and the env build dies. The heal parses uv's
    // own hint into an `--exclude-newer-package` value cut one second past the earliest
    // surviving upload; unrelated errors must parse to None (no retry storm).
    #[test]
    fn deleted_history_hint_parses_into_a_per_package_cutoff() {
        let stderr = "\u{d7} No solution found when resolving dependencies:\n\
            \u{2570}\u{2500}\u{25b6} Because there are no versions of atomicwrites ...\n\
            hint: `atomicwrites` was filtered by `exclude-newer` to only include packages \
            uploaded before 2019-04-13T16:17:45Z. The latest version satisfying the requirement \
            is v1.4.1, published at 2022-07-08T18:31:40.459Z. Consider using \
            `exclude-newer-package` to override the cutoff for this package.";
        assert_eq!(
            deleted_history_override(stderr).as_deref(),
            Some("atomicwrites=2022-07-08T18:31:41Z"),
            "one second past the surviving upload, exclusive-bound safe"
        );

        assert_eq!(
            deleted_history_override("error: could not compile `foo` due to previous errors"),
            None,
            "a non-resolver failure must not synthesize an override"
        );
        assert_eq!(
            deleted_history_override(
                "hint: `pkg` was filtered ... Consider using `exclude-newer-package` ..."
            ),
            None,
            "a hint with no parsable publish timestamp must not synthesize an override"
        );
    }

    // what this catches: the wheel-less era sdist (pylint-5859, live 2026-08-11) —
    // lazy-object-proxy 1.7.1 has no arm64 wheel and its sdist stamps 0.0.0, so the
    // date-pinned install can NEVER succeed; the heal must lift exactly that package's
    // cutoff. Unrelated errors and mismatches without a named package parse to None.
    #[test]
    fn metadata_mismatch_parses_into_a_lifted_cutoff() {
        let stderr = "\u{2570}\u{2500}\u{25b6} Package metadata version `0.0.0` does not \
            match given version `1.7.1`\n\nhint: `lazy-object-proxy` (v1.7.1) was included \
            because `pylint` (v2.13.0.dev0) depends on `astroid` (v2.9.3)";
        assert_eq!(
            metadata_mismatch_override(stderr).as_deref(),
            Some("lazy-object-proxy=9999-01-01T00:00:00Z"),
        );
        assert_eq!(
            metadata_mismatch_override("error: some other failure"),
            None
        );
        assert_eq!(
            metadata_mismatch_override(
                "Package metadata version `0.0.0` does not match given version `1.0` (no hint)"
            ),
            None,
            "mismatch without a named package must not synthesize an override"
        );
    }

    // what this catches: the setuptools/importlib-metadata build clash (pytest-5413 fresh
    // env, live 2026-08-12) — a 2019 graph pulls importlib-metadata 0.x, which crashes the
    // modern setuptools that --no-build-isolation builds with; the heal lifts exactly that
    // package's cutoff. Unrelated errors parse to None.
    #[test]
    fn setuptools_importlib_clash_lifts_that_packages_cutoff() {
        let stderr = "SetuptoolsWarning: Incompatibility problem.\n\
            `importlib-metadata` version is incompatible with `setuptools`.\n\
            This problem is likely to be solved by installing an updated version of \
            `importlib-metadata`.";
        assert_eq!(
            setuptools_importlib_clash_override(stderr).as_deref(),
            Some("importlib-metadata=9999-01-01T00:00:00Z"),
        );
        assert_eq!(
            setuptools_importlib_clash_override("error: unrelated"),
            None
        );
    }

    // what this catches: the hidden-collateral verdict (atlas-24066-n7) — a patch that
    // broke 30 pass-to-pass tests produced retry feedback built from the f2p report
    // alone, so she resubmitted the identical broken patch twice. The REGRESSION
    // section must lead the excerpt whenever p2p broke, name the broken tests, and
    // still carry the f2p tail after it; no-breakage keeps the old f2p-only shape.
    #[test]
    fn regression_breakage_leads_the_failure_excerpt() {
        let broken: Vec<String> = (0..12).map(|i| format!("test_p2p_{i}")).collect();
        let both = compose_failure_excerpt(
            &broken,
            "E ImportError: cannot import name 'Exp'",
            true,
            "E AssertionError: target still fails",
        )
        .expect("both sections");
        assert!(both.starts_with("REGRESSION"), "breakage must LEAD: {both}");
        assert!(both.contains("BROKE 12 test(s)"));
        assert!(
            both.contains("test_p2p_0") && both.contains("(+2 more)"),
            "names capped at 10: {both}"
        );
        assert!(
            both.contains("ImportError") && both.contains("AssertionError"),
            "both report tails present"
        );
        let regression_at = both.find("REGRESSION").unwrap();
        let f2p_at = both.find("AssertionError").unwrap();
        assert!(regression_at < f2p_at, "regression before target-test tail");

        let clean = compose_failure_excerpt(&[], "", true, "E AssertionError: target still fails")
            .expect("f2p-only");
        assert!(
            !clean.contains("REGRESSION"),
            "no fabricated regression on a clean tree"
        );

        assert!(
            compose_failure_excerpt(&[], "", false, "noise").is_none(),
            "nothing failing → no excerpt"
        );
    }

    // what this catches: the id-shape assumption that mis-scored GOLD as a real failure.
    // sympy's FAIL_TO_PASS entries are bare function names because sympy ships its own runner;
    // handing those to pytest as paths produced "file or directory not found", scored as a
    // failing test, and returned RESOLVED=0 on three instances whose environments were fine.
    #[test]
    fn a_required_id_resolves_by_node_id_or_by_bare_function_name() {
        let report = "\
tests/test_polysys.py::test_solve_poly_system PASSED [ 50%]
tests/test_polysys.py::test_solve_biquadratic FAILED [100%]
tests/test_x.py::TestC::test_param[3-4] PASSED";
        let (by_node, by_func) = parse_pytest_report(report);

        // flask/pytest shape — full node id.
        assert_eq!(
            verdict_for(
                "tests/test_polysys.py::test_solve_poly_system",
                &by_node,
                &by_func
            ),
            Some(true)
        );
        // sympy shape — BARE function name, the case that was broken.
        assert_eq!(
            verdict_for("test_solve_poly_system", &by_node, &by_func),
            Some(true)
        );
        assert_eq!(
            verdict_for("test_solve_biquadratic", &by_node, &by_func),
            Some(false)
        );
        // parametrised tests resolve by their base name.
        assert_eq!(verdict_for("test_param", &by_node, &by_func), Some(true));
        // an id nothing in the report matches is UNKNOWN, never a silent pass.
        assert_eq!(verdict_for("test_never_ran", &by_node, &by_func), None);
    }

    // what this catches: a COLORIZED report scoring 0 of N and being blamed on the environment.
    // Live incident 2026-08-18 (astropy-14995): astropy's own setup.cfg carries
    // `addopts = --color=yes`, so pytest emitted color onto a pipe. The escape lands INSIDE the
    // node id and before the verdict, `starts_with("PASSED")` never matched, and the grader
    // reported "UNGRADEABLE — PASS_TO_PASS passes 0 of 40 on the PRISTINE tree: the suite does
    // not run in this environment" — in a run whose own tail read `1 failed, 179 passed`. The
    // env was fine; the grader could not read it. Bytes below are the real shape pytest emits.
    #[test]
    fn a_colorized_report_parses_exactly_like_a_plain_one() {
        let plain = "\
astropy/nddata/mixins/tests/test_ndarithmetic.py::test_nddata_bitmask_arithmetic FAILED
astropy/nddata/mixins/tests/test_ndarithmetic.py::test_arithmetics_data PASSED";
        let colored = "\
astropy/nddata/mixins/tests/test_ndarithmetic.py::\u{1b}[1mtest_nddata_bitmask_arithmetic\u{1b}[0m \u{1b}[31mFAILED\u{1b}[0m
astropy/nddata/mixins/tests/test_ndarithmetic.py::\u{1b}[1mtest_arithmetics_data\u{1b}[0m \u{1b}[32mPASSED\u{1b}[0m";

        let (plain_node, plain_func) = parse_pytest_report(plain);
        let (color_node, color_func) = parse_pytest_report(colored);
        assert_eq!(
            color_node, plain_node,
            "color must not change which node ids were seen"
        );
        assert_eq!(color_func, plain_func, "nor the bare-name verdicts");

        // The specific thing that was returning "0 of N": a PASS must read as a pass.
        assert_eq!(
            verdict_for("test_arithmetics_data", &color_node, &color_func),
            Some(true)
        );
        assert_eq!(
            verdict_for("test_nddata_bitmask_arithmetic", &color_node, &color_func),
            Some(false)
        );
        assert!(
            !color_node.is_empty(),
            "an empty map is the failure mode that got misreported as a broken environment"
        );
    }

    // what this catches: strip_ansi corrupting ordinary text, or allocating when it need not.
    #[test]
    fn strip_ansi_is_borrow_only_when_clean_and_lossless_when_not() {
        assert!(matches!(
            strip_ansi("plain text ::  PASSED"),
            std::borrow::Cow::Borrowed(_)
        ));
        assert_eq!(strip_ansi("\u{1b}[32mPASSED\u{1b}[0m"), "PASSED");
        // OSC (title-set) sequences terminate on BEL or ST, and swallow neither more nor less.
        assert_eq!(strip_ansi("a\u{1b}]0;title\u{7}b"), "ab");
        assert_eq!(strip_ansi("a\u{1b}]0;title\u{1b}\\b"), "ab");
        // A truncated escape at end-of-input must not panic or emit garbage.
        assert_eq!(strip_ansi("tail\u{1b}"), "tail");
        assert_eq!(strip_ansi("tail\u{1b}["), "tail");
    }

    // what this catches: a bare name appearing in two files where one fails — counting it as
    // passing would let real breakage through.
    #[test]
    fn a_duplicated_bare_name_passes_only_if_every_occurrence_passed() {
        let report = "\
tests/a.py::test_shared PASSED
tests/b.py::test_shared FAILED";
        let (by_node, by_func) = parse_pytest_report(report);
        assert_eq!(verdict_for("test_shared", &by_node, &by_func), Some(false));
        assert_eq!(
            verdict_for("tests/a.py::test_shared", &by_node, &by_func),
            Some(true)
        );
    }

    // what this catches: the scope of the test run. Running the whole suite is slow and
    // running the wrong file scores nothing; the instance's own test_patch names the files.
    #[test]
    fn the_test_scope_comes_from_the_instances_own_test_patch() {
        let patch = "\
diff --git a/tests/test_polysys.py b/tests/test_polysys.py
--- a/tests/test_polysys.py
+++ b/tests/test_polysys.py
@@ -1 +1,2 @@
 x
+y
diff --git a/sympy/solvers/tests/test_other.py b/sympy/solvers/tests/test_other.py
--- a/sympy/solvers/tests/test_other.py
+++ b/sympy/solvers/tests/test_other.py
@@ -1 +1,2 @@
 a
+b";
        assert_eq!(
            patched_test_files(patch),
            vec![
                "sympy/solvers/tests/test_other.py".to_string(),
                "tests/test_polysys.py".to_string()
            ]
        );
        assert!(patched_test_files("").is_empty());
    }

    // what this catches: the INTERPRETER has an era too, not just the dependency graph. A 2014
    // requests vendors a urllib3 doing `from collections import Mapping`, deleted in 3.10 — no
    // dependency pin can rescue that, the language moved. The 3.10 rung for 2020..=2022 is the
    // same lesson from the other side (pylint-5859, live 2026-08-11): 3.11 removed
    // `inspect.formatargspec`, which era sdist BUILDS (wrapt 1.13.3) still import — a
    // March-2022 graph never targeted an interpreter released 2022-10.
    #[test]
    fn the_interpreter_is_chosen_by_the_instances_era() {
        assert_eq!(interpreter_for_year(2014), "3.9");
        assert_eq!(interpreter_for_year(2019), "3.9");
        assert_eq!(interpreter_for_year(2020), "3.10");
        assert_eq!(interpreter_for_year(2021), "3.10");
        assert_eq!(interpreter_for_year(2022), "3.10");
        assert_eq!(interpreter_for_year(2023), "3.11");
    }

    // what this catches: the silent-death hole. A detached run wrote NOTHING until it
    // finished, so "still working" and "died an hour ago" were the same observation — an
    // absent file — and two core reboots killed a run with no trace. A `running` marker that
    // outlives its core must be journaled as failed, naming the cause.
    #[test]
    fn a_run_still_marked_running_at_boot_is_journaled_as_killed() {
        let dir = tempfile::tempdir().expect("tmp");
        let p = dir.path();
        // The name `agent/solve` ACTUALLY writes. This test used to spell it
        // `swe-solve-*` — a name nothing in the tree has ever written — and asserted that
        // an `agent-solve-*` file was "another subsystem's ledger" left untouched. There is
        // no other subsystem: that WAS production, so both the reaper and the reboot guard
        // were green here and dead in the field (see SOLVE_LEDGER_PREFIX for the 19-orphan
        // measurement). A fixture that names the file differently from the writer proves
        // nothing about the writer.
        std::fs::write(
            solve_ledger_path(p, "alive"),
            r#"{"state":"running","runId":"alive","instance":"sympy__sympy-22005",
                "workspace":"/tmp/ws/sympy__sympy-22005","persona_id":"abc","acts":3}"#,
        )
        .unwrap();
        // A FINISHED run must survive the reap untouched — reaping a real verdict would
        // destroy the only record of a measurement that actually happened.
        std::fs::write(
            solve_ledger_path(p, "done"),
            r#"{"instance":"sympy__sympy-21379","acts":7,"detached":false}"#,
        )
        .unwrap();
        // A grade file is a SIBLING of its run, never a run — the `.grade` phantom.
        std::fs::write(solve_grade_path(p, "done"), r#"{"resolved":true}"#).unwrap();

        assert_eq!(
            in_flight_solve_runs_in(p),
            vec![("alive".to_string(), "sympy__sympy-22005".to_string())],
            "only OUR unfinished runs count as in flight"
        );

        let reaped = reap_orphaned_solve_runs_in(p);
        assert_eq!(reaped, vec!["alive".to_string()]);

        let after = std::fs::read_to_string(solve_ledger_path(p, "alive")).unwrap();
        assert!(
            after.contains("\"failed\":true"),
            "the orphan is now a FAILED run: {after}"
        );
        assert!(
            after.contains("killed by a core restart"),
            "and it names the cause rather than leaving a bare zero: {after}"
        );
        // The reap ANNOTATES; it must not erase where the dead run left its patch. Without
        // this the orphan becomes ungradeable — the artifact is on disk and nothing can say
        // where.
        assert!(
            after.contains("/tmp/ws/sympy__sympy-22005"),
            "the workspace pointer survives the reap: {after}"
        );
        assert!(
            after.contains("\"acts\":3"),
            "and so does what the run had journaled about itself: {after}"
        );

        let done = std::fs::read_to_string(solve_ledger_path(p, "done")).unwrap();
        assert!(
            done.contains("\"acts\":7"),
            "a finished verdict is never rewritten"
        );
        let grade = std::fs::read_to_string(solve_grade_path(p, "done")).unwrap();
        assert!(
            grade.contains("\"resolved\":true"),
            "a grade sibling is never enumerated as a run, so never reaped"
        );

        assert!(
            in_flight_solve_runs_in(p).is_empty(),
            "after the reap nothing is in flight — a second boot must not re-reap"
        );
    }

    // what this catches: a REAL verdict must survive the process that produced it, and the
    // two verdicts that must NEVER be recorded must not be. Before 2026-08-18 nothing was
    // recorded at all: two genuine SWE-bench Lite resolutions were watched passing and the
    // system retained nothing, so the board kept calling their artifacts `ungraded`.
    //
    // The fixture goes through `record_verdict` — the PRODUCTION writer — and is read back
    // through `read_verdict`/`recorded_verdicts`, the readers the board actually uses. A
    // hand-authored file here would test my belief about the writer instead of the writer,
    // which is exactly how the boot reaper stayed green while pointed at a filename nothing
    // emits ([[run-ledgers-are-typed-artifacts]] L4).
    #[test]
    fn a_real_verdict_persists_and_gold_or_errored_ones_never_do() {
        let home = tempfile::tempdir().expect("tmp");
        // `verdict_dir` derives from CONTINUUM_HOME via swe_cache_dir; isolate this test's
        // writes rather than touching the operator's real benchmarks root.
        let prev = std::env::var("CONTINUUM_HOME").ok();
        std::env::set_var("CONTINUUM_HOME", home.path());

        let real = SweVerdict {
            instance_id: "astropy__astropy-14995".into(),
            resolved: true,
            f2p_passed: 1,
            f2p_total: 1,
            p2p_passed: 40,
            p2p_total: 40,
            gate_ok: true,
            ..Default::default()
        };
        assert!(record_verdict(&real, false).unwrap().is_some());

        let back = read_verdict("astropy__astropy-14995").expect("a recorded verdict reads back");
        assert!(back.resolved, "resolution survives the round trip");
        assert_eq!((back.f2p_passed, back.p2p_total), (1, 40), "counts survive too");
        assert_eq!(
            recorded_verdicts().len(),
            1,
            "and the board's enumerator sees exactly it"
        );

        // A gold pass proves the ENV, not the citizen. Recording it would render a positive
        // control as our result.
        let gold = SweVerdict {
            instance_id: "sympy__sympy-24152".into(),
            resolved: true,
            ..Default::default()
        };
        assert!(record_verdict(&gold, true).unwrap().is_none(), "gold never records");

        // An errored verdict is an ABSENCE, never a scored zero (#384). Two ways in: an env
        // fault, and — found by the live positive control the day this landed — an EMPTY
        // candidate. A pristine tree grades `resolved: false, gate_ok: true` forever, which
        // is indistinguishable from a citizen who tried and missed, so `swe-grade` now stamps
        // an empty candidate as an error rather than letting the board score her absence.
        let errored = SweVerdict {
            instance_id: "django__django-11049".into(),
            error: Some("no candidate patch to grade — the workspace holds no diff".into()),
            ..Default::default()
        };
        assert!(
            record_verdict(&errored, false).unwrap().is_none(),
            "an errored run is an absence, not a tallied failure"
        );

        assert_eq!(
            recorded_verdicts().len(),
            1,
            "neither the control nor the fault reached the durable record"
        );

        match prev {
            Some(v) => std::env::set_var("CONTINUUM_HOME", v),
            None => std::env::remove_var("CONTINUUM_HOME"),
        }
    }

    // what this catches: the run-id parse must accept the name the writer emits and reject
    // the grade sibling. Both halves were re-derived per call site before 2026-08-18, and
    // the two spellings diverged (`swe-solve-` vs `agent-solve-`), which silently disarmed
    // the boot reaper AND the reboot guard for every production run.
    #[test]
    fn a_ledger_file_name_yields_its_run_id_and_a_grade_sibling_yields_none() {
        assert_eq!(
            solve_run_id_from_file_name("agent-solve-claim-912427a1.json"),
            Some("claim-912427a1")
        );
        assert_eq!(
            solve_run_id_from_file_name("agent-solve-claim-912427a1.grade.json"),
            None,
            "a grade is read as a sibling, never enumerated as a run"
        );
        assert_eq!(solve_run_id_from_file_name("agent-solve-.json"), None);
        assert_eq!(solve_run_id_from_file_name("swe-solve-legacy.json"), None);
        assert_eq!(solve_run_id_from_file_name("competition-abc.json"), None);
        // The writer and the reader must agree by construction, not by memory.
        let path = solve_ledger_path(std::path::Path::new("/x"), "r1");
        assert_eq!(
            solve_run_id_from_file_name(path.file_name().unwrap().to_str().unwrap()),
            Some("r1")
        );
    }

    // what this catches: test-id lists arrive JSON-encoded inside a string field, and a
    // missing/blank one must yield an empty list rather than panicking a whole run.
    #[test]
    fn test_id_lists_decode_from_their_string_field() {
        let inst = SweInstance {
            instance_id: "x__y-1".into(),
            repo: "x/y".into(),
            base_commit: "abc".into(),
            patch: String::new(),
            test_patch: String::new(),
            problem_statement: String::new(),
            created_at: "2021-05-01T00:00:00Z".into(),
            fail_to_pass: "[\"test_a\", \"test_b\"]".into(),
            pass_to_pass: String::new(),
        };
        assert_eq!(inst.f2p(), vec!["test_a".to_string(), "test_b".to_string()]);
        assert!(inst.p2p().is_empty(), "a blank list must not panic the run");
        assert_eq!(inst.year(), 2021);
    }

    // what this catches: #383's headline — the grader ran pytest for EVERYTHING, so all
    // 114 django instances of Lite-300 (38%) were structurally ungradeable. The runner is
    // selected from the instance's repo field; django resolves to its own runtests.py with
    // the official harness's directive transform (tests/x/y.py → x.y), every pytest repo
    // keeps the exact pre-seam argv, and an UNKNOWN repo defaults to pytest so a new class
    // fails loudly through the pristine gates instead of inventing a third path.
    #[test]
    fn the_runner_is_selected_by_repo_and_django_grades_through_runtests() {
        assert_eq!(runner_for_repo("django/django"), TestRunner::DjangoRuntests);
        for repo in ["sympy/sympy", "pytest-dev/pytest", "astropy/astropy", "never/heard-of-it"] {
            assert_eq!(runner_for_repo(repo), TestRunner::Pytest, "{repo}");
        }

        let files = vec![
            "tests/migrations/test_operations.py".to_string(),
            "tests/expressions/tests.py".to_string(),
        ];
        assert_eq!(
            test_invocation(TestRunner::DjangoRuntests, &files),
            vec![
                "tests/runtests.py",
                "--verbosity",
                "2",
                "--settings=test_sqlite",
                "--parallel",
                "1",
                "migrations.test_operations",
                "expressions.tests",
            ],
            "django's command shape mirrors the official per-repo spec"
        );
        assert_eq!(
            test_invocation(TestRunner::Pytest, &files[..1].to_vec()),
            vec![
                "-m",
                "pytest",
                "tests/migrations/test_operations.py",
                "-v",
                "-p",
                "no:cacheprovider",
                "--color=no",
            ],
            "the pytest argv pins the era-portable flag set, color explicitly OFF"
        );
    }

    // what this catches: selecting the JSON runner through a flag runtests.py does not have.
    // Shipped once as `--testrunner=continuum_json_runner.JsonRunner` — argparse rejected the
    // WHOLE invocation ("unrecognized arguments"), so django-10914's own GOLD patch graded
    // 0/40 in 4 seconds. runtests.py has no such flag in ANY era; it reads settings.TEST_RUNNER
    // and defaults it only when unset (verified in-tree at 1.11/2.2/3.2/4.2/5.2/main). So the
    // runner is selected by a settings MODULE, and the invocation must differ from the plain
    // one in exactly one argument — the settings value — and in nothing else.
    #[test]
    fn the_json_runner_is_selected_through_settings_never_a_flag() {
        let files = vec!["tests/expressions/tests.py".to_string()];
        let plain = test_invocation_with(TestRunner::DjangoRuntests, &files, false);
        let json = test_invocation_with(TestRunner::DjangoRuntests, &files, true);

        assert_eq!(plain.len(), json.len(), "the JSON path adds no argument");
        let differences: Vec<_> = plain.iter().zip(&json).filter(|(a, b)| a != b).collect();
        assert_eq!(
            differences,
            vec![(
                &"--settings=test_sqlite".to_string(),
                &format!("--settings={DJANGO_JSON_SETTINGS_MODULE}")
            )],
            "settings is the ONLY difference: {plain:?} vs {json:?}"
        );
        assert!(
            !json.iter().any(|a| a.contains("--testrunner")),
            "runtests.py has no --testrunner flag; passing one aborts the run before any test"
        );
        // The shim must inherit django's own suite settings rather than restate them, or it
        // drifts from whatever the era's test_sqlite happens to configure.
        assert!(DJANGO_JSON_SETTINGS_SRC.contains("from test_sqlite import *"));
        assert!(DJANGO_JSON_SETTINGS_SRC
            .contains(&format!("TEST_RUNNER = \"{DJANGO_JSON_RUNNER_MODULE}.JsonRunner\"")));
    }

    // what this catches: a canonical id alone cannot grade django. SWE-bench harvested its
    // django ids from unittest's verbose log, and unittest prints a test's DOCSTRING instead
    // of its id when it has one — so for those tests the dataset's PASS_TO_PASS entry is
    // docstring prose with no id in it at all. Measured on django-10914's gold gate: with
    // ids + renderings only, p2p capped at 38/40 and the two misses were exactly its two
    // docstring-spelled ids. All three spellings must resolve to the same outcome, and a
    // docstring shared by two tests must fold conservatively rather than let a pass mask
    // a fail.
    #[test]
    fn a_docstringed_django_test_resolves_by_its_docstring_too() {
        let report = "\
CONTINUUM_TEST {\"id\": \"test_utils.tests.AssertRaisesMsgTest.test_special_re_chars\", \"ok\": true, \"desc\": \"assertRaisesMessage shouldn't interpret RE special chars.\"}
CONTINUUM_TEST {\"id\": \"test_utils.tests.Plain.test_plain\", \"ok\": true}
CONTINUUM_TEST {\"id\": \"a.B.test_shared_one\", \"ok\": true, \"desc\": \"A shared docstring.\"}
CONTINUUM_TEST {\"id\": \"a.B.test_shared_two\", \"ok\": false, \"desc\": \"A shared docstring.\"}
noise that is not a row
CONTINUUM_TEST not json at all";
        let (by_node, by_func) = parse_django_json(report);

        // The dataset's spelling for a docstringed test IS the docstring.
        assert_eq!(
            verdict_for(
                "assertRaisesMessage shouldn't interpret RE special chars.",
                &by_node,
                &by_func
            ),
            Some(true),
            "a docstring-spelled dataset id must resolve"
        );
        // …and the canonical + rendered spellings of that same test still resolve.
        for spelling in [
            "test_utils.tests.AssertRaisesMsgTest.test_special_re_chars",
            "test_special_re_chars (test_utils.tests.AssertRaisesMsgTest)",
        ] {
            assert_eq!(verdict_for(spelling, &by_node, &by_func), Some(true), "{spelling}");
        }
        // A docstring on two tests, one failing, must NOT read as a pass.
        assert_eq!(
            verdict_for("A shared docstring.", &by_node, &by_func),
            Some(false),
            "a shared docstring folds conservatively"
        );
        // A test with no docstring is unaffected, and unparseable lines are skipped.
        assert_eq!(
            verdict_for("test_utils.tests.Plain.test_plain", &by_node, &by_func),
            Some(true)
        );
    }

    // what this catches: django report ids never resolving. The dataset's FAIL_TO_PASS
    // shape is `test_name (module.Class)`; django ≤4.0 prints exactly that, ≥4.1 appends
    // the method to the class path — both must hit the same dataset id, and the bare-name
    // fallback must extract the LEADING token (the rsplit('.') chain used for pytest ids
    // would extract `Class)` and miss forever).
    #[test]
    fn a_django_report_resolves_dataset_shaped_ids_across_django_versions() {
        let report = "\
Testing against Django installed in '/x/django'
test_combine (expressions.tests.CombinedExprTests) ... ok
test_broken (expressions.tests.CombinedExprTests) ... FAIL
test_new_form (migrations.test_operations.OperationTests.test_new_form) ... ok
test_errored (migrations.test_operations.OperationTests.test_errored) ... ERROR
test_skipped (expressions.tests.SkipTests) ... skipped 'no oracle'
System check identified no issues (0 silenced).
FAIL: test_broken (expressions.tests.CombinedExprTests)";
        let (by_node, by_func) = parse_django_report(report);

        assert_eq!(
            verdict_for(
                "test_combine (expressions.tests.CombinedExprTests)",
                &by_node,
                &by_func
            ),
            Some(true)
        );
        assert_eq!(
            verdict_for(
                "test_broken (expressions.tests.CombinedExprTests)",
                &by_node,
                &by_func
            ),
            Some(false),
            "the `FAIL:` detail header must not overwrite the per-test verdict"
        );
        // django ≥4.1 line shape normalizes back to the dataset id.
        assert_eq!(
            verdict_for(
                "test_new_form (migrations.test_operations.OperationTests)",
                &by_node,
                &by_func
            ),
            Some(true)
        );
        assert_eq!(
            verdict_for(
                "test_errored (migrations.test_operations.OperationTests)",
                &by_node,
                &by_func
            ),
            Some(false),
            "ERROR is a failure, exactly as in the pytest parser"
        );
        assert_eq!(
            verdict_for(
                "test_skipped (expressions.tests.SkipTests)",
                &by_node,
                &by_func
            ),
            Some(true),
            "skipped counts as not-failed, matching the pytest parser's SKIPPED rule"
        );
        // Bare-name fallback goes through the leading token, not rsplit('.').
        assert_eq!(
            verdict_for("test_combine (some.other.Class)", &by_node, &by_func),
            Some(true)
        );
        assert_eq!(
            verdict_for("test_never_ran (expressions.tests.X)", &by_node, &by_func),
            None,
            "an id nothing in the report matches is UNKNOWN, never a silent pass"
        );
    }

    // Per-env-class POSITIVE CONTROLS (#380's gold gate): build the REAL env for one
    // instance of a class that previously could not be graded and prove its own harness
    // executes on the pristine tree. Network- and minutes-heavy, so compile-time gated
    // like every other load test: `cargo test --features stress-tests`.
    #[cfg(feature = "stress-tests")]
    mod stress {
        use super::*;

        async fn lite_instance(id: &str) -> SweInstance {
            load_dataset("princeton-nlp/SWE-bench_Lite")
                .await
                .expect("dataset")
                .into_iter()
                .find(|i| i.instance_id == id)
                .unwrap_or_else(|| panic!("{id} not in Lite"))
        }

        // what this catches: the astropy env class (#383 cause 2) — pyerfa's sdist build
        // imports jinja2, which `--no-build-isolation` never installs, so every astropy
        // instance died at env build (`ModuleNotFoundError: No module named 'jinja2'`,
        // live 2026-08-16). Building the real env end-to-end is the only honest proof.
        #[tokio::test(flavor = "multi_thread")]
        async fn positive_control_the_astropy_env_class_builds() {
            let inst = lite_instance("astropy__astropy-12907").await;
            let work = swe_cache_dir().join("work").join(&inst.instance_id);
            clone_at(&inst, &work).await.expect("clone at base_commit");
            let py = ensure_env(&inst, &work).await.expect("env build");
            assert!(py.exists(), "venv python must exist: {}", py.display());
        }

        // what this catches: the django env+runner class (#383 cause 1) — on the pristine
        // tree with the test_patch applied, django's OWN runner must produce the known
        // profile: every FAIL_TO_PASS id resolves to a definite FAIL (the bug is present
        // and the report parses). An id resolving to None means the runner/report seam is
        // broken; resolving to true means the tree isn't at base.
        #[tokio::test(flavor = "multi_thread")]
        async fn positive_control_a_django_instance_shows_the_known_fail_profile() {
            let inst = lite_instance("django__django-16139").await;
            assert_eq!(runner_for_repo(&inst.repo), TestRunner::DjangoRuntests);
            let work = swe_cache_dir().join("work").join(&inst.instance_id);
            clone_at(&inst, &work).await.expect("clone at base_commit");
            let py = ensure_env(&inst, &work).await.expect("env build");
            apply_patch(&work, &inst.test_patch, "gate-test")
                .await
                .expect("test patch applies");
            let files = patched_test_files(&inst.test_patch);
            let (_, report) = run_tests(
                &work,
                &py,
                &inst.f2p(),
                &files,
                TestRunner::DjangoRuntests,
            )
            .await;
            assert!(!report.is_empty(), "runtests.py produced no output at all");
            // `run_tests` collapses UNKNOWN to false, which would let a broken seam
            // masquerade as the known-fail profile — so require a DEFINITE fail: the
            // report line was found, parsed, and says the bug is present.
            let (by_node, by_func) = parse_django_report(&report);
            for id in inst.f2p() {
                assert_eq!(
                    verdict_for(&id, &by_node, &by_func),
                    Some(false),
                    "{id} must resolve to a DEFINITE fail on the pristine tree; None \
                     means the runner/report seam never saw it, true means the tree \
                     is not at base. Report tail:\n{}",
                    report_tail(&report)
                );
            }
            reset_worktree(&work).await;
        }
    }
}
