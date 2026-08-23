//! Terminal-Bench 2.1 (Stanford × Laude Institute) — the agentic terminal-work
//! benchmark, imported from the Harbor registry repo
//! (<https://github.com/harbor-framework/terminal-bench-2-1>, Apache-2.0).
//!
//! 89 tasks, each a directory of `task.toml` (metadata + the verifier's own
//! timeout), `instruction.md` (the prompt, verbatim), `environment/` (a Dockerfile
//! that CONSTRUCTS the task's initial state under `/app`), and `tests/` (a pytest
//! `test_outputs.py` that inspects the FINAL state — the oracle). `tests/test.sh`
//! is Harbor's wrapper (uvx pinning + `/logs/verifier/reward.txt`), i.e. the
//! upstream HARNESS, and per BENCHMARKS-ARE-ADAPTERS we import task + oracle ONLY —
//! so the wrapper is dropped and our dod runs pytest on the oracle directly.
//!
//! # The Docker seam, and the LOUD skip policy
//!
//! Terminal-Bench environments are container images. We do not run their
//! containers (the room is the runner; her workspace is the environment), so a
//! task converts ONLY when its initial state is reproducible as plain files:
//!
//! - the Dockerfile is single-stage, `WORKDIR` under `/app`, and its `RUN` lines
//!   are exclusively package installs / apt-cache cleanup (dependency wrangling
//!   she can do herself — the SUPER lesson: the env is part of the exam);
//! - every `COPY` lands under `/app` from files present in the repo;
//! - `tests/test_outputs.py` exists (the pytest oracle).
//!
//! Anything else — `RUN` lines that BUILD state inside the image (compile a
//! binary, generate data, clone a repo), multi-stage builds, `COPY --from`
//! external images, compose topologies, foreign workdirs — is a COUNTED skip with
//! the reason recorded in the materialize outcome and the fetch receipt. Never
//! silent: a quietly shrunk denominator publishes a wrong pass rate. Measured on
//! the 2026-08-23 registry clone: 53 of 89 tasks convert; 36 skip (31
//! image-built-state, 3 foreign workdir, 2 copy-outside-/app).
//!
//! # Path projection (`/app`, `/tests`)
//!
//! Verifiers hardcode the container's absolute `/app` (and 16 of them `/tests`).
//! The staged runner maps both at GRADE time — it copies `tests/` to a resolved
//! dir rewriting the two literals to this task's absolute workspace paths, then
//! runs pytest there. The rewrite is a boundary-guarded token substitution of the
//! container mount names, applied to the ORACLE'S OWN text at run time — the task
//! text itself (instruction, staged files) is never mutated.
//!
//! # Frontier context (why this suite earns a catalog row)
//!
//! TB 2.1 is the mid-rung of the Terminal-Bench ladder: frontier harness+model
//! pairs score 74–84% (Fable 5 + Claude Code: 83.8%), while TB 3.0 ("Frontier
//! Bench") ceilings at 34.4% (GPT-5.6 Sol). Contamination honesty: the registry
//! is public GitHub; scores are an internal signal and a harness-proof, never an
//! external claim.

use crate::cognition::eval::EvalTask;

/// Where the upstream registry checkout lives inside the benchmark cache.
pub fn repo_dir() -> std::path::PathBuf {
    crate::cognition::gym::gym_cache_dir().join("terminal-bench-2-1-repo")
}

/// Where each convertible task's SEED (app/ files + tests/ + run.py) is staged at
/// materialize time. `setup_shell` copies from here into her workspace — one
/// `cp -R` instead of megabytes of base64 in the jsonl (binary fixtures: dbs,
/// pngs, fasta files). Same cache-resident-oracle shape as AlgoTune's repo.
pub fn staged_root() -> std::path::PathBuf {
    crate::cognition::gym::gym_cache_dir().join("terminal-bench-2-1-staged")
}

const UPSTREAM: &str = "https://github.com/harbor-framework/terminal-bench-2-1.git";

/// The ONE grader program for every task (data, not per-task codegen — the
/// DS-1000 shape). It stages a path-resolved copy of the oracle and runs pytest
/// on it with the task's OWN verifier timeout. Exit code IS the grade; every
/// refusal names itself on stderr so the recovery loop hands her something
/// fixable (pytest missing → pip install --user) instead of a silent env zero.
const RUNNER_PY: &str = r#"import pathlib, re, shutil, subprocess, sys

timeout = int(sys.argv[1]) if len(sys.argv) > 1 else 900
root = pathlib.Path.cwd()
tests = root / "tests"
if not (tests / "test_outputs.py").is_file():
    print("terminal-bench harness: tests/test_outputs.py missing - staging corrupt", file=sys.stderr)
    sys.exit(4)
try:
    import pytest  # noqa: F401 - presence probe only; pytest runs as a subprocess below
except ImportError:
    print("terminal-bench harness: pytest is not installed - run `pip install --user pytest` "
          "and verify again", file=sys.stderr)
    sys.exit(5)
app = root / "app"
app.mkdir(exist_ok=True)
# Container-mount projection: the upstream oracle hardcodes /app (task state) and
# /tests (its own fixtures). Rewrite those two literals - boundary-guarded so
# /apple etc. never matches - in the ORACLE's text only, at grade time.
resolved = root / "_resolved_tests"
if resolved.exists():
    shutil.rmtree(resolved)
resolved.mkdir()
mounts = [(re.compile(r"/app(?![A-Za-z0-9_.-])"), str(app.resolve())),
          (re.compile(r"/tests(?![A-Za-z0-9_.-])"), str(resolved.resolve()))]
for src in sorted(tests.rglob("*")):
    dst = resolved / src.relative_to(tests)
    if src.is_dir():
        dst.mkdir(parents=True, exist_ok=True)
        continue
    dst.parent.mkdir(parents=True, exist_ok=True)
    data = src.read_bytes()
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        dst.write_bytes(data)  # binary fixture: staged verbatim
        continue
    for pat, mapped in mounts:
        text = pat.sub(mapped, text)
    dst.write_text(text)
try:
    r = subprocess.run([sys.executable, "-m", "pytest", str(resolved / "test_outputs.py"), "-rA"],
                       cwd=root, timeout=timeout)
except subprocess.TimeoutExpired:
    print(f"terminal-bench harness: verifier timeout ({timeout}s - the task's own cap)",
          file=sys.stderr)
    sys.exit(2)
sys.exit(r.returncode)
"#;

/// One converted task, kept intermediate so the projection is testable without
/// the network or a checkout in the loop.
#[derive(Debug)]
pub struct TerminalBenchRow {
    /// Task directory name (kebab-case, e.g. `regex-log`).
    pub name: String,
    /// `instruction.md`, verbatim — the task text is never mutated.
    pub instruction: String,
    /// `[verifier] timeout_sec` from `task.toml` — the suite's own per-task cap.
    pub verifier_timeout_sec: u64,
}

/// Why a task did NOT convert. Every variant is a counted, named exclusion in
/// the materialize outcome — the honesty contract of the Docker seam.
#[derive(Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// `tests/test_outputs.py` absent — no pytest oracle to import.
    NoPytestOracle,
    /// docker-compose / multi-container topology.
    ComposeTopology,
    /// Not exactly one Dockerfile under `environment/`.
    MultiDockerfile(usize),
    /// Multi-stage build (state assembled across images).
    MultiStage(usize),
    /// `WORKDIR` outside `/app` — the path projection only maps `/app`.
    ForeignWorkdir(String),
    /// A `RUN` line that builds state inside the image (the offending command).
    ImageBuiltState(String),
    /// `COPY`/`ADD` with flags (`--from=` external images etc.).
    CopyFlagUnsupported(String),
    /// `COPY`/`ADD` destination outside `/app`.
    CopyOutsideApp(String),
    /// `ADD` from a URL — a network fetch is image-built state.
    AddFromUrl(String),
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPytestOracle => write!(f, "no pytest oracle (tests/test_outputs.py missing)"),
            Self::ComposeTopology => write!(f, "docker-compose container topology"),
            Self::MultiDockerfile(n) => write!(f, "{n} Dockerfiles under environment/"),
            Self::MultiStage(n) => write!(f, "multi-stage build ({n} FROMs)"),
            Self::ForeignWorkdir(w) => write!(f, "WORKDIR {w} outside /app"),
            Self::ImageBuiltState(cmd) => write!(f, "image-built state: RUN {cmd}"),
            Self::CopyFlagUnsupported(l) => write!(f, "unsupported COPY flags: {l}"),
            Self::CopyOutsideApp(d) => write!(f, "COPY destination {d} outside /app"),
            Self::AddFromUrl(u) => write!(f, "ADD from URL {u}"),
        }
    }
}

/// One staging instruction distilled from a Dockerfile `COPY`/`ADD`: sources are
/// paths relative to `environment/`; `dest` is normalized relative to the staged
/// `app/` dir (`""` = app root). `dest_is_dir` carries Docker's dir-vs-file dst
/// semantics so staging reproduces them exactly.
#[derive(Debug, PartialEq, Eq)]
pub struct CopySpec {
    pub sources: Vec<String>,
    pub dest: String,
    pub dest_is_dir: bool,
}

/// Is this one `RUN` piece (post `&&`/`;` split) pure dependency wrangling —
/// package-manager ops and apt-cache cleanup — rather than state construction?
/// The whitelist errs toward SKIPPING: an unrecognized command marks the task
/// Docker-required, never the other way round.
fn is_environment_only_run(piece: &str) -> bool {
    let mut p = piece.trim();
    // Strip leading VAR=value env assignments (DEBIAN_FRONTEND=noninteractive …).
    while let Some((head, rest)) = p.split_once(char::is_whitespace) {
        let is_assign = head.split_once('=').is_some_and(|(k, _)| {
            !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        });
        if is_assign {
            p = rest.trim_start();
        } else {
            break;
        }
    }
    const PKG_MGRS: [&str; 8] = [
        "apt-get ", "apt ", "pip ", "pip3 ", "npm ", "uv pip ", "python -m pip ",
        "python3 -m pip ",
    ];
    const PKG_VERBS: [&str; 6] = ["install", "update", "upgrade", "clean", "purge", "autoremove"];
    if PKG_MGRS.iter().any(|m| p.starts_with(m)) && PKG_VERBS.iter().any(|v| p.contains(v)) {
        return true;
    }
    // Apt cache cleanup — image-size hygiene, zero task state.
    if p.starts_with("rm ") && (p.contains("/var/lib/apt/lists") || p.contains("/var/cache")) {
        return true;
    }
    p == "ldconfig" || p == "true"
}

/// Classify one Dockerfile: `Ok(copy_specs)` when the initial state is pure file
/// staging (+ installable deps), `Err(reason)` when it needs the image. Pure over
/// the Dockerfile text — testable against real registry Dockerfiles verbatim.
pub fn classify_dockerfile(dockerfile: &str) -> Result<Vec<CopySpec>, SkipReason> {
    // Join backslash line continuations so a wrapped RUN is one logical line.
    let joined = dockerfile.replace("\\\r\n", " ").replace("\\\n", " ");
    let mut froms = 0usize;
    let mut workdir: Option<String> = None;
    let mut specs: Vec<CopySpec> = Vec::new();
    for raw in joined.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((instr, rest)) = line.split_once(char::is_whitespace) else {
            continue; // a bare word (e.g. a stray token) carries no instruction
        };
        let rest = rest.trim();
        match instr.to_ascii_uppercase().as_str() {
            "FROM" => froms += 1,
            "WORKDIR" => {
                if rest != "/app" && !rest.starts_with("/app/") {
                    return Err(SkipReason::ForeignWorkdir(rest.to_string()));
                }
                workdir = Some(rest.to_string());
            }
            "RUN" => {
                for piece in rest.split(&['&', ';'][..]).map(str::trim) {
                    // split on '&' handles both '&&' (empty middle piece) and ';'
                    if !piece.is_empty() && !is_environment_only_run(piece) {
                        return Err(SkipReason::ImageBuiltState(piece.to_string()));
                    }
                }
            }
            kind @ ("COPY" | "ADD") => {
                let tokens: Vec<&str> = rest.split_whitespace().collect();
                if let Some(flag) = tokens.iter().find(|t| t.starts_with("--")) {
                    return Err(SkipReason::CopyFlagUnsupported((*flag).to_string()));
                }
                if kind == "ADD" {
                    if let Some(url) = tokens.iter().find(|t| t.starts_with("http")) {
                        return Err(SkipReason::AddFromUrl((*url).to_string()));
                    }
                }
                let [srcs @ .., dst] = tokens.as_slice() else {
                    return Err(SkipReason::CopyFlagUnsupported(rest.to_string()));
                };
                if srcs.is_empty() {
                    return Err(SkipReason::CopyFlagUnsupported(rest.to_string()));
                }
                specs.push(normalize_copy(srcs, dst, workdir.as_deref())?);
            }
            // ENV/ARG/EXPOSE/CMD/ENTRYPOINT/LABEL/USER/SHELL: no initial-state effect
            // the oracle reads through the filesystem — ignored by construction.
            _ => {}
        }
    }
    if froms != 1 {
        return Err(SkipReason::MultiStage(froms));
    }
    Ok(specs)
}

/// Resolve one COPY's destination against the (possibly deep) `/app` workdir and
/// normalize it relative to the staged `app/` root, refusing anything outside.
fn normalize_copy(
    srcs: &[&str],
    dst: &str,
    workdir: Option<&str>,
) -> Result<CopySpec, SkipReason> {
    let abs = if dst.starts_with('/') {
        dst.to_string()
    } else {
        // Relative dst resolves against WORKDIR. No WORKDIR yet ⇒ Docker's default
        // `/` — which is outside /app, so it must refuse, not guess.
        let base = workdir.unwrap_or("/"); // Docker's documented default workdir is the image root
        let d = dst.trim_start_matches("./");
        if d == "." || d.is_empty() {
            base.to_string()
        } else {
            format!("{}/{}", base.trim_end_matches('/'), d)
        }
    };
    let trailing_slash = abs.len() > 1 && (abs.ends_with('/') || abs.ends_with("/."));
    let trimmed = abs.trim_end_matches('.').trim_end_matches('/');
    let rel = if trimmed == "/app" {
        String::new()
    } else if let Some(r) = trimmed.strip_prefix("/app/") {
        r.to_string()
    } else {
        return Err(SkipReason::CopyOutsideApp(abs));
    };
    // Docker: dst is a DIRECTORY when it ends with '/', is the app root, or the
    // COPY has multiple sources. (A dir SOURCE also implies contents-into-dir —
    // staging handles that per-source, since it knows which sources are dirs.)
    let dest_is_dir = trailing_slash || rel.is_empty() || srcs.len() > 1;
    Ok(CopySpec {
        sources: srcs
            .iter()
            .map(|s| {
                s.trim_start_matches("./")
                    .trim_end_matches("/.")
                    .trim_end_matches('/')
                    .to_string()
            })
            .collect(),
        dest: rel,
        dest_is_dir,
    })
}

/// Extract `[verifier] timeout_sec` from a `task.toml`. Malformed TOML is a LOUD
/// error (registry corruption), a merely-absent field takes Harbor's 900s schema
/// default (all 89 tasks carry it explicitly on the 2026-08-23 clone).
pub fn parse_verifier_timeout_sec(task_toml: &str) -> Result<u64, String> {
    let v: toml::Value =
        toml::from_str(task_toml).map_err(|e| format!("task.toml parse failed: {e}"))?;
    let secs = v
        .get("verifier")
        .and_then(|t| t.get("timeout_sec"))
        .and_then(|x| x.as_float().or_else(|| x.as_integer().map(|i| i as f64)))
        .unwrap_or(900.0); // Harbor schema default; absence is legal, only malformed TOML errors above
    if !secs.is_finite() || secs <= 0.0 {
        return Err(format!("task.toml verifier.timeout_sec is not a positive number: {secs}"));
    }
    Ok(secs as u64)
}

/// Project one converted task onto the gym rails. Pure: same row + staged root
/// in, same task out.
pub fn to_eval_task(r: &TerminalBenchRow, staged: &std::path::Path) -> EvalTask {
    let id = format!("tb21-{}", r.name);
    let dir = format!("terminalbench/{id}");
    EvalTask {
        id: id.clone(),
        prompt: format!(
            "[Terminal-Bench 2.1 · {name}] A real terminal task. Its working directory \
             (the instruction's `/app`) is staged at `{dir}/app` in your workspace — do all \
             the work there; wherever the instruction says `/app`, use `{dir}/app`. Verify \
             with the task's own tests: `cd {dir} && python3 run.py {timeout}` (exit 0 = \
             pass; it prints the failing assertions otherwise). If a tool or python module \
             is missing, install it for your user (pip install --user <pkg>) and re-verify \
             — environment wrangling is part of the task.\n\n{instruction}",
            name = r.name,
            dir = dir,
            timeout = r.verifier_timeout_sec,
            instruction = r.instruction,
        ),
        // dod supersedes test/expect; the grade is the task's own pytest oracle.
        dod_shell: Some(format!(
            "cd {dir} && python3 run.py {timeout}",
            timeout = r.verifier_timeout_sec
        )),
        // The seed (app/ files, tests/ oracle, run.py) was staged into the cache at
        // materialize; setup copies it whole — no program text passes through shell
        // quoting, and binary fixtures arrive byte-exact.
        setup_shell: Some(format!(
            "mkdir -p {dir} && cp -R {staged}/{name}/. {dir}/",
            staged = staged.display(),
            name = r.name,
        )),
        lang: Some("python".to_string()),
        ..Default::default()
    }
}

/// Fingerprint of THIS adapter's conversion: the canonical probe task's JSON plus
/// the runner the tasks stage (part of the oracle living outside the jsonl — the
/// AlgoTune precedent). Any change to the prompt template, dod, staging shape, or
/// runner moves it automatically; `resolve_gym` refuses a stale cache. Fixed
/// probe path: the fingerprint tracks the CODE, not this machine's cache dir.
pub fn adapter_fingerprint() -> String {
    let probe = TerminalBenchRow {
        name: "fingerprint-probe".to_string(),
        instruction: "canonical probe instruction".to_string(),
        verifier_timeout_sec: 900,
    };
    let task = serde_json::to_string(&to_eval_task(
        &probe,
        std::path::Path::new("/probe/terminal-bench-staged"),
    ))
    .unwrap_or_else(|e| format!("unserializable:{e}")); // still a deterministic fingerprint input; the real conversion would fail loud at materialize
    crate::cognition::gym::fingerprint_parts(&[&task, RUNNER_PY])
}

/// The materialize receipt: where the gym landed, how many tasks converted, and
/// EVERY exclusion with its reason — the counted-skip half of the Docker seam's
/// honesty contract.
pub struct TerminalBenchMaterialized {
    pub path: std::path::PathBuf,
    pub converted: usize,
    /// `(task_name, reason)` for every registry task that did not convert.
    pub skipped: Vec<(String, String)>,
}

/// Clone/refresh the registry, classify + stage every task, write the converted
/// gym. Registry-shape corruption (missing instruction.md, unreadable task.toml,
/// a COPY source that does not exist) is a LOUD error; only the declared Docker
/// seam produces (counted) skips.
pub async fn materialize_gym(limit: Option<usize>) -> Result<TerminalBenchMaterialized, String> {
    let repo = repo_dir();
    if !repo.join("tasks").is_dir() {
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
                "terminal-bench clone failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }
    let tasks_dir = repo.join("tasks");
    let staged = staged_root();
    std::fs::create_dir_all(&staged).map_err(|e| format!("create {}: {e}", staged.display()))?;

    let mut names: Vec<String> = std::fs::read_dir(&tasks_dir)
        .map_err(|e| format!("read {}: {e}", tasks_dir.display()))?
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();

    let mut lines: Vec<String> = Vec::new();
    let mut skipped: Vec<(String, String)> = Vec::new();
    for name in &names {
        if let Some(cap) = limit {
            if lines.len() >= cap {
                break;
            }
        }
        let task_dir = tasks_dir.join(name);
        match convert_task(name, &task_dir, &staged)? {
            Conversion::Converted(task) => lines.push(
                serde_json::to_string(&task)
                    .map_err(|e| format!("terminal-bench {name}: serialize: {e}"))?,
            ),
            Conversion::Skipped(reason) => skipped.push((name.clone(), reason.to_string())),
        }
    }
    if lines.is_empty() {
        return Err(format!(
            "terminal-bench: zero tasks converted of {} in the registry — upstream layout \
             changed, or the Docker seam now excludes everything (skips: {:?})",
            names.len(),
            skipped
        ));
    }
    let (path, converted) = crate::cognition::gym::write_fetched_gym(
        "terminal-bench.jsonl",
        &lines,
        &adapter_fingerprint(),
    )?;
    crate::probe!(
        class = "benchmark.terminalbench.materialized",
        converted = converted,
        skipped = skipped.len(),
        registry_tasks = names.len(),
        path = %path.display(),
        "terminal-bench gym materialized; every exclusion is counted in the receipt"
    );
    Ok(TerminalBenchMaterialized {
        path,
        converted,
        skipped,
    })
}

enum Conversion {
    Converted(EvalTask),
    Skipped(SkipReason),
}

/// Classify one registry task dir and, when convertible, stage its seed and
/// project it. `Err` = registry corruption (loud); `Skipped` = the Docker seam.
fn convert_task(
    name: &str,
    task_dir: &std::path::Path,
    staged: &std::path::Path,
) -> Result<Conversion, String> {
    let tests_dir = task_dir.join("tests");
    if !tests_dir.join("test_outputs.py").is_file() {
        return Ok(Conversion::Skipped(SkipReason::NoPytestOracle));
    }
    let env_dir = task_dir.join("environment");
    if !env_dir.is_dir() {
        return Ok(Conversion::Skipped(SkipReason::MultiDockerfile(0)));
    }
    let mut dockerfiles: Vec<std::path::PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&env_dir)
        .map_err(|e| format!("terminal-bench {name}: read environment/: {e}"))?
        .flatten()
    {
        let fname = entry.file_name().to_string_lossy().into_owned();
        if fname.contains("compose") {
            return Ok(Conversion::Skipped(SkipReason::ComposeTopology));
        }
        if fname.contains("Dockerfile") {
            dockerfiles.push(entry.path());
        }
    }
    let [dockerfile] = dockerfiles.as_slice() else {
        return Ok(Conversion::Skipped(SkipReason::MultiDockerfile(dockerfiles.len())));
    };
    let df_text = std::fs::read_to_string(dockerfile)
        .map_err(|e| format!("terminal-bench {name}: read Dockerfile: {e}"))?;
    let specs = match classify_dockerfile(&df_text) {
        Ok(specs) => specs,
        Err(reason) => return Ok(Conversion::Skipped(reason)),
    };

    let instruction = std::fs::read_to_string(task_dir.join("instruction.md"))
        .map_err(|e| format!("terminal-bench {name}: read instruction.md: {e}"))?;
    let task_toml = std::fs::read_to_string(task_dir.join("task.toml"))
        .map_err(|e| format!("terminal-bench {name}: read task.toml: {e}"))?;
    let verifier_timeout_sec = parse_verifier_timeout_sec(&task_toml)
        .map_err(|e| format!("terminal-bench {name}: {e}"))?;

    stage_seed(name, &env_dir, &tests_dir, staged, &specs)
        .map_err(|e| format!("terminal-bench {name}: stage seed: {e}"))?;

    let row = TerminalBenchRow {
        name: name.to_string(),
        instruction: instruction.trim().to_string(),
        verifier_timeout_sec,
    };
    Ok(Conversion::Converted(to_eval_task(&row, staged)))
}

/// Build `staged/<name>/` from scratch: `app/` per the Dockerfile's COPY specs
/// (Docker semantics: a dir source contributes its CONTENTS), `tests/` verbatim
/// minus Harbor's `test.sh` wrapper, and the runner. Rebuilt every materialize so
/// a re-fetch always reflects the current registry checkout.
fn stage_seed(
    name: &str,
    env_dir: &std::path::Path,
    tests_dir: &std::path::Path,
    staged: &std::path::Path,
    specs: &[CopySpec],
) -> Result<(), String> {
    let seed = staged.join(name);
    if seed.exists() {
        std::fs::remove_dir_all(&seed).map_err(|e| format!("clear stale seed: {e}"))?;
    }
    let app = seed.join("app");
    std::fs::create_dir_all(&app).map_err(|e| format!("create app/: {e}"))?;

    for spec in specs {
        let dest_base = if spec.dest.is_empty() {
            app.clone()
        } else {
            app.join(&spec.dest)
        };
        for src_name in &spec.sources {
            let src = env_dir.join(src_name);
            if src.is_dir() {
                // Docker copies a dir source's CONTENTS into the dst directory.
                copy_dir_contents(&src, &dest_base)?;
            } else if src.is_file() {
                let target = if spec.dest_is_dir {
                    let base = src.file_name().ok_or_else(|| {
                        format!("COPY source `{src_name}` has no file name component")
                    })?;
                    dest_base.join(base)
                } else {
                    dest_base.clone()
                };
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("create {}: {e}", parent.display()))?;
                }
                std::fs::copy(&src, &target)
                    .map_err(|e| format!("copy {src_name} → {}: {e}", target.display()))?;
            } else {
                return Err(format!(
                    "Dockerfile COPYs `{src_name}` but environment/ has no such file — \
                     registry corruption, refusing to stage a partial seed"
                ));
            }
        }
    }

    // The oracle: everything under tests/ except Harbor's wrapper (we import task
    // + oracle only; the wrapper is the upstream harness).
    let staged_tests = seed.join("tests");
    copy_dir_contents_filtered(tests_dir, &staged_tests, &|p| {
        p.file_name().is_some_and(|f| f == "test.sh")
    })?;

    std::fs::write(seed.join("run.py"), RUNNER_PY).map_err(|e| format!("stage runner: {e}"))?;
    Ok(())
}

fn copy_dir_contents(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    copy_dir_contents_filtered(src, dst, &|_| false)
}

/// Recursive contents-of-src → dst copy, skipping entries the filter names.
fn copy_dir_contents_filtered(
    src: &std::path::Path,
    dst: &std::path::Path,
    skip: &dyn Fn(&std::path::Path) -> bool,
) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| format!("create {}: {e}", dst.display()))?;
    for entry in std::fs::read_dir(src)
        .map_err(|e| format!("read {}: {e}", src.display()))?
        .flatten()
    {
        let from = entry.path();
        if skip(&from) {
            continue;
        }
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_contents_filtered(&from, &to, skip)?;
        } else {
            std::fs::copy(&from, &to)
                .map_err(|e| format!("copy {} → {}: {e}", from.display(), to.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real registry Dockerfiles, verbatim from the 2026-08-23 clone — the fixtures
    // exercise the exact shapes the classifier will meet.
    const DF_COBOL: &str = "FROM ubuntu:24.04\nWORKDIR /app\nRUN apt-get update && \\\n    apt-get install -y gnucobol python3 && \\\n    rm -rf /var/lib/apt/lists/*\nCOPY src/ /app/src/\nCOPY data/ /app/data/\n";
    const DF_CHESS: &str = "FROM ubuntu:24.04\nWORKDIR /app\nRUN apt update -y\nRUN apt install -y python3-pip\nRUN mkdir -p /fonts\nCOPY make.py /app\nRUN python3 make.py\n";
    const DF_UV_STAGE: &str = "FROM ubuntu:24.04\nWORKDIR /app\nCOPY --from=ghcr.io/astral-sh/uv:0.8.14 /uv /uvx /bin/\n";

    // what this catches: the Docker seam itself. A RUN line that builds state
    // inside the image (chess-best-move's `python3 make.py` generates the board
    // png at build time) can never be reproduced by file staging — converting it
    // anyway would grade a permanently-empty workspace as a citizen failure. The
    // skip must be a NAMED exclusion carrying the offending command.
    #[test]
    fn image_built_state_is_a_named_skip_not_a_conversion() {
        let err = classify_dockerfile(DF_CHESS).unwrap_err();
        assert_eq!(err, SkipReason::ImageBuiltState("mkdir -p /fonts".to_string()));
        assert!(err.to_string().contains("mkdir -p /fonts"), "{err}");
    }

    // what this catches: the whitelist's direction of error. Package installs and
    // apt-cache cleanup are dependency wrangling she performs herself (the SUPER
    // lesson) — treating them as image-built state would silently shrink the
    // suite from 53 to 32 tasks, a wrong denominator by omission.
    #[test]
    fn dependency_wrangling_runs_convert_and_copies_project_under_app() {
        let specs = classify_dockerfile(DF_COBOL).expect("cobol-modernization converts");
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].sources, vec!["src".to_string()]);
        assert_eq!(specs[0].dest, "src");
        assert!(specs[0].dest_is_dir, "trailing-slash dst is a directory");
    }

    // what this catches: `COPY --from=<image>` pulls files out of an EXTERNAL
    // image with no FROM line betraying it — the one multi-stage shape a
    // FROM-count check misses (measured in-registry: the uv installer copy).
    #[test]
    fn copy_from_an_external_image_is_refused() {
        assert_eq!(
            classify_dockerfile(DF_UV_STAGE).unwrap_err(),
            SkipReason::CopyFlagUnsupported("--from=ghcr.io/astral-sh/uv:0.8.14".to_string())
        );
    }

    // what this catches: Docker COPY dst semantics. `COPY a.red b.red /app/` puts
    // both FILES into the dir; `COPY x /app/deps/illum1.pov` names the file
    // itself; a dst outside /app (adaptive-rejection-sampler's /protected) is
    // beyond the path projection and must skip. Getting dir-vs-file wrong stages
    // a file NAMED `deps` where the oracle expects a directory — a silent zero.
    #[test]
    fn copy_destinations_follow_docker_semantics_and_refuse_foreign_roots() {
        let multi = classify_dockerfile(
            "FROM x\nWORKDIR /app\nCOPY warriors/flashpaper.red warriors/rave.red /app/\n",
        )
        .expect("multi-source copy converts");
        assert!(multi[0].dest_is_dir, "multi-source dst is always a directory");
        assert_eq!(multi[0].sources.len(), 2);

        let file = classify_dockerfile("FROM x\nCOPY deps/illum1.pov /app/deps/illum1.pov\n")
            .expect("explicit file dst converts");
        assert_eq!(file[0].dest, "deps/illum1.pov");
        assert!(!file[0].dest_is_dir);

        // Relative dst resolves against WORKDIR (dna-assembly: `COPY sequences.fasta .`).
        let rel = classify_dockerfile("FROM x\nWORKDIR /app\nCOPY sequences.fasta .\n")
            .expect("workdir-relative dst converts");
        assert_eq!(rel[0].dest, "");
        assert!(rel[0].dest_is_dir);

        assert_eq!(
            classify_dockerfile("FROM x\nCOPY protected.tar.gz.enc /protected/\n").unwrap_err(),
            SkipReason::CopyOutsideApp("/protected/".to_string())
        );
    }

    // what this catches: the grade path wiring — dod must run the staged runner in
    // the task's own dir WITH the task's own verifier timeout, setup must copy the
    // staged seed (never inline program text through shell quoting), and the
    // prompt must name the same dir the grade reads (the graded-against-a-path-
    // she-was-never-told bug class), including the /app → workspace mapping.
    #[test]
    fn task_wiring_names_one_consistent_dir_and_carries_the_tasks_own_timeout() {
        let row = TerminalBenchRow {
            name: "regex-log".to_string(),
            instruction: "Write the regex to /app/regex.txt".to_string(),
            verifier_timeout_sec: 1800,
        };
        let t = to_eval_task(&row, std::path::Path::new("/cache/tb-staged"));
        assert_eq!(t.id, "tb21-regex-log");
        assert_eq!(
            t.dod_shell.as_deref(),
            Some("cd terminalbench/tb21-regex-log && python3 run.py 1800")
        );
        let setup = t.setup_shell.as_deref().unwrap();
        assert!(setup.contains("cp -R /cache/tb-staged/regex-log/. terminalbench/tb21-regex-log/"), "{setup}");
        assert!(t.prompt.contains("terminalbench/tb21-regex-log/app"), "the /app mapping must be told");
        assert!(t.prompt.contains("python3 run.py 1800"), "she must know the verify command");
        assert!(t.prompt.contains("Write the regex to /app/regex.txt"), "instruction verbatim");
    }

    // what this catches: the runner's oracle contract. It must call the task's own
    // pytest oracle (never Harbor's test.sh wrapper), project BOTH container
    // mounts (/app and /tests — 16 registry verifiers hardcode /tests) with a
    // boundary guard so /apple never matches, keep binary fixtures byte-exact, and
    // enforce the task's own timeout — each a measured registry shape.
    #[test]
    fn the_runner_projects_both_container_mounts_and_runs_the_pytest_oracle() {
        assert!(RUNNER_PY.contains("test_outputs.py"), "the oracle is the pytest file");
        assert!(!RUNNER_PY.contains("test.sh"), "Harbor's wrapper is the harness, not the oracle");
        assert!(RUNNER_PY.contains(r#"r"/app(?![A-Za-z0-9_.-])""#), "boundary-guarded /app mapping");
        assert!(RUNNER_PY.contains(r#"r"/tests(?![A-Za-z0-9_.-])""#), "/tests mapping for fixture-referencing oracles");
        assert!(RUNNER_PY.contains("UnicodeDecodeError"), "binary fixtures staged verbatim");
        assert!(RUNNER_PY.contains("TimeoutExpired"), "the task's own verifier cap is enforced");
        assert!(RUNNER_PY.contains("pip install --user pytest"), "a missing grader dep is fixable, not a silent zero");
    }

    // what this catches: verifier timeout extraction. The suite's caps span 600s
    // to 12000s; grading a 3600s task under a hardcoded 900s would time out
    // honest work. Malformed TOML must be loud (registry corruption), and a
    // non-positive value must never become a 0-second instant kill.
    #[test]
    fn verifier_timeout_is_the_tasks_own_and_malformed_toml_is_loud() {
        let t = "[verifier]\ntimeout_sec = 3600.0\n[agent]\ntimeout_sec = 900.0\n";
        assert_eq!(parse_verifier_timeout_sec(t).unwrap(), 3600);
        assert_eq!(parse_verifier_timeout_sec("[task]\nname = \"x\"\n").unwrap(), 900);
        assert!(parse_verifier_timeout_sec("not [ toml").is_err());
        assert!(parse_verifier_timeout_sec("[verifier]\ntimeout_sec = -5.0\n").is_err());
    }
}
