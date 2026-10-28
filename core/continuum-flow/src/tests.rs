#![cfg(test)]
//! The rules the flow keeps, as a validator that runs in milliseconds (SERVING-ENGINE §1c).
use super::*;
use std::time::{Duration, Instant};

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
