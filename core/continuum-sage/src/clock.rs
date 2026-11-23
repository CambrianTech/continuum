//! The node's one clock: every deadline a component chose is delivered from here.
//!
//! A deadline is an event like any other (her continuation falls due, waiting work ages a class),
//! so nothing else in SAGE sleeps: a component asks the clock for an [`Alarm`] and is woken by it.
//! One task holds every pending deadline in order and waits for the earliest; registering or
//! dropping an alarm wakes it to re-plan. Dropping an [`Alarm`] cancels it.

use std::collections::BTreeMap;
use std::sync::OnceLock;

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

struct Clock {
    commands: mpsc::UnboundedSender<Command>,
    next_id: std::sync::atomic::AtomicU64,
}

static CLOCK: OnceLock<Clock> = OnceLock::new();

fn clock() -> &'static Clock {
    CLOCK.get_or_init(|| {
        let (commands, inbox) = mpsc::unbounded_channel();
        tokio::spawn(run(inbox));
        Clock { commands, next_id: std::sync::atomic::AtomicU64::new(0) }
    })
}

/// Run `wake` at `at`, unless the returned [`Alarm`] is dropped first. `wake` runs on the clock's
/// task: it hands an input to its component and returns.
pub fn at(at: Instant, wake: impl FnOnce() + Send + 'static) -> Alarm {
    let clock = clock();
    let id = clock.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if clock.commands.send(Command::At { at, id, wake: Box::new(wake) }).is_err() {
        continuum_base::probe!(class = "sage.clock.ended", "an alarm was set after the node's clock ended");
    }
    Alarm { id, commands: clock.commands.clone() }
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
