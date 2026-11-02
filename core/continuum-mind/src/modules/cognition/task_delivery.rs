//! Durable replay rides cognition's existing lifecycle tick. No persona needs to
//! remember a handle or poll; cursors are scan progress, never delivery authority.
use super::CognitionModule;
use continuum_kernel::runtime::tasks::{TaskLedger, TaskOwnerSink};
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroUsize;
use uuid::Uuid;

// Storage page policy, independent of prompt/context budgets. Each resident gets
// one bounded page per existing lifecycle tick, including through empty pages.
const REPLAY_PAGE_ROWS: usize = 64;

#[derive(Default)]
pub(super) struct ReplaySweep {
    after: BTreeMap<Uuid, Option<Uuid>>,
}

impl ReplaySweep {
    async fn advance<S: TaskOwnerSink>(
        &mut self,
        ledger: &TaskLedger,
        owners: Vec<Uuid>,
        sink: &S,
        limit: NonZeroUsize,
    ) -> Result<(), String> {
        let live: BTreeSet<_> = owners.into_iter().collect();
        self.after.retain(|owner, _| live.contains(owner));
        let mut failures = Vec::new();
        for owner in live {
            let after = *self.after.entry(owner).or_insert(None);
            match ledger.deliver_page(owner, after, limit, sink).await {
                Ok(page) => {
                    // New ends can sort before our cursor. Every completed sweep
                    // starts again; acknowledged rows cannot repeat consumption.
                    self.after
                        .insert(owner, if page.exhausted { None } else { page.after });
                }
                Err(error) => {
                    // No cursor advance on failure: the durable outbox retains
                    // every unacknowledged result, including failed consumption.
                    failures.push(format!("{owner}: {error}"));
                }
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "resident task replay failed: {}",
                failures.join("; ")
            ))
        }
    }
}

impl CognitionModule {
    pub(super) async fn reconcile_tasks(&self) -> Result<(), String> {
        let registry = crate::cognition::persona_workspace::global();
        let owners: Vec<_> = registry.roster().into_iter().map(|(id, _)| id).collect();
        if owners.is_empty() {
            self.task_replay.lock().await.after.clear();
            return Ok(());
        }
        let ledger = self.executor.require()?.task_ledger().await?;
        let sink = crate::cognition::persona_workspace::ResidentTaskSink::new(registry);
        let limit =
            NonZeroUsize::new(REPLAY_PAGE_ROWS).ok_or("task replay page policy must be nonzero")?;
        self.task_replay
            .lock()
            .await
            .advance(ledger, owners, &sink, limit)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_kernel::runtime::tasks::{TaskEnd, TaskExecution};
    use std::sync::atomic::{AtomicBool, Ordering};

    // Scheduling fixture only; real durable consumption is tested by cognition's
    // ledger-to-resident integration test, not claimed by this in-memory sink.
    struct Sink {
        refused: Uuid,
        refuse: AtomicBool,
    }
    #[async_trait::async_trait]
    impl TaskOwnerSink for Sink {
        type Error = std::io::Error;
        async fn consume(
            &self,
            owner: Uuid,
            ends: &[(Uuid, TaskEnd)],
        ) -> Result<BTreeSet<Uuid>, Self::Error> {
            if owner == self.refused && self.refuse.load(Ordering::SeqCst) {
                Err(std::io::Error::other("injected owner refusal"))
            } else {
                Ok(ends.iter().map(|(task, _)| *task).collect())
            }
        }
    }

    #[tokio::test]
    async fn failed_owner_does_not_starve_peer_and_later_ends_are_revisited(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (adapter, _directory) = continuum_kernel::orm::store::fresh_adapter().await;
        let ledger = TaskLedger::new(adapter).await?;
        let refused = Uuid::new_v4();
        let healthy = Uuid::new_v4();
        let execution = TaskExecution::new_command_executor();
        let blocked = ledger.accept(refused, "blocked".into(), execution).await?;
        let ready = ledger.accept(healthy, "ready".into(), execution).await?;
        let running = ledger.accept(healthy, "running".into(), execution).await?;
        ledger
            .finish(
                blocked,
                refused,
                ledger.execution(blocked, refused).await?,
                TaskEnd::Stopped,
            )
            .await?;
        ledger
            .finish(
                ready,
                healthy,
                ledger.execution(ready, healthy).await?,
                TaskEnd::Stopped,
            )
            .await?;
        let sink = Sink {
            refused,
            refuse: AtomicBool::new(true),
        };
        let mut sweep = ReplaySweep::default();
        let limit = NonZeroUsize::MIN;
        // Scan through individual fact rows, including pages with no pending end.
        for _ in 0..12 {
            let _ = sweep
                .advance(&ledger, vec![refused, healthy], &sink, limit)
                .await;
        }
        assert_eq!(ledger.pending(ready, healthy).await?, None);
        assert_eq!(
            ledger.pending(blocked, refused).await?,
            Some(TaskEnd::Stopped)
        );
        // End an already-scanned acceptance. Fresh sweeps must find it.
        ledger
            .finish(
                running,
                healthy,
                ledger.execution(running, healthy).await?,
                TaskEnd::Lost,
            )
            .await?;
        sink.refuse.store(false, Ordering::SeqCst);
        for _ in 0..16 {
            sweep
                .advance(&ledger, vec![refused, healthy], &sink, limit)
                .await?;
        }
        assert_eq!(ledger.pending(running, healthy).await?, None);
        assert_eq!(ledger.pending(blocked, refused).await?, None);
        sweep.advance(&ledger, vec![healthy], &sink, limit).await?;
        assert!(!sweep.after.contains_key(&refused));
        Ok(())
    }
}
