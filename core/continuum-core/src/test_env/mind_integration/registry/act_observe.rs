#![cfg(test)]
//! From `cognition::act_observe` (observation, perception): verb classification read off the
//! REAL command registry.

use crate::cognition::act_observe::test_support::{act, receipt, settle, wrote_without_observation};
use crate::cognition::act_observe::*;

// what this catches (Joel, 2026-10-06: "0 writes always a serious plumbing bug"): her
// submissions, review verdicts, ledger notes and new cards reach no disk, so `wrote`
// counted them as reading. They publish; looking at the board does not. Regression for
// 2026-10-07: Kimi's PR comment on #4812 classified as Other, so a published review read
// as no write at all.
#[test]
fn board_output_publishes_and_reading_the_board_does_not() {
    for verb in [
        "work/submit", "work/review", "work/note", "work/create", "work_submit",
        "code/github/pr-create", "code/github/pr-comment", "code/github/issue-create",
    ] {
        assert!(ToolVerb::classify(verb).publishes(), "{verb}");
        assert!(!ToolVerb::classify(verb).mutates(), "{verb}: the board is not her disk");
    }
    for verb in ["work/get", "work/list", "work/claim", "work/submission", "code/read"] {
        assert!(!ToolVerb::classify(verb).publishes(), "{verb}");
    }
}

// what this catches: the observation-gap geometry (Joel 2026-07-11 — the
// run+observe half of the creation loop is part of THEIR process). A concern
// that mutated the workspace with no LATER observation act (run/shell/read/
// screenshot) is unobserved; observation BEFORE the mutation doesn't count;
// a prior settled concern's writes don't nag the next one. Typed over ToolVerb.
#[test]
fn unobserved_mutation_geometry() {
    let w = || receipt(vec![act("code/write", Some("game.rs"))]);
    let r = || receipt(vec![act("code/shell", None)]);
    let read_first = || receipt(vec![act("code/read", Some("game.rs"))]);

    // write with no later observation → unobserved
    assert!(wrote_without_observation(&[w()]));
    // write then run → observed
    assert!(!wrote_without_observation(&[w(), r()]));
    // The live schema offers aliases, not just canonical names. Reading,
    // testing or seeing the changed page must not produce a false nag.
    for observed in ["read_file", "bash", "run_code", "screenshot", "perception_observe"] {
        assert!(!wrote_without_observation(&[
            receipt(vec![act("edit_file", Some("game.rs"))]),
            receipt(vec![act(observed, Some("game.rs"))]),
        ]), "{observed} must count after an edit");
    }
    // read BEFORE the write doesn't count as observing the write
    assert!(wrote_without_observation(&[read_first(), w()]));
    // a prior settled concern's write never leaks into this concern
    assert!(!wrote_without_observation(&[w(), settle(), r()]));
    // no mutation at all → nothing to observe
    assert!(!wrote_without_observation(&[receipt(vec![act(
        "code/tree",
        None
    )])]));
}
