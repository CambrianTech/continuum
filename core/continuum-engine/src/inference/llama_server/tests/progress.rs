//! Progress regression fixtures.
#![cfg(test)]
use super::*;

// what this catches (card 9c67e5fe; Fable on #5500): 'condemned' read from the readiness bit, so a
// transient not-ready (a 0 /props window under AlreadyServing) ended acts at their checkpoints and
// stalled queue waits. Condemned is a condemnation no ready engine has answered: a ready publish
// alone condemns nothing, and the replacement's ready answers every condemnation before it.
#[test]
fn condemned_means_a_condemnation_no_ready_engine_has_answered() {
    let condemnations = Condemnations::new();
    assert!(!condemnations.pending(), "nothing served, nothing condemned");
    condemnations.recovered();
    assert!(!condemnations.pending(), "a ready publish alone condemns nothing");
    condemnations.condemn();
    condemnations.condemn();
    assert!(condemnations.pending(), "decided teardowns are pending until a ready engine answers");
    condemnations.recovered();
    assert!(!condemnations.pending(), "the replacement's ready answers every condemnation before it");
}

// what this catches (card 4d2a64a2; Fable on #5500): a condemnation an in-flight wait cannot
// hear until its next checkpoint, and a wait that hears something that is not one. The wait
// resolves at the next condemnation after it began, not at one before it, and never on a
// closed channel.
#[tokio::test(start_paused = true)]
async fn a_wait_hears_the_next_condemnation_the_moment_it_happens() {
    let (tx, _) = tokio::sync::watch::channel(0u64);
    tx.send_replace(1); // a condemnation before the act began: not hers to hear
    let wait = tokio::spawn(until_condemned_on(tx.subscribe()));
    tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
    assert!(!wait.is_finished(), "an earlier condemnation does not end a later act");
    tx.send_modify(|generation| *generation += 1);
    tokio::time::timeout(std::time::Duration::from_millis(1), wait).await.unwrap().unwrap(); // unwrap: the wait must resolve at once

    let wait = tokio::spawn(until_condemned_on(tx.subscribe()));
    drop(tx);
    tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
    assert!(!wait.is_finished(), "a closed channel never condemns");
}
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

// what this catches (card 0b52b957, enrico 2026-10-26 07:59-08:59Z): a page save queued behind
// another slot's prefill refused as "engine stopped progressing" 15-17 s in, 3 of 3, each mind
// cold on her next turn. The engine prefilled one 2,048-token batch per ~130-200 s (10.4-15.4
// tok/s) and its `/slots` counters only move per batch, while the page switch checked every
// 16.8 s and called processing-and-unmoved stuck at the first checkpoint. With the window
// (two batches at the measured rate) the same wait rides the batch and gets its receipt; an
// engine that stays unmoved past the window still ends, inside it plus one checkpoint.
#[tokio::test(start_paused = true)]
async fn a_switch_behind_a_slow_batch_rides_it_and_a_stuck_engine_still_ends() {
    let checkpoint = std::time::Duration::from_millis(16_830);
    let bounds = WaitBounds { checkpoint, unmoved: window_at_rate(checkpoint, Some(10.4)), unmoved_rate: Some(10.4), unmoved_rate_source: "per_request" };
    assert!(bounds.unmoved > std::time::Duration::from_secs(200), "two batches at 10.4 tok/s: {:?}", bounds.unmoved);

    // the other slot's batch completes at 190 s (the counters move), then the save answers
    let batch_ends = std::time::Duration::from_secs(190);
    let started = tokio::time::Instant::now();
    let other_slot = move || async move {
        let fingerprint = if started.elapsed() < batch_ends { 2048 } else { 4096 };
        EngineProbe::Progress(EngineProgress { fingerprint, any_processing: true })
    };
    let save = async {
        tokio::time::sleep(std::time::Duration::from_secs(200)).await;
        "quote"
    };
    assert_eq!(wait_while_engine_progresses(save, bounds, 8, other_slot, |_| {}).await, Some("quote"),
        "a switch behind a 190 s batch is served, not refused");

    // the receipt wins whenever it lands, even mid-window with nothing moving
    let stuck = || async { EngineProbe::Progress(EngineProgress { fingerprint: 7, any_processing: true }) };
    let early = async {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        "receipt"
    };
    assert_eq!(wait_while_engine_progresses(early, bounds, 8, stuck, |_| {}).await, Some("receipt"));

    // processing and never moving: ends once the window has passed, at the next checkpoint
    let started = tokio::time::Instant::now();
    assert_eq!(wait_while_engine_progresses(std::future::pending::<&str>(), bounds, 8, stuck, |_| {}).await, None);
    let waited = started.elapsed();
    assert!(waited >= bounds.unmoved && waited <= bounds.unmoved + checkpoint, "stuck ends within one checkpoint of the window: {waited:?}");

    // a plain Duration keeps the strict reading: unmoved while processing is stuck at once
    let started = tokio::time::Instant::now();
    assert_eq!(wait_while_engine_progresses(std::future::pending::<&str>(), checkpoint, 8, stuck, |_| {}).await, None);
    assert!(started.elapsed() <= checkpoint, "a Duration bound is its own patience");
}

// what this catches (Codex and Cormac on #4506): a lane that prefills one 2,048-token chunk
// every ~176 s, with measured gaps up to 214 s, shows no counter move for most of a 90 s
// window while it is busy. The window stretches to TWO ubatches at the measured rate; a
// fast lane keeps the idle bound; no rate measured keeps the idle bound.
#[test]
fn the_work_window_is_two_ubatches_at_the_measured_prefill_rate() {
    let idle = std::time::Duration::from_secs(90);
    let slow = window_at_rate(idle, Some(11.6));
    assert!(slow > std::time::Duration::from_secs(214), "covers the slowest measured chunk gap (214 s): {slow:?}");
    assert!(slow < std::time::Duration::from_secs(360), "two chunks, not an open wait: {slow:?}");
    assert_eq!(window_at_rate(idle, Some(2_000.0)), idle, "a fast lane keeps the idle bound");
    assert_eq!(window_at_rate(idle, None), idle, "no rate measured keeps the idle bound");
    assert_eq!(window_at_rate(idle, Some(1e-300)), UNMOVED_CEILING, "a vanishing rate is capped, never a Duration overflow panic");
    // the ceiling caps the rate's part, never the caller's own floor (Codex on #5462)
    let above = UNMOVED_CEILING + std::time::Duration::from_secs(1);
    assert_eq!(window_at_rate(above, Some(1e-300)), above, "a floor above the ceiling stands");
}

// what this catches (card e1a5a21a, enrico 2026-10-26 13:15-14:15Z on 0eeceda84): the window
// sized from the server-total prefill rate (implied 64-174 tok/s) while fresh prefill ran at
// 9.7-16.2 tok/s, so 3 saves were refused 23-64 s into a ~125-210 s batch (two batches: 250-420 s). The window takes the
// slower measurement, names its source, and ignores an unusable one.
#[test]
fn the_window_is_sized_from_the_slower_measured_rate() {
    assert_eq!(window_rate(Some(68.0), Some(9.7)), (Some(9.7), "per_request"), "enrico: the per-request rate is the slower");
    assert_eq!(window_rate(Some(9.7), Some(68.0)), (Some(9.7), "server_total"), "whichever is slower");
    assert_eq!(window_rate(Some(68.0), None), (Some(68.0), "server_total"));
    assert_eq!(window_rate(None, Some(9.7)), (Some(9.7), "per_request"));
    assert_eq!(window_rate(Some(f64::NAN), Some(0.0)), (None, "none"), "neither usable: the floor");
    let checkpoint = std::time::Duration::from_millis(16_830);
    let (rate, _) = window_rate(Some(68.0), Some(9.7));
    assert!(window_at_rate(checkpoint, rate) > std::time::Duration::from_secs(400), "two 2,048-token batches at 9.7 tok/s");
    assert!(window_at_rate(checkpoint, Some(68.0)) < std::time::Duration::from_secs(65), "the old server-total window: about 60 s, shorter than one batch here");
}
