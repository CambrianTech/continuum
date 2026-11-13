//! AlgoTune — Tier-1 pick #2 of the 2026-08-22 landscape sweep (NeurIPS'25,
//! Epoch-mirrored). 150+ "beat the reference library" tasks (SciPy/NumPy/sklearn/
//! CVXPY/cryptography): structurally contamination-proof because there is NO hidden
//! answer to memorize — the reference is public library code and the task is to be
//! FASTER than it, correctness-gated.
//!
//! # Adapter shape (BENCHMARKS-ARE-ADAPTERS)
//!
//! Import task + oracle only; gym rails run it:
//! - **materialize**: `git clone --depth 1` of the upstream repo into the benchmark
//!   cache + ONE embedded harness beside it; tasks project from each task dir's
//!   `description.txt`.
//! - **prompt** — the task description + "write `solver.py` exposing
//!   `solve(problem)`" into her workspace.
//! - **dod_shell** — the harness imports the upstream task class directly (their
//!   code IS the oracle: `generate_problem` for held-out instances, `is_solution`
//!   for the correctness gate, `solve` as the reference), then times reference vs
//!   her solver with the paper's own discipline: one untimed WARMUP, then
//!   **min-of-10** via `perf_counter_ns` (min suppresses contention instead of
//!   averaging it in). Correctness gate is HARD: any invalid solution exits nonzero
//!   regardless of speed ("incorrect but fast code is not useful" — KernelBench).
//!   The measured `SPEEDUP: x.xx` prints into the dod output so every receipt
//!   carries the continuous score; v1's pass/fail is the correctness gate plus
//!   parity (speedup ≥ 0.95× — she must at least MATCH the library to pass, beating
//!   it is the score). The official harmonic-mean aggregate is a report over
//!   receipts, computed when a full-suite run exists — never faked per-task.
//! - **calibration** is machine-relative, per the same-machine-ratio law: the
//!   harness grows `n` until the reference costs ≥50ms on THIS box, so the ratio is
//!   measured where timing noise is small, wherever it runs.
//!
//! License note: repo metadata says MIT (README carries no text — flagged in the
//! landscape doc's uncertainty register). We import; we never redistribute.

use crate::cognition::eval::EvalTask;

/// Where the upstream checkout lives inside the benchmark cache.
pub fn repo_dir() -> std::path::PathBuf {
    crate::cognition::gym::gym_cache_dir().join("algotune-repo")
}

const UPSTREAM: &str = "https://github.com/oripress/AlgoTune.git";

/// The ONE harness for every task (data, not per-task codegen). Runs with cwd =
/// repo root so `AlgoTuneTasks.<task>` imports exactly as upstream wrote it.
const HARNESS_PY: &str = r#"import importlib, pathlib, sys, time

task_name, workspace = sys.argv[1], sys.argv[2]

mod = importlib.import_module(f"AlgoTuneTasks.{task_name}.{task_name}")
from AlgoTuneTasks.base import TASK_REGISTRY  # populated by the import above
task_cls = TASK_REGISTRY[task_name]
task = task_cls()

sol_path = pathlib.Path(workspace) / "solver.py"
if not sol_path.exists():
    print(f"algotune harness: {sol_path} not written yet", file=sys.stderr)
    sys.exit(3)
ns = {}
exec(compile(sol_path.read_text(), str(sol_path), "exec"), ns)
if "solve" not in ns:
    print("algotune harness: solver.py defines no solve(problem)", file=sys.stderr)
    sys.exit(3)
candidate = ns["solve"]

# Machine-relative calibration: grow n until the REFERENCE costs >=50ms here.
n = 8
while True:
    problem = task.generate_problem(n, random_seed=1)
    t0 = time.perf_counter_ns()
    ref = task.solve(problem)
    ref_ms = (time.perf_counter_ns() - t0) / 1e6
    if ref_ms >= 50 or n >= 4096:
        break
    n *= 2

def min_of(fn, problem, reps=10):
    fn(problem)  # untimed warmup
    best = None
    for _ in range(reps):
        t0 = time.perf_counter_ns()
        fn(problem)
        dt = time.perf_counter_ns() - t0
        best = dt if best is None or dt < best else best
    return best

# Held-out instances: different seeds than anything she could have probed.
speedups = []
for seed in (101, 202, 303):
    problem = task.generate_problem(n, random_seed=seed)
    out = candidate(problem)
    ok = task.is_solution(problem, out)
    if not ok:
        print(f"algotune harness: INVALID solution on seed {seed} (n={n}) — "
              "correctness gate is hard, speed does not matter", file=sys.stderr)
        sys.exit(1)
    t_ref = min_of(task.solve, problem)
    t_cand = min_of(candidate, problem)
    speedups.append(t_ref / max(t_cand, 1))

s = min(speedups)  # worst-case across held-out instances — no cherry-picking
print(f"SPEEDUP: {s:.3f} (n={n}, per-seed: {[f'{x:.2f}' for x in speedups]})")
# Pass = valid AND at least parity with the reference (0.95 tolerance for jitter).
sys.exit(0 if s >= 0.95 else 1)
"#;

/// Project one task dir onto the gym rails. Pure over (name, description).
pub fn to_eval_task(name: &str, description: &str, repo: &std::path::Path) -> EvalTask {
    let dir = format!("algotune/{name}");
    EvalTask {
        id: format!("algotune-{name}"),
        prompt: format!(
            "[AlgoTune · {name}] Beat the reference implementation. Write `{dir}/solver.py` \
             in your workspace exposing a function `solve(problem)` that returns the same \
             answer as the reference for this task, FASTER. Your solution is validated by \
             the task's own checker on held-out instances (hard gate), then timed against \
             the reference (warmup + min-of-10). You PASS at parity; your score is the \
             speedup. You may use any library importable here (pip install --user if \
             needed). The reference implementation is public — read it: \
             {repo}/AlgoTuneTasks/{name}/{name}.py\n\n{description}",
            repo = repo.display(),
        ),
        dod_shell: Some(format!(
            "cd {repo} && python3 algotune_harness.py {name} $OLDPWD/{dir}",
            repo = repo.display(),
        )),
        solution_file: Some(format!("{dir}/solver.py")),
        setup_shell: Some(format!("mkdir -p {dir}")),
        lang: Some("python".to_string()),
        ..Default::default()
    }
}

/// Fingerprint of THIS adapter's conversion. Unlike DS-1000, part of the oracle
/// lives OUTSIDE the jsonl: `HARNESS_PY` is staged onto disk at materialize and
/// dod runs it from the repo — so the fingerprint hashes BOTH the canonical
/// converted task and the harness source. A harness edit without re-fetch would
/// otherwise grade under the old on-disk harness, the same stale-cache class as
/// the #2366 DS-1000 shadow. Fixed probe path: the fingerprint tracks the CODE,
/// not this machine's cache location.
pub fn adapter_fingerprint() -> String {
    let task = serde_json::to_string(&to_eval_task(
        "fingerprint_probe",
        "canonical probe description",
        std::path::Path::new("/probe/algotune-repo"),
    ))
    .unwrap_or_else(|e| format!("unserializable:{e}")); // still a deterministic fingerprint input; the real conversion would fail loud at materialize
    crate::cognition::gym::fingerprint_parts(&[&task, HARNESS_PY])
}

/// Clone/refresh the upstream repo, stage the harness, and write the converted gym.
pub async fn materialize_gym(limit: Option<usize>) -> Result<(std::path::PathBuf, usize), String> {
    let repo = repo_dir();
    if !repo.join("AlgoTuneTasks").is_dir() {
        if let Some(parent) = repo.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create cache dir: {e}"))?;
        }
        let out = tokio::process::Command::new("git")
            .args(["clone", "--depth", "1", UPSTREAM])
            .arg(&repo)
            .output()
            .await
            .map_err(|e| format!("git spawn: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "algotune clone failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }
    std::fs::write(repo.join("algotune_harness.py"), HARNESS_PY)
        .map_err(|e| format!("stage harness: {e}"))?;

    let tasks_dir = repo.join("AlgoTuneTasks");
    let mut names: Vec<String> = std::fs::read_dir(&tasks_dir)
        .map_err(|e| format!("read {}: {e}", tasks_dir.display()))?
        .flatten()
        .filter(|e| e.path().join("description.txt").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    if let Some(cap) = limit {
        names.truncate(cap);
    }
    if names.is_empty() {
        return Err("algotune: zero task dirs with description.txt — upstream layout changed".into());
    }
    let mut lines = Vec::with_capacity(names.len());
    for name in &names {
        let desc = std::fs::read_to_string(tasks_dir.join(name).join("description.txt"))
            .map_err(|e| format!("algotune {name}: read description: {e}"))?;
        let task = to_eval_task(name, desc.trim(), &repo);
        lines.push(
            serde_json::to_string(&task).map_err(|e| format!("algotune {name}: serialize: {e}"))?,
        );
    }
    crate::cognition::gym::write_fetched_gym("algotune.jsonl", &lines, &adapter_fingerprint())
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the oracle wiring. dod must run the harness FROM the repo
    // root (upstream imports depend on it) against HER workspace dir; the prompt
    // must name the same artifact the grade reads (the graded-against-a-path-she-
    // was-never-told bug class); parity gate documented in the prompt.
    #[test]
    fn task_wiring_is_consistent_and_repo_rooted() {
        let repo = std::path::Path::new("/cache/algotune-repo");
        let t = to_eval_task("svd", "SVD Task: decompose.", repo);
        assert_eq!(t.id, "algotune-svd");
        assert_eq!(t.solution_file.as_deref(), Some("algotune/svd/solver.py"));
        let dod = t.dod_shell.as_deref().unwrap();
        assert!(dod.starts_with("cd /cache/algotune-repo &&"), "{dod}");
        assert!(dod.contains("algotune_harness.py svd"), "{dod}");
        assert!(t.prompt.contains("algotune/svd/solver.py"));
        assert!(t.prompt.contains("AlgoTuneTasks/svd/svd.py"), "she must be told where the reference lives");
    }

    // what this catches: the harness's own contract drifting. The embedded harness
    // must keep the three load-bearing disciplines the landscape doc stole from the
    // paper: warmup+min-of-N timing, a HARD correctness gate before any timing
    // verdict, and worst-case (min) aggregation across held-out seeds.
    #[test]
    fn harness_keeps_the_papers_timing_discipline() {
        assert!(HARNESS_PY.contains("perf_counter_ns"));
        assert!(HARNESS_PY.contains("untimed warmup"));
        assert!(HARNESS_PY.contains("is_solution"), "correctness gate must be the task's own checker");
        assert!(HARNESS_PY.contains("min(speedups)"), "no cherry-picking across seeds");
        assert!(HARNESS_PY.contains("sys.exit(1)"), "invalid must exit nonzero");
    }
}
