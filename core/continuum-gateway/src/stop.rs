//! What the gateway's module says to everything it stops: the raise-once signal it governs
//! its listener, connections and calls with, and the words an agent is told. The bottom of
//! the crate's module graph: `answer`, `sever`, `listener` and `server` all depend on it, and
//! it on none of them.

use std::future::Future;

use tokio::sync::watch;

/// What an agent whose call was in flight is told when the node stops.
pub(crate) const STOPPING: &str =
    "this node's gateway is stopping; the call was stopped before it finished";

/// One raise-once signal from the module to what it governs. Its receiver resolves when the
/// module raises it, or when the module is gone (which ends what it guards all the same).
#[derive(Clone)]
pub(crate) struct Signal(watch::Receiver<bool>);

impl Signal {
    /// The sender the owner raises, and the signal it governs.
    pub(crate) fn pair() -> (watch::Sender<bool>, Self) {
        let (raise, raised) = watch::channel(false);
        (raise, Self(raised))
    }

    /// Resolves once the signal is raised or its owner dropped it.
    pub(crate) fn raised(&self) -> impl Future<Output = ()> + Send + 'static {
        let mut signal = self.0.clone();
        async move {
            match signal.wait_for(|raised| *raised).await {
                Ok(_) => {}
                // The owner dropped its sender: the listener it governs is gone.
                Err(_) => {}
            }
        }
    }
}
