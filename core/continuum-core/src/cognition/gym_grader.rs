//! Objective test-grading for the coder gym — shared by the eval harness
//! (`cognition/eval`) and the teacher-episode generator (`genome/teach`).
//!
//! The grade is the language's OWN verdict: compile the model's Rust with `rustc`,
//! drive it from a `main` built from the task's test, run it — pass = exit 0. Not
//! substring-on-prose, not Python. One grader, one source of truth: both the A/B
//! evaluator and the trajectory generator must grade identically, or a gene that
//! "passes" generation could "fail" eval (and vice versa) for grader-shape reasons
//! alone. Extracted here so that can never drift.

use uuid::Uuid;

/// Per-step grade timeout. A compile or run that overruns is SIGKILLed on drop.
const TEST_GRADE_TIMEOUT_SECS: u64 = 10;

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
/// SAFETY: compiles and runs model-generated code in a temp dir, each step under a
/// 10s timeout with `kill_on_drop` so a runaway is reaped, never orphaned. That is
/// the pragmatic floor for an OWNER's local dev machine (what coding agents do); it
/// is NOT a sandbox. Before public/untrusted tasks, this MUST run in a real sandbox
/// (container/seccomp). Slice 1 = prove the grading mechanism; sandbox is a P1 req.
pub async fn test_grade(answer: &str, lang: &str, test: &str) -> (bool, String) {
    let code = extract_code_block(answer);
    // Rust only — the persona ships Rust, so the gym grades Rust. Anything else
    // fails LOUD with a named reason, never a silent pass.
    match lang {
        "rust" | "rs" => {}
        other => {
            return (
                false,
                format!("unsupported lang '{other}' (Rust gym: lang must be 'rust')"),
            )
        }
    }
    let dir = std::env::temp_dir().join(format!("cu-gym-{}", Uuid::new_v4()));
    if std::fs::create_dir_all(&dir).is_err() {
        return (false, "temp dir create failed".to_string());
    }
    let result = grade_rust(&dir, &code, test).await;
    let _ = std::fs::remove_dir_all(&dir);
    match result {
        Ok(()) => (true, "tests passed".to_string()),
        Err(msg) => (false, msg),
    }
}

/// ARTIFACT grade: read her acted solution from an in-workspace file (her HANDS) and run the
/// SAME harness as [`test_grade`] (strip her `main`, append the task's test, compile, run) — the
/// only difference is the code source: the file she wrote + compiled, not a block extracted from
/// her spoken answer. This is the honest measure of an ACTING persona; a spoken-text grade is
/// blind to code she put in a file. `rel_path` resolves against the core cwd (the workspace root
/// `code/write` sandboxes to). Fails LOUD (never a silent pass) if she wrote nothing, the path
/// escapes the workspace, or the file is empty. The file is removed after grading so a stale
/// artifact from this or a prior task can never false-pass a later one.
pub async fn test_grade_file(rel_path: &str, lang: &str, test: &str) -> (bool, String) {
    match lang {
        "rust" | "rs" => {}
        other => {
            return (
                false,
                format!("unsupported lang '{other}' (Rust gym: lang must be 'rust')"),
            )
        }
    }
    let path = std::path::Path::new(rel_path);
    if path.is_absolute() || rel_path.contains("..") {
        return (
            false,
            format!("solution_file must be a relative in-workspace path, got '{rel_path}'"),
        );
    }
    let code = match std::fs::read_to_string(path) {
        Ok(c) if !c.trim().is_empty() => c,
        Ok(_) => {
            let _ = std::fs::remove_file(path);
            return (
                false,
                format!("solution_file '{rel_path}' is empty — she wrote no code"),
            );
        }
        Err(_) => {
            return (
                false,
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
    let _ = std::fs::remove_file(path);
    let dir = std::env::temp_dir().join(format!("cu-gym-{}", Uuid::new_v4()));
    if std::fs::create_dir_all(&dir).is_err() {
        return (false, "temp dir create failed".to_string());
    }
    let result = grade_rust(&dir, &code, test).await;
    let _ = std::fs::remove_dir_all(&dir);
    match result {
        Ok(()) => (true, "tests passed".to_string()),
        Err(msg) => (false, msg),
    }
}

/// Compile the candidate (with a `main` built from the task's test) using `rustc`,
/// then run the binary. `Ok(())` iff it compiles AND the test asserts don't panic
/// (exit 0). `Err` carries the first failing step's stderr — compile error or panic
/// message — so the failure is diagnosable, not a vibe.
async fn grade_rust(dir: &std::path::Path, code: &str, test: &str) -> Result<(), String> {
    let src = dir.join("sol.rs");
    let bin = dir.join("sol");
    // GUARD the false-pass (proven 2026-07-21): the task's `test` is spliced into a
    // GENERATED `fn main() { <test> }`, so it MUST be bare assertion statements. If it
    // instead defines a `#[test]` function or its own `fn main`, that becomes a DEAD
    // nested item main never calls — the assertions never run, the process exits 0, and
    // WRONG CODE FALSE-PASSES. A benchmark that green-lights broken code is worse than no
    // benchmark. Fail LOUD with the fix, never a silent green. [[fallbacks-are-illegal-fail-loud]]
    if let Some(bad) = ["#[test]", "#[cfg(test)]", "fn main"]
        .into_iter()
        .find(|m| test.contains(m))
    {
        return Err(format!(
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
    let full = format!("#![allow(dead_code)]\n{code}\n\nfn main() {{\n{test}\n}}\n");
    std::fs::write(&src, full).map_err(|e| format!("temp write failed: {e}"))?;

    let mut rustc = tokio::process::Command::new("rustc");
    rustc
        .arg("--edition")
        .arg("2021")
        .arg("-o")
        .arg(&bin)
        .arg(&src);
    let compiled = run_capped(&mut rustc, "compile").await?;
    if !compiled.status.success() {
        return Err(format!("compile error: {}", trunc_stderr(&compiled.stderr)));
    }

    let mut run = tokio::process::Command::new(&bin);
    let ran = run_capped(&mut run, "run").await?;
    if ran.status.success() {
        Ok(())
    } else {
        Err(trunc_stderr(&ran.stderr))
    }
}

/// Run one grader subprocess under the grade timeout with `kill_on_drop(true)` — a
/// step that overruns is SIGKILLed on drop, never orphaned to init burning a core.
async fn run_capped(
    cmd: &mut tokio::process::Command,
    label: &str,
) -> Result<std::process::Output, String> {
    cmd.kill_on_drop(true);
    match tokio::time::timeout(
        std::time::Duration::from_secs(TEST_GRADE_TIMEOUT_SECS),
        cmd.output(),
    )
    .await
    {
        Ok(Ok(out)) => Ok(out),
        Ok(Err(e)) => Err(format!("{label} spawn failed: {e}")),
        Err(_) => Err(format!("{label} timeout ({TEST_GRADE_TIMEOUT_SECS}s)")),
    }
}

/// First 180 chars of trimmed stderr — enough of the compiler/panic message to
/// diagnose without flooding the grade field.
fn trunc_stderr(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .trim()
        .chars()
        .take(180)
        .collect()
}

/// [`Verifier`](crate::cognition::resolution::Verifier) over the real code grader
/// ([`test_grade`]) — **outlier A** of the will-driven resolution spine
/// ([`crate::cognition::resolution`]). The draft is the persona's spoken answer;
/// verification compiles + runs it against the task's test harness. The compiler +
/// tests ARE the necessity detector (WILL-DRIVEN-RESOLUTION.md §2): a PASS ships, a
/// FAIL carries the real compiler/panic output as the escalation reason so a climb
/// re-drafts INFORMED by why the cheaper resolution fell short.
///
/// This is what makes "a pass with the higher model for code" fall out of the
/// escalation loop automatically AND guarantees the benchmark cannot regress —
/// failure is what summons the smarts. Rust-only today (the grader's constraint;
/// any other `lang` fails loud, never a silent pass).
pub struct CodeVerifier {
    lang: String,
    test: String,
}

impl CodeVerifier {
    /// Verify a draft against a task's test harness. `test` is the assertion body the
    /// grader wraps in `fn main()` — the same contract [`test_grade`] takes.
    pub fn new(lang: impl Into<String>, test: impl Into<String>) -> Self {
        Self {
            lang: lang.into(),
            test: test.into(),
        }
    }
}

impl crate::cognition::resolution::Verifier for CodeVerifier {
    type Draft = String;
    async fn verify(&self, draft: &String) -> crate::cognition::resolution::Verdict {
        let (passed, detail) = test_grade(draft, &self.lang, &self.test).await;
        if passed {
            crate::cognition::resolution::Verdict::pass(detail)
        } else {
            crate::cognition::resolution::Verdict::fail(detail)
        }
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
        // WRONG sum_evens + a #[test]-wrapped test: must FAIL loud (format error), never pass.
        let (ok, msg) = test_grade(
            "```rust\nfn sum_evens(nums: &[i32]) -> i32 { 0 }\n```",
            "rust",
            "#[test]\nfn t() { assert_eq!(sum_evens(&[2,4]), 6); }",
        )
        .await;
        assert!(
            !ok,
            "a #[test]-wrapped test must NOT pass — that is the false-pass bug"
        );
        assert!(
            msg.contains("format error") && msg.contains("#[test]"),
            "must fail LOUD naming the bad format, got: {msg}"
        );
        // Sanity: the same WRONG code with a bare-assert test correctly FAILS on the panic.
        let (ok2, _) = test_grade(
            "```rust\nfn sum_evens(nums: &[i32]) -> i32 { 0 }\n```",
            "rust",
            "assert_eq!(sum_evens(&[2,4]), 6);",
        )
        .await;
        assert!(
            !ok2,
            "wrong code with a bare-assert test must fail on the assertion"
        );
    }

    // what this catches (#168): CodeVerifier bridges the REAL rustc grader to the
    // resolution spine's Verdict — a correct draft verifies PASS, a wrong draft
    // verifies FAIL with the real compiler/assert output as the escalation reason.
    // This is outlier A of the will-driven resolution loop proven on real ground:
    // the objective verifier that lets "failure summons the smarts" hold.
    #[tokio::test]
    async fn code_verifier_passes_correct_draft_fails_wrong_one() {
        use crate::cognition::resolution::Verifier;
        let v = CodeVerifier::new("rust", "assert_eq!(add(2, 3), 5);");

        let good = "```rust\nfn add(a: i32, b: i32) -> i32 { a + b }\n```".to_string();
        let good_verdict = v.verify(&good).await;
        assert!(
            good_verdict.passed,
            "correct code must PASS, got: {}",
            good_verdict.detail
        );

        let bad = "```rust\nfn add(a: i32, b: i32) -> i32 { a - b }\n```".to_string();
        let bad_verdict = v.verify(&bad).await;
        assert!(
            !bad_verdict.passed,
            "wrong code must FAIL to trigger escalation"
        );
        assert!(
            !bad_verdict.detail.is_empty(),
            "a failure must carry a reason to escalate on"
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
        let answer = "```rust\nfn add(a: i32, b: i32) -> i32 { a + b }\n```";
        let (ok, grade) = test_grade(answer, "rust", "assert_eq!(add(2, 3), 5);").await;
        assert!(ok, "grade was: {grade}");
        assert_eq!(grade, "tests passed");
    }

    // what this catches: code that compiles but whose assert fails → non-zero exit →
    // FAIL, and the panic message (not a vibe) is surfaced as the grade.
    #[tokio::test]
    async fn failing_test_grades_fail_with_panic() {
        let answer = "```rust\nfn add(a: i32, b: i32) -> i32 { a - b }\n```";
        let (ok, grade) = test_grade(answer, "rust", "assert_eq!(add(2, 3), 5);").await;
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
        let answer = "```rust\nfn add(a: i32, b: i32) -> i32 { a + }\n```";
        let (ok, grade) = test_grade(answer, "rust", "let _ = add(2, 3);").await;
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
        let rel = format!("cu-gym-artifact-{}.rs", uuid::Uuid::new_v4());
        std::fs::write(&rel, "pub fn dbl(x: i32) -> i32 { x * 2 }\n").unwrap();
        let (ok, grade) = test_grade_file(&rel, "rust", "assert_eq!(dbl(3), 6);").await;
        assert!(ok, "a correct solution file should pass: {grade}");
        assert!(
            !std::path::Path::new(&rel).exists(),
            "the file must be removed after grading so it can't false-pass a later task"
        );
        let (ok2, grade2) = test_grade_file(&rel, "rust", "assert_eq!(dbl(3), 6);").await;
        assert!(
            !ok2 && grade2.contains("never wrote it"),
            "a missing file must fail loud, not silently pass: {grade2}"
        );
        let (ok3, _) = test_grade_file("../escape.rs", "rust", "").await;
        assert!(!ok3, "a path escaping the workspace must be refused");
    }

    // what this catches: a candidate that self-verifies with its OWN `fn main` (the
    // behavior we coach with "verify it works before answering") still grades on the
    // task's authoritative test instead of colliding with the grader's wrapper main
    // (regression for the prime_check `error[E0428]: the name 'main' is defined
    // multiple times` — Asha's answer carried a demo main).
    #[tokio::test]
    async fn candidate_with_its_own_main_still_grades_against_the_test() {
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
        let (ok, grade) = test_grade(
            answer,
            "rust",
            "assert!(is_prime(7)); assert!(!is_prime(9)); assert!(is_prime(2)); assert!(!is_prime(1));",
        )
        .await;
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
        let (ok, grade) = test_grade("print('x')", "python", "// test").await;
        assert!(!ok);
        assert!(
            grade.contains("unsupported lang 'python'"),
            "grade was: {grade}"
        );
    }
}
