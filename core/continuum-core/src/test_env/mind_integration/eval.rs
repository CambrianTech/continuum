#![cfg(test)]
//! From `cognition::eval`: a held-out check graded but never shown, read through the card
//! body core's `commands::benchmark` dispatches. Asserts exactly what it asserted before the
//! move.

use crate::cognition::eval::*;

// what this catches (card 47d2ab33): a held-out check that leaks. The lc-related set
// measures whether she applies a learned rule unprompted; if the rule reaches her through
// the card body or a verdict line, every arm is taught it and the measurement is void.
// The check must also gate the grade (a red held-out fails a green DoD) and must never
// run on work whose visible DoD is already red.
#[tokio::test]
async fn a_held_out_check_grades_without_ever_being_shown() {
    let dir = tempfile::tempdir().expect("tempdir");
    let secret = "grep -q SECRET-RULE-MARKER rule.txt";
    let t = EvalTask {
        id: "lc-related/env-port".into(),
        prompt: "implement port()".into(),
        dod_shell: Some("true".into()),
        held_out_shell: Some(secret.into()),
        ..Default::default()
    };
    let body = crate::commands::benchmark::dispatch_card_body("coursework-0123456789ab", &t);
    assert!(!body.contains("SECRET-RULE-MARKER"), "the card never shows the held-out check:\n{body}");

    // the file exists (a missing one would read as an infra fault, not a red result)
    std::fs::write(dir.path().join("rule.txt"), "").expect("write");
    let red = run_workspace_grade(Some(dir.path()), &t, "true").await;
    assert!(matches!(red, DodVerdict::Fail(_)), "a missed held-out requirement fails a green DoD: {red:?}");
    assert!(!red.message().contains("SECRET-RULE-MARKER"), "nor does a verdict: {}", red.message());

    std::fs::write(dir.path().join("rule.txt"), "SECRET-RULE-MARKER").expect("write");
    let green = run_workspace_grade(Some(dir.path()), &t, "true").await;
    assert!(green.passed(), "both checks green: {green:?}");
    assert!(!green.message().contains("SECRET-RULE-MARKER"), "{}", green.message());

    let t_marks = EvalTask { held_out_shell: Some("touch held-out-ran".into()), ..t };
    let visible_red = run_workspace_grade(Some(dir.path()), &t_marks, "false").await;
    assert!(matches!(visible_red, DodVerdict::Fail(_)));
    assert!(!dir.path().join("held-out-ran").exists(), "a red DoD is the verdict; the held-out check does not run");
}
