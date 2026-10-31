//! Terminal custody: a task's proposed end is held until the ledger durably commits it, and only the
//! durable winner is ever delivered. Before this, a failed `finish` logged and returned, so the end was
//! lost and its durable acceptance stayed unresolved forever (Codex on #5585, background.rs).
//!
//! The rules, each a test below:
//! - first proposal per task wins and is immutable while held;
//! - a commit never holds the custody lock across the store's IO;
//! - one commit attempt per task at a time (a claim), so a finish and a flush racing cannot both
//!   deliver; a dropped (cancelled) attempt releases its claim and keeps the terminal held;
//! - success yields the store's DURABLE winner (a competing supervisor may have committed another
//!   end first), never this writer's losing proposal;
//! - a failed commit keeps the terminal held; it is retried by `flush` on the existing lifecycle tick,
//!   never by a private timer;
//! - `shutdown` drains exclusively: it moves the custody to `Draining` (holds are still accepted, so
//!   they are reported; outside commits and flushes claim nothing), waits for in-flight attempts on an
//!   armed notification, makes the final attempt itself, then closes and reports every terminal it
//!   still holds under the same lock, typed, so none escapes both delivery and the report (Codex on
//!   #5664: an outside commit could claim after the empty check and return with IO in flight);
//! - a store failure is a typed outcome, never a silent zero: `flush` reports what committed and
//!   what failed, with the store's reason (Codex on #5664).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::Notify;
use uuid::Uuid;

use super::{TaskEnd, TaskLedger};

#[cfg(test)]
mod tests;

/// Where a terminal is made durable: the ledger in production, a failing store in tests.
#[async_trait::async_trait]
pub trait TerminalStore: Send + Sync {
    /// Commit `end` for `task` and return the durable winner (the first terminal ever committed).
    async fn finish(&self, task: Uuid, owner: Uuid, end: TaskEnd) -> Result<TaskEnd, String>;
}

#[async_trait::async_trait]
impl TerminalStore for TaskLedger {
    async fn finish(&self, task: Uuid, owner: Uuid, end: TaskEnd) -> Result<TaskEnd, String> {
        TaskLedger::finish(self, task, owner, end).await
    }
}

/// A terminal held until it is durable, and what delivering it needs.
#[derive(Clone, Debug, PartialEq)]
pub struct HeldTerminal {
    pub task: Uuid,
    pub owner: Uuid,
    pub end: TaskEnd,
    /// The command path the completion event names.
    pub command: String,
    /// How long the task ran, for the completion event.
    pub elapsed_ms: u64,
}

/// A terminal made durable: the held record and the store's winning end, which is what is delivered.
#[derive(Clone, Debug, PartialEq)]
pub struct Committed {
    pub held: HeldTerminal,
    pub winner: TaskEnd,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CustodyError {
    /// Shutdown is draining or closed custody: an outside commit or flush claims nothing (shutdown
    /// makes the final attempt itself), and after close a new terminal is refused; the caller reports it.
    Closed,
    /// The store did not make the terminal durable; it stays held for the next flush.
    Store(String),
}

/// What one pass over the held terminals did: delivered winners, every failure with its reason, and
/// whether shutdown began draining part-way (the rest stay held for shutdown; what this pass already
/// committed is still here to deliver, never dropped with the refusal).
#[derive(Debug, Default)]
pub struct FlushReport {
    pub committed: Vec<Committed>,
    pub failed: Vec<(Uuid, String)>,
    pub closed_mid_pass: bool,
}

/// What shutdown did: its final pass, and every terminal still held after it closed.
#[derive(Debug, Default)]
pub struct ShutdownReport {
    pub last_pass: FlushReport,
    pub unresolved: Vec<HeldTerminal>,
}

/// Custody's lifetime: open, draining (shutdown alone commits; holds are kept to be reported), closed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Open,
    Draining,
    Closed,
}

#[derive(Default)]
struct State {
    held: HashMap<Uuid, HeldTerminal>,
    committing: HashSet<Uuid>,
    phase: Phase,
}

#[derive(Default)]
pub struct TerminalCustody {
    state: Mutex<State>,
    settled: Notify,
}

/// Releases a commit claim however the attempt ends, including a cancelled future.
struct Claim<'a> {
    custody: &'a TerminalCustody,
    task: Uuid,
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        self.custody.state.lock().committing.remove(&self.task);
        self.custody.settled.notify_waiters();
    }
}

impl TerminalCustody {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Take custody of a proposed end. The first proposal for a task is kept; a later one for the
    /// same task is ignored (one terminal per task).
    pub fn hold(&self, terminal: HeldTerminal) -> Result<(), CustodyError> {
        let mut state = self.state.lock();
        if state.phase == Phase::Closed {
            return Err(CustodyError::Closed);
        }
        state.held.entry(terminal.task).or_insert(terminal);
        Ok(())
    }

    /// Try to make one held terminal durable. `Ok(Some)` when this attempt committed it (deliver the
    /// winner), `Ok(None)` when it is not held or another attempt has it claimed, `Err(Store)` when the
    /// store failed (the terminal stays held for the next flush), `Err(Closed)` once shutdown drains.
    pub async fn commit(&self, store: &dyn TerminalStore, task: Uuid) -> Result<Option<Committed>, CustodyError> {
        self.commit_in(store, task, Phase::Open).await
    }

    /// One attempt, claimed only while the custody is in `phase`: outside callers pass `Open`,
    /// shutdown's final pass `Draining`, so nothing outside can claim once draining has begun.
    async fn commit_in(&self, store: &dyn TerminalStore, task: Uuid, phase: Phase) -> Result<Option<Committed>, CustodyError> {
        let held = {
            let mut state = self.state.lock();
            if state.phase != phase {
                return Err(CustodyError::Closed);
            }
            let Some(held) = state.held.get(&task).cloned() else { return Ok(None) };
            if !state.committing.insert(task) {
                return Ok(None);
            }
            held
        };
        let _claim = Claim { custody: self, task };
        let winner = store.finish(held.task, held.owner, held.end.clone()).await.map_err(CustodyError::Store)?;
        self.state.lock().held.remove(&task);
        Ok(Some(Committed { held, winner }))
    }

    /// Try every held terminal not already being committed; committed winners are delivered, failures
    /// reported with their reason, and both stay typed. Called on the existing lifecycle tick.
    pub async fn flush(&self, store: &dyn TerminalStore) -> Result<FlushReport, CustodyError> {
        if self.state.lock().phase != Phase::Open {
            return Err(CustodyError::Closed);
        }
        Ok(self.pass(store, Phase::Open).await)
    }

    /// One pass in `phase`. When the custody leaves `phase` part-way (shutdown began draining), the
    /// pass stops, keeps what it already committed, and says so; the rest stay held.
    async fn pass(&self, store: &dyn TerminalStore, phase: Phase) -> FlushReport {
        let tasks: Vec<Uuid> = {
            let state = self.state.lock();
            state.held.keys().filter(|task| !state.committing.contains(task)).copied().collect()
        };
        let mut report = FlushReport::default();
        for task in tasks {
            match self.commit_in(store, task, phase).await {
                Ok(Some(done)) => report.committed.push(done),
                Ok(None) => {}
                Err(CustodyError::Store(reason)) => report.failed.push((task, reason)),
                Err(CustodyError::Closed) => {
                    report.closed_mid_pass = true;
                    break;
                }
            }
        }
        report
    }

    /// How many terminals are held (committed or not yet).
    pub fn held(&self) -> usize {
        self.state.lock().held.len()
    }

    /// Drain exclusively and close. The first shutdown owns it: from here outside commits and flushes
    /// claim nothing; it waits for the attempts already in flight, makes the final pass, then closes and
    /// reports every terminal still held, read under the lock that closes, so a hold either reached this
    /// report or was refused. A concurrent shutdown never reopens draining: it waits for the owner to
    /// close and reports what is still held.
    pub async fn shutdown(&self, store: &dyn TerminalStore) -> ShutdownReport {
        let owner = {
            let mut state = self.state.lock();
            let open = state.phase == Phase::Open;
            if open {
                state.phase = Phase::Draining;
            }
            open
        };
        if !owner {
            self.wait_until(|state| state.phase == Phase::Closed).await;
            return ShutdownReport { last_pass: FlushReport::default(), unresolved: self.state.lock().held.values().cloned().collect() };
        }
        self.wait_until(|state| state.committing.is_empty()).await;
        // Only this owner moves the phase off Draining, so its own pass cannot be refused.
        let last_pass = self.pass(store, Phase::Draining).await;
        let unresolved = {
            let mut state = self.state.lock();
            state.phase = Phase::Closed;
            state.held.values().cloned().collect()
        };
        self.settled.notify_waiters();
        ShutdownReport { last_pass, unresolved }
    }

    /// Wait until `ready` holds, on a notification armed before each check, so a change between the
    /// check and the await still wakes it.
    async fn wait_until(&self, ready: impl Fn(&State) -> bool) {
        loop {
            let settled = self.settled.notified();
            tokio::pin!(settled);
            settled.as_mut().enable();
            if ready(&self.state.lock()) {
                return;
            }
            settled.await;
        }
    }
}
