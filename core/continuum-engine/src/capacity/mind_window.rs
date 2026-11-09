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
}

impl std::fmt::Display for WindowRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(why) => why.fmt(f),
            Self::Ungranted(why) => write!(f, "no window granted after her measure: {why}"),
        }
    }
}

impl From<GrantsUnavailable> for WindowRefused {
    fn from(why: GrantsUnavailable) -> Self {
        Self::Unavailable(why)
    }
}

/// The first grants evaluation that BEGAN after `measured`, so it read her measure. `wait_for` checks
/// the current value first, so an evaluation published before this call is never lost.
pub fn evaluation_after(
    measured: std::time::Instant,
) -> Result<impl std::future::Future<Output = Result<super::window_grant::GrantEvaluation, GrantsUnavailable>>, GrantsUnavailable> {
    super::window_grant::evaluations_receiver().map(|rx| next_evaluation(rx, measured)).ok_or(GrantsUnavailable)
}

fn next_evaluation(
    mut rx: tokio::sync::watch::Receiver<Option<super::window_grant::GrantEvaluation>>,
    measured: std::time::Instant,
) -> impl std::future::Future<Output = Result<super::window_grant::GrantEvaluation, GrantsUnavailable>> {
    async move {
        rx.wait_for(|evaluation| evaluation.as_ref().is_some_and(|e| e.started > measured))
            .await
            .map_err(|_| GrantsUnavailable)?
            .clone()
            .ok_or(GrantsUnavailable)
    }
}

/// Decide her window. An ungranted mind is MEASURED first (`measure` builds her view, which records
/// her wish, so the next grants evaluation can grant her), then waits for the first evaluation that
/// began after her measure (Cormac on #5752). Granted there, she decides again; not granted, her turn
/// is refused with the evaluation's own reason, so the wait is one publisher cadence, never the act
/// floor (card 5e79e1c1: the grants watch fires only on CHANGE, so a mind never granted parked 25 min).
/// No evaluations are a refusal only when she is ungranted: a decided window needs none.
pub async fn decide_window<D, M, E, F>(persona: uuid::Uuid, decide: D, mut measure: M, evaluation_after: E) -> Result<Decided, WindowRefused>
where
    D: Fn() -> MindWindow,
    M: FnMut(),
    E: Fn(std::time::Instant) -> Result<F, GrantsUnavailable>,
    F: std::future::Future<Output = Result<super::window_grant::GrantEvaluation, GrantsUnavailable>>,
{
    loop {
        match decide() {
            MindWindow::Decided(decided) => return Ok(decided),
            MindWindow::Ungranted => {
                measure();
                let evaluation = evaluation_after(std::time::Instant::now())?.await?;
                if let Some(why) = evaluation.ungranted(persona) {
                    return Err(WindowRefused::Ungranted(why));
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
        Some(super::super::window_grant::GrantEvaluation { seq: 1, started, engine: None, grants, nobody, excluded: Default::default() })
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
            sender.send(evaluation(after, None, Some(NoGrants::PoolKindUnknown))).expect("the waiter holds a receiver");
        });
        let refused = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            decide_window(her, || MindWindow::Ungranted, || {}, |at| Ok(next_evaluation(installed.clone(), at))),
        )
        .await
        .expect("one evaluation ends the wait");
        publisher.await.expect("publisher");
        assert_eq!(refused, Err(WindowRefused::Ungranted(Ungranted::Nobody(NoGrants::PoolKindUnknown))), "the newer evaluation's reason, not the stale one's");
    }

    // what this catches: a turn hanging on a watch that can never fire, and a decided window refused
    // only because no watch exists.
    #[tokio::test]
    async fn no_evaluations_refuse_only_an_ungranted_mind() {
        type Never = std::future::Ready<Result<super::super::window_grant::GrantEvaluation, GrantsUnavailable>>;
        let her = uuid::Uuid::from_u128(4);
        let refused = decide_window(her, || MindWindow::Ungranted, || {}, |_| Err::<Never, _>(GrantsUnavailable)).await;
        assert_eq!(refused, Err(WindowRefused::Unavailable(GrantsUnavailable)));
        let bound = decide_window(her, || MindWindow::Decided(Decided::Bound(8_192)), || {}, |_| Err::<Never, _>(GrantsUnavailable)).await;
        assert_eq!(bound, Ok(Decided::Bound(8_192)), "a bound window needs no evaluations");
    }
}
