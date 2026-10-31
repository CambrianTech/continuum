//! Readiness regression fixtures.
#![cfg(test)]
use super::*;
// what this catches: the decode smoke-probe passing a lane that cannot chat at all. The gate
// used to assert only `completion_tokens >= 5`, which measures VOLUME, not correctness — so
// when an EMBEDDING model was bound to the port the persona lane was configured for
// (BigMama 2026-08-04, qwen3-embedding-0.6b on :8090), it sailed through and every persona on
// the node "thought" through it. The literal reply below is the real one that passed.
//
// Mutation checks: accepting any digit instead of a run of three passes the embedding string
// ("UIUUIU..." has none, but "1 interface 7 ui 3" would); requiring exact formatting fails the
// legitimate preamble/comma cases a real model produces.
#[test]
fn decode_probe_content_check_rejects_a_lane_that_cannot_count() {
    // The measured failure — fluent-looking noise, well past any token-count gate.
    assert!(
        !counts_upward("user interface interface user interface UIUUIUUIuiuiuiuiuiuiuiui"),
        "the embedding model's real reply must NOT pass the decode probe"
    );
    // Scattered digits are not comprehension.
    assert!(!counts_upward("7 interface 3 ui 9"));
    assert!(!counts_upward("1 1 1 1"));
    assert!(!counts_upward(""));
    // Healthy answers, in the shapes real models actually emit.
    assert!(counts_upward("1 2 3 4 5 6 7 8 9 10"));
    assert!(counts_upward("Sure! 1, 2, 3, 4, 5"));
    assert!(counts_upward("1\n2\n3"));
    // A truncated-but-correct answer (max_tokens cut it off) is still healthy.
    assert!(counts_upward("1 2 3 4"));
}

// what this catches (#205 unmask, glass-boxed 2026-07-28): an ephemeral eval lane
// whose child was OS-killed (SIGKILL/137) before it emitted anything produced an
// EMPTY stderr log and masked as a bare 240s /health timeout. `tail_or_hang_marker`
// must turn an empty log into the "no output before it stopped" fingerprint naming
// the OOM-kill vs. early-init-hang split (so the error names the real memory-wall
// failure mode) while a non-empty log yields its last lines in order. Mutation
// checks: dropping the empty-branch fails the OOM/jetsam assert; a non-reversed tail
// fails the ordering assert.
// what this catches (the 5090, 2026-10-06): a verified new engine whose first load
// listened at 92s was declared failed at the 90s live budget, and the slot rollback put
// the lanes back on the old engine. A server of ours saying "loading" earns the cold-load
// ceiling; every other state keeps the tight budget, so a wedge still fails fast.
#[test]
fn a_loading_engine_of_ours_is_not_a_failed_one() {
    assert_eq!(ready_budget(READY_TIMEOUT, true), EPHEMERAL_READY_TIMEOUT);
    assert!(
        ready_budget(READY_TIMEOUT, true) > Duration::from_secs(92),
        "the measured first load fits"
    );
    assert_eq!(
        ready_budget(READY_TIMEOUT, false),
        READY_TIMEOUT,
        "silence or an adopted process keeps the tight budget"
    );
    assert_eq!(
        ready_budget(EPHEMERAL_READY_TIMEOUT, true),
        EPHEMERAL_READY_TIMEOUT,
        "never shorter than the caller's own budget"
    );
    // Regression: total cold load may exceed 240s while measured work advances.
    // Repeated 503, stage labels, wrong children and warmup silence earn no time.
    assert!(
        serde_json::from_str::<LoadingHealth>(r#"{"error":{"code":503}}"#)
            .expect("older engine health shape")
            .load_progress
            .is_none()
    );
    assert!(
        serde_json::from_str::<LoadingHealth>(r#"{"load_progress":{"pid":"wrong"}}"#).is_err()
    );
    let mut progress = ReadyLoadProgress::default();
    let sample = |pid, current, value| {
        serde_json::from_value::<LoadProgressSample>(serde_json::json!({
            "pid": pid, "stages": ["weights", "vision"], "current": current, "value": value,
        }))
        .expect("native snapshot shape")
    };
    let seconds = Duration::from_secs;
    assert!(!progress.observe(&sample(7, "weights", 0.0), 7, seconds(100)));
    assert!(progress.stalled(seconds(240), EPHEMERAL_READY_TIMEOUT));
    assert!(progress.observe(&sample(7, "weights", 0.5), 7, seconds(200)));
    assert!(!progress.stalled(seconds(300), EPHEMERAL_READY_TIMEOUT));
    assert!(!progress.observe(&sample(7, "weights", 0.5), 7, seconds(300)));
    assert!(!progress.observe(&sample(7, "weights", 0.4), 7, seconds(300)));
    assert!(!progress.observe(&sample(8, "weights", 1.0), 7, seconds(300)));
    assert!(!progress.observe(&sample(7, "vision", 0.0), 7, seconds(300)));
    assert!(!progress.observe(&sample(7, "vision", 1.1), 7, seconds(300)));
    let mut changed = sample(7, "vision", 0.5);
    changed.stages.reverse();
    assert!(!progress.observe(&changed, 7, seconds(300)));
    assert_eq!(
        progress.last_advanced,
        seconds(200),
        "rejected samples cannot renew the allowance"
    );
    assert!(progress.observe(&sample(7, "vision", 1.0), 7, seconds(400)));
    assert!(!progress.stalled(seconds(639), EPHEMERAL_READY_TIMEOUT));
    assert!(
        progress.stalled(seconds(640), EPHEMERAL_READY_TIMEOUT),
        "unmeasured warmup stays bounded"
    );
}

#[test]
fn tail_or_hang_marker_fingerprints_empty_and_tails_nonempty() {
    assert!(
        tail_or_hang_marker("").contains("OOM/jetsam"),
        "an empty log is the no-output fingerprint naming the OOM-kill cause, not a silent blank"
    );
    assert!(
        tail_or_hang_marker("   \n\n \t \n").contains("OOM/jetsam"),
        "whitespace-only counts as empty"
    );
    let log = (1..=30)
        .map(|i| format!("line{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let tail = tail_or_hang_marker(&log);
    assert!(
        tail.contains("line30") && tail.contains("line11") && !tail.contains("line10"),
        "keeps the LAST 20 lines in original order, not the first: {tail}"
    );
    // sanity: order preserved (line11 appears before line30)
    assert!(tail.find("line11") < tail.find("line30"));
}

// what this catches (M5 2026-10-25, card 9e3350bd's second site): a synthetic smoke decode
// seated on a slot holding a mind's tokens replaces her KV, so her next turn re-prefills from 0
// and her next page save writes the probe as her page. The seat is the scratch slot when the
// pool has one, any slot only when the server holds nothing, and otherwise no decode at all.
#[test]
fn a_smoke_decode_never_takes_a_slot_holding_a_mind() {
    use serde_json::json;
    let fresh = json!([{"id": 0, "is_processing": false, "n_prompt_tokens": 0}, {"id": 1, "is_processing": false}]);
    let resident = json!([{"id": 0, "is_processing": false, "n_prompt_tokens": 26728}, {"id": 1, "is_processing": false, "n_prompt_tokens": 0}]);
    let busy = json!([{"id": 0, "is_processing": true, "n_prompt_tokens": 0}]);
    assert_eq!(smoke_seat(Some(2), &resident), SmokeSeat::Scratch(2), "the scratch slot is always a seat");
    assert_eq!(smoke_seat(None, &fresh), SmokeSeat::Empty, "a server holding nothing may prove decode anywhere");
    assert_eq!(smoke_seat(None, &resident), SmokeSeat::Occupied, "one slot holding her prompt: no synthetic decode");
    assert_eq!(smoke_seat(None, &busy), SmokeSeat::Occupied, "a processing slot is taken");
    assert_eq!(smoke_seat(None, &json!({"error": "x"})), SmokeSeat::Occupied, "an unreadable account is never read as empty");
}
