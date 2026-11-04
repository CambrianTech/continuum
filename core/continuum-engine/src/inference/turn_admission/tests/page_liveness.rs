#![cfg(test)]
//! A page switch judges its engine by the engine PROCESS's CPU when `/slots` cannot answer
//! (`turn_admission::page_liveness`). Every case here is the IntelMac, 2026-11-04 16:24:20Z: a
//! restore went uncertain while the engine's main thread was in `update_slots -> decode`, the
//! endpoint was quarantined and the engine replaced, and every resident cold-prefilled 24-32k
//! tokens at 20-28 tok/s.

use super::super::page_liveness::{await_receipt, Pending};
use super::*;
use crate::inference::llama_server::{window_at_rate, EngineProbe, EngineProgress, WaitBounds};
use crate::system_resources::process_tree::CpuSample;
use futures::FutureExt;
use std::time::Duration;

/// The IntelMac's wait: the floor checkpoint, and an unmoved window of two batches at the
/// 20 tok/s it prefilled at, through the one owner that sizes it.
fn intelmac_bounds() -> WaitBounds {
    let checkpoint = kv_page_wedge_bound(0);
    let rate = Some(20.0);
    WaitBounds { checkpoint, unmoved: window_at_rate(checkpoint, rate), unmoved_rate: rate, unmoved_rate_source: "per_request" }
}

/// A CPU source the test scripts: each read returns the next sample, the last one repeating.
fn scripted(samples: Vec<CpuSample>) -> PageLiveness {
    let mut read = 0usize;
    PageLiveness::new(Box::new(move || {
        let sample = samples.get(read).or(samples.last()).copied();
        read += 1;
        sample
    }))
}

/// What a busy engine's CPU counter moves over one checkpoint in these fixtures: a few cores'
/// worth of a 10 s checkpoint. Any nonzero amount is "advanced"; the value is only evidence.
const ENGINE_CPU_PER_CHECKPOINT: Duration = Duration::from_secs(38);

fn busy_cpu() -> PageLiveness {
    scripted(vec![CpuSample::Alive { spent: None }, CpuSample::Alive { spent: Some(ENGINE_CPU_PER_CHECKPOINT) }])
}

fn restore_landing_after(after: Duration) -> Pending<&'static str> {
    Box::pin(async move {
        tokio::time::sleep(after).await;
        "restored"
    })
}

// what this catches (the IntelMac 2026-11-04 16:24Z replacement): a restore queued behind one long
// CPU batch, `/slots` silent the whole time, its engine's process spending CPU. It must wait and
// land, past the quiet cap that replaced the engine; without the CPU read the same silence ends
// the wait at that cap, which is the replacement.
#[tokio::test(start_paused = true)]
async fn a_busy_engine_whose_slots_never_answer_finishes_its_switch() {
    let bounds = intelmac_bounds();
    let lands = bounds.checkpoint * 15;
    assert!(lands < bounds.unmoved, "the fixture lands inside the measured window: {bounds:?}");
    let liveness = busy_cpu();
    let mut checkpoints = 0;
    let got = await_receipt(restore_landing_after(lands), bounds, &liveness, || async { EngineProbe::Busy }, |n| checkpoints = n).await;
    assert!(matches!(got, Receipt::Answered("restored")), "a working engine's switch lands");
    assert!(checkpoints > 8, "it outlived the quiet cap, {checkpoints} checkpoints");
    assert_eq!(liveness.cpu_spent(), Some(ENGINE_CPU_PER_CHECKPOINT), "the busy probe carries the CPU time it read");

    let blind = PageLiveness::new(Box::new(|| None));
    let got = await_receipt(restore_landing_after(lands), bounds, &blind, || async { EngineProbe::Busy }, |_| {}).await;
    assert!(matches!(got, Receipt::Stopped), "HTTP alone reads the same engine dead: the bug this fixes");
}

// what this catches (the IntelMac 2026-11-04 16:24Z replacement, its other side): CPU is not a
// licence to wait forever. A silent engine that spent NO CPU across a checkpoint is stopped at
// that checkpoint, never carried to the quiet cap or the unmoved window.
#[tokio::test(start_paused = true)]
async fn a_silent_engine_with_flat_cpu_is_stopped_at_its_checkpoint() {
    let bounds = intelmac_bounds();
    let flat = scripted(vec![CpuSample::Alive { spent: None }, CpuSample::Alive { spent: Some(Duration::ZERO) }]);
    let started = tokio::time::Instant::now();
    let got = await_receipt(Box::pin(std::future::pending::<&str>()), bounds, &flat, || async { EngineProbe::Busy }, |_| {}).await;
    assert!(matches!(got, Receipt::Stopped), "silent and idle is stopped");
    assert_eq!(started.elapsed(), bounds.checkpoint, "at the first checkpoint");
}

// what this catches (the IntelMac 2026-11-04 16:24Z replacement, Codex's fence): a process that
// exited, or a pid that now names another process, is a dead engine AT ONCE — never "busy", even
// when something on its port still answers `/slots` with a moving fingerprint.
#[tokio::test(start_paused = true)]
async fn a_vanished_engine_process_ends_the_switch_at_once() {
    let bounds = intelmac_bounds();
    let gone_at_once = scripted(vec![CpuSample::Gone]);
    let started = tokio::time::Instant::now();
    let got = await_receipt(Box::pin(std::future::pending::<&str>()), bounds, &gone_at_once, || async { EngineProbe::Busy }, |_| {}).await;
    assert!(matches!(got, Receipt::Stopped));
    assert_eq!(started.elapsed(), Duration::ZERO, "gone at the first read ends before any checkpoint");

    let exits_midway = scripted(vec![CpuSample::Alive { spent: None }, CpuSample::Alive { spent: Some(ENGINE_CPU_PER_CHECKPOINT) }, CpuSample::Gone]);
    let mut tick = 0u64;
    let started = tokio::time::Instant::now();
    let got = await_receipt(
        Box::pin(std::future::pending::<&str>()),
        bounds,
        &exits_midway,
        || {
            tick += 1;
            let fingerprint = tick;
            async move { EngineProbe::Progress(EngineProgress { fingerprint, any_processing: true }) }
        },
        |_| {},
    )
    .await;
    assert!(matches!(got, Receipt::Stopped), "a moving fingerprint never outranks a dead incarnation");
    assert_eq!(started.elapsed(), bounds.checkpoint * 2, "the checkpoint that read it gone");
}

// what this catches (the IntelMac 2026-11-04 16:24Z replacement, Codex's bound): CPU that keeps
// moving while the switch never completes (a spin, or another slot's work) is evidence against
// replacing the engine, not progress. The wait ends at the measured unmoved window as UNRESOLVED,
// hands the request back still in flight, and that outcome does not replace the engine.
#[tokio::test(start_paused = true)]
async fn busy_cpu_without_a_receipt_is_unresolved_within_its_measured_bound() {
    let bounds = intelmac_bounds();
    let liveness = busy_cpu();
    let started = tokio::time::Instant::now();
    let got = await_receipt(Box::pin(std::future::pending::<&str>()), bounds, &liveness, || async { EngineProbe::Busy }, |_| {}).await;
    let waited = started.elapsed();
    let Receipt::Unresolved { cpu_spent, mut pending } = got else {
        panic!("test: busy CPU with no receipt must be unresolved, never stopped or endless");
    };
    assert_eq!(cpu_spent, ENGINE_CPU_PER_CHECKPOINT);
    assert!(waited >= bounds.unmoved && waited < bounds.unmoved + bounds.checkpoint * 2,
        "bounded by the measured window: waited {waited:?}, window {:?}", bounds.unmoved);
    assert!((&mut pending).now_or_never().is_none(), "the request is handed back in flight, not cancelled");
    assert!(!PageOutcome::Unresolved.requires_replacement(), "unresolved alone never replaces the engine");
    assert!(PageOutcome::Uncertain.requires_replacement());
}
