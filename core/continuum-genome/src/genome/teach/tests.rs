#![cfg(test)]

use super::*;

// what this catches: a validated trajectory flattens to the ShareGPT shape
// mlx_lm.lora/dataset consume — role+content per turn, ORDER preserved (the
// write→error→fix→pass ordering IS the lesson). Drift here silently corrupts
// every example the teacher emits.
#[test]
fn build_sharegpt_preserves_role_content_and_order() {
    let traj = vec![
        ChatMessage::text("system", "be an engineer"),
        ChatMessage::text("user", "write add"),
        ChatMessage::text("assistant", "```rust\nfn add(){}\n```"),
        ChatMessage::text("user", "Your solution failed: compile error"),
        ChatMessage::text("assistant", "```rust\nfn add(a:i32,b:i32)->i32{a+b}\n```"),
    ];
    let v = build_sharegpt(&traj, &[]);
    let msgs = v["messages"].as_array().expect("messages array");
    assert_eq!(msgs.len(), 5);
    assert_eq!(msgs[0]["role"], "system");
    assert_eq!(msgs[1]["content"], "write add");
    assert_eq!(msgs[3]["role"], "user");
    assert_eq!(msgs[4]["role"], "assistant");
    assert!(msgs[4]["content"].as_str().unwrap().contains("a+b"));
}

    mod progress {
        use super::super::{AttemptProgress, Grade, GradeStage};

        fn grade(stage: GradeStage, failures: &[&str], test_line: Option<u32>, text: &str) -> Grade {
            Grade { stage, failures: failures.iter().map(|f| f.to_string()).collect(), test_line, text: text.into() }
        }
        fn compile(failures: &[&str]) -> Grade {
            grade(GradeStage::Compile, failures, None, "compile error")
        }
        fn run(line: u32) -> Grade {
            grade(GradeStage::Run, &[&format!("test line {line}")], Some(line), "panicked")
        }

        // what this catches: a teacher fixing her way to green cut off by a count. Fewer compile
        // errors, then running, then a later failing assertion are each progress and each continue.
        #[test]
        fn a_converging_fix_keeps_its_turns() {
            let mut p = AttemptProgress::default();
            for g in [compile(&["E0308: mismatched types", "E0425: cannot find value", "E0599: no method"]), compile(&["E0599: no method"]), run(1), run(3)] {
                assert_eq!(p.admit(&g), Ok(()), "{g:?}");
            }
        }

        // what this catches: a stuck loop granted more turns because its error text changes (a
        // timing, a temp path) while the failure is the same one.
        #[test]
        fn the_same_failure_again_ends_the_task_whatever_its_text() {
            let mut p = AttemptProgress::default();
            assert_eq!(p.admit(&grade(GradeStage::Run, &["test line 2"], Some(2), "panicked after 0.0031s")), Ok(()));
            assert!(p.admit(&grade(GradeStage::Run, &["test line 2"], Some(2), "panicked after 0.0047s")).is_err());
        }

        // what this catches: oscillation and diagnostic churn read as progress. One sideways step at
        // the best level is allowed; a second, or a return to an earlier failure, ends the task.
        #[test]
        fn one_sideways_step_per_best_level_then_it_ends() {
            let mut p = AttemptProgress::default();
            assert_eq!(p.admit(&compile(&["E0308: a", "E0425: b"])), Ok(()));
            assert_eq!(p.admit(&compile(&["E0599: c", "E0061: d"])), Ok(()), "one sideways step");
            assert!(p.admit(&compile(&["E0277: e", "E0614: f"])).is_err(), "a second sideways step at the same level");
            let mut back = AttemptProgress::default();
            assert_eq!(back.admit(&compile(&["E0308: a"])), Ok(()));
            assert_eq!(back.admit(&compile(&["E0599: c"])), Ok(()));
            assert!(back.admit(&compile(&["E0308: a"])).is_err(), "oscillating back to an earlier failure");
        }

        // what this catches (Codex on card a4bbae0a): fixing a syntax error exposes more errors from
        // rustc's later passes; that is progress past the parser, not a regression.
        #[test]
        fn getting_past_the_parser_is_progress_even_with_more_errors() {
            let mut p = AttemptProgress::default();
            assert_eq!(p.admit(&compile(&["error: expected `;`, found `}`"])), Ok(()));
            assert_eq!(p.admit(&compile(&["E0308: a", "E0425: b", "E0599: c"])), Ok(()));
            assert!(p.admit(&compile(&["error: unexpected token"])).is_err(), "back into the parser is a regression");
        }

        // what this catches: a refusal (an unsupported format, no scratch) counted as an attempt
        // that could progress, and a regression past the best kept running.
        #[test]
        fn a_refusal_or_a_regression_ends_the_task() {
            let mut refused = AttemptProgress::default();
            assert!(refused.admit(&grade(GradeStage::Refused, &["gym test format error"], None, "x")).is_err());
            let mut p = AttemptProgress::default();
            assert_eq!(p.admit(&run(3)), Ok(()));
            assert!(p.admit(&compile(&["E0308: a"])).is_err(), "back to a compile error after running");
        }
    }
