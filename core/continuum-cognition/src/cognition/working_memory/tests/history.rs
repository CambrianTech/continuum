#![cfg(test)]
use super::*;
use crate::cognition::tool_executor::spill;

/// A mind with an archive, in the store fixture's own home (never `~/.continuum`).
fn archived_mind(persona: Uuid) -> WorkingMemory {
    spill::prepare_store_fixture();
    let wm = WorkingMemory::new(8);
    wm.bind_archive(persona).expect("test: her archive opens in the fixture's store");
    wm
}

/// A hex handle no file backs: what a record names, for tests that never read the output.
fn hex(seq: u64) -> String {
    format!("{seq:016x}")
}

/// Record `count` acts, each saved under its own hex handle, and move every body but the newest
/// to its handle (as the memory owner's shed does under pressure).
fn act(wm: &WorkingMemory, count: u64) {
    for _ in 0..count {
        let seq = wm.actions_taken() + 1;
        wm.record_receipt_typed(&[], &format!("act {seq} read a file"), None, saved(&hex(seq)));
    }
    shed(wm, 0);
}

/// Her view's start, as `release_folded` reads it: the newest `unfolded` acts stay unfolded, the
/// rest are its fold run. The number is the test's view; production reads the start's measured edge.
fn start(wm: &WorkingMemory, unfolded: u64) {
    wm.release_through(wm.actions_taken().saturating_sub(unfolded));
}

/// The archive directory's files, by suffix.
fn archive_files(persona: Uuid, suffix: &str) -> usize {
    let dir = spill::store().expect("test: the store").owned_by(persona).path(std::path::Path::new("archive")).expect("test: path");
    std::fs::read_dir(dir).map_or(0, |entries| entries.filter_map(Result::ok).filter(|e| e.file_name().to_string_lossy().ends_with(suffix)).count())
}

fn checkpoint_bytes(wm: &WorkingMemory) -> usize {
    serde_json::to_vec(&wm.snapshot()).expect("test: a snapshot serializes").len()
}

// what this catches (the M5, 2026-11-10): a resident keeping her WHOLE history in RAM and in her
// checkpoint (4,522-5,686 records, 12-46 MB of JSON rewritten every ~15 s per mind), and an archive
// that only appends. After 5,000 acts her resident log is what her view shows unfolded plus the acts
// since its last start, her checkpoint is no larger than at act 1,000, and her archive compacted
// everything past her measured reopen horizon and drains a span retention deleted.
#[test]
fn five_thousand_acts_keep_a_bounded_log_a_flat_checkpoint_and_a_draining_archive() {
    let persona = Uuid::new_v4();
    let wm = archived_mind(persona);
    let (unfolded, per_start) = (50, 100);
    let mut at_1000 = 0;
    for round in 1..=50 {
        act(&wm, per_start);
        start(&wm, unfolded);
        wm.tick_archive().expect("test: a tick writes the outbox");
        if round == 10 {
            at_1000 = checkpoint_bytes(&wm);
        }
        if round == 5 {
            // One reopen of an archived act, 120 acts after it: her measured reopen horizon.
            let seq = wm.actions_taken() - 120;
            assert_eq!(wm.saved_handle(seq), Ok(hex(seq)), "an archived act reopens by its number");
        }
    }
    assert_eq!(wm.actions_taken(), 5_000);
    assert!(wm.resident_acts() as u64 <= unfolded, "resident: {} acts, her view's unfolded {unfolded}", wm.resident_acts());
    let at_5000 = checkpoint_bytes(&wm);
    assert!(at_5000 <= at_1000 + at_1000 / 10, "the checkpoint grew with history: {at_1000} -> {at_5000} bytes");
    // Compaction is one span per tick: tick until it settles.
    for _ in 0..wm.archive_segments() {
        wm.tick_archive().expect("test: a compaction step");
    }
    let (lines, spans) = (archive_files(persona, ".acts.jsonl"), archive_files(persona, ".span.json"));
    assert!(spans >= 45, "segments past her reopen horizon compacted: {spans} spans");
    assert!(lines <= 3, "only segments inside her reopen horizon keep their lines: {lines}");
    // Retention deletes her oldest span under disk pressure (the tool-output pool); the archive
    // notices at its oldest end and forgets it.
    let segments = wm.archive_segments();
    let oldest = spill::store().expect("test: store").owned_by(persona).path(std::path::Path::new(&format!("archive/{:020}-{:020}.span.json", 1, 50))).expect("test: path");
    std::fs::remove_file(&oldest).expect("test: the oldest span is where its range names it");
    wm.tick_archive().expect("test: a retention tick");
    assert_eq!(wm.archive_segments(), segments - 1, "a retired span leaves the index");
    assert!(matches!(wm.saved_handle(10), Err(ActOutputRefusal::Unknown { .. })), "nothing of a retired span remains to reopen");
}

// what this catches: an act that left RAM no longer reopening (her `#n` is how every fold block and
// pointer tells her to get it back), and a compacted one answering with an error string or with some
// other act's output instead of its span's summary, typed.
#[test]
fn an_archived_act_reopens_by_number_and_a_compacted_one_as_its_span() {
    let persona = Uuid::new_v4();
    let wm = archived_mind(persona);
    act(&wm, 10);
    start(&wm, 2);
    assert_eq!(wm.resident_acts(), 2);
    assert_eq!(wm.saved_handle(3), Ok(hex(3)), "from the outbox, before any tick wrote it");
    wm.tick_archive().expect("test: tick");
    assert_eq!(wm.saved_handle(4), Ok(hex(4)), "from its segment, read for this call");
    // That reopen was 6 acts after act 4: her horizon is 6. Twenty acts later the segment is past it.
    act(&wm, 20);
    start(&wm, 2);
    wm.tick_archive().expect("test: tick compacts the first segment");
    match wm.saved_handle(4) {
        Err(ActOutputRefusal::Compacted { act, span }) => {
            assert_eq!(act, 4);
            assert_eq!((span.first, span.last, span.acts), (1, 8, 8), "its span, named");
            assert!(span.to_string().contains("unrecorded ×8"), "what she did in it: {span}");
        }
        other => panic!("a compacted act answers with its span's summary, got {other:?}"),
    }
    assert_eq!(wm.saved_handle(29), Ok(hex(29)), "a resident act is untouched");
}

// what this catches: a restore that reloads her whole history (the M5's restore read all of it), or
// one that loses what left RAM. The checkpoint carries only her resident acts; an older act pages in
// from her archive by its number on the first reopen, and nothing else of it is loaded.
#[test]
fn a_restore_loads_only_her_resident_acts_and_an_older_one_pages_in() {
    let persona = Uuid::new_v4();
    let wm = archived_mind(persona);
    act(&wm, 300);
    start(&wm, 50);
    wm.tick_archive().expect("test: tick");
    let snapshot = wm.snapshot();
    assert_eq!(snapshot.actions.len(), 50, "the checkpoint carries what her view showed unfolded");
    drop(wm);
    let woke = archived_mind(persona);
    woke.restore(snapshot);
    assert_eq!(woke.resident_acts(), 50, "restore loads the bounded part");
    assert_eq!(woke.saved_handle(10), Ok(hex(10)), "an older act pages in by its number");
    assert_eq!(woke.resident_acts(), 50, "and is not re-pinned resident");
}

// what this catches: a memory with nowhere durable for an act to go (a fork, a test that asks for
// none) losing acts when her view folds them.
#[test]
fn an_unbound_memory_keeps_its_log() {
    let wm = WorkingMemory::new(8);
    act(&wm, 20);
    assert_eq!(wm.release_through(15), 0);
    assert_eq!(wm.resident_acts(), 20);
}

// what this catches: her view folding around the read of a file she is editing (her work in hand,
// `working_memory::in_hand`) while the memory owner archives every act up to the fold edge: the read
// her view keeps whole would leave RAM and her next start would not find it. The release stops
// before that read, and resumes past it once no edit holds it.
#[test]
fn a_fold_past_the_file_she_is_editing_leaves_its_read_resident() {
    let persona = Uuid::new_v4();
    let wm = archived_mind(persona);
    act(&wm, 5);
    let read = wm.actions_taken() + 1;
    super::open_views::record_read_and_edit(&wm);
    act(&wm, 5);
    assert_eq!(wm.in_hand().reads, std::collections::HashSet::from([ActSeq(read)]), "precondition: her edit holds the read");
    wm.release_through(wm.actions_taken() - 1);
    assert_eq!(wm.resident_acts() as u64, wm.actions_taken() - (read - 1), "every act from the read on stays resident");
}

// what this catches: her thought log only growing (every thought she ever had, in RAM and in every
// checkpoint), the shape her action log had before her view's fold edge drained it. Thoughts up to
// the edge whose wholes are saved leave; a thought whose body is in RAM alone, her note on an edit
// in hand, and everything past the edge stay.
#[test]
fn her_thoughts_drain_through_her_views_fold_edge() {
    let persona = Uuid::new_v4();
    let wm = archived_mind(persona);
    for n in 0..6 {
        wm.record(&format!("thought {n}"));
    }
    wm.save_unsaved_thoughts(persona);
    wm.record("thought 6, not yet saved");
    let occurrences: Vec<u64> = wm.thoughts().iter().map(|thought| thought.occurrence).collect();
    let key_of = |occurrence| crate::cognition::context_streams::thoughts::Thoughts::key_of(persona, occurrence);
    let kept = std::collections::HashSet::from([ThoughtOccurrence(occurrences[1])]);
    let released = wm.thoughts.lock().drain_through(key_of, &key_of(occurrences[3]), &kept);
    assert_eq!(released, [0, 2, 3].map(|index| key_of(occurrences[index])).into_iter().collect::<std::collections::HashSet<String>>(), "saved thoughts up to the edge leave, her note in hand stays");
    let left: Vec<u64> = wm.thoughts().iter().map(|thought| thought.occurrence).collect();
    assert_eq!(left, vec![occurrences[1], occurrences[4], occurrences[5], occurrences[6]], "past the edge, and unsaved, they stay");
}
