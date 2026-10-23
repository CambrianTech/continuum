//! The confirmed-save ledger: what survives a core or engine restart, and what is refused.
use super::*;

// What this catches (the M5, 2026-10-10 18:49Z: turn_cached 0 of 31,811 after a
// deploy, pages on disk): confirmed-save eligibility that dies with the process.
// A new pool under the SAME page format restores what the last one saved; a page whose
// file is gone and a key lost before its save completed restore nothing.
#[test]
fn confirmed_saves_survive_a_restart_under_the_same_page_format() {
    let dir = tempfile::tempdir().expect("test: page dir");
    let contract = KvPageContract {
        model_id: "fixture".into(),
        model: "fixture.gguf".into(),
        adapters: vec![],
        page_dir: Some(dir.path().to_path_buf()),
        context: 32768,
        slots: 2,
        cache_type: Some("q8_0".into()),
        flash_attn: Some(true),
        engine_build: Some("4a79fed98".into()),
        engine: "fixture-engine".into(),
        revisions: Some(vec![("fixture.gguf".into(), 73, std::time::SystemTime::UNIX_EPOCH)]),
    };
    let fp = contract.fingerprint().expect("test: an exact contract fingerprints");
    let ledger = || SavedLedger::in_dir(dir.path(), fp.clone());
    let (kept, gone, interrupted) = (key(1, 2), key(3, 4), key(5, 6));
    let first = KvSlotPool::new("test://ledger", 2);
    assert_eq!(first.attach_ledger(ledger(), true), 0, "nothing saved yet");
    for k in [kept, gone, interrupted] {
        first.note_saved(k);
        std::fs::write(dir.path().join(page_filename(&k)), b"kv").expect("test: page");
    }
    std::fs::remove_file(dir.path().join(page_filename(&gone))).expect("test: page gone");
    first.note_page_lost(&interrupted); // a save began over it and never confirmed

    let restarted = KvSlotPool::new("test://ledger", 2);
    assert_eq!(restarted.attach_ledger(ledger(), true), 1);
    assert!(restarted.saved.lock().contains(&kept), "a confirmed save survives the restart");
    assert!(!restarted.saved.lock().contains(&gone), "a page whose file is gone is never offered");
    assert!(!restarted.saved.lock().contains(&interrupted), "an unconfirmed save is never offered");
}

// What this catches (the M5, 2026-10-23 04:39Z: a relaunch from one 129,024-token slot
// to two of 130,048 read the ledger as `written under another contract`, and every
// resident's first act after the 05:09Z deploy prefilled 71k-110k tokens cold): the
// fingerprint is the PAGE FORMAT, so an engine restart at another window, slot count or
// engine file of the same build keeps every page, and any field that changes what the KV
// means (model file, adapter, draft, cache type, flash attention, engine build, or the
// engine file when the build is unknown) refuses them all.
#[test]
fn a_page_survives_an_engine_restart_unless_its_format_changed() {
    let dir = tempfile::tempdir().expect("test: page dir");
    let at = |s: u64| std::time::UNIX_EPOCH + std::time::Duration::from_secs(s);
    let contract = KvPageContract {
        model_id: "ggml-org/Qwen3.8-27B-GGUF".into(),
        model: "/models/qwen.gguf".into(),
        adapters: vec!["/genes/a.lora".into()],
        page_dir: Some(dir.path().to_path_buf()),
        context: 129_024,
        slots: 1,
        cache_type: Some("q8_0".into()),
        flash_attn: Some(true),
        engine_build: Some("4a79fed98".into()),
        engine: "/engines/slot-a/llama-server".into(),
        revisions: Some(vec![("/models/qwen.gguf".into(), 17_000_000_000, at(100)), ("/genes/a.lora".into(), 12, at(200))]),
    };
    let fp = contract.fingerprint().expect("test: fingerprint");
    let k = key(11, 12);
    let first = KvSlotPool::new("test://format", 2);
    first.attach_ledger(SavedLedger::in_dir(dir.path(), fp), true);
    first.note_saved(k);
    std::fs::write(dir.path().join(page_filename(&k)), b"kv").expect("test: page");
    let restored = |c: &KvPageContract| {
        let pool = KvSlotPool::new("test://format", 3);
        let fp = c.fingerprint().expect("test: fingerprint");
        pool.attach_ledger(SavedLedger::in_dir(dir.path(), fp), true);
        let kept = pool.saved.lock().contains(&k);
        kept
    };
    // Same format, another launch: the window, the slot count and the engine's file.
    let relaunch = KvPageContract {
        context: 130_048,
        slots: 2,
        engine: "/engines/slot-b/llama-server".into(),
        ..contract.clone()
    };
    assert!(restored(&relaunch), "a relaunch at another window and slot count keeps her page");
    // Every field that changes what the KV means refuses it. Each arm restores the
    // ledger first (a refusal rewrites it under the new format), then checks.
    let reset = || {
        let pool = KvSlotPool::new("test://format", 2);
        pool.attach_ledger(SavedLedger::in_dir(dir.path(), contract.fingerprint().expect("test: fp")), false);
        pool.note_saved(k);
    };
    let changed: Vec<(&str, KvPageContract)> = vec![
        ("model revision", KvPageContract { revisions: Some(vec![("/models/qwen.gguf".into(), 17_000_000_001, at(100)), ("/genes/a.lora".into(), 12, at(200))]), ..contract.clone() }),
        ("model path", KvPageContract { model: "/models/other.gguf".into(), ..contract.clone() }),
        ("adapter", KvPageContract { adapters: vec!["/genes/b.lora".into()], ..contract.clone() }),
        ("cache type", KvPageContract { cache_type: Some("f16".into()), ..contract.clone() }),
        ("flash attention", KvPageContract { flash_attn: Some(false), ..contract.clone() }),
        ("engine build", KvPageContract { engine_build: Some("965d38a90".into()), ..contract.clone() }),
        ("unknown build: the engine file stands in", KvPageContract {
            engine_build: None,
            revisions: Some(vec![("/models/qwen.gguf".into(), 17_000_000_000, at(100)), ("/genes/a.lora".into(), 12, at(200)), ("/engines/slot-a/llama-server".into(), 9, at(300))]),
            ..contract.clone()
        }),
    ];
    for (what, other) in changed {
        reset();
        assert!(!restored(&other), "a changed {what} refuses the page");
    }
    for unknown in [
        KvPageContract { cache_type: None, ..contract.clone() },
        KvPageContract { flash_attn: None, ..contract.clone() },
        KvPageContract { revisions: None, ..contract.clone() },
        KvPageContract { page_dir: None, ..contract.clone() },
    ] {
        assert!(unknown.fingerprint().is_none(), "what cannot be compared exactly never persists");
    }
}

// What this catches: the ledgers already on the M5 were written under the old contract
// fingerprint (exact window, slot count, engine file), which no launch can recompute.
// Migrating decides: a legacy ledger's pages written after every identity file (model,
// adapters, draft) last changed are adopted, an older one (computed by a model file since
// replaced) is dropped, and the ledger is rewritten in the new format at once.
#[test]
fn a_legacy_ledger_adopts_only_pages_newer_than_the_model() {
    let dir = tempfile::tempdir().expect("test: page dir");
    let (fresh, stale) = (key(21, 22), key(23, 24));
    for k in [fresh, stale] {
        std::fs::write(dir.path().join(page_filename(&k)), b"kv").expect("test: page");
    }
    let floor = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_800_000_000);
    let set_mtime = |k: &ActivityKey, t: std::time::SystemTime| {
        std::fs::File::options().write(true).open(dir.path().join(page_filename(k))).and_then(|f| f.set_modified(t)).expect("test: mtime");
    };
    set_mtime(&fresh, floor + std::time::Duration::from_secs(60));
    set_mtime(&stale, floor - std::time::Duration::from_secs(60));
    let legacy = serde_json::json!({
        "fingerprint": "9d987bf2befc05cd6dcce35fc4c3bfbc654374e996d7e4b90130d9c002683162",
        "keys": [[fresh.persona, fresh.room], [stale.persona, stale.room]],
        "incarnation": [38527, 1792732176u64],
    });
    std::fs::write(dir.path().join(SAVED_LEDGER_FILE), legacy.to_string()).expect("test: legacy ledger");

    let undated = KvSlotPool::new("test://legacy", 2);
    assert_eq!(undated.attach_ledger(SavedLedger::in_dir(dir.path(), "fp".into()), true), 0, "no identity files, no adoption");
    std::fs::write(dir.path().join(SAVED_LEDGER_FILE), legacy.to_string()).expect("test: legacy ledger again");

    let pool = KvSlotPool::new("test://legacy", 2);
    let ledger = SavedLedger::in_dir(dir.path(), "fp".into()).with_identity_floor(Some(floor));
    assert_eq!(pool.attach_ledger(ledger, true), 1);
    assert!(pool.saved.lock().contains(&fresh), "written after the model: adopted");
    assert!(!pool.saved.lock().contains(&stale), "written before the model changed: dropped");
    let file: SavedLedgerFile = serde_json::from_slice(&std::fs::read(dir.path().join(SAVED_LEDGER_FILE)).expect("test: read")).expect("test: parse");
    assert_eq!((file.format, file.fingerprint.as_str()), (LEDGER_FORMAT, "fp"), "rewritten in the new format");
    assert_eq!(file.keys, vec![(fresh.persona, fresh.room)]);
    assert!(dir.path().join(page_filename(&stale)).is_file(), "a dropped page's file is never deleted");
}

// What this catches: a core deploy that adopts the running engine started a pool with no
// idea which mind each slot held, so her next turn restored an OLDER page over the warm
// KV in her slot, or another mind overwrote it unsaved. The same engine process's ledger
// now names its residents; an adopter resumes them in place (and a returning mind takes
// back the slot that still holds her KV); a NEW process's pool never inherits them.
#[tokio::test]
async fn an_adopted_engine_resumes_its_residents_in_place() {
    let dir = tempfile::tempdir().expect("test: page dir");
    let (a, b) = (key(31, 32), key(33, 34));
    let old_core = KvSlotPool::new("test://resident", 3);
    old_core.attach_ledger(SavedLedger::in_dir(dir.path(), "fp".into()), true);
    old_core.stamp_incarnation((5151, 1_800_000_000));
    assert!(!old_core.plan_paging(1, a).already_resident);
    old_core.plan_paging(0, b);
    old_core.forget_resident(0, b); // b's turn failed: its slot proves nothing

    let new_core = KvSlotPool::new("test://resident", 3);
    assert_eq!(adopt_engine_ledger(&new_core, dir.path(), 5151, 1_800_000_000), Ok(0));
    let slot = new_core.lease(a).await.expect("test: lease");
    assert_eq!(slot, 1, "she takes back the slot that still holds her KV");
    let paging = new_core.plan_paging(slot, a);
    assert!(paging.already_resident && !paging.restore && paging.save_first.is_none(), "her KV is reused in place");
    assert_ne!(new_core.lease(b).await, Some(1));

    let spawned = KvSlotPool::new("test://resident", 3);
    spawned.attach_ledger(SavedLedger::in_dir(dir.path(), "fp".into()), true);
    assert!(spawned.holders.lock().is_empty(), "a new engine process holds nobody's KV");
}

// What this catches (IntelMac, 2026-10-10 20:29Z: the new core ADOPTED the running
// engine, and an adopted engine's pool has no launch contract, so #4921's ledger never
// attached and every saved page was ignored): an adopting core restores the ledger
// its engine left, and ONLY when that ledger names the same process. Another pid,
// another start time of the same pid (a recycled number), or a ledger the spawn
// never stamped adopts nothing.
#[test]
fn an_adopted_engine_restores_only_the_ledger_its_own_process_wrote() {
    let dir = tempfile::tempdir().expect("test: page dir");
    let contract = KvPageContract {
        model_id: "fixture".into(),
        model: "fixture.gguf".into(),
        adapters: vec![],
        page_dir: Some(dir.path().to_path_buf()),
        context: 32768,
        slots: 2,
        cache_type: Some("q8_0".into()),
        flash_attn: Some(true),
        engine_build: None,
        engine: "fixture-engine".into(),
        revisions: Some(vec![("fixture.gguf".into(), 73, std::time::SystemTime::UNIX_EPOCH)]),
    };
    let fp = contract.fingerprint().expect("test: fingerprint");
    let spawned = KvSlotPool::new("test://adopt", 2);
    spawned.attach_ledger(SavedLedger::in_dir(dir.path(), fp), true);
    let k = key(7, 8);
    spawned.note_saved(k);
    std::fs::write(dir.path().join(page_filename(&k)), b"kv").expect("test: page");

    let empty = tempfile::tempdir().expect("test: empty dir");
    let first_core = KvSlotPool::new("test://adopt", 2);
    assert_eq!(adopt_engine_ledger(&first_core, empty.path(), 4242, 1_700_000_000), Err(AdoptRefusal::NoLedger));
    // ...but the adopting core starts this process's ledger, so its saves survive the next deploy
    let saved_after = key(9, 10);
    first_core.note_saved(saved_after);
    std::fs::write(empty.path().join(page_filename(&saved_after)), b"kv").expect("test: page");
    let next_core = KvSlotPool::new("test://adopt", 2);
    assert_eq!(adopt_engine_ledger(&next_core, empty.path(), 4242, 1_700_000_000), Ok(1), "the next deploy restores what the adopter saved");
    assert_eq!(adopt_engine_ledger(&KvSlotPool::new("test://adopt", 2), empty.path(), 4243, 1_700_000_000), Err(AdoptRefusal::AnotherProcess), "never another process");
    std::fs::remove_file(empty.path().join(SAVED_LEDGER_FILE)).expect("test: reset for the cases below");
    assert_eq!(adopt_engine_ledger(&KvSlotPool::new("test://adopt", 2), dir.path(), 4242, 1_700_000_000), Err(AdoptRefusal::Unstamped), "an unstamped ledger proves no process");
    spawned.stamp_incarnation((4242, 1_700_000_000));

    let adopted = KvSlotPool::new("test://adopt", 2);
    assert_eq!(adopt_engine_ledger(&adopted, dir.path(), 4242, 1_700_000_000), Ok(1));
    assert!(adopted.saved.lock().contains(&k), "the same process's confirmed save is restorable after the core restart");
    assert_eq!(adopt_engine_ledger(&KvSlotPool::new("test://adopt", 2), dir.path(), 4243, 1_700_000_000), Err(AdoptRefusal::AnotherProcess), "another process");
    assert_eq!(adopt_engine_ledger(&KvSlotPool::new("test://adopt", 2), dir.path(), 4242, 1_700_000_999), Err(AdoptRefusal::AnotherProcess), "a recycled pid");
    assert_eq!(adopt_engine_ledger(&KvSlotPool::new("test://adopt", 2), dir.path(), 4242, 0), Err(AdoptRefusal::NoStartTime), "no start time, no proof");
    std::fs::write(empty.path().join(SAVED_LEDGER_FILE), b"not json").expect("test: garbage");
    assert_eq!(adopt_engine_ledger(&KvSlotPool::new("test://adopt", 2), empty.path(), 4242, 1_700_000_000), Err(AdoptRefusal::Unreadable));
}
