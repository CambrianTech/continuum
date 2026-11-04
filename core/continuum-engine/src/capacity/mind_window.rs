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
    /// the grants watch ([`grants_changed`]); never a provisional number.
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
pub async fn grants_changed() -> Result<(), GrantsUnavailable> {
    let mut rx = super::window_grant::grants_receiver().ok_or(GrantsUnavailable)?;
    rx.changed().await.map_err(|_| GrantsUnavailable)
}

/// Decide her window. An ungranted mind is MEASURED first (`measure` builds her view, which records
/// her wish, so the next grants tick can grant her), then waits for new grants and decides again:
/// measure, park, wake, decide (Cormac on #5752: a wait before the measurement waits forever).
pub async fn decide_window<D, M, W, F>(decide: D, mut measure: M, wait: W) -> Result<Decided, GrantsUnavailable>
where
    D: Fn() -> MindWindow,
    M: FnMut(),
    W: Fn() -> F,
    F: std::future::Future<Output = Result<(), GrantsUnavailable>>,
{
    loop {
        match decide() {
            MindWindow::Decided(decided) => return Ok(decided),
            MindWindow::Ungranted => {
                measure();
                wait().await?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::planning::window_allocator::test_tokens as t;
    use std::cell::Cell;

    // what this catches (Cormac on #5752): a new mind that waits for a grant before anything measures
    // her, so she is never granted. She is measured, then parks, then is granted and runs.
    #[tokio::test]
    async fn an_ungranted_mind_is_measured_before_she_parks_then_runs_at_her_grant() {
        let measured = Cell::new(0);
        let woke = Cell::new(0);
        let decided = decide_window(
            || match measured.get() {
                0 => MindWindow::Ungranted,
                _ => MindWindow::Decided(Decided::Granted(t(20_817))),
            },
            || measured.set(measured.get() + 1),
            || {
                assert_eq!(measured.get(), 1, "measured before she parks");
                woke.set(woke.get() + 1);
                async { Ok(()) }
            },
        )
        .await;
        assert_eq!(decided, Ok(Decided::Granted(t(20_817))));
        assert_eq!((measured.get(), woke.get()), (1, 1), "one measurement, one wait, then her grant");
    }

    // what this catches: a turn hanging silently on a watch that can never fire.
    #[tokio::test]
    async fn no_grants_watch_is_a_typed_refusal_never_a_hang() {
        let decided = decide_window(|| MindWindow::Ungranted, || {}, || async { Err(GrantsUnavailable) }).await;
        assert_eq!(decided, Err(GrantsUnavailable));
    }
}
