#![cfg(test)]
use super::*;

// what this catches (the M5, 2026-11-10): every resident's checkpoint rewritten whole every ~15 s
// whether or not anything changed (120 writes per 30 min at 51-161 ms each, ~44 s of core CPU per
// 30 min across four minds). A periodic checkpoint whose mind has not moved since the last write
// returns before her snapshot is taken: no serialization, 0 bytes, the file untouched. The first
// change writes again.
#[test]
fn a_periodic_checkpoint_with_nothing_changed_writes_nothing() {
    let dir = tempfile::tempdir().expect("test: tempdir");
    let _home = crate::paths::NativeHomeOverride::install(&dir.path().join("native"));
    let persona = Uuid::new_v4();
    let memory = WorkingMemory::new(8);
    memory.record_receipt_typed(&[], "the first act's result", None, crate::ai::types::ResultCustody::Unsaved);
    let first = save_volatile(persona, SaveReason::Periodic, None, &memory).expect("test: first checkpoint");
    assert!(matches!(first, Checkpointed::Written { bytes, .. } if bytes > 0), "{first:?}");
    let path = volatile_path(persona).expect("test: path");
    let (written, modified) = (std::fs::read(&path).expect("test: read"), std::fs::metadata(&path).and_then(|m| m.modified()).expect("test: mtime"));

    let again = save_volatile(persona, SaveReason::Periodic, None, &memory).expect("test: second checkpoint");
    assert!(matches!(again, Checkpointed::Unchanged { .. }), "nothing changed, nothing written: {again:?}");
    assert_eq!(std::fs::read(&path).expect("test: read"), written, "the file is the last write's");
    assert_eq!(std::fs::metadata(&path).and_then(|m| m.modified()).expect("test: mtime"), modified, "and was not rewritten");

    memory.record_receipt_typed(&[], "a second act", None, crate::ai::types::ResultCustody::Unsaved);
    let moved = save_volatile(persona, SaveReason::Periodic, None, &memory).expect("test: third checkpoint");
    assert!(matches!(moved, Checkpointed::Written { .. }), "a change writes: {moved:?}");
    let seam = save_volatile(persona, SaveReason::Seam, None, &memory).expect("test: seam checkpoint");
    assert!(matches!(seam, Checkpointed::Written { .. }), "a seam always writes its handoff: {seam:?}");
}
