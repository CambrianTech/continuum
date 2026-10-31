#![cfg(test)]
use super::*;

// what this catches: the Anwen regression (2026-08-16). Act results used to
// live only in the K=1 `last_action` slot (any act overwrites) and the
// settlement-gated pinned block — so a compile error vanished the moment she
// glanced at a file listing, and she re-ran the same unfixed code for three
// turns. The recent-results channel must keep a finding visible past
// subsequent trivial acts, and the buffer must stay inside its
// window-derived char bound.
#[test]
fn recent_results_survive_subsequent_acts_and_stay_bounded() {
    let wm = WorkingMemory::new(8);
    // A known window, as the act→observe seam pins it before any receipt in production.
    record_saved(&wm, "[action] code/run(sol.rs) error[E0601]: `main` function not found");
    record_saved(&wm, "[action] code/list() sol.rs sol2.rs");
    record_saved(&wm, "[action] code/tree() workspace/");
    let block = wm.recent_results_messages().join("\n");
    assert!(
        block.contains("E0601"),
        "the compile error must survive later trivial acts: {block}"
    );
    // A shed (only the memory owner's, under pressure) collapses the OLDEST to pointers,
    // and every result still shown is shown whole.
    let unit = BODY;
    for _ in 0..8 {
        record_saved(&wm, &"x".repeat(unit));
    }
    shed(&wm, 4 * unit);
    let messages = wm.recent_results_messages();
    let kept: Vec<&String> = messages.iter().filter(|m| m.starts_with("[result ")).collect();
    assert!(!kept.is_empty(), "the newest results stay");
    assert!(
        kept.iter().all(|m| m.contains(&"x".repeat(unit))),
        "every result still shown is shown WHOLE, never a tail"
    );
    assert!(messages.iter().any(|m| m.contains("collapsed")), "the rest are pointers");
    assert!(
        !messages.join("\n").contains("E0601"),
        "the oldest collapse to pointers once shed — bounded, not immortal"
    );
    assert!(wm.resident_result_bytes() <= 4 * unit as u64, "down to the allocation: {}", wm.resident_result_bytes());
    // Between sheds the ring only appends: receipts and settles move nothing, so the head
    // stays byte-identical (the KV divergence audit, 2026-09-13).
    let head_after_collapse = wm.recent_results_messages().get(1).cloned();
    record_saved(&wm, "[action] code/read(a.rs) fine");
    wm.record_settlement("one", None);
    record_saved(&wm, "[action] code/read(b.rs) fine");
    wm.record_settlement("two", None);
    assert_eq!(
        wm.recent_results_messages().get(1).cloned(),
        head_after_collapse,
        "the ledger head is byte-stable across receipts and settles between sheds"
    );
}

// what this catches: e731576c — actual settlement records must be kept, in order and whole,
// even when a one-word correction resembles an older answer, or a later answer repeats the
// original state.
#[test]
fn settlement_trail_preserves_corrections_and_later_repeated_reports() {
    let passed = "I verified the candidate build and all regression tests passed \
                  after checking the shared workspace changes against the review requirements.";
    let failed = "I verified the candidate build and all regression tests failed \
                  after checking the shared workspace changes against the review requirements.";
    assert!(super::super::super::deliberation_budget::near_identical_substantial(passed, failed));
    let wm = WorkingMemory::new(8);
    wm.record_settlement(passed, None);
    wm.record_receipt("code/read found the regression");
    wm.record_settlement(failed, None);
    wm.record_settlement(passed, None);
    let entries = wm.recent_entries();
    assert_eq!(
        entries.iter().map(|entry| &entry.kind).collect::<Vec<_>>(),
        vec![
            &WmKind::Settlement,
            &WmKind::Receipt { n: 1 },
            &WmKind::Settlement,
            &WmKind::Settlement
        ]
    );
    let texts: Vec<&str> = entries.iter().map(|entry| entry.text.as_str()).collect();
    assert!(texts[0].ends_with(passed), "the first report, whole: {}", texts[0]);
    assert!(texts[2].ends_with(failed), "the correction, whole, though it resembles the first: {}", texts[2]);
    assert!(texts[3].ends_with(passed), "the later repeat, kept as its own entry: {}", texts[3]);
}

// what this catches: loop-awareness — `note_action_fingerprint` counts repeats of the
// IDENTICAL call so the act→observe step can surface "you've issued this N times." A
// different call resets to 1; the count rises only on exact repeats. This is the explicit
// proprioception that breaks a smaller model out of the search-loop the glass box caught.
// (Stray duplicate #[test] removed 2026-07-12 — it made the next test run twice.)
// what this catches: the investigation-shape fact's source of truth — the
// verb tally aggregates fingerprints by tool name, most-used first, so
// "9 acts, all code/search" is derivable as pure structure (SWE flask-4045
// glass-box: distinct searches never tripped the exact-repeat note and the
// imbalance was invisible). // regression for the [investigation] brick
#[test]
fn action_verb_tally_aggregates_by_tool_name() {
    let wm = WorkingMemory::new(16);
    for args in [
        "{\"pattern\":\"a\"}",
        "{\"pattern\":\"b\"}",
        "{\"pattern\":\"c\"}",
    ] {
        wm.note_action_fingerprint(&format!("code/search|{args}"));
    }
    wm.note_action_fingerprint("code/read|{\"file_path\":\"x.py\"}");
    let tally = wm.action_verb_tally();
    assert_eq!(
        tally[0],
        ("code/search".to_string(), 3),
        "most-used first: {tally:?}"
    );
    assert_eq!(tally[1], ("code/read".to_string(), 1));
}

#[test]
fn recent_action_status_replaces_rolling_counts_without_replacing_history() {
    let wm = WorkingMemory::new(3);
    wm.record_fact("independent historical fact");
    for call in ["code/search|a", "code/search|b", "code/read|c"] {
        wm.note_action_fingerprint(call);
    }
    wm.refresh_recent_action_status();
    assert!(wm.pinned_facts()[0].contains("code/search ×2"));
    wm.note_action_fingerprint("code/read|d");
    wm.refresh_recent_action_status();
    wm.refresh_recent_action_status();
    let notices = wm.pinned_facts();
    assert_eq!(notices.len(), 1);
    assert!(notices[0].contains("3 most recent retained calls"));
    assert!(notices[0].contains("code/search ×1"));
    assert!(notices[0].contains("code/read ×2"));
    assert!(!notices[0].contains("so far"));
    assert!(wm.snapshot().entries.iter().any(|entry| entry.text == "independent historical fact"));
    wm.clear();
    assert!(wm.pinned_facts().is_empty());
    wm.refresh_recent_action_status();
    assert!(wm.pinned_facts().is_empty());
}

// what this catches: the Memento fix's foundation — the volatile tier
// round-trips through the JSON snapshot losslessly: typed entries (kinds
// intact), the full last result, fingerprints, and the receipt counter
// (so post-restore receipts keep ascending numbers instead of colliding
// with restored ones). The NEWEST fact names the checkpoint's save age and
// recorded pending labels without inventing interruption time or outcomes.
#[test]
fn volatile_snapshot_round_trips_and_renders_the_interruption() {
    let wm = WorkingMemory::new(8);
    wm.record("thinking about the tokenizer");
    wm.record_receipt("I ran code/list({\"path\":\".\"}) Result: 4 files");
    wm.record_fact("[unfulfilled] I said I would run commands, but no tool ran");
    wm.record_settlement("shared the plan", None);
    wm.note_action_fingerprint("code/list|{\"path\":\".\"}");
    // A dispatched compile still Running at snapshot time — only its LABEL
    // survives, not evidence about later completion or side effects.
    let handle = Uuid::new_v4();
    wm.record_dispatch_event(
        handle,
        "cargo build (dispatched)",
        "compiling…",
        DispatchStatus::Running,
    );
    let snap = wm.snapshot();
    assert_eq!(
        snap.interrupted_dispatches,
        vec!["cargo build (dispatched)"]
    );
    assert!(snap.saved_at_ms > 0);
    let json = serde_json::to_string(&snap).expect("serializes");
    let back: VolatileSnapshot = serde_json::from_str(&json).expect("deserializes");

    let fresh = WorkingMemory::new(8);
    fresh.restore(back);
    // Everything restored, and the [resumed] notice PINNED (a two-turn pin, never a
    // ring entry — 2026-09-16, the notice that outlived a whole life).
    let restored = fresh.recent();
    assert_eq!(
        restored.as_slice(),
        wm.recent().as_slice(),
        "the window is identical; the notice is not an entry"
    );
    let resumed = fresh.pinned_facts();
    assert!(
        resumed[0].starts_with("[resumed] your memory was restored from a checkpoint saved ")
            && resumed[0].contains(" ago."),
        "the age describes the checkpoint save: {resumed:?}"
    );
    assert!(
        resumed[0].contains(
            "Dispatches recorded as pending at that save: cargo build (dispatched)."
        ) && resumed[0].contains("Their completion and side effects are unknown."),
        "pending work is named without inventing its outcome: {resumed:?}"
    );
    for unsupported in [
        "was interrupted",
        "NOT completed",
        "safe to repeat",
        "nothing was in flight",
    ] {
        assert!(
            !resumed[0].contains(unsupported),
            "unsupported claim {unsupported:?}: {resumed:?}"
        );
    }
    assert!(
        !restored.iter().any(|l| l.contains("[rebuilt]")),
        "SAME build across the restart — no rebuild fact, or we cry wolf on every \
         ordinary restart and the marker stops meaning anything: {restored:?}"
    );
    assert!(fresh.has_receipt(), "receipt kind survives the trip");
    assert_eq!(
        fresh.last_action_full(),
        wm.last_action_full(),
        "full latest result survives"
    );
    // The counter resumes PAST the restored receipts — the next act gets a
    // fresh number, never a collision with a restored one. (#3, not #2:
    // the dispatched compile consumed a seq too — dispatches are acts.)
    fresh.record_receipt("I ran code/tree({}) Result: ok");
    let last = fresh.recent();
    assert!(
        last.last().unwrap().starts_with("[action #3]"),
        "counter resumed: {last:?}"
    );

    // Missing and zero save times remain unknown even with pending labels.
    for missing_saved_at in [false, true] {
        let mut legacy: serde_json::Value = serde_json::from_str(&json).expect("snapshot JSON");
        if missing_saved_at {
            let _ = legacy
                .as_object_mut()
                .expect("snapshot object")
                .remove("saved_at_ms");
        } else {
            legacy["saved_at_ms"] = serde_json::json!(0);
        }
        let legacy = serde_json::from_value(legacy).expect("legacy snapshot deserializes");
        let restored_legacy = WorkingMemory::new(8);
        restored_legacy.restore(legacy);
        let pinned = restored_legacy.pinned_facts();
        let resumed = pinned.first().expect("restored checkpoint notice");
        assert!(
            resumed.contains("checkpoint with an unknown save time."),
            "{resumed}"
        );
        assert!(!resumed.contains("ago"), "no fabricated save age: {resumed}");
        assert!(resumed.contains("cargo build (dispatched)"), "{resumed}");
        assert!(
            resumed.contains("Their completion and side effects are unknown."),
            "{resumed}"
        );
    }

    // An empty list describes this checkpoint, not everything that was running.
    let quiet = WorkingMemory::new(8);
    quiet.restore(VolatileSnapshot {
        pending_emissions: Vec::new(),
        entries: Vec::new(),
        action_statements: Vec::new(),
        last_action: None,
        action_fps: Vec::new(),
        recent_results: Vec::new(),
        next_action_seq: 1,
        saved_at_ms: 0, // pre-field snapshot: no save time guessed
        interrupted_dispatches: Vec::new(),
        build_sha: String::new(), // pre-field snapshot: no rebuild guessed either
        acting_card: None,
        handoff: None,
        actions: Vec::new(),
        context_view: None,
        receipt_children: Default::default(),
        open_views: Vec::new(),
    });
    assert!(quiet.recent().is_empty(), "an empty snapshot restores an empty window");
    let q = quiet.pinned_facts();
    assert_eq!(q.len(), 1);
    assert!(
        q[0].contains("No pending dispatches were recorded in that checkpoint."),
        "{q:?}"
    );
    assert!(!q[0].contains("nothing was in flight"), "{q:?}");
    assert!(q[0].contains("checkpoint with an unknown save time."), "{q:?}");
    assert!(
        !q[0].contains("ago"),
        "no fabricated save age on legacy snapshots: {q:?}"
    );
}
