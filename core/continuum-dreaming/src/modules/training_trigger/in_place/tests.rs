use super::*;

fn outcome(committed_steps: i64, effects_uncertain: bool) -> LearnOutcome {
    serde_json::from_value(serde_json::json!({
        "success": committed_steps > 0, "committed_steps": committed_steps, "effects_uncertain": effects_uncertain,
        "refusal": "none", "loss": 0.0, "n_labels": 0, "t_ms": 0.0, "max_chunk_ms": 0.0, "restoration_ms": 0.0
    }))
    .expect("test: an engine outcome")
}

// what this catches: the cursor moving on an example the engine did not commit, or a quarantined
// shadow kept learning. Only a committed update with certain effects counts; uncertainty wins.
#[test]
fn only_a_committed_certain_update_moves_the_cursor() {
    assert_eq!(verdict(&outcome(1, false)), Verdict::Learned);
    assert_eq!(verdict(&outcome(0, false)), Verdict::Again, "refused, deferred, or ended before an update");
    assert_eq!(verdict(&outcome(1, true)), Verdict::Quarantined, "committed, then uncertain: quarantined, never counted");
    assert_eq!(verdict(&outcome(0, true)), Verdict::Quarantined);
}

#[test]
fn a_trait_kind_becomes_a_file_name_part() {
    assert_eq!(file_safe("code/owner"), "code-owner");
}
