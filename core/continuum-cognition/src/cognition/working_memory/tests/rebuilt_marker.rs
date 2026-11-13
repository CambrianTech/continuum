#![cfg(test)]
use super::*;

#[test]
fn authored_continuity_survives_checkpoint_without_breaking_rollback_or_root_boundaries() {
    // This is the pre-authored-entry wire vocabulary. Unknown enum variants
    // fail serde decoding; unknown snapshot fields are ignored on rollback.
    #[derive(serde::Deserialize)]
    enum LegacyKind {
        Thought,
        Receipt { n: u64 },
        Fact,
        Settlement,
    }
    #[derive(serde::Deserialize)]
    struct LegacyEntry {
        kind: LegacyKind,
        text: String,
    }
    #[derive(serde::Deserialize)]
    struct LegacySnapshot {
        entries: Vec<LegacyEntry>,
    }

    let room = Uuid::new_v4();
    let wm = WorkingMemory::new(16);
    wm.record_fact("prior fact");
    wm.record_action_statement("unrooted decision", room);
    wm.record_settlement("another room finished", Some(Uuid::new_v4()));
    wm.record_settlement("unknown room finished", None);
    assert_eq!(wm.action_statement(room).as_deref(), Some("unrooted decision"));
    wm.set_scope(Some("task-a".into()));
    assert_eq!(wm.action_statement(room), None, "a newly inferred root cannot inherit unscoped authorship");
    wm.record_action_statement("task-a decision", room);
    wm.record_settlement("another room in task-a finished", Some(Uuid::new_v4()));
    assert_eq!(wm.action_statement(room).as_deref(), Some("task-a decision"));
    wm.record_receipt("read the declaration");
    wm.set_scope(Some("task-b".into()));
    wm.record_action_statement("task-b decision", room);
    wm.record_settlement("task-b finished", Some(room));
    assert_eq!(wm.action_statement(room), None);
    wm.set_scope(None);
    assert_eq!(wm.action_statement(room).as_deref(), Some("unrooted decision"));
    wm.record_settlement("conversation finished", Some(room));
    wm.set_scope(Some("task-a".into()));
    assert_eq!(wm.action_statement(room).as_deref(), Some("task-a decision"), "other roots and unscoped settlement cannot close task-a");

    let before = wm.recent_entries();
    let json = serde_json::to_vec(&wm.snapshot()).expect("checkpoint wire");
    let legacy: LegacySnapshot = serde_json::from_slice(&json).expect("rollback reader keeps prior entries");
    assert!(legacy.entries.iter().any(|e| matches!(e.kind, LegacyKind::Fact) && e.text == "prior fact"));
    assert!(legacy.entries.iter().any(|e| matches!(e.kind, LegacyKind::Receipt { n: 1 })));
    assert!(legacy.entries.iter().all(|e| !e.text.contains("decision")), "older renderers cannot relabel authored text as private thought");

    let restored = WorkingMemory::new(16);
    restored.restore(serde_json::from_slice(&json).expect("current reader"));
    assert_eq!(restored.action_statement(room), None, "restore does not guess the current root");
    restored.set_scope(Some("task-a".into()));
    assert_eq!(restored.action_statement(room).as_deref(), Some("task-a decision"));
    assert_eq!(restored.recent_entries().iter().map(|e| (&e.kind, &e.text)).collect::<Vec<_>>(), before.iter().map(|e| (&e.kind, &e.text)).collect::<Vec<_>>(), "authored entries retain their ring position");
    restored.record_settlement("task-a finished", Some(room));
    assert_eq!(restored.action_statement(room), None);
    let settled = WorkingMemory::new(16);
    settled.restore(restored.snapshot());
    settled.set_scope(Some("task-a".into()));
    assert_eq!(settled.action_statement(room), None, "settled authorship cannot resurrect on restore");
}

fn snap_from(wm: &WorkingMemory, build: &str) -> VolatileSnapshot {
    let mut s = wm.snapshot();
    s.build_sha = build.to_string();
    s
}

// what this catches: #165, measured live 2026-08-07. After the #326 adapter fix,
// `work/list(claimable=true)` returned real cards and ZERO refusals occurred — and
// citizens kept reporting "attempting to claim tasks or list them without success",
// because the OLD refusal was still the newest thing they remembered. Correct
// behaviour, dead world. Restore must say the ground moved.
#[test]
fn a_rebuild_across_the_restore_becomes_a_perceivable_fact() {
    let wm = WorkingMemory::new(8);
    wm.record_receipt("work/list(state=open) → forbidden: no policy grants access");
    let fresh = WorkingMemory::new(8);
    fresh.restore(snap_from(&wm, "0000000deadbeef"));

    let lines = fresh.recent();
    let pinned = fresh.pinned_facts();
    let marker = pinned
        .iter()
        .find(|l| l.contains("[rebuilt]"))
        .unwrap_or_else(|| panic!("rebuild must be perceivable: {pinned:?}"));
    assert!(
        marker.contains("0000000de"),
        "names the build she was recorded against: {marker}"
    );
    // what this catches (Freya, 2026-09-06, after the tenth deploy of the night): the
    // fact read as "distrust everything above" and every citizen re-read her whole
    // checkout after every deploy. Her workspace did not change; the fact says so and
    // reserves doubt for the tools.
    assert!(
        marker.contains("UNCHANGED") && marker.contains("resume from your last conclusion"),
        "the fact must say her workspace and reads still stand: {marker}"
    );
    assert!(
        marker.contains("re-trying") || marker.contains("not evidence"),
        "tells her the receipts are not evidence about NOW: {marker}"
    );
    // NEVER deletes the memory — proprioception depends on the full trace.
    assert!(
        lines.iter().any(|l| l.contains("forbidden")),
        "the receipt itself survives; only its authority is qualified: {lines:?}"
    );
}

// what this catches: crying wolf. A snapshot written before this field exists
// (serde default = "") must NOT be reported as a rebuild — restore says nothing
// rather than guessing a change it cannot see.
#[test]
fn a_snapshot_with_no_build_recorded_stays_silent() {
    let wm = WorkingMemory::new(8);
    wm.record_receipt("work/list() → ok");
    let fresh = WorkingMemory::new(8);
    fresh.restore(snap_from(&wm, ""));
    assert!(
        !fresh.pinned_facts().iter().any(|l| l.contains("[rebuilt]")),
        "unknown build is not a known change"
    );
}

// what this catches (Kira, 2026-09-16): the wake notices outliving the wake. As ring
// entries they were checkpointed and restored forever — "saved under a minute ago"
// on every turn for five hours. They are two-turn pins now, never entries, and a
// snapshot that still carries the old lines as entries loses them on the way in.
#[test]
fn the_wake_notices_last_two_turns_and_never_ride_a_checkpoint() {
    let wm = WorkingMemory::new(8);
    wm.record_receipt("code/read(a.rs) → 40 lines");
    wm.record_fact("[resumed] your memory was restored from a checkpoint saved ~54 min ago.");
    wm.record_fact("[rebuilt] the substrate was rebuilt while you were away (a → b).");
    let fresh = WorkingMemory::new(8);
    fresh.restore(snap_from(&wm, "0000000deadbeef"));
    // Only the receipt came back as memory; the old notices were dropped.
    let lines = fresh.recent();
    assert!(lines.iter().any(|l| l.contains("code/read")), "{lines:?}");
    assert!(!lines.iter().any(|l| l.contains("[resumed]") || l.contains("[rebuilt]")), "{lines:?}");
    // The NEW notices are pins: present at the wake, gone after two work turns.
    let pinned = fresh.pinned_facts();
    assert!(pinned.iter().any(|l| l.starts_with("[resumed]")), "{pinned:?}");
    assert!(pinned.iter().any(|l| l.starts_with("[rebuilt]")), "{pinned:?}");
    fresh.end_of_work_turn();
    assert!(fresh.pinned_facts().iter().any(|l| l.starts_with("[resumed]")), "the next turn still reads it");
    fresh.end_of_work_turn();
    assert!(fresh.pinned_facts().is_empty(), "{:?}", fresh.pinned_facts());
    // And a checkpoint taken from the restored mind never carries them.
    let again = fresh.snapshot();
    assert!(!again.entries.iter().any(|w| w.text.starts_with("[resumed]") || w.text.starts_with("[rebuilt]")));
}
