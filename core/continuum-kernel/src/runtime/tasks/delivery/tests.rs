//! The delivery boundary cannot acknowledge an unconsumed terminal result.
#![cfg(test)]
use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

struct Sink {
    ledger: Arc<TaskLedger>,
    refuse: AtomicBool,
    seen: parking_lot::Mutex<Vec<(Uuid, Uuid, TaskEnd)>>,
}

#[async_trait::async_trait]
impl TaskOwnerSink for Sink {
    type Error = std::io::Error;

    async fn consume(&self, owner: Uuid, task: Uuid, end: &TaskEnd) -> Result<(), Self::Error> {
        let pending = self
            .ledger
            .pending(task, owner)
            .await
            .map_err(std::io::Error::other)?;
        assert_eq!(pending.as_ref(), Some(end), "consumption must precede ACK");
        if self.refuse.load(Ordering::SeqCst) {
            return Err(std::io::Error::other("injected persistence refusal"));
        }
        self.seen.lock().push((owner, task, end.clone()));
        Ok(())
    }
}

#[tokio::test]
async fn refused_consumption_remains_pending_and_replay_needs_no_registered_handle(
) -> Result<(), Box<dyn std::error::Error>> {
    let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
    let ledger = Arc::new(TaskLedger::new(adapter.clone()).await?);
    let owner = Uuid::new_v4();
    let task = ledger.accept(owner, "build".into()).await?;
    ledger.finish(task, owner, TaskEnd::Stopped).await?;
    let sink = Sink {
        ledger: ledger.clone(),
        refuse: AtomicBool::new(true),
        seen: Default::default(),
    };
    let limit = NonZeroUsize::new(32).expect("test: nonzero page");
    assert!(
        matches!(ledger.deliver_page(owner, None, limit, &sink).await, Err(DeliveryError::Consume { task: failed, .. }) if failed == task)
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
    let mine = ledger.accept(owner, "mine".into()).await?;
    let theirs = ledger.accept(foreign, "theirs".into()).await?;
    ledger.finish(mine, owner, TaskEnd::Lost).await?;
    ledger.finish(theirs, foreign, TaskEnd::Stopped).await?;
    let sink = Sink {
        ledger: ledger.clone(),
        refuse: AtomicBool::new(false),
        seen: Default::default(),
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
