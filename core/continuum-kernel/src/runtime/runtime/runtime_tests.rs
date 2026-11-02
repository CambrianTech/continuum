#[path = "self_shutdown_tests.rs"]
mod self_shutdown_tests;

use super::*;
use crate::runtime::runtime::piece_2_pr3_dispatch_tests::RecordingModule;
use std::sync::atomic::Ordering;

#[tokio::test]
async fn shutdown_waits_for_acceptance_and_terminal_storage_after_receipt_is_forgotten(
) -> Result<(), String> {
    use crate::orm::gated_adapter::GatedAdapter;
    use crate::runtime::tasks::TaskLedger;
    use crate::runtime::CommandExecutor;
    let runtime = Runtime::new();
    let (module, _) = RecordingModule::new("storage-boundary", Vec::new());
    runtime.register(module.clone());
    let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
    let adapter = Arc::new(GatedAdapter::new(adapter));
    let ledger = Arc::new(TaskLedger::new(adapter.clone()).await?);
    let executor = Arc::new(
        CommandExecutor::new(runtime.registry.clone()).with_message_bus(runtime.bus.clone()),
    );
    executor.set_task_ledger(ledger.clone())?;
    let mut events = runtime.bus.receiver();
    let owner = uuid::Uuid::new_v4();
    let caller =
        crate::routing::CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(owner));
    let acceptance = adapter.pause_next_write();
    {
        let receipt =
            executor.dispatch_background("test/unhandled", serde_json::Value::Null, Some(caller));
        tokio::pin!(receipt);
        assert!(futures::poll!(receipt.as_mut()).is_pending());
        acceptance.entered.notified().await;
        // Drop the actual caller while its durable acceptance write is paused.
    }
    let shutdown = runtime.shutdown_within(std::time::Duration::from_secs(30));
    tokio::pin!(shutdown);
    assert!(futures::poll!(shutdown.as_mut()).is_pending());
    assert_eq!(module.saves.load(Ordering::SeqCst), 0);
    let terminal = adapter.pause_next_write();
    acceptance.release.notify_one();
    terminal.entered.notified().await;
    assert!(futures::poll!(shutdown.as_mut()).is_pending());
    assert_eq!(module.saves.load(Ordering::SeqCst), 0);
    terminal.release.notify_one();
    let receipt = shutdown.await;
    assert!(receipt.state_is_durable());
    assert_eq!(module.saves.load(Ordering::SeqCst), 1);
    let completion = events.recv().await.map_err(|error| error.to_string())?;
    let task = completion.payload["handle"]
        .as_str()
        .ok_or("missing completion handle")?;
    let task = uuid::Uuid::parse_str(task).map_err(|error| error.to_string())?;
    // The command was unhandled, but its failure is durable and deliverable;
    // module shutdown cannot overtake either storage boundary.
    assert!(matches!(
        ledger.pending(task, owner).await?,
        Some(crate::runtime::tasks::TaskEnd::Failed { .. })
    ));
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn shutdown_owner_retries_withheld_custody_without_repeating_module_stop(
) -> Result<(), String> {
    let runtime = Arc::new(Runtime::new());
    let (module, _) = RecordingModule::new("retry-custody-store", Vec::new());
    runtime.register(module.clone());
    let custody = runtime
        .registry
        .task_admission
        .admit_owned()
        .ok_or("admission refused")?;
    let operation = ShutdownOperation::new();
    let mut first = operation.begin(Some(runtime.clone()), std::time::Duration::ZERO);
    first.changed().await.map_err(|error| error.to_string())?;
    let receipt = first
        .borrow_and_update()
        .clone()
        .ok_or("missing withheld receipt")?;
    assert!(!receipt.tasks.is_quiet());
    assert_eq!(module.saves.load(Ordering::SeqCst), 0);
    drop(first);
    drop(custody);
    let mut retry = operation.begin(Some(runtime.clone()), std::time::Duration::ZERO);
    assert!(retry.borrow().is_none());
    retry.changed().await.map_err(|error| error.to_string())?;
    let receipt = retry
        .borrow_and_update()
        .clone()
        .ok_or("missing completed receipt")?;
    assert!(receipt.state_is_durable());
    let again = operation.begin(Some(runtime), std::time::Duration::ZERO);
    assert_eq!(again.borrow().as_ref(), Some(&receipt));
    assert_eq!(module.saves.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn unfinished_custody_withholds_module_stop_and_a_later_drain_can_finish(
) -> Result<(), String> {
    let runtime = Runtime::new();
    let (module, _) = RecordingModule::new("custody-store", Vec::new());
    runtime.register(module.clone());
    let custody = runtime
        .registry
        .task_admission
        .admit_owned()
        .ok_or("admission refused")?;
    let receipt = runtime
        .shutdown_within(std::time::Duration::from_millis(10))
        .await;
    assert!(!receipt.state_is_durable());
    assert!(!receipt.tasks.is_quiet());
    assert!(receipt.modules.is_empty());
    assert_eq!(module.saves.load(Ordering::SeqCst), 0);
    assert!(runtime.registry.task_admission.admit_owned().is_none());
    drop(custody);
    let receipt = runtime
        .shutdown_within(std::time::Duration::from_millis(10))
        .await;
    assert!(receipt.state_is_durable());
    assert_eq!(module.saves.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn cancelling_shutdown_wait_does_not_release_task_custody() -> Result<(), String> {
    let runtime = Runtime::new();
    let (module, _) = RecordingModule::new("cancel-custody-store", Vec::new());
    runtime.register(module.clone());
    let custody = runtime
        .registry
        .task_admission
        .admit_owned()
        .ok_or("admission refused")?;
    {
        let stop = runtime.shutdown_within(std::time::Duration::from_secs(1));
        tokio::pin!(stop);
        assert!(futures::poll!(stop.as_mut()).is_pending());
    }
    assert_eq!(module.saves.load(Ordering::SeqCst), 0);
    assert_eq!(runtime.registry.task_admission.in_flight(), 1);
    assert!(runtime.registry.task_admission.admit_owned().is_none());
    drop(custody);
    assert!(runtime
        .shutdown_within(std::time::Duration::from_secs(1))
        .await
        .state_is_durable());
    Ok(())
}

#[tokio::test]
async fn failed_terminal_persistence_is_not_a_quiet_runtime() {
    let runtime = Runtime::new();
    let (module, _) = RecordingModule::new("failed-custody-store", Vec::new());
    runtime.register(module.clone());
    // Inject the execution owner's terminal-write failure fact; this tests
    // shutdown's consumption of it, not the storage adapter's fault handling.
    runtime
        .registry
        .task_persistence_failed
        .store(true, Ordering::Release);
    let receipt = runtime
        .shutdown_within(std::time::Duration::from_secs(1))
        .await;
    assert!(!receipt.state_is_durable());
    assert_eq!(module.saves.load(Ordering::SeqCst), 0);
    assert!(matches!(receipt.tasks, DrainOutcome::Unknown { .. }));
    assert!(receipt.modules.is_empty());
}
