//! Pages regression fixtures.
#![cfg(test)]
use super::*;
// what this catches (the M5, 2026-10-23 04:06Z): a relaunch from a 112,384- to a
// 129,024-token window crossed a page-dir step and the spawn swept the old dir, 98 GB,
// every resident's page. (a) the page dir names the model only, never the window; (b) the
// newest legacy per-window dir moves INTO it once, by rename, and nothing is deleted: an
// older legacy dir, another model's dir and a live lane's dir (protected by NAME, however
// the registry spelled its path) stay where they are; (c) it moves only into a dir with no
// pages yet.
#[test]
fn one_page_dir_per_model_and_the_newest_legacy_window_dir_moves_in_once() {
    assert_eq!(kv_page_dir("org/some model"), kv_pages_root().join("org_some_model"), "no window in the name");
    let root = tempfile::tempdir().expect("tempdir");
    let at = |secs: u64| std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_800_000_000 + secs);
    let mk = |name: &str, written: std::time::SystemTime| {
        let d = root.path().join(name);
        std::fs::create_dir_all(&d).expect("test: legacy dir");
        std::fs::write(d.join("a-page.bin"), name.as_bytes()).expect("test: page");
        std::fs::write(d.join("a-page.bin.ckpt"), b"ckpt").expect("test: sidecar");
        std::fs::write(d.join("saved-ledger.json"), b"{}").expect("test: ledger");
        std::fs::File::options().write(true).open(d.join("saved-ledger.json")).and_then(|f| f.set_modified(written)).expect("test: mtime");
        d
    };
    let older = mk("some-model--c24576", at(1));
    let newest = mk("some-model--c57344", at(2));
    let live = mk("some-model--c98304", at(3)); // the newest of all, but a live lane's
    let other_model = mk("other-model--c57344", at(4));
    let not_legacy = mk("some-model--cache", at(5));
    let current = root.path().join("some-model");
    std::fs::create_dir_all(&current).expect("test: current");

    let other_form = PathBuf::from("/some/other/root/./").join(live.file_name().expect("test: named"));
    assert_eq!(migrate_legacy_page_dir(&current, &[other_form.clone()]), 3, "page, sidecar and ledger moved");
    assert_eq!(std::fs::read(current.join("a-page.bin")).expect("test: moved"), b"some-model--c57344", "the newest unprotected dir");
    assert!(!newest.exists(), "the emptied legacy dir is removed");
    assert!(older.join("a-page.bin").is_file(), "an older legacy dir stays for the operator");
    assert!(live.join("a-page.bin").is_file(), "a live lane's dir is never touched");
    assert!(other_model.join("a-page.bin").is_file() && not_legacy.join("a-page.bin").is_file());
    assert_eq!(migrate_legacy_page_dir(&current, &[other_form]), 0, "only into a dir with no pages yet");
    assert!(older.join("a-page.bin").is_file());
}

// what this catches (Astra's block, re-block and third review on #4069), with
// deterministic barriers and the PRODUCTION ordering (reserve, THEN mkdir):
//  (a) B tries to reserve+create while A holds the guard mid-migration (after census,
//      before the move): B blocks until A is done, so B's insert can never land
//      between A's census and A's move — and because B creates its dir only AFTER
//      its reservation, A cannot move pages out of a dir B has made.
#[test]
fn a_reservation_cannot_slip_between_a_siblings_census_and_move() {
    use std::sync::{mpsc, Arc, Barrier};
    let root = tempfile::tempdir().expect("tempdir");
    // a legacy dir from an earlier run: unowned at A's census
    let stale = root.path().join("race-model--c32768");
    std::fs::create_dir_all(&stale).unwrap(); // JUSTIFIED unwrap: test scaffolding
    std::fs::write(stale.join("old-page.bin"), b"kv").unwrap(); // JUSTIFIED unwrap: test scaffolding
    let b_dir = stale.clone(); // B launches into that same dir
    let a_dir = root.path().join("race-model");
    std::fs::create_dir_all(&a_dir).unwrap(); // JUSTIFIED unwrap: test scaffolding

    let at_census = Arc::new(Barrier::new(2));
    let (b_reserved_tx, b_reserved_rx) = mpsc::channel::<bool>();
    let b_dir2 = b_dir.clone();
    let at_census_b = Arc::clone(&at_census);
    let b = std::thread::spawn(move || {
        at_census_b.wait(); // A is mid-migration now
        // THE PRODUCTION ORDERING: reserve first, create second. Must BLOCK on the guard.
        let (r, ready) = PageDirReservation::take_and_create(&b_dir2);
        let _ = b_reserved_tx.send(ready);
        r
    });
    {
        let guard = PAGE_DIR_GUARD.lock(); // A's census+move critical section
        at_census.wait();
        assert!(
            b_reserved_rx.recv_timeout(std::time::Duration::from_millis(300)).is_err(),
            "B's reservation must not land while A holds the guard"
        );
        // A's census saw no reservation for B's dir → A moves its page under the guard.
        let protect: Vec<PathBuf> = guard.reservations.iter().cloned().collect();
        assert_eq!(migrate_legacy_page_dir(&a_dir, &protect), 1);
        assert!(!b_dir.exists(), "the emptied legacy dir is gone");
        assert!(a_dir.join("old-page.bin").is_file(), "its page moved, never deleted");
    }
    // Guard released → B's reservation lands, THEN B creates its dir: it exists,
    // it is protected, and nothing A did could have touched it.
    let b_res = b.join().expect("B joins");
    let ready = b_reserved_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("B reserved after A released");
    assert!(ready && b_dir.is_dir(), "B's dir was created after its reservation");
    assert!(b_res.is_held());
    assert!(
        PAGE_DIR_GUARD
            .lock()
            .reservations
            .iter()
            .any(|d| d == &b_dir),
        "B is protected from here on"
    );
    // A later migration with the guard's contents as the protect set never moves B's pages,
    // even once A's dir is empty again.
    std::fs::write(b_dir.join("b-page.bin"), b"kv").unwrap(); // JUSTIFIED unwrap: test scaffolding
    std::fs::remove_file(a_dir.join("old-page.bin")).unwrap(); // JUSTIFIED unwrap: test scaffolding
    let protect: Vec<PathBuf> = PAGE_DIR_GUARD.lock().reservations.iter().cloned().collect();
    assert_eq!(migrate_legacy_page_dir(&a_dir, &protect), 0);
    assert!(b_dir.join("b-page.bin").is_file());
}

//  (b) the handoff (record written + reservation dropped) is ONE critical section:
//      concurrent censuses and ordered observations on both sides see the
//      reservation or record, never neither. The registry is a temp
//      dir (the test seam), never the canonical `~/.continuum/run/lanes/`.
#[test]
fn a_census_under_the_guard_never_finds_a_lane_in_neither_list_during_handoff() {
    use crate::inference::lane_registry::{records_checked_in, LaneRecord, LaneRole};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    let root = tempfile::tempdir().expect("tempdir");
    let registry = tempfile::tempdir().expect("tempdir");
    let b_dir = root.path().join("handoff-model--c32768");
    let (mut res, _ready) = PageDirReservation::take_and_create(&b_dir);
    let rec = LaneRecord {
        pid: 4_000_001, // never a live pid; the census here reads lists, not liveness
        port: 8123,
        role: LaneRole::Ephemeral,
        model: "handoff-model".into(),
        context_window: 32768,
        lanes: 1,
        page_dir: Some(b_dir.clone()),
        engine_bin: None,
        weights: None,
        started_s: 0,
        placement: None,
    };
    let (before_tx, before_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let reg = registry.path().to_path_buf();
    let dir = b_dir.clone();
    // Contend during handoff, and observe both sides explicitly. A fixed
    // iteration count could exhaust before the handoff thread ran.
    let census = std::thread::spawn(move || {
        let mut seen_before = 0usize;
        let mut seen_after = 0usize;
        // Initial BEFORE census, concurrent censuses, then explicit AFTER.
        let observe = |seen_before: &mut usize, seen_after: &mut usize| {
            let g = PAGE_DIR_GUARD.lock();
            let in_res = g.reservations.iter().any(|d| d == &dir);
            let in_rec = records_checked_in(&reg)
                .expect("temp registry readable")
                .iter()
                .any(|r| r.page_dir.as_deref() == Some(dir.as_path()));
            drop(g);
            assert!(in_res || in_rec, "a census found the lane in NEITHER list");
            assert!(!(in_res && in_rec), "a census found the lane in BOTH lists");
            if in_res { *seen_before += 1 } else { *seen_after += 1 }
        };
        observe(&mut seen_before, &mut seen_after);
        before_tx.send(()).expect("before census notification");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match done_rx.try_recv() {
                Ok(()) => break,
                Err(mpsc::TryRecvError::Empty) => {
                    assert!(Instant::now() < deadline, "handoff completion deadline");
                    observe(&mut seen_before, &mut seen_after);
                    std::thread::yield_now();
                }
                Err(mpsc::TryRecvError::Disconnected) => panic!("handoff owner disconnected"),
            }
        }
        observe(&mut seen_before, &mut seen_after);
        (seen_before, seen_after)
    });
    before_rx.recv_timeout(Duration::from_secs(5)).expect("initial census completed");
    res.handoff_in(&rec, registry.path()).expect("handoff writes the temp registry");
    assert!(!res.is_held(), "a successful handoff releases the reservation");
    done_tx.send(()).expect("handoff complete");
    let (before, after) = census.join().expect("census joins");
    assert!(before > 0 && after > 0, "censuses ran on both sides of the handoff: {before}/{after}");
    // And a FAILED handoff keeps the reservation: an unwritable registry dir.
    let (mut res2, _) = PageDirReservation::take_and_create(&root.path().join("handoff-model--c49152"));
    let not_a_dir = registry.path().join("file-not-dir");
    std::fs::write(&not_a_dir, b"x").unwrap(); // JUSTIFIED unwrap: test scaffolding
    assert!(res2.handoff_in(&rec, &not_a_dir).is_err());
    assert!(res2.is_held(), "a failed record write keeps the dir protected");
}

// what this catches (Astra's third review): the census fails CLOSED. With a malformed
// `.lane` file in the registry the guarded migration moves NOTHING, even though a legacy
// dir is right there.
#[test]
fn an_unreadable_registry_skips_the_migration_entirely() {
    let root = tempfile::tempdir().expect("tempdir");
    let registry = tempfile::tempdir().expect("tempdir");
    let stale = root.path().join("closed-model--c32768");
    std::fs::create_dir_all(&stale).unwrap(); // JUSTIFIED unwrap: test scaffolding
    std::fs::write(stale.join("a-page.bin"), b"kv").unwrap(); // JUSTIFIED unwrap: test scaffolding
    let current = root.path().join("closed-model");
    std::fs::create_dir_all(&current).unwrap(); // JUSTIFIED unwrap: test scaffolding
    std::fs::write(registry.path().join("garbage.lane"), "{not json").unwrap(); // JUSTIFIED unwrap: test scaffolding
    assert_eq!(migrate_legacy_page_dir_guarded_in(&current, registry.path()), None);
    assert!(stale.join("a-page.bin").is_file(), "nothing moved on an unreadable inventory");
    std::fs::remove_file(registry.path().join("garbage.lane")).unwrap(); // JUSTIFIED unwrap: test scaffolding
    assert_eq!(migrate_legacy_page_dir_guarded_in(&current, registry.path()), Some(1));
    assert!(current.join("a-page.bin").is_file() && !stale.exists());
}

// what this catches: the live-lane inventory — a live record contributes its dir, a
// live record without one (written before the field existed) makes the inventory
// INCOMPLETE so the migration stays its hand, and a dead record is ignored either way.
#[test]
fn the_live_lane_inventory_is_complete_only_when_every_live_record_names_its_dir() {
    use crate::inference::lane_registry::{LaneRecord, LaneRole};
    let rec = |pid: u32, page_dir: Option<&str>| LaneRecord {
        pid,
        port: 8000 + pid as u16,
        role: LaneRole::Live,
        model: "m".into(),
        context_window: 4096,
        lanes: 1,
        page_dir: page_dir.map(PathBuf::from),
        engine_bin: None,
        weights: None,
        started_s: 0,
        placement: None,
    };
    let alive = |pid: u32| pid != 3;
    let (dirs, complete) = page_dirs_of(vec![rec(1, Some("/p/a")), rec(2, Some("/p/b")), rec(3, None)], alive);
    assert_eq!(dirs, vec![PathBuf::from("/p/a"), PathBuf::from("/p/b")]);
    assert!(complete, "a DEAD record without a dir does not spoil the inventory");
    let (dirs, complete) = page_dirs_of(vec![rec(1, Some("/p/a")), rec(2, None)], alive);
    assert_eq!(dirs, vec![PathBuf::from("/p/a")]);
    assert!(!complete, "a LIVE record without a dir → incomplete → no migration");
}

// what this catches (card 689084e7): one unreadable launch revision (a deleted adapter, a
// PATH-only engine name) silently made every saved KV page unrestorable across a launch.
// The evidence now names the file that failed and why; a launch whose files all read keeps
// one entry per file, in order.
#[test]
fn revision_evidence_names_the_unreadable_file() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let model = dir.path().join("model.gguf");
    std::fs::write(&model, b"gguf")?;
    let missing = dir.path().join("llama-server");
    let read = revision_evidence(std::iter::once(model.clone()))
        .map_err(|(path, error)| format!("{}: {error}", path.display()))?;
    assert_eq!((read.len(), read[0].0.clone(), read[0].1), (1, model.clone(), 4));
    match revision_evidence([model, missing.clone()].into_iter()) {
        Err((path, error)) => {
            assert_eq!(path, missing, "the unreadable file is named, not the first");
            assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        }
        Ok(evidence) => panic!("a missing engine binary produced evidence: {evidence:?}"),
    }
    Ok(())
}
