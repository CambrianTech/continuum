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

/// Decide her window. An ungranted mind is MEASURED first (`measure` builds her view, which records
/// her wish, so the next grants tick can grant her), then waits for NEWER grants and decides again
/// (Cormac on #5752). The waiter is taken before each decision, so no publish between the two is lost.
/// No grants watch is a refusal only when she is ungranted: a decided window needs none.
pub async fn decide_window<D, M, S, W>(decide: D, mut measure: M, subscribe: S) -> Result<Decided, GrantsUnavailable>
where
    D: Fn() -> MindWindow,
    M: FnMut(),
    S: Fn() -> Result<W, GrantsUnavailable>,
    W: std::future::Future<Output = Result<(), GrantsUnavailable>>,
{
    loop {
        let waiter = subscribe();
        match decide() {
            MindWindow::Decided(decided) => return Ok(decided),
            MindWindow::Ungranted => {
                measure();
                waiter?.await?;
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

    // what this catches (Cormac on #5752): a new mind that waits for a grant before anything measures
    // her. She is measured, then parks, then is granted and runs; and a grant published between her
    // decision and her wait is not lost (the waiter is taken first).
    #[tokio::test]
    async fn an_ungranted_mind_is_measured_parks_then_runs_at_her_grant() {
        let her = uuid::Uuid::from_u128(4);
        let (sender, installed) = tokio::sync::watch::channel(grants(&[]));
        let measured = Cell::new(0);
        let granted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let decided = decide_window(
            || match granted.load(std::sync::atomic::Ordering::SeqCst) {
                false => MindWindow::Ungranted,
                true => MindWindow::Decided(Decided::Granted(t(14_860))),
            },
            || {
                measured.set(measured.get() + 1);
                // the serving tick that grants her runs after she is measured
                granted.store(true, std::sync::atomic::Ordering::SeqCst);
                let _ = sender.send(grants(&[(her, 14_860)])); // the receiver is held by the waiter
            },
            || Ok(GrantsWaiter::new(installed.clone()).changed()),
        )
        .await;
        assert_eq!(decided, Ok(Decided::Granted(t(14_860))));
        assert_eq!(measured.get(), 1, "measured once, then granted");
    }

    // what this catches: a turn hanging on a watch that can never fire, and a decided window refused
    // only because no watch exists.
    #[tokio::test]
    async fn no_grants_watch_refuses_only_an_ungranted_mind() {
        let refused = decide_window(|| MindWindow::Ungranted, || {}, || Err::<std::future::Ready<Result<(), GrantsUnavailable>>, _>(GrantsUnavailable)).await;
        assert_eq!(refused, Err(GrantsUnavailable));
        let bound = decide_window(|| MindWindow::Decided(Decided::Bound(8_192)), || {}, || Err::<std::future::Ready<Result<(), GrantsUnavailable>>, _>(GrantsUnavailable)).await;
        assert_eq!(bound, Ok(Decided::Bound(8_192)), "a bound window needs no grants watch");
    }
}
