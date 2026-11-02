#![cfg(test)]
    use super::*;
    use uuid::Uuid;

    fn persona() -> Uuid {
        Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap()
    }

    fn ctx() -> RagContext {
        RagContext::for_persona(persona(), 1_000_000)
    }

    fn layout(dirs: &[&str]) -> WorkspaceLayout {
        WorkspaceLayout {
            root: PathBuf::from("/repo"),
            repository: None,
            top_level_dirs: dirs.iter().map(|s| s.to_string()).collect(),
            // Default fixture: entries == dirs. The tests that care about the
            // difference (files-only, hidden-only) build the struct themselves.
            top_level_entries: dirs.len(),
        }
    }

    // what this catches (81097858): a shallow checkout on main being presented as
    // the allocated integration base, or absent legacy evidence being guessed.
    #[tokio::test]
    async fn checkout_observation_does_not_replace_allocation_intent() {
        let mut world = layout(&["core"]);
        world.repository = Some(RepositoryEvidence {
            origin_repo: Some(airc_work::RepoId::new("observed/project").expect("repo")),
            checkout: parse_checkout(&format!("true\n{}\nrefs/heads/main\n", "b".repeat(40))),
            allocation: Ok(Some(airc_lib::work_worktree::AllocationEvidence {
                repo: airc_work::RepoId::new("acme/project").expect("repo"),
                requested_branch: "task/implementation".into(),
                integration_base: Some("integration".into()),
                start_ref: "refs/remotes/origin/integration".into(),
                creation_sha: "a".repeat(40),
            })),
        });
        let source = WorkspaceMapSource::new(
            persona(),
            Arc::new(StubReader {
                layout: Ok(world.clone()),
            }),
        );
        let delivery = source
            .deliver(&ctx())
            .await;
        let body = &delivery.items[0].content;
        for fact in [
            "repo acme/project",
            "Observed origin repository: observed/project",
            "requested branch task/implementation",
            "integration base integration",
            "branch refs/heads/main",
            "shallow true",
            "does not prove a change is absent",
        ] {
            assert!(body.contains(fact), "missing {fact}: {body}");
        }
        let evidence = world.repository.as_mut().expect("repository fixture");
        evidence.allocation = Ok(None);
        assert!(render_repository(evidence).contains("current HEAD does not establish"));
        evidence.allocation = Err("unreadable allocation".into());
        assert!(render_repository(evidence).contains("could not be read: unreadable allocation"));
        assert!(
            parse_checkout("true\n").is_err(),
            "partial observations are not complete facts"
        );
    }

    // what this catches: the source telling a persona her workspace is EMPTY when it
    // is not. The old gate was `top_level_dirs.is_empty()` — a claim about DIRECTORIES
    // wearing the words "there are no files or directories here yet", plus a listing
    // that excluded dotfiles. Measured live 2026-08-06: a persona sent to fix
    // `.gymtool/solution.rs` (the shape EVERY tool-bugfix-rs task uses) was told, as a
    // structural fact, that there was nothing to read and that she should CREATE files
    // instead. She recovered by trusting the task text over her own grounding — but the
    // grounding was a lie we authored. [[empty-workspace-is-a-confabulation-not-infra]]
    #[test]
    fn a_workspace_with_files_is_never_described_as_empty() {
        // Only a dot-directory: zero non-hidden dirs, but emphatically not empty.
        let hidden_only = WorkspaceLayout {
            root: PathBuf::from("/repo"),
            repository: None,
            top_level_dirs: vec![".gymtool".into()],
            top_level_entries: 1,
        };
        assert!(!hidden_only.is_truly_empty());
        let body = render_layout(&hidden_only);
        assert!(!body.contains("It is EMPTY"), "{body}");
        assert!(
            !body.contains(
                "nothing \\
             to read"
            ),
            "must never tell her reading cannot work here: {body}"
        );

        // Files at the root, no subdirectories at all — the OTHER shape the old
        // dirs-only gate got wrong.
        let files_only = WorkspaceLayout {
            root: PathBuf::from("/repo"),
            repository: None,
            top_level_dirs: vec![],
            top_level_entries: 3,
        };
        assert!(!files_only.is_truly_empty());
        assert!(!render_layout(&files_only).contains("It is EMPTY"));

        // A genuinely empty workspace still gets the build-first grounding, which is
        // a real finding (#206) and must survive this fix.
        let truly_empty = WorkspaceLayout {
            root: PathBuf::from("/repo"),
            repository: None,
            top_level_dirs: vec![],
            top_level_entries: 0,
        };
        assert!(truly_empty.is_truly_empty());
        assert!(render_layout(&truly_empty).contains("It is EMPTY"));
    }

    struct StubReader {
        layout: Result<WorkspaceLayout, String>,
    }

    impl StubReader {
        fn ok(dirs: &[&str]) -> Self {
            Self {
                layout: Ok(layout(dirs)),
            }
        }
        fn err() -> Self {
            Self {
                layout: Err("disk gone".to_string()),
            }
        }
    }

    #[async_trait::async_trait]
    impl WorkspaceLayoutReader for StubReader {
        async fn layout(&self) -> Result<WorkspaceLayout, String> {
            self.layout.clone()
        }
    }

    // what this catches: the layout surfaces as a delivery the brain renders into
    // the [workspace-map] grounding block — and it names the REAL top-level dirs,
    // so a reasoner can avoid the `src/**/*.rs` guess that matched nothing.
    #[tokio::test]
    async fn layout_surfaces_with_real_dirs() {
        let reader = Arc::new(StubReader::ok(&["apps", "core", "docs", "src"]));
        let source = WorkspaceMapSource::new(persona(), reader);
        let delivery = source.deliver(&ctx()).await;
        assert_eq!(delivery.items.len(), 1);
        let content = &delivery.items[0].content;
        // Both `core` and `src` are present — the exact ambiguity that tripped
        // the eval. The map names both; it never says which holds the code.
        assert!(content.contains("core"));
        assert!(content.contains("src"));
        assert!(content.contains("/repo"));
        // The generic path-semantics rule that lets her avoid assuming `src/`.
        assert!(content.contains("**/"));
        assert_eq!(delivery.items[0].metadata["top_level_dirs"][1], "core");
        // Explicitly refutes the recalled "workspace is empty" confabulation with a
        // concrete count, so live ground truth beats a stale memory.
        assert!(
            content.contains("NOT empty"),
            "refutes the empty-belief: {content}"
        );
        assert!(
            content.contains("4 top-level directories"),
            "states the count: {content}"
        );
    }

    // what this catches: we do NOT steer — the block never tells her which
    // directory holds the source (no "Rust is under core/" prose). It states the
    // layout and the tool semantics; the reasoning is hers.
    #[tokio::test]
    async fn grounds_without_steering() {
        let reader = Arc::new(StubReader::ok(&["core", "src"]));
        let source = WorkspaceMapSource::new(persona(), reader);
        let delivery = source.deliver(&ctx()).await;
        let content = delivery.items[0].content.to_lowercase();
        assert!(
            !content.contains("rust is under") && !content.contains("rust lives"),
            "must not name where code lives — that is steering, not grounding: {content}"
        );
    }

    // what this catches: a layout read failure degrades to empty, never panics —
    // cognition stays up if the filesystem is momentarily unreadable.
    #[tokio::test]
    async fn read_error_returns_empty_no_panic() {
        let source = WorkspaceMapSource::new(persona(), Arc::new(StubReader::err()));
        let delivery = source.deliver(&ctx()).await;
        assert!(delivery.items.is_empty());
    }

    // what this catches: cross-persona ctx gets nothing (defense in depth).
    #[tokio::test]
    async fn cross_persona_ctx_returns_empty() {
        let source = WorkspaceMapSource::new(persona(), Arc::new(StubReader::ok(&["core"])));
        let alien = Uuid::parse_str("00000000-0000-0000-0000-000000000bbb").unwrap();
        let delivery = source
            .deliver(&RagContext::for_persona(alien, 1_000_000))
            .await;
        assert!(delivery.items.is_empty());
    }

    // what this catches: an empty workspace grounds WRITE-FIRST — it states the space
    // is empty, that reading/listing it finds nothing, and to CREATE with code/write.
    // This counters the native-tool discovery-loop (explore-a-void instead of building)
    // that scored a from-scratch web-dev task 0. Truthful (it IS empty) + non-steering
    // (never says WHAT to build). Regression for #206.
    #[tokio::test]
    async fn empty_layout_grounds_write_first() {
        let source = WorkspaceMapSource::new(persona(), Arc::new(StubReader::ok(&[])));
        let delivery = source.deliver(&ctx()).await;
        assert_eq!(delivery.items.len(), 1);
        let content = &delivery.items[0].content;
        assert!(content.contains("EMPTY"), "states it's empty: {content}");
        assert!(
            content.contains("code/write"),
            "points at creation, not exploration: {content}"
        );
        assert!(
            content.contains("nothing to read"),
            "explicitly counters the read-a-void reflex: {content}"
        );
        // Non-steering: it must not name WHAT to build.
        let lc = content.to_lowercase();
        assert!(
            !lc.contains("index.html") && !lc.contains("login") && !lc.contains("button"),
            "grounds the empty state, never dictates the artifact: {content}"
        );
    }

    // what this catches (card 0c42c0bf): a citizen who leased a port for her dev server not
    // seeing it in her grounding, so she guesses a port or re-leases every turn; and a line
    // that costs prompt tokens for a citizen who holds no leases.
    #[test]
    fn her_leased_ports_are_one_line_and_absent_when_she_has_none() {
        use crate::code::ports::PortLease;
        let lease = |service: &str, port| PortLease {
            base: crate::orm::entity::BaseEntity::for_new_record(),
            node: crate::identity::PeerId::from_uuid(persona()),
            holder: crate::identity::PeerId::from_uuid(persona()),
            service: service.into(),
            port,
            leased_at_ms: 1,
            released_at_ms: None,
        };
        assert_eq!(render_ports(&[]), None);
        let line =
            render_ports(&[lease("cw-db", 31_001), lease("cw-web", 31_000)]).expect("a line");
        assert!(
            line.contains("cw-db -> 31001") && line.contains("cw-web -> 31000"),
            "{line}"
        );
        assert!(!line.contains('\n'), "one line");
    }

    // what this catches: the map is delivered whole under the stable key "map", with the
    // root-line-plus-count summary and the code/tree pointer, and never a truncation
    // marker -- the view picks the level, the source never sizes.
    #[tokio::test]
    async fn the_map_is_delivered_whole_with_levels() {
        let source = WorkspaceMapSource::new(
            persona(),
            Arc::new(StubReader::ok(&["apps", "core", "docs", "src", "tools"])),
        );
        let delivery = source.deliver(&ctx()).await;
        let item = &delivery.items[0];
        assert_eq!(item.key, "map");
        assert!(item.content.contains("apps, core, docs, src, tools"));
        assert!(!item.content.contains("truncated"));
        let summary = item.summary.as_deref().expect("summary level");
        assert!(summary.contains("/repo") && summary.contains("5 top-level entries"), "{summary}");
        assert_eq!(item.pointer.as_deref(), Some("[workspace map: reopen with code/tree]"));
        assert!(!delivery.truncated);
    }

    // what this catches: the production reader reads our RECORDED checkout (never the
    // process cwd, card 2a2cd6f1) and returns a non-empty, sorted directory list — the
    // same machinery the code tools resolve against (drift-proofing).
    #[tokio::test]
    async fn own_checkout_reader_reads_real_workspace() {
        // its own home, under the env lock: the reader resolves the recorded checkout, and
        // recording it must never touch the developer's real ~/.continuum
        let home = tempfile::tempdir().expect("tempdir");
        let _home = crate::paths::test_home::HomeGuard::set(home.path()).await;
        let checkout = crate::code::repo_registry::record_own_checkout_for_test();
        let layout = OwnCheckoutWorkspaceLayoutReader::default()
            .layout()
            .await
            .expect("own checkout layout must read");
        assert_eq!(layout.root, checkout, "the map roots at the recorded checkout, never the cwd");
        assert!(
            !layout.top_level_dirs.is_empty(),
            "a real checkout has top-level directories"
        );
        let mut sorted = layout.top_level_dirs.clone();
        sorted.sort();
        assert_eq!(layout.top_level_dirs, sorted, "dirs must be sorted");
    }

    // what this catches: the fixed-root reader (eval workspace_root pin) roots the
    // MAP at the pinned dir — an EMPTY pinned dir renders write-first grounding, so a
    // from-scratch build task's map matches its clean hands instead of describing the
    // repo (#206). Regression for the map-follows-hands link.
    #[tokio::test]
    async fn fixed_root_empty_dir_grounds_write_first() {
        let dir = tempfile::tempdir().expect("tempdir");
        let reader = FixedRootWorkspaceLayoutReader::new(dir.path());
        let layout = reader.layout().await.expect("empty dir layout reads");
        assert!(
            layout.repository.is_none(),
            "plain workspaces have no Git error grounding"
        );
        assert!(
            layout.top_level_dirs.is_empty(),
            "a fresh temp dir has no subdirs"
        );
        let body = render_layout(&layout);
        assert!(
            body.contains("EMPTY"),
            "empty pinned root grounds write-first: {body}"
        );
        assert!(body.contains("code/write"), "points at creation: {body}");
        std::fs::write(dir.path().join(".git"), "broken git marker").expect("broken marker");
        let broken = reader.layout().await.expect("layout remains readable");
        assert!(broken
            .repository
            .expect("broken repository is not absence")
            .checkout
            .is_err());
    }

    // what this catches (5039 review): a receipt memo surviving a different blob
    // or creation anchor, and malformed evidence being downgraded to absence.
    #[tokio::test]
    async fn allocation_memo_tracks_immutable_identity_and_preserves_errors() {
        use crate::code::git_bridge::run_git;
        use airc_lib::work_worktree::{AllocationEvidence, ALLOCATION_REF, CREATION_BASE_REF};
        let dir = tempfile::tempdir().expect("repo");
        let root = dir.path();
        run_git(root, &["init"]).await.expect("init");
        let nested = root.join("plain-workspace");
        std::fs::create_dir(&nested).expect("nested plain workspace");
        let nested_layout = FixedRootWorkspaceLayoutReader::new(&nested)
            .layout()
            .await
            .expect("nested layout");
        assert!(
            nested_layout.repository.is_none(),
            "the owned Git runner cannot inherit the parent checkout across its discovery ceiling"
        );
        run_git(
            root,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.test",
                "commit",
                "--allow-empty",
                "-m",
                "initial",
            ],
        )
        .await
        .expect("commit");
        let creation = run_git(root, &["rev-parse", "HEAD"]).await.expect("HEAD");
        let creation = creation.trim();
        run_git(root, &["update-ref", CREATION_BASE_REF, creation])
            .await
            .expect("anchor");
        let reader = LayoutReader::default();
        // Regression 2be63714: legacy allocation absence must not hide the real
        // checkout identity or expose credentials when grounding that identity.
        run_git(
            root,
            &[
                "remote",
                "add",
                "origin",
                "https://user:secret@example.test/acme/actual.git?token=secret",
            ],
        )
        .await
        .expect("origin");
        let legacy = reader
            .read_layout(root.to_path_buf())
            .await
            .expect("layout");
        let repository = legacy.repository.expect("repository");
        assert_eq!(
            repository.origin_repo.as_ref().map(|repo| repo.as_str()),
            Some("acme/actual")
        );
        let rendered = layout_reader::render_repository(&repository);
        assert!(rendered.contains("Allocation evidence is unavailable"));
        assert!(rendered.contains("Observed origin repository: acme/actual"));
        assert!(!rendered.contains("secret"));
        run_git(
            root,
            &[
                "remote",
                "set-url",
                "origin",
                "git@example.test:other/project.git",
            ],
        )
        .await
        .expect("changed origin");
        let changed = reader
            .read_layout(root.to_path_buf())
            .await
            .expect("changed layout");
        assert_eq!(
            changed
                .repository
                .expect("repository")
                .origin_repo
                .expect("origin")
                .as_str(),
            "other/project"
        );
        run_git(root, &["remote", "set-url", "origin", "../local/project.git"])
            .await.expect("local origin");
        assert!(reader.read_layout(root.to_path_buf()).await.expect("local layout")
            .repository.expect("repository").origin_repo.is_none());
        run_git(root, &["remote", "remove", "origin"]).await.expect("remove origin");
        assert!(reader.read_layout(root.to_path_buf()).await.expect("no origin layout")
            .repository.expect("repository").origin_repo.is_none());
        assert!(reader
            .read_allocation(root)
            .await
            .expect("legacy")
            .is_none());
        for branch in ["first", "replacement"] {
            let evidence = AllocationEvidence {
                repo: airc_work::RepoId::new("acme/project").expect("repo"),
                requested_branch: branch.into(),
                integration_base: Some("integration".into()),
                start_ref: "refs/remotes/origin/integration".into(),
                creation_sha: creation.into(),
            };
            let bytes = serde_json::to_vec(&evidence).expect("allocation JSON fixture"); // Git blob persistence boundary, matching AIRC allocation receipt format
            std::fs::write(root.join("receipt.json"), bytes).expect("receipt");
            let oid = run_git(root, &["hash-object", "-w", "receipt.json"])
                .await
                .expect("blob");
            run_git(root, &["update-ref", ALLOCATION_REF, oid.trim()])
                .await
                .expect("receipt ref");
            for _ in 0..2 {
                assert_eq!(
                    reader.read_allocation(root).await.expect("receipt"),
                    Some(evidence.clone())
                );
            }
        }
        run_git(root, &["update-ref", "-d", CREATION_BASE_REF])
            .await
            .expect("remove anchor");
        assert!(
            reader.read_allocation(root).await.is_err(),
            "cached receipt cannot hide a missing anchor"
        );
        run_git(root, &["update-ref", CREATION_BASE_REF, creation])
            .await
            .expect("restore anchor");
        std::fs::write(root.join("receipt.json"), "not JSON").expect("malformed receipt");
        let malformed = run_git(root, &["hash-object", "-w", "receipt.json"])
            .await
            .expect("malformed blob");
        run_git(root, &["update-ref", ALLOCATION_REF, malformed.trim()])
            .await
            .expect("malformed ref");
        assert!(
            reader.read_allocation(root).await.is_err(),
            "malformed receipt is an error, not cached success or absence"
        );
        run_git(root, &["update-ref", "-d", ALLOCATION_REF])
            .await
            .expect("remove receipt");
        assert!(reader
            .read_allocation(root)
            .await
            .expect("legacy again")
            .is_none());
    }

    // what this catches: a NON-empty pinned root (a SWE-bench repo clone) lists its
    // real dirs — the map follows the pinned hands, correct for the existing
    // workspace_root caller too, not just the clean-build case.
    #[tokio::test]
    async fn fixed_root_with_dirs_lists_them() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::create_dir(dir.path().join("tests")).unwrap();
        let layout = FixedRootWorkspaceLayoutReader::new(dir.path())
            .layout()
            .await
            .expect("layout reads");
        assert_eq!(
            layout.top_level_dirs,
            vec!["src".to_string(), "tests".to_string()]
        );
        let body = render_layout(&layout);
        assert!(
            body.contains("src") && body.contains("tests"),
            "lists real dirs: {body}"
        );
        assert!(
            body.contains("NOT empty"),
            "refutes the empty-belief: {body}"
        );
    }

    // what this catches: the citizen-layer reader roots at the persona's OWN
    // workspace when it exists (the #49 swap — map root == the tools' root), and
    // falls back to the shared base (cwd) when the layer isn't provisioned yet —
    // truthful either way, never empty, never a panic. Here the peer has no layer
    // (random id under a temp home), so the fallback path must yield the real cwd
    // layout rather than erroring.
    #[tokio::test]
    async fn citizen_layer_reader_provisions_and_tells_the_real_root() {
        // what this catches (#49, the mixed-truth window): the map must NEVER
        // describe the shared cwd while the tools act in the peer's layer. The
        // reader now PROVISIONS the layer (same ensure the hands use) and
        // reports THAT root — so root != cwd, and the layout it lists is the
        // layer's own (a CoW clone of the base, hence non-empty). The old
        // fallback-to-cwd behavior this replaces produced the live 2026-08-02
        // "I see it / mine is empty" roommate contradictions.
        let peer = "00000000-0000-0000-0000-0000000000ff";
        let reader = CitizenLayerWorkspaceLayoutReader {
            peer: peer.to_string(),
            reader: LayoutReader::default(),
        };
        // OWN HOME, UNDER THE ENV LOCK (CI red on #4062 and #4078, 2026-09-15): the
        // layer is provisioned under `$HOME/.continuum/citizens/peers/<peer>`, and
        // this test read whatever HOME a concurrent `with_test_home` caller had set —
        // a tempdir that vanished mid-clone ("could not open …/objects/pack/tmp_pack_…
        // No such file"). Holding the same lock with a home of its own makes the
        // clone land in a dir this test keeps alive, and never in the developer's real
        // `~/.continuum/citizens/`.
        let home = tempfile::tempdir().expect("tempdir");
        let _home = crate::paths::test_home::HomeGuard::set(home.path()).await;
        crate::code::repo_registry::record_own_checkout_for_test(); // the shared base the layer clones from
        let layout = reader.layout().await.expect("reader provisions the layer");
        let cwd = std::env::current_dir().expect("cwd");
        assert_ne!(
            layout.root, cwd,
            "map root must be the peer's OWN layer, never the shared cwd"
        );
        assert!(
            layout.root.to_string_lossy().contains(peer),
            "root is the peer's citizen layer: {}",
            layout.root.display()
        );
        assert!(
            !layout.top_level_dirs.is_empty(),
            "the provisioned layer clones the base layout — never empty"
        );
        // The throwaway peer's layer lives under this test's temp home; `home` drops it.
        assert!(
            layout.root.starts_with(home.path()),
            "the layer was provisioned under the test's own home: {}",
            layout.root.display()
        );
    }
