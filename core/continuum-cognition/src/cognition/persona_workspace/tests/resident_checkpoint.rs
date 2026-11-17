#![cfg(test)]
use super::*;

// A successful IO is not permission to retire a newer memory or later work.
#[test]
fn resident_checkpoint_rejects_later_work_and_replacement() {
    let home = tempfile::tempdir().expect("test home");
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let registry = PersonaWorkspaceRegistry::new();
    let persona = Uuid::new_v4();
    registry.register_from_cfg(cfg_for(persona)).expect("resident");
    let target = registry.checkpoint_target(persona).expect("target");
    let saved = target.flush(&registry).expect("checkpoint");
    saved.verify(&registry).expect("unchanged memory is covered");
    registry.cycles.lock()[&persona].working_memory.record_receipt("completed after save");
    assert!(matches!(saved.verify(&registry), Err(ResidentCheckpointError::Changed)));
    let latest = target.flush(&registry).expect("save later work");
    let path = volatile_path(persona).expect("path");
    let bytes = std::fs::read(&path).expect("saved bytes");
    registry.register_from_cfg(cfg_for(persona)).expect("replacement restores saved work");
    assert!(matches!(latest.verify(&registry), Err(ResidentCheckpointError::Replaced)));
    assert!(matches!(target.flush(&registry), Err(ResidentCheckpointError::Replaced)));
    assert_eq!(std::fs::read(path).expect("preserved bytes"), bytes, "an old target cannot overwrite the successor's checkpoint");
}

#[test]
fn resident_checkpoint_failure_retains_memory_and_unrelated_checkpoint() {
    let home = tempfile::tempdir().expect("test home");
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let registry = PersonaWorkspaceRegistry::new();
    let persona = Uuid::new_v4();
    let other = Uuid::new_v4();
    registry.register_from_cfg(cfg_for(persona)).expect("resident");
    registry.register_from_cfg(cfg_for(other)).expect("other resident");
    let target = registry.checkpoint_target(persona).expect("target");
    registry.checkpoint_target(other).expect("other target").flush(&registry).expect("other saved");
    let other_path = volatile_path(other).expect("other path");
    let other_bytes = std::fs::read(&other_path).expect("other bytes");
    let path = volatile_path(persona).expect("path");
    std::fs::create_dir_all(&path).expect("a directory cannot be replaced by the checkpoint file");
    let memory = Arc::clone(&registry.cycles.lock()[&persona].working_memory);
    memory.record_receipt("must survive refused save");
    assert!(matches!(target.flush(&registry), Err(ResidentCheckpointError::Storage(_))));
    assert!(Arc::ptr_eq(&registry.cycles.lock()[&persona].working_memory, &memory));
    assert_eq!(std::fs::read(other_path).expect("other unchanged"), other_bytes);
}
