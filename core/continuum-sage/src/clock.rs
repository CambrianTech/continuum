//! The node's one clock: every deadline a component chose is delivered from here.
//!
//! A deadline is an event like any other (her continuation falls due, waiting work ages a class),
//! so nothing else in SAGE sleeps: a component asks the clock for an [`Alarm`] and is woken by it.
//! One task per runtime thread holds its pending deadlines in order and waits for the earliest; registering or
//! dropping an alarm wakes it to re-plan. Dropping an [`Alarm`] cancels it.

use std::collections::BTreeMap;

use tokio::sync::mpsc;
use tokio::time::Instant;

type Wake = Box<dyn FnOnce() + Send>;

enum Command {
    At { at: Instant, id: u64, wake: Wake },
    Cancel(u64),
}

/// A pending deadline; dropping it cancels the wake.
pub struct Alarm {
    id: u64,
    commands: mpsc::UnboundedSender<Command>,
}

impl Drop for Alarm {
    fn drop(&mut self) {
        // A clock that has ended holds no alarms to cancel.
        if self.commands.send(Command::Cancel(self.id)).is_err() {
            continuum_base::probe!(class = "sage.clock.ended", "an alarm was dropped after the node's clock ended");
        }
    }
}

thread_local! {
    /// This runtime thread's clock. A deadline is measured on the runtime that set it (its time
    /// may be paused, as in a test), so the clock task lives on that runtime: one per runtime
    /// thread, never one per process captured by whichever runtime asked first. That process-wide
    /// clock rang nothing for one runtime while another's paused test held it (2026-11-22), and
    /// nothing at all once its runtime ended.
    static CLOCK: std::cell::RefCell<Option<mpsc::UnboundedSender<Command>>> = const { std::cell::RefCell::new(None) };
}
static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Run `wake` at `at`, unless the returned [`Alarm`] is dropped first. `wake` runs on the clock's
/// task: it hands an input to its component and returns.
pub fn at(at: Instant, wake: impl FnOnce() + Send + 'static) -> Alarm {
    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    CLOCK.with_borrow_mut(|clock| {
        let mut command = Command::At { at, id, wake: Box::new(wake) };
        if let Some(commands) = clock.as_ref() {
            match commands.send(command) {
                Ok(()) => return Alarm { id, commands: commands.clone() },
                // This thread's last runtime ended, and its clock with it: start one on this one.
                Err(mpsc::error::SendError(returned)) => command = returned,
            }
        }
        let (commands, inbox) = mpsc::unbounded_channel();
        tokio::spawn(run(inbox));
        if commands.send(command).is_err() {
            continuum_base::probe!(class = "sage.clock.ended", "a clock ended as it started");
        }
        *clock = Some(commands.clone());
        Alarm { id, commands }
    })
}

async fn run(mut inbox: mpsc::UnboundedReceiver<Command>) {
    let mut pending: BTreeMap<(Instant, u64), Wake> = BTreeMap::new();
    loop {
        let earliest = pending.keys().next().map(|(at, _)| *at);
        let command = tokio::select! {
            command = inbox.recv() => command,
            // The one wait in SAGE: the earliest deadline any component chose.
            () = async { match earliest { Some(at) => tokio::time::sleep_until(at).await, None => std::future::pending().await } } => {
                let now = Instant::now();
                let due: Vec<(Instant, u64)> = pending.range(..=(now, u64::MAX)).map(|(key, _)| *key).collect();
                for key in due {
                    if let Some(wake) = pending.remove(&key) {
                        wake();
                    }
                }
                continue;
            }
        };
        match command {
            Some(Command::At { at, id, wake }) => {
                pending.insert((at, id), wake);
            }
            Some(Command::Cancel(id)) => pending.retain(|(_, pending_id), _| *pending_id != id),
            None => return,
        }
    }
}
