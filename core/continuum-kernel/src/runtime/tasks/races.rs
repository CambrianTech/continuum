//! A cancelled writer may still commit: storage owns the in-flight write.
use super::*;
use crate::orm::gated_adapter::GatedAdapter;

async fn notified(signal: &tokio::sync::Notify) -> Result<(), String> {
    tokio::time::timeout(std::time::Duration::from_secs(5), signal.notified())
        .await
        .map_err(|error| error.to_string())
}

#[tokio::test]
async fn cancelled_terminal_write_cannot_replace_winner_or_resurrect_delivery() -> Result<(), String>
{
    let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
    let gated = Arc::new(GatedAdapter::new(adapter.clone()));
    let delayed = Arc::new(TaskLedger::new(gated.clone()).await?);
    let current = TaskLedger::new(adapter.clone()).await?;
    let owner = Uuid::new_v4();
    let task = current.accept(owner, "code/shell".into()).await?;
    let pause = gated.pause_next_write();
    let writer = tokio::spawn(async move { delayed.finish(task, owner, TaskEnd::Lost).await });
    notified(&pause.entered).await?;
    writer.abort();
    assert!(matches!(writer.await, Err(error) if error.is_cancelled()));

    // The competing supervisor observed the actual execution ending. Its result
    // reaches the sink while the abandoned storage operation is still pending.
    let winner = TaskEnd::Done(TaskOutput::Json(serde_json::json!({"saved": true})));
    assert_eq!(current.finish(task, owner, winner.clone()).await?, winner);
    assert_eq!(current.pending(task, owner).await?, Some(winner.clone()));
    current.acknowledge(task, owner).await?;
    pause.release.notify_one();
    notified(&pause.done).await?;
    drop(current);

    let reopened = TaskLedger::new(adapter).await?;
    assert_eq!(reopened.finish(task, owner, TaskEnd::Lost).await?, winner);
    assert_eq!(reopened.pending(task, owner).await?, None);
    assert!(reopened
        .pending_page(owner, None, 10)
        .await?
        .ends
        .is_empty());
    Ok(())
}
