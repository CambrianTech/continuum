//! Liveness regression fixtures.
#![cfg(test)]
use super::*;
// what this catches: the work-scaled patience rule (2026-08-23, three
// MirrorCode-baseline kills): a lane holding a window-scale in-flight
// prompt earns DOUBLE the miss threshold (a kill destroys minutes of
// re-prefill and can loop); a quiet or small-investment lane keeps base
// patience and still dies fast. Relative quarter-window bound — no
// absolute token constant to drift.
#[test]
fn wedge_patience_scales_with_inflight_investment() {
    assert_eq!(wedge_miss_threshold(2, None, 65_024), 2, "no in-flight = base");
    assert_eq!(wedge_miss_threshold(2, Some(1_000), 65_024), 2, "small investment = base");
    assert_eq!(wedge_miss_threshold(2, Some(16_256), 65_024), 4, "quarter-window doubles");
    assert_eq!(wedge_miss_threshold(2, Some(82_668), 163_072), 4, "the live case doubles");
    assert_eq!(wedge_miss_threshold(2, Some(82_668), 0), 2, "unknown window = base, never a guess");
}

// what this catches (2026-08-15, round-killer #2 of the day): a never-started
// stream timeout stamped `note_real_decode_failure` unconditionally, so an
// OVERSUBSCRIBED lane (4 demand lanes, 1-2 slots, multi-minute thinking turns)
// hit the relaunch threshold from queue starvation alone — the health monitor
// killed a lane whose own log showed 11k-token completions at 17.5 t/s, every
// 2 minutes, forever. The classifier reads the lane's own delivery record:
// tokens for ANYONE within the wait = Starved (alive, no stamp); no delivery
// across the whole wait, or none ever observed = WedgeEvidence (stamp — the
// #363 undersized/dead class this counter exists for).
#[test]
fn never_started_timeout_on_a_delivering_lane_is_starvation_not_wedge_evidence() {
    use super::{classify_never_started_timeout, NeverStartedClass};
    // Busy lane: someone got tokens 30s into our 120s wait — alive.
    assert_eq!(
        classify_never_started_timeout(Some(30_000), 120_000),
        NeverStartedClass::Starved
    );
    // Dead lane: last delivery predates our whole wait — genuine evidence.
    assert_eq!(
        classify_never_started_timeout(Some(300_000), 120_000),
        NeverStartedClass::WedgeEvidence
    );
    // Fresh boot / unreadable clock: no delivery record — do not excuse the
    // timeout on faith; the wedge path (and its relaunch) is the safe read.
    assert_eq!(
        classify_never_started_timeout(None, 120_000),
        NeverStartedClass::WedgeEvidence
    );
    // Boundary: delivery exactly as old as the wait means nothing came out
    // DURING it — evidence, not starvation.
    assert_eq!(
        classify_never_started_timeout(Some(120_000), 120_000),
        NeverStartedClass::WedgeEvidence
    );
}

// what this catches (2026-08-16, the 23-minute boot kill-loop): a smoke miss on
// a lane doing GHOST WORK (a dead core's in-flight turns, no adapter stream →
// no L9 stamps) counted as wedge evidence, so 2 misses killed a lane llama's
// own log showed grinding flat out — three times in one boot. The verdict must
// exonerate a miss when the server's own /slots state ADVANCED between misses,
// keep a FROZEN fingerprint counting (the 2026-08-05 wedge signature holds one
// task at one impossible progress forever), never exonerate on a first miss
// (nothing to compare), and never exonerate on a fetch error.
#[test]
fn smoke_miss_on_an_advancing_lane_is_contention_not_wedge_evidence() {
    use super::{judge_smoke_miss, SmokeMissVerdict};
    // Slots advanced between the two misses: real work held the probe out.
    assert_eq!(
        judge_smoke_miss(Some(1), Some(2)),
        SmokeMissVerdict::AliveViaSlotProgress
    );
    // Frozen fingerprint: the wedge signature — count it.
    assert_eq!(judge_smoke_miss(Some(7), Some(7)), SmokeMissVerdict::CountMiss);
    // First miss: nothing to compare — count it (threshold ≥ 2 keeps the
    // detection latency identical to pre-L11).
    assert_eq!(judge_smoke_miss(None, Some(7)), SmokeMissVerdict::CountMiss);
    // /slots unreadable: a fetch error is never exoneration.
    assert_eq!(judge_smoke_miss(Some(7), None), SmokeMissVerdict::CountMiss);
    assert_eq!(judge_smoke_miss(None, None), SmokeMissVerdict::CountMiss);
}

// what this catches: the fingerprint must move with WORK (prefill advance, task
// turnover, decode count) and must NOT move with volatile per-request params —
// hashing the params blob would churn every poll and exonerate a dead lane
// forever. JSON shape pinned from a live /slots response (2026-08-16, :58057).
#[test]
fn slots_fingerprint_moves_with_work_and_ignores_params() {
    use super::slots_activity_fingerprint_of;
    let slot = |processed: u64, temp: f64| {
        serde_json::json!([{
            "id": 0, "n_ctx": 32512, "is_processing": true, "id_task": 55395,
            "n_prompt_tokens": 143, "n_prompt_tokens_processed": processed,
            "params": { "temperature": temp, "top_k": 20 }
        }])
    };
    let base = slots_activity_fingerprint_of(&slot(4, 0.0)).expect("array parses");
    // Prefill advanced → fingerprint moves.
    assert_ne!(
        base,
        slots_activity_fingerprint_of(&slot(90, 0.0)).unwrap(),
        "prefill advance must change the fingerprint"
    );
    // Only volatile params changed → fingerprint stable.
    assert_eq!(
        base,
        slots_activity_fingerprint_of(&slot(4, 0.9)).unwrap(),
        "params churn must NOT change the fingerprint"
    );
    // Non-array body (error page, wrong endpoint) → None, never a fake hash.
    assert_eq!(
        slots_activity_fingerprint_of(&serde_json::json!({"error": "nope"})),
        None
    );
}

// what this catches (2026-08-24, the 1-lane kill-loop): current llama-server
// wraps decode state in an ARRAY (`next_token: [{n_decoded: N}]`); the old
// string-key read returned None on it, so during a long DECODE (prefill done →
// `n_prompt_tokens_processed` frozen, one task → `id_task` frozen) the
// fingerprint was static and L11 judged a lane decoding at 31 tok/s as WEDGED —
// two smoke misses killed a 35k-token in-flight turn. Shape pinned from the live
// /slots response (:58057). Decode advance must move the fingerprint through
// BOTH the array shape and the older object shape.
#[test]
fn slots_fingerprint_sees_decode_through_the_array_shaped_next_token() {
    use super::slots_activity_fingerprint_of;
    let decoding = |n_decoded: u64, n_prompt: u64| {
        serde_json::json!([{
            "id": 0, "n_ctx": 166_400, "is_processing": true, "id_task": 350,
            "n_prompt_tokens": n_prompt, "n_prompt_tokens_processed": 35_144,
            "next_token": [{ "has_next_token": true, "n_remain": 697, "n_decoded": n_decoded }]
        }])
    };
    // Pure decode: processed/id_task static, only n_decoded (+ context growth) move.
    assert_ne!(
        slots_activity_fingerprint_of(&decoding(148, 47_211)).unwrap(),
        slots_activity_fingerprint_of(&decoding(400, 47_211)).unwrap(),
        "array-shaped n_decoded advance must change the fingerprint"
    );
    // Same story via the context-growth belt alone (build hides n_decoded).
    assert_ne!(
        slots_activity_fingerprint_of(&decoding(148, 47_211)).unwrap(),
        slots_activity_fingerprint_of(&decoding(148, 47_704)).unwrap(),
        "n_prompt_tokens growth during decode must change the fingerprint"
    );
    // Truly frozen (the 2026-08-05 4-hour wedge shape): identical looks stay equal.
    assert_eq!(
        slots_activity_fingerprint_of(&decoding(148, 47_211)).unwrap(),
        slots_activity_fingerprint_of(&decoding(148, 47_211)).unwrap(),
        "a frozen slot must keep a frozen fingerprint"
    );
    // Older builds: object-shaped next_token still read.
    let object_shape = |n: u64| {
        serde_json::json!([{
            "id": 0, "is_processing": true, "id_task": 1,
            "n_prompt_tokens": 100, "n_prompt_tokens_processed": 100,
            "next_token": { "n_decoded": n }
        }])
    };
    assert_ne!(
        slots_activity_fingerprint_of(&object_shape(5)).unwrap(),
        slots_activity_fingerprint_of(&object_shape(6)).unwrap(),
        "object-shaped n_decoded advance must change the fingerprint"
    );
}

// what this catches (2026-08-15, the round-killer): the smoke verdict judged a
// THINKING model's healthy decode as a dead lane because the whole quick budget
// went to the reasoning channel and `content` came back empty — serving was
// marked degraded forever and every turn waited on a transition that never
// resolved. The verdict must read that shape as ThinkStarved (retry), keep the
// wedged-lane shape (~2 tokens) Dead, keep the embedding-model-on-the-port
// shape (garbage in the VISIBLE channel, no reasoning) Dead, and pass a real
// answer regardless of whether reasoning is present.
#[test]
fn smoke_verdict_separates_thinking_from_wedged_from_garbage() {
    use super::{smoke_verdict, SmokeVerdict};
    // Healthy non-thinking model: answer in the visible channel.
    assert_eq!(
        smoke_verdict(20, "1 2 3 4 5 6 7 8 9 10", 0),
        SmokeVerdict::Alive
    );
    // Healthy THINKING model after the big-budget retry: reasoning present AND
    // the visible answer counts — reasoning must not disqualify a real answer.
    assert_eq!(
        smoke_verdict(120, "1 2 3 4 5 6 7 8 9 10", 400),
        SmokeVerdict::Alive
    );
    // Thinking model starved by the quick budget: decode proven, all of it
    // private, visible empty — retry, never "dead lane".
    assert_eq!(smoke_verdict(24, "", 300), SmokeVerdict::ThinkStarved);
    assert_eq!(smoke_verdict(24, "   ", 300), SmokeVerdict::ThinkStarved);
    // Wedged lane: ~2 tokens then stop. Dead on either budget.
    assert_eq!(smoke_verdict(2, "", 0), SmokeVerdict::Dead);
    assert_eq!(smoke_verdict(2, "", 50), SmokeVerdict::Dead);
    // Embedding model bound to the port: garbage in the VISIBLE channel, no
    // reasoning channel at all (the BigMama incident) — Dead, never a retry.
    assert_eq!(
        smoke_verdict(24, "user interface interface UIUUIUUIuiui", 0),
        SmokeVerdict::Dead
    );
    // Empty visible with NO reasoning: nothing proves a mind is attached.
    assert_eq!(smoke_verdict(24, "", 0), SmokeVerdict::Dead);
}

// what this catches: the latched-backend wedge class, AND the empty that must NOT
// trigger it. A Metal compute context that has gone bad still answers 200 and streams
// a well-formed SSE body with ZERO deltas, so no transport-failure stamp fires and the
// lane reads healthy while every citizen on it gets an empty turn. Measured on the M5
// 2026-09-22: 2 root Metal OOMs latched the backend, 113 tasks ran prefill and never
// generated, 73 empty-completion faults landed on six citizens — and `REAL_DECODE_FAILS`
// stayed 0 the whole time.
//
// The finish reason is load-bearing and was missing from the first cut of this fix
// (Cormac's review of #4344). The IntelMac's empties the same hour were `Length` on a
// lane with 27 successful generations in the same window: the cap ended the turn, the
// backend is fine, and two of them would have bounced a healthy lane for nothing. Same
// sentence in the receipt, opposite cure.
#[test]
fn only_an_empty_that_stopped_is_the_lanes_failure() {
    use super::{classify_real_decode, RealDecodeOutcome};
    use crate::ai::types::FinishReason;

    // Nothing produced at all: the relaunch is the cure.
    assert_eq!(
        classify_real_decode(true, false, FinishReason::Stop),
        RealDecodeOutcome::DeliveredNothing,
        "an empty local-lane generation that STOPPED must count AGAINST the lane — \
         this is the evidence the heartbeat relaunches on"
    );
    // The cap ended it. The backend generated; the allowance ran out. Relaunching
    // would bounce a working lane and fix nothing — the output allowance owns this.
    assert_eq!(
        classify_real_decode(true, false, FinishReason::Length),
        RealDecodeOutcome::CappedBeforeOutput,
        "an empty that hit the OUTPUT CAP is an allowance fault on healthy hardware, \
         never lane wedge evidence"
    );
    // Real output is still proof of life whatever ended it, and still ends the streak.
    assert_eq!(
        classify_real_decode(true, true, FinishReason::Stop),
        RealDecodeOutcome::ProofOfLife
    );
    assert_eq!(
        classify_real_decode(true, true, FinishReason::Length),
        RealDecodeOutcome::ProofOfLife,
        "a truncated turn still DELIVERED — it is not a wedge, whatever else it is"
    );
    // A cloud provider's empty turn says nothing about our hardware. Every finish
    // reason, so a future refactor cannot let remote traffic smear the local record.
    for fr in [
        FinishReason::Stop,
        FinishReason::Length,
        FinishReason::ToolUse,
        FinishReason::Error,
    ] {
        assert_eq!(classify_real_decode(false, false, fr), RealDecodeOutcome::NotOurLane);
        assert_eq!(classify_real_decode(false, true, fr), RealDecodeOutcome::NotOurLane);
    }
}
