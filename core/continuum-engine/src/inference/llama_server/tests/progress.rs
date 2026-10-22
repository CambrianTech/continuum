//! Progress regression fixtures.
#![cfg(test)]
use super::*;
// what this catches (card 6f3218ed, the act-bound half): the per-request rule that keeps
// a lost request from riding someone else's progress. The first checkpoint needs only a
// processing slot; a later one needs the SAME task with its work moved; a new task on the
// slot, an idle slot, or no reading at all ends the act.
#[test]
fn an_act_extends_only_while_its_own_slot_advances() {
    let body = serde_json::json!([
        {"id": 0, "id_task": 7, "is_processing": true, "n_prompt_tokens_processed": 4096, "next_token": [{"n_decoded": 0}]},
        {"id": 3, "id_task": 9, "is_processing": false, "n_prompt_tokens_processed": 2048, "next_token": [{"n_decoded": 0}]}
    ]);
    let ours = slot_work_of(&body, 0).expect("slot 0");
    assert_eq!((ours.task, ours.processing, ours.work), (7, true, 4096));
    assert_eq!(slot_work_of(&body, 5), None, "no such slot");
    let idle = slot_work_of(&body, 3);
    assert!(slot_advanced(None, Some(ours)), "first checkpoint: processing is enough");
    assert!(!slot_advanced(None, idle), "an idle slot is not progress, whatever its counters");
    assert!(!slot_advanced(None, None), "no reading is no evidence");
    let moved = SlotWork { work: 6144, ..ours };
    assert!(slot_advanced(Some(ours), Some(moved)), "same task, work moved");
    assert!(!slot_advanced(Some(ours), Some(ours)), "same task, nothing moved: stuck");
    assert!(!slot_advanced(Some(ours), Some(SlotWork { task: 8, ..moved })), "another task on the slot: ours is gone");
}

// what this catches (card 6f3218ed, IntelMac 2026-09-27): a generation's header wait
// killing a request that is queued or prefilling. With quiet_checkpoints = 1 (the header
// wait's setting), an engine whose work fingerprint keeps moving holds the wait across
// many checkpoints and the response is returned; an engine that goes quiet (nothing
// moved, nothing processing) ends it after ONE grace checkpoint, not the page switch's 8;
// and an engine processing with nothing moving is stuck at the first checkpoint.
#[tokio::test(start_paused = true)]
async fn a_header_wait_rides_a_moving_engine_and_ends_on_a_quiet_one() {
    let bound = std::time::Duration::from_secs(300);
    // c610ef8f: only counters from this exact restore hold its waiter. A
    // completed snapshot cannot replace the pending terminal HTTP receipt.
    let operation = uuid::Uuid::new_v4();
    let snapshot = |bytes, phase| serde_json::json!({
        "operation_id": operation, "id_slot": 0, "action": "restore",
        "phase": phase, "state_bytes": bytes, "sidecar_bytes": 0, "checkpoints": 0,
    });
    assert!(restore_progress(&snapshot(1, 1), uuid::Uuid::new_v4(), 0).is_none());
    assert!(restore_progress(&snapshot(1, 1), operation, 1).is_none());
    // Replaced/unknown operations cannot inherit another slot's moving
    // counters. Legacy compatibility is restricted to a successful array.
    let legacy = serde_json::json!([{"id_task": 9, "is_processing": true}]);
    assert!(matches!(restore_probe_response(reqwest::StatusCode::OK, &legacy, operation, 0), EngineProbe::Progress(_)));
    for (status, body) in [
        (reqwest::StatusCode::NOT_FOUND, legacy),
        (reqwest::StatusCode::OK, snapshot(900, 2)),
    ] {
        let unknown = restore_probe_response(status, &body, uuid::Uuid::new_v4(), 0);
        assert_eq!(unknown, EngineProbe::Busy);
        assert_eq!(wait_while_engine_progresses(std::future::pending::<()>(), bound, 1,
            || async { unknown }, |_| {}).await, None);
    }
    let completed = restore_progress(&snapshot(100, 3), operation, 0).unwrap();
    assert_eq!(wait_while_engine_progresses(std::future::pending::<()>(), bound, 1,
        || async { completed }, |_| {}).await, None);
    for phase in [4, 5] {
        let ended = restore_progress(&snapshot(100, phase), operation, 0).unwrap();
        let started = tokio::time::Instant::now();
        assert_eq!(wait_while_engine_progresses(std::future::pending::<()>(), bound, 1,
            || async { ended }, |_| {}).await, None);
        assert_eq!(started.elapsed(), Duration::ZERO);
    }
    let mut bytes = 0;
    let progressing = || {
        bytes += 1;
        let receipt = restore_progress(&snapshot(bytes, 2), operation, 0).unwrap();
        async move { receipt }
    };
    let terminal = async { tokio::time::sleep(bound * 5).await; "HTTP receipt" };
    assert_eq!(wait_while_engine_progresses(terminal, bound, 1, progressing, |_| {}).await,
        Some("HTTP receipt"));
    // Card 198ffe08: a slow initial/checkpoint probe must not stop polling
    // work or discard its terminal receipt when the probe later reports dead.
    for slow_initial in [true, false] {
        let mut calls = 0;
        let slow_probe = || {
            calls += 1;
            let slow = slow_initial || calls > 1;
            async move {
                if slow {
                    tokio::time::sleep(bound * 3).await;
                    EngineProbe::Unreachable
                } else {
                    EngineProbe::Busy
                }
            }
        };
        let completes_after = if slow_initial { bound / 2 } else { bound + bound / 2 };
        let work = async {
            tokio::time::sleep(completes_after).await;
            "terminal receipt"
        };
        let started = tokio::time::Instant::now();
        assert_eq!(wait_while_engine_progresses(work, bound, 1, slow_probe, |_| {}).await,
            Some("terminal receipt"));
        assert_eq!(started.elapsed(), completes_after,
            "completion must not wait for the unrelated probe deadline");
    }
    let moving = std::sync::atomic::AtomicU64::new(0);
    let advancing = || {
        let n = moving.fetch_add(2048, std::sync::atomic::Ordering::Relaxed);
        async move { EngineProbe::Progress(EngineProgress { fingerprint: n, any_processing: true }) }
    };
    let late = async {
        tokio::time::sleep(bound * 5).await;
        "headers"
    };
    let mut checkpoints = 0;
    let got = wait_while_engine_progresses(late, bound, 1, advancing, |n| checkpoints = n).await;
    assert_eq!(got, Some("headers"), "a moving engine is busy, not dead, for five bounds");
    assert!(checkpoints >= 4, "each checkpoint was a busy one: {checkpoints}");

    let started = tokio::time::Instant::now();
    let quiet = || async { EngineProbe::Progress(EngineProgress { fingerprint: 7, any_processing: false }) };
    let never = wait_while_engine_progresses(std::future::pending::<&str>(), bound, 1, quiet, |_| {}).await;
    assert_eq!(never, None);
    assert!(started.elapsed() <= bound * 2, "one grace checkpoint, then dead: {:?}", started.elapsed());

    let stuck = || async { EngineProbe::Progress(EngineProgress { fingerprint: 7, any_processing: true }) };
    let started = tokio::time::Instant::now();
    assert_eq!(wait_while_engine_progresses(std::future::pending::<&str>(), bound, 1, stuck, |_| {}).await, None);
    assert!(started.elapsed() <= bound, "processing with nothing moving is stuck at the first checkpoint");

    // A CONDEMNED engine ends the wait at its checkpoint even while it keeps moving: the
    // daemon is replacing it, and its relaunch's drain waits on this very turn (IntelMac
    // 2026-09-27: 25 and then 46 min dark while header waits rode a prefilling engine).
    let mut calls = 0u64;
    let condemned_later = || {
        calls += 1;
        let c = calls;
        async move {
            if c == 1 {
                EngineProbe::Progress(EngineProgress { fingerprint: c, any_processing: true })
            } else {
                EngineProbe::Condemned
            }
        }
    };
    let started = tokio::time::Instant::now();
    assert_eq!(wait_while_engine_progresses(std::future::pending::<&str>(), bound, 1, condemned_later, |_| {}).await, None);
    assert!(started.elapsed() <= bound, "condemned at the first checkpoint ends it there: {:?}", started.elapsed());
}
