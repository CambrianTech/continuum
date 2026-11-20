#![cfg(test)]
//! The rules the flow keeps, as a validator that runs in milliseconds (CONTINUUM-ENGINE.md 4.1c).
use super::*;
use std::time::{Duration, Instant};

/// A reliable stage with one input waiting: the double buffer.
fn one() -> Capacity {
    Capacity::granted(std::num::NonZeroUsize::MIN)
}

fn aging() -> Aging {
    // Background rises a class every 10 s, utility every 5 s; work and interactive do not age.
    Aging([Duration::from_secs(10), Duration::from_secs(5), Duration::ZERO, Duration::ZERO])
}

fn at(base: Instant, secs: u64) -> Instant {
    base + Duration::from_secs(secs)
}

// what this catches: a directed reply waiting behind four residents' acts. It is not queued
// behind them: the lowest running work is asked to yield at once, and the reply starts on release.
#[test]
fn a_directed_reply_asks_the_lowest_work_to_yield_and_starts_on_release() {
    let t0 = Instant::now();
    let mut s = Scheduler::new(4, aging());
    for (i, mind) in ["aiko", "aris", "atlas", "benchy"].into_iter().enumerate() {
        s.submit(Ticket(i as u64), mind, Qos::Work, t0);
    }
    assert_eq!(s.decide(t0).len(), 4);
    s.submit(Ticket(9), "joel-asked-aiko", Qos::Interactive, at(t0, 1));
    let asked = s.decide(at(t0, 1));
    assert_eq!(asked, vec![Decision::Yield(Ticket(3))], "the newest-started work yields first");
    assert!(s.decide(at(t0, 1)).is_empty(), "asked once, not every event");
    s.release(Ticket(3));
    assert_eq!(s.decide(at(t0, 1)), vec![Decision::Run(Ticket(9))]);
}

// what this catches: background work taking a free slot ahead of a resident's act.
#[test]
fn work_never_waits_behind_a_dream() {
    let t0 = Instant::now();
    let mut s = Scheduler::new(1, aging());
    s.submit(Ticket(1), "dream", Qos::Background, t0);
    s.submit(Ticket(2), "aiko", Qos::Work, at(t0, 1));
    assert_eq!(s.decide(at(t0, 1)), vec![Decision::Run(Ticket(2))]);
}

// what this catches: a dream starved forever on a busy node. It ages to `Work`, then wins on
// age the next time a slot frees: it runs irregularly, and it never preempts work to do so.
#[test]
fn a_dream_still_runs_on_a_busy_node_without_preempting_work() {
    let t0 = Instant::now();
    let mut s = Scheduler::new(1, aging());
    s.submit(Ticket(1), "dream", Qos::Background, t0);
    s.submit(Ticket(2), "aiko", Qos::Work, t0);
    assert_eq!(s.decide(t0), vec![Decision::Run(Ticket(2))]);
    s.submit(Ticket(3), "aris", Qos::Work, at(t0, 25));
    assert!(s.decide(at(t0, 25)).is_empty(), "an aged dream never asks work to yield");
    s.release(Ticket(2));
    assert_eq!(s.decide(at(t0, 25)), vec![Decision::Run(Ticket(1))], "aged to work, and older than aris");
    s.release(Ticket(1));
    assert_eq!(s.decide(at(t0, 26)), vec![Decision::Run(Ticket(3))]);
}

// what this catches: an aged ticket passing a directed reply or preempting anyone.
#[test]
fn aging_stops_at_work() {
    let t0 = Instant::now();
    let mut s = Scheduler::new(1, aging());
    s.submit(Ticket(1), "dream", Qos::Background, t0);
    s.submit(Ticket(2), "aiko", Qos::Interactive, at(t0, 1000));
    assert_eq!(s.decide(at(t0, 1000)), vec![Decision::Run(Ticket(2))]);
}

// what this catches: one mind running two acts at once (each owner is a serial queue), and a
// free slot left idle while another owner's work waits.
#[test]
fn an_owner_runs_one_unit_at_a_time_and_others_fill_the_rest() {
    let t0 = Instant::now();
    let mut s = Scheduler::new(3, aging());
    s.submit(Ticket(1), "aiko", Qos::Work, t0);
    s.submit(Ticket(3), "aris", Qos::Utility, t0);
    assert_eq!(s.decide(t0).len(), 2);
    s.submit(Ticket(2), "aiko", Qos::Interactive, t0);
    assert!(s.decide(t0).is_empty(), "aiko's next unit waits for her first, though a slot is free");
    s.release(Ticket(1));
    assert_eq!(s.decide(t0), vec![Decision::Run(Ticket(2))]);
}

// what this catches: a mind's queue growing. Her newer work replaces her waiting work, keeps
// her place in line and the higher class, and the replaced ticket is told why it left.
#[test]
fn a_newer_submission_supersedes_the_owners_waiting_one_and_keeps_her_place() {
    let t0 = Instant::now();
    let mut s = Scheduler::new(1, aging());
    s.submit(Ticket(1), "aris", Qos::Work, t0);
    assert_eq!(s.decide(t0), vec![Decision::Run(Ticket(1))]);
    s.submit(Ticket(2), "aiko", Qos::Interactive, at(t0, 1));
    s.submit(Ticket(3), "atlas", Qos::Work, at(t0, 2));
    assert_eq!(s.submit(Ticket(4), "aiko", Qos::Work, at(t0, 5)), vec![(Ticket(2), Dropped::Superseded)]);
    assert_eq!(s.waiting(), 2, "aiko still has one waiting unit, not two");
    s.release(Ticket(1));
    assert_eq!(s.decide(at(t0, 5)), vec![Decision::Run(Ticket(4))], "interactive class and her t+1 place kept");
}

// what this catches: a background backlog building up behind busy work. Past its bound the
// oldest background work is shed, and work is never shed.
#[test]
fn a_full_background_queue_sheds_its_oldest_and_never_sheds_work() {
    let t0 = Instant::now();
    let mut s = Scheduler::new(1, aging());
    s.submit(Ticket(1), "aris", Qos::Work, t0);
    s.decide(t0);
    assert!(s.submit(Ticket(2), "dream-a", Qos::Background, t0).is_empty());
    assert_eq!(s.submit(Ticket(3), "dream-b", Qos::Background, at(t0, 1)), vec![(Ticket(2), Dropped::Shed)]);
    for (i, mind) in ["aiko", "atlas", "benchy"].into_iter().enumerate() {
        assert!(s.submit(Ticket(10 + i as u64), mind, Qos::Work, at(t0, 1)).is_empty(), "work is bounded by its owners, never shed");
    }
    assert_eq!(s.waiting(), 4);
}

// what this catches: a superseded submitter waiting forever. Through the real dispatcher it
// resolves at once with the reason.
#[tokio::test(start_paused = true)]
async fn a_superseded_submission_resolves_at_once_with_its_reason() {
    let flow: Flow<&'static str> = Flow::spawn(1, aging());
    let _busy = flow.submit("aris", Qos::Work).await;
    let first = tokio::spawn({
        let flow = flow.clone();
        async move { flow.submit("aiko", Qos::Work).await.err() }
    });
    tokio::task::yield_now().await;
    let _second = tokio::spawn({
        let flow = flow.clone();
        async move { flow.submit("aiko", Qos::Work).await.is_ok() }
    });
    let why = tokio::time::timeout(Duration::from_secs(1), first).await;
    assert!(matches!(why, Ok(Ok(Some(Refused::Dropped(Dropped::Superseded))))), "{why:?}");
}

// what this catches: a dispatcher that polls. It sleeps until the next aging step, or forever
// when nothing can still age.
#[test]
fn the_dispatcher_wakes_only_at_the_next_aging_step() {
    let t0 = Instant::now();
    let mut s = Scheduler::new(0, aging());
    assert_eq!(s.next_wakeup(t0), None);
    s.submit(Ticket(1), "dream", Qos::Background, t0);
    assert_eq!(s.next_wakeup(at(t0, 3)), Some(at(t0, 10)));
    assert_eq!(s.next_wakeup(at(t0, 21)), None, "at work it no longer ages");
    s.submit(Ticket(2), "aiko", Qos::Work, t0);
    assert_eq!(s.next_wakeup(at(t0, 21)), None, "work does not age");
}

// what this catches: a submitter that stops waiting keeping its unit, and a dropped grant
// not freeing capacity; through the real dispatcher task, not the state machine alone.
#[tokio::test(start_paused = true)]
async fn dropping_a_grant_or_a_waiting_submission_frees_the_unit() {
    let flow: Flow<&'static str> = Flow::spawn(1, aging());
    let held = flow.submit("aiko", Qos::Work).await;
    assert!(held.is_ok());
    let waiting = tokio::spawn({
        let flow = flow.clone();
        async move { flow.submit("aris", Qos::Work).await.is_ok() }
    });
    tokio::task::yield_now().await;
    waiting.abort();
    drop(held);
    let next = tokio::time::timeout(Duration::from_secs(1), flow.submit("atlas", Qos::Work)).await;
    assert!(matches!(next, Ok(Ok(_))), "neither the aborted wait nor the dropped grant kept the unit");
}

// A queued device request must leave independent capacity usable until its device is released.
#[tokio::test(start_paused = true)]
async fn independent_capacity_completes_while_device_work_is_pending() {
    let scenario = async {
        let device = Flow::spawn(1, aging());
        let cpu = Flow::spawn(1, aging());
        let mut holding = device.submit("decode", Qos::Background).await.expect("live device"); // expect: test owns dispatcher
        let pending = tokio::spawn(async move { device.submit("reply", Qos::Interactive).await });
        holding.yield_requested().await;

        let available = cpu.submit("prepare", Qos::Work).await.expect("free capacity"); // expect: test owns dispatcher
        drop(available);
        assert!(!pending.is_finished());
        drop(holding);
        assert!(pending.await.expect("device request joined").is_ok()); // expect: test owns task
    };
    assert!(tokio::time::timeout(Duration::from_secs(1), scenario).await.is_ok());
}

// what this catches: a running low class never told to yield through the dispatcher.
#[tokio::test(start_paused = true)]
async fn a_running_dream_hears_the_yield_when_a_reply_arrives() {
    let flow: Flow<&'static str> = Flow::spawn(1, aging());
    let mut dream = flow.submit("dream", Qos::Background).await.expect("dispatcher running"); // expect: the test owns the live dispatcher
    let reply = tokio::spawn({
        let flow = flow.clone();
        async move { flow.submit("aiko", Qos::Interactive).await.is_ok() }
    });
    tokio::time::timeout(Duration::from_secs(1), dream.yield_requested()).await.expect("asked to yield"); // expect: the assertion itself
    drop(dream);
    assert!(reply.await.expect("reply task")); // expect: the assertion itself
}

// what this catches (Codex on the FLOW plan): a tool result riding a queue that drops. A reliable
// stage hands a result back when full and never loses one; only a signal stage drops, and only
// its oldest wakes.
#[tokio::test(start_paused = true)]
async fn a_reliable_stage_never_drops_a_result_and_a_signal_stage_keeps_the_newest() {
    use std::sync::{Arc, Mutex};
    struct Results(Arc<Mutex<Vec<u32>>>, Arc<tokio::sync::Notify>);
    impl Stage for Results {
        type Input = u32;
        const QOS: Qos = Qos::Work;
        async fn handle(&mut self, input: u32) {
            self.1.notified().await; // hold the worker so the pending slot fills
            self.0.lock().unwrap_or_else(|e| e.into_inner()).push(input); // test: plain data
        }
    }
    struct Wakes(Arc<Mutex<Vec<u32>>>, Arc<tokio::sync::Notify>);
    impl Stage for Wakes {
        type Input = u32;
        const QOS: Qos = Qos::Utility;
        const OVERFLOW: Overflow = Overflow::KeepNewest;
        async fn handle(&mut self, input: u32) {
            self.1.notified().await;
            self.0.lock().unwrap_or_else(|e| e.into_inner()).push(input); // test: plain data
        }
    }
    let (got, gate) = (Arc::new(Mutex::new(Vec::new())), Arc::new(tokio::sync::Notify::new()));
    let (results, _results_worker) = spawn_stage(Results(Arc::clone(&got), Arc::clone(&gate)), one());
    assert_eq!(results.push(1), Ok(0));
    tokio::task::yield_now().await; // the worker takes 1 and waits on the gate
    assert_eq!(results.push(2), Ok(0));
    assert_eq!(results.push(3), Err(crate::PushError::Full(3)), "full: push hands the result back, nothing is dropped");
    let waiting = results.clone();
    let sent = tokio::spawn(async move { waiting.send(3).await });
    tokio::task::yield_now().await;
    assert!(!sent.is_finished(), "a reliable producer waits for the slot instead of spinning or dropping");
    for _ in 0..3 {
        gate.notify_one();
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
    }
    assert_eq!(sent.await.expect("test: the sender task"), Ok(()));
    assert_eq!(*got.lock().unwrap_or_else(|e| e.into_inner()), vec![1, 2, 3], "the awaited result is delivered"); // test: plain data

    let (seen, hold) = (Arc::new(Mutex::new(Vec::new())), Arc::new(tokio::sync::Notify::new()));
    let (wakes, _wakes_worker) = spawn_stage(Wakes(Arc::clone(&seen), Arc::clone(&hold)), Capacity::latest());
    assert_eq!(wakes.push(1), Ok(0));
    tokio::task::yield_now().await;
    assert_eq!(wakes.push(2), Ok(0));
    assert_eq!(wakes.push(3), Ok(1), "a signal stage drops its oldest wake");
    for _ in 0..2 {
        hold.notify_one();
        tokio::task::yield_now().await;
    }
    assert_eq!(*seen.lock().unwrap_or_else(|e| e.into_inner()), vec![1, 3], "the newest wake is handled"); // test: plain data
}

// what this catches: a token stream turned back between handles. A stage that declares a depth
// takes that many waiting inputs before handing one back, and its worker takes everything waiting
// in one wake, in order (CBAR's queueSize; Joel 2026-11-19: "on a turn do many").
#[tokio::test(start_paused = true)]
async fn a_deep_stage_queues_its_depth_and_handles_all_waiting_in_one_wake_in_order() {
    use std::sync::{Arc, Mutex};
    struct Stream(Arc<Mutex<Vec<Vec<u32>>>>, Arc<tokio::sync::Notify>);
    impl Stage for Stream {
        type Input = u32;
        const QOS: Qos = Qos::Interactive;
        async fn handle(&mut self, _input: u32) {}
        async fn handle_all(&mut self, inputs: Vec<u32>) {
            self.1.notified().await; // hold the worker so the queue fills behind it
            self.0.lock().unwrap_or_else(|e| e.into_inner()).push(inputs); // test: plain data
        }
    }
    let (wakes, gate) = (Arc::new(Mutex::new(Vec::new())), Arc::new(tokio::sync::Notify::new()));
    let (stream, _stream_worker) = spawn_stage(Stream(Arc::clone(&wakes), Arc::clone(&gate)), Capacity::granted(std::num::NonZeroUsize::new(3).expect("test: three")));
    assert_eq!(stream.push(1), Ok(0));
    tokio::task::yield_now().await; // the worker takes [1] and waits on the gate
    for token in 2..=4 {
        assert_eq!(stream.push(token), Ok(0), "a depth-3 stage queues three behind the one in hand");
    }
    assert_eq!(stream.push(5), Err(crate::PushError::Full(5)), "the fourth waiting input is handed back, never dropped");
    for _ in 0..2 {
        gate.notify_one();
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
    }
    assert_eq!(*wakes.lock().unwrap_or_else(|e| e.into_inner()), vec![vec![1], vec![2, 3, 4]], "everything waiting is handled in one wake, in order"); // test: plain data
}

// Last producers may close concurrently while the worker sleeps or drains accepted input.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn simultaneous_last_producers_close_and_drain_the_stage() {
    use std::sync::{Arc, Barrier};
    struct Drain {
        seen: Vec<u32>,
        ended: Option<tokio::sync::oneshot::Sender<Vec<u32>>>,
    }
    impl Stage for Drain {
        type Input = u32;
        const QOS: Qos = Qos::Work;
        async fn handle(&mut self, input: u32) { self.seen.push(input); }
    }
    impl Drop for Drain {
        fn drop(&mut self) {
            if let Some(ended) = self.ended.take() {
                let _ = ended.send(std::mem::take(&mut self.seen));
            }
        }
    }
    for queued in [false, true] {
        let (ended, done) = tokio::sync::oneshot::channel();
        let (first, _first_worker) = spawn_stage(Drain { seen: Vec::new(), ended: Some(ended) }, one());
        let second = first.clone();
        tokio::task::yield_now().await;
        let barrier = Arc::new(Barrier::new(2));
        std::thread::scope(|scope| {
            let barrier = &barrier;
            scope.spawn(move || {
                barrier.wait();
                if queued { assert_eq!(first.push(1), Ok(0)); }
                drop(first);
            });
            scope.spawn(move || {
                barrier.wait();
                drop(second);
            });
        });
        let mut seen = tokio::time::timeout(Duration::from_secs(2), done)
            .await.expect("test: the closed stage must terminate")
            .expect("test: stage drop reports drained input");
        seen.sort_unstable();
        assert_eq!(seen, if queued { vec![1] } else { vec![] });
    }
}

// what this catches (Cormac on #5729): a handler that panics leaving the stage looking full forever,
// so a waiting producer hangs and a pushing one retries a slot nothing will take.
#[tokio::test]
async fn a_stage_whose_handler_panicked_hands_inputs_back() {
    struct Breaks;
    impl Stage for Breaks {
        type Input = u32;
        const QOS: Qos = Qos::Work;
        async fn handle(&mut self, input: u32) {
            assert_ne!(input, 1, "test: the handler breaks on its first input");
        }
    }
    let (stage, _stage_worker) = spawn_stage(Breaks, one());
    assert_eq!(stage.push(1), Ok(0));
    for _ in 0..50 {
        tokio::task::yield_now().await;
        if stage.push(2) == Err(crate::PushError::Stopped(2)) {
            break;
        }
    }
    assert_eq!(stage.push(2), Err(crate::PushError::Stopped(2)), "a stopped stage keeps nothing");
    assert_eq!(stage.send(3).await, Err(crate::Stopped(3)), "a sender gets its input back instead of waiting");
}

// A producer can replace a heavy payload and close intake while its destructor is still pending.
#[tokio::test]
async fn eviction_and_final_cleanup_belong_to_the_joined_worker() {

    struct Payload(std::thread::ThreadId, std::sync::mpsc::Sender<bool>);
    impl Drop for Payload {
        fn drop(&mut self) {
            let _ = self.1.send(std::thread::current().id() != self.0);
        }
    }
    struct Consumer;
    impl Stage for Consumer {
        type Input = Payload;
        const QOS: Qos = Qos::Utility;
        const OVERFLOW: Overflow = Overflow::KeepNewest;
        async fn handle(&mut self, _: Payload) {}
    }
    let (input, worker) = spawn_stage(Consumer, Capacity::latest());
    let (released, observed) = std::sync::mpsc::channel();
    // The current-thread executor cannot poll the worker until the producer returns.
    std::thread::scope(|scope| {
        scope.spawn(move || {
            let payload = || Payload(std::thread::current().id(), released.clone());
            assert!(matches!(input.push(payload()), Ok(0)));
            assert!(matches!(input.push(payload()), Ok(1)));
        });
    });
    assert!(observed.try_recv().is_err());
    worker.stop();
    worker.join().await.expect("test: worker drained successfully");
    assert_eq!(observed.into_iter().collect::<Vec<_>>(), [true, true]);
}

// what this catches (Codex, 2026-11-19, on the granted capacity): a stalled sink losing or reordering
// what its producer could not hand over. A producer whose stage is full keeps the rest in its outbox
// against the same grant, and `drain` resumes it only on the stage's space, never on a poll: a stalled
// answer, then the next answer's items, arrive whole and in order once the sink resumes.
#[tokio::test(start_paused = true)]
async fn a_stalled_stage_holds_its_producer_and_drain_resumes_it_on_space_in_order() {
    use std::sync::{Arc, Mutex};
    struct Sink(Arc<Mutex<Vec<u32>>>, Arc<tokio::sync::Notify>);
    impl Stage for Sink {
        type Input = u32;
        const QOS: Qos = Qos::Interactive;
        async fn handle(&mut self, input: u32) {
            self.1.notified().await; // a stalled caller: each item waits for the gate
            self.0.lock().unwrap_or_else(|e| e.into_inner()).push(input); // test: plain data
        }
    }
    let (got, gate) = (Arc::new(Mutex::new(Vec::new())), Arc::new(tokio::sync::Notify::new()));
    let (input, worker) = spawn_stage(Sink(Arc::clone(&got), Arc::clone(&gate)), one());
    let mut outbox = crate::Outbox::new(input);
    for item in 1..=4 {
        outbox.send(item); // the first answer's end and the next answer's first items
    }
    tokio::task::yield_now().await;
    assert!(outbox.waiting() > 0, "a stage of one holds the rest in its producer's outbox, never drops them");
    let resumed = async {
        for _ in 0..4 {
            gate.notify_one();
            tokio::task::yield_now().await;
        }
    };
    tokio::join!(outbox.drain(), resumed);
    assert_eq!(outbox.waiting(), 0, "drain hands over everything once the stage takes it");
    drop(outbox);
    worker.join().await.expect("test: the worker drained");
    assert_eq!(*got.lock().unwrap_or_else(|e| e.into_inner()), vec![1, 2, 3, 4], "every item arrives once, in order"); // test: plain data
}

// what this catches (Codex, 2026-11-19: "completion, next turn and cancellation under one stalled
// sink"): an answer's end overtaking its tokens, the next answer interleaving with the last, or a
// cancellation the stalled consumer raised being missed by a held producer. Against one grant of one,
// the producer (the engine thread's shape: send, and check the answer's cancel flag before each step)
// hands over answer 1 whole, then answer 2 until the consumer cancels it, then answer 2's end.
#[tokio::test(start_paused = true)]
async fn a_stalled_sink_keeps_completion_next_turn_and_cancellation_whole_and_in_order() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Out {
        Token(u8, u32),
        End(u8),
    }
    struct Sink(Arc<Mutex<Vec<Out>>>, Arc<tokio::sync::Notify>, Arc<AtomicBool>);
    impl Stage for Sink {
        type Input = Out;
        const QOS: Qos = Qos::Interactive;
        async fn handle(&mut self, out: Out) {
            self.1.notified().await; // a stalled caller: each item waits for the gate
            if out == Out::Token(2, 1) {
                self.2.store(true, Ordering::Release); // the caller hung up on answer 2: cancel
            }
            self.0.lock().unwrap_or_else(|e| e.into_inner()).push(out); // test: plain data
        }
    }
    let (got, gate, cancel) = (Arc::new(Mutex::new(Vec::new())), Arc::new(tokio::sync::Notify::new()), Arc::new(AtomicBool::new(false)));
    let (input, worker) = spawn_stage(Sink(Arc::clone(&got), Arc::clone(&gate), Arc::clone(&cancel)), one());
    let mut outbox = crate::Outbox::new(input);
    let release = async {
        // Released one item at a time until the sink has handled answer 2's end: a Notify holds one
        // permit, so a fixed count could coalesce and leave the sink waiting.
        while !got.lock().unwrap_or_else(|e| e.into_inner()).contains(&Out::End(2)) { // test: plain data
            gate.notify_one();
            tokio::task::yield_now().await;
        }
    };
    let produce = async {
        for t in 0..3 {
            outbox.send(Out::Token(1, t));
        }
        outbox.send(Out::End(1)); // completion: answer 1's end, behind its tokens
        for t in 0..8 {
            if cancel.load(Ordering::Acquire) {
                break; // the engine reads the flag at its next step and decodes no more of her
            }
            outbox.send(Out::Token(2, t));
            outbox.drain().await; // held while the sink is stalled; resumed on its space
        }
        outbox.send(Out::End(2));
        outbox.drain().await;
    };
    tokio::join!(produce, release);
    drop(outbox);
    worker.join().await.expect("test: the worker drained");
    let got = got.lock().unwrap_or_else(|e| e.into_inner()).clone(); // test: plain data
    let end_1 = got.iter().position(|o| *o == Out::End(1)).expect("test: answer 1 ended");
    assert_eq!(&got[..=end_1], &[Out::Token(1, 0), Out::Token(1, 1), Out::Token(1, 2), Out::End(1)], "answer 1 arrives whole, its end after its tokens");
    assert!(got[end_1 + 1..].iter().all(|o| matches!(o, Out::Token(2, _) | Out::End(2))), "the next turn never interleaves with the last");
    assert_eq!(got.last(), Some(&Out::End(2)), "a cancelled answer still ends, once, last");
    let after_cancel = got.iter().filter(|o| matches!(o, Out::Token(2, t) if *t > 2)).count();
    assert_eq!(after_cancel, 0, "the held producer saw the cancellation within one step of it");
}

// what this catches (Codex on f530b00670): a producer blocked in `drain` behind a stalled sink that
// a new submission or the stop's Suspend cannot wake, and a drain dropped mid-wait losing an item.
// The driver's wait is one `select!` over its queue and the stage's space; a submission arriving
// after the drain began wakes it at once, and the items it held all arrive later, in order, once.
#[tokio::test(start_paused = true)]
async fn a_submission_wakes_a_producer_held_by_a_stalled_sink_and_the_drain_loses_nothing() {
    use std::sync::{Arc, Mutex};
    struct Sink(Arc<Mutex<Vec<u32>>>, Arc<tokio::sync::Notify>);
    impl Stage for Sink {
        type Input = u32;
        const QOS: Qos = Qos::Interactive;
        async fn handle(&mut self, item: u32) {
            self.1.notified().await; // held until the test releases it
            self.0.lock().unwrap_or_else(|e| e.into_inner()).push(item); // test: plain data
        }
    }
    let (got, gate) = (Arc::new(Mutex::new(Vec::new())), Arc::new(tokio::sync::Notify::new()));
    let (input, worker) = spawn_stage(Sink(Arc::clone(&got), Arc::clone(&gate)), one());
    let mut outbox = crate::Outbox::new(input);
    for item in 1..=4 {
        outbox.send(item);
    }
    let (submit, mut queue) = tokio::sync::mpsc::unbounded_channel::<&'static str>();
    let late = tokio::spawn(async move {
        tokio::task::yield_now().await; // after the drain has begun waiting
        submit.send("suspend").expect("test: the queue is open");
    });
    let woke = tokio::select! {
        biased;
        submission = queue.recv() => submission,
        () = outbox.drain() => None,
    };
    late.await.expect("test: the submitter ran");
    assert_eq!(woke, Some("suspend"), "the stop's suspend wakes the held producer while the sink is stalled");
    assert!(got.lock().unwrap_or_else(|e| e.into_inner()).is_empty(), "the sink never moved: the wake was the submission"); // test: plain data
    let release = async {
        while got.lock().unwrap_or_else(|e| e.into_inner()).len() < 4 { // test: plain data
            gate.notify_one();
            tokio::task::yield_now().await;
        }
    };
    tokio::join!(outbox.drain(), release);
    drop(outbox);
    worker.join().await.expect("test: the worker drained");
    assert_eq!(*got.lock().unwrap_or_else(|e| e.into_inner()), vec![1, 2, 3, 4], "the cancelled drain lost nothing and kept the order"); // test: plain data
}
