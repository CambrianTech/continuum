//! The teacher's capacity wait after the incumbent exited, ended by a typed terminal outcome,
//! never a stopwatch (CONTINUUM-ENGINE.md RTOS rule 10).
//!
//! The incumbent's exit is announced only after its lease was returned, so the board already
//! carries it; the memory it freed shows in the first pressure reading the monitor publishes
//! after the exit. A lane that still does not fit once both have spoken is a refusal now, and
//! the batch restores the incumbent instead of holding the minds' engine down on a guess that
//! some other consumer may free memory later.

use std::future::Future;

use tokio::sync::watch;

use crate::sdk_codegen::CommandError;

/// How a wait for teacher capacity ended.
#[derive(Debug)]
pub(super) enum Capacity<T> {
    /// The private lane was admitted.
    Granted(T),
    /// It does not fit with the incumbent's lease and memory already returned.
    Refused,
    /// The serving intent changed under the batch.
    Superseded,
    /// The caller cancelled the batch.
    Cancelled,
    /// The resource authority's board closed: no producer is left to grant anything.
    AuthorityGone,
}

impl<T> Capacity<T> {
    /// The granted lane, or the failure that ends the batch (its cleanup restores the incumbent).
    pub(super) fn granted(self) -> Result<T, CommandError> {
        let reason = match self {
            Self::Granted(lane) => return Ok(lane),
            Self::Refused => "teacher lane does not fit with the incumbent's capacity returned",
            Self::Superseded => "serving intent changed during admission",
            Self::Cancelled => "teacher cancelled while awaiting capacity",
            Self::AuthorityGone => "resource authority ended",
        };
        Err(CommandError::Internal(reason.into()))
    }
}

/// The publications a capacity wait re-checks on, and the caller's cancel.
pub(super) struct CapacitySignals<'a, B, M> {
    /// The resource authority's board.
    pub board: &'a mut watch::Receiver<B>,
    /// The pressure monitor's publications, subscribed after the incumbent's exit; `None` when no
    /// monitor runs, so nothing vetoes on memory and the board alone is the truth.
    pub memory: Option<watch::Receiver<M>>,
    /// The batch's cancellation.
    pub cancellation: &'a mut watch::Receiver<bool>,
}

/// Admit the lane as soon as `prepare` grants it, re-checking on every board publication and
/// every memory reading, until one of [`Capacity`]'s terminal outcomes.
pub(super) async fn await_capacity<T, B, M, F, Fut>(
    mut prepare: F,
    signals: CapacitySignals<'_, B, M>,
    current: impl Fn() -> bool,
) -> Result<Capacity<T>, CommandError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Option<T>, CommandError>>,
{
    let CapacitySignals { board, mut memory, cancellation } = signals;
    let mut memory_read_after_exit = memory.is_none();
    loop {
        if *cancellation.borrow() {
            return Ok(Capacity::Cancelled);
        }
        if !current() {
            return Ok(Capacity::Superseded);
        }
        if let Some(lane) = prepare().await? {
            return Ok(Capacity::Granted(lane));
        }
        if memory_read_after_exit {
            return Ok(Capacity::Refused);
        }
        tokio::select! {
            changed = board.changed() => {
                if changed.is_err() {
                    return Ok(Capacity::AuthorityGone);
                }
            }
            read = next_reading(&mut memory) => {
                memory_read_after_exit = true;
                if !read {
                    // The monitor stopped: its last reading stands, nothing newer will come.
                    memory = None;
                }
            }
            _ = cancellation.changed() => return Ok(Capacity::Cancelled),
        }
    }
}

/// The next memory publication: `true` for a reading, `false` once the monitor is gone. Pending
/// forever without a monitor, so the select waits on the board and the caller alone.
async fn next_reading<M>(memory: &mut Option<watch::Receiver<M>>) -> bool {
    match memory.as_mut() {
        Some(readings) => readings.changed().await.is_ok(),
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A `prepare` that refuses until `fits` is set, counting its calls.
    fn lane(fits: &Arc<AtomicBool>, calls: &Arc<AtomicUsize>) -> impl FnMut() -> std::future::Ready<Result<Option<u8>, CommandError>> {
        let fits = Arc::clone(fits);
        let calls = Arc::clone(calls);
        move || {
            calls.fetch_add(1, Ordering::SeqCst);
            std::future::ready(Ok(fits.load(Ordering::SeqCst).then_some(7)))
        }
    }

    // what this catches: the wait deciding on elapsed time instead of the producer. A board
    // publication that frees room wakes the waiter and admits; nothing before it resolves.
    #[tokio::test]
    async fn a_board_publication_wakes_the_waiter_and_admits() {
        let (fits, calls) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicUsize::new(0)));
        let (board_tx, mut board) = watch::channel(());
        let (memory_tx, memory) = watch::channel(());
        let (_cancel_tx, mut cancel) = watch::channel(false);
        let mut wait = Box::pin(await_capacity(lane(&fits, &calls), CapacitySignals { board: &mut board, memory: Some(memory), cancellation: &mut cancel }, || true));
        assert!((&mut wait).now_or_never().is_none(), "refused before any publication: still waiting");
        fits.store(true, Ordering::SeqCst);
        board_tx.send_replace(());
        assert!(matches!(wait.await, Ok(Capacity::Granted(7))));
        assert_eq!(calls.load(Ordering::SeqCst), 2, "one check per publication, no polling");
        drop(memory_tx);
    }

    // what this catches: a refusal held open on a timer. Once a memory reading published after
    // the exit still refuses, the outcome is Refused and the incumbent can be restored.
    #[tokio::test]
    async fn a_refusal_after_the_post_exit_memory_reading_is_terminal() {
        let (fits, calls) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicUsize::new(0)));
        let (_board_tx, mut board) = watch::channel(());
        let (memory_tx, memory) = watch::channel(());
        let (_cancel_tx, mut cancel) = watch::channel(false);
        let mut wait = Box::pin(await_capacity(lane(&fits, &calls), CapacitySignals { board: &mut board, memory: Some(memory), cancellation: &mut cancel }, || true));
        assert!((&mut wait).now_or_never().is_none());
        memory_tx.send_replace(());
        assert!(matches!(wait.await, Ok(Capacity::Refused)));
    }

    // what this catches: the producer dying read as completion or waited on forever. A closed
    // board is the typed AuthorityGone, and a cancel is the typed Cancelled.
    #[tokio::test]
    async fn a_dead_producer_and_a_caller_cancel_are_typed_outcomes() {
        let (fits, calls) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicUsize::new(0)));
        let (board_tx, mut board) = watch::channel(());
        let (_memory_tx, memory) = watch::channel(());
        let (_cancel_tx, mut cancel) = watch::channel(false);
        let mut wait = Box::pin(await_capacity(lane(&fits, &calls), CapacitySignals { board: &mut board, memory: Some(memory.clone()), cancellation: &mut cancel }, || true));
        assert!((&mut wait).now_or_never().is_none());
        drop(board_tx);
        assert!(matches!(wait.await, Ok(Capacity::AuthorityGone)));

        let (_board_tx, mut board) = watch::channel(());
        let (cancel_tx, mut cancel) = watch::channel(false);
        let mut wait = Box::pin(await_capacity(lane(&fits, &calls), CapacitySignals { board: &mut board, memory: Some(memory), cancellation: &mut cancel }, || true));
        assert!((&mut wait).now_or_never().is_none());
        cancel_tx.send_replace(true);
        assert!(matches!(wait.await, Ok(Capacity::Cancelled)));
    }

    // what this catches: with no pressure monitor running there is no reading to wait for; the
    // board already carries the exit, so a refusal is immediate, and changed intent wins first.
    #[tokio::test]
    async fn without_a_monitor_the_board_alone_decides() {
        let (fits, calls) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicUsize::new(0)));
        let (_board_tx, mut board) = watch::channel(());
        let (_cancel_tx, mut cancel) = watch::channel(false);
        let outcome = await_capacity(lane(&fits, &calls), CapacitySignals { board: &mut board, memory: None::<watch::Receiver<()>>, cancellation: &mut cancel }, || true).now_or_never();
        assert!(matches!(outcome, Some(Ok(Capacity::Refused))));
        let outcome = await_capacity(lane(&fits, &calls), CapacitySignals { board: &mut board, memory: None::<watch::Receiver<()>>, cancellation: &mut cancel }, || false).now_or_never();
        assert!(matches!(outcome, Some(Ok(Capacity::Superseded))));
    }
}
