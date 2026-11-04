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

/// Resolves when the serving daemon publishes new grants: what an ungranted mind waits on, and
/// what a mind on the shared pool watches for a changed window. Pending forever when no grants
/// are installed in this process (nothing will ever grant).
pub async fn grants_changed() {
    match super::window_grant::grants_receiver() {
        Some(mut rx) => {
            if rx.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        }
        None => std::future::pending::<()>().await,
    }
}
