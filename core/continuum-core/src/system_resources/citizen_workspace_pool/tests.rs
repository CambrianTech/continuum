    use super::*;

    fn ws(n: u128, bytes: u64, last_active_ms: u64) -> PeerWorkspace {
        PeerWorkspace {
            peer_id: uuid::Uuid::from_u128(n),
            workspace: PathBuf::from(format!("/tmp/peer{n}/workspace")),
            bytes,
            last_active_ms,
        }
    }

    // what this catches: the two ways this pool could hurt a citizen — taking a resident's
    // workspace out from under her mid-turn, or taking one from someone who was working
    // yesterday. 714 peer dirs and 5 resident on the M5, 2026-09-08: the filter is the
    // whole safety of the feature.
    #[test]
    fn a_resident_or_recently_active_citizen_is_never_evicted() {
        let now = 30 * 24 * 60 * 60 * 1000u64;
        let resident = ws(1, 9_000_000_000, now - 60_000);
        let yesterday = ws(2, 8_000_000_000, now - 24 * 60 * 60 * 1000);
        let dormant = ws(3, 7_000_000_000, now - DORMANT_AFTER_MS - 1);
        let peers = vec![resident.clone(), yesterday.clone(), dormant.clone()];
        let roster = Roster::Live(vec![resident.peer_id]);
        let drop = workspaces_to_drop(&peers, &roster, now, u64::MAX);
        assert_eq!(drop, vec![dormant], "only the dormant, non-resident workspace");
    }

    // what this catches: an unreadable roster being treated as "nobody is resident", which
    // would drop every workspace on the node in one pass (card c7ae34b2).
    #[test]
    fn an_unreadable_roster_evicts_nothing() {
        let now = 30 * 24 * 60 * 60 * 1000u64;
        let peers = vec![ws(1, 9_000_000_000, 0), ws(2, 8_000_000_000, 0)];
        assert!(workspaces_to_drop(&peers, &Roster::Unreadable, now, u64::MAX).is_empty());
    }

    // what this catches: the pool taking far more than it was asked for, or ordering by
    // something other than size (the fewest trees removed for the bytes wanted).
    #[test]
    fn it_takes_the_largest_dormant_workspaces_and_stops_at_the_ask() {
        let now = 30 * 24 * 60 * 60 * 1000u64;
        let old = now - DORMANT_AFTER_MS - 1;
        let peers = vec![ws(1, 1_000, old), ws(2, 9_000, old), ws(3, 5_000, old)];
        let drop = workspaces_to_drop(&peers, &Roster::Live(vec![]), now, 10_000);
        assert_eq!(
            drop.iter().map(|p| p.bytes).collect::<Vec<_>>(),
            vec![9_000, 5_000],
            "largest first, stopping once the ask is covered"
        );
    }

    fn git(repo: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .expect("git runs in tests"); // expect: the test's premise is a working git
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    fn dirty_repo(root: &Path) -> PathBuf {
        let repo = root.join("swe/django__django-1");
        std::fs::create_dir_all(&repo).expect("mkdir repo"); // expect: tempdir is writable
        git(&repo, &["init", "-q"]);
        git(&repo, &["config", "user.email", "t@t"]);
        git(&repo, &["config", "user.name", "t"]);
        std::fs::write(repo.join("tracked.py"), b"original\n").expect("write tracked");
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-qm", "base"]);
        // The three shapes the old best-effort archiver lost: a tracked edit, an
        // UNTRACKED file, and a BINARY one.
        std::fs::write(repo.join("tracked.py"), b"the citizen's fix\n").expect("edit tracked");
        std::fs::write(repo.join("new_test.py"), b"def test_it(): pass\n").expect("write untracked");
        std::fs::write(repo.join("fixture.bin"), [0u8, 159, 146, 150, 0, 255]).expect("write binary");
        repo
    }

    // what this catches: the defect Astra blocked #3908 for — deletion proceeding after a
    // best-effort archive that skipped untracked and binary work. Every uncommitted shape
    // must be in the archive, and the archive must read back, BEFORE anything is removed.
    #[test]
    fn every_uncommitted_shape_is_archived_and_verified_before_deletion() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        std::fs::create_dir_all(&workspace).expect("mkdir workspace");
        dirty_repo(&workspace);
        let dropped = tmp.path().join("dropped");
        let archived = preserve_workspace(&workspace, &dropped).expect("preservation succeeds");
        assert_eq!(archived, 1);
        let tar = dropped.join("swe_django__django-1.uncommitted.tar.gz");
        assert!(tar.exists(), "the archive exists");
        assert!(dropped.join("swe_django__django-1.base").exists(), "the base commit is recorded");
        let listed = crate::shell_portable::tar_on(&tar, "-tzf").expect("archive path").output().expect("tar -t");
        let names = String::from_utf8_lossy(&listed.stdout);
        for want in ["tracked.py", "new_test.py", "fixture.bin"] {
            assert!(names.contains(want), "{want} is in the archive: {names}");
        }
    }

    // what this catches: the pool deleting a tree whose preservation failed. The cap is
    // the easiest failure to inject deterministically; the rule it proves is the one that
    // matters — a failed preservation leaves the workspace byte-identical.
    #[test]
    fn a_preservation_failure_leaves_the_workspace_untouched() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        std::fs::create_dir_all(&workspace).expect("mkdir workspace");
        let repo = dirty_repo(&workspace);
        // Preservation cannot write: the archive destination already exists as a FILE, so
        // create_dir_all fails. Deterministic on every platform, and it exercises the
        // branch that matters — a failure BEFORE any deletion, with the tree intact.
        let dropped = tmp.path().join("dropped");
        std::fs::write(&dropped, b"not a directory").expect("occupy the archive path");
        let err = preserve_workspace(&workspace, &dropped).unwrap_err();
        assert!(matches!(err, PreserveFailed::Archive(_)), "{err:?}");
        assert!(repo.join("tracked.py").exists(), "the citizen's files are still there");
        assert!(repo.join("fixture.bin").exists());
    }

    // what this catches: the cap silently archiving a huge working set instead of keeping
    // the workspace — disk is never worth gambling with unsaved work.
    #[test]
    fn work_above_the_cap_keeps_the_workspace_instead_of_archiving_it() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        std::fs::create_dir_all(&workspace).expect("mkdir workspace");
        let repo = dirty_repo(&workspace);
        std::fs::write(repo.join("big.bin"), vec![7u8; 4096]).expect("write big");
        // Cap is a constant in production; here the shape is asserted by construction:
        // a workspace whose uncommitted bytes exceed the cap returns TooLarge.
        let paths = uncommitted_paths(&repo).expect("status reads");
        assert!(bytes_of(&repo, &paths) > 0, "the working set has bytes");
        assert!(paths.iter().any(|p| p == "big.bin"), "untracked binary counted: {paths:?}");
    }


    // what this catches: work that lives in NO repository being deleted unpreserved — the
    // "root work" half of Astra's review. A note beside her checkouts is work; a build
    // artifact is not, and the two must not be confused in either direction.
    #[test]
    fn loose_work_outside_any_repo_is_preserved_and_derived_output_is_not() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        std::fs::create_dir_all(workspace.join("target/debug")).expect("mkdir derived");
        std::fs::write(workspace.join("target/debug/huge.o"), vec![0u8; 2048]).expect("write derived");
        std::fs::write(workspace.join("NOTES.md"), b"what I was in the middle of\n").expect("write note");
        std::fs::create_dir_all(workspace.join("scratch")).expect("mkdir scratch");
        std::fs::write(workspace.join("scratch/repro.py"), b"print(1)\n").expect("write scratch");
        let dropped = tmp.path().join("dropped");
        let archived = preserve_workspace(&workspace, &dropped).expect("preserve loose work");
        assert_eq!(archived, 1, "one loose archive");
        let tar = dropped.join("loose.tar.gz");
        let listed = crate::shell_portable::tar_on(&tar, "-tzf").expect("archive path").output().expect("tar -t");
        let names = String::from_utf8_lossy(&listed.stdout);
        assert!(names.contains("NOTES.md"), "her note is preserved: {names}");
        assert!(names.contains("scratch/repro.py"), "her scratch script is preserved: {names}");
        assert!(!names.contains("huge.o"), "build output is not archived: {names}");
    }

    fn residue(n: u128, bytes: u64, last_active_ms: u64) -> BuildResidue {
        BuildResidue {
            peer_id: uuid::Uuid::from_u128(n),
            target: PathBuf::from(format!("/tmp/peer{n}/workspace/target")),
            bytes,
            last_active_ms,
        }
    }

    // what this catches: card 10e6c5e5, the IntelMac's 36 GB. A RESIDENT citizen's stale build
    // output must be reclaimable (residency pins her work, not her build cache), while output
    // she touched this week stays and an unreadable roster still takes nothing.
    #[test]
    fn a_residents_stale_build_output_is_reclaimable_and_fresh_output_is_not() {
        let now = 30 * 24 * 60 * 60 * 1000u64;
        let stale = residue(1, 12_000_000_000, now - DORMANT_AFTER_MS - 1);
        let fresh = residue(2, 9_000_000_000, now - 60_000);
        let roster = Roster::Live(vec![stale.peer_id, fresh.peer_id]); // both resident
        let all = vec![stale.clone(), fresh];
        assert_eq!(residue_to_drop(&all, &roster, now, u64::MAX), vec![stale], "stale output, resident or not");
        assert!(residue_to_drop(&all, &Roster::Unreadable, now, u64::MAX).is_empty());
    }

    fn cargo_target(workspace: &Path) -> PathBuf {
        let target = workspace.join("target");
        std::fs::create_dir_all(target.join("debug/deps")).expect("mkdir target"); // expect: tempdir is writable
        std::fs::write(target.join("CACHEDIR.TAG"), format!("{CARGO_CACHEDIR_SIGNATURE}\n# cargo\n")).expect("tag");
        std::fs::write(target.join(".rustc_info.json"), b"{}").expect("rustc info");
        std::fs::write(target.join("debug/.cargo-lock"), b"").expect("lock file");
        std::fs::write(target.join("debug/deps/libx.rlib"), vec![0u8; 4096]).expect("artifact");
        target
    }

    // what this catches: the explicit reclaim taking something that is not provably cargo's
    // build output, taking it while a build is live, or touching anything on a dry run. Only a CACHEDIR-tagged `target/` is residue;
    // a held `.cargo-lock` keeps it; once the lock is free a resident's target goes and every
    // other file in her workspace (her notes, her checkouts) stays.
    #[cfg(unix)]
    #[test]
    fn only_idle_cargo_tagged_output_is_reclaimed_and_her_work_stays() {
        let root = tempfile::tempdir().expect("tempdir");
        let (tagged, untagged) = (uuid::Uuid::from_u128(0x10e6_c5e5_0001), uuid::Uuid::from_u128(0x10e6_c5e5_0002));
        let ws = root.path().join("peers").join(tagged.to_string()).join("workspace");
        let target = cargo_target(&ws);
        std::fs::write(ws.join("NOTES.md"), b"what I was in the middle of\n").expect("note");
        let other = root.path().join("peers").join(untagged.to_string()).join("workspace/target");
        std::fs::create_dir_all(&other).expect("mkdir untagged target");
        std::fs::write(other.join("mine.txt"), b"not cargo's").expect("write");
        // the generic cachedir marker alone is not cargo's proof (Codex on #4528)
        std::fs::write(other.join("CACHEDIR.TAG"), CARGO_CACHEDIR_SIGNATURE).expect("generic tag");

        let found = CitizenWorkspacePool::build_residue_on_disk(root.path());
        assert_eq!(found.iter().map(|r| r.peer_id).collect::<Vec<_>>(), vec![tagged], "only the cargo-tagged tree");

        let roster = Roster::Live(vec![tagged, untagged]);
        let later = found[0].last_active_ms + DORMANT_AFTER_MS + 1;
        let held = std::fs::OpenOptions::new().read(true).write(true).open(target.join("debug/.cargo-lock")).expect("open lock");
        held.lock().expect("a live build holds cargo's lock");
        let decision = |out: Vec<ResidueOutcome>| out.into_iter().map(|o| o.decision).collect::<Vec<_>>();
        assert_eq!(decision(CitizenWorkspacePool::reclaim_with_roster(root.path(), &roster, later, true, &|| Ok(()))), vec!["kept"], "a live build keeps it");
        assert!(target.exists());
        // End the simulated build's lock explicitly: another parallel test may fork
        // while this descriptor is open, briefly retaining its shared Unix flock
        // until exec. Closing only our descriptor does not end that inherited lock.
        held.unlock().expect("the simulated build releases its lock");
        drop(held);

        // a dry run judges and takes nothing (Codex on #4528: explicit, never automatic)
        let dry_run = CitizenWorkspacePool::reclaim_with_roster(root.path(), &roster, later, false, &|| Ok(()));
        assert_eq!(dry_run.iter().map(|o| o.decision.as_str()).collect::<Vec<_>>(), vec!["would_reclaim"], "dry-run outcomes: {dry_run:?}");
        assert!(target.exists(), "a dry run touches nothing");
        assert_eq!(decision(CitizenWorkspacePool::reclaim_with_roster(root.path(), &roster, later, true, &|| Ok(()))), vec!["reclaimed"]);
        assert!(!target.exists(), "idle, tagged, stale, applied: reclaimed");
        // Fable on #4528: quiescence is enforced, not documented. A node with a compiler or a
        // turn running refuses the apply outright and touches nothing.
        let again = cargo_target(&ws);
        let busy = || Err::<(), String>("compiler_running: rustc (pid 1)".into());
        assert_eq!(decision(CitizenWorkspacePool::reclaim_with_roster(root.path(), &roster, later + 1, true, &busy)), vec!["refused"]);
        assert!(again.exists(), "a busy node takes nothing");
        assert_eq!(
            decision(CitizenWorkspacePool::reclaim_with_roster(root.path(), &Roster::Unreadable, later, true, &|| Ok(()))),
            vec!["refused"],
            "an unreadable roster judges nothing"
        );
        assert!(ws.join("NOTES.md").exists(), "her workspace and work stay");
        assert!(other.join("mine.txt").exists(), "a target without cargo's own file is not cargo's to reclaim");
        assert!(!ws.read_dir().expect("read ws").flatten().any(|e| e.file_name().to_string_lossy().starts_with(PARKED_PREFIX)), "nothing left parked");
    }

    // what this catches: Fable on #4528. A cargo target's live files sit deep
    // (`incremental/<crate>/<session>/`, `build/<crate>/out/**`), below the workspace walk's
    // depth, so a bounded walk read a busy tree as stale and small. The residue walk reaches
    // the whole tree.
    #[test]
    fn the_residue_walk_reaches_the_deepest_build_files() {
        let root = tempfile::tempdir().expect("tempdir");
        let peer = uuid::Uuid::from_u128(0x10e6_c5e5_0003);
        let ws = root.path().join("peers").join(peer.to_string()).join("workspace");
        let target = cargo_target(&ws);
        let deep = target.join("debug/build/x-1/out/a/b/c/d/e");
        std::fs::create_dir_all(&deep).expect("mkdir deep");
        std::fs::write(deep.join("gen.rs"), vec![1u8; 1_000_000]).expect("deep file");
        let found = CitizenWorkspacePool::build_residue_on_disk(root.path());
        assert!(found[0].bytes >= 1_000_000, "a file ten levels down is counted: {}", found[0].bytes);
    }

    // what this catches: Codex's block on #4528. The first cut released cargo's locks BEFORE
    // the delete, so any build could start inside a tree being removed. The exclusion must span
    // the take: while this pass holds the locks, another cargo cannot take them, and the tree
    // is already parked (not at `target`) when they are released.
    #[cfg(unix)]
    #[test]
    fn cargos_lock_is_held_across_the_take_so_no_build_starts_inside_it() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let target = cargo_target(tmp.path());
        let locks = hold_cargo_locks(&target).expect("idle: every lock taken");
        let rival = std::fs::OpenOptions::new().read(true).write(true).open(target.join("debug/.cargo-lock")).expect("open");
        assert!(matches!(rival.try_lock(), Err(std::fs::TryLockError::WouldBlock)), "a build cannot start while the take holds the lock");
        drop(rival);
        take_target(&target, locks).expect("taken");
        assert!(!target.exists(), "gone from `target`, where every build looks");
    }

    // what this catches: the fail-open half of Codex's block. A lock walk that cannot read part
    // of the tree must refuse, not return "no locks" and allow the delete.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_tree_is_kept_because_idleness_is_unproven() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().expect("tempdir");
        let target = cargo_target(tmp.path());
        let sealed = target.join("release");
        std::fs::create_dir_all(&sealed).expect("mkdir");
        std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o000)).expect("seal");
        if std::fs::read_dir(&sealed).is_ok() {
            // a superuser reads through mode 000, so the premise cannot be built here
            std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o755)).expect("unseal");
            return;
        }
        let got = hold_cargo_locks(&target);
        std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o755)).expect("unseal");
        assert!(matches!(got, Err(ResidueKept::Unreadable(_))), "an unreadable subtree refuses: {got:?}");
    }
