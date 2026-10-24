#![cfg(test)]
    use super::*;

    // what this catches (Astra on #5026): a claim's close dropping a checkout's sync lock while
    // a sync still holds it, so a new claim's sync on the same
    // checkout ran beside it. The lock leaves only when no sync holds it.
    #[tokio::test]
    async fn a_checkout_lock_held_by_a_running_sync_is_never_dropped() {
        let root = std::path::PathBuf::from(format!("/checkout-{}", Uuid::new_v4()));
        let running = CHECKOUT_SYNC.entry(root.clone()).or_default().clone(); // the timed-out sync's clone
        forget_checkout(&root);
        let next = CHECKOUT_SYNC.entry(root.clone()).or_default().clone();
        assert!(std::sync::Arc::ptr_eq(&running, &next), "a new claim's sync takes the SAME lock the running one holds");
        drop((running, next));
        forget_checkout(&root);
        assert!(!CHECKOUT_SYNC.contains_key(&root), "once nothing holds it, the lock leaves with the claim");
    }

    async fn git(dir: &Path, args: &[&str]) -> String {
        run_git(dir, args).await.unwrap_or_else(|e| panic!("git {args:?} in {}: {e}", dir.display()))
    }
    async fn head(dir: &Path) -> String {
        git(dir, &["rev-parse", "HEAD"]).await.trim().to_string()
    }
    async fn identity(dir: &Path) {
        git(dir, &["config", "user.email", "t@t"]).await;
        git(dir, &["config", "user.name", "t"]).await;
    }

    /// The act-end sync's MECHANISM, with the carry decision granted. An offline fixture's
    /// remote is a local bare repo — exactly what the POLICY refuses — so the two are
    /// tested apart: the policy in
    /// `a_push_only_transfers_when_the_other_node_can_read_its_target`, the mechanism here.
    async fn sync(root: &Path, card: Uuid, node: &str) -> PushOutcome {
        sync_after_act_over(root, card, node, Carry::Via("https://origin.test/repo.git".into()), None, CommitPolicy::TurnEnd).await
    }

    /// A bare `origin` and two clones — machine A and machine B — sharing one `main`
    /// commit; A stands on the card's branch, cut from main, the way `ensure_worktree`
    /// cuts it from the clone's HEAD.
    struct TwoMachines {
        tmp: tempfile::TempDir,
        origin: PathBuf,
        a: PathBuf,
        b: PathBuf,
        branch: String,
    }

    async fn two_machines() -> TwoMachines {
        let tmp = tempfile::tempdir().expect("tempdir");
        let origin = tmp.path().join("origin.git");
        let a = tmp.path().join("machine-a");
        let b = tmp.path().join("machine-b");
        git(tmp.path(), &["init", "-q", "--bare", "-b", "main", &origin.to_string_lossy()]).await;
        git(tmp.path(), &["init", "-q", "-b", "main", &a.to_string_lossy()]).await;
        identity(&a).await;
        std::fs::write(a.join("README"), "shared\n").unwrap();
        git(&a, &["add", "README"]).await;
        git(&a, &["commit", "-q", "-m", "base"]).await;
        git(&a, &["remote", "add", "origin", &origin.to_string_lossy()]).await;
        git(&a, &["push", "-q", "-u", "origin", "main"]).await;
        git(tmp.path(), &["clone", "-q", &origin.to_string_lossy(), &b.to_string_lossy()]).await;
        identity(&b).await;
        let branch = "73eefbbb/workspace-moves-with-the-mind".to_string();
        git(&a, &["checkout", "-q", "-b", &branch]).await;
        TwoMachines { tmp, origin, a, b, branch }
    }

    // what this catches (card 73eefbbb): the transfer itself. Machine A's act leaves an
    // edit; the act-end sync WIP-commits and pushes the card branch; machine B stages
    // the card (a branch cut from ITS clone's HEAD, the pre-fix state) and arrives:
    // fetched, checked out at A's sha, tree equal to A's, the from-node read off the
    // commit. Before this nothing pushed and B's branch stayed at the shared base.
    #[tokio::test]
    async fn machine_b_arrives_at_the_sha_machine_a_pushed_with_the_same_tree() {
        let m = two_machines().await;
        let card = Uuid::new_v4();
        std::fs::write(m.a.join("src.txt"), "edit from A\n").unwrap();
        assert!(has_unpushed_work(&m.a).await, "a dirty tree is unpushed work");
        let out = sync(&m.a, card, "node-a").await;
        let sha = match &out {
            PushOutcome::Ok { sha, committed: true, pushed: true, branch } => {
                assert_eq!(*branch, m.branch);
                sha.clone()
            }
            other => panic!("expected a committed push, got {other:?}"),
        };
        assert!(!has_unpushed_work(&m.a).await, "pushed = nothing unpushed");
        assert_eq!(git(&m.origin, &["rev-parse", &format!("refs/heads/{}", m.branch)]).await.trim(), sha, "origin holds the branch at her sha");
        assert!(
            matches!(sync(&m.a, card, "node-a").await, PushOutcome::Ok { committed: false, pushed: false, .. }),
            "nothing new = nothing committed, nothing pushed"
        );

        git(&m.b, &["checkout", "-q", "-B", &m.branch]).await;
        let arrived = arrive_over(&m.b, &m.branch, 7).await;
        match &arrived {
            ArrivalOutcome::Transferred { sha: s, stranded: None, from_node: Some(n), .. } => {
                assert_eq!(*s, sha);
                assert_eq!(n, "node-a", "the WIP commit names the node it came from");
            }
            other => panic!("expected a clean transfer, got {other:?}"),
        }
        assert_eq!(head(&m.b).await, sha);
        assert_eq!(std::fs::read_to_string(m.b.join("src.txt")).unwrap(), "edit from A\n", "B's tree is A's");
        assert_eq!(git(&m.b, &["status", "--porcelain"]).await.trim(), "");
        assert!(matches!(arrive_over(&m.b, &m.branch, 8).await, ArrivalOutcome::Current { .. }), "a second arrival moves nothing");
        assert!(!has_unpushed_work(&m.b).await, "at origin's tip: nothing to carry");
    }

    // what this catches: another origin branch containing her commit does not deliver
    // it to the card branch that the receiving node fetches (nor release placement).
    #[tokio::test]
    async fn publication_and_placement_follow_the_exact_card_branch() {
        let m = two_machines().await;
        let (card, persona) = (Uuid::new_v4(), Uuid::new_v4());
        note_acted_root(persona, m.a.clone());
        // The new card branch is absent, although its base already exists on main.
        assert!(move_blocker_over(persona, None).await.is_some());
        assert!(matches!(sync(&m.a, card, "node-a").await, PushOutcome::Ok { committed: false, pushed: true, .. }));
        let base = head(&m.a).await;
        assert_eq!(git(&m.origin, &["rev-parse", &format!("refs/heads/{}", m.branch)]).await.trim(), base);
        assert!(move_blocker_over(persona, None).await.is_none());

        std::fs::write(m.a.join("review.txt"), "reviewed successor\n").unwrap();
        git(&m.a, &["add", "review.txt"]).await;
        git(&m.a, &["commit", "-q", "-m", "reviewed successor"]).await;
        let successor = head(&m.a).await;
        git(&m.a, &["push", "-q", "origin", "HEAD:refs/heads/other"]).await;
        assert_eq!(git(&m.a, &["rev-parse", "refs/remotes/origin/other"]).await.trim(), successor);
        assert_eq!(git(&m.origin, &["rev-parse", &format!("refs/heads/{}", m.branch)]).await.trim(), base);
        assert!(move_blocker_over(persona, None).await.is_some(), "another branch is not card delivery");
        assert!(matches!(sync(&m.a, card, "node-a").await, PushOutcome::Ok { committed: false, pushed: true, .. }));
        assert!(move_blocker_over(persona, None).await.is_none(), "card publication releases placement");
        assert_eq!(git(&m.origin, &["rev-parse", &format!("refs/heads/{}", m.branch)]).await.trim(), successor);
        git(&m.b, &["checkout", "-q", "-B", &m.branch]).await;
        assert!(matches!(arrive_over(&m.b, &m.branch, 40).await, ArrivalOutcome::Transferred { .. }));
        assert_eq!(head(&m.b).await, successor);
        assert_eq!(std::fs::read_to_string(m.b.join("review.txt")).unwrap(), "reviewed successor\n");
    }

    // what this catches: a push that cannot reach origin is a NAMED outcome, the work is
    // committed locally (nothing lost), and the placement question answers "blocked"
    // until a later act's sync lands the push — never a panic, never a silent move.
    #[tokio::test]
    async fn an_unreachable_remote_is_named_and_defers_the_move_until_the_push_lands() {
        let m = two_machines().await;
        let (card, persona) = (Uuid::new_v4(), Uuid::new_v4());
        std::fs::write(m.a.join("x.txt"), "x\n").unwrap();
        let gone = m.tmp.path().join("origin.gone");
        std::fs::rename(&m.origin, &gone).unwrap();
        let out = sync(&m.a, card, "node-a").await;
        assert!(matches!(out, PushOutcome::Unreachable { .. }), "{out:?}");
        assert_eq!(out.label(), "unreachable");
        // a push git refused doesn't prove the remote unchanged (Codex reviewer on #5037)
        assert!(REMOTE_UNKNOWN.contains(&m.a), "the remote stays unknown after a refused push");
        assert_eq!(git(&m.a, &["status", "--porcelain"]).await.trim(), "", "the WIP commit landed locally");
        assert!(has_unpushed_work(&m.a).await, "a local-only commit is unpushed work");
        note_acted_root(persona, m.a.clone());
        assert!(persona_has_unpushed_work(persona).await);
        let why = move_blocker_over(persona, None).await.expect("blocked");
        assert!(why.contains("unpushed work at"), "{why}");
        assert!(
            move_blocker_over(persona, Some(m.a.clone())).await.expect("blocked").starts_with("turn in flight"),
            "a turn in flight blocks before the git question is even asked"
        );
        std::fs::rename(&gone, &m.origin).unwrap();
        let out = sync(&m.a, card, "node-a").await;
        assert!(matches!(out, PushOutcome::Ok { committed: false, pushed: true, .. }), "{out:?}");
        assert!(!REMOTE_UNKNOWN.contains(&m.a), "the next sync read the remote, and its push settled it");
        assert!(move_blocker_over(persona, None).await.is_none(), "the push landed: free to move");
        // A vanished root is pruned, not a permanent blocker.
        note_acted_root(persona, m.tmp.path().join("never-existed"));
        assert!(move_blocker_over(persona, None).await.is_none());
    }

    // what this catches (Kimi card 74ec9613): re-claiming reset her reviewed successor
    // commit to an older origin. Repeated arrival preserves both committed and dirty work.
    #[tokio::test]
    async fn repeated_arrival_preserves_same_branch_successors_and_dirty_work() {
        let m = two_machines().await;
        let card = Uuid::new_v4();
        std::fs::write(m.a.join("src.txt"), "published base\n").unwrap();
        let PushOutcome::Ok { sha: remote_sha, .. } = sync(&m.a, card, "node-a").await else { panic!("push") };
        std::fs::write(m.a.join("src.txt"), "review corrections\n").unwrap();
        git(&m.a, &["add", "src.txt"]).await;
        git(&m.a, &["commit", "-q", "-m", "independent review corrections"]).await;
        let successor = head(&m.a).await;
        for now in [50, 51] {
            assert_eq!(arrive_over(&m.a, &m.branch, now).await, ArrivalOutcome::LocalWorkPreserved {
                branch: m.branch.clone(), sha: successor.clone(), remote_sha: remote_sha.clone(),
                local_commits: 1, dirty: false,
            });
            assert_eq!(head(&m.a).await, successor);
            assert_eq!(std::fs::read_to_string(m.a.join("src.txt")).unwrap(), "review corrections\n");
        }
        assert!(git(&m.a, &["for-each-ref", "--format=%(refname)", STRANDED_REF_PREFIX]).await.trim().is_empty());
        assert!(sync(&m.a, card, "node-a").await.is_ok());
        // Equal HEAD/origin with staged, unstaged and untracked edits: preserve all three.
        std::fs::write(m.a.join("src.txt"), "staged corrections\n").unwrap();
        git(&m.a, &["add", "src.txt"]).await;
        std::fs::write(m.a.join("src.txt"), "continued corrections\n").unwrap();
        std::fs::write(m.a.join("untracked.txt"), "new work\n").unwrap();
        let index = git(&m.a, &["write-tree"]).await;
        let status = git(&m.a, &["status", "--porcelain", "--untracked-files=all"]).await;
        for now in [52, 53] {
            assert_eq!(arrive_over(&m.a, &m.branch, now).await, ArrivalOutcome::LocalWorkPreserved {
                branch: m.branch.clone(), sha: successor.clone(), remote_sha: successor.clone(),
                local_commits: 0, dirty: true,
            });
            assert_eq!(head(&m.a).await, successor);
            assert_eq!(git(&m.a, &["write-tree"]).await, index);
            assert_eq!(git(&m.a, &["status", "--porcelain", "--untracked-files=all"]).await, status);
            assert_eq!(std::fs::read_to_string(m.a.join("src.txt")).unwrap(), "continued corrections\n");
            assert_eq!(std::fs::read_to_string(m.a.join("untracked.txt")).unwrap(), "new work\n");
        }
        assert!(git(&m.a, &["for-each-ref", "--format=%(refname)", STRANDED_REF_PREFIX]).await.trim().is_empty());
    }

    // what this catches: a local checkout that DIVERGED from what origin holds (its own
    // commit and a dirty edit) is kept whole under the stranded ref and the remote wins
    // the branch and the tree — never a silent discard, never a guess between the two.
    // Also: a push origin refuses (non-fast-forward) reads `rejected`, not `unreachable`.
    #[tokio::test]
    async fn a_diverged_local_checkout_is_stranded_under_a_ref_and_the_remote_wins() {
        let m = two_machines().await;
        let card = Uuid::new_v4();
        std::fs::write(m.a.join("src.txt"), "from A\n").unwrap();
        let PushOutcome::Ok { sha: sha_a, .. } = sync(&m.a, card, "node-a").await else { panic!("push") };
        // B: the same branch cut from main, its own commit Y, and an uncommitted edit.
        git(&m.b, &["checkout", "-q", "-B", &m.branch]).await;
        std::fs::write(m.b.join("y.txt"), "B's commit\n").unwrap();
        git(&m.b, &["add", "y.txt"]).await;
        git(&m.b, &["commit", "-q", "-m", "B's own"]).await;
        let y = head(&m.b).await;
        std::fs::write(m.b.join("z.txt"), "B's dirty edit\n").unwrap();
        let arrived = arrive_over(&m.b, &m.branch, 42).await;
        let ArrivalOutcome::Transferred { sha, stranded: Some(reference), .. } = &arrived else {
            panic!("expected a transfer with a stranded ref, got {arrived:?}");
        };
        assert_eq!(*sha, sha_a);
        assert_eq!(*reference, format!("{STRANDED_REF_PREFIX}{}-42", m.branch));
        let kept = git(&m.b, &["rev-parse", reference]).await.trim().to_string();
        assert_eq!(git(&m.b, &["rev-parse", &format!("{reference}^")]).await.trim(), y, "the stranded commit sits on B's own commit");
        assert_eq!(git(&m.b, &["show", &format!("{kept}:z.txt")]).await, "B's dirty edit\n", "the dirty edit is inside it");
        assert_eq!(head(&m.b).await, sha_a, "the remote won the branch");
        assert!(!m.b.join("y.txt").exists() && !m.b.join("z.txt").exists(), "…and the tree");
        assert_eq!(std::fs::read_to_string(m.b.join("src.txt")).unwrap(), "from A\n");
        assert_eq!(git(&m.b, &["status", "--porcelain"]).await.trim(), "");
        // A clean checkout that is merely BEHIND strands nothing.
        std::fs::write(m.a.join("src.txt"), "from A again\n").unwrap();
        let PushOutcome::Ok { sha: sha_a2, .. } = sync(&m.a, card, "node-a").await else { panic!("push") };
        match arrive_over(&m.b, &m.branch, 43).await {
            ArrivalOutcome::Transferred { sha, stranded: None, .. } => assert_eq!(sha, sha_a2),
            other => panic!("behind, clean: a plain fast-forward, got {other:?}"),
        }
        // Rejected: B commits on the tip, A pushes first, B's push is a non-fast-forward.
        std::fs::write(m.a.join("src.txt"), "A wins the race\n").unwrap();
        assert!(sync(&m.a, card, "node-a").await.is_ok());
        std::fs::write(m.b.join("late.txt"), "late\n").unwrap();
        let out = sync(&m.b, card, "node-b").await;
        assert!(matches!(out, PushOutcome::Rejected { .. }), "{out:?}");
        // The next arrival on B strands the rejected commit and takes A's tip.
        assert!(matches!(arrive_over(&m.b, &m.branch, 44).await, ArrivalOutcome::Transferred { stranded: Some(_), .. }));
    }

    // what this catches: the two pure readers. The push-error classifier must not call a
    // refused push "unreachable" (a move would wait on a network that is fine) nor a dead
    // network "rejected"; the WIP subject parser is the only source of the from-node.
    #[tokio::test]
    async fn push_errors_classify_and_the_wip_subject_names_its_node() {
        assert_eq!(classify_push_error("! [rejected] x -> x (fetch first)"), PushFailure::Rejected);
        assert_eq!(classify_push_error("remote: error: GH006: Protected branch update failed"), PushFailure::Rejected);
        assert_eq!(classify_push_error("fatal: Could not read from remote repository."), PushFailure::Unreachable);
        assert_eq!(classify_push_error("ssh: Could not resolve hostname github.com"), PushFailure::Unreachable);
        assert_eq!(classify_push_error("something new"), PushFailure::Unreachable, "unknown reads as not delivered");
        let card = Uuid::new_v4();
        assert_eq!(node_of_wip_subject(&wip_message(card, "node-a", None)).as_deref(), Some("node-a"));
        assert!(wip_message(card, "n", None).starts_with(&format!("wip({})", airc_core::shown_id::shown_form(card))));
        // what this catches (card e5e72485): her act commits carried no trailer, so her
        // authorship of her own work had to be added by hand. Credited, and the node still
        // reads back from the subject line.
        let kimi = Uuid::from_u128(0xe2f0);
        let credited = wip_message(card, "node-a", Some((kimi, "Kimi")));
        assert!(credited.ends_with(&format!("Co-Authored-By: Kimi <{kimi}@airc.citizen>")), "{credited}");
        assert_eq!(node_of_wip_subject(&credited).as_deref(), Some("node-a"));
        assert_eq!(node_of_wip_subject("fix: the printer"), None);
        // Not pushable is said, not guessed.
        let m = two_machines().await;
        git(&m.a, &["checkout", "-q", "--detach"]).await;
        assert_eq!(sync(&m.a, card, "n").await, PushOutcome::NotPushable { reason: DETACHED_REASON });
        assert!(!has_unpushed_work(&m.a).await, "a detached tree has no branch of hers, so it never defers");
        assert!(matches!(arrive_over(&m.b, "no/such-branch", 1).await, ArrivalOutcome::NoRemoteBranch { .. }));
    }

    // what this catches (Cormac's condition on #4278 — the defect that would have stranded
    // her acts under a receipt saying "carried"): a staged SWE-bench copy's `origin` is
    // THIS NODE'S OWN local mirror (`clone_at`: `git clone --shared <cache>/swe/mirrors/…`).
    // A push there reports success into a repository no other node has ever heard of — the
    // blocker clears, she moves, and the arriving node's fetch finds nothing. So a
    // local-path origin is NOT a transfer target: never pushed, never committed into, and
    // the work still sitting there keeps `has_unpushed_work` TRUE so the move defers and
    // she is pinned. The https/ssh case (a continuum card's worktree) must stay pushable,
    // or this test would also pass with the transfer simply switched off.
    #[tokio::test]
    async fn a_push_only_transfers_when_the_other_node_can_read_its_target() {
        for shared in [
            "https://github.com/CambrianTech/continuum.git",
            "http://gitlab.example.com/acme/widget",
            "ssh://git@github.com/CambrianTech/continuum.git",
            "git@github.com:CambrianTech/continuum.git",
            "git://example.com/repo.git",
        ] {
            assert!(is_transfer_target(shared), "{shared} is readable from another node");
        }
        for local in [
            "/Users/someone/.continuum/cache/swe/mirrors/django__django", // what clone_at clones from
            "/srv/repos/continuum.git",
            "../sibling-clone",
            "./mirror.git",
            "~/.continuum/cache/swe/mirrors/astropy__astropy",
            "file:///Users/someone/.continuum/cache/swe/mirrors/sympy__sympy",
            "C:\\repos\\continuum",
            "C:/repos/continuum",
            "mirrors/psf__requests",
            "",
            "   ",
        ] {
            assert!(!is_transfer_target(local), "{local:?} never leaves this machine");
        }

        // The live shape, built the way `swe_bench::clone_at` builds it: a bare local
        // mirror, then a `--shared` clone of it. Her act leaves an edit there.
        let m = two_machines().await;
        let mirror = m.tmp.path().join("mirror.git");
        let staged = m.tmp.path().join("staged-copy");
        git(m.tmp.path(), &["clone", "-q", "--bare", &m.a.to_string_lossy(), &mirror.to_string_lossy()]).await;
        git(m.tmp.path(), &["clone", "-q", "--shared", &mirror.to_string_lossy(), &staged.to_string_lossy()]).await;
        identity(&staged).await;
        // `-B`, not `-b`: `clone --bare` copies the source's HEAD symref, so a `--shared`
        // clone of the mirror already SITS on the card branch and `-b` would fail here.
        git(&staged, &["checkout", "-q", "-B", &m.branch]).await;
        assert_eq!(carry_of(&staged).await, Carry::LocalOrigin, "a --shared clone of a local mirror carries nothing");
        std::fs::write(staged.join("work.txt"), "her act\n").unwrap();

        let (card, persona) = (Uuid::new_v4(), Uuid::new_v4());
        let out = sync_after_act(&staged, card, "node-a", None).await;
        assert_eq!(out, PushOutcome::NotPushable { reason: LOCAL_ORIGIN_REASON }, "never a push into this node's own cache");
        assert_eq!(out.label(), "not_pushable");
        assert!(out.pins_the_mind(), "work that cannot be carried must pin her");
        assert_eq!(git(&staged, &["status", "--porcelain"]).await.trim(), "?? work.txt", "left exactly as her act left it");
        assert!(has_unpushed_work(&staged).await, "the work is real and no push can move it");
        note_acted_root(persona, staged.clone());
        let why = move_blocker_over(persona, None).await.expect("a move would strand her work");
        assert!(why.contains(&staged.display().to_string()), "{why}");
        // …and a fetch there cannot deliver another node's work either.
        assert!(matches!(arrive(&staged, &m.branch, 1).await, ArrivalOutcome::NotTransferable { .. }));

        // The positive control on the POLICY: a checkout whose origin is the grid-owned
        // remote (a continuum card's worktree — Cormac: "right as written") carries. Without
        // this the test would also pass with the transfer simply switched off.
        let card_worktree = m.tmp.path().join("card-worktree");
        git(m.tmp.path(), &["init", "-q", "-b", "main", &card_worktree.to_string_lossy()]).await;
        identity(&card_worktree).await;
        // A root commit, for the same reason the orphan below needs one: an unborn branch
        // is no branch, and `carry_of` would answer `Nothing`, not `Via` — which would
        // make this positive control assert the wrong thing for the wrong reason.
        std::fs::write(card_worktree.join("seed.txt"), "seed\n").unwrap();
        git(&card_worktree, &["add", "seed.txt"]).await;
        git(&card_worktree, &["commit", "-q", "-m", "workspace: initial state"]).await;
        git(&card_worktree, &["remote", "add", "origin", "https://github.com/CambrianTech/continuum.git"]).await;
        assert_eq!(
            carry_of(&card_worktree).await,
            Carry::Via("https://github.com/CambrianTech/continuum.git".into()),
            "the continuum-card case must stay pushable"
        );
        // …and the MECHANISM, granted that carry, pushes and stops pinning her.
        std::fs::write(m.a.join("work.txt"), "her act\n").unwrap();
        let out = sync(&m.a, card, "node-a").await;
        assert!(out.is_ok() && !out.pins_the_mind(), "{out:?}");
        assert!(!has_unpushed_work(&m.a).await, "nothing left here to strand");
        // A checkout with no origin at all pins her the same way, for the other reason.
        let orphan = m.tmp.path().join("orphan");
        git(m.tmp.path(), &["init", "-q", "-b", "main", &orphan.to_string_lossy()]).await;
        identity(&orphan).await;
        // A root commit first: `git_init_if_needed` gives every citizen workspace one, and
        // an UNBORN branch has no branch at all (`rev-parse --abbrev-ref HEAD` fails), which
        // is the `Nothing` shape, not this one.
        std::fs::write(orphan.join("seed.txt"), "seed\n").unwrap();
        git(&orphan, &["add", "seed.txt"]).await;
        git(&orphan, &["commit", "-q", "-m", "workspace: initial state"]).await;
        std::fs::write(orphan.join("a.txt"), "x\n").unwrap();
        assert_eq!(carry_of(&orphan).await, Carry::NoOrigin);
        assert_eq!(sync_after_act(&orphan, card, "n", None).await, PushOutcome::NotPushable { reason: NO_ORIGIN_REASON });
        assert!(has_unpushed_work(&orphan).await, "dirty work with nowhere to go pins her");
    }

    // what this catches (Codex root on bfa87dd5): the wait for a checkout's lock spent none of
    // the sync's bound, so a sync queued behind a hung one waited without limit and then ran its
    // git on a fresh bound. The deadline is ONE: contention spends it, and a sync that never got
    // the lock runs no git.
    #[tokio::test]
    async fn waiting_for_the_checkout_spends_the_sync_deadline_and_runs_no_git() {
        let m = two_machines().await;
        std::fs::write(m.a.join("src.txt"), "edit from A\n").unwrap();
        let lock = CHECKOUT_SYNC.entry(m.a.clone()).or_default().clone();
        let held = lock.lock().await; // another turn's sync of the same checkout
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(200);
        let out = sync_by(deadline, &m.a, sync(&m.a, Uuid::new_v4(), "node-a")).await;
        assert!(matches!(out, PushOutcome::TimedOut), "the wait spent the deadline: {out:?}");
        assert_eq!(git(&m.a, &["status", "--porcelain"]).await.trim(), "?? src.txt", "no git ran: her edit is uncommitted");
        drop(held);
        drop(lock);
        forget_checkout(&m.a);
    }

    // what this catches (card bfa87dd5): a push the deadline ended was classified from its error
    // text as unreachable or rejected, though it may have landed. It is PushInterrupted, the remote
    // is marked unknown, and the next sync reads the remote's branch before deciding to publish.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_push_the_deadline_ends_is_interrupted_and_the_next_sync_reads_the_remote_first() {
        use std::os::unix::fs::PermissionsExt;
        let m = two_machines().await;
        let hook = m.origin.join("hooks").join("pre-receive");
        std::fs::write(&hook, "#!/bin/sh\nexec sleep 600\n").unwrap(); // the remote holds the push
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(m.a.join("src.txt"), "edit from A\n").unwrap();
        let card = Uuid::new_v4();
        // 5 s is the margin for the local steps before the push, which the hook then holds for 600 s
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        let out = crate::code::git_bridge::within(deadline, sync(&m.a, card, "node-a")).await;
        assert!(matches!(out, PushOutcome::PushInterrupted { ref branch, .. } if *branch == m.branch), "{out:?}");
        assert!(REMOTE_UNKNOWN.contains(&m.a), "what the remote holds is unknown until read");
        std::fs::remove_file(&hook).unwrap();
        let next = sync(&m.a, card, "node-a").await;
        assert!(
            matches!(next, PushOutcome::Ok { committed: false, pushed: true, .. }),
            "the next sync read the remote, found the branch absent, and published it: {next:?}"
        );
        assert!(!REMOTE_UNKNOWN.contains(&m.a), "the read settled it");
    }

    // what this catches (Codex reviewer on #5037 at 992b027ac): a sync cancelled mid-push let go
    // of the checkout's lock with its future while its git's tree was ended after it, so the next
    // sync could run beside that git; and only a DEADLINE marked the remote unknown, so a
    // cancelled push left the next sync trusting a stale tracking ref. Phase by phase: the push is
    // under way (its hook recorded itself), the sync is cancelled, the checkout comes free with
    // the git's tree confirmed ended, the remote is unknown, and the next sync reads it before
    // publishing.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_sync_cancelled_mid_push_frees_the_checkout_once_its_git_has_ended_and_leaves_the_remote_unknown() {
        use std::os::unix::fs::PermissionsExt;
        let m = two_machines().await;
        let hook_pid = m.tmp.path().join("hook.pid");
        let hook = m.origin.join("hooks").join("pre-receive");
        std::fs::write(&hook, format!("#!/bin/sh\necho $$ > {}\nexec sleep 600\n", hook_pid.display())).unwrap(); // the remote holds the push
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(m.a.join("src.txt"), "edit from A\n").unwrap();
        let card = Uuid::new_v4();
        let root = m.a.clone();
        // under an admission, as the turn path's sync always is (sync_after_act_for)
        let context = crate::identity::hands_context::HandsContext::fresh();
        let lease = std::sync::Arc::new(crate::identity::hands_gate::admit(context).expect("open hands"));
        let syncing = tokio::spawn(async move {
            let unbounded_here = tokio::time::Instant::now() + std::time::Duration::from_secs(600); // a cancel ends it, not this
            crate::identity::hands_gate::admitted(Some(lease), sync_by(unbounded_here, &root, sync(&root, card, "node-a"))).await
        });
        while !hook_pid.exists() {
            assert!(!syncing.is_finished(), "the sync ended before its push reached the remote");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        syncing.abort();
        assert!(syncing.await.expect_err("cancelled").is_cancelled(), "the sync was cancelled mid-push");

        // that the lock is released only AFTER the git's tree has ended is lane_process's strict
        // order test (a witness reads the grandchild at the instant its holder drops); here, the
        // cancelled sync's checkout comes free, and a release confirms nothing was left behind
        let lock = CHECKOUT_SYNC.get(&m.a).map(|entry| entry.clone()).expect("the checkout's lock");
        let freed = tokio::time::timeout(std::time::Duration::from_secs(10), lock.clone().lock_owned())
            .await
            .expect("the checkout is free once the cancelled git's tree has ended");
        drop(freed);
        let lingering = crate::identity::hands_gate::close(context).await;
        assert!(lingering.is_empty(), "the cancelled push's tree was confirmed ended: {lingering:?}");

        assert!(REMOTE_UNKNOWN.contains(&m.a), "a cancelled push leaves the remote unknown");
        std::fs::remove_file(&hook).unwrap();
        let next = sync(&m.a, card, "node-a").await;
        assert!(
            matches!(next, PushOutcome::Ok { committed: false, pushed: true, .. }),
            "the next sync read the remote, found the branch absent, and published it: {next:?}"
        );
        assert!(!REMOTE_UNKNOWN.contains(&m.a), "the read settled it");
        drop(lock);
        forget_checkout(&m.a);
    }

    /// what this catches: the carry owner (modules::card_carry) committing a resident's
    /// work-in-progress mid-edit. It sweeps checkouts outside her turn, so it may only carry
    /// what she has already committed; a dirty tree is her turn's to commit (2026-10-17).
    #[tokio::test]
    async fn the_carry_owner_never_commits_a_residents_uncommitted_work() {
        use std::future::Future;
        use std::task::Poll;

        let m = two_machines().await;
        let root = m.a.clone();
        // As in sync(), separate transfer policy from the offline mechanism: a
        // network-shaped fetch URL qualifies, but every push goes to our bare fixture.
        git(&root, &["remote", "set-url", "origin", "https://origin.test/repo.git"]).await;
        git(&root, &["remote", "set-url", "--push", "origin", &m.origin.to_string_lossy()]).await;
        std::fs::write(root.join("half-written.rs"), "fn her_edit(\n").unwrap();
        let original_head = head(&root).await;

        let (persona, card, claim) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let context = crate::identity::hands_context::HandsContext::for_claim(persona, card, claim);
        let lease = std::sync::Arc::new(
            crate::identity::hands_gate::admit(context).expect("open hands admit"),
        );
        assert_eq!(carry_committed(lease.clone(), persona, "Benchy", root.clone(), card).await, None, "a dirty tree is not the carry's");
        assert_eq!(head(&root).await, original_head, "no commit was made for her");
        assert_eq!(git(&root, &["status", "--porcelain"]).await.trim(), "?? half-written.rs", "her edit is exactly as she left it");
        std::fs::remove_file(root.join("half-written.rs")).unwrap();

        // Regression for #5133: enter the REAL carry path while clean, hold its
        // checkout lock, then dirty it after the unlocked prechecks have finished.
        // Poll the same future until sync_by owns its lock clone: no sleeps or race.
        let lock = CHECKOUT_SYNC.entry(root.clone()).or_default().clone();
        let held = lock.clone().lock_owned().await;
        let holders_before = std::sync::Arc::strong_count(&lock);
        let mut carrying = Box::pin(carry_committed(lease.clone(), persona, "Benchy", root.clone(), card));
        std::future::poll_fn(|cx| {
            assert!(carrying.as_mut().poll(cx).is_pending(), "carry must wait on the held checkout lock");
            if std::sync::Arc::strong_count(&lock) > holders_before { Poll::Ready(()) } else { Poll::Pending }
        }).await;
        std::fs::write(root.join("README"), "her staged edit\n").unwrap();
        git(&root, &["add", "README"]).await;
        std::fs::write(root.join("README"), "her unfinished successor\n").unwrap();
        std::fs::write(root.join("half-written.rs"), "fn her_edit(\n").unwrap();
        let staged = git(&root, &["diff", "--cached", "--binary"]).await;
        let status = git(&root, &["status", "--porcelain"]).await;
        drop(held);
        assert_eq!(carrying.await, Some(PushOutcome::NotPushable { reason: DIRTY_CARRY_REASON }));
        assert_eq!(head(&root).await, original_head, "carry must never create a commit after its clean precheck");
        assert_eq!(git(&root, &["diff", "--cached", "--binary"]).await, staged, "her staged edit is preserved");
        assert_eq!(git(&root, &["status", "--porcelain"]).await, status);
        assert_eq!(std::fs::read_to_string(root.join("README")).unwrap(), "her unfinished successor\n");
        assert_eq!(std::fs::read_to_string(root.join("half-written.rs")).unwrap(), "fn her_edit(\n");

        // Only her own commit makes the tree carryable. Publication preserves its
        // exact identity, including author, rather than substituting a carry commit.
        git(&root, &["add", "."]).await;
        git(&root, &["commit", "-q", "-m", "her completed edit"]).await;
        let authored = head(&root).await;
        assert!(matches!(carry_committed(lease.clone(), persona, "Benchy", root.clone(), card).await,
            Some(PushOutcome::Ok { committed: false, pushed: true, sha, .. }) if sha == authored));
        assert_eq!(head(&root).await, authored);
        assert_eq!(git(&m.origin, &["rev-parse", &format!("refs/heads/{}", m.branch)]).await.trim(), authored);
        drop(lease);
        assert!(crate::identity::hands_gate::close(context).await.is_empty());
        drop(lock);
        forget_checkout(&root);
    }
