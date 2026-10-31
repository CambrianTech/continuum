#![cfg(test)]
use super::*;
use std::sync::Arc;

    // what this catches (Fable on the partition, 2026-10-11): a job directory now holds PARKED
    // examples (partition.json beside request.json) that nobody has trained. No tracked class may
    // contain a persona's job directories, so no pool's eviction can delete them; one that is
    // added must first decide what happens to parked examples (move them, or skip such dirs).
    #[test]
    fn no_tracked_class_contains_a_persona_job_directory() {
        let home = std::path::Path::new("/h");
        let job = home.join(".continuum/genome/Kimi/code_owner/00000000-0000-0000-0000-000000000001");
        for dir in standard_tracked_dirs(home, &home.join(".continuum")) {
            assert!(!job.starts_with(dir.path()), "{} ({}) contains persona job directories", dir.class(), dir.path().display());
        }
    }

    // what this catches (2026-09-21, the M5 at 97% full): a cargo target tree that has a
    // TrackedDir row but no eviction owner, because the boot registration loop held its
    // OWN literal list of classes and nobody updated both places. `cargo-target-airc`
    // held 9.8 GB that no reporter could see and no pool could evict — the same failure
    // `cargo-target-wt` had at 87 GB, repeating after the law against it was written,
    // because a law lives in a doc and this lives in a test.
    //
    // The invariant: every tracked class that IS a cargo target tree appears in
    // `CARGO_TARGET_CLASSES`, which `ipc/mod.rs` walks to hand each one a
    // `CargoTargetPool`. Add a fourth tree's row without adding it here and this fails.
    //
    // Mutation check: dropping "cargo-target-airc" from the const fails the first assert.
    #[test]
    fn every_cargo_target_class_has_an_owner() {
        let dirs = standard_tracked_dirs(std::path::Path::new("/h"), std::path::Path::new("/h/.continuum"));
        let tracked_cargo: Vec<&str> = dirs
            .iter()
            .map(|d| d.class())
            .filter(|c| c.starts_with("cargo-target"))
            .collect();

        for class in tracked_cargo {
            assert!(
                CARGO_TARGET_CLASSES.contains(&class),
                "tracked class {class:?} is a cargo target tree with no entry in \
                 CARGO_TARGET_CLASSES — it has a row (so the scanner sees it) but the boot \
                 loop in ipc/mod.rs never hands it a CargoTargetPool, so nothing evicts it. \
                 Add it to the const; both consumers then follow."
            );
        }

        // ...and the const names nothing that is not actually tracked, so a stale entry
        // cannot sit there implying an owner for a class that no longer exists.
        let tracked: Vec<&str> = dirs.iter().map(|d| d.class()).collect();
        for class in CARGO_TARGET_CLASSES {
            assert!(
                tracked.contains(&class),
                "CARGO_TARGET_CLASSES names {class:?}, which standard_tracked_dirs does not \
                 track — the boot loop will look it up, find nothing, and silently register \
                 no pool at all."
            );
        }
    }

    // what this catches: the hf-hub class tracking a path the artifacts are NOT at. It hardcoded
    // `home/.cache/huggingface` while the cold-storage installer relocates HF_HOME — measured on
    // BigMama, the class reported 0 bytes at the default while 2.48 TB sat on the cold drive,
    // invisible to the disk governor and owned by no eviction pool. A tracked dir pointing at the
    // wrong place is worse than an untracked one: it reports a reassuring zero.
    //
    // Mutation check: reverting to `home.join(".cache/huggingface")` fails the first assert.
    #[test]
    fn hf_hub_class_follows_a_relocated_cache_root() {
        let relocated = tempfile::tempdir().expect("tempdir");
        let home = tempfile::tempdir().expect("home");
        // Under the one crate-wide home lock (#4082): an ordinary #[test] is concurrent
        // with the rest of libtest, and every HomeGuard writes HF_HOME. The guard pins
        // the four home variables to `home`; the relocated HF_HOME override sits on top
        // and the guard's drop restores everything.
        let _home = crate::paths::test_home::HomeGuard::set_blocking(home.path());
        std::env::set_var("HF_HOME", relocated.path());

        let dirs = standard_tracked_dirs(home.path(), &home.path().join(".continuum"));
        let hf = dirs
            .iter()
            .find(|d| d.name == "hf-hub")
            .expect("hf-hub class must exist");
        assert_eq!(
            hf.path(),
            relocated.path(),
            "hf-hub must track the RELOCATED cache root, not the default under home"
        );
        assert!(
            !hf.path().starts_with(home.path()),
            "tracking a path under home while HF_HOME points elsewhere is the false-zero bug"
        );
    }


    // what this catches: the standard cache-class registry names exactly
    // the directories from the 2026-07-13 incident — losing one from this
    // list silently un-names a known creep source.
    #[test]
    fn standard_dirs_cover_the_incident_cache_classes() {
        let dirs = standard_tracked_dirs(std::path::Path::new("/home/u"), std::path::Path::new("/home/u/.continuum"));
        let names: Vec<&str> = dirs.iter().map(|d| d.name).collect();
        for must in [
            "cargo-target",
            "genome-models",
            "hf-hub",
            "citizens",
            "forge",
        ] {
            assert!(names.contains(&must), "missing cache class: {must}");
        }
    }

    // what this catches (card 4dedb177, plan line 7, Codex's review of #5368): every continuum
    // cache class was a hand-built `~/.continuum/...`, so a relocated `$CONTINUUM_HOME` left the
    // tracker measuring a home nothing uses; and a tracker moved without its writer would hide
    // real downloads. The artifact writer's root IS a tracked root; the other continuum classes
    // follow the continuum home; airc stays under the user home; bytes already in the historical
    // artifact root stay tracked (once) while they exist, and only that class remains there.
    #[test]
    fn a_relocated_continuum_home_tracks_the_writers_roots_and_the_legacy_bytes() {
        let home = tempfile::tempdir().expect("home");
        let continuum = tempfile::tempdir().expect("continuum home");
        let _home = crate::paths::test_home::HomeGuard::set_blocking(home.path());
        let at = |dirs: &[Arc<TrackedDir>], class: &str| dirs.iter().find(|d| d.class() == class).map(|d| d.path().to_path_buf());

        let dirs = standard_tracked_dirs(home.path(), continuum.path());
        let writer = crate::paths::artifacts::root_under(continuum.path()).expect("writer root");
        assert!(dirs.iter().any(|d| d.path() == writer.as_path()), "the writer's root is tracked");
        assert_eq!(at(&dirs, "kv-pages"), Some(continuum.path().join("cache/kv-pages")));
        assert_eq!(at(&dirs, "genome-models"), Some(continuum.path().join("genome/models")));
        assert_eq!(at(&dirs, "airc-worktrees"), Some(home.path().join(".airc/worktrees")), "airc keeps its own home");
        assert_eq!(at(&dirs, "core-artifacts-legacy"), None, "no historical bytes, no row");

        let legacy = crate::paths::artifacts::legacy_root(home.path());
        std::fs::create_dir_all(&legacy).expect("historical artifacts");
        let dirs = standard_tracked_dirs(home.path(), continuum.path());
        assert_eq!(at(&dirs, "core-artifacts-legacy"), Some(legacy.clone()), "bytes already written stay visible");
        let old: Vec<&str> = dirs.iter().filter(|d| d.path().starts_with(home.path().join(".continuum"))).map(|d| d.class()).collect();
        assert_eq!(old, ["core-artifacts-legacy"], "only the historical artifacts remain there");
        let default = standard_tracked_dirs(home.path(), &home.path().join(".continuum"));
        assert_eq!(default.iter().filter(|d| d.path() == legacy.as_path()).count(), 1, "default homes: counted once");
    }
