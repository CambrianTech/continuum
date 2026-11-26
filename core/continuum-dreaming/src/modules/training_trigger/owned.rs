//! What each of the trainer's owned operations is, while it runs (card 979d9e0b). Shutdown joins
//! them; a join that never ends must say which operation it waits for, of what kind, for whose
//! bucket, and since when (the 5090 standalone, 2026-11-22: `shutdown.join_operations` began with
//! six in flight and timed out, and none of the six could be named).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use uuid::Uuid;

/// What an owned operation does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OwnedKind {
    /// The serving wake's recovery pass (not joined by shutdown: the wake stops it).
    Recover,
    Submit,
    Retire,
    /// Moving a bucket stranded under a base spelling to the catalog id (genome/training-trigger/rekey).
    Rekey,
    /// Carrying a bucket's examples not learned yet to the base she runs (genome/training-trigger/transfer).
    Transfer,
    /// Offering a bucket's newest generation as a candidate.
    Sweep,
    #[cfg(test)]
    Test,
}

/// One operation running now, as a shutdown report names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InFlight {
    pub(crate) kind: OwnedKind,
    /// Whose bucket it works on; none for one that works on every bucket.
    pub(crate) persona: Option<Uuid>,
    pub(crate) age_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct EntryId(u64);

struct Entry {
    kind: OwnedKind,
    persona: Option<Uuid>,
    since: std::time::Instant,
}

/// The owned operations running now.
#[derive(Default, Clone)]
pub(crate) struct Running {
    entries: Arc<parking_lot::Mutex<HashMap<EntryId, Entry>>>,
    next: Arc<AtomicU64>,
}

impl Running {
    /// `work`, named from now until it completes or is dropped (an aborted operation leaves too).
    pub(crate) fn named<F: std::future::Future>(&self, kind: OwnedKind, persona: Option<Uuid>, work: F) -> impl std::future::Future<Output = F::Output> {
        let id = EntryId(self.next.fetch_add(1, Ordering::Relaxed));
        self.entries.lock().insert(id, Entry { kind, persona, since: std::time::Instant::now() });
        let leaves = Leaves { running: self.clone(), id };
        async move {
            let _leaves = leaves;
            work.await
        }
    }

    /// Every operation running now, oldest first.
    pub(crate) fn now(&self) -> Vec<InFlight> {
        let mut running: Vec<(std::time::Instant, InFlight)> = self
            .entries
            .lock()
            .values()
            .map(|entry| (entry.since, InFlight { kind: entry.kind, persona: entry.persona, age_ms: entry.since.elapsed().as_millis() as u64 }))
            .collect();
        running.sort_by_key(|(since, _)| *since);
        running.into_iter().map(|(_, in_flight)| in_flight).collect()
    }
}

/// Removes its operation's entry when the operation's future is dropped, finished or not.
struct Leaves {
    running: Running,
    id: EntryId,
}

impl Drop for Leaves {
    fn drop(&mut self) {
        self.running.entries.lock().remove(&self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (the 5090 standalone, 2026-11-22: six operations in flight at a timed-out
    // shutdown, none named): an operation missing from the report while it runs, or one still listed
    // after it finished or was aborted, so the report blames something already gone.
    #[tokio::test]
    async fn an_operation_is_named_while_it_runs_and_leaves_when_it_ends_or_is_aborted() {
        let running = Running::default();
        let her = Uuid::new_v4();
        let (release, released) = tokio::sync::oneshot::channel::<()>();
        let mut set = tokio::task::JoinSet::new();
        set.spawn(running.named(OwnedKind::Retire, Some(her), async move {
            let _ = released.await;
        }));
        set.spawn(running.named(OwnedKind::Sweep, None, std::future::pending::<()>()));
        let kinds: Vec<(OwnedKind, Option<Uuid>)> = running.now().into_iter().map(|op| (op.kind, op.persona)).collect();
        assert_eq!(kinds, [(OwnedKind::Retire, Some(her)), (OwnedKind::Sweep, None)], "both named from admission, oldest first");
        release.send(()).expect("test: the step waits");
        set.join_next().await.expect("test: one finished").expect("test: it ran");
        assert_eq!(running.now().iter().map(|op| op.kind).collect::<Vec<_>>(), [OwnedKind::Sweep], "a finished step leaves");
        set.abort_all();
        while set.join_next().await.is_some() {}
        assert!(running.now().is_empty(), "an aborted fence leaves too");
    }
}
