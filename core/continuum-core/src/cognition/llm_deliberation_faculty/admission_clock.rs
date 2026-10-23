//! Queue time belongs to this settle step, not to its generation watchdog.
//! Faculties are polled by join_all in the step's task; this scope deliberately
//! does not follow detached workers or another turn of the same persona.
use std::{future::Future, time::Duration};
use tokio::{sync::watch, time::Instant};

#[derive(Clone, Copy, Default)]
pub(crate) struct QueueTime {
    completed: Duration,
    started: Option<Instant>,
    waiting: usize,
}

impl QueueTime {
    pub(crate) fn deadline(self, base: Instant) -> Option<Instant> {
        let deadline = base + self.completed;
        // A wait registered after the budget was already exhausted cannot revive it.
        match self.started {
            Some(started) if started < deadline => None,
            _ => Some(deadline),
        }
    }
}

tokio::task_local! {
    static QUEUE_TIME: watch::Sender<QueueTime>;
}

pub(crate) async fn scope<T>(tx: watch::Sender<QueueTime>, work: impl Future<Output = T>) -> T {
    QUEUE_TIME.scope(tx, work).await
}

struct Waiting(watch::Sender<QueueTime>);

impl Drop for Waiting {
    fn drop(&mut self) {
        self.0.send_modify(|state| {
            state.waiting -= 1;
            if state.waiting == 0 {
                if let Some(started) = state.started.take() {
                    state.completed += started.elapsed();
                }
            }
        });
    }
}

/// Register only while the acquisition future is actually being polled. Drop
/// records the interval on grant, directed yield, cancellation or shutdown.
pub(crate) async fn waiting<T>(acquire: impl Future<Output = T>) -> T {
    let guard = QUEUE_TIME.try_with(|tx| {
        tx.send_modify(|state| {
            if state.waiting == 0 {
                state.started = Some(Instant::now());
            }
            state.waiting += 1;
        });
        Waiting(tx.clone())
    }).ok();
    let result = acquire.await;
    drop(guard);
    result
}
