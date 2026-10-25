//! Confinement tests, moved out of `mod.rs` to keep it under 1,000 lines (2026-10-24).
//! Bodies keep their original indentation.
#![cfg(test)]

    use super::*;

    /// A fake home OUTSIDE the process temp dir: temp is a write root for every
    /// citizen (code/run lives there), so a home under it would be wholly permitted
    /// and the tests would pass for the wrong reason (they did, first run).
    fn home_with(entries: &[&str]) -> tempfile::TempDir {
        let outside_temp = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let home = tempfile::Builder::new()
            .prefix(".confinement-test-home-")
            .tempdir_in(outside_temp)
            .expect("test: home");
        for e in entries {
            let p = home.path().join(e);
            if e.ends_with('/') {
                std::fs::create_dir_all(&p).expect("test: dir");
            } else {
                std::fs::create_dir_all(p.parent().expect("test: parent")).expect("test: parent dir");
                std::fs::write(&p, "x").expect("test: file");
            }
        }
        home
    }

    // what this catches (card d598c806; Cormac on #4882): the policy granting a citizen
    // the operator's files, a credential (named or inside a granted directory), or
    // another citizen's directory; or refusing her own workspace (a card worktree under
    // `~/.airc`, a denied-by-omission dot entry), her citizen directory, the toolchains
    // or the shared caches.
    // what this catches (independent review of #5383): a granted root lying INSIDE a carve-out
    // was kept whole, so an allow-only enforcer (landlock) granted what the carve-out denies.
    #[test]
    fn a_root_inside_a_carve_out_is_never_granted() {
        let home = home_with(&[".ssh/keys/id_ed25519", "Documents/letter.txt"]);
        let h = home.path().canonicalize().expect("test: canonical");
        let mut roots = vec![h.join(".ssh/keys"), h.join("Documents")];
        carve(&mut roots, &[h.join(".ssh")]);
        assert_eq!(roots, vec![h.join("Documents")], "the root inside .ssh is dropped, the other kept");
    }

    // what this catches (independent review of #5383): a citizen re-pointing her own
    // `workspace` at another citizen's private state (or anywhere else) to have it granted as a
    // shared read-write root. Only a real directory inside its own citizen's directory counts.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_or_escaping_workspace_is_never_shared() {
        let (me, honest, linker, victim) = (uuid::Uuid::from_u128(7), uuid::Uuid::from_u128(8), uuid::Uuid::from_u128(9), uuid::Uuid::from_u128(10));
        let home = home_with(&[
            &format!(".continuum/citizens/peers/{me}/workspace/"),
            &format!(".continuum/citizens/peers/{honest}/workspace/src.rs"),
            &format!(".continuum/citizens/peers/{linker}/notes"),
            &format!(".continuum/citizens/peers/{victim}/experience.jsonl"),
        ]);
        let h = home.path().canonicalize().expect("test: canonical");
        let peers = h.join(".continuum/citizens/peers");
        std::os::unix::fs::symlink(peers.join(victim.to_string()), peers.join(linker.to_string()).join("workspace"))
            .expect("test: the linker re-points her workspace at the victim's directory");
        let ws = peers.join(me.to_string()).join("workspace");
        let c = Confinement::for_caller(&h, &h.join(".continuum"), &me.to_string(), &ws);
        let shared = c.shared_workspaces();
        assert!(shared.contains(&peers.join(honest.to_string()).join("workspace")), "a real workspace is shared: {shared:?}");
        assert!(!shared.iter().any(|p| p.starts_with(peers.join(victim.to_string()))), "the victim's private state is never shared: {shared:?}");
        let list = c.materialize();
        assert!(!list.permits_read(&peers.join(victim.to_string()).join("experience.jsonl")), "nor readable through the allow list");
    }

    #[test]
    fn a_citizens_allow_list_holds_her_roots_and_nothing_of_the_operators() {
        let me = uuid::Uuid::from_u128(7);
        let other = uuid::Uuid::from_u128(8);
        let home = home_with(&[
            ".zshrc",
            ".zsh_history",
            ".gitconfig",
            ".git-credentials",
            ".cargo/registry/",
            ".cache/huggingface/token",
            ".cache/huggingface/hub/",
            ".cache/pip/",
            ".local/share/uv/tools/",
            ".local/share/keyrings/login.keyring",
            ".config/gh/hosts.yml",
            ".config/git/config",
            ".config/git/credentials",
            ".claude/credentials.json",
            ".ssh/id_ed25519",
            ".airc/worktrees/ab12cd34/src/",
            ".airc/identity/key",
            ".continuum/config.env",
            ".continuum/models/",
            ".continuum/cache/cargo-target/",
            ".continuum/citizens/humans/",
            &format!(".continuum/citizens/peers/{me}/workspace/"),
            &format!(".continuum/citizens/peers/{other}/workspace/"),
            &format!(".continuum/citizens/peers/{other}/experience.jsonl"),
            "miniconda3/envs/",
            "Pictures/holiday.jpg",
            "Documents/resume/.git/",
            "Development/continuum/",
        ]);
        let h = home.path().canonicalize().expect("test: canonical");
        let card_worktree = h.join(".airc/worktrees/ab12cd34");
        let c = Confinement::for_caller(&h, &h.join(".continuum"), &me.to_string(), &card_worktree);
        let list = c.materialize();
        let ok = |rel: &str| list.permits_read(&h.join(rel));
        // hers
        assert!(ok(".airc/worktrees/ab12cd34/src/lib.rs"), "her card worktree, under a dot entry that is not granted");
        assert!(ok(&format!(".continuum/citizens/peers/{me}/experience.jsonl")), "her citizen dir, under a peers root that is never granted whole");
        assert!(ok(".zshrc"), "her login shell's profile");
        assert!(ok(".gitconfig"), "git's config");
        assert!(ok(".cargo/registry/index"), "the toolchain");
        assert!(ok(".cache/pip/wheels"), "a package cache");
        assert!(ok(".local/share/uv/tools/ruff"), "uv's tool installs");
        assert!(ok(".cache/huggingface/hub/x.gguf"), "the model cache");
        assert!(ok(".config/git/config"), "git's config dir");
        assert!(ok("miniconda3/envs/py311"), "a non-dot toolchain install");
        assert!(ok(".continuum/models/x.gguf"), "the models");
        assert!(ok(".continuum/cache/cargo-target/debug"), "the shared build cache");
        assert!(ok(".continuum/citizens/humans/joel"), "citizens that are not peers");
        assert!(ok(&format!(".continuum/citizens/peers/{other}/workspace/src.rs")), "ANOTHER citizen's workspace: the commons (2026-10-24)");
        let commons = h.join(format!(".continuum/citizens/peers/{other}/workspace/src.rs"));
        assert!(list.write.iter().any(|r| commons.starts_with(r)), "and writable, not only readable: the commons is a write root");
        // never
        assert!(!ok(".airc/identity/key"), "airc's identity, beside her worktree");
        assert!(!ok(&format!(".continuum/citizens/peers/{other}/experience.jsonl")), "ANOTHER citizen's private state");
        assert!(!ok(".ssh/id_ed25519"), "a secret");
        assert!(!ok(".git-credentials"), "git credentials");
        assert!(!ok(".config/gh/hosts.yml"), "the GitHub token");
        assert!(!ok(".config/git/credentials"), "a credential inside a granted dir");
        assert!(!ok(".cache/huggingface/token"), "a credential inside a WRITE root");
        assert!(!ok(".claude/credentials.json"), "an agent's auth");
        assert!(!ok(".local/share/keyrings/login.keyring"), "the Linux secret store, beside the uv tools");
        assert!(!ok(".zsh_history"), "a shell history");
        assert!(!ok(".continuum/config.env"), "the core's secrets");
        assert!(!ok("Pictures/holiday.jpg"), "the operator's photos");
        assert!(!ok("Documents/resume/.git/HEAD"), "the operator's resume");
        assert!(!ok("Development/continuum/Cargo.toml"), "the operator's own checkout");
        // write vs read
        assert!(list.write.contains(&card_worktree), "her workspace is writable");
        assert!(list.write.iter().any(|w| w.ends_with(".cargo")), "a build writes the registry");
        assert!(!list.write.iter().any(|w| w.ends_with(".zshrc") || w.ends_with("models") || w.ends_with("miniconda3")), "rc files, models and installs are read-only");

        // d86: a relative operator target follows the actual child cwd, never
        // the workspace or an arbitrary per-session environment override.
        let child_cwd = h.join("build-location");
        let target = crate::paths::cargo::resolve_target(Path::new("derived"), &child_cwd);
        let override_root = h.join("private-override");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::create_dir_all(&override_root).unwrap();
        let grant = CargoTargetGrant::admitted(c.validate_cargo_target(&target).unwrap(), c.clone());
        let configured_list = c.materialize_with_cargo(Some(&grant));
        assert!(configured_list.write.contains(&target.canonicalize().unwrap()));
        assert!(!configured_list.write.contains(&child_cwd.canonicalize().unwrap()), "no parent grant");
        assert!(!configured_list.write.contains(&override_root.canonicalize().unwrap()), "session overrides do not grant access");
        for forbidden in [h.clone(), h.parent().unwrap().to_path_buf(), h.join(".local/share/keyrings/cargo"), h.join(".continuum/citizens/peers").join(other.to_string()).join("derived")] {
            assert!(c.validate_cargo_target(&forbidden).is_err(), "unsafe physical grant: {}", forbidden.display());
        }
        let other_policy = Confinement::for_caller(&h, &h.join(".continuum"), &other.to_string(), &card_worktree);
        assert!(!other_policy.materialize_with_cargo(Some(&grant)).write.contains(&target.canonicalize().unwrap()), "an admitted grant cannot cross citizen policies");
        let relocated = h.join("relocated-substrate");
        let relocated_policy = Confinement::for_caller(&h, &relocated, &me.to_string(), &card_worktree);
        assert!(relocated_policy.validate_cargo_target(&crate::identity::citizen_peer_dir(&relocated, crate::identity::PeerId::from_uuid(other))).is_err());
        assert!(relocated_policy.validate_cargo_target(&relocated.join("config.env/derived")).is_err());
        assert!(relocated_policy.validate_cargo_target(&crate::identity::citizen_peer_dir(&relocated, crate::identity::PeerId::from_uuid(me)).join("derived")).is_ok());
        let explicit = command_for(None, "operator", Path::new("tool"), &[], Some(&child_cwd), None);
        assert_eq!(explicit.as_std().get_current_dir(), Some(child_cwd.as_path()));
        let inherited = command_for(None, "operator", Path::new("tool"), &[], None, None);
        assert_eq!(inherited.as_std().get_current_dir(), None, "None preserves inherited cwd");
        let confined = command_for(Some(&c), &me.to_string(), Path::new("tool"), &[], Some(&child_cwd), Some(&grant));
        assert_eq!(confined.as_std().get_current_dir(), Some(child_cwd.as_path()), "the real confinement branch retains the actual spawn cwd");
        #[cfg(target_os = "macos")]
        {
            let expected = seatbelt_profile(&c.home, &c.materialize_with_cargo(Some(&grant)));
            let args: Vec<_> = confined.as_std().get_args().collect();
            assert_eq!(args[1], std::ffi::OsStr::new(&expected), "command_for materializes configuration against its own cwd");
        }
    }

    // what this catches (Joel 2026-10-18 "keep creds off-limits"): a host-reach resident
    // granted a credential store, a scope's signing key, or another citizen's directory;
    // or refused her own citizen dir or the operator's ordinary files. Also the carve
    // defect this caught: siblings of the FIRST carve-out (`.aws` beside `.ssh`,
    // `.config` holding `.config/gh`) were granted whole.
    #[test]
    fn a_resident_reaches_the_host_except_every_credential_and_other_citizens() {
        let me = uuid::Uuid::from_u128(7);
        let other = uuid::Uuid::from_u128(8);
        let home = home_with(&[
            ".ssh/id_ed25519",
            ".aws/credentials",
            ".config/gh/hosts.yml",
            ".config/htop/htoprc",
            ".claude/credentials.json",
            ".cargo/credentials.toml",
            ".cargo/registry/",
            ".continuum/config.env",
            ".continuum/models/",
            &format!(".continuum/citizens/peers/{me}/notes"),
            &format!(".continuum/citizens/peers/{other}/notes"),
            "Development/continuum/.airc/identity.key",
            "Development/continuum/src/lib.rs",
            "Documents/letter.txt",
        ]);
        let h = home.path().canonicalize().expect("test: canonical");
        let ws = h.join("Development/continuum");
        let c = Confinement::for_caller(&h, &h.join(".continuum"), &me.to_string(), &ws).reaching_host();
        let list = c.materialize();
        let ok = |rel: &str| list.permits_read(&h.join(rel));
        let mine = format!(".continuum/citizens/peers/{me}/notes");
        let theirs = format!(".continuum/citizens/peers/{other}/notes");
        for rel in ["Documents/letter.txt", ".config/htop/htoprc", ".cargo/registry", ".continuum/models", mine.as_str(), "Development/continuum/src/lib.rs"] {
            assert!(ok(rel), "hers: {rel}");
        }
        for rel in [".ssh/id_ed25519", ".aws/credentials", ".config/gh/hosts.yml", ".claude/credentials.json", ".cargo/credentials.toml", ".continuum/config.env", theirs.as_str(), "Development/continuum/.airc/identity.key"] {
            assert!(!ok(rel), "never: {rel}");
        }
        // The allow-only side: no granted root may CONTAIN a denial, or landlock would hand
        // it over. This is the assertion the single-carve-out defect failed.
        for granted in &list.write {
            assert!(!list.deny.iter().any(|d| d.starts_with(granted)), "{} holds a denial", granted.display());
        }
        assert!(list.listable.contains(&h), "her `ls ~` works");
        let p = seatbelt_host_profile(&list);
        assert!(p.starts_with("(version 1)\n(allow default)\n") && !p.contains("(allow file-write*"), "denials only, nothing to go stale:\n{p}");
        assert!(p.contains(&format!("(deny file-read* (subpath \"{}\"))", h.join(".ssh").display())), "{p}");
        assert!(p.contains("(deny file-read* (regex #\"/\\.airc/identity\\.key$\"))"), "{p}");
    }

    // what this catches: the seatbelt profile granting before it denies the home, or a
    // carve-out written before the grant of the root that held it (it would lose: last
    // match wins within an operation), or a granted root still containing a carve-out,
    // or a `file*` rule (which loses to any earlier specific allow).
    #[test]
    fn the_seatbelt_profile_denies_the_home_then_grants_then_carves_out_last() {
        let me = uuid::Uuid::from_u128(7);
        let home = home_with(&[".zshrc", ".cache/huggingface/token", ".cache/huggingface/hub/", &format!(".continuum/citizens/peers/{me}/workspace/")]);
        let h = home.path().canonicalize().expect("test: canonical");
        let ws = h.join(".continuum/citizens/peers").join(me.to_string()).join("workspace");
        let c = Confinement::for_caller(&h, &h.join(".continuum"), &me.to_string(), &ws);
        let list = c.materialize();
        let p = seatbelt_profile(&h, &list);
        let at = |needle: &str| p.find(needle).unwrap_or_else(|| panic!("profile lacks {needle}:\n{p}"));
        let deny_home = at(&format!("(deny file-read* (subpath \"{}\"))", h.display()));
        let read_zshrc = at(&format!("(allow file-read* (subpath \"{}\"))", h.join(".zshrc").display()));
        let grant_hub = at(&format!("(allow file-write* (subpath \"{}\"))", h.join(".cache/huggingface/hub").display()));
        let grant_ws = at(&format!("(allow file-write* (subpath \"{}\"))", ws.display()));
        let deny_token = at(&format!("(deny file-read* (subpath \"{}\"))", h.join(".cache/huggingface/token").display()));
        assert!(p.starts_with("(version 1)\n(allow default)\n(deny file-write* (subpath \"/\"))\n"), "writes are denied host-wide before anything is granted:\n{p}");
        let devnull = at("(allow file-write* (literal \"/dev/null\"))");
        assert!(devnull < deny_home && deny_home < read_zshrc && read_zshrc < grant_ws && grant_hub < deny_token, "{p}");
        assert!(!p.contains(&format!("(subpath \"{}\"))", h.join(".cache/huggingface").display())), "the root holding a carve-out is granted by its children, never whole:\n{p}");
        assert!(!list.write.iter().chain(list.read.iter()).any(|r| r.ends_with("huggingface")), "same on the allow-only side");
        assert!(!p.contains("file* "), "never the wildcard class: a deny in it loses to an earlier specific allow\n{p}");
    }

    // what this catches (Joel 2026-10-18 and 2026-10-24, kernel-level): a resident's real child
    // under the host profile refused the operator's ordinary files, a fresh directory, or
    // another citizen's workspace (the commons), or reading a credential, another citizen's
    // private state, or a scope's signing key. Real `sandbox-exec`.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn on_macos_a_residents_child_reaches_the_host_but_never_a_credential() {
        let me = uuid::Uuid::from_u128(7);
        let other = uuid::Uuid::from_u128(8);
        let home = home_with(&[
            ".ssh/id",
            ".config/gh/hosts.yml",
            "Documents/letter.txt",
            "Development/continuum/.airc/identity.key",
            &format!(".continuum/citizens/peers/{other}/notes"),
            &format!(".continuum/citizens/peers/{other}/workspace/repo/fix.txt"),
        ]);
        let h = home.path().canonicalize().expect("test: canonical");
        let c = Confinement::for_caller(&h, &h.join(".continuum"), &me.to_string(), &h.join("Development/continuum")).reaching_host();
        let leak = |rel: &str, tag: &str| format!("(cat '{}/{rel}' && echo LEAK-{tag}); ", h.display());
        let script = format!(
            "cat '{h}/Documents/letter.txt' && mkdir '{h}/Fresh' && echo made > '{h}/Fresh/new.txt' && cat '{h}/Fresh/new.txt' && ls '{h}' >/dev/null && echo LISTED; \
             cat '{h}/.continuum/citizens/peers/{other}/workspace/repo/fix.txt' && echo fixed >> '{h}/.continuum/citizens/peers/{other}/workspace/repo/fix.txt' && echo COMMONS; \
             {s}{g}{k}{o}(echo pwn >> '{h}/.ssh/id' && echo LEAK-WRITE); echo done",
            h = h.display(),
            s = leak(".ssh/id", "SSH"),
            g = leak(".config/gh/hosts.yml", "GH-TOKEN"),
            k = leak("Development/continuum/.airc/identity.key", "SCOPE-KEY"),
            o = leak(&format!(".continuum/citizens/peers/{other}/notes"), "OTHER-CITIZEN"),
            other = other,
        );
        let (mut cmd, receipt) = confined_command(&c, Path::new("/bin/sh"), &["-c".into(), script.into()], None, None);
        assert!(receipt.is_enforced(), "{receipt:?}");
        let out = cmd.output().await.expect("test: spawn");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stdout.contains("done") && stdout.contains("made") && stdout.contains("LISTED"), "the operator's files, a fresh dir, a home listing: stdout={stdout} stderr={stderr}");
        // 2026-10-24: another citizen's workspace is the commons (a reviewer runs its tests, a
        // colleague repairs it); her private notes beside it stay hers.
        assert!(stdout.contains("COMMONS"), "another citizen's workspace reads and writes: stdout={stdout} stderr={stderr}");
        for tag in ["SSH", "GH-TOKEN", "SCOPE-KEY", "OTHER-CITIZEN", "WRITE"] {
            assert!(!stdout.contains(&format!("LEAK-{tag}")), "{tag} leaked: stdout={stdout}");
        }
        assert_eq!(std::fs::read_to_string(h.join(".ssh/id")).unwrap(), "x", "the key is untouched");
        assert!(stderr.contains("Operation not permitted"), "the refusal is the OS's: stderr={stderr}");
    }

    // what this catches (the M5 incident, kernel-level): with the profile applied, a
    // child reading the operator's Pictures is refused by the OS, while her workspace
    // and a dotfile read fine. The enforcer is the real `sandbox-exec`, not a mock.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn on_macos_the_child_cannot_read_outside_her_roots() {
        let me = uuid::Uuid::from_u128(7);
        // Her workspace is a card worktree under `~/.airc` (Cormac on #4882): a dot entry
        // the policy never grants, so only the write-root-wins ordering lets her in.
        let home = home_with(&[
            ".zshrc",
            ".airc/worktrees/ab12cd34/hello.txt",
            ".airc/identity/key",
            ".config/gh/hosts.yml",
            ".cache/huggingface/token",
            ".cache/huggingface/hub/model.bin",
            &format!(".continuum/citizens/peers/{me}/experience.jsonl"),
            "Pictures/holiday.jpg",
            ".ssh/id",
        ]);
        let h = home.path().canonicalize().expect("test: canonical");
        let ws = h.join(".airc/worktrees/ab12cd34");
        let c = Confinement::for_caller(&h, &h.join(".continuum"), &me.to_string(), &ws);
        let leak = |rel: &str, tag: &str| format!("(cat '{}/{rel}' && echo LEAK-{tag}); ", h.display());
        let script = format!(
            "cat '{ws}/hello.txt' && cat '{h}/.zshrc' && cat '{h}/.cache/huggingface/hub/model.bin' && cat '{h}/.continuum/citizens/peers/{me}/experience.jsonl'; \
             {p}{s}{a}{g}{t}(ls '{h}' && echo LEAK-HOME); echo done",
            ws = ws.display(),
            h = h.display(),
            p = leak("Pictures/holiday.jpg", "PICTURES"),
            s = leak(".ssh/id", "SSH"),
            a = leak(".airc/identity/key", "AIRC-IDENTITY"),
            g = leak(".config/gh/hosts.yml", "GH-TOKEN"),
            t = leak(".cache/huggingface/token", "HF-TOKEN"),
        );
        // card 0e941074: outside the home only the write roots are writable. A host path she
        // has no root in is refused, while her workspace and /dev/null stay writable.
        let shared = std::path::PathBuf::from("/Users/Shared").join(format!("continuum-confinement-{}", uuid::Uuid::new_v4()));
        let script = format!(
            "{script}; (echo x > '{shared}' && echo LEAK-HOST-WRITE); echo ok > '{ws}/written' && echo hi > /dev/null && echo WROTE",
            shared = shared.display(),
            ws = ws.display(),
        );
        let (mut cmd, receipt) = confined_command(&c, Path::new("/bin/sh"), &["-c".into(), script.into()], None, None);
        assert!(receipt.is_enforced(), "{receipt:?}");
        let out = cmd.output().await.expect("test: spawn");
        let _ = std::fs::remove_file(&shared); // only present if the deny failed; the assertion below names it
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stdout.contains("done"), "stdout={stdout} stderr={stderr}");
        assert!(stdout.starts_with("xxxx"), "her worktree, her rc file, the model cache and her citizen dir read: stdout={stdout} stderr={stderr}");
        assert!(stdout.contains("WROTE"), "her workspace and /dev/null stay writable: stdout={stdout} stderr={stderr}");
        for tag in ["PICTURES", "SSH", "AIRC-IDENTITY", "GH-TOKEN", "HF-TOKEN", "HOME", "HOST-WRITE"] {
            assert!(!stdout.contains(&format!("LEAK-{tag}")), "{tag} leaked: stdout={stdout}");
        }
        assert!(stderr.contains("Operation not permitted"), "the refusal is the OS's: stderr={stderr}");
    }

    // what this catches (card d166e4f2, kernel-level): a resident's card checkout is a clone
    // that borrows objects from the node's mirror. Under the real `sandbox-exec` her git must
    // READ the borrowed objects (log of the base) and COMMIT into her own .git, while a write
    // into the shared mirror is refused by the OS. Without the read-only mirror root the log
    // fails; with a write grant the mirror could be corrupted for every other card.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn on_macos_a_card_clone_reads_its_mirror_and_commits_but_cannot_write_the_mirror() {
        let me = uuid::Uuid::from_u128(8);
        let home = home_with(&[".zshrc"]);
        let h = home.path().canonicalize().expect("test: canonical");
        let git = |dir: &Path, args: &[&str]| {
            let out = std::process::Command::new("git").current_dir(dir).args(args).output().expect("test: git");
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        };
        let seed = h.join("seed");
        std::fs::create_dir_all(&seed).expect("test: seed dir");
        git(&seed, &["init", "--quiet", "--initial-branch=canary"]);
        std::fs::write(seed.join("lib.rs"), "fn a() {}\n").expect("test: write");
        git(&seed, &["add", "."]);
        git(&seed, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "--quiet", "-m", "base"]);
        let mirror = h.join(".airc/mirrors/o/r.git");
        std::fs::create_dir_all(mirror.parent().expect("test: parent")).expect("test: mirrors dir");
        git(&h, &["clone", "--quiet", "--mirror", "--no-hardlinks", seed.to_str().expect("utf8"), mirror.to_str().expect("utf8")]);
        let ws = h.join(".airc/worktrees/cd34ef56");
        std::fs::create_dir_all(ws.parent().expect("test: parent")).expect("test: worktrees dir");
        git(&h, &["clone", "--quiet", "--shared", mirror.to_str().expect("utf8"), ws.to_str().expect("utf8")]);

        let c = Confinement::for_caller(&h, &h.join(".continuum"), &me.to_string(), &ws);
        let script = format!(
            "git -C '{ws}' log --format=BASE-%s -1 && \
             echo b >> '{ws}/lib.rs' && git -C '{ws}' -c user.email=t@t -c user.name=t commit --quiet -am mine && echo COMMITTED; \
             (touch '{mirror}/objects/forged' && echo LEAK-MIRROR-WRITE); echo done",
            ws = ws.display(),
            mirror = mirror.display(),
        );
        let (mut cmd, receipt) = confined_command(&c, Path::new("/bin/sh"), &["-c".into(), script.into()], None, None);
        assert!(receipt.is_enforced(), "{receipt:?}");
        let out = cmd.output().await.expect("test: spawn");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stdout.contains("BASE-base"), "her git reads the borrowed objects: stdout={stdout} stderr={stderr}");
        assert!(stdout.contains("COMMITTED"), "her commit lands in her own .git: stdout={stdout} stderr={stderr}");
        assert!(!stdout.contains("LEAK-MIRROR-WRITE"), "the shared mirror is read-only to her: stdout={stdout}");
        assert!(stderr.contains("Operation not permitted"), "the refusal is the OS's: stderr={stderr}");
        assert!(!mirror.join("objects/forged").exists());
    }
