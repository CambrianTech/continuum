#![cfg(test)]
use super::*;

// What this catches: cold payload placement must retain the SAME slot,
// verified, previous and rollback protocol without writing hot-home/bin.
#[test]
fn relocated_payload_keeps_engine_lifecycle() {
    let home = tempfile::tempdir().unwrap();
    let cold = tempfile::tempdir().unwrap();
    std::fs::write(
        home.path().join("payload-root"),
        cold.path().to_str().unwrap(),
    )
    .unwrap();
    let root = root(home.path()).unwrap();
    engine(&root, "engine-a", "first:cuda");
    promote(&root, "engine-a", "first:cuda").unwrap();
    mark_verified(&root, "engine-a").unwrap();
    engine(&root, "engine-b", "second:cuda");
    promote(&root, "engine-b", "second:cuda").unwrap();
    assert_eq!(current_slot(&root), Some("engine-b"));
    assert_eq!(previous_slot(&root), Some("engine-a"));
    assert!(!is_verified(&root, "engine-b"));
    assert!(is_verified(&root, "engine-a"));
    assert_eq!(rollback(&root, "engine-b").unwrap(), "engine-a");
    assert_eq!(current_slot(&root), Some("engine-a"));
    assert!(!home.path().join("bin").exists());
}

fn engine(root: &Path, slot: &str, stamp: &str) {
    std::fs::create_dir_all(root.join(slot)).unwrap();
    std::fs::write(slot_bin(root, slot), b"bin").unwrap();
    std::fs::write(root.join(slot).join(STAMP_FILE), format!("{stamp}\n")).unwrap();
}

// what this catches (card 2c5d0ec0): an engine build landing in a slot a lane still runs
// from (on Windows a locked exe fails the copy; on Unix a live lane's binary is replaced
// under it), or over the rollback while another slot is free. With every slot in use the
// deploy is refused, never guessed.
#[test]
fn the_idle_slot_is_one_no_lane_runs_and_current_does_not_name_sparing_the_rollback() {
    let root = PathBuf::from("/h/.continuum/bin");
    let lane = |s: &str| Some(slot_bin(&root, s));
    let all = |_: &str| true;
    assert_eq!(idle_slot(&root, None, None, &[], all), Ok("engine-a"), "a fresh node: the first install is the same path");
    assert_eq!(idle_slot(&root, Some("engine-a"), None, &[lane("engine-a")], all), Ok("engine-b"));
    assert_eq!(idle_slot(&root, Some("engine-b"), Some("engine-a"), &[lane("engine-b")], all), Ok("engine-c"), "the rollback is spared");
    assert_eq!(idle_slot(&root, Some("engine-b"), Some("engine-c"), &[lane("engine-a")], all), Ok("engine-c"), "a lane on a holds it; the rollback is the only free slot");
    assert_eq!(idle_slot(&root, Some("engine-a"), None, &[lane("engine-b"), lane("engine-c")], all), Err(NotIdle::AllInUse));
    assert_eq!(idle_slot(&root, None, None, &[Some(root.join(exe_name()))], all), Ok("engine-a"), "a lane on the pre-slot install holds no slot");
    assert_eq!(
        idle_slot(&root, None, None, &[Some(root.join("engine-ab").join(exe_name()))], all),
        Ok("engine-a"),
        "a sibling dir sharing a prefix is not the slot"
    );
    // A lane recorded before lanes named their engine may run any populated slot: only an
    // empty one is idle (the 5090 today: a and b populated, a lane up, c empty).
    let a_and_b = |s: &str| s != "engine-c";
    assert_eq!(idle_slot(&root, None, None, &[None], a_and_b), Ok("engine-c"));
    assert_eq!(idle_slot(&root, None, None, &[None], all), Err(NotIdle::AllInUse), "fail closed, never over a maybe-running engine");
}

// what this catches: a failed or partial build promoted to current, and a rollback that
// lands on nothing. Promotion needs the binary AND the exact stamp built; the replaced slot
// becomes previous; rollback swaps back only onto an intact engine.
#[test]
fn only_a_verified_slot_is_promoted_and_a_rollback_swaps_back_onto_an_intact_engine() {
    let dir = tempfile::tempdir().expect("test: dir");
    let root = dir.path();
    assert_eq!(current_slot(root), None, "no current file: no slot");
    std::fs::create_dir_all(root.join("engine-b")).unwrap();
    assert!(promote(root, "engine-b", "abc123:cuda").is_err(), "no binary");
    std::fs::write(slot_bin(root, "engine-b"), b"bin").unwrap();
    assert!(promote(root, "engine-b", "abc123:cuda").is_err(), "no stamp: the build did not verify");
    std::fs::write(root.join("engine-b").join(STAMP_FILE), "old000:cuda\n").unwrap();
    assert!(promote(root, "engine-b", "abc123:cuda").is_err(), "stamp is not the pin built");
    engine(root, "engine-a", "aaa000:cuda");
    let fresh = HandoffSelection::capture(root, &slot_bin(root, "engine-a")).expect("capture absent selection");
    assert!(fresh.confirm_preserved().is_err(), "no fallback cannot be a successful deferred handoff");
    fresh.restore().expect("preparation refused before promotion");
    promote(root, "engine-a", "aaa000:cuda").expect("first install promotes");
    fresh.restore().expect("failed first handoff restores absence");
    assert_eq!(current_slot(root), None);
    promote(root, "engine-a", "aaa000:cuda").expect("retry first install");
    assert!(rollback(root, "engine-a").is_err(), "nothing recorded to roll back to");
    engine(root, "engine-b", "abc123:cuda");
    // A core rollback must restore its actual prior engine before the old
    // reader encounters the newly published stamp-bearing receipt.
    let handoff = HandoffSelection::capture(root, &slot_bin(root, "engine-b")).expect("capture actual prior engine");
    assert!(handoff.confirm_preserved().is_err(), "unproven fallback cannot authorize core adoption");
    mark_verified(root, "engine-a").unwrap();
    handoff.confirm_preserved().expect("deferred published candidate preserves verified prior selection");
    promote(root, "engine-b", "abc123:cuda").expect("verified slot promotes");
    assert!(handoff.confirm_preserved().is_err(), "changed selection cannot claim preservation");
    handoff.confirm().expect("promotion receipt");
    handoff.restore().expect("core failure restores actual prior engine");
    handoff.restore().expect("repeated recovery preserves restored selection");
    assert_eq!(resolve(root, None, current_slot(root)), Resolved::Slot("engine-a"));
    promote(root, "engine-b", "abc123:cuda").expect("retry promotion");
    engine(root, "engine-c", "ccc000:cuda");
    promote(root, "engine-c", "ccc000:cuda").expect("newer independent selection");
    assert!(handoff.restore().is_err(), "never undo a newer selection");
    assert_eq!(current_slot(root), Some("engine-c"));
    rollback(root, "engine-c").expect("restore fixture selection");
    assert!(handoff.restore().is_err(), "matching current is insufficient when prior selection changed");
    promote(root, "engine-a", "aaa000:cuda").expect("reset actual prior");
    promote(root, "engine-b", "abc123:cuda").expect("reset promotion");
    assert_eq!((current_slot(root), previous_slot(root)), (Some("engine-b"), Some("engine-a")));
    // Reinstalling the engine already current with its own stamp (the 5090, Codex): a
    // no-op that keeps its verification and the rollback; a different stamp still refuses.
    mark_verified(root, "engine-b").unwrap();
    assert!(is_current_with(root, "engine-b", "abc123:cuda"));
    assert_eq!(promote(root, "engine-b", "abc123:cuda"), Ok(Promotion::AlreadyCurrent));
    assert!(is_verified(root, "engine-b"), "a reinstall keeps the proven engine verified");
    assert_eq!(previous_slot(root), Some("engine-a"), "and keeps the rollback");
    assert!(!is_current_with(root, "engine-b", "zzz999:cuda"));
    assert!(promote(root, "engine-b", "zzz999:cuda").is_err(), "a different stamp is still refused");
    assert!(rollback(root, "engine-a").is_err(), "a stale failure report for a slot no longer current");
    assert_eq!(current_slot(root), Some("engine-b"), "the newer promotion stands");
    assert_eq!(rollback(root, "engine-b"), Ok("engine-a"));
    assert_eq!((current_slot(root), previous_slot(root)), (Some("engine-a"), Some("engine-b")), "the failed slot is kept as previous");
    std::fs::remove_file(slot_bin(root, "engine-b")).unwrap();
    assert!(rollback(root, "engine-a").is_err(), "previous no longer intact");
    assert_eq!(current_slot(root), Some("engine-a"), "a refused rollback moves nothing");
    assert!(promote(root, "engine-z", "abc123:cuda").is_err(), "not a slot");
    std::fs::write(root.join(CURRENT_FILE), "../elsewhere\n").unwrap();
    assert_eq!(current_slot(root), None, "current naming anything but a slot is no slot");
    assert!(HandoffSelection::capture(root, &slot_bin(root, "engine-a")).is_err(), "corrupt prior selection is not an absent selection");
}

// what this catches: the engine resolved from the wrong place. Every override is the
// operator's, even inside a slot (provenance, not path). The Windows service-host's old
// injection becomes `register`, which moves current to each newly registered release
// (on Windows the registration is the promotion) and keeps the replaced one as previous.
#[test]
fn an_override_is_always_the_operators_and_a_service_registration_moves_current() {
    let dir = tempfile::tempdir().expect("test: dir");
    let root = dir.path();
    assert_eq!(resolve(root, None, None), Resolved::Path);
    std::fs::write(root.join(exe_name()), b"x").unwrap();
    assert_eq!(resolve(root, None, None), Resolved::Legacy(root.join(exe_name())));
    engine(root, "engine-a", "aaa000:cuda");
    let a = slot_bin(root, "engine-a").to_string_lossy().into_owned();
    assert_eq!(resolve(root, Some(&a), Some("engine-a")), Resolved::Operator(a.clone()), "a pin inside a slot is still a pin");
    assert!(register(root, "engine-c").is_err(), "no stamped engine: not registered");
    assert_eq!(register(root, "engine-a"), Ok(true));
    assert_eq!(register(root, "engine-a"), Ok(false), "every service start re-registers: idempotent");
    assert_eq!(resolve(root, None, current_slot(root)), Resolved::Slot("engine-a"));
    engine(root, "engine-b", "bbb000:cuda");
    assert_eq!(register(root, "engine-b"), Ok(true), "the next Windows release moves current");
    assert_eq!((current_slot(root), previous_slot(root)), (Some("engine-b"), Some("engine-a")));
    assert_eq!(resolve(root, None, Some("engine-c")), Resolved::Slot("engine-c"), "an empty current slot fails loud at spawn, never the legacy engine");
    assert_eq!(slot_of(root, Path::new(&a)), Some("engine-a"));
    // Verbatim prefixes are Windows path syntax, not Unix filename aliases.
    #[cfg(windows)]
    {
        assert_eq!(slot_of(root, Path::new(&format!(r"\\?\{a}"))), Some("engine-a"), "a verbatim-prefixed path is the same slot");
        let verbatim_root = PathBuf::from(format!(r"\\?\{}", root.display()));
        assert_eq!(
            slot_of(&verbatim_root, Path::new(&format!(r"\\?\{a}"))),
            Some("engine-a"),
            "a verbatim descriptor gives a verbatim root: the service-host's own derivation"
        );
    }
}

// what this catches (Fable on #4491): a promoted engine that verifies and then fails at
// launch leaves a first-and-only machine dark until a human runs rollback. A failed launch
// from the current slot restores the previous one, once: the restored engine failing too
// is not rolled "back" onto the failed one (a swap loop between two bad engines).
#[test]
fn a_failed_launch_restores_the_previous_engine_once_and_never_swaps_back() {
    let dir = tempfile::tempdir().expect("test: dir");
    let root = dir.path();
    assert_eq!(rollback_after_failed_launch(root), Ok(None), "no slot recorded: nothing to roll back");
    engine(root, "engine-a", "good00:cuda");
    promote(root, "engine-a", "good00:cuda").unwrap();
    engine(root, "engine-b", "bad000:cuda");
    promote(root, "engine-b", "bad000:cuda").unwrap();
    assert_eq!(rollback_after_failed_launch(root), Ok(Some(("engine-b", "engine-a"))));
    assert_eq!(current_slot(root), Some("engine-a"));
    assert_eq!(rollback_after_failed_launch(root), Ok(None), "the restored engine failing is not rolled back onto the failed one");
    assert_eq!(current_slot(root), Some("engine-a"));
    // Disk evidence, independent of the process-only RESTORED optimization.
    assert!(attempts::failed_selection(root, "engine-b").unwrap());
    mark_verified(root, "engine-a").unwrap();
    assert_eq!(promote(root, "engine-b", "bad000:cuda"), Ok(Promotion::Deferred("engine-a")));
    // Copying the same legacy candidate to another slot does not reset failure.
    engine(root, "engine-c", "bad000:cuda");
    assert_eq!(promote(root, "engine-c", "bad000:cuda"), Ok(Promotion::Deferred("engine-a")));
    assert!(promote_with_retry(root, "engine-c", "bad000:cuda", Some("legacy:other")).is_err());
    assert_eq!(promote_with_retry(root, "engine-c", "bad000:cuda", Some("legacy:bad000:cuda")), Ok(Promotion::Switched));
    mark_verified(root, "engine-c").unwrap();
    assert!(!attempts::failed_selection(root, "engine-b").unwrap());
    // Corrupt bookkeeping refuses new admission, but does not veto an actual
    // ready engine or an administrative restoration of intact payloads.
    std::fs::write(root.join("engine-b/.engine-attempt.json"), b"broken").unwrap();
    assert!(promote(root, "engine-b", "bad000:cuda").is_err());
    clear_verified(root, "engine-c").unwrap();
    mark_verified(root, "engine-c").unwrap();
    assert!(is_verified(root, "engine-c"));
    assert_eq!(rollback(root, "engine-c"), Ok("engine-a"));
    assert_eq!(current_slot(root), Some("engine-a"));
    std::fs::remove_file(root.join("engine-b/.engine-attempt.json")).unwrap();
    engine(root, "engine-c", "next00:cuda");
    assert_eq!(promote(root, "engine-c", "next00:cuda"), Ok(Promotion::Switched));
    // Make the receipt staging leaf unwritable without changing payloads.
    std::fs::create_dir(root.join("engine-c/.engine-attempt.tmp")).unwrap();
    assert_eq!(rollback_after_failed_launch(root), Ok(Some(("engine-c", "engine-a"))));
    assert_eq!(current_slot(root), Some("engine-a"), "receipt IO failure must not strand serving");
}

// what this catches (Fable on #4491): a GOOD engine demoted after a NotReady that was memory,
// a model load or a wedge, silently running the node on the older engine. An engine that
// came up ready once is proven; its later failures are ordinary launch failures. A fresh
// promotion clears the proof, so the next build into that slot starts unproven.
#[test]
fn a_proven_engine_is_never_rolled_back_and_a_promotion_clears_the_proof() {
    let dir = tempfile::tempdir().expect("test: dir");
    let root = dir.path();
    engine(root, "engine-a", "good00:cuda");
    promote(root, "engine-a", "good00:cuda").unwrap();
    engine(root, "engine-b", "new000:cuda");
    promote(root, "engine-b", "new000:cuda").unwrap();
    mark_verified(root, "engine-b").unwrap();
    assert_eq!(rollback_after_failed_launch(root), Ok(None), "launched ready once, then NotReady: not the engine's fault");
    assert_eq!(current_slot(root), Some("engine-b"));
    engine(root, "engine-c", "nxt000:cuda");
    mark_verified(root, "engine-c").unwrap(); // a proof left behind by the slot's previous engine
    promote(root, "engine-c", "nxt000:cuda").unwrap();
    assert!(!is_verified(root, "engine-c"), "a freshly promoted engine has proven nothing");
    assert!(is_verified(root, "engine-b"), "the replaced engine keeps its proof");
}

// what this catches (card d5584dfc, option (b)): a promotion undone by the next restart. The
// service host writes `current` from its registered release only to BOOTSTRAP (none recorded,
// or it names a slot with no stamped engine); a stamped `current` stands, so an unattended
// deploy's promote and an automatic rollback survive the restart. A descriptor outside the
// slots is refused whatever `current` says.
#[test]
fn the_service_host_bootstraps_current_and_never_overwrites_a_promotion() {
    let dir = tempfile::tempdir().expect("test: dir");
    let root = dir.path();
    engine(root, "engine-b", "bbb000:cuda");
    let b = slot_bin(root, "engine-b");
    assert_eq!(bootstrap_service_engine(root, &b), Ok(true), "nothing recorded: bootstrapped from the release");
    assert_eq!(bootstrap_service_engine(root, &b), Ok(false), "each service start: idempotent");
    engine(root, "engine-c", "ccc000:cuda");
    promote(root, "engine-c", "ccc000:cuda").unwrap();
    assert_eq!(bootstrap_service_engine(root, &b), Ok(false), "the release still names b, the deploy promoted c");
    assert_eq!(current_slot(root), Some("engine-c"), "a promotion survives the restart");
    std::fs::remove_file(&b).unwrap();
    assert_eq!(bootstrap_service_engine(root, &b), Ok(false), "the release's own binary gone: the standing engine still starts");
    std::fs::write(&b, b"bin").unwrap();
    std::fs::remove_file(root.join("engine-c").join(STAMP_FILE)).unwrap();
    assert_eq!(bootstrap_service_engine(root, &b), Ok(true), "current names an unstamped slot: bootstrapped");
    assert_eq!(current_slot(root), Some("engine-b"));
    let foreign = root.join("elsewhere").join(exe_name());
    std::fs::create_dir_all(foreign.parent().unwrap()).unwrap();
    std::fs::write(&foreign, b"x").unwrap();
    assert_eq!(bootstrap_service_engine(root, &foreign), Ok(false), "a stale or foreign release never stops a standing engine");
    let fresh = tempfile::tempdir().expect("test: dir");
    let stray = fresh.path().join("elsewhere").join(exe_name());
    std::fs::create_dir_all(stray.parent().unwrap()).unwrap();
    std::fs::write(&stray, b"x").unwrap();
    assert!(bootstrap_service_engine(fresh.path(), &stray).is_err(), "nothing standing and outside the slots: refused");
    std::fs::create_dir_all(fresh.path().join("engine-a")).unwrap();
    std::fs::write(slot_bin(fresh.path(), "engine-a"), b"x").unwrap();
    assert!(bootstrap_service_engine(fresh.path(), &slot_bin(fresh.path(), "engine-a")).is_err(), "nothing recorded and no stamp: refused");
    assert_eq!(current_slot(fresh.path()), None, "a refusal moves nothing");
}

mod reservations {
    // Installer receipts can retain an engine independently of runtime pointers.
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

    // what this catches (Codex on #5535): a crashed installer's claim pinning a slot forever,
    // or a claim whose owner merely cannot be read being reaped (a build then overwrote a slot
    // a launch was about to run); `release-slot` removing a claim it does not own; two launches
    // of one process (live and ephemeral lanes on the current slot) refusing each other, or the
    // first to finish releasing the slot under the second.
    #[test]
    fn one_claim_per_slot_reaped_only_when_its_owner_is_verified_gone() {
        use crate::inference::engine_residency::{probe_process, ProcessProbe};
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let claims = root.join("reservations");
        std::fs::create_dir_all(&claims).unwrap();
        let no_lanes = || Ok(Vec::new());
        let me = std::process::id();
        let dead_builder = |pid: u32| if pid == 4_000_000 { ProcessProbe::Absent } else { probe_process(pid) };

        // A crashed builder (verified gone) held engine-a: reaped, and engine-a is claimed.
        std::fs::write(claims.join("engine-a"), "build 4000000 1700000000\n").unwrap();
        assert!(matches!(claim_idle_slot(root, &["engine-b", "engine-c"], me, no_lanes, &dead_builder), Ok("engine-a")));
        // Another process (pid 1: it never holds this claim) releases nothing.
        let other = release_build_claim(root, "engine-a", 1);
        assert!(!matches!(&other, Ok(said) if said == "engine-a released"), "{other:?}");
        assert!(claims.join("engine-a").exists(), "another owner's release removes nothing");
        assert_eq!(release_build_claim(root, "engine-a", me).unwrap(), "engine-a released");

        // An owner that cannot be read, or a claim that cannot be parsed, holds its slot.
        let unreadable = |pid: u32| if pid == 4_000_001 { ProcessProbe::Unreadable } else { probe_process(pid) };
        std::fs::write(claims.join("engine-a"), "build 4000001 1700000000\n").unwrap();
        std::fs::write(claims.join("engine-b"), "").unwrap();
        assert!(matches!(claim_idle_slot(root, &["engine-c"], me, no_lanes, &unreadable), Err(VerbError::NoIdleSlot)));
        std::fs::remove_file(claims.join("engine-a")).unwrap();
        std::fs::remove_file(claims.join("engine-b")).unwrap();

        // Two launches of this process share engine-a's claim; it outlives the first.
        let first = SlotClaim::acquire_with(root, "engine-a", &probe_process).unwrap();
        let second = SlotClaim::acquire_with(root, "engine-a", &probe_process).unwrap();
        drop(first);
        assert!(matches!(claim_idle_slot(root, &["engine-b", "engine-c"], me, no_lanes, &probe_process), Err(VerbError::NoIdleSlot)));
        drop(second);
        assert!(!claims.join("engine-a").exists(), "the last launch releases it");
    }

    // what this catches (Codex on #5535): a crash between creating a claim and writing its
    // owner left an EMPTY claim, holding its slot until an operator removed it. A claim is
    // published whole or not at all: a builder that staged its claim and died before linking
    // leaves no claim (the slot stays claimable), and the scratch files a dead writer left are
    // swept, while a live writer's are not.
    #[test]
    fn a_claim_is_published_whole_or_not_at_all_and_dead_strays_are_swept() {
        use crate::inference::engine_residency::{probe_process, ProcessProbe};
        use crate::inference::engine_slots::claim::{stage, ClaimKind, Owner};
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let claims = root.join("reservations");
        std::fs::create_dir_all(&claims).unwrap();
        let me = std::process::id();

        let staged = stage(&claims, &Owner::of(ClaimKind::Build, me).unwrap()).unwrap();
        assert!(staged.is_file(), "the claim was written");
        assert!(!claims.join("engine-a").exists(), "and, never linked, no claim exists");

        let stray = claims.join(".claim.4000003.1700000000.42");
        let tomb = claims.join(".claim.4000003.1700000000.43");
        std::fs::write(&stray, "build 4000003 1700000000\n").unwrap();
        std::fs::write(&tomb, "launch 4000003 1700000000\n").unwrap();
        let dead = |pid: u32| if pid == 4_000_003 { ProcessProbe::Absent } else { probe_process(pid) };
        assert!(matches!(claim_idle_slot(root, &["engine-b", "engine-c"], me, || Ok(Vec::new()), &dead), Ok("engine-a")), "the slot is claimable");
        let held = std::fs::read_to_string(claims.join("engine-a")).unwrap();
        assert!(held.starts_with(&format!("build {me} ")) && held.ends_with('\n'), "the published claim is whole: {held:?}");
        assert!(!stray.exists() && !tomb.exists(), "a dead writer's scratch files are swept");
        assert!(staged.exists(), "a live writer's scratch file is not");
        assert_eq!(release_build_claim(root, "engine-a", me).unwrap(), "engine-a released");
    }
}
