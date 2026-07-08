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

/// Extract a code block from a model response for test-grading. Prefers the first
/// ```fenced``` block (stripping the language tag line); falls back to the whole
/// text. Small models wrap code in fences inconsistently — this is forgiving.
pub fn extract_code_block(answer: &str) -> String {
    if let Some(start) = answer.find("```") {
        let after = &answer[start + 3..];
        let body = match after.find('\n') {
            Some(i) => &after[i + 1..], // skip the ```lang tag line
            None => after,
        };
        if let Some(end) = body.find("```") {
            return body[..end].trim().to_string();
        }
    }
    answer.trim().to_string()
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
                format!("solution_file '{rel_path}' not found — she never wrote it (acts=0 on this task)"),
            )
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
    // The candidate defines the item(s); the task's `test` drives them from main and
    // panics (assert!/assert_eq!) on failure, so a non-zero exit == fail. Strip any
    // `fn main` the candidate wrote to self-verify — the grader's test-driven main is
    // authoritative, and two module-level mains are an E0428 collision.
    let code = strip_top_level_main(code);
    let full = format!("#![allow(dead_code)]\n{code}\n\nfn main() {{\n{test}\n}}\n");
    std::fs::write(&src, full).map_err(|e| format!("temp write failed: {e}"))?;

    let mut rustc = tokio::process::Command::new("rustc");
    rustc.arg("--edition").arg("2021").arg("-o").arg(&bin).arg(&src);
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
    String::from_utf8_lossy(stderr).trim().chars().take(180).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let stripped = strip_top_level_main("fn f() -> i32 { 1 }\nfn main() {\n  let _ = f();\n}\n");
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
        assert!(grade.contains("unsupported lang 'python'"), "grade was: {grade}");
    }
}
