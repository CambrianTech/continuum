//! The delivery boundary cannot acknowledge an unconsumed terminal result.
#![cfg(test)]
use super::*;
use crate::runtime::tasks::TaskExecution;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};

struct PerceptionSink(parking_lot::Mutex<BTreeSet<Uuid>>);

#[async_trait::async_trait]
impl TaskOwnerSink for PerceptionSink {
    type Error = std::io::Error;

    async fn consume(&self, _: Uuid, _: &[(Uuid, TaskEnd)]) -> Result<BTreeSet<Uuid>, Self::Error> {
        Ok(self.0.lock().clone())
    }
}

#[tokio::test]
async fn only_page_scoped_perception_can_ack_and_invalid_batch_has_no_partial_effects(
) -> Result<(), Box<dyn std::error::Error>> {
    let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
    let ledger = TaskLedger::new(adapter).await?;
    let owner = Uuid::new_v4();
    let first = ledger
        .accept(owner, "first".into(), TaskExecution::new_command_executor())
        .await?;
    let second = ledger
        .accept(
            owner,
            "second".into(),
            TaskExecution::new_command_executor(),
        )
        .await?;
    ledger
        .finish(
            first,
            owner,
            ledger.execution(first, owner).await?,
            TaskEnd::Stopped,
        )
        .await?;
    ledger
        .finish(
            second,
            owner,
            ledger.execution(second, owner).await?,
            TaskEnd::Lost,
        )
        .await?;
    let sink = PerceptionSink(Default::default());
    let limit = NonZeroUsize::new(32).ok_or("invalid fixture page")?;
    assert_eq!(
        ledger
            .deliver_page(owner, None, limit, &sink)
            .await?
            .acknowledged,
        0
    );
    let foreign = Uuid::new_v4();
    sink.0.lock().extend([first, foreign]);
    assert!(
        matches!(ledger.deliver_page(owner, None, limit, &sink).await,
        Err(DeliveryError::InvalidReceipt { task }) if task == foreign)
    );
    assert_eq!(ledger.pending(first, owner).await?, Some(TaskEnd::Stopped));
    sink.0.lock().remove(&foreign);
    assert_eq!(
        ledger
            .deliver_page(owner, None, limit, &sink)
            .await?
            .acknowledged,
        1
    );
    assert_eq!(ledger.pending(first, owner).await?, None);
    assert_eq!(ledger.pending(second, owner).await?, Some(TaskEnd::Lost));
    Ok(())
}

struct Sink {
    ledger: Arc<TaskLedger>,
    refuse: AtomicBool,
    commits: AtomicUsize,
    seen: parking_lot::Mutex<Vec<(Uuid, Uuid, TaskEnd)>>,
}

#[async_trait::async_trait]
impl TaskOwnerSink for Sink {
    type Error = std::io::Error;

    async fn consume(
        &self,
        owner: Uuid,
        ends: &[(Uuid, TaskEnd)],
    ) -> Result<BTreeSet<Uuid>, Self::Error> {
        for (task, end) in ends {
            let pending = self
                .ledger
                .pending(*task, owner)
                .await
                .map_err(std::io::Error::other)?;
            assert_eq!(pending.as_ref(), Some(end), "consumption must precede ACK");
        }
        if self.refuse.load(Ordering::SeqCst) {
            return Err(std::io::Error::other("injected persistence refusal"));
        }
        self.commits.fetch_add(1, Ordering::SeqCst);
        self.seen
            .lock()
            .extend(ends.iter().map(|(task, end)| (owner, *task, end.clone())));
        Ok(ends.iter().map(|(task, _)| *task).collect())
    }
}

#[tokio::test]
async fn refused_consumption_remains_pending_and_replay_needs_no_registered_handle(
) -> Result<(), Box<dyn std::error::Error>> {
    let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
    let ledger = Arc::new(TaskLedger::new(adapter.clone()).await?);
    let owner = Uuid::new_v4();
    let task = ledger
        .accept(owner, "build".into(), TaskExecution::new_command_executor())
        .await?;
    ledger
        .finish(
            task,
            owner,
            ledger.execution(task, owner).await?,
            TaskEnd::Stopped,
        )
        .await?;
    let sink = Sink {
        ledger: ledger.clone(),
        refuse: AtomicBool::new(true),
        seen: Default::default(),
        commits: AtomicUsize::new(0),
    };
    let limit = NonZeroUsize::new(32).expect("test: nonzero page");
    assert!(
        matches!(ledger.deliver_page(owner, None, limit, &sink).await, Err(DeliveryError::Consume { owner: failed, .. }) if failed == owner)
    );
    assert_eq!(ledger.pending(task, owner).await?, Some(TaskEnd::Stopped));
    assert!(sink.seen.lock().is_empty());

    let reopened = TaskLedger::new(adapter).await?;
    sink.refuse.store(false, Ordering::SeqCst);
    assert_eq!(
        reopened
            .deliver_page(owner, None, limit, &sink)
            .await?
            .acknowledged,
        1
    );
    assert_eq!(*sink.seen.lock(), vec![(owner, task, TaskEnd::Stopped)]);
    assert_eq!(
        reopened
            .deliver_page(owner, None, limit, &sink)
            .await?
            .acknowledged,
        0
    );
    assert_eq!(sink.seen.lock().len(), 1);
    Ok(())
}

#[tokio::test]
async fn page_delivery_keeps_owner_isolation_and_crosses_empty_pages(
) -> Result<(), Box<dyn std::error::Error>> {
    let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
    let ledger = Arc::new(TaskLedger::new(adapter).await?);
    let owner = Uuid::new_v4();
    let foreign = Uuid::new_v4();
    let mine = ledger
        .accept(owner, "mine".into(), TaskExecution::new_command_executor())
        .await?;
    let theirs = ledger
        .accept(
            foreign,
            "theirs".into(),
            TaskExecution::new_command_executor(),
        )
        .await?;
    ledger
        .finish(
            mine,
            owner,
            ledger.execution(mine, owner).await?,
            TaskEnd::Lost,
        )
        .await?;
    ledger
        .finish(
            theirs,
            foreign,
            ledger.execution(theirs, foreign).await?,
            TaskEnd::Stopped,
        )
        .await?;
    let sink = Sink {
        ledger: ledger.clone(),
        refuse: AtomicBool::new(false),
        seen: Default::default(),
        commits: AtomicUsize::new(0),
    };
    let mut after = None;
    let mut exhausted = false;
    // Acceptance, terminal and optional ACK rows plus a final empty page.
    for _ in 0..4 {
        let page = ledger
            .deliver_page(
                owner,
                after,
                NonZeroUsize::new(1).expect("test: nonzero"),
                &sink,
            )
            .await?;
        if page.exhausted {
            exhausted = true;
            break;
        }
        assert_ne!(page.after, after);
        after = page.after;
    }
    assert!(exhausted);
    assert_eq!(*sink.seen.lock(), vec![(owner, mine, TaskEnd::Lost)]);
    assert_eq!(
        ledger.pending(theirs, foreign).await?,
        Some(TaskEnd::Stopped)
    );
    Ok(())
}

#[tokio::test]
async fn one_owner_page_is_consumed_once_before_its_acknowledgements(
) -> Result<(), Box<dyn std::error::Error>> {
    let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
    let ledger = Arc::new(TaskLedger::new(adapter).await?);
    let owner = Uuid::new_v4();
    let first = ledger
        .accept(owner, "first".into(), TaskExecution::new_command_executor())
        .await?;
    let second = ledger
        .accept(
            owner,
            "second".into(),
            TaskExecution::new_command_executor(),
        )
        .await?;
    ledger
        .finish(
            first,
            owner,
            ledger.execution(first, owner).await?,
            TaskEnd::Stopped,
        )
        .await?;
    ledger
        .finish(
            second,
            owner,
            ledger.execution(second, owner).await?,
            TaskEnd::Lost,
        )
        .await?;
    let sink = Sink {
        ledger: ledger.clone(),
        refuse: AtomicBool::new(false),
        commits: AtomicUsize::new(0),
        seen: Default::default(),
    };
    let page = ledger
        .deliver_page(
            owner,
            None,
            NonZeroUsize::new(32).ok_or("invalid test page")?,
            &sink,
        )
        .await?;
    assert_eq!(page.acknowledged, 2);
    assert_eq!(sink.commits.load(Ordering::SeqCst), 1);
    assert_eq!(sink.seen.lock().len(), 2);
    assert_eq!(ledger.pending(first, owner).await?, None);
    assert_eq!(ledger.pending(second, owner).await?, None);
    Ok(())
}
