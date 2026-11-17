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

// what this catches (Codex's review of 9cf06ade): a head record that does not decode, rewritten by
// the next checkpoint as its normalized parse, so the bytes that were on disk are gone. Its whole
// checkpoint is kept, content-addressed, at the load that finds it, before any rewrite; a later load
// of the rewritten checkpoint keeps the reference and preserves nothing new.
#[test]
fn a_corrupt_head_record_keeps_its_original_checkpoint_byte_for_byte() {
    use crate::cognition::hands_custody::{CorruptHands, HandsCustody};
    let dir = tempfile::tempdir().expect("test: tempdir");
    let _home = crate::paths::NativeHomeOverride::install(&dir.path().join("native"));
    let persona = Uuid::new_v4();
    let memory = WorkingMemory::new(8);
    let unreadable = CorruptHands { raw: serde_json::json!({"generation": "x"}), error: "invalid type".to_string(), original: None };
    memory.keep_custody(None, HandsCustody::Corrupt(unreadable));
    save_volatile(persona, SaveReason::Seam, None, &memory).expect("test: checkpoint");
    let path = volatile_path(persona).expect("test: path");
    let on_disk = std::fs::read(&path).expect("test: read");

    let restored = load_volatile(persona).expect("test: load").expect("test: a checkpoint");
    let HandsCustody::Corrupt(record) = &restored.wm.context_hands else {
        panic!("a corrupt record restores corrupt"); // JUSTIFIED: test fixture
    };
    let kept = record.original.clone().expect("its checkpoint is kept at the load that found it");
    assert_eq!(std::fs::read(&kept.path).expect("test: read kept"), on_disk, "byte for byte");

    let again = WorkingMemory::new(8);
    again.restore(restored.wm);
    save_volatile(persona, SaveReason::Seam, None, &again).expect("test: rewrite");
    let reread = load_volatile(persona).expect("test: load").expect("test: a checkpoint");
    assert!(matches!(&reread.wm.context_hands, HandsCustody::Corrupt(record) if record.original.as_ref() == Some(&kept)), "the reference survives the rewrite");
    let kept_files = std::fs::read_dir(kept.path.parent().expect("test: parent")).expect("test: list").count();
    assert_eq!(kept_files, 1, "nothing re-preserved from the rewritten bytes");
}
