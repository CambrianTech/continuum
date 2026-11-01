use super::*;
use crate::runtime::runtime::piece_2_pr3_dispatch_tests::RecordingModule;
use crate::runtime::{CommandExecutor, CommandInterceptor, InterceptorOutcome};
use std::sync::atomic::Ordering;

struct RequestOwnShutdown {
    runtime: Arc<Runtime>,
    operation: Arc<ShutdownOperation>,
}

#[async_trait::async_trait]
impl CommandInterceptor for RequestOwnShutdown {
    fn name(&self) -> &'static str {
        "request-own-shutdown"
    }

    async fn try_route(
        &self,
        _: &str,
        _: &serde_json::Value,
        _: Option<&crate::routing::CallerIdentity>,
    ) -> Result<InterceptorOutcome, String> {
        let direct = self.runtime.shutdown().await;
        assert!(!direct.tasks.is_quiet());
        assert!(direct.modules.is_empty());
        let observer = self
            .operation
            .begin(Some(self.runtime.clone()), Duration::ZERO);
        let receipt = observer
            .borrow()
            .clone()
            .ok_or("self-shutdown was not refused immediately")?;
        assert!(!receipt.tasks.is_quiet());
        assert!(self.runtime.registry.task_admission.is_open());
        Ok(InterceptorOutcome::Handled(CommandResult::Json(
            serde_json::Value::Null,
        )))
    }
}

#[tokio::test]
async fn actual_background_self_shutdown_refuses_before_closing_and_can_finish(
) -> Result<(), String> {
    let runtime = Arc::new(Runtime::new());
    let (module, _) = RecordingModule::new("self-shutdown-store", Vec::new());
    runtime.register(module.clone());
    let operation = Arc::new(ShutdownOperation::new());
    let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
    let ledger = Arc::new(crate::runtime::tasks::TaskLedger::new(adapter).await?);
    let mut events = runtime.bus.receiver();
    let executor = Arc::new(
        CommandExecutor::new(runtime.registry.clone())
            .with_message_bus(runtime.bus.clone())
            .with_interceptor(Arc::new(RequestOwnShutdown {
                runtime: runtime.clone(),
                operation: operation.clone(),
            })),
    );
    executor.set_task_ledger(ledger.clone())?;
    let owner = uuid::Uuid::new_v4();
    let caller =
        crate::routing::CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(owner));
    let handle = executor
        .dispatch_background("test/shutdown", serde_json::Value::Null, Some(caller))
        .await
        .map_err(|e| e.message)?;
    tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    assert!(matches!(
        ledger.pending(handle, owner).await?,
        Some(crate::runtime::tasks::TaskEnd::Done(_))
    ));
    assert_eq!(module.saves.load(Ordering::SeqCst), 0);
    assert!(runtime.registry.task_admission.is_open());
    // The refused request did not poison the retained process operation.
    let observer = operation.begin(Some(runtime.clone()), Duration::ZERO);
    let receipt = await_shutdown(observer, Duration::from_secs(5))
        .await
        .ok_or("external shutdown did not finish")?;
    assert!(receipt.state_is_durable());
    assert_eq!(module.saves.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn storage_unwind_cannot_release_custody_as_a_clean_shutdown() -> Result<(), String> {
    let runtime = Arc::new(Runtime::new());
    let (module, _) = RecordingModule::new("storage-unwind", Vec::new());
    runtime.register(module.clone());
    let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
    let adapter = Arc::new(crate::orm::gated_adapter::GatedAdapter::new(adapter));
    let ledger = Arc::new(crate::runtime::tasks::TaskLedger::new(adapter.clone()).await?);
    let executor = Arc::new(CommandExecutor::new(runtime.registry.clone()));
    executor.set_task_ledger(ledger)?;
    adapter.unwind_next_write();
    let caller = crate::routing::CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(
        uuid::Uuid::new_v4(),
    ));
    let result = executor
        .dispatch_background("test/result", serde_json::Value::Null, Some(caller))
        .await;
    assert!(result.is_err());
    assert_eq!(runtime.registry.task_admission.in_flight(), 0);
    let receipt = runtime.shutdown().await;
    assert!(!receipt.tasks.is_quiet());
    assert!(receipt.modules.is_empty());
    assert_eq!(module.saves.load(Ordering::SeqCst), 0);
    Ok(())
}

struct FailTerminalWrite(Arc<crate::orm::gated_adapter::GatedAdapter>);

#[tokio::test]
async fn lost_acceptance_acknowledgement_is_uncertain_and_withholds_shutdown() -> Result<(), String>
{
    use crate::orm::adapter::StorageAdapter;
    let runtime = Arc::new(Runtime::new());
    let (module, _) = RecordingModule::new("acceptance-ack-loss", Vec::new());
    runtime.register(module.clone());
    let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
    let adapter = Arc::new(crate::orm::gated_adapter::GatedAdapter::new(adapter));
    let ledger = Arc::new(crate::runtime::tasks::TaskLedger::new(adapter.clone()).await?);
    let executor = Arc::new(CommandExecutor::new(runtime.registry.clone()));
    executor.set_task_ledger(ledger)?;
    adapter.fail_after_next_create();
    let caller = crate::routing::CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(
        uuid::Uuid::new_v4(),
    ));
    let result = executor
        .dispatch_background("test/result", serde_json::Value::Null, Some(caller))
        .await;
    let Err(error) = result else {
        return Err("lost acceptance acknowledgement was reported as success".into());
    };
    assert_eq!(
        error.outcome,
        crate::runtime::command_executor::DispatchOutcome::Uncertain
    );
    // The adapter really committed the acceptance before losing its reply.
    let count = adapter
        .count(crate::orm::query::QueryBuilder::new("kernel_task_facts").build())
        .await;
    assert!(count.success);
    assert_eq!(count.data, Some(1));
    let receipt = runtime.shutdown().await;
    assert!(matches!(receipt.tasks, DrainOutcome::Unknown { .. }));
    assert!(receipt.modules.is_empty());
    assert_eq!(module.saves.load(Ordering::SeqCst), 0);
    Ok(())
}

#[async_trait::async_trait]
impl CommandInterceptor for FailTerminalWrite {
    fn name(&self) -> &'static str {
        "fail-terminal-write"
    }

    async fn try_route(
        &self,
        _: &str,
        _: &serde_json::Value,
        _: Option<&crate::routing::CallerIdentity>,
    ) -> Result<InterceptorOutcome, String> {
        // Acceptance has already committed. Fail only the terminal write after
        // a successful handler result, outside the handler unwind boundary.
        self.0.unwind_next_write();
        Ok(InterceptorOutcome::Handled(CommandResult::Json(
            serde_json::Value::Null,
        )))
    }
}

#[tokio::test]
async fn terminal_storage_unwind_neither_announces_completion_nor_allows_shutdown(
) -> Result<(), String> {
    let runtime = Arc::new(Runtime::new());
    let (module, _) = RecordingModule::new("terminal-unwind", Vec::new());
    runtime.register(module.clone());
    let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
    let adapter = Arc::new(crate::orm::gated_adapter::GatedAdapter::new(adapter));
    let ledger = Arc::new(crate::runtime::tasks::TaskLedger::new(adapter.clone()).await?);
    let mut events = runtime.bus.receiver();
    let executor = Arc::new(
        CommandExecutor::new(runtime.registry.clone())
            .with_message_bus(runtime.bus.clone())
            .with_interceptor(Arc::new(FailTerminalWrite(adapter))),
    );
    executor.set_task_ledger(ledger.clone())?;
    let owner = uuid::Uuid::new_v4();
    let caller =
        crate::routing::CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(owner));
    let handle = executor
        .dispatch_background("test/result", serde_json::Value::Null, Some(caller))
        .await
        .map_err(|error| error.message)?;
    tokio::time::timeout(
        Duration::from_secs(5),
        runtime.registry.task_admission.close_and_wait(),
    )
    .await
    .map_err(|error| error.to_string())?;
    assert_eq!(ledger.pending(handle, owner).await?, None);
    assert!(matches!(
        events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
    let receipt = runtime.shutdown().await;
    assert!(matches!(receipt.tasks, DrainOutcome::Unknown { .. }));
    assert!(receipt.modules.is_empty());
    assert_eq!(module.saves.load(Ordering::SeqCst), 0);
    Ok(())
}
