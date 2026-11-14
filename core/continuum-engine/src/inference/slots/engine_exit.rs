//! An owned engine's exit as an event (CONTINUUM-ENGINE.md RTOS rule 10): the retired child's
//! reaper announces it through the generation's `watch`, and whoever needs the exit subscribes.
//! Nothing here times out: until the child reports exit, custody (its page dir, its resource
//! lease) stays with the retiring owner, and a waiter that stops waiting releases nothing.

use tokio::sync::watch;

/// A subscription to one engine generation's exit, from [`super::EngineGeneration::exit`].
pub struct EngineExit(watch::Receiver<bool>);

/// Why an exit wait ended without the exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EngineExitError {
    /// Every holder of the generation dropped it before its child reported exit: no owner is
    /// left to announce it, so the wait can never complete.
    #[error("the engine generation's owner is gone before its child reported exit")]
    OwnerGone,
}

impl EngineExit {
    pub(super) fn of(exited: watch::Receiver<bool>) -> Self {
        Self(exited)
    }

    /// Resolves once the owned child's exit is observed (immediately if it already was).
    /// Cancel-safe: dropping this future leaves the generation, its child and its custody as
    /// they were.
    pub async fn confirmed(mut self) -> Result<(), EngineExitError> {
        self.0.wait_for(|exited| *exited).await.map(drop).map_err(|_| EngineExitError::OwnerGone)
    }
}
