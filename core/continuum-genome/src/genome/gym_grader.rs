//! Objective test-grading for the coder gym — shared by the eval harness
//! (`cognition/eval`) and the teacher-episode generator (`genome/teach`).
//!
//! The grade is the language's OWN verdict: compile the model's Rust with `rustc`,
//! drive it from a `main` built from the task's test, run it — pass = exit 0. Not
//! substring-on-prose, not Python. One grader, one source of truth: both the A/B
//! evaluator and the trajectory generator must grade identically, or a gene that
//! "passes" generation could "fail" eval (and vice versa) for grader-shape reasons
//! alone. Extracted here so that can never drift.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use continuum_kernel::system_resources::disk_reporters::ClassWriter;
use uuid::Uuid;

/// Where a grade's scratch source and binary live: the measured, evicted temp class
/// (`disk_reporters::standard`), one owner directory per grade, removed after it.
const SCRATCH_CLASS: &str = "eval-roots";

/// How far a graded attempt got. `Refused` means the grader could not grade at all (an
/// unsupported language, a test in a format that would false-pass, no candidate file, no scratch
/// class); it says nothing about the candidate, so it is never compared as progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradeStage {
    Refused,
    Compile,
    Run,
    Pass,
}

/// One grade, typed: what the teacher's fix loop measures progress by, read from the grader's
/// own result and never by scanning its text (Codex and Fable on card a4bbae0a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grade {
    pub stage: GradeStage,
    /// The failure set by identity, never raw text, so timings and temp paths cannot make one
    /// failure look like another: a compile error as `code: message` (its location shifts as the
    /// candidate changes), a run failure as the test line that panicked or the solution's panic
    /// message. Empty on a pass.
    pub failures: BTreeSet<String>,
    /// For a run failure inside the test, which test line panicked (1-based): a later line means
    /// the assertions before it passed. `None` otherwise.
    pub test_line: Option<u32>,
    /// The whole text the teacher reads: every rendered compiler diagnostic, or the run's whole
    /// stderr. Never cut.
    pub text: String,
}

impl Grade {
    pub fn passed(&self) -> bool {
        self.stage == GradeStage::Pass
    }

    /// Pass and the whole text, for a caller that reports a verdict and measures nothing.
    pub fn verdict(self) -> (bool, String) {
        (self.passed(), self.text)
    }

    fn refused(text: impl Into<String>) -> Self {
        let text = text.into();
        Grade { stage: GradeStage::Refused, failures: BTreeSet::from([text.clone()]), test_line: None, text }
    }
}

/// Per-step grade timeouts. A step that overruns is SIGKILLed on drop.
///
/// These are GARBAGE CEILINGS, not performance targets (stopwatch-test doctrine):
/// grading shares the machine with a 35B decode, so a flat tight budget converts
/// scheduler contention into a false FAIL against the candidate — the one verdict
/// the grader must never invent. Compile gets minutes because `rustc` on a busy
/// box is legitimately slow and a compile CANNOT hang forever on its own; the run
/// step stays an order of magnitude tighter because it is the infinite-loop guard
/// — gym tests assert in microseconds, so anything near this ceiling IS a hang.
const COMPILE_TIMEOUT_SECS: u64 = 120;
const RUN_TIMEOUT_SECS: u64 = 30;

/// Extract the model's code from a response for test-grading.
///
/// A persona commonly authors ONE program across SEVERAL fences — "First the
/// imports: ```rust\nuse std::fs;\n``` then the logic: ```rust\nfn solve(){ fs::…
/// }\n```". Grading only the FIRST fence (the old behavior) dropped the rest of her
/// real code — glass-boxed 2026-07-14: `error[E0432]: fs not found` on the eval
/// while `use std::fs;` sat in a sibling fence, failing tasks the model actually
/// got right (system tax, not model weakness). So: concatenate ALL Rust-tagged
/// fences in order — faithful (only her code, all of it), never fabricated. When
/// NO fence is Rust-tagged, fall back to the first fenced block (the original
/// forgiving heuristic for models that fence inconsistently), then to the whole text.
pub fn extract_code_block(answer: &str) -> String {
    let blocks = fenced_blocks(answer);
    if blocks.is_empty() {
        return answer.trim().to_string();
    }
    let rust: Vec<&str> = blocks
        .iter()
        .filter(|(lang, _)| lang == "rust" || lang == "rs")
        .map(|(_, code)| code.as_str())
        .collect();
    if !rust.is_empty() {
        return rust.join("\n\n");
    }
    // No Rust-tagged fence — a text/output/other fence, or an untagged block. Keep
    // the original single-fence behavior rather than risk concatenating a non-code
    // fence (expected-output samples, shell transcripts) into the compiled unit.
    blocks[0].1.clone()
}

/// Parse every ```lang … ``` fence in order, returning `(lowercased lang tag, trimmed
/// body)` pairs. An unterminated final fence takes the rest of the text as its body
/// (a model that forgot the closing fence still gets graded on what it wrote).
fn fenced_blocks(answer: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = answer;
    while let Some(start) = rest.find("```") {
        let after = &rest[start + 3..];
        let (lang, body) = match after.find('\n') {
            Some(i) => (after[..i].trim().to_ascii_lowercase(), &after[i + 1..]),
            None => (String::new(), after),
        };
        match body.find("```") {
            Some(end) => {
                out.push((lang, body[..end].trim().to_string()));
                rest = &body[end + 3..];
            }
            None => {
                out.push((lang, body.trim().to_string()));
                break;
            }
        }
    }
    out
}

/// Strip a top-level `fn main() { … }` from candidate code, returning the code
/// with that definition removed. The gym drives the candidate from ITS OWN `main`
/// built from the task's authoritative `test` (see [`grade_rust`]); a candidate that
/// ALSO defines a `main` — which is exactly what we coach the persona to do when we
/// say "verify it works before answering" — would otherwise collide with the
/// grader's wrapper (`error[E0428]: the name 'main' is defined multiple times`).
/// We keep the grader's test-driven `main` authoritative and drop the candidate's
/// demo `main`, brace-matched so a `{`/`}` inside the body doesn't cut it short.
///
/// Only a MODULE-LEVEL `fn main` is stripped (the candidate's items live at module
/// scope). A `main` nested inside another fn is left alone — it's not a collision.
fn strip_top_level_main(code: &str) -> String {
    let bytes = code.as_bytes();
    let mut search_from = 0usize;
    while let Some(rel) = code[search_from..].find("fn main") {
        let kw = search_from + rel;
        // Must be at module level (start of a line, ignoring indentation) so we
        // don't strip a `fn main` nested inside another function body.
        let line_start = code[..kw].rfind('\n').map_or(0, |i| i + 1);
        let indent_is_blank = code[line_start..kw].chars().all(|c| c == ' ' || c == '\t');
        // After `fn main` expect `(` `)` then `{` (allowing whitespace between).
        let after = &code[kw + "fn main".len()..];
        let trimmed = after.trim_start();
        if indent_is_blank && trimmed.starts_with("()") {
            let after_parens = trimmed["()".len()..].trim_start();
            if let Some(open_off) = after_parens.find('{') {
                // Absolute index of the opening brace.
                let consumed = (after.len() - trimmed.len())
                    + "()".len()
                    + (trimmed["()".len()..].len() - after_parens.len())
                    + open_off;
                let brace_open = kw + "fn main".len() + consumed;
                if let Some(brace_close) = matching_brace(bytes, brace_open) {
                    let mut out = String::with_capacity(code.len());
                    out.push_str(&code[..line_start]);
                    out.push_str(&code[brace_close + 1..]);
                    return out.trim().to_string();
                }
            }
        }
        search_from = kw + "fn main".len();
    }
    code.to_string()
}

/// Index of the `}` that closes the `{` at `open`, or `None` if unbalanced. Naive
/// brace counter (ignores braces inside strings/comments) — sufficient for the gym
/// floor: candidate `main`s are short demos, not string-literal-heavy code.
fn matching_brace(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// TEST-GRADE a coder task: take the model's Rust code, drive it from a `main`
/// built from the task's test, COMPILE + RUN it — pass = exit 0. The gym's
/// objective grade in the language the persona actually ships: not
/// substring-on-prose, and not Python. Returns `(passed, grade_message)` where on
/// failure the message carries the first failing step's compiler/panic output so
/// the failure is diagnosable — and so a teacher can read the REAL error and fix.
///
/// SAFETY: compiles and runs model-generated code in a temp dir, each step under
/// its garbage-ceiling timeout, owned so a runaway and what it started are ended, never orphaned. That is
/// the pragmatic floor for an OWNER's local dev machine (what coding agents do); it
/// is NOT a sandbox. Before public/untrusted tasks, this MUST run in a real sandbox
/// (container/seccomp). Slice 1 = prove the grading mechanism; sandbox is a P1 req.
/// The typed grade of `answer`'s code against `test` (the teacher's fix loop measures progress by it).
pub async fn grade(answer: &str, lang: &str, test: &str) -> Grade {
    let code = extract_code_block(answer);
    // Rust only — the persona ships Rust, so the gym grades Rust. Anything else
    // fails LOUD with a named reason, never a silent pass.
    match lang {
        "rust" | "rs" => {}
        other => return Grade::refused(format!("unsupported lang '{other}' (Rust gym: lang must be 'rust')")),
    }
    grade_in_scratch(&code, test).await
}

/// [`grade`]'s verdict: pass and the whole text, for a caller that measures nothing.
pub async fn test_grade(answer: &str, lang: &str, test: &str) -> (bool, String) {
    grade(answer, lang, test).await.verdict()
}

/// ARTIFACT grade: read her acted solution from an in-workspace file (her HANDS) and run the
/// SAME harness as [`test_grade`] (strip her `main`, append the task's test, compile, run) — the
/// only difference is the code source: the file she wrote + compiled, not a block extracted from
/// her spoken answer. This is the honest measure of an ACTING persona; a spoken-text grade is
/// blind to code she put in a file. `rel_path` resolves against the core cwd (the workspace root
/// `code/write` sandboxes to). Fails LOUD (never a silent pass) if she wrote nothing, the path
/// escapes the workspace, or the file is empty. The file is removed after grading so a stale
/// artifact from this or a prior task can never false-pass a later one.
pub async fn grade_file(
    root: Option<&std::path::Path>,
    rel_path: &str,
    lang: &str,
    test: &str,
) -> Grade {
    match lang {
        "rust" | "rs" => {}
        other => return Grade::refused(format!("unsupported lang '{other}' (Rust gym: lang must be 'rust')")),
    }
    // Resolve against the RUN'S ROOT, never the process CWD. The grader read
    // bare `rel_path` from the core's cwd (the repo checkout) while her hands
    // wrote into the #312 ephemeral clone — so every artifact-graded task
    // reported "she never wrote it" about a file she demonstrably wrote
    // (glass-boxed 2026-08-23, take 2 of the Ornith battery: sol_roman_to_int.rs
    // correct IN the clone, grade said not-found). `None` = cwd-relative, for
    // callers that genuinely run in the target directory (tests).
    if std::path::Path::new(rel_path).is_absolute() || rel_path.contains("..") {
        return Grade::refused(format!("solution_file must be a relative in-workspace path, got '{rel_path}'"));
    }
    let path = match root {
        Some(r) => r.join(rel_path),
        None => std::path::PathBuf::from(rel_path),
    };
    let code = match std::fs::read_to_string(&path) {
        Ok(c) if !c.trim().is_empty() => c,
        Ok(_) => {
            let _ = std::fs::remove_file(path);
            return Grade::refused(format!("solution_file '{rel_path}' is empty — she wrote no code"));
        }
        Err(_) => {
            return Grade::refused(
                // State only what THIS function observed: the file is absent. The old text
                // asserted "(acts=0 on this task)" — a fact the grader does not have and which
                // is routinely FALSE: the first live run under the hands-only grade spent 8 acts
                // and still wrote nothing. "0 acts" and "8 acts, none of them a write" are
                // different diagnoses pointing at different fixes, and a grade line that guesses
                // wrong sends the reader chasing the lane instead of the cognition.
                // [[easy-diagnostics-are-cheap-wrongness]]
                format!(
                    "solution_file '{rel_path}' not found — she never wrote it. The task is \
                     graded on the file her hands produced; check the act trail for whether she \
                     acted at all or acted without ever calling a write."
                ),
            );
        }
    };
    let _ = std::fs::remove_file(&path);
    grade_in_scratch(&code, test).await
}

/// [`grade_file`]'s verdict: pass and the whole text.
pub async fn test_grade_file(
    root: Option<&std::path::Path>,
    rel_path: &str,
    lang: &str,
    test: &str,
) -> (bool, String) {
    grade_file(root, rel_path, lang, test).await.verdict()
}

/// Grade `code` against `test` in a scratch directory of its own, removed afterwards.
async fn grade_in_scratch(code: &str, test: &str) -> Grade {
    let scratch = match ClassWriter::of(SCRATCH_CLASS) {
        Ok(class) => class.owned_by(Uuid::new_v4()),
        Err(e) => return Grade::refused(format!("grader scratch unavailable: {e}")),
    };
    let grade = grade_rust(&scratch, code, test).await;
    if let Ok(src) = scratch.path(Path::new(SOURCE)) {
        if let Some(dir) = src.parent() {
            let _ = std::fs::remove_dir_all(dir); // a leftover is the class's to evict; the grade stands
        }
    }
    grade
}

/// The generated source and binary names inside a grade's scratch directory.
const SOURCE: &str = "sol.rs";
const BINARY: &str = "sol";

/// Compile the candidate (with a `main` built from the task's test) using `rustc`, then run the
/// binary. Passes iff it compiles AND the test's asserts don't panic (exit 0). A failure carries
/// its stage, its identity set and the whole text, so the failure is diagnosable and comparable.
async fn grade_rust(scratch: &ClassWriter, code: &str, test: &str) -> Grade {
    // GUARD the false-pass (proven 2026-07-21): the task's `test` is spliced into a
    // GENERATED `fn main() { <test> }`, so it MUST be bare assertion statements. If it
    // instead defines a `#[test]` function or its own `fn main`, that becomes a DEAD
    // nested item main never calls — the assertions never run, the process exits 0, and
    // WRONG CODE FALSE-PASSES. A benchmark that green-lights broken code is worse than no
    // benchmark. Fail LOUD with the fix, never a silent green. [[fallbacks-are-illegal-fail-loud]]
    if let Some(bad) = ["#[test]", "#[cfg(test)]", "fn main"].into_iter().find(|m| test.contains(m)) {
        return Grade::refused(format!(
            "gym test format error: `test` contains `{bad}`, which the harness splices into a \
             generated `fn main()` where it is NEVER CALLED — the assertions would not run and \
             wrong code would FALSE-PASS. Author `test` as bare assert!/assert_eq! statements; the \
             harness drives them from main."
        ));
    }
    // The candidate defines the item(s); the task's `test` drives them from main and
    // panics (assert!/assert_eq!) on failure, so a non-zero exit == fail. Strip any
    // `fn main` the candidate wrote to self-verify — the grader's test-driven main is
    // authoritative, and two module-level mains are an E0428 collision.
    let code = strip_top_level_main(code);
    let prelude = format!("#![allow(dead_code)]\n{code}\n\nfn main() {{\n");
    // The line `fn main() {` stands on: a panic at a later line is in the test, and its offset from
    // here names the test line whatever the candidate's length.
    let main_line = prelude.lines().count() as u32;
    let full = format!("{prelude}{test}\n}}\n");
    if let Err(e) = scratch.replace(Path::new(SOURCE), full.as_bytes()) {
        return Grade::refused(format!("grader scratch write failed: {e}"));
    }
    let (src, bin): (PathBuf, PathBuf) = match (scratch.path(Path::new(SOURCE)), scratch.path(Path::new(BINARY))) {
        (Ok(src), Ok(bin)) => (src, bin),
        (Err(e), _) | (_, Err(e)) => return Grade::refused(format!("grader scratch path refused: {e}")),
    };

    let mut rustc = tokio::process::Command::new("rustc");
    rustc.arg("--edition").arg("2021").arg("--error-format=json").arg("-o").arg(&bin).arg(&src);
    let compiled = match run_capped(rustc, "compile", COMPILE_TIMEOUT_SECS).await {
        Ok(out) => out,
        Err(e) => return failed(GradeStage::Compile, BTreeSet::from([e.clone()]), None, e),
    };
    if !compiled.status.success() {
        return compile_failure(&compiled.stderr);
    }

    let ran = match run_capped(tokio::process::Command::new(&bin), "run", RUN_TIMEOUT_SECS).await {
        Ok(out) => out,
        Err(e) => return failed(GradeStage::Run, BTreeSet::from([e.clone()]), None, e),
    };
    if ran.status.success() {
        return Grade { stage: GradeStage::Pass, failures: BTreeSet::new(), test_line: None, text: "tests passed".into() };
    }
    run_failure(&ran.stderr, main_line)
}

fn failed(stage: GradeStage, failures: BTreeSet<String>, test_line: Option<u32>, text: String) -> Grade {
    Grade { stage, failures, test_line, text }
}

/// A failed compile from rustc's own diagnostics (`--error-format=json`): each error's identity is
/// its code and message, and the teacher reads every rendered diagnostic whole.
fn compile_failure(stderr: &[u8]) -> Grade {
    let raw = String::from_utf8_lossy(stderr);
    let mut failures = BTreeSet::new();
    let mut rendered = String::new();
    for line in raw.lines() {
        let Ok(diagnostic) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(text) = diagnostic.get("rendered").and_then(|r| r.as_str()) {
            rendered.push_str(text);
        }
        // An error with a code or a location is a failure of the candidate; rustc's closing
        // "aborting due to N previous errors" has neither and only counts the others.
        let is_error = diagnostic.get("level").and_then(|l| l.as_str()) == Some("error");
        let code = diagnostic.pointer("/code/code").and_then(|c| c.as_str());
        let located = diagnostic.get("spans").and_then(|s| s.as_array()).is_some_and(|s| !s.is_empty());
        if is_error && (code.is_some() || located) {
            let message = diagnostic.get("message").and_then(|m| m.as_str()).unwrap_or_default(); // unwrap_or_default: a diagnostic without a message is still identified by its code
            failures.insert(format!("{}: {message}", code.unwrap_or("error"))); // unwrap_or: an uncoded error is identified by its message
        }
    }
    if failures.is_empty() {
        // rustc failed without a diagnostic it could serialize: the raw output is all there is.
        failures.insert("compile failed".into());
        rendered = raw.into_owned();
    }
    failed(GradeStage::Compile, failures, None, format!("compile error:\n{}", rendered.trim_end()))
}

/// A failed run: the panic's site names it. A panic inside the generated `main` is in the test,
/// identified by its test line (a later line means the assertions before it passed); a panic in
/// the candidate's own code is identified by its message.
fn run_failure(stderr: &[u8], main_line: u32) -> Grade {
    let text = String::from_utf8_lossy(stderr).trim_end().to_string();
    // `thread 'main' panicked at <path>/sol.rs:<line>:<column>:` then the message on the next line.
    let site = text.split("panicked at ").nth(1).and_then(|rest| {
        let location = rest.split_once('\n').map_or(rest, |(site, _)| site).trim_end_matches(':'); // map_or: a site with no message line is the whole rest
        let mut parts = location.rsplitn(3, ':');
        let _column = parts.next()?;
        parts.next()?.parse::<u32>().ok()
    });
    let message = text.lines().skip_while(|l| !l.contains("panicked at ")).nth(1).unwrap_or("").trim().to_string(); // unwrap_or: a panic with no message line is named by its site alone
    match site {
        Some(line) if line > main_line => {
            let test_line = line - main_line;
            failed(GradeStage::Run, BTreeSet::from([format!("test line {test_line}")]), Some(test_line), text)
        }
        Some(_) => failed(GradeStage::Run, BTreeSet::from([format!("solution panicked: {message}")]), None, text),
        None => failed(GradeStage::Run, BTreeSet::from(["exited without a panic site".to_string()]), None, text),
    }
}

/// Run one grader subprocess under the grade timeout, OWNED (`continuum_process::run_owned`): a
/// step that overruns, or whose grade is dropped, is ended with everything it started (model
/// code can spawn), never orphaned to init burning a core.
async fn run_capped(cmd: tokio::process::Command, label: &str, timeout_secs: u64) -> Result<std::process::Output, String> {
    use continuum_process::{run_owned, Ended};
    let out = run_owned(cmd, Some(std::time::Duration::from_secs(timeout_secs)), "gym-grader", std::sync::Arc::default())
        .await
        .map_err(|e| format!("{label} spawn failed: {e}"))?;
    if let (Ended::Exited(_), Some(note)) = (&out.ended, out.capture_note()) {
        return Err(format!("{label} exited but {note}"));
    }
    match out.ended {
        Ended::Exited(status) => Ok(std::process::Output { status, stdout: out.stdout.bytes, stderr: out.stderr.bytes }),
        Ended::TimedOut { unconfirmed } if unconfirmed.is_empty() => Err(format!("{label} timeout ({timeout_secs}s)")),
        Ended::TimedOut { unconfirmed } => Err(format!("{label} timeout ({timeout_secs}s); not confirmed gone: {unconfirmed:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (regression, 2026-07-21 forensics): a `test` authored as a
    // `#[test]` function is spliced into the generated `fn main()` as a DEAD nested fn
    // that never runs, so WRONG code exits 0 and FALSE-PASSES. The guard must reject that
    // format LOUD, never green-light broken code. Both the bare-assert path (grades) and
    // the #[test] path (rejected) are pinned so the two can never be confused again.
    #[tokio::test]
    async fn hashtest_wrapped_test_is_rejected_not_false_passed() {
        continuum_kernel::system_resources::disk_reporters::prepare_writer_fixture().await;
        let _home = crate::paths::test_home::HomeGuard::preserve().await;
        // WRONG sum_evens + a #[test]-wrapped test: must FAIL loud (format error), never pass.
        let (ok, msg) = { let g = grade(
            "```rust\nfn sum_evens(nums: &[i32]) -> i32 { 0 }\n```",
            "rust",
            "#[test]\nfn t() { assert_eq!(sum_evens(&[2,4]), 6); }",
        )
        .await; (g.passed(), g.text) };
        assert!(
            !ok,
            "a #[test]-wrapped test must NOT pass — that is the false-pass bug"
        );
        assert!(
            msg.contains("format error") && msg.contains("#[test]"),
            "must fail LOUD naming the bad format, got: {msg}"
        );
        // Sanity: the same WRONG code with a bare-assert test correctly FAILS on the panic.
        let ok2 = grade(
            "```rust\nfn sum_evens(nums: &[i32]) -> i32 { 0 }\n```",
            "rust",
            "assert_eq!(sum_evens(&[2,4]), 6);",
        )
        .await.passed();
        assert!(
            !ok2,
            "wrong code with a bare-assert test must fail on the assertion"
        );
    }

    // what this catches: the first fenced block is extracted and the ```lang tag
    // line is stripped, so a model that wraps its answer in ```rust … ``` is
    // graded on the CODE, not the fences.
    #[test]
    fn extracts_fenced_code_block_stripping_lang_tag() {
        let answer =
            "Sure!\n```rust\nfn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n```\nHope that helps.";
        assert_eq!(
            extract_code_block(answer),
            "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}"
        );
    }

    // what this catches: un-fenced answers fall back to the whole (trimmed) text,
    // so a model that emits bare code still gets graded rather than scoring 0.
    #[test]
    fn unfenced_answer_falls_back_to_whole_text() {
        assert_eq!(
            extract_code_block("  fn one() -> i32 { 1 }  "),
            "fn one() -> i32 { 1 }"
        );
    }

    // what this catches: a persona who splits ONE program across several ```rust
    // fences (imports first, then the logic) must be graded on ALL of it — the old
    // first-fence-only extraction dropped the logic and failed with `E0432: fs not
    // found` on code the model actually wrote (system tax). // regression: live
    // 2026-07-14 eval E0432 cluster
    #[test]
    fn concatenates_all_rust_fences_so_split_imports_survive() {
        let answer = "First, the imports:\n```rust\nuse std::fs;\n```\n\
                      Then the logic:\n```rust\nfn read_it(p: &str) -> String {\n    \
                      fs::read_to_string(p).unwrap()\n}\n```";
        let code = extract_code_block(answer);
        assert!(
            code.contains("use std::fs;"),
            "keeps the imports fence: {code}"
        );
        assert!(code.contains("fn read_it"), "keeps the logic fence: {code}");
    }

    // what this catches: a NON-Rust fence (expected-output sample, shell transcript)
    // that follows the Rust solution must NOT be concatenated into the compiled unit
    // — only Rust-tagged fences are joined; a text fence is ignored.
    #[test]
    fn ignores_non_rust_fences_when_a_rust_fence_exists() {
        let answer = "```rust\nfn add(a: i32, b: i32) -> i32 { a + b }\n```\n\
                      Expected output:\n```text\n5\n```";
        let code = extract_code_block(answer);
        assert_eq!(code, "fn add(a: i32, b: i32) -> i32 { a + b }");
    }

    // what this catches: correct Rust that compiles AND whose asserts hold → exit 0
    // → PASS with "tests passed". Drives the real rustc grader end-to-end.
    #[tokio::test]
    async fn passing_test_grades_pass() {
        continuum_kernel::system_resources::disk_reporters::prepare_writer_fixture().await;
        let _home = crate::paths::test_home::HomeGuard::preserve().await;
        let answer = "```rust\nfn add(a: i32, b: i32) -> i32 { a + b }\n```";
        let (ok, grade) = { let g = grade(answer, "rust", "assert_eq!(add(2, 3), 5);").await; (g.passed(), g.text) };
        assert!(ok, "grade was: {grade}");
        assert_eq!(grade, "tests passed");
    }

    // what this catches: code that compiles but whose assert fails → non-zero exit →
    // FAIL, and the panic message (not a vibe) is surfaced as the grade.
    #[tokio::test]
    async fn failing_test_grades_fail_with_panic() {
        continuum_kernel::system_resources::disk_reporters::prepare_writer_fixture().await;
        let _home = crate::paths::test_home::HomeGuard::preserve().await;
        let answer = "```rust\nfn add(a: i32, b: i32) -> i32 { a - b }\n```";
        let (ok, grade) = { let g = grade(answer, "rust", "assert_eq!(add(2, 3), 5);").await; (g.passed(), g.text) };
        assert!(!ok);
        assert!(
            grade.contains("assert") || grade.contains("panic") || grade.contains("left"),
            "grade was: {grade}"
        );
    }

    // what this catches: code that does not compile → FAIL with the compiler error
    // surfaced, so the persona can read what broke rather than guessing.
    #[tokio::test]
    async fn non_compiling_code_grades_fail_with_compile_error() {
        continuum_kernel::system_resources::disk_reporters::prepare_writer_fixture().await;
        let _home = crate::paths::test_home::HomeGuard::preserve().await;
        let answer = "```rust\nfn add(a: i32, b: i32) -> i32 { a + }\n```";
        let (ok, grade) = { let g = grade(answer, "rust", "let _ = add(2, 3);").await; (g.passed(), g.text) };
        assert!(!ok);
        assert!(grade.contains("compile error"), "grade was: {grade}");
    }

    // what this catches: ARTIFACT grading reads HER FILE (an acting persona's hands), not
    // spoken text — a correct file passes, the file is removed after grading so a stale
    // artifact can't false-pass a later task, a missing file fails LOUD ("never wrote it" =
    // acts=0), and a path escaping the workspace is refused. This is the seam that makes an
    // ACTING persona measurable at all (a spoken-text grade is blind to code she filed).
    #[tokio::test]
    async fn artifact_grade_reads_the_file_passes_correct_cleans_up_and_fails_loud() {
        continuum_kernel::system_resources::disk_reporters::prepare_writer_fixture().await;
        let _home = crate::paths::test_home::HomeGuard::preserve().await;
        let rel = format!("cu-gym-artifact-{}.rs", uuid::Uuid::new_v4());
        std::fs::write(&rel, "pub fn dbl(x: i32) -> i32 { x * 2 }\n").unwrap();
        let (ok, grade) = { let g = grade_file(None, &rel, "rust", "assert_eq!(dbl(3), 6);").await; (g.passed(), g.text) };
        assert!(ok, "a correct solution file should pass: {grade}");
        assert!(
            !std::path::Path::new(&rel).exists(),
            "the file must be removed after grading so it can't false-pass a later task"
        );
        let (ok2, grade2) = { let g = grade_file(None, &rel, "rust", "assert_eq!(dbl(3), 6);").await; (g.passed(), g.text) };
        assert!(
            !ok2 && grade2.contains("never wrote it"),
            "a missing file must fail loud, not silently pass: {grade2}"
        );
        let ok3 = grade_file(None, "../escape.rs", "rust", "").await.passed();

        // what this catches: THE take-2 defect — the grader must read the file
        // from the RUN'S root, not the process cwd. A correct solution in the
        // eval root graded "not found" (a manufactured lie about the solver)
        // when resolution used bare rel_path. Regression for 2026-08-23.
        let rooted = tempfile::tempdir().expect("tempdir");
        std::fs::write(rooted.path().join("sol_rooted.rs"), "pub fn dbl(x: i32) -> i32 { x * 2 }")
            .expect("write");
        let (ok4, grade4) = { let g = grade_file(
            Some(rooted.path()),
            "sol_rooted.rs",
            "rust",
            "assert_eq!(dbl(4), 8);",
        )
        .await; (g.passed(), g.text) };
        assert!(ok4, "rooted resolution must grade the real file: {grade4}");
        assert!(!ok3, "a path escaping the workspace must be refused");
    }

    // what this catches: a candidate that self-verifies with its OWN `fn main` (the
    // behavior we coach with "verify it works before answering") still grades on the
    // task's authoritative test instead of colliding with the grader's wrapper main
    // (regression for the prime_check `error[E0428]: the name 'main' is defined
    // multiple times` — Asha's answer carried a demo main).
    #[tokio::test]
    async fn candidate_with_its_own_main_still_grades_against_the_test() {
        continuum_kernel::system_resources::disk_reporters::prepare_writer_fixture().await;
        let _home = crate::paths::test_home::HomeGuard::preserve().await;
        let answer = "```rust\n\
            fn is_prime(n: u64) -> bool {\n\
            \x20   if n <= 1 { return false; }\n\
            \x20   let mut i = 2;\n\
            \x20   while i * i <= n { if n % i == 0 { return false; } i += 1; }\n\
            \x20   true\n\
            }\n\n\
            fn main() {\n\
            \x20   println!(\"{}\", is_prime(7));\n\
            }\n```";
        let (ok, grade) = { let g = grade(
            answer,
            "rust",
            "assert!(is_prime(7)); assert!(!is_prime(9)); assert!(is_prime(2)); assert!(!is_prime(1));",
        )
        .await; (g.passed(), g.text) };
        assert!(ok, "grade was: {grade}");
    }

    // what this catches: strip_top_level_main removes ONLY a module-level main and
    // leaves the rest of the candidate (and a nested `main` inside another fn) intact.
    #[test]
    fn strip_top_level_main_removes_module_main_only() {
        let stripped =
            strip_top_level_main("fn f() -> i32 { 1 }\nfn main() {\n  let _ = f();\n}\n");
        assert_eq!(stripped, "fn f() -> i32 { 1 }");
        // a `main` nested in another fn body is not a module-level collision — keep it.
        let nested = "fn wrap() { fn main() { } }";
        assert_eq!(strip_top_level_main(nested), nested);
    }

    // what this catches: a non-Rust language fails LOUD (named reason), never
    // silently passes — the fail-loud contract for the Rust-only gym.
    #[tokio::test]
    async fn unsupported_lang_fails_loud() {
        continuum_kernel::system_resources::disk_reporters::prepare_writer_fixture().await;
        let (ok, grade) = { let g = grade("print('x')", "python", "// test").await; (g.passed(), g.text) };
        assert!(!ok);
        assert!(
            grade.contains("unsupported lang 'python'"),
            "grade was: {grade}"
        );
    }

    // what this catches: the teacher's progress read from text instead of the grader's own result,
    // and its error cut (the old 180-char head). A type error is a coded compile failure with the
    // whole rendered text; a failing second assertion names test line 2, whatever the candidate's length.
    #[tokio::test]
    async fn a_grade_is_typed_by_stage_identity_and_test_line() {
        continuum_kernel::system_resources::disk_reporters::prepare_writer_fixture().await;
        let _home = crate::paths::test_home::HomeGuard::preserve().await;
        let typed = grade("```rust\nfn add(a: i32, b: i32) -> i32 { \"no\" }\n```", "rust", "assert_eq!(add(1, 1), 2);").await;
        assert_eq!(typed.stage, GradeStage::Compile, "{typed:?}");
        assert!(typed.failures.iter().all(|f| f.starts_with("E0308")), "{:?}", typed.failures);
        assert!(typed.text.contains("mismatched types") && typed.text.len() > 180, "whole text: {}", typed.text);
        let padding = "// a longer candidate\n".repeat(30);
        let ran = grade(
            &format!("```rust\n{padding}fn add(a: i32, b: i32) -> i32 {{ a * b }}\n```"),
            "rust",
            "assert_eq!(add(2, 2), 4);\nassert_eq!(add(1, 1), 2);",
        )
        .await;
        assert_eq!((ran.stage, ran.test_line), (GradeStage::Run, Some(2)), "{ran:?}");
        assert!(ran.failures.contains("test line 2"));
    }
}
