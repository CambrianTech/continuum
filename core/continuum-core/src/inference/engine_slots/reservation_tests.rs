//! Installer receipts can retain an engine independently of runtime pointers.
#![cfg(test)]
use super::*;

#[test]
fn shared_receipt_engine_is_preserved_while_another_slot_is_selected() {
    let root = Path::new("engines");
    let live = [Some(root.join("engine-c").join(exe_name()))];
    assert_eq!(idle_slot(root, Some("engine-c"), Some("engine-b"), &live, |_| true), Ok("engine-a"));
    assert_eq!(idle_slot_reserved(root, Some("engine-c"), Some("engine-b"), &live, &["engine-a"], |_| true), Ok("engine-b"));
    assert_eq!(idle_slot_reserved(root, Some("engine-c"), Some("engine-b"), &live, &["engine-a", "engine-b"], |_| true), Err(NotIdle::AllInUse));
}

#[test]
fn reservations_never_release_live_or_unknown_lanes() {
    let root = Path::new("engines");
    let live = [Some(root.join("engine-b").join(exe_name()))];
    assert_eq!(idle_slot_reserved(root, Some("engine-c"), None, &live, &["engine-a"], |_| true), Err(NotIdle::AllInUse));
    assert_eq!(idle_slot_reserved(root, None, None, &[None], &["engine-a"], |_| true), Err(NotIdle::AllInUse));
}
