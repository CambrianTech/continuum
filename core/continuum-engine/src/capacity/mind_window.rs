//! The one determiner of a mind's window (card de7bb6e8; Joel, 2026-11-04: "need one centralized
//! determiner"). Every reader of a mind's window asks [`mind_window`]; none reads an adapter's
//! lane window or the serving snapshot as hers. On the M5 (0b2854327) two owners fought over
//! Aiko's window: the re-home reconciler bound her grant (20,817), then each turn adopted the
//! snapshot's per-sequence window (18,380) back into her binding, 15 rebinds in 12 minutes.

use crate::ai::adapter::AIProviderAdapter;
use crate::inference::planning::window_allocator::Tokens;

/// A mind's window, decided once per question: decided, or not granted yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MindWindow {
    /// On the shared pool with no grant yet: she has no window, and her turn waits for one on
    /// the grants watch ([`grants_waiter`]); never a provisional number.
    Ungranted,
    Decided(Decided),
}

/// A decided window and who decided it. It always has a size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decided {
    /// On the node's shared pool: her grant (`window_grant`).
    Granted(Tokens),
    /// A lane that reports its own window (a remote responder's slot).
    Lane(Tokens),
    /// A lane whose window is her binding's (a dedicated lane pinned from its own `/props`,
    /// cloud, in-process).
    Bound(u32),
}

impl Decided {
    pub fn tokens(self) -> u32 {
        match self {
            Self::Granted(t) | Self::Lane(t) => t.get(),
            Self::Bound(w) => w,
        }
    }
}

/// The window of mind `persona`, whose requests go to `adapter` and whose binding says `bound`.
pub fn mind_window(persona: uuid::Uuid, adapter: &dyn AIProviderAdapter, bound: u32) -> MindWindow {
    if adapter.serves_on_shared_pool() {
        if super::window_grant::is_grant_retiring(persona) { return MindWindow::Ungranted; }
        return match super::window_grant::current_grant(persona).and_then(Tokens::new) {
            Some(granted) => MindWindow::Decided(Decided::Granted(granted)),
            None => MindWindow::Ungranted,
        };
    }
    match adapter.live_served_window().and_then(Tokens::new) {
        Some(reported) => MindWindow::Decided(Decided::Lane(reported)),
        None => MindWindow::Decided(Decided::Bound(bound)),
    }
}

/// Decide her window and, when it is a grant, hold it for the act about to render against it
/// ([`super::window_grant::ActHold`]). A grant taken between the decision and the hold is waited for (the next grants
/// evaluation, then she decides again), never a failed turn (card d50e8e66).
pub async fn decide_and_hold<D, M, E, F>(persona: uuid::Uuid, decide: D, mut measure: M, evaluation_after: E) -> Result<(Decided, Option<super::window_grant::ActHold>), WindowRefused>
where
    D: Fn() -> MindWindow,
    M: FnMut(),
    E: Fn(std::time::Instant) -> Result<F, GrantsUnavailable>,
    F: std::future::Future<Output = Result<super::window_grant::GrantEvaluation, WindowRefused>>,
{
    loop {
        let decided = decide_window(persona, &decide, &mut measure, &evaluation_after).await?;
        if !matches!(decided, Decided::Granted(_)) {
            return Ok((decided, None));
        }
        let asked = std::time::Instant::now();
        match super::window_grant::hold_for_act(persona) {
            Some(hold) => return Ok((decided, Some(hold))),
            None => {
                continuum_base::probe!(
                    class = "delib.window.waiting",
                    persona = %persona,
                    reason = "her grant moved between her decision and her act",
                    "her turn waits for a later grants evaluation"
                );
                evaluation_after(asked)?.await?;
            }
        }
    }
}

/// No grants watch is installed in this process, so a mind on the shared pool can never be granted
/// a window here: a typed refusal, never a wait on a watch that cannot fire (Cormac on #5752).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrantsUnavailable;

impl std::fmt::Display for GrantsUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("no grants are published in this process: a mind on the shared pool has no window")
    }
}

/// Resolves when the serving daemon publishes new grants: what an ungranted mind waits on, and what a
/// mind on the shared pool watches for a changed window.
///
/// Take it BEFORE deciding (`grants_waiter`), so a grant published between the decision and the wait
/// still wakes her.
pub fn grants_waiter() -> Result<impl std::future::Future<Output = Result<(), GrantsUnavailable>>, GrantsUnavailable> {
    super::window_grant::grants_receiver().map(|rx| GrantsWaiter::new(rx).changed()).ok_or(GrantsUnavailable)
}

/// A wait for grants NEWER than those current when it was made. The receiver is a clone of the
/// installed one, which never marks a value seen, so the clone marks the current grants seen first:
/// without that, `changed()` resolves at once on every call, and an ungranted mind looped measure,
/// wake, ungranted with nothing changed (the M5 core at 339% CPU on d39469463, Atlas 751 times in a
/// minute).
struct GrantsWaiter(tokio::sync::watch::Receiver<Option<super::window_grant::PoolGrants>>);

impl GrantsWaiter {
    fn new(mut rx: tokio::sync::watch::Receiver<Option<super::window_grant::PoolGrants>>) -> Self {
        rx.borrow_and_update();
        Self(rx)
    }

    /// Resolves when newer grants are published; `GrantsUnavailable` when the sender is gone.
    async fn changed(mut self) -> Result<(), GrantsUnavailable> {
        self.0.changed().await.map_err(|_| GrantsUnavailable)
    }
}

/// Why an ungranted mind's turn does not wait on: no grants are published here, or the first grants
/// evaluation after her measure granted her nothing, and says why (card 5e79e1c1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowRefused {
    Unavailable(GrantsUnavailable),
    /// A deferral: her next tick measures and asks again; nothing was dispatched.
    Ungranted(super::window_grant::Ungranted),
    /// The publisher missed a beat of its own cadence (a stalled serving tick): named, never waited
    /// out to the act floor (Fable on #5890). `silent_for` is since its last evaluation began; `None`
    /// when it has never evaluated.
    PublisherSilent { silent_for: Option<std::time::Duration> },
}

impl std::fmt::Display for WindowRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(why) => why.fmt(f),
            Self::Ungranted(why) => write!(f, "no window granted after her measure: {why}"),
            Self::PublisherSilent { silent_for: Some(silent) } => {
                write!(f, "the grants publisher missed its cadence: no evaluation began in the last {silent:?}")
            }
            Self::PublisherSilent { silent_for: None } => f.write_str("the grants publisher has not evaluated yet"),
        }
    }
}

impl From<GrantsUnavailable> for WindowRefused {
    fn from(why: GrantsUnavailable) -> Self {
        Self::Unavailable(why)
    }
}

/// The first grants evaluation that BEGAN after `measured`, so it read her measure. The current value
/// is read first, so an evaluation published before this call is never lost. The wait is bounded by
/// the publisher's own cadence, carried on its evaluation: one missed beat (no evaluation began within
/// two cadences of the last) is `PublisherSilent`, so a stalled serving tick is named, not waited out.
pub fn evaluation_after(
    measured: std::time::Instant,
) -> Result<impl std::future::Future<Output = Result<super::window_grant::GrantEvaluation, WindowRefused>>, GrantsUnavailable> {
    super::window_grant::evaluations_receiver().map(|rx| next_evaluation(rx, measured)).ok_or(GrantsUnavailable)
}

fn next_evaluation(
    mut rx: tokio::sync::watch::Receiver<Option<super::window_grant::GrantEvaluation>>,
    measured: std::time::Instant,
) -> impl std::future::Future<Output = Result<super::window_grant::GrantEvaluation, WindowRefused>> {
    async move {
        loop {
            // Cloned out, so no watch borrow is held across the match or the wait.
            let latest = rx.borrow_and_update().clone();
            let (due, last) = match latest {
                Some(evaluation) if evaluation.started > measured => return Ok(evaluation),
                Some(evaluation) => (evaluation.started + evaluation.cadence * 2, evaluation.started),
                None => return Err(WindowRefused::PublisherSilent { silent_for: None }),
            };
            match tokio::time::timeout_at(tokio::time::Instant::from_std(due), rx.changed()).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) => return Err(WindowRefused::Unavailable(GrantsUnavailable)),
                Err(_) => return Err(WindowRefused::PublisherSilent { silent_for: Some(last.elapsed()) }),
            }
        }
    }
}

/// Decide her window. An ungranted mind is MEASURED once (`measure` builds her view, which records
/// her wish, so the next grants evaluation can grant her), then waits for the first evaluation that
/// began after her measure (Cormac on #5752). Granted there, she decides again. Waiting for room
/// ([`Ungranted::awaits_room`]: queued, or the engine relaunching), she waits for the next
/// evaluation with her demand standing: on the M5 (26960c09d) each queued turn was refused and
/// re-measured instead, 186 failed turns an hour. Any other reason refuses her turn with the
/// evaluation's own words, so a defect is named within one publisher cadence, never parked (card
/// 5e79e1c1). No evaluations are a refusal only when she is ungranted: a decided window needs none.
pub async fn decide_window<D, M, E, F>(persona: uuid::Uuid, decide: D, mut measure: M, evaluation_after: E) -> Result<Decided, WindowRefused>
where
    D: Fn() -> MindWindow,
    M: FnMut(),
    E: Fn(std::time::Instant) -> Result<F, GrantsUnavailable>,
    F: std::future::Future<Output = Result<super::window_grant::GrantEvaluation, WindowRefused>>,
{
    let mut measured = false;
    loop {
        match decide() {
            MindWindow::Decided(decided) => return Ok(decided),
            MindWindow::Ungranted => {
                let first = !measured;
                if first {
                    measure();
                    measured = true;
                }
                // After her measure: only an evaluation that began later has read her demand.
                let asked = std::time::Instant::now();
                if first {
                    // Her demand is recorded: evaluate it now, once. A later pass waits for the
                    // next event (an act's end, a pool step), never re-triggers an evaluation.
                    super::window_grant::demand_moved();
                }
                let evaluation = evaluation_after(asked)?.await?;
                match evaluation.ungranted(persona) {
                    Some(why) if !why.awaits_room() => return Err(WindowRefused::Ungranted(why)),
                    Some(why) => continuum_base::probe!(
                        class = "delib.window.waiting",
                        persona = %persona,
                        reason = %why,
                        "her turn waits for a later grants evaluation"
                    ),
                    // Her grant is listed but she is ungranted: it is being reclaimed for a ready mind.
                    // She holds nothing the reclaim takes (`Ungranted::awaits_room`), so she waits for
                    // the evaluation after it, within her turn.
                    None => continuum_base::probe!(
                        class = "delib.window.waiting",
                        persona = %persona,
                        reason = "her seat is being reclaimed",
                        "her turn waits for a later grants evaluation"
                    ),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::planning::window_allocator::test_tokens as t;
    use std::cell::Cell;

    fn grants(by_mind: &[(uuid::Uuid, u32)]) -> Option<super::super::window_grant::PoolGrants> {
        Some(super::super::window_grant::PoolGrants {
            pool_tokens: 200_000,
            per_sequence_cap: 200_000,
            by_mind: by_mind.iter().copied().collect(),
            ..Default::default()
        })
    }

    // what this catches (the M5 core at 339% CPU on d39469463): a waiter over grants that never
    // change resolving at once. A clone of a receiver that never marked a value seen did, so an
    // ungranted mind spun. Unchanged grants park the waiter; only a newer publish wakes it.
    #[tokio::test]
    async fn a_waiter_parks_on_unchanged_grants_and_wakes_on_a_newer_publish() {
        let (sender, installed) = tokio::sync::watch::channel(grants(&[]));
        let waiter = GrantsWaiter::new(installed.clone());
        let mut waiting = Box::pin(waiter.changed());
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(std::future::Future::poll(waiting.as_mut(), &mut cx).is_pending(), "unchanged grants: it parks");
        let _ = sender.send(grants(&[(uuid::Uuid::from_u128(4), 14_860)])); // the receiver is held above
        assert_eq!(waiting.await, Ok(()), "a newer publish wakes it");
    }

    fn evaluation(started: std::time::Instant, grants: Option<super::super::window_grant::PoolGrants>, nobody: Option<super::super::window_grant::NoGrants>) -> Option<super::super::window_grant::GrantEvaluation> {
        Some(super::super::window_grant::GrantEvaluation {
            seq: 1,
            started,
            cadence: std::time::Duration::from_secs(5),
            engine: None,
            grants,
            nobody,
            excluded: Default::default(),
            waiting: Default::default(),
        })
    }

    // what this catches (Cormac on #5752): a new mind that waits for a grant before anything measures
    // her, or that runs on an evaluation that began before her measure (it never read her). She is
    // measured, the stale evaluation is passed over, and the first one after her measure grants her.
    #[tokio::test]
    async fn an_ungranted_mind_is_measured_then_runs_at_the_first_evaluation_after_it() {
        let her = uuid::Uuid::from_u128(4);
        let (sender, installed) = tokio::sync::watch::channel(evaluation(std::time::Instant::now(), grants(&[]), None));
        let measured = Cell::new(0);
        let granted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let publisher = {
            let granted = granted.clone();
            tokio::spawn(async move {
                tokio::task::yield_now().await;
                granted.store(true, std::sync::atomic::Ordering::SeqCst);
                let after = std::time::Instant::now() + std::time::Duration::from_millis(1);
                sender.send(evaluation(after, grants(&[(her, 14_860)]), None)).expect("the waiter holds a receiver");
            })
        };
        let decided = decide_window(
            her,
            || match granted.load(std::sync::atomic::Ordering::SeqCst) {
                false => MindWindow::Ungranted,
                true => MindWindow::Decided(Decided::Granted(t(14_860))),
            },
            || measured.set(measured.get() + 1),
            |at| Ok(next_evaluation(installed.clone(), at)),
        )
        .await;
        publisher.await.expect("publisher");
        assert_eq!(decided, Ok(Decided::Granted(t(14_860))));
        assert_eq!(measured.get(), 1, "measured once, then granted");
    }

    // what this catches (card 5e79e1c1, the M5's 25-minute parks): a publisher that keeps evaluating
    // and keeps granting her nothing changes no grants, so a wait on the grants watch never woke. The
    // first evaluation after her measure ends the wait with its own reason.
    #[tokio::test]
    async fn an_evaluation_that_grants_her_nothing_ends_the_wait_with_its_reason() {
        use super::super::window_grant::{NoGrants, Ungranted};
        let her = uuid::Uuid::from_u128(4);
        let (sender, installed) = tokio::sync::watch::channel(evaluation(std::time::Instant::now(), None, Some(NoGrants::NotReady)));
        let publisher = tokio::spawn(async move {
            tokio::task::yield_now().await;
            let after = std::time::Instant::now() + std::time::Duration::from_millis(1);
            sender.send(evaluation(after, None, Some(NoGrants::ModelUnresolved))).expect("the waiter holds a receiver");
        });
        let refused = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            decide_window(her, || MindWindow::Ungranted, || {}, |at| Ok(next_evaluation(installed.clone(), at))),
        )
        .await
        .expect("one evaluation ends the wait");
        publisher.await.expect("publisher");
        assert_eq!(refused, Err(WindowRefused::Ungranted(Ungranted::Nobody(NoGrants::ModelUnresolved))), "the newer evaluation's reason, not the stale one's");
    }

    // what this catches (card d50e8e66; the M5 under P3, 26512dc03, 2026-11-16: Aris's turn ended three
    // times in fifteen minutes): her own seat being reclaimed (listed, but she is ungranted while it
    // retires) ended her turn with `Reclaiming`. She waits for the evaluation after the reclaim, within
    // her turn, measured once, and runs when it grants her.
    #[tokio::test]
    async fn a_mind_whose_seat_is_reclaimed_waits_within_her_turn_then_runs() {
        let her = uuid::Uuid::from_u128(5);
        let measured = Cell::new(0);
        let reseated = Cell::new(false);
        let evaluations = Cell::new(0);
        // each evaluation lists her seat (it is retiring, so she is ungranted); from the second on the
        // reclaim has finished and she is seated again
        let decided = decide_window(
            her,
            || match reseated.get() {
                false => MindWindow::Ungranted,
                true => MindWindow::Decided(Decided::Granted(t(30_000))),
            },
            || measured.set(measured.get() + 1),
            |at| {
                evaluations.set(evaluations.get() + 1);
                reseated.set(evaluations.get() >= 2);
                match evaluations.get() {
                    n if n > 3 => Err(GrantsUnavailable), // a wait that never ends would show here, not hang
                    _ => Ok(std::future::ready(evaluation(at, grants(&[(her, 30_000)]), None).ok_or(WindowRefused::Unavailable(GrantsUnavailable)))),
                }
            },
        )
        .await;
        assert_eq!(decided, Ok(Decided::Granted(t(30_000))), "she waited while her seat was reclaimed and ran");
        assert_eq!(measured.get(), 1, "her demand stood while she waited: measured once");
        assert_eq!(evaluations.get(), 2, "one evaluation saw her seat retiring, the next seated her");
    }

    // what this catches (Codex on #5890; the M5 on 26960c09d, 186 failed turns an hour): a demand
    // waiting for room refused as a failed turn, or reported as one that never arrived. She waits with
    // her demand standing, measured once, and runs when a later evaluation grants her.
    #[tokio::test]
    async fn a_waiting_mind_waits_her_turn_measured_once_then_runs() {
        use super::super::window_grant::{Ungranted, Waiting};
        let her = uuid::Uuid::from_u128(4);
        let queued_at = move |acting| {
            let mut queued = evaluation(std::time::Instant::now() + std::time::Duration::from_millis(1), grants(&[]), None).expect("evaluation");
            queued.waiting.insert(her, Waiting::BehindActs { acting });
            queued
        };
        let (sender, installed) = tokio::sync::watch::channel(Some(queued_at(2)));
        let measured = Cell::new(0);
        let granted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let publisher = {
            let granted = granted.clone();
            tokio::spawn(async move {
                for position in [2, 1] {
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    sender.send(Some(queued_at(position))).expect("the waiter holds a receiver");
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                granted.store(true, std::sync::atomic::Ordering::SeqCst);
                let after = std::time::Instant::now() + std::time::Duration::from_millis(1);
                sender.send(evaluation(after, grants(&[(her, 14_860)]), None)).expect("the waiter holds a receiver");
            })
        };
        let decided = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            decide_window(
                her,
                || match granted.load(std::sync::atomic::Ordering::SeqCst) {
                    false => MindWindow::Ungranted,
                    true => MindWindow::Decided(Decided::Granted(t(14_860))),
                },
                || measured.set(measured.get() + 1),
                |at| Ok(next_evaluation(installed.clone(), at)),
            ),
        )
        .await
        .expect("granted within the publisher's beats");
        publisher.await.expect("publisher");
        assert_eq!(decided, Ok(Decided::Granted(t(14_860))), "she waited her turn and ran");
        assert_eq!(measured.get(), 1, "her demand stood while she waited: measured once");
        assert!(Ungranted::Waiting(Waiting::BehindActs { acting: 2 }).awaits_room());
        assert!(Ungranted::Waiting(Waiting::ForReclaim).awaits_room(), "cells reclaimed for her: she waits for the receipt");
        assert!(!Ungranted::Waiting(Waiting::NeedAbovePool { need: 9, pool: 8 }).awaits_room(), "a need above the pool is a defect, named");
        assert!(Ungranted::Waiting(Waiting::Reclaiming).awaits_room(), "her seat being reclaimed: she waits within her turn (card d50e8e66)");
        let absent = evaluation(std::time::Instant::now(), grants(&[]), None).expect("evaluation");
        assert_eq!(absent.ungranted(her), Some(Ungranted::NotADemand), "no demand of hers reached it");
    }

    // what this catches (Fable on #5890): a stalled publisher (its tick blocked, its sender alive)
    // parking her to the act floor again. The last evaluation began before her measure and none
    // follows: one missed beat of its own cadence refuses, named; a publisher that never evaluated
    // refuses at once.
    #[tokio::test]
    async fn a_publisher_that_misses_its_cadence_is_named_not_waited_out() {
        let her = uuid::Uuid::from_u128(4);
        let mut stale = evaluation(std::time::Instant::now(), None, None).expect("evaluation");
        stale.cadence = std::time::Duration::from_millis(20);
        let (_sender, installed) = tokio::sync::watch::channel(Some(stale));
        let refused = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            decide_window(her, || MindWindow::Ungranted, || {}, |at| Ok(next_evaluation(installed.clone(), at))),
        )
        .await
        .expect("bounded by the publisher's cadence");
        assert!(matches!(refused, Err(WindowRefused::PublisherSilent { silent_for: Some(_) })), "{refused:?}");

        let (_sender, never) = tokio::sync::watch::channel(None);
        let refused = decide_window(her, || MindWindow::Ungranted, || {}, |at| Ok(next_evaluation(never.clone(), at))).await;
        assert_eq!(refused, Err(WindowRefused::PublisherSilent { silent_for: None }));
    }

    // what this catches: a turn hanging on a watch that can never fire, and a decided window refused
    // only because no watch exists.
    #[tokio::test]
    async fn no_evaluations_refuse_only_an_ungranted_mind() {
        type Never = std::future::Ready<Result<super::super::window_grant::GrantEvaluation, WindowRefused>>;
        let her = uuid::Uuid::from_u128(4);
        let refused = decide_window(her, || MindWindow::Ungranted, || {}, |_| Err::<Never, _>(GrantsUnavailable)).await;
        assert_eq!(refused, Err(WindowRefused::Unavailable(GrantsUnavailable)));
        let bound = decide_window(her, || MindWindow::Decided(Decided::Bound(8_192)), || {}, |_| Err::<Never, _>(GrantsUnavailable)).await;
        assert_eq!(bound, Ok(Decided::Bound(8_192)), "a bound window needs no evaluations");
    }

    // what this catches (card 250d7603; observed live on engine pid 33236, build 4f96d32b): a publisher
    // that keeps evaluating on its own cadence and keeps reporting her demand as Waiting::BehindActs
    // grants her nothing. Each single wait is bounded by the publisher's cadence (PublisherSilent),
    // but the loop across evaluations has no named bound of its own, so she parks to the act's
    // TICK_DEADLINE floor (1500s observed; lanes physically free: 2 of 3) instead of deferring at her
    // own named bound with her act budget intact. The orphan-acting defect that keeps BehindActs true
    // forever is pinned separately as its own evidence in fair/tests.rs; this pin does not depend on it
    // - ANY publisher behavior that keeps her ungranted across evaluations must end in a typed
    // deferral within LANE_WAIT_CEILING, never at the tick floor. Un-ignore with the fix (card
    // question 2: give the grants wait a bound derived like lane_wait_bound from the queue she is
    // joining - pure over measurements, so it is reachable within her act without wall-clock
    // dependence).
    #[tokio::test(start_paused = true)]
    #[ignore = "pins card 250d7603: an ungranted mind parks to the tick floor while every evaluation reports her BehindActs; un-ignore once decide_window defers at her named bound"]
    async fn a_starved_mind_defers_at_her_named_bound_not_the_tick_floor() {
        use super::super::window_grant::Waiting;
        let her = uuid::Uuid::from_u128(4);
        // The publisher's healthy beat, in story time: evaluations keep flowing (the live seat
        // re-reported BehindAct hundreds of times an hour), so PublisherSilent never fires on her.
        let beat = std::time::Duration::from_millis(500);
        let behind_acts = move |at: std::time::Instant| {
            let mut e = evaluation(at, grants(&[]), None).expect("evaluation");
            e.waiting.insert(her, Waiting::BehindActs { acting: 2 });
            e
        };
        // The state at park time: her demand reached the publisher and waits BehindAct.
        let (sender, installed) = tokio::sync::watch::channel(Some(behind_acts(std::time::Instant::now())));
        let publisher = tokio::spawn(async move {
            loop {
                tokio::time::sleep(beat).await;
                if sender.send(behind_acts(std::time::Instant::now() + std::time::Duration::from_millis(1))).is_err() {
                    return; // the waiter is gone: her bound fired, or this test gave up on her
                }
            }
        });
        let measured = Cell::new(0);
        let started_at = tokio::time::Instant::now(); // story time: the controlled clock
        let refused = tokio::time::timeout(
            crate::inference::resource_admission::LANE_WAIT_CEILING + beat * 2,
            decide_window(her, || MindWindow::Ungranted, || measured.set(measured.get() + 1), |at| Ok(next_evaluation(installed.clone(), at))),
        )
        .await
        .expect("she defers within her named bound instead of parking to the tick floor (card 250d7603)");
        publisher.abort();
        assert!(matches!(refused, Err(WindowRefused::Ungranted(_)), "a typed deferral - her next tick measures and asks again; nothing was dispatched: {refused:?}"));
        assert_eq!(measured.get(), 1, "her demand stood while she waited: measured once");
        let elapsed = started_at.elapsed();
        assert!(elapsed <= crate::inference::resource_admission::LANE_WAIT_CEILING + beat, "budget intact: deferred at her named bound ({elapsed:?}), not the act's tick floor (25 min)");
    }
}
