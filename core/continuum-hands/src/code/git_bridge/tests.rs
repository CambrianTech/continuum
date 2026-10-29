#![cfg(test)]
    use super::*;
    use std::fs;

    // what this catches: a remote URL form that yields the WRONG board key, or none.
    // `benchmark/dispatch` keys every card it posts by this slug, so a bad parse
    // silently files a run's cards under a repo nobody is looking at — and the scp-style
    // `git@host:owner/name` form is the one that breaks naive `split('/')` parsing,
    // because it has no scheme and uses `:` where the others use `/`.
    #[tokio::test]
    async fn a_remote_url_yields_the_owner_slash_name_board_key() {
        for (url, want) in [
            (
                "https://github.com/CambrianTech/continuum.git",
                "CambrianTech/continuum",
            ),
            (
                "https://github.com/CambrianTech/continuum",
                "CambrianTech/continuum",
            ),
            (
                "git@github.com:CambrianTech/continuum.git",
                "CambrianTech/continuum",
            ),
            (
                "ssh://git@github.com/CambrianTech/continuum.git",
                "CambrianTech/continuum",
            ),
            // Host-agnostic: a non-GitHub remote is just as valid a board key.
            ("https://gitlab.com/acme/widget.git", "acme/widget"),
            // GitLab subgroups nest deeper — take the project's own group/name.
            ("https://gitlab.com/acme/team/widget.git", "team/widget"),
            (
                "https://user:secret@example.test:8443/acme/widget.git?token=secret#secret",
                "acme/widget",
            ),
            ("ssh://git@example.test:2222/acme/widget.git", "acme/widget"),
        ] {
            assert_eq!(
                repo_slug_from_remote_url(url).as_deref(),
                Some(want),
                "remote {url:?}"
            );
        }
        // Nothing to key on → None, so the caller is ASKED for `repo` rather than
        // handed a fabricated key.
        for junk in [
            "",
            "   ",
            "https://github.com/",
            "not-a-url",
            "/tmp/acme/widget.git",
            "../acme/widget.git",
            "C:/acme/widget.git",
            "file:///tmp/acme/widget.git",
            "git@host:acme/widget?token=secret",
        ] {
            assert_eq!(repo_slug_from_remote_url(junk), None, "junk {junk:?}");
        }
    }

    async fn setup_git_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();

        // Initialize a git repo
        run_git(dir.path(), &["init"]).await.expect("git init should work");
        run_git(dir.path(), &["config", "user.email", "test@test.com"]).await.expect("git config email");
        run_git(dir.path(), &["config", "user.name", "Test"]).await.expect("git config name");

        // Create an initial commit
        fs::write(dir.path().join("initial.txt"), "hello\n").unwrap();
        run_git(dir.path(), &["add", "."]).await.expect("git add");
        run_git(dir.path(), &["commit", "-m", "Initial"]).await.expect("git commit");

        dir
    }

    // what this catches: the self-heal for stale citizen clones must bring the
    // workspace CURRENT with shared AND preserve the persona's own work. A
    // regression here silently either strands a persona on stale code (Casper's
    // pre-restructure clone) or clobbers her work on refresh.
    #[tokio::test]
    async fn sync_from_shared_brings_current_and_preserves_persona_work() {
        let shared = setup_git_repo().await;

        // citizen: a cp-clone of shared incl. .git — mirrors the `cp -cR` CoW clone
        // (related history, the whole reason a fetch+merge works).
        let citizen = tempfile::tempdir().unwrap();
        let out = std::process::Command::new("cp")
            .arg("-R")
            .arg(format!("{}/.", shared.path().display()))
            .arg(citizen.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "cp clone failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        run_git(citizen.path(), &["config", "user.email", "citizen@test"]).await.unwrap();
        run_git(citizen.path(), &["config", "user.name", "Citizen"]).await.unwrap();

        // shared advances (a new framework file), citizen has its OWN uncommitted work.
        fs::write(shared.path().join("framework.rs"), "// shared code\n").unwrap();
        run_git(shared.path(), &["add", "."]).await.unwrap();
        run_git(shared.path(), &["commit", "-m", "shared: add framework.rs"]).await.unwrap();
        fs::write(citizen.path().join("my_work.rs"), "// persona code\n").unwrap();

        let report = git_sync_from_shared(citizen.path(), shared.path()).await.expect("sync ok");
        assert!(report.synced, "should report a sync: {}", report.summary);

        // BOTH survive: shared's new file arrives, the persona's work is preserved.
        assert!(
            citizen.path().join("framework.rs").exists(),
            "shared file must arrive"
        );
        assert!(
            citizen.path().join("my_work.rs").exists(),
            "persona work must survive"
        );
        assert!(
            citizen.path().join("initial.txt").exists(),
            "base file still present"
        );

        // Idempotent: a second sync is a clean no-op.
        let again = git_sync_from_shared(citizen.path(), shared.path()).await.expect("2nd sync ok");
        assert!(!again.synced, "already current: {}", again.summary);
    }

    // what this catches (card 8fecb762): the sync autosaving a citizen's workspace onto
    // her card branch and merging shared into it. Kimi's 63855189 branch carried five such
    // autosaves (3,026 files) and both its PRs were closed unreviewable. On a card branch
    // the sync must commit nothing, merge nothing, and leave her uncommitted work in place.
    #[tokio::test]
    async fn sync_from_shared_leaves_a_card_branch_untouched() {
        let shared = setup_git_repo().await;
        let citizen = tempfile::tempdir().unwrap();
        let out = std::process::Command::new("cp")
            .arg("-R")
            .arg(format!("{}/.", shared.path().display()))
            .arg(citizen.path())
            .output()
            .unwrap();
        assert!(out.status.success(), "cp clone failed: {}", String::from_utf8_lossy(&out.stderr));
        run_git(citizen.path(), &["checkout", "-b", "63855189/code-github-pr-create-checks-the"]).await.unwrap();
        let head = run_git(citizen.path(), &["rev-parse", "HEAD"]).await.unwrap();

        fs::write(shared.path().join("framework.rs"), "// shared code\n").unwrap();
        run_git(shared.path(), &["add", "."]).await.unwrap();
        run_git(shared.path(), &["commit", "-m", "shared: add framework.rs"]).await.unwrap();
        fs::write(citizen.path().join("repro_scratch.py"), "# workspace debris\n").unwrap();

        let report = git_sync_from_shared(citizen.path(), shared.path()).await.expect("sync ok");
        assert!(!report.synced, "a card branch is not synced: {}", report.summary);
        assert_eq!(run_git(citizen.path(), &["rev-parse", "HEAD"]).await.unwrap(), head, "no autosave or merge commit");
        assert!(!citizen.path().join("framework.rs").exists(), "shared was not merged in");
        assert!(
            run_git(citizen.path(), &["status", "--porcelain"]).await.unwrap().contains("repro_scratch.py"),
            "her uncommitted file stays uncommitted"
        );
    }

    // what this catches: the citizen-sync modify/delete strand (glass-boxed
    // 2026-08-03 round 2): shared DELETED debris files (the #2135 sweep) that the
    // persona's autosave had modified — `-X theirs` never auto-resolves
    // modify/delete, so the sync failed forever on the very files it was meant to
    // clear. Shared's deletion must win the working tree; the persona's version
    // survives in her autosave commit (history, not tree).
    #[tokio::test]
    async fn sync_resolves_modify_delete_shared_deletion_wins() {
        let shared = setup_git_repo().await;
        // Debris file exists in the common base.
        fs::write(shared.path().join("conway.rs"), "// v1\n").unwrap();
        run_git(shared.path(), &["add", "."]).await.unwrap();
        run_git(shared.path(), &["commit", "-m", "base: debris present"]).await.unwrap();

        // Citizen cp-clone (related history).
        let citizen = tempfile::tempdir().unwrap();
        let out = std::process::Command::new("cp")
            .arg("-R")
            .arg(format!("{}/.", shared.path().display()))
            .arg(citizen.path())
            .output()
            .unwrap();
        assert!(out.status.success());
        run_git(citizen.path(), &["config", "user.email", "citizen@test"]).await.unwrap();
        run_git(citizen.path(), &["config", "user.name", "Citizen"]).await.unwrap();

        // Shared SWEEPS the debris + ships a framework file; citizen MODIFIES it
        // (uncommitted — the autosave inside the sync commits it, the live shape).
        run_git(shared.path(), &["rm", "-q", "conway.rs"]).await.unwrap();
        fs::write(shared.path().join("framework3.rs"), "// newer\n").unwrap();
        run_git(shared.path(), &["add", "framework3.rs"]).await.unwrap();
        run_git(shared.path(), &["commit", "-m", "shared: sweep debris"]).await.unwrap();
        fs::write(citizen.path().join("conway.rs"), "// persona edit\n").unwrap();

        let report = git_sync_from_shared(citizen.path(), shared.path()).await
            .expect("sync must survive modify/delete");
        assert!(report.synced, "should sync: {}", report.summary);
        assert!(
            !citizen.path().join("conway.rs").exists(),
            "shared's deletion wins the tree"
        );
        assert!(
            citizen.path().join("framework3.rs").exists(),
            "shared file arrives"
        );
        // Preserve-first: her edit is one commit back in HER history.
        let show = run_git(
            citizen.path(),
            &["log", "--all", "-S", "persona edit", "--oneline"],
        ).await
        .unwrap_or_default();
        assert!(
            !show.trim().is_empty(),
            "the persona's modified version survives in history"
        );
    }

    // what this catches: the citizen-sync submodule strand (glass-boxed 2026-08-03,
    // all four residents, every ensure): the persona's autosave commits her stale
    // vendored-submodule pointer while shared advances it, and git's recursive
    // merge refuses non-trivial gitlink divergence regardless of -X theirs
    // ("Recursive merging with submodules currently only supports trivial cases")
    // — so the self-heal ABORTED forever and every workspace stayed stale. The
    // sync must resolve gitlink conflicts shared-wins and conclude the merge.
    #[tokio::test]
    async fn sync_resolves_diverged_submodule_pointer_shared_wins() {
        fn raw_git(dir: &Path, args: &[&str]) -> std::process::Output {
            let mut c = std::process::Command::new("git");
            c.args(args).current_dir(dir);
            for v in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_PREFIX"] {
                c.env_remove(v);
            }
            c.output().expect("git spawn")
        }

        // Upstream submodule repo with two commits: P0 (the pointer both clones
        // start at) and S2 (what shared advances to).
        let sub = setup_git_repo().await;
        let _p0 = run_git(sub.path(), &["rev-parse", "HEAD"]).await
            .unwrap()
            .trim()
            .to_string();
        fs::write(sub.path().join("v2.txt"), "v2\n").unwrap();
        run_git(sub.path(), &["add", "."]).await.unwrap();
        run_git(sub.path(), &["commit", "-m", "sub: v2"]).await.unwrap();
        let s2 = run_git(sub.path(), &["rev-parse", "HEAD"]).await
            .unwrap()
            .trim()
            .to_string();

        // Shared checkout carrying the submodule at P0.
        let shared = setup_git_repo().await;
        let out = raw_git(
            shared.path(),
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub.path().display().to_string(),
                "vendor/sub",
            ],
        );
        assert!(
            out.status.success(),
            "submodule add: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        // Pin the submodule at P0 (submodule add checks out its current HEAD = S2 tip;
        // rewind the gitlink so both sides start from a common base pointer).
        run_git(shared.path(), &["commit", "-m", "shared: add submodule"]).await.unwrap();

        // Citizen: cp-clone of shared (related history), like the CoW layer.
        let citizen = tempfile::tempdir().unwrap();
        let out = std::process::Command::new("cp")
            .arg("-R")
            .arg(format!("{}/.", shared.path().display()))
            .arg(citizen.path())
            .output()
            .unwrap();
        assert!(out.status.success());
        run_git(citizen.path(), &["config", "user.email", "citizen@test"]).await.unwrap();
        run_git(citizen.path(), &["config", "user.name", "Citizen"]).await.unwrap();

        // Shared moves the pointer to S2 + ships a framework file.
        run_git(
            shared.path(),
            &[
                "update-index",
                "--cacheinfo",
                &format!("160000,{s2},vendor/sub"),
            ],
        ).await
        .unwrap();
        fs::write(shared.path().join("framework2.rs"), "// newer shared\n").unwrap();
        run_git(shared.path(), &["add", "framework2.rs"]).await.unwrap();
        run_git(
            shared.path(),
            &["commit", "-m", "shared: bump sub + framework2"],
        ).await
        .unwrap();

        // Citizen diverges the pointer to a THIRD sha (what autosave does with a
        // drifted vendored tree) plus her own uncommitted work.
        let stray = run_git(citizen.path(), &["rev-parse", "HEAD"]).await
            .unwrap()
            .trim()
            .to_string();
        run_git(
            citizen.path(),
            &[
                "update-index",
                "--cacheinfo",
                &format!("160000,{stray},vendor/sub"),
            ],
        ).await
        .unwrap();
        run_git(
            citizen.path(),
            &["commit", "-m", "citizen: drifted pointer"],
        ).await
        .unwrap();
        fs::write(citizen.path().join("my_work.rs"), "// persona code\n").unwrap();

        let report = git_sync_from_shared(citizen.path(), shared.path()).await
            .expect("sync must survive a diverged submodule pointer");
        assert!(report.synced, "should sync: {}", report.summary);

        // Shared's pointer wins; shared's file arrives; persona work survives.
        let tree = run_git(citizen.path(), &["ls-tree", "HEAD", "vendor/sub"]).await.unwrap();
        assert!(
            tree.contains(&s2),
            "gitlink must resolve to shared's sha: {tree}"
        );
        assert!(citizen.path().join("framework2.rs").exists());
        assert!(citizen.path().join("my_work.rs").exists());
    }

    #[tokio::test]
    async fn test_git_status_clean() {
        let dir = setup_git_repo().await;
        let status = git_status(dir.path()).await;
        assert!(status.success);
        assert!(status.branch.is_some());
        assert!(status.modified.is_empty());
        assert!(status.untracked.is_empty());
    }

    #[tokio::test]
    async fn test_git_status_modified() {
        let dir = setup_git_repo().await;
        fs::write(dir.path().join("initial.txt"), "modified\n").unwrap();

        let status = git_status(dir.path()).await;
        assert!(status.success);
        assert!(status.modified.contains(&"initial.txt".to_string()));
    }

    #[tokio::test]
    async fn test_git_status_untracked() {
        let dir = setup_git_repo().await;
        fs::write(dir.path().join("new_file.txt"), "new\n").unwrap();

        let status = git_status(dir.path()).await;
        assert!(status.success);
        assert!(status.untracked.contains(&"new_file.txt".to_string()));
    }

    #[tokio::test]
    async fn test_git_diff() {
        let dir = setup_git_repo().await;
        fs::write(dir.path().join("initial.txt"), "modified\n").unwrap();

        let diff = git_diff(dir.path(), false).await.unwrap();
        assert!(diff.contains("-hello"));
        assert!(diff.contains("+modified"));
    }

    #[tokio::test]
    async fn test_git_log() {
        let dir = setup_git_repo().await;
        let log = git_log(dir.path(), 5).await.unwrap();
        assert!(log.contains("Initial"));
    }

    // what this catches (Codex reviewer on #5041): the runner's discovery ceiling was the root
    // ITSELF, and git never chdirs up INTO a ceiling but does check its children, so a root with
    // an empty `.git` resolved the repository ABOVE it, and its identity and origin with it.
    // Measured on macOS and Windows git: the ceiling must be the root's parent.
    #[tokio::test]
    async fn a_root_never_discovers_the_repository_above_it() {
        let parent = setup_git_repo().await;
        let child = parent.path().join("child");
        fs::create_dir_all(child.join(".git")).unwrap(); // an empty, invalid .git
        let err = run_git(&child, &["rev-parse", "--show-toplevel"]).await.expect_err("the child is not a repository");
        assert!(err.contains("not a git repository"), "never the parent's toplevel: {err}");
        let plain = parent.path().join("plain");
        fs::create_dir_all(&plain).unwrap(); // no .git at all
        assert!(run_git(&plain, &["rev-parse", "--show-toplevel"]).await.is_err(), "a plain subdirectory is not its parent's repository");
    }

    #[tokio::test]
    async fn test_git_status_not_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        let status = git_status(dir.path()).await;
        // Should still return a result (possibly with error)
        // git status in non-repo returns error
        assert!(!status.success || status.branch.is_none());
    }

    #[tokio::test]
    async fn test_git_add_and_commit() {
        let dir = setup_git_repo().await;

        // Create a new file
        fs::write(dir.path().join("feature.txt"), "new feature\n").unwrap();

        // Stage it
        git_add(dir.path(), &["feature.txt"]).await.expect("git add should work");

        // Status should show it as added
        let status = git_status(dir.path()).await;
        assert!(status.added.contains(&"feature.txt".to_string()));

        // Commit it
        let hash = git_commit(dir.path(), "Add feature").await.expect("git commit should work");
        assert!(!hash.is_empty());
        assert!(hash.len() >= 7); // At least a short hash

        // Status should be clean now
        let status_after = git_status(dir.path()).await;
        assert!(status_after.modified.is_empty());
        assert!(status_after.added.is_empty());
        assert!(status_after.untracked.is_empty());
    }

    #[tokio::test]
    async fn test_git_commit_empty_fails() {
        let dir = setup_git_repo().await;
        // Nothing staged — commit should fail
        let result = git_commit(dir.path(), "Empty commit").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_git_add_all() {
        let dir = setup_git_repo().await;

        fs::write(dir.path().join("a.txt"), "aaa\n").unwrap();
        fs::write(dir.path().join("b.txt"), "bbb\n").unwrap();

        git_add(dir.path(), &["."]).await.expect("git add . should work");

        let status = git_status(dir.path()).await;
        // Both files should be staged (added)
        assert!(status.added.contains(&"a.txt".to_string()));
        assert!(status.added.contains(&"b.txt".to_string()));
    }

    // what this catches (card bfa87dd5): a git the core waited on ran unbounded on a blocking
    // thread, so a cancelled command or a passed deadline abandoned it, and the git and what it
    // started kept running under hands a release then removed. A git now ends with its waiter and
    // says how: at the deadline it is INTERRUPTED (its effect unknown, never "failed"), with no
    // time left it never starts, cancelled it ends before the hands' release completes, and stdin
    // it never took is an error, never a result. That the TREE ends is lane_process's contract,
    // tested once there; this pins that every git goes through it and how its ends read.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_git_ends_with_its_waiter_and_says_how_it_ended() {
        use std::os::unix::fs::PermissionsExt;
        let dir = setup_git_repo().await;
        let hooks = tempfile::tempdir().unwrap();
        let hook_pid = hooks.path().join("hook.pid");
        // a pre-commit hook is a process git starts: it records itself and outlives every bound here
        let hook = hooks.path().join("pre-commit");
        fs::write(&hook, format!("#!/bin/sh\necho $$ > {}\nexec sleep 600\n", hook_pid.display())).unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        let hooks_path = format!("core.hooksPath={}", hooks.path().display());
        let commit = ["-c", hooks_path.as_str(), "commit", "--allow-empty", "-q", "-m", "held"];

        // the deadline only has to land while the hook sleeps (600 s); 2 s is that margin, not a tuning
        let started = tokio::time::Instant::now();
        let err = within(started + std::time::Duration::from_secs(2), run_git(dir.path(), &commit)).await.unwrap_err();
        assert!(interrupted(&err) && out_of_time(&err), "a git the deadline ended is interrupted: {err}");
        assert!(started.elapsed() < std::time::Duration::from_secs(60), "the deadline ended it, not the hook");
        // regression for card 0d18171d (canary a3546a125, run 38689841317): SIGKILL to another
        // process is not synchronous, so the first check after it may still see the hook running
        // and report it "not confirmed gone" (the hands lease keeps it for release's recheck).
        // What must hold is that the tree ENDS: the hook it started is gone (a zombie is gone,
        // the same rule as continuum_process::still_present) well inside this test's own bound.
        let hook = fs::read_to_string(&hook_pid).expect("the hook ran and recorded its pid");
        let hook = sysinfo::Pid::from_u32(hook.trim().parse().expect("the hook's pid"));
        let hooks = [hook];
        let gone_by = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let mut sys = sysinfo::System::new();
            let only_hook = sysinfo::ProcessesToUpdate::Some(&hooks);
            sys.refresh_processes_specifics(only_hook, true, sysinfo::ProcessRefreshKind::nothing());
            if sys.process(hook).is_none_or(|p| p.status() == sysinfo::ProcessStatus::Zombie) {
                break;
            }
            assert!(tokio::time::Instant::now() < gone_by, "the hook the deadline ended is still running: {err}");
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }

        let err = within(tokio::time::Instant::now(), run_git(dir.path(), &["status"])).await.unwrap_err();
        assert!(out_of_time(&err) && !interrupted(&err), "with no time left a git never starts: {err}");

        let _ = fs::remove_file(&hook_pid);
        let context = crate::identity::hands_context::HandsContext::fresh();
        let lease = std::sync::Arc::new(crate::identity::hands_gate::admit(context).expect("open hands"));
        let run = crate::identity::hands_gate::admitted(Some(lease), run_git(dir.path(), &commit));
        tokio::select! {
            out = run => panic!("the git ended on its own: {out:?}"),
            () = async {
                while !hook_pid.exists() {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            } => {}, // the command is cancelled while its git's hook runs: the waiter is dropped
        }
        // The release, as code/release-hands runs it: close, then re-check what the cancelled git's
        // tree left (end_tree takes ONE snapshot after its kill, so a descendant still dying is
        // reported, conservatively, never forgotten), retrying until confirmed. Nothing may stay
        // unknown, and every reported process must be confirmed gone by the release's own recheck
        // within the deadline (#5062's CI, 2026-10-15: pid 15439 was unconfirmed at the snapshot;
        // card fb166622; the same race #5053 fixed for the hook pid).
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let lingering = crate::identity::hands_gate::close(context).await;
            assert!(lingering.unknown.is_empty(), "no tree it started is unknown: {lingering:?}");
            let mut still = lingering.processes;
            while !still.is_empty() {
                still = continuum_process::recheck(&still);
                crate::identity::hands_gate::retain_processes(context, still.clone());
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            assert!(crate::identity::hands_gate::close(context).await.is_empty(), "the release confirmed everything it started");
        })
        .await
        .expect("the release confirms the cancelled git's tree ended");

        // a pipe holds 64 KiB; 1 MiB to a git that never reads stdin can't be delivered whole
        let err = run_git_input(dir.path(), &["--version"], &[], Some(vec![b'x'; 1 << 20])).await.unwrap_err();
        assert_eq!(err.end, GitEnd::Unknown, "it exited 0 without taking its input: what it did is unknown");
        assert!(err.to_string().contains("--version"), "input the git never took is an error naming it, never its output: {err}");
    }
