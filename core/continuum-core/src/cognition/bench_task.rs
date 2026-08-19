//! One task shape every benchmark projects into — so an adapter carries ONLY the differences.
//!
//! # Why this exists (Joel, 2026-08-19)
//!
//! > *"Supporting more benchmarks makes your adapter design robust and correct. This will make
//! > the next benchmark so much easier. The adapter handles only the differences."*
//!
//! Before this, "adapter" meant one function fused to one row shape: [`super::swe_bench`] could
//! read `SweInstance` and nothing else, so twenty catalogued suites were names with URLs beside
//! them that nothing could read. Fixing that by adding a second bespoke loader would have bought
//! one suite and left the third to be written from scratch again.
//!
//! So the fetch trunk is shared ([`super::swe_bench::fetch_hf_rows`]) and the per-suite delta is
//! a [`SuiteAdapter`]: given a raw row, produce a [`BenchTask`]. Everything downstream — the
//! card, the room, the workspace, the grade — speaks `BenchTask` and never knows which suite it
//! came from.
//!
//! # The interface was validated against its two EXTREMES, not its average
//!
//! Per the outlier-validation rule in CLAUDE.md: build outlier A and outlier B, and if the
//! interface fits both *without forcing*, the middle is guaranteed. The two extremes here are
//! maximally far apart, and both are real rows measured off disk on 2026-08-19:
//!
//! | | outlier A: `swe-bench-lite` | outlier B: `cruxeval` |
//! |---|---|---|
//! | workspace | a real multi-thousand-file repo at a pinned commit | none |
//! | citizen writes | a unified diff against existing source | **no code at all** |
//! | oracle | apply a held-out test patch, run named pytest node-ids | exact string match |
//! | failure mode | a passing test regressed | wrong answer |
//!
//! A shape that carries both carries `evalplus` and `bigcodebench` (write one function, run a
//! test script) trivially — which is why those two landed as one adapter, not two.
//!
//! # Absence is never a default
//!
//! Every projection FAILS LOUD on a missing field rather than substituting an empty string.
//! A task with an empty `statement` is not a hard task, and a task with an empty oracle is not
//! an unresolvable one — both are *import bugs*, and defaulting them turns a broken import into
//! a capability zero. That is the same confusion [[an-absence-is-an-unfinished-measurement]]
//! names, moved one layer earlier.

use serde_json::Value;

/// What the citizen is asked to produce. Distinct from HOW it is scored ([`Oracle`]) because the
/// two vary independently: two suites can both want a written function and grade it completely
/// differently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Deliverable {
    /// Patch an existing repository at a pinned commit. The workspace IS that repo, and the
    /// deliverable is a diff against it.
    RepoPatch { repo: String, base_commit: String },
    /// Write code from a prompt into a fresh workspace. `preamble` is the given source the
    /// citizen completes (imports + signature + docstring); `entry_point` is the symbol the
    /// oracle will call.
    Program {
        entry_point: String,
        preamble: String,
    },
    /// Produce an answer in words. Nothing is written to any workspace — this is the reasoning
    /// tier, and it is the reason `Deliverable` is an enum rather than a workspace path.
    Answer,
}

/// The held-out scoring oracle. NEVER rendered into a citizen's prompt — it is the answer key,
/// and a suite whose oracle leaks into the statement is measuring recall, not capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Oracle {
    /// Apply a test patch, then run named tests. Resolved iff every `fail_to_pass` passes AND
    /// no `pass_to_pass` regresses — the second half is what makes a plausible-but-breaking
    /// patch score zero instead of one.
    RepoTests {
        test_patch: String,
        fail_to_pass: Vec<String>,
        pass_to_pass: Vec<String>,
    },
    /// Run a test program against the produced code.
    TestProgram { source: String },
    /// Compare against a known answer.
    ExactAnswer { expected: String },
}

/// One benchmark task, suite-agnostic. This is what a card, a room, and a grade all speak.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchTask {
    /// The suite's own id for this task, verbatim (`astropy__astropy-12907`, `HumanEval/0`,
    /// `sample_0`). Kept as the upstream string so a published result can be joined against
    /// anyone else's published result without a translation table.
    pub id: String,
    /// Which catalogued suite this came from.
    pub suite: String,
    /// What the citizen reads. The task as posed — never the answer.
    pub statement: String,
    pub deliverable: Deliverable,
    pub oracle: Oracle,
}

/// Projects one suite's raw rows into [`BenchTask`]s.
///
/// Polymorphism rather than a match arm (the OpenCV `cv::Algorithm` shape CLAUDE.md prescribes):
/// adding a suite is a new impl plus one registry row, and nothing that consumes tasks changes.
pub trait SuiteAdapter: Send + Sync {
    /// Which catalogued suite names this adapter serves. Declared BY the adapter so the
    /// registry never becomes a second place that has to know.
    fn serves(&self) -> &'static [&'static str];
    /// Project one raw row. `Err` names the missing field — an unprojectable row is a loud
    /// import failure, never a silently-skipped task that shrinks the denominator.
    fn project(&self, suite: &str, row: &Value) -> Result<BenchTask, String>;
}

/// Read a required string field, or say exactly which one was missing and on what.
fn req_str(row: &Value, field: &str, id_hint: &str) -> Result<String, String> {
    row.get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("row `{id_hint}` has no string field `{field}`"))
}

/// `FAIL_TO_PASS`/`PASS_TO_PASS` arrive as a JSON-encoded array INSIDE a string — a real quirk
/// of the SWE-bench rows, not a guess (verified against the staged rows 2026-08-19). Accept the
/// native array too, so a re-export that fixes the quirk upstream does not break the import.
fn test_name_list(row: &Value, field: &str, id_hint: &str) -> Result<Vec<String>, String> {
    let raw = row
        .get(field)
        .ok_or_else(|| format!("row `{id_hint}` has no field `{field}`"))?;
    let arr = match raw {
        Value::Array(a) => a.clone(),
        Value::String(s) => serde_json::from_str::<Vec<String>>(s)
            .map_err(|e| format!("row `{id_hint}` field `{field}` is not a JSON array: {e}"))?
            .into_iter()
            .map(Value::String)
            .collect(),
        other => {
            return Err(format!(
                "row `{id_hint}` field `{field}` is {other:?}, expected an array or an \
                 array-encoding string"
            ))
        }
    };
    Ok(arr
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect())
}

/// OUTLIER A — repo-scale work: a real project at a pinned commit, graded by held-out pytest.
pub struct SweAdapter;

impl SuiteAdapter for SweAdapter {
    fn serves(&self) -> &'static [&'static str] {
        &["swe-bench-lite", "swe-bench-verified"]
    }

    fn project(&self, suite: &str, row: &Value) -> Result<BenchTask, String> {
        let id = req_str(row, "instance_id", "<unidentified swe row>")?;
        let fail_to_pass = test_name_list(row, "FAIL_TO_PASS", &id)?;
        if fail_to_pass.is_empty() {
            // A SWE instance with no failing test has no way to be resolved — importing it
            // would add a task that can only ever score zero and drag the denominator.
            return Err(format!("row `{id}` has an empty FAIL_TO_PASS — nothing to resolve"));
        }
        Ok(BenchTask {
            statement: req_str(row, "problem_statement", &id)?,
            deliverable: Deliverable::RepoPatch {
                repo: req_str(row, "repo", &id)?,
                base_commit: req_str(row, "base_commit", &id)?,
            },
            oracle: Oracle::RepoTests {
                test_patch: req_str(row, "test_patch", &id)?,
                fail_to_pass,
                pass_to_pass: test_name_list(row, "PASS_TO_PASS", &id)?,
            },
            id,
            suite: suite.to_string(),
        })
    }
}

/// The middle tier — write one function, run a test script against it. Two suites, ONE adapter,
/// because the only thing that differs between them is which field holds the prompt.
pub struct ProgramAdapter;

impl SuiteAdapter for ProgramAdapter {
    fn serves(&self) -> &'static [&'static str] {
        &["evalplus", "bigcodebench"]
    }

    fn project(&self, suite: &str, row: &Value) -> Result<BenchTask, String> {
        let id = req_str(row, "task_id", "<unidentified program row>")?;
        // bigcodebench ships BOTH a natural-language `instruct_prompt` and a code-completion
        // `complete_prompt`; evalplus ships only `prompt`. Prefer the instruction form — it is
        // the one that measures whether a citizen can work from a described task rather than
        // from an already-half-written function, which is what our citizens actually do.
        let statement = req_str(row, "instruct_prompt", &id)
            .or_else(|_| req_str(row, "prompt", &id))
            .map_err(|_| {
                format!("row `{id}` has none of `instruct_prompt` / `prompt` to pose the task")
            })?;
        // The code the citizen starts from. bigcodebench names it `complete_prompt`; evalplus
        // reuses `prompt` for both roles.
        let preamble = req_str(row, "complete_prompt", &id)
            .or_else(|_| req_str(row, "prompt", &id))
            .unwrap_or_else(|_| statement.clone());
        Ok(BenchTask {
            statement,
            deliverable: Deliverable::Program {
                entry_point: req_str(row, "entry_point", &id)?,
                preamble,
            },
            oracle: Oracle::TestProgram {
                source: req_str(row, "test", &id)?,
            },
            id,
            suite: suite.to_string(),
        })
    }
}

/// OUTLIER B — the reasoning tier: NO workspace, NO code written, exact-match graded. This is
/// the shape that proves the interface, because forcing it into a "write a file" contract would
/// have required a fake workspace and a fake deliverable.
pub struct ExecutionReasoningAdapter;

impl SuiteAdapter for ExecutionReasoningAdapter {
    fn serves(&self) -> &'static [&'static str] {
        &["cruxeval"]
    }

    fn project(&self, suite: &str, row: &Value) -> Result<BenchTask, String> {
        let id = req_str(row, "id", "<unidentified cruxeval row>")?;
        let code = req_str(row, "code", &id)?;
        let input = req_str(row, "input", &id)?;
        Ok(BenchTask {
            // cruxeval rows carry no prose — the task is posed by the harness, not the dataset.
            // Composing it HERE (rather than at a call site) is precisely the "adapter handles
            // only the differences" line: downstream still just reads `statement`.
            statement: format!(
                "Given the Python function below, determine the exact output of `f({input})`. \
                 Reason about what the code does, then state the output.\n\n{code}"
            ),
            deliverable: Deliverable::Answer,
            oracle: Oracle::ExactAnswer {
                expected: req_str(row, "output", &id)?,
            },
            id,
            suite: suite.to_string(),
        })
    }
}

/// Every adapter in the tree. Adding a suite family = one impl + one row here.
fn adapters() -> Vec<Box<dyn SuiteAdapter>> {
    vec![
        Box::new(SweAdapter),
        Box::new(ProgramAdapter),
        Box::new(ExecutionReasoningAdapter),
    ]
}

/// Project a whole suite's rows into tasks.
///
/// Refuses rather than guesses in both directions: an unadapted suite says so by name, and a row
/// that will not project aborts the import instead of shrinking the task list silently. A
/// benchmark whose denominator quietly depends on how many rows happened to parse is not
/// comparable to anyone's published number.
pub fn project_suite(suite: &str, rows: &[Value]) -> Result<Vec<BenchTask>, String> {
    let all = adapters();
    let adapter = all
        .iter()
        .find(|a| a.serves().contains(&suite))
        .ok_or_else(|| {
            let known: Vec<&str> = all.iter().flat_map(|a| a.serves().iter().copied()).collect();
            format!(
                "`{suite}` has no SuiteAdapter — its rows can be fetched but not posed as tasks. \
                 Adapted suites: {}. Adding one is an impl of SuiteAdapter plus a registry row.",
                known.join(", ")
            )
        })?;
    rows.iter()
        .map(|r| adapter.project(suite, r))
        .collect::<Result<Vec<_>, _>>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Rows in the EXACT shapes measured off the staged caches on 2026-08-19 — not invented
    /// shapes, which would let the adapters pass against a fiction.
    fn swe_row() -> Value {
        json!({
            "instance_id": "astropy__astropy-12907",
            "repo": "astropy/astropy",
            "base_commit": "d16bfe05a744909de4b27f5875fe0d4ed41ce607",
            "problem_statement": "separability_matrix does not compute separability correctly",
            "patch": "diff --git a/astropy/modeling/separable.py ...",
            // NOTE: a STRING holding a JSON array — the real quirk, pinned below.
            "test_patch": "diff --git a/astropy/modeling/tests/test_separable.py ...",
            "FAIL_TO_PASS": "[\"astropy/modeling/tests/test_separable.py::test_separable\"]",
            "PASS_TO_PASS": "[\"astropy/modeling/tests/test_separable.py::test_coord_matrix\"]",
        })
    }

    /// The answer is deliberately DIFFERENT from the input here. An identity-function fixture
    /// (which the real `sample_0` nearly is) cannot detect an oracle leak at all — the answer
    /// string is already legitimately present as the input.
    fn crux_row() -> Value {
        json!({
            "id": "sample_0",
            "code": "def f(nums):\n    return len(nums)",
            "input": "[1, 1, 3]",
            "output": "3",
        })
    }

    /// what this catches: the interface silently failing its own outlier test. A shape that fits
    /// repo-patching but forces the reasoning tier into a fake workspace would still COMPILE and
    /// still pass a SWE-only test — this asserts both extremes project cleanly and land on
    /// genuinely different deliverables and oracles, which is the whole claim of the design.
    #[test]
    fn the_two_extremes_both_project_without_forcing_either_one() {
        let swe = SweAdapter.project("swe-bench-lite", &swe_row()).unwrap();
        assert_eq!(swe.id, "astropy__astropy-12907");
        assert!(matches!(swe.deliverable, Deliverable::RepoPatch { .. }));
        let Oracle::RepoTests {
            fail_to_pass,
            pass_to_pass,
            ..
        } = &swe.oracle
        else {
            panic!("a SWE task is graded by repo tests");
        };
        assert_eq!(fail_to_pass.len(), 1, "the array-in-a-string must decode");
        assert_eq!(pass_to_pass.len(), 1, "and regressions must be carried too");

        let crux = ExecutionReasoningAdapter
            .project("cruxeval", &crux_row())
            .unwrap();
        assert_eq!(crux.deliverable, Deliverable::Answer, "NO code is written");
        assert_eq!(
            crux.oracle,
            Oracle::ExactAnswer {
                expected: "3".to_string()
            }
        );
        // The statement must pose the task even though the dataset carries no prose at all.
        assert!(crux.statement.contains("def f(nums)") && crux.statement.contains("f([1, 1, 3])"));
        // And the ANSWER must never appear in what the citizen reads — a statement built by
        // string-formatting every field of the row would measure reading, not reasoning.
        let Oracle::ExactAnswer { expected } = &crux.oracle else {
            unreachable!()
        };
        assert!(
            !crux.statement.contains(&format!("= {expected}"))
                && !crux.statement.contains(&format!("is {expected}")),
            "the oracle leaked into the statement: {}",
            crux.statement
        );
    }

    /// what this catches: the single most expensive failure this module can have — an import
    /// that drops rows it cannot read. A suite that silently projects 280 of 300 rows produces a
    /// pass RATE over a denominator nobody chose, which is not comparable to any published
    /// number and reads exactly like a real score.
    #[test]
    fn an_unreadable_row_aborts_the_import_instead_of_shrinking_the_suite() {
        let mut broken = swe_row();
        broken.as_object_mut().unwrap().remove("problem_statement");
        let err = project_suite("swe-bench-lite", &[swe_row(), broken]).unwrap_err();
        assert!(
            err.contains("problem_statement") && err.contains("astropy__astropy-12907"),
            "the refusal must name the field AND the row: {err}"
        );

        // Same rule for a SWE instance with nothing to resolve.
        let mut no_target = swe_row();
        no_target.as_object_mut().unwrap()["FAIL_TO_PASS"] = json!("[]");
        let err = project_suite("swe-bench-lite", &[no_target]).unwrap_err();
        assert!(err.contains("FAIL_TO_PASS"), "{err}");
    }

    /// what this catches: a fetched-but-unadapted suite looking runnable. `benchmark/fetch` can
    /// stage rows for any HF suite; that is NOT the same as being able to pose them. The refusal
    /// has to say which suites ARE adapted, or the caller cannot tell a typo from a gap.
    #[test]
    fn a_fetchable_but_unadapted_suite_says_so_and_lists_what_is_adapted() {
        let err = project_suite("apps", &[]).unwrap_err();
        assert!(err.contains("no SuiteAdapter"), "{err}");
        assert!(
            err.contains("swe-bench-lite") && err.contains("cruxeval"),
            "the refusal must enumerate the adapted suites: {err}"
        );
    }

    /// what this catches: the two program suites drifting into two adapters. They differ ONLY in
    /// which field poses the task, and bigcodebench's `instruct_prompt` must win over its
    /// `complete_prompt` — otherwise the citizen is handed a half-written function and we
    /// measure completion instead of the described task.
    #[test]
    fn both_program_suites_share_one_adapter_and_prefer_the_instruction_form() {
        let evalplus = json!({
            "task_id": "HumanEval/0", "entry_point": "has_close_elements",
            "prompt": "from typing import List\n\ndef has_close_elements(...):",
            "test": "def check(candidate): ...",
        });
        let big = json!({
            "task_id": "BigCodeBench/0", "entry_point": "task_func",
            "instruct_prompt": "Calculates the average of the sums of absolute differences.",
            "complete_prompt": "import itertools\ndef task_func(...):",
            "test": "import unittest ...",
        });
        let tasks = project_suite("evalplus", &[evalplus])
            .unwrap()
            .into_iter()
            .chain(project_suite("bigcodebench", &[big]).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(tasks.len(), 2);
        assert!(tasks[0].statement.contains("has_close_elements"));
        assert_eq!(
            tasks[1].statement,
            "Calculates the average of the sums of absolute differences.",
            "the instruction form must pose the task, not the code-completion form"
        );
        let Deliverable::Program { preamble, .. } = &tasks[1].deliverable else {
            panic!("a program task carries the source the citizen starts from");
        };
        assert!(
            preamble.contains("import itertools"),
            "and the completion form survives as the PREAMBLE: {preamble}"
        );
        for t in &tasks {
            assert!(matches!(t.oracle, Oracle::TestProgram { .. }));
        }
    }
}
