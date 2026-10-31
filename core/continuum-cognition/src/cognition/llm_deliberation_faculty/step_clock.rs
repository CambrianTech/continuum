//! An act step's clock: its deadline, its extensions, and whether its model call was dispatched.
//! join_all polls faculties in this task; detached workers do not inherit it.
//!
//! It used to also pause the deadline while a step waited in the serving-lane queue. No step
//! waits in one any more (SERVING-ENGINE §1c): admission is the adapter's turn admission, whose
//! time counts as the generation's.
use std::{future::Future, time::Duration};
use tokio::{sync::watch, time::Instant};

#[derive(Clone, Copy)]
pub(crate) struct StepClock {
    deadline: Instant,
    bound: Duration,
    /// The step's model call was dispatched (its drop witness armed): a deadline that ends
    /// the step from here drops a generation, never a wait (card 843d5570).
    pub(crate) dispatched: bool,
}
impl StepClock {
    pub(crate) fn new(deadline: Instant, bound: Duration) -> Self {
        Self { deadline, bound, dispatched: false }
    }
    pub(crate) fn deadline(self) -> Instant {
        self.deadline
    }
    pub(crate) fn extend(&mut self) {
        self.deadline += self.bound;
    }
}

tokio::task_local! { static STEP_CLOCK: watch::Sender<StepClock>; }
pub(crate) async fn scope<T>(tx: watch::Sender<StepClock>, work: impl Future<Output = T>) -> T {
    STEP_CLOCK.scope(tx, work).await
}
/// Mark this step's model call dispatched. Outside a step's scope there is no deadline to
/// classify, so there is nothing to mark.
pub(crate) fn note_dispatched() {
    STEP_CLOCK
        .try_with(|tx| tx.send_modify(|state| state.dispatched = true))
        .unwrap_or(()); // unwrap_or: no step scope = no step deadline to tell a drop from a wait
}
