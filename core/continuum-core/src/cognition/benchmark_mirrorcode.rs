//! MirrorCode (Epoch AI × METR, MIT) — reimplement an ENTIRE program from its
//! observable behavior only: Unix utils (`cal`, `rev`, `sed`, `numfmt`), format
//! tools (`gron`, `bib2json`, `pkl`), a C preprocessor, a TeX macro engine, a
//! CAS subset, a scripting-language CLI (`wren_cli`), a linter subset (`ruff
//! check`), a brotli decompressor. One target program = one task. The oracle is
//! exact stdout/stderr/exit-code equality against recorded invocations of the
//! reference program. Frontier 2026-08 (epoch.ai/benchmarks/mirrorcode): Claude
//! Fable 5 64%, GPT-5.6 Sol 20% — on 15 programs × Go/Ada with a 10B-token
//! budget.
//!
//! # What the upstream data ACTUALLY is (measured 2026-08-23)
//!
//! The framework repo `epoch-research/MirrorCode` ships, in plain git (no LFS):
//! - `data/gold_outputs/<prog>.jsonl` — for each of 26 public programs, the FULL
//!   recorded case set: `test_case` (args/env/stdin/files) + gold
//!   stdout/stderr/returncode. Rows labeled `hidden` are the anti-hardcoding
//!   duals of visible cases; **they are public in this repo too** — "held out"
//!   means held out from the agent's workspace, not from the world.
//! - `mc/<prog>/` — the task definition, the reference source (`src/`), and for
//!   most programs the reference manual (`docs/`).
//! - `mc/_data/batch_score_test_cases.py` + `mc/scorer.py` — the official
//!   execution + comparison discipline this adapter's harness ports verbatim:
//!   per-case 2s timeout, fresh run dir with per-case `files` staged, exit codes
//!   compared mod 256, streams compared exactly with a ONE-trailing-newline
//!   tolerance (`permissive_equals`).
//!
//! (The sibling repo `epoch-research/MirrorCode-data` is NOT the import source:
//! it holds LFS-gated run exports for a handful of eval sets, not the task data.)
//!
//! # Adapter shape (BENCHMARKS-ARE-ADAPTERS, verbatim)
//!
//! Import task + oracle only; the room stays the runner. Each program becomes one
//! [`EvalTask`] on the existing gym rails:
//! - **materialize** — `git clone --depth 1` of the upstream repo into the
//!   benchmark cache; every gold row is validated LOUD (a malformed row corrupts
//!   the case denominator); the VISIBLE rows are projected into
//!   `visible_cases/<prog>.jsonl` beside the checkout (labels stripped, expected
//!   outputs kept — see the declared deltas), and ONE embedded harness is staged.
//! - **prompt** — reimplement `<target_cmd>` in Rust from the staged visible
//!   cases (+ the reference manual when upstream ships one). The reference
//!   SOURCE in the checkout is declared out of bounds — behavior-only is the
//!   exam.
//! - **setup_shell** — stages a Cargo crate skeleton (manifest via base64 —
//!   never raw shell-quoted payloads) and COPIES the visible-case file from the
//!   cache (up to 43 MB for `gotree`; embedding that as base64 in a shell string
//!   would blow past macOS's 1 MB ARG_MAX, so big payloads travel by `cp`, which
//!   keeps the same no-payload-in-shell property).
//! - **dod_shell** — the harness builds her crate (`cargo build --release`,
//!   `CARGO_TARGET_DIR` explicitly dropped so the shared cache cannot swallow
//!   the binary) and replays EVERY recorded case — visible AND hidden duals —
//!   with the official comparison. Any mismatch fails loud with the first diffs
//!   (case name, stream, byte offset, excerpts) on stderr; 100% pass required,
//!   exactly like upstream's leaderboard scoring.
//!
//! # Declared deltas from the official configuration (receipt honesty)
//!
//! 1. **Rust target language** — our citizens' strongest toolchain; the official
//!    leaderboard runs Go/Ada. Scores are a language variant, not comparable.
//! 2. **Expected outputs stand in for the reference binary.** Upstream agents
//!    get case INPUTS plus a runnable reference binary / an `evaluate_testcases`
//!    oracle tool; we have neither, so the visible cases are staged WITH their
//!    expected outputs. Same information channel, different plumbing — declared.
//! 3. **Our own act budget**, not their 10B-token attempts (their longest single
//!    sample ran 19 days).
//! 4. **Contamination**: the ENTIRE oracle (hidden duals included) is public in
//!    the upstream repo, and the reference source sits in the same checkout her
//!    tools could reach (kept out by instruction, not by sandbox). Scores are an
//!    INTERNAL signal only, never an external claim
//!    ([[readme-is-a-beta-prospectus]] — receipts carry honesty).

use base64::Engine as _;

use crate::cognition::eval::EvalTask;

/// Where the upstream checkout lives inside the benchmark cache.
pub fn repo_dir() -> std::path::PathBuf {
    crate::cognition::gym::gym_cache_dir().join("mirrorcode-repo")
}

const UPSTREAM: &str = "https://github.com/epoch-research/MirrorCode.git";

/// The 26 public target programs and the command whose behavior each one
/// reimplements — upstream's `mc/<prog>/<prog>.py::run_cmd()`, transcribed
/// (2026-08-23). This table is the conversion allowlist: a gold-output file
/// upstream adds later has NO known target command, so it is a LOUD counted
/// skip at materialize (probe class `benchmark.mirrorcode.skip`), never a
/// silently converted task with a guessed invocation.
const PROGRAMS: &[(&str, &str)] = &[
    ("bib2json", "pandoc -f biblatex -t csljson"),
    ("bitwise", "bitwise"),
    ("brotlid", "brotli -d"),
    ("cal", "cal"),
    ("cal_simple", "cal"), // CalSimpleTarget subclasses CalTarget: same command, reduced scope
    ("choose", "choose"),
    ("cprepro", "cc1 -E -quiet"),
    ("dirname", "dirname"),
    ("false", "false"),
    ("giac_subset", "giac"),
    ("gotree", "gotree"),
    ("gron", "gron"),
    ("hexyl", "hexyl"),
    ("jq_simple", "jq"),
    ("mailauth", "mailauth"),
    ("nonogrid", "nonogrid"),
    ("numfmt", "numfmt"),
    ("pkl", "pkl eval"),
    ("qsv_select", "qsv select"),
    ("rev", "rev"),
    ("ruff", "ruff check"),
    ("sed", "sed"),
    ("texmacros", "pdftex"),
    ("tssql", "tssql"),
    ("uuidparse", "uuidparse"),
    ("wren_cli", "wren_cli"),
];

/// The ONE harness for every program (data, not per-task codegen). Runs with
/// cwd = repo root; args = program name + her task dir. Ports the official
/// discipline from `mc/_data/batch_score_test_cases.py` + `mc/scorer.py`:
/// fresh per-case run dir with `files` staged (str / bytes_base64), env merged
/// over the process env, stdin utf-8, 2s per case, exit codes mod 256, streams
/// exact with one-trailing-newline tolerance. Verdict is all-or-nothing (their
/// leaderboard bar): the first mismatches print case name, stream, byte offset
/// and excerpts to stderr so the recovery loop has a real diff to fix.
const HARNESS_PY: &str = r#"import base64, json, os, pathlib, shutil, subprocess, sys, time

PER_CASE_TIMEOUT = 2      # upstream mc PER_TEST_CASE_TIMEOUT
HARNESS_DEADLINE_S = 600  # whole-suite ceiling: a hung binary is an env fault, never a verdict
MAX_REPORTED_FAILURES = 3

# resolve(): per-case exec runs with cwd = the case's run dir, so a relative
# task dir would make the binary path dangle (caught live on the first smoke).
prog, task_dir = sys.argv[1], pathlib.Path(sys.argv[2]).resolve()
start = time.monotonic()

gold_path = pathlib.Path("data/gold_outputs") / (prog + ".jsonl")
if not gold_path.is_file():
    print(f"mirrorcode harness: {gold_path} missing - upstream layout changed", file=sys.stderr)
    sys.exit(4)

main_rs = task_dir / "src" / "main.rs"
if not main_rs.is_file():
    print(f"mirrorcode harness: {main_rs} not written yet", file=sys.stderr)
    sys.exit(3)

env = os.environ.copy()
# The shared cargo cache would swallow the binary; build INSIDE the task dir.
env.pop("CARGO_TARGET_DIR", None)
try:
    build = subprocess.run(
        ["cargo", "build", "--release", "--target-dir", "target"],
        cwd=task_dir, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        timeout=HARNESS_DEADLINE_S,
    )
except subprocess.TimeoutExpired:
    print(f"mirrorcode harness: cargo build exceeded {HARNESS_DEADLINE_S}s", file=sys.stderr)
    sys.exit(2)
if build.returncode != 0:
    tail = build.stderr.decode("utf-8", "replace")[-4000:]
    print(f"mirrorcode harness: cargo build failed\n{tail}", file=sys.stderr)
    sys.exit(1)

binary = task_dir / "target" / "release" / prog
if not binary.is_file():
    print(f"mirrorcode harness: build ok but {binary} missing - the [[bin]] target "
          f"must stay named `{prog}` (it is staged that way in Cargo.toml)", file=sys.stderr)
    sys.exit(1)

def normalize_exit_code(code):
    # upstream mc.scorer: signed/unsigned 8-bit equivalence (-6 == 250)
    return code % 256

def permissive_equals(a, b):
    # upstream mc.scorer: EXACT match, tolerating exactly one trailing newline
    if a == b:
        return True
    if a.endswith("\n") and a[:-1] == b:
        return True
    if b.endswith("\n") and a == b[:-1]:
        return True
    return False

def first_diff(expected, actual):
    n = min(len(expected), len(actual))
    for j in range(n):
        if expected[j] != actual[j]:
            return j
    return n

def excerpt(s, at):
    lo, hi = max(0, at - 30), at + 30
    return repr(s[lo:hi])

run_dir = task_dir / "run"
run_dir.mkdir(exist_ok=True)

def stage_files(files):
    for rel, f in files.items():
        p = run_dir / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        if f["format"] == "bytes_base64":
            p.write_bytes(base64.b64decode(f["content"]))
        elif f["format"] == "str":
            p.write_text(f["content"])
        else:
            raise RuntimeError(f"unknown file format {f['format']}")

def cleanup():
    for item in run_dir.iterdir():
        if item.is_dir():
            shutil.rmtree(item)
        else:
            item.unlink(missing_ok=True)

with open(gold_path, encoding="utf-8") as fh:
    cases = [json.loads(line) for line in fh if line.strip()]

passed = 0
visible = 0
hidden = 0
failures = []
attempted = 0
stopped_early = False
for i, row in enumerate(cases):
    if time.monotonic() - start > HARNESS_DEADLINE_S:
        print(f"mirrorcode harness: {HARNESS_DEADLINE_S}s deadline exceeded after {i} cases",
              file=sys.stderr)
        sys.exit(2)
    tc = row["test_case"]
    labels = tc.get("labels") or []
    if "hidden" in labels or "ablated" in labels:
        hidden += 1
    else:
        visible += 1
    name = tc.get("name") or f"case[{i}]"
    attempted += 1
    verdict = None
    try:
        stage_files(tc.get("files") or {})
        cenv = env.copy()
        cenv.update(tc.get("env") or {})
        r = subprocess.run(
            [str(binary)] + (tc.get("args") or []),
            input=(tc.get("stdin") or "").encode("utf-8"),
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            env=cenv, cwd=run_dir, timeout=PER_CASE_TIMEOUT,
        )
        out = r.stdout.decode("utf-8", "replace")
        err = r.stderr.decode("utf-8", "replace")
        if normalize_exit_code(r.returncode) != normalize_exit_code(row["returncode"]):
            verdict = (f"exit code {normalize_exit_code(r.returncode)} != "
                       f"expected {normalize_exit_code(row['returncode'])}")
        elif not permissive_equals(out, row["stdout"]):
            at = first_diff(row["stdout"], out)
            verdict = (f"stdout diff at byte {at}: expected {excerpt(row['stdout'], at)} "
                       f"got {excerpt(out, at)}")
        elif not permissive_equals(err, row["stderr"]):
            at = first_diff(row["stderr"], err)
            verdict = (f"stderr diff at byte {at}: expected {excerpt(row['stderr'], at)} "
                       f"got {excerpt(err, at)}")
    except subprocess.TimeoutExpired:
        verdict = f"timed out ({PER_CASE_TIMEOUT}s)"
    except Exception as e:
        verdict = f"harness exception: {e}"
    finally:
        cleanup()
    if verdict is None:
        passed += 1
    else:
        failures.append(f"FAIL {name} args={tc.get('args') or []}: {verdict}")
        if len(failures) >= MAX_REPORTED_FAILURES:
            stopped_early = i + 1 < len(cases)
            break

if failures:
    for f in failures:
        print(f, file=sys.stderr)
    note = f" (stopped early after {attempted} of {len(cases)})" if stopped_early else ""
    print(f"mirrorcode harness: FAILED {len(failures)} of {attempted} attempted cases, "
          f"{passed} passed{note} - 100% of {len(cases)} required", file=sys.stderr)
    sys.exit(1)

print(f"MIRRORCODE PASS: {passed}/{len(cases)} cases (visible {visible}, hidden {hidden})")
sys.exit(0)
"#;

fn b64(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
}

/// The Cargo manifest staged into her workspace. The PACKAGE name is prefixed
/// (`mirrorcode_<prog>`) because bare program names collide with Rust keywords
/// — `false` is a real target program and an illegal package name — while the
/// `[[bin]]` TARGET keeps the program's own name, which is what the harness
/// executes (`target/release/<prog>`; verified `[[bin]] name = "false"` builds).
pub fn cargo_manifest(program: &str) -> String {
    format!(
        "[package]\nname = \"mirrorcode_{program}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [[bin]]\nname = \"{program}\"\npath = \"src/main.rs\"\n"
    )
}

/// One validated gold row, reduced to what conversion needs.
#[derive(Debug)]
pub struct GoldRow {
    /// `hidden`/`ablated` label — excluded from the agent-visible projection,
    /// still graded by the harness.
    pub hidden: bool,
    /// The agent-view JSON line: `test_case` with `labels`/`visible_duals`
    /// stripped (upstream strips them for the agent too) + the expected
    /// outputs (our declared stand-in for the reference binary).
    pub agent_view: String,
}

/// Validate one gold-output row. Malformed rows FAIL LOUD with program + row
/// index — a silently dropped row corrupts the case denominator the receipt
/// reports, and the harness would still grade against it.
pub fn parse_gold_row(program: &str, idx: usize, v: &serde_json::Value) -> Result<GoldRow, String> {
    let loud = |what: &str| format!("mirrorcode {program} row {idx}: {what}");
    let tc = v
        .get("test_case")
        .and_then(|x| x.as_object())
        .ok_or_else(|| loud("missing/non-object test_case"))?;
    let stdout = v
        .get("stdout")
        .and_then(|x| x.as_str())
        .ok_or_else(|| loud("missing/non-string gold stdout"))?;
    let stderr = v
        .get("stderr")
        .and_then(|x| x.as_str())
        .ok_or_else(|| loud("missing/non-string gold stderr"))?;
    let returncode = v
        .get("returncode")
        .and_then(|x| x.as_i64())
        .ok_or_else(|| loud("missing/non-integer gold returncode"))?;
    // Per-case staged files must be decodable by the harness: upstream's
    // serializer has a third `error` format that records a capture FAILURE —
    // staging it would grade against corruption, so refuse at conversion.
    if let Some(files) = tc.get("files").and_then(|x| x.as_object()) {
        for (path, f) in files {
            let fmt = f.get("format").and_then(|x| x.as_str()).unwrap_or("<missing>"); // absent format IS the refusal case below — the placeholder only names it in the error
            let content_ok = f.get("content").and_then(|x| x.as_str()).is_some();
            if !matches!(fmt, "str" | "bytes_base64") || !content_ok {
                return Err(loud(&format!(
                    "staged file '{path}' has format '{fmt}' — only str/bytes_base64 \
                     are stageable; 'error' records a capture failure upstream"
                )));
            }
        }
    }
    let hidden = tc
        .get("labels")
        .and_then(|x| x.as_array())
        .map(|ls| {
            ls.iter()
                .filter_map(|l| l.as_str())
                .any(|l| l == "hidden" || l == "ablated")
        })
        .unwrap_or(false); // no labels array = a plain visible case — upstream omits the field for agent views
    let mut tc_clean = tc.clone();
    tc_clean.remove("labels");
    tc_clean.remove("visible_duals");
    let agent = serde_json::json!({
        "test_case": tc_clean,
        "expected": {"stdout": stdout, "stderr": stderr, "returncode": returncode},
    });
    Ok(GoldRow {
        hidden,
        agent_view: serde_json::to_string(&agent)
            .map_err(|e| loud(&format!("agent view serialize: {e}")))?,
    })
}

/// Project one program onto the gym rails. Pure over its arguments.
pub fn to_eval_task(
    program: &str,
    target_cmd: &str,
    visible: usize,
    hidden: usize,
    repo: &std::path::Path,
    has_docs: bool,
) -> EvalTask {
    let total = visible + hidden;
    let dir = format!("mirrorcode/{program}");
    let docs_line = if has_docs {
        format!(
            "The reference program's manual is at {repo}/mc/{program}/docs/ — reading the \
             docs is allowed (when the manual and the recorded behavior disagree, the \
             recorded behavior wins).\n",
            repo = repo.display(),
        )
    } else {
        String::new()
    };
    EvalTask {
        id: format!("mirrorcode-{program}"),
        prompt: format!(
            "[MirrorCode · {program}] Reimplement the program `{target_cmd}` in Rust, from \
             its observable behavior ONLY.\n\
             A Cargo crate is staged at `{dir}` (package `mirrorcode_{program}`, binary \
             target `{program}` — keep both names). Write your implementation to \
             `{dir}/src/main.rs`; you may add dependencies to `{dir}/Cargo.toml`. Build \
             with `cd {dir} && cargo build --release --target-dir target`; the grader runs \
             `target/release/{program}` with each recorded case's arguments (stdin piped, \
             per-case files staged in the working directory, per-case env applied).\n\
             Your behavior spec is `{dir}/cases_visible.jsonl`: {visible} recorded \
             invocations of the reference program, each with args/env/stdin/files and the \
             expected stdout, stderr and exit code. Self-test against it with your tools \
             before you finish.\n\
             Grading is EXACT match on stdout, stderr AND exit code (one trailing newline \
             tolerated; exit codes compare mod 256; 2s per case) over ALL {total} recorded \
             cases — the {visible} visible ones plus {hidden} hidden duals that probe the \
             same behaviors with different inputs, so hardcoding the visible answers \
             fails. 100% must pass.\n\
             {docs_line}\
             INTEGRITY: the reference implementation's source exists in the benchmark \
             cache (mc/{program}/src/) — reading it, or shelling out to an existing \
             system binary, voids this exam. Behavior-only.",
        ),
        dod_shell: Some(format!(
            "cd {repo} && python3 mirrorcode_harness.py {program} $OLDPWD/{dir}",
            repo = repo.display(),
        )),
        solution_file: Some(format!("{dir}/src/main.rs")),
        // The manifest travels as base64 (never raw shell-quoted); the visible
        // cases travel by cp because they reach 43 MB (gotree) and a shell
        // string caps at ARG_MAX (1 MB on macOS).
        setup_shell: Some(format!(
            "mkdir -p {dir}/src && cp {repo}/visible_cases/{program}.jsonl \
             {dir}/cases_visible.jsonl && printf '%s' '{toml}' | base64 -d > {dir}/Cargo.toml",
            repo = repo.display(),
            toml = b64(&cargo_manifest(program)),
        )),
        lang: Some("rust".to_string()),
        ..Default::default()
    }
}

/// Fingerprint of THIS adapter's conversion. Like AlgoTune, part of the oracle
/// lives OUTSIDE the jsonl: `HARNESS_PY` and the derived `visible_cases/` files
/// are staged onto disk at materialize — so the fingerprint hashes BOTH the
/// canonical converted task and the harness source. A harness edit without
/// re-fetch would otherwise grade under the old on-disk harness (the #2366
/// stale-cache class). Fixed probe path: the fingerprint tracks the CODE, not
/// this machine's cache location.
pub fn adapter_fingerprint() -> String {
    let task = serde_json::to_string(&to_eval_task(
        "fingerprint_probe",
        "probe --cmd",
        3,
        2,
        std::path::Path::new("/probe/mirrorcode-repo"),
        true,
    ))
    .unwrap_or_else(|e| format!("unserializable:{e}")); // still a deterministic fingerprint input; the real conversion would fail loud at materialize
    crate::cognition::gym::fingerprint_parts(&[&task, HARNESS_PY])
}

/// Clone/refresh the upstream repo, validate + project every program's gold
/// cases, stage the harness and the visible-case files, and write the converted
/// gym. Returns (path, task_count).
pub async fn materialize_gym(limit: Option<usize>) -> Result<(std::path::PathBuf, usize), String> {
    let repo = repo_dir();
    let gold_dir = repo.join("data/gold_outputs");
    if !gold_dir.is_dir() {
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
                "mirrorcode clone failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        if !gold_dir.is_dir() {
            return Err(format!(
                "mirrorcode: clone succeeded but {} is missing — upstream layout changed",
                gold_dir.display()
            ));
        }
    }
    std::fs::write(repo.join("mirrorcode_harness.py"), HARNESS_PY)
        .map_err(|e| format!("stage harness: {e}"))?;
    let vis_dir = repo.join("visible_cases");
    std::fs::create_dir_all(&vis_dir).map_err(|e| format!("create visible_cases: {e}"))?;

    // LOUD counted skip: a gold file upstream ships that our allowlist doesn't
    // know has no known target command — converting it with a guessed
    // invocation would grade garbage, so it is named and counted, never silent.
    let known: std::collections::HashSet<&str> = PROGRAMS.iter().map(|(n, _)| *n).collect();
    let mut skipped: Vec<String> = std::fs::read_dir(&gold_dir)
        .map_err(|e| format!("read {}: {e}", gold_dir.display()))?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".jsonl")
                .filter(|stem| !known.contains(stem))
                .map(str::to_string)
        })
        .collect();
    skipped.sort();
    if !skipped.is_empty() {
        crate::probe!(
            class = "benchmark.mirrorcode.skip",
            skipped = %skipped.join(","),
            count = skipped.len(),
            "gold-output files with no known target command — NOT converted; add them \
             to the PROGRAMS allowlist with their upstream run_cmd to import them"
        );
    }

    let mut lines: Vec<String> = Vec::with_capacity(PROGRAMS.len());
    for (program, target_cmd) in PROGRAMS {
        let gold_path = gold_dir.join(format!("{program}.jsonl"));
        // A KNOWN program missing upstream is a layout change, not a skip.
        let text = std::fs::read_to_string(&gold_path)
            .map_err(|e| format!("mirrorcode {program}: read {}: {e}", gold_path.display()))?;
        let mut visible_lines: Vec<String> = Vec::new();
        let mut hidden = 0usize;
        for (idx, line) in text.lines().filter(|l| !l.trim().is_empty()).enumerate() {
            let v: serde_json::Value = serde_json::from_str(line)
                .map_err(|e| format!("mirrorcode {program} row {idx}: bad json: {e}"))?;
            let row = parse_gold_row(program, idx, &v)?;
            if row.hidden {
                hidden += 1;
            } else {
                visible_lines.push(row.agent_view);
            }
        }
        if visible_lines.is_empty() {
            return Err(format!(
                "mirrorcode {program}: zero visible cases — nothing to hand the citizen \
                 as a behavior spec; refusing to stage an unsolvable task"
            ));
        }
        // Atomic write, same tmp+rename discipline as write_fetched_gym: a
        // half-written spec file must never be what setup_shell copies.
        let vis_path = vis_dir.join(format!("{program}.jsonl"));
        let tmp = vis_path.with_extension("jsonl.tmp");
        std::fs::write(&tmp, visible_lines.join("\n") + "\n")
            .map_err(|e| format!("mirrorcode {program}: write visible cases: {e}"))?;
        std::fs::rename(&tmp, &vis_path)
            .map_err(|e| format!("mirrorcode {program}: rename visible cases: {e}"))?;

        let has_docs = repo.join("mc").join(program).join("docs").is_dir();
        let task = to_eval_task(program, target_cmd, visible_lines.len(), hidden, &repo, has_docs);
        lines.push(
            serde_json::to_string(&task)
                .map_err(|e| format!("mirrorcode {program}: serialize: {e}"))?,
        );
        if let Some(cap) = limit {
            if lines.len() >= cap {
                break;
            }
        }
    }
    crate::cognition::gym::write_fetched_gym("mirrorcode.jsonl", &lines, &adapter_fingerprint())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gold(labels: &[&str]) -> serde_json::Value {
        serde_json::json!({
            "test_case": {
                "args": ["-x"],
                "env": {},
                "stdin": "in",
                "files": {},
                "name": "a_case",
                "labels": labels,
                "visible_duals": [],
            },
            "stdout": "out\n",
            "stderr": "",
            "returncode": 0,
        })
    }

    // what this catches: the grade-path wiring. dod must run the harness FROM
    // the repo root (the gold oracle is addressed relative to it) against HER
    // task dir; the prompt must name the same artifact, spec file and binary
    // the grade reads (the graded-against-a-path-she-was-never-told bug class);
    // the no-source integrity rule and the hidden-dual warning must be spoken.
    #[test]
    fn task_wiring_is_consistent_and_repo_rooted() {
        let repo = std::path::Path::new("/cache/mirrorcode-repo");
        let t = to_eval_task("choose", "choose", 127, 122, repo, true);
        assert_eq!(t.id, "mirrorcode-choose");
        assert_eq!(t.solution_file.as_deref(), Some("mirrorcode/choose/src/main.rs"));
        let dod = t.dod_shell.as_deref().unwrap();
        assert!(dod.starts_with("cd /cache/mirrorcode-repo &&"), "{dod}");
        assert!(dod.contains("mirrorcode_harness.py choose"), "{dod}");
        assert!(t.prompt.contains("mirrorcode/choose/src/main.rs"));
        assert!(t.prompt.contains("cases_visible.jsonl"));
        assert!(t.prompt.contains("249 recorded"), "total = visible + hidden must be spoken");
        assert!(t.prompt.contains("hidden duals"), "hardcoding deterrent must be declared");
        assert!(t.prompt.contains("voids this exam"), "the behavior-only integrity rule");
        let setup = t.setup_shell.as_deref().unwrap();
        assert!(setup.contains("cp /cache/mirrorcode-repo/visible_cases/choose.jsonl"));
        assert!(t.prompt.contains("/cache/mirrorcode-repo/mc/choose/docs/"));
        // and a docs-less program must not promise a manual that isn't there
        let bare = to_eval_task("rev", "rev", 156, 52, repo, false);
        assert!(!bare.prompt.contains("docs/"));
    }

    // what this catches: denominator corruption. A malformed gold row silently
    // dropped shrinks the case count the harness grades against; conversion
    // must refuse LOUD, naming program + row, exactly like the ds-1000 rule.
    #[test]
    fn a_malformed_gold_row_is_refused_with_its_address() {
        let mut v = gold(&[]);
        v.as_object_mut().unwrap().remove("stdout"); // test constructs the row; the key is present by construction
        let err = parse_gold_row("choose", 7, &v).unwrap_err();
        assert!(err.contains("choose row 7"), "{err}");
        assert!(err.contains("stdout"), "{err}");
        // and the 'error' file format (an upstream capture FAILURE) is refused,
        // never staged as if it were test data
        let mut v = gold(&[]);
        v["test_case"]["files"]["broken.txt"] =
            serde_json::json!({"format": "error", "content": "boom"});
        let err = parse_gold_row("choose", 8, &v).unwrap_err();
        assert!(err.contains("broken.txt"), "{err}");
        assert!(err.contains("capture failure"), "{err}");
    }

    // what this catches: the anti-hardcoding seam. Hidden/ablated rows must be
    // excluded from the agent-visible projection (or the duals deter nothing),
    // and the visible projection must strip the labels/visible_duals metadata
    // (upstream strips it for agents too) while KEEPING the expected outputs —
    // our declared stand-in for the reference binary.
    #[test]
    fn hidden_rows_stay_out_of_the_agent_view_and_labels_are_stripped() {
        assert!(parse_gold_row("cal", 0, &gold(&["hidden"])).unwrap().hidden);
        assert!(parse_gold_row("cal", 0, &gold(&["ablated"])).unwrap().hidden);
        let vis = parse_gold_row("cal", 0, &gold(&[])).unwrap();
        assert!(!vis.hidden);
        let v: serde_json::Value = serde_json::from_str(&vis.agent_view).unwrap(); // round-trips: we serialized it one line up
        assert!(v["test_case"].get("labels").is_none(), "labels must be stripped");
        assert!(v["test_case"].get("visible_duals").is_none());
        assert_eq!(v["expected"]["stdout"], "out\n");
        assert_eq!(v["expected"]["returncode"], 0);
        assert_eq!(v["test_case"]["args"][0], "-x");
    }

    // what this catches: the harness's own contract drifting from upstream's
    // scorer. The load-bearing disciplines: per-case 2s timeout, exit codes
    // mod 256, one-trailing-newline tolerance ONLY (never whitespace-blind),
    // a nonzero exit on ANY mismatch (100% bar), and dropping CARGO_TARGET_DIR
    // so the shared cache cannot swallow the graded binary. A one-byte stdout
    // diff must fail: the only equality escapes in permissive_equals are the
    // two single-trailing-newline branches, and a mismatch always routes to
    // sys.exit(1).
    #[test]
    fn harness_keeps_the_official_comparison_discipline() {
        assert!(HARNESS_PY.contains("PER_CASE_TIMEOUT = 2"), "upstream's own per-case cap");
        assert!(HARNESS_PY.contains("code % 256"), "signed/unsigned exit-code equivalence");
        assert!(HARNESS_PY.contains("permissive_equals"), "exact match + trailing-newline tolerance");
        assert!(
            !HARNESS_PY.contains("whitespace_insensitive"),
            "the verdict must never use upstream's whitespace-blind DIAGNOSTIC comparison"
        );
        assert!(HARNESS_PY.contains("sys.exit(1)"), "any mismatch exits nonzero");
        assert!(
            HARNESS_PY.contains("env.pop(\"CARGO_TARGET_DIR\", None)"),
            "the shared cargo cache would swallow target/release/<prog>"
        );
        assert!(
            HARNESS_PY.contains("100% of"),
            "the receipt must state the all-or-nothing bar"
        );
    }

    // what this catches: quoting corruption + the Rust-keyword collision. The
    // staged manifest is a PROGRAM travelling through the setup shell; the b64
    // round-trip must be byte-exact and its alphabet can never terminate the
    // single-quoted token. `false` is a real target program AND a Rust keyword:
    // the package name must be prefixed while the [[bin]] target keeps the
    // program name the harness executes.
    #[test]
    fn staged_manifest_round_trips_and_survives_the_false_program() {
        let t = to_eval_task("false", "false", 145, 0, std::path::Path::new("/c"), false);
        let setup = t.setup_shell.unwrap();
        let b64_toml = setup
            .split('\'')
            .nth(3)
            .expect("setup_shell embeds the manifest as the second single-quoted token");
        assert!(!b64_toml.contains('\''));
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(b64_toml)
            .expect("valid base64");
        let toml = String::from_utf8(decoded).unwrap(); // manifest is ASCII by construction
        assert_eq!(toml, cargo_manifest("false"));
        assert!(toml.contains("name = \"mirrorcode_false\""), "keyword-safe package name");
        assert!(toml.contains("name = \"false\""), "bin target keeps the program name");
    }
}
