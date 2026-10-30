use super::*;
use continuum_kernel::runtime::tasks::TaskEnd;

#[tokio::test]
async fn forgotten_end_is_committed_before_receipt_and_restored_with_wake(
) -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let registry = PersonaWorkspaceRegistry::new();
    let owner = Uuid::new_v4();
    let task = Uuid::new_v4();
    registry.register_from_cfg(cfg_for(owner))?;
    let receipt = registry.consume_task_end(owner, task, TaskEnd::Stopped)?;
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
    assert!(memory.admit_task_wake(task));
    let replay = registry.consume_task_end(owner, task, TaskEnd::Stopped)?;
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
        matches!(registry.consume_task_end(owner, task, TaskEnd::Lost), Err(TaskConsumptionError::Absent(id)) if id == owner)
    );
    registry.register_from_cfg(cfg_for(owner))?;
    registry.consume_task_end(owner, task, TaskEnd::Lost)?;
    let path = volatile_path(owner)?;
    let before = std::fs::read(&path)?;
    assert!(matches!(
        registry.consume_task_end(owner, task, TaskEnd::Stopped),
        Err(TaskConsumptionError::Receipt(_))
    ));
    assert_eq!(std::fs::read(&path)?, before);
    for (_, result) in registry.stop_volatile_checkpoints() {
        result?;
    }
    let before = std::fs::read(&path)?;
    assert!(matches!(
        registry.consume_task_end(owner, Uuid::new_v4(), TaskEnd::Lost),
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
        registry.consume_task_end(owner, task, TaskEnd::Lost),
        Err(TaskConsumptionError::Checkpoint(_))
    ));
    assert_eq!(std::fs::read(&path)?, before);
    std::fs::remove_dir(&obstruction)?;
    let receipt = registry.consume_task_end(owner, task, TaskEnd::Lost)?;
    assert_eq!(receipt.consumed_tasks(), &[task]);
    let restored = WorkingMemory::new(8);
    restored.restore(load_volatile(owner)?.ok_or("checkpoint absent")?.wm);
    assert_eq!(restored.snapshot().task_results.consumed_ids(), vec![task]);
    assert_eq!(restored.pending_task_wakes(), vec![task]);
    Ok(())
}
