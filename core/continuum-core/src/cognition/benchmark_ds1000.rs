//! DS-1000 — the first Tier-1 adapter from the 2026-08-22 competitive-landscape sweep
//! (docs/planning/COMPETITIVE-BENCHMARK-LANDSCAPE-RESEARCH-TIER.md).
//!
//! 1,000 data-science problems (Pandas/NumPy/SciPy/sklearn/Matplotlib/PyTorch/TF)
//! with the best oracle economics in the landscape: fully local execution grading,
//! seconds per task, no Docker, no LLM judge, ~1.8% false-accept (ICML 2023).
//! License: CC-BY-SA-4.0 (both code and data — share-alike; we import, never
//! redistribute a derived set without the same license).
//!
//! # The adapter shape (BENCHMARKS-ARE-ADAPTERS, verbatim)
//!
//! Import task + oracle ONLY; the room stays the runner. Each HF row becomes one
//! [`EvalTask`] on the EXISTING gym rails — no new execution machinery:
//!
//! - **prompt** — the DS-1000 problem, plus the artifact instruction.
//! - **`solution_file`** — `ds1000/<id>/solution.py`: her hands write the artifact.
//! - **`setup_shell`** — stages the row's `code_context` (the oracle program with its
//!   `[insert]` marker) and one generic `run.py` into her workspace, both via base64
//!   so no quoting in the shell path can corrupt a program-as-data payload.
//! - **`dod_shell`** — `python3 run.py`: substitute her solution at `[insert]`,
//!   execute with the official 120s timeout, exit code IS the grade. The recovery
//!   loop feeds stderr back on failure, so a missing library is something she can
//!   FIX (pip install --user) rather than a silent env zero — the SUPER lesson:
//!   env-wrangling is part of the exam.
//!
//! # Contamination honesty
//!
//! DS-1000's perturbation defense is from 2022; a 2026 model has likely seen the set.
//! Scores are an INTERNAL regression signal and a harness-proof, never an external
//! claim ([[readme-is-a-beta-prospectus]] — receipts carry honesty).

use base64::Engine as _;

use crate::cognition::eval::EvalTask;

/// One converted row, kept intermediate so the pure conversion is testable without
/// the network or the EvalTask serializer in the loop.
#[derive(Debug)]
pub struct Ds1000Row {
    pub problem_id: u64,
    pub library: String,
    pub prompt: String,
    pub code_context: String,
}

/// The generic per-task oracle runner. ONE program for all 1,000 tasks — the task
/// dirs differ only in `context.py`, so the runner is data, not per-task codegen.
/// It appends a CALL to the context's own `test_execution(solution_string)` (+
/// `test_string` when defined) — the official API. It must NEVER text-splice the
/// solution into the source: the first cut did, and the first live grade caught it
/// corrupting quote-bearing solutions and clobbering the context's own literal
/// `.replace("[insert]", ...)` line. A context without `test_execution` fails
/// LOUD (exit 4) as staging corruption.
const RUNNER_PY: &str = r#"import pathlib, subprocess, sys
ctx = pathlib.Path("context.py").read_text()
if "def test_execution" not in ctx:
    print("ds1000 harness: context.py defines no test_execution - staging corrupt", file=sys.stderr)
    sys.exit(4)
if not pathlib.Path("solution.py").exists():
    print("ds1000 harness: solution.py not written yet", file=sys.stderr)
    sys.exit(3)
# THE OFFICIAL API: the solution is a STRING ARGUMENT to test_execution(); the
# context's own runtime performs the [insert] substitution safely inside its
# exec_context template. Textual splicing at this layer (the first cut) corrupted
# any solution containing quotes AND clobbered the context's literal
# `.replace("[insert]", ...)` call — caught on the first live grade, 2026-08-22.
driver = ctx + "\n\nimport pathlib as _pl\n_sol = _pl.Path('solution.py').read_text()\ntest_execution(_sol)\n"
if "def test_string" in ctx:
    driver += "test_string(_sol)\n"
pathlib.Path("program.py").write_text(driver)
try:
    r = subprocess.run([sys.executable, "program.py"], timeout=120)
except subprocess.TimeoutExpired:
    print("ds1000 harness: 120s official timeout exceeded", file=sys.stderr)
    sys.exit(2)
sys.exit(r.returncode)
"#;

fn b64(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
}

/// Parse one datasets-server row. Malformed rows FAIL LOUD with the row index —
/// a silently skipped row publishes a wrong denominator.
pub fn parse_row(idx: usize, v: &serde_json::Value) -> Result<Ds1000Row, String> {
    let get = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_str())
            .map(str::to_string)
            .ok_or_else(|| format!("ds-1000 row {idx}: missing/non-string field '{k}'"))
    };
    let problem_id = v
        .pointer("/metadata/problem_id")
        .and_then(|x| x.as_u64())
        .ok_or_else(|| format!("ds-1000 row {idx}: missing metadata.problem_id"))?;
    let library = v
        .pointer("/metadata/library")
        .and_then(|x| x.as_str())
        .unwrap_or("unknown") // library is prompt garnish only — never grading input; a missing tag must not void a gradeable task
        .to_string();
    let code_context = get("code_context")?;
    if !code_context.contains("[insert]") {
        return Err(format!(
            "ds-1000 row {idx} (problem {problem_id}): code_context lacks the [insert] \
             marker the oracle substitutes on — refusing to stage an ungradeable task"
        ));
    }
    Ok(Ds1000Row {
        problem_id,
        library,
        prompt: get("prompt")?,
        code_context,
    })
}

/// Project one row onto the gym rails. Pure: same row in, same task out.
pub fn to_eval_task(r: &Ds1000Row) -> EvalTask {
    let id = format!("ds1000-{:04}", r.problem_id);
    let dir = format!("ds1000/{id}");
    EvalTask {
        id: id.clone(),
        prompt: format!(
            "[DS-1000 · {lib}] Solve the following data-science problem. Write ONLY the \
             solution code (the part that replaces the problem's placeholder — typically \
             assigning the required variable, e.g. `result = ...`) to `{dir}/solution.py`. \
             Do NOT restate the surrounding context code; the grader passes your file's contents \
             to the official test_execution() as a string. If a library import fails when you \
             verify, install it for your user (pip install --user <lib>) and re-run.\n\n{p}",
            lib = r.library,
            dir = dir,
            p = r.prompt,
        ),
        // dod_shell supersedes test/expect; grading is the official program's exit code.
        dod_shell: Some(format!("cd {dir} && python3 run.py")),
        solution_file: Some(format!("{dir}/solution.py")),
        setup_shell: Some(format!(
            "mkdir -p {dir} && printf '%s' '{ctx}' | base64 -d > {dir}/context.py && \
             printf '%s' '{run}' | base64 -d > {dir}/run.py",
            dir = dir,
            ctx = b64(&r.code_context),
            run = b64(RUNNER_PY),
        )),
        lang: Some("python".to_string()),
        ..Default::default()
    }
}

/// Fingerprint of THIS adapter's conversion — the hash of one canonical probe
/// row's converted output, so any change to the prompt template, staged runner,
/// setup shape, or dod moves it automatically (no hand-bumping). The gym cache
/// sidecar carries it; `resolve_gym` refuses a mismatch. Regression anchor: the
/// #2366 oracle fix left 1,000 cached tasks staging the outlawed splicing runner.
pub fn adapter_fingerprint() -> String {
    let probe = Ds1000Row {
        problem_id: 0,
        library: "Pandas".to_string(),
        prompt: "fingerprint probe".to_string(),
        code_context: "def test_execution(solution: str):\n    pass\n[insert]".to_string(),
    };
    let task = serde_json::to_string(&to_eval_task(&probe))
        .unwrap_or_else(|e| format!("unserializable:{e}")); // still a deterministic fingerprint input; the real conversion would fail loud at materialize
    crate::cognition::gym::fingerprint_parts(&[&task])
}

/// Fetch the suite off the datasets-server (cached by `stream_hf_rows`' own JSONL
/// layer) and write the converted gym file. Returns (path, task_count).
pub async fn materialize_gym(limit: Option<usize>) -> Result<(std::path::PathBuf, usize), String> {
    let mut tasks: Vec<String> = Vec::new();
    let mut idx = 0usize;
    crate::cognition::swe_bench::stream_hf_rows("xlangai/DS-1000", "default", "test", |row| {
        if let Some(cap) = limit {
            if tasks.len() >= cap {
                return Ok(());
            }
        }
        let parsed = parse_row(idx, row)?;
        idx += 1;
        let task = to_eval_task(&parsed);
        tasks.push(
            serde_json::to_string(&task)
                .map_err(|e| format!("ds-1000 {}: serialize failed: {e}", task.id))?,
        );
        Ok(())
    })
    .await?;
    if tasks.is_empty() {
        return Err("ds-1000: zero rows streamed — dataset unreachable or renamed".into());
    }
    crate::cognition::gym::write_fetched_gym("ds-1000.jsonl", &tasks, &adapter_fingerprint())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(problem_id: u64, ctx: &str) -> serde_json::Value {
        serde_json::json!({
            "prompt": "Problem: compute the thing.",
            "reference_code": "result = df.sum()",
            "metadata": {"problem_id": problem_id, "library": "Pandas"},
            "code_context": ctx,
        })
    }

    // what this catches: the oracle contract itself. A context without [insert] can
    // never grade a solution; staging it anyway would produce a permanent silent
    // zero for that task — refuse at parse, naming the problem id.
    #[test]
    fn a_context_without_the_insert_marker_is_refused() {
        let err = parse_row(7, &row(42, "print('no marker here')")).unwrap_err();
        assert!(err.contains("problem 42"), "{err}");
        assert!(err.contains("[insert]"), "{err}");
    }

    // what this catches: quoting corruption in the shell staging path. code_context
    // is a PROGRAM carrying every quote/newline/dollar shell metacharacter; the
    // round-trip through the generated setup_shell must be byte-exact or the oracle
    // grades against a corrupted test. Decoding the b64 we embed proves it.
    #[test]
    fn staged_context_round_trips_byte_exact_through_the_setup_shell() {
        let nasty = "x = \"quo'te\"\n# $HOME `cmd` \\ \u{1F600}\ntest_execution('[insert]')\n";
        let r = parse_row(0, &row(3, nasty)).unwrap();
        let t = to_eval_task(&r);
        let setup = t.setup_shell.unwrap();
        // Extract the first b64 payload (context) exactly as printf would emit it.
        let b64_ctx = setup
            .split('\'')
            .nth(3)
            .expect("setup_shell embeds the context as the second single-quoted token");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(b64_ctx)
            .expect("valid base64");
        assert_eq!(String::from_utf8(decoded).unwrap(), nasty);
        // And base64's alphabet can never terminate the single-quoted shell string.
        assert!(!b64_ctx.contains('\''));
    }

    // what this catches: the oracle calling convention itself — THE first-live-grade
    // bug. The runner must pass the solution as a STRING to test_execution() and
    // must never text-splice it into source: a quote-bearing solution corrupted
    // program.py into a SyntaxError, and the splice also clobbered the context's
    // own literal `.replace("[insert]", ...)` runtime line (both measured on
    // ds1000-0002, 2026-08-22).
    #[test]
    fn the_runner_calls_the_official_api_and_never_splices() {
        assert!(RUNNER_PY.contains("test_execution(_sol)"), "official API call");
        assert!(RUNNER_PY.contains("read_text()"), "solution delivered as a string");
        assert!(!RUNNER_PY.contains("ctx.replace"), "text-splicing is the outlawed first cut");
        assert!(RUNNER_PY.contains("def test_string"), "conditional surface-form gate preserved");
    }

    // what this catches: the grade path wiring. dod_shell must run the staged runner
    // in the task's own dir, and solution_file must be where the prompt says it is —
    // a citizen graded against a path she was never told is the measured #ds gym bug
    // class this file's card-body derivation exists to prevent.
    #[test]
    fn task_wiring_names_one_consistent_artifact_path() {
        let t = to_eval_task(&parse_row(0, &row(17, "a\n[insert]\nb")).unwrap());
        assert_eq!(t.id, "ds1000-0017");
        assert_eq!(t.solution_file.as_deref(), Some("ds1000/ds1000-0017/solution.py"));
        assert!(t.dod_shell.as_deref().unwrap().starts_with("cd ds1000/ds1000-0017 &&"));
        assert!(t.prompt.contains("ds1000/ds1000-0017/solution.py"));
        assert!(t.setup_shell.as_deref().unwrap().contains("mkdir -p ds1000/ds1000-0017"));
    }
}
