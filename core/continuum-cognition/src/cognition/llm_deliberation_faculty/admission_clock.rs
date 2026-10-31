//! Exact-step admission time, protected only while its blocking queue advances.
//! join_all polls faculties in this task; detached workers do not inherit it.
use crate::inference::{
    llama_server::{InFlightSlot, SlotWork},
    resource_admission::LaneWait,
};
use std::{future::Future, time::Duration};
use tokio::{sync::watch, time::Instant};

#[derive(Clone, Copy)]
pub(crate) struct QueueTime {
    base: Instant,
    bound: Duration,
    completed: Duration,
    started: Option<Instant>,
    waiting: usize,
    pub(crate) stalled: bool,
    /// The step's model call was dispatched (its drop witness armed): a deadline that ends
    /// the step from here drops a generation, never a wait (card 843d5570).
    pub(crate) dispatched: bool,
}
impl QueueTime {
    pub(crate) fn new(base: Instant, bound: Duration) -> Self {
        Self {
            base,
            bound,
            completed: Duration::ZERO,
            started: None,
            waiting: 0,
            stalled: false,
            dispatched: false,
        }
    }
    /// Generation extensions accrue only outside a registered wait. Completed
    /// intervals include cancelled waits, but their step's state is never reused.
    pub(crate) fn deadline(self) -> Option<Instant> {
        if self.started.is_some() {
            None
        } else {
            Some(self.base + self.completed)
        }
    }
    pub(crate) fn extend(&mut self) {
        self.base += self.bound;
    }
}

tokio::task_local! { static QUEUE_TIME: watch::Sender<QueueTime>; }
pub(crate) async fn scope<T>(tx: watch::Sender<QueueTime>, work: impl Future<Output = T>) -> T {
    QUEUE_TIME.scope(tx, work).await
}
/// Mark this step's model call dispatched. Outside a step's scope there is no deadline to
/// classify, so there is nothing to mark.
pub(crate) fn note_dispatched() {
    QUEUE_TIME
        .try_with(|tx| tx.send_modify(|state| state.dispatched = true))
        .unwrap_or(()); // unwrap_or: no step scope = no step deadline to tell a drop from a wait
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

pub(crate) struct QueueProgress {
    pub(crate) grants: (u64, u64),
    pub(crate) holders: Vec<(u64, InFlightSlot, Option<SlotWork>)>,
}
impl QueueProgress {
    fn advanced_from(&self, previous: &Self) -> bool {
        self.grants != previous.grants
            || self.holders.iter().any(|(id, slot, now)| {
                previous.holders.iter().any(|(old_id, old_slot, before)| {
                    id == old_id
                        && slot == old_slot
                        && before.is_some()
                        && crate::inference::llama_server::slot_advanced(*before, *now)
                })
            })
    }
}
async fn sample(wait: &LaneWait) -> QueueProgress {
    let snapshot = wait.progress();
    let holders =
        futures::future::join_all(snapshot.holders.into_iter().filter_map(|(id, persona)| {
            crate::inference::llama_server::in_flight_slot(persona).map(|slot| async move {
                let work = crate::inference::llama_server::slot_work(
                    &slot.root,
                    slot.slot,
                    probe_client(),
                )
                .await;
                (id, slot, work)
            })
        }))
        .await;
    QueueProgress {
        grants: snapshot.grants,
        holders,
    }
}
fn probe_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}
/// Registered from the acquisition's first poll until completion or drop,
/// including time pending. Grant, directed yield, cancellation and shutdown all
/// release the original entry. Checkpoints use the existing act bound, not a
/// new queue timeout; only actual relevant grants or holder work extend a wait.
pub(crate) async fn waiting<T>(wait: LaneWait, acquire: impl Future<Output = T>) -> T {
    waiting_with_probe(acquire, || sample(&wait)).await
}

pub(crate) async fn waiting_with_probe<T, P, S>(acquire: impl Future<Output = T>, mut probe: P) -> T
where
    P: FnMut() -> S,
    S: Future<Output = QueueProgress>,
{
    let guard = QUEUE_TIME
        .try_with(|tx| {
            tx.send_modify(|state| {
                if state.waiting == 0 {
                    let now = Instant::now();
                    // Expired registration never earns time, even if it later completes.
                    if now < state.base + state.completed {
                        state.started = Some(now);
                    }
                }
                state.waiting += 1;
            });
            Waiting(tx.clone())
        })
        .ok();
    let monitor = async {
        if let Some(guard) = &guard {
            let bound = guard.0.borrow().bound;
            let mut previous = probe().await;
            loop {
                tokio::time::sleep(bound).await;
                let current = probe().await;
                if crate::inference::llama_server::engine_condemned()
                    || !current.advanced_from(&previous)
                {
                    continuum_base::probe!(
                        class = "settle.queue.stalled",
                        bound_s = bound.as_secs(),
                        "the registered queue made no relevant grant or blocking-slot progress within the act bound"
                    );
                    guard.0.send_modify(|state| state.stalled = true);
                    break;
                }
                previous = current;
            }
        }
        std::future::pending::<T>().await
    };
    tokio::select! {
        biased;
        value = acquire => value,
        value = monitor => value,
    }
}
