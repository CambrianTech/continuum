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

/// Compile the candidate (with a `main` built from the task's test) using `rustc`,
/// then run the binary. `Ok(())` iff it compiles AND the test asserts don't panic
/// (exit 0). `Err` carries the first failing step's stderr — compile error or panic
/// message — so the failure is diagnosable, not a vibe.
async fn grade_rust(dir: &std::path::Path, code: &str, test: &str) -> Result<(), String> {
    let src = dir.join("sol.rs");
    let bin = dir.join("sol");
    // The candidate defines the item(s); the task's `test` drives them from main and
    // panics (assert!/assert_eq!) on failure, so a non-zero exit == fail.
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

    // what this catches: a non-Rust language fails LOUD (named reason), never
    // silently passes — the fail-loud contract for the Rust-only gym.
    #[tokio::test]
    async fn unsupported_lang_fails_loud() {
        let (ok, grade) = test_grade("print('x')", "python", "// test").await;
        assert!(!ok);
        assert!(grade.contains("unsupported lang 'python'"), "grade was: {grade}");
    }
}
