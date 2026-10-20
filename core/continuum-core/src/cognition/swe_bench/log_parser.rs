use super::*;

/// A named log parser from SWE-bench Multilingual's `log_parser` column. Each
/// variant reads ONE test framework's output into (test id → passed). The 19
/// names in the dataset map to languages below; the two that run here are the
/// Rust and Go slices (85 of 300 instances) — the rest refuse BY NAME until their
/// parser lands, never by a silent zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LogParser {
    Cargo,
    GoTest,
}

impl LogParser {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "parse_log_cargo" => Some(Self::Cargo),
            "parse_log_gotest" => Some(Self::GoTest),
            _ => None,
        }
    }

    /// The toolchain binary the marked command needs on PATH.
    pub fn toolchain(self) -> &'static str {
        match self {
            Self::Cargo => "cargo",
            Self::GoTest => "go",
        }
    }

    pub fn parse(self, log: &str) -> HashMap<String, bool> {
        match self {
            Self::Cargo => parse_log_cargo(log),
            Self::GoTest => parse_log_gotest(log),
        }
    }
}

/// The language a parser name implies — the `language` filter on `benchmark/import`
/// reads this, so a round asks for `rust` without knowing parser names. Names this
/// grader does not run yet still map: the filter is catalogue knowledge, and the
/// grader refuses those instances by name.
pub fn language_of_parser(name: &str) -> &'static str {
    match name {
        "parse_log_cargo" => "rust",
        "parse_log_gotest" => "go",
        "parse_log_phpunit" => "php",
        "parse_log_rspec_transformed_json" | "parse_log_ruby_unit" | "parse_log_jekyll" => "ruby",
        "parse_log_karma" | "parse_log_jest" | "parse_log_tap" | "parse_log_vitest"
        | "parse_log_immutable_js" => "javascript",
        "parse_log_ant" | "parse_log_maven" | "parse_log_gradle_custom" => "java",
        "parse_log_googletest" | "parse_log_doctest" | "parse_log_redis" | "parse_log_jq"
        | "parse_log_micropython_test" => "c",
        _ => "unknown",
    }
}

impl SweInstance {
    /// `python` for the SWE-bench family; the parser's language for Multilingual.
    pub fn language(&self) -> &'static str {
        self.log_parser.as_deref().map_or("python", language_of_parser)
    }
}

/// The ONE test command inside an eval script: the line between the
/// `>>>>> Start Test Output` and `>>>>> End Test Output` markers, minus the
/// `( … ) | cat` wrapper the docker harness adds. Everything else the script does
/// (checkout at base, test-patch apply, restore) `grade()` already does itself.
pub fn test_command_from_eval_script(script: &str) -> Option<String> {
    let start = script.find(">>>>> Start Test Output")?;
    let end = script[start..].find(">>>>> End Test Output")? + start;
    let line = script[start..end]
        .lines()
        .skip(1)
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with(':'))?;
    let line = line.strip_suffix("| cat").map(str::trim).unwrap_or(line); // unwrap_or: no `| cat` wrapper means the line IS the command
    let line = line
        .strip_prefix('(')
        .and_then(|l| l.strip_suffix(')'))
        .unwrap_or(line); // unwrap_or: an unparenthesised command is already bare
    Some(line.trim().to_string())
}

/// How an instance's tests run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Harness {
    /// Our venv + the repo era's runner (the Python SWE-bench family).
    Native { venv_py: PathBuf, runner: TestRunner },
    /// The instance's own marked command and named parser (SWE-bench Multilingual).
    Script { command: String, parser: LogParser },
}

impl Harness {
    /// Script when the instance carries a harness, Native otherwise. A script
    /// harness this grader cannot read refuses by parser name and language.
    pub fn for_instance(
        instance: &SweInstance,
        venv_py: PathBuf,
        runner: TestRunner,
    ) -> Result<Self, String> {
        let Some(script) = instance.eval_script.as_deref() else {
            return Ok(Self::Native { venv_py, runner });
        };
        let name = instance.log_parser.as_deref().unwrap_or(""); // unwrap_or: a script with no parser name refuses below as unsupported
        let parser = LogParser::from_name(name).ok_or_else(|| unsupported_harness(instance, name))?;
        let command = test_command_from_eval_script(script).ok_or_else(|| {
            format!(
                "MALFORMED HARNESS — {}'s eval script has no marked test command",
                instance.instance_id
            )
        })?;
        Ok(Self::Script { command, parser })
    }
}

pub(super) fn unsupported_harness(instance: &SweInstance, name: &str) -> String {
    format!(
        "UNSUPPORTED HARNESS — {} ships log parser '{name}' ({}); this grader reads \
         parse_log_cargo (rust) and parse_log_gotest (go) so far",
        instance.instance_id,
        language_of_parser(name)
    )
}

/// `run_tests` for either harness. A script harness runs the marked command in the
/// checkout (bounded by `SUBPROCESS_CEILING`) and reads stdout+stderr with the
/// named parser; an id the log never names reads as failed.
pub async fn run_harness(
    repo_dir: &Path,
    harness: &Harness,
    ids: &[String],
    test_files: &[String],
) -> (HashMap<String, bool>, String) {
    match harness {
        Harness::Native { venv_py, runner } => {
            run_tests(repo_dir, venv_py, ids, test_files, *runner).await
        }
        Harness::Script { command, parser } => {
            if ids.is_empty() {
                return (HashMap::new(), String::new());
            }
            let Ok(out) = run("bash", &["-c", command.as_str()], Some(repo_dir)).await else {
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
            let seen = parser.parse(&report);
            (
                ids.iter()
                    .map(|i| (i.clone(), seen.get(i).copied().unwrap_or(false))) // unwrap_or: an id the log never names did not pass
                    .collect(),
                report,
            )
        }
    }
}

/// `test <name> ... ok|FAILED|ignored` — cargo's libtest lines. A `- should panic`
/// suffix is the name's display, not its id; `ignored` is neither pass nor fail.
pub fn parse_log_cargo(log: &str) -> HashMap<String, bool> {
    let mut out = HashMap::new();
    for line in log.lines() {
        let Some(rest) = line.trim_start().strip_prefix("test ") else {
            continue;
        };
        let Some((name, status)) = rest.rsplit_once(" ... ") else {
            continue;
        };
        let name = name.split(" - ").next().unwrap_or(name).trim(); // unwrap_or: split always yields a head; the name itself
        let status = status.trim();
        let passed = status.starts_with("ok");
        if passed || status.starts_with("FAILED") {
            out.insert(name.to_string(), passed);
        }
    }
    out
}

/// `--- PASS: TestName (0.01s)` / `--- FAIL: TestName` — go test's verbose lines,
/// subtests included (`TestA/sub`).
pub fn parse_log_gotest(log: &str) -> HashMap<String, bool> {
    let mut out = HashMap::new();
    for line in log.lines() {
        let l = line.trim_start();
        let (passed, rest) = if let Some(r) = l.strip_prefix("--- PASS: ") {
            (true, r)
        } else if let Some(r) = l.strip_prefix("--- FAIL: ") {
            (false, r)
        } else {
            continue;
        };
        if let Some(name) = rest.split_whitespace().next() {
            out.insert(name.to_string(), passed);
        }
    }
    out
}

/// A script-harness instance has no venv: its "env" is the language toolchain the
/// marked command needs. Absent = COULD_NOT_LOOK by name, never a python venv.
pub(super) async fn toolchain_on_path(tool: &str) -> Result<PathBuf, String> {
    match run("which", &[tool], None).await {
        Ok(out) if out.status.success() => {
            Ok(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
        }
        _ => Err(format!(
            "TOOLCHAIN ABSENT — `{tool}` is not on PATH on this node; install it or route \
             this card to a node that has it"
        )),
    }
}

/// The exact argv (after the venv's `python`) that runs an instance's test scope.
/// Pure so the per-repo command shape is table-testable without a venv.
/// The module name of the JSON runner dropped into django's `tests/` directory.
/// `tests/` is on `sys.path` when runtests.py runs, so a bare module name resolves.
pub(super) const DJANGO_JSON_RUNNER_MODULE: &str = "continuum_json_runner";

/// The settings module that SELECTS the JSON runner — the only seam django offers.
///
/// `runtests.py` has **no `--testrunner` flag in any era**; it reads
/// `settings.TEST_RUNNER` and defaults it only when unset (verified in-tree at 1.11,
/// 2.2, 3.2, 4.2, 5.2 and main — same three lines throughout). Passing a flag it does
/// not know is not a no-op: argparse rejects the whole invocation before a single test
/// runs, which is exactly what happened live on django-10914 (`unrecognized arguments:
/// --testrunner=…`, suite 0/40 in 4s). So the runner is selected by a settings module
/// that re-exports django's own `test_sqlite` and overrides that one key.
pub(super) const DJANGO_JSON_SETTINGS_MODULE: &str = "continuum_json_settings";

/// Settings for [`DJANGO_JSON_SETTINGS_MODULE`]. A pure ADDITION to the clone: django's
/// own `test_sqlite` stays untouched and supplies every database/hasher setting, so this
/// shim cannot drift from whatever the era's suite settings happen to be.
pub(super) const DJANGO_JSON_SETTINGS_SRC: &str = r#"# Written by continuum's SWE-bench grader. Not part of django.
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
pub(super) const DJANGO_JSON_RUNNER_SRC: &str = r#"# Written by continuum's SWE-bench grader. Not part of django.
import json, sys, unittest
from django.test.runner import DiscoverRunner

class _ContinuumResult(unittest.TextTestResult):
    def __init__(self, *a, **kw):
        super().__init__(*a, **kw)
        self.continuum_rows = {}
        self.continuum_skipped = set()
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
        super().addSkip(test, reason); self._record(test, True); self.continuum_skipped.add(test.id())
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
            if tid in getattr(result, "continuum_skipped", set()):
                row["skipped"] = True
            if desc:
                row["desc"] = desc
            sys.stderr.write("CONTINUUM_TEST " + json.dumps(row) + "\n")
        sys.stderr.flush()
        return result
"#;

/// Drop the JSON runner AND the settings module that selects it into the clone's `tests/`
/// dir. Returns whether both are usable — either alone does nothing, so this is one unit.
/// Idempotent — grading re-runs over the same clone just overwrite them.
pub(super) async fn install_django_json_runner(repo_dir: &Path) -> bool {
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
pub(super) fn django_directive(file: &str) -> String {
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
pub(super) fn django_outcome(outcome: &str) -> Option<bool> {
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
        let Some((node, verdict)) = split_node_outcome(line) else {
            continue;
        };
        if !node.contains("::") {
            continue;
        }
        // XPASS is a test that RAN AND PASSED under an xfail marker whose condition this
        // env does not meet (xarray-4356, card 27a5d94d: `test_datetime_mean[False]/[True]`
        // are xfail-marked against a pandas version the era env does not have, pass when
        // run, and were graded as two P2P misses). The official harness never emits XPASS
        // in the env that minted the P2P list — there the test simply PASSED — so the
        // faithful reading of "it passed here too" is a pass. A STRICT xfail that passes is
        // reported by pytest as FAILED, so strictness is never lost by this mapping.
        let ok = if verdict.starts_with("PASSED")
            || verdict.starts_with("XFAIL")
            || verdict.starts_with("XPASS")
            || verdict.starts_with("SKIPPED")
        {
            true
        } else if verdict.starts_with("FAILED") || verdict.starts_with("ERROR") {
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

/// The six outcome words a pytest `-v` line can carry after its node id.
pub(super) const PYTEST_OUTCOMES: [&str; 6] = ["PASSED", "FAILED", "ERROR", "XFAIL", "XPASS", "SKIPPED"];
