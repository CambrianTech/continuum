#![cfg(test)]
use super::*;
use continuum_kernel::runtime::tasks::TaskEnd;

#[tokio::test]
async fn durable_terminal_source_receipts_retire_only_the_exact_published_end(
) -> Result<(), Box<dyn std::error::Error>> {
    use continuum_context::source::Source;
    use continuum_context::view::View;
    let home = tempfile::tempdir()?;
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let registry = PersonaWorkspaceRegistry::new();
    let owner = Uuid::new_v4();
    registry.register_from_cfg(cfg_for(owner))?;
    let task = Uuid::new_v4();
    registry.consume_task_ends(owner, vec![(task, TaskEnd::TimedOut)])?;
    registry.register_from_cfg(cfg_for(owner))?;
    let memory = Arc::clone(&registry.cycles.lock()[&owner].working_memory);
    let source = crate::cognition::context_streams::tasks::Tasks { memory: &memory, persona: owner };
    let (view, _) = View::<crate::cognition::context_streams::ContextMessage>::start(Vec::new(), &[&source as &dyn Source<_>], |claims| {
        claims.iter().map(|claim| claim.full_chars).collect()
    })?;
    assert!(view.render().any(|message| message.text.contains("timed out task")));
    let mut receipt = crate::cognition::render_receipt::RenderReceipt {
        owner, incarnation: memory.incarnation(), request: "submitted-request".to_owned(), view: view.receipt(),
    };
    let exact = receipt.clone();
    for item in &mut receipt.view.items { item.element = None; }
    assert!(registry.record_rendered_tasks(&receipt)?.is_none());
    receipt = exact.clone();
    for item in &mut receipt.view.items { item.level = continuum_context::level::Level::Folded; }
    assert!(registry.record_rendered_tasks(&receipt)?.is_none());
    receipt = exact.clone();
    for item in &mut receipt.view.items { item.retired = true; }
    assert!(registry.record_rendered_tasks(&receipt)?.is_none());
    assert_eq!(memory.pending_task_wakes(), vec![task]);
    assert!(registry.record_rendered_tasks(&exact)?.is_some());
    assert!(memory.pending_task_wakes().is_empty());
    let checkpoint = std::fs::read(volatile_path(owner)?)?;
    assert!(registry.record_rendered_tasks(&exact)?.is_none());
    assert_eq!(std::fs::read(volatile_path(owner)?)?, checkpoint);
    assert!(source.items().is_empty(), "a consumed end must not leave a standing task banner");
    let restored = WorkingMemory::new(8);
    restored.restore(load_volatile(owner)?.ok_or("checkpoint absent")?.wm);
    assert!(restored.pending_task_wakes().is_empty());
    Ok(())
}

#[tokio::test]
async fn forgotten_end_is_committed_before_receipt_and_restored_with_wake(
) -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let registry = PersonaWorkspaceRegistry::new();
    let owner = Uuid::new_v4();
    let task = Uuid::new_v4();
    registry.register_from_cfg(cfg_for(owner))?;
    let receipt = registry.consume_task_ends(owner, vec![(task, TaskEnd::Stopped)])?;
    assert_eq!(receipt.owner(), owner);
    assert_eq!(receipt.consumed_tasks(), &[task]);
    let old_incarnation = receipt.incarnation();
    let on_disk = load_volatile(owner)?.ok_or("checkpoint absent")?;
    let restored = WorkingMemory::new(8);
    restored.restore(on_disk.wm);
    assert_eq!(restored.snapshot().task_results.consumed_ids(), vec![task]);
    assert_eq!(restored.pending_task_wakes(), vec![task]);
    registry.register_from_cfg(cfg_for(owner))?;
    let memory = Arc::clone(&registry.cycles.lock()[&owner].working_memory);
    assert_ne!(memory.incarnation(), old_incarnation);
    assert_eq!(memory.pending_task_wakes(), vec![task]);
    assert!(memory.stage_task_reads(&[task])?);
    let replay = registry.consume_task_ends(owner, vec![(task, TaskEnd::Stopped)])?;
    assert_eq!(replay.incarnation(), memory.incarnation());
    let next = WorkingMemory::new(8);
    next.restore(load_volatile(owner)?.ok_or("checkpoint absent")?.wm);
    assert!(next.pending_task_wakes().is_empty());
    assert_eq!(next.snapshot().task_results.consumed_ids(), vec![task]);
    Ok(())
}

#[tokio::test]
async fn absent_conflicting_and_shutdown_consumption_are_explicit_refusals(
) -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let registry = PersonaWorkspaceRegistry::new();
    let owner = Uuid::new_v4();
    let task = Uuid::new_v4();
    assert!(
        matches!(registry.consume_task_ends(owner, vec![(task, TaskEnd::Lost)]), Err(TaskConsumptionError::Absent(id)) if id == owner)
    );
    registry.register_from_cfg(cfg_for(owner))?;
    registry.consume_task_ends(owner, vec![(task, TaskEnd::Lost)])?;
    let path = volatile_path(owner)?;
    let before = std::fs::read(&path)?;
    assert!(matches!(
        registry.consume_task_ends(owner, vec![(task, TaskEnd::Stopped)]),
        Err(TaskConsumptionError::Receipt(_))
    ));
    assert_eq!(std::fs::read(&path)?, before);
    for (_, result) in registry.stop_volatile_checkpoints() {
        result?;
    }
    let before = std::fs::read(&path)?;
    assert!(matches!(
        registry.consume_task_ends(owner, vec![(Uuid::new_v4(), TaskEnd::Lost)]),
        Err(TaskConsumptionError::Stopped)
    ));
    assert_eq!(std::fs::read(&path)?, before);
    Ok(())
}

#[tokio::test]
async fn failed_checkpoint_returns_no_receipt_and_replay_commits_staged_end(
) -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let registry = PersonaWorkspaceRegistry::new();
    let owner = Uuid::new_v4();
    let task = Uuid::new_v4();
    registry.register_from_cfg(cfg_for(owner))?;
    for (_, result) in registry.flush_volatile_all() {
        result?;
    }
    let path = volatile_path(owner)?;
    let before = std::fs::read(&path)?;
    // A directory at the writer's temp-file path forces a real filesystem refusal.
    // The existing committed checkpoint is never removed or replaced by the test.
    let obstruction = path.with_extension("json.tmp");
    std::fs::create_dir(&obstruction)?;
    assert!(matches!(
        registry.consume_task_ends(owner, vec![(task, TaskEnd::Lost)]),
        Err(TaskConsumptionError::Checkpoint(_))
    ));
    assert_eq!(std::fs::read(&path)?, before);
    let memory = Arc::clone(&registry.cycles.lock()[&owner].working_memory);
    assert!(memory.pending_task_wakes().is_empty());
    assert!(memory.stage_task_reads(&[task]).is_err());
    std::fs::remove_dir(&obstruction)?;
    // The ordinary checkpoint owner also publishes a staged end after IO recovers.
    for (_, result) in registry.flush_volatile_all() {
        result?;
    }
    assert_eq!(memory.pending_task_wakes(), vec![task]);
    let receipt = registry.consume_task_ends(owner, vec![(task, TaskEnd::Lost)])?;
    assert_eq!(receipt.consumed_tasks(), &[task]);
    let restored = WorkingMemory::new(8);
    restored.restore(load_volatile(owner)?.ok_or("checkpoint absent")?.wm);
    assert_eq!(restored.snapshot().task_results.consumed_ids(), vec![task]);
    assert_eq!(restored.pending_task_wakes(), vec![task]);
    Ok(())
}

#[tokio::test]
async fn ledger_ack_follows_resident_commit_and_restart_keeps_wake_without_handle(
) -> Result<(), Box<dyn std::error::Error>> {
    use continuum_kernel::runtime::tasks::{TaskLedger, TaskOutput};
    let home = tempfile::tempdir()?;
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let (adapter, _database) = continuum_kernel::orm::store::fresh_adapter().await;
    let ledger = TaskLedger::new(adapter.clone()).await?;
    let registry = Arc::new(PersonaWorkspaceRegistry::new());
    let sink = ResidentTaskSink::new(Arc::clone(&registry));
    let owner = Uuid::new_v4();
    let task = ledger.accept(owner, "test/result".into()).await?;
    let end = TaskEnd::Done(TaskOutput::Json(
        serde_json::json!({"result":"useful result"}),
    ));
    ledger.finish(task, owner, end.clone()).await?;
    let limit = std::num::NonZeroUsize::new(32).ok_or("invalid fixture page")?;
    assert!(ledger
        .deliver_page(owner, None, limit, &sink)
        .await
        .is_err());
    assert_eq!(ledger.pending(task, owner).await?, Some(end.clone()));
    registry.register_from_cfg(cfg_for(owner))?;
    let checkpoint = volatile_path(owner)?;
    std::fs::create_dir_all(checkpoint.parent().ok_or("checkpoint parent absent")?)?;
    let obstruction = checkpoint.with_extension("json.tmp");
    std::fs::create_dir(&obstruction)?;
    assert!(ledger
        .deliver_page(owner, None, limit, &sink)
        .await
        .is_err());
    assert_eq!(ledger.pending(task, owner).await?, Some(end.clone()));
    std::fs::remove_dir(&obstruction)?;
    assert_eq!(
        ledger
            .deliver_page(owner, None, limit, &sink)
            .await?
            .acknowledged,
        1
    );
    assert_eq!(ledger.pending(task, owner).await?, None);
    let reopened = TaskLedger::new(adapter).await?;
    let restored = WorkingMemory::new(8);
    restored.restore(load_volatile(owner)?.ok_or("checkpoint absent")?.wm);
    assert_eq!(restored.pending_task_wakes(), vec![task]);
    assert_eq!(reopened.terminal(task, owner).await?, Some(end));
    assert_eq!(
        reopened
            .deliver_page(owner, None, limit, &sink)
            .await?
            .acknowledged,
        0
    );
    assert_eq!(restored.pending_task_wakes(), vec![task]);
    Ok(())
}

#[tokio::test]
async fn failed_read_checkpoint_keeps_wake_pending_until_retry(
) -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let registry = PersonaWorkspaceRegistry::new();
    let owner = Uuid::new_v4();
    let task = Uuid::new_v4();
    registry.register_from_cfg(cfg_for(owner))?;
    registry.consume_task_ends(owner, vec![(task, TaskEnd::Stopped)])?;
    let memory = Arc::clone(&registry.cycles.lock()[&owner].working_memory);
    assert!(memory.stage_task_reads(&[task])?);
    let obstruction = volatile_path(owner)?.with_extension("json.tmp");
    std::fs::create_dir(&obstruction)?;
    let failed = registry.flush_volatile_all();
    assert_eq!(failed.len(), 1);
    assert!(failed[0].1.is_err());
    assert_eq!(memory.pending_task_wakes(), vec![task]);
    let restarted = WorkingMemory::new(8);
    restarted.restore(load_volatile(owner)?.ok_or("checkpoint absent")?.wm);
    assert_eq!(restarted.pending_task_wakes(), vec![task]);
    assert!(
        !memory.stage_task_reads(&[task])?,
        "already staged is not durable success"
    );
    std::fs::remove_dir(&obstruction)?;
    for (_, result) in registry.flush_volatile_all() {
        result?;
    }
    assert!(memory.pending_task_wakes().is_empty());
    let restarted = WorkingMemory::new(8);
    restarted.restore(load_volatile(owner)?.ok_or("checkpoint absent")?.wm);
    assert!(restarted.pending_task_wakes().is_empty());
    Ok(())
}

#[tokio::test]
async fn task_read_receipt_rejects_replaced_memory_and_retries_failed_commit(
) -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let registry = PersonaWorkspaceRegistry::new();
    let owner = Uuid::new_v4();
    let task = Uuid::new_v4();
    registry.register_from_cfg(cfg_for(owner))?;
    let accepted = registry.consume_task_ends(owner, vec![(task, TaskEnd::Stopped)])?;
    registry.register_from_cfg(cfg_for(owner))?;
    let current = Arc::clone(&registry.cycles.lock()[&owner].working_memory);
    assert!(
        matches!(registry.record_task_reads(owner, accepted.incarnation(), &[task]), Err(TaskConsumptionError::Replaced(id)) if id == owner)
    );
    assert_eq!(current.pending_task_wakes(), vec![task]);
    let obstruction = volatile_path(owner)?.with_extension("json.tmp");
    std::fs::create_dir(&obstruction)?;
    assert!(matches!(
        registry.record_task_reads(owner, current.incarnation(), &[task]),
        Err(TaskConsumptionError::Checkpoint(_))
    ));
    assert_eq!(current.pending_task_wakes(), vec![task]);
    std::fs::remove_dir(&obstruction)?;
    registry.record_task_reads(owner, current.incarnation(), &[task])?;
    assert!(current.pending_task_wakes().is_empty());
    let restored = WorkingMemory::new(8);
    restored.restore(load_volatile(owner)?.ok_or("checkpoint absent")?.wm);
    assert!(restored.pending_task_wakes().is_empty());
    Ok(())
}

#[tokio::test]
async fn rejected_read_batch_cannot_partially_retire_a_durable_wake(
) -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let registry = PersonaWorkspaceRegistry::new();
    let owner = Uuid::new_v4();
    let task = Uuid::new_v4();
    registry.register_from_cfg(cfg_for(owner))?;
    registry.consume_task_ends(owner, vec![(task, TaskEnd::Stopped)])?;
    let memory = Arc::clone(&registry.cycles.lock()[&owner].working_memory);
    let path = volatile_path(owner)?;
    let before = std::fs::read(&path)?;
    assert!(matches!(
        registry.record_task_reads(owner, memory.incarnation(), &[task, Uuid::new_v4()]),
        Err(TaskConsumptionError::Receipt(_))
    ));
    assert_eq!(std::fs::read(&path)?, before);
    // An unrelated later checkpoint must not publish a partial staging from the
    // refused batch. Checking only the immediate wake would miss that failure.
    for (_, result) in registry.flush_volatile_all() {
        result?;
    }
    let restored = WorkingMemory::new(8);
    restored.restore(load_volatile(owner)?.ok_or("checkpoint absent")?.wm);
    assert_eq!(memory.pending_task_wakes(), vec![task]);
    assert_eq!(restored.pending_task_wakes(), vec![task]);
    Ok(())
}

#[tokio::test]
async fn served_read_arriving_after_shutdown_preserves_restart_delivery(
) -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let registry = PersonaWorkspaceRegistry::new();
    let owner = Uuid::new_v4();
    let task = Uuid::new_v4();
    registry.register_from_cfg(cfg_for(owner))?;
    registry.consume_task_ends(owner, vec![(task, TaskEnd::Lost)])?;
    let memory = Arc::clone(&registry.cycles.lock()[&owner].working_memory);
    for (_, result) in registry.stop_volatile_checkpoints() {
        result?;
    }
    let path = volatile_path(owner)?;
    let before = std::fs::read(&path)?;
    assert!(matches!(
        registry.record_task_reads(owner, memory.incarnation(), &[task]),
        Err(TaskConsumptionError::Stopped)
    ));
    assert_eq!(std::fs::read(&path)?, before);
    let restored = WorkingMemory::new(8);
    restored.restore(load_volatile(owner)?.ok_or("checkpoint absent")?.wm);
    assert_eq!(memory.pending_task_wakes(), vec![task]);
    assert_eq!(restored.pending_task_wakes(), vec![task]);
    Ok(())
}
