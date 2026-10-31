#![cfg(test)]

    // what this catches (Codex root, review of #5017): work/review read the room's policy
    // twice, so a Declared(self: false) read followed by an Unavailable read published the
    // author's own review under a policy nobody could verify. The command now reads ONCE and
    // decides on that snapshot; every outcome of the one read is pinned here.
    #[test]
    fn her_own_review_publishes_only_on_a_readable_policy_that_admits_it() {
        use crate::experience::binding::ReviewPolicy;
        use crate::persona::work_board::review::PolicyRead;
        use crate::sdk_codegen::CommandError;
        let card = airc_work::WorkCardId::new();
        assert!(super::own_review_publishable(&PolicyRead::Absent, &card).is_ok(), "no policy keeps today's behaviour");
        let admits = ReviewPolicy { self_review: true, ..ReviewPolicy::default() };
        assert!(super::own_review_publishable(&PolicyRead::Declared(admits), &card).is_ok());
        let refuses = ReviewPolicy { self_review: false, ..ReviewPolicy::default() };
        assert!(super::own_review_publishable(&PolicyRead::Declared(refuses), &card).is_err());
        let Err(CommandError::Invalid(why)) =
            super::own_review_publishable(&PolicyRead::Unavailable("wall unreachable".into()), &card)
        else {
            panic!("an unreadable policy must refuse her own review, by name");
        };
        assert!(why.contains("could not be read") && why.contains("wall unreachable"), "{why}");
    }

    // what this catches (card d6e1b7a2, BigMama on #4836): a card she HOLDS on a room she is
    // not subscribed to, seen by her gate but unreachable by work/review, by full id or by the
    // short handle the board showed her. Her followed-claim record names its room; a handle
    // that matches none of her records names nothing (the caller's own refusal stands).
    #[test]
    fn a_card_she_holds_on_an_unsubscribed_room_names_its_room() {
        let home = tempfile::tempdir().expect("test: tempdir");
        let room = airc_lib::Room::from_name(home.path(), "cambriantech").expect("test: a room");
        let card = uuid::Uuid::parse_str("017843bb-d898-4a83-9712-668535b188c6").expect("test: uuid");
        let recorded = vec![crate::persona::held_claims::HeldClaim {
            room: room.clone(),
            card_id: card,
            claim_id: uuid::Uuid::from_u128(1),
            recorded_at_ms: 1,
            refusals: 0,
        }];
        assert_eq!(super::followed_room(&recorded, "017843bb").map(|r| r.channel), Some(room.channel));
        assert_eq!(super::followed_room(&recorded, &card.to_string()).map(|r| r.channel), Some(room.channel));
        assert!(super::followed_room(&recorded, "deadbeef").is_none(), "not hers: no room");
        assert!(super::followed_room(&[], "017843bb").is_none());
    }

    // what this catches (Kimi, 2026-10-05 12:01Z): a refused submission handle that
    // names only its shape. The refusal lists the card's submissions as prefixes the
    // resolver accepts, or says there are none, so the next call can be a real one.
    #[test]
    fn a_refused_submission_handle_is_told_the_cards_submissions() {
        let id = uuid::Uuid::parse_str("0a5b96b8-1111-4222-8333-444455556666").expect("test: id");
        let publisher = uuid::Uuid::parse_str("e2f0e022-04ac-4f66-a26c-7146551745b4").expect("test: publisher");
        let hint = super::submissions_on_card(&[(id, publisher)]);
        assert_eq!(hint, "submissions on this card: 0a5b96b8 by e2f0e022");
        let refused = super::resolve_shown("placeholder", &[id], "submission").expect_err("test: not a handle");
        let told = super::with_hint(refused, Some(hint));
        assert!(matches!(&told, super::CommandError::Invalid(m) if m.contains("0a5b96b8 by e2f0e022")), "{told:?}");
        assert!(super::resolve_shown("0a5b96b8", &[id], "submission").is_ok(), "the listed prefix resolves");
        assert!(super::submissions_on_card(&[]).contains("work/submit"));
    }
    // what this catches (2026-09-17): a submit the citizen could not satisfy by hand —
    // the manual's zero hash is a placeholder and never an artifact; the artifact is
    // the SHA-256 of her patch; a benchmark checkout names its instance from its path
    // and a repo worktree names the card; a worktree with no base to diff against is a
    // named refusal, never a diff against HEAD.
    use super::{artifact_of_patch, base_sha_of, holder_guard, instance_of_checkout, is_placeholder_hash, resolve_shown, room_label, short8};

    /// The seam's fixture: no datasets, no grader, and the candidate diff in the grader's form
    /// computed by plain `git diff`, so the derivation is exercised against a real repository.
    struct GitDiffBenchmark;

    #[async_trait::async_trait]
    impl super::BenchmarkSubmissionAdapter for GitDiffBenchmark {
        async fn base_commit(&self, _instance: &str) -> Result<Option<airc_work::GitObjectId>, super::CommandError> {
            Ok(None)
        }

        fn candidate_diff(&self, workspace: &std::path::Path, base: Option<&airc_work::GitObjectId>) -> Result<String, super::CommandError> {
            let out = std::process::Command::new("git")
                .args(["diff", "--binary", "--full-index", "--no-ext-diff", "--no-textconv", "--no-color", "--src-prefix=a/", "--dst-prefix=b/", base.map_or("HEAD", |b| b.as_str()), "--", "."])
                .current_dir(workspace)
                .output()
                .map_err(|e| super::CommandError::Internal(e.to_string()))?;
            String::from_utf8(out.stdout).map_err(|e| super::CommandError::Internal(e.to_string()))
        }

        async fn grade_hand_in(&self, _airc: std::sync::Arc<airc_lib::Airc>, _room: airc_lib::RoomId, _title: String, _instance: String, _patch: airc_blobs::MediaRef) {}
    }

    // what this catches (card aff659aa; the 5090 2026-10-11, Kimi on 27f133e4): ~5 h of work turns
    // asking which state transition triggers grading, and then (#4954) a first wording that said
    // "no state you set triggers it" while a bench card's own text says marking it done does. The
    // line now differs by card: a bench card is graded by its oracle at submit (done runs it too)
    // and the verdict posts to the room; a reviewed card waits on its reviewer. A send-back says
    // revise and resubmit, a pass says it fired, and nothing submitted says nothing.
    #[test]
    fn after_a_submit_the_receipt_says_what_happens_next() {
        use airc_work::WorkReviewOutcome::{Failed, Passed, Unknown};
        let id = uuid::Uuid::new_v4();
        let bench = super::next_after_submission(Some(id), &[], true, None).expect("submitted");
        assert!(bench.contains(&short8(id)) && bench.contains("test oracle grades it now") && bench.contains("verdict in the room"), "{bench}");
        assert!(bench.contains("marking the card done") && !bench.contains("No state"), "never contradicts the bench card's own text: {bench}");
        assert!(bench.contains("failed grade: revise and work/submit again"), "{bench}");
        let handoff = Some(airc_lib::WorkCardId::new());
        let reviewed = super::next_after_submission(Some(id), &[], false, handoff).expect("submitted");
        assert!(reviewed.contains("Await the review") && reviewed.contains("Do not submit again unless it is sent back"), "{reviewed}");
        assert_eq!(super::next_after_submission(Some(id), &[Unknown], false, handoff), Some(reviewed.clone()), "an unknown verdict is still waiting");
        assert!(super::next_after_submission(Some(id), &[], false, None).unwrap().contains("no review handoff is recorded"));
        let back = super::next_after_submission(Some(id), &[Failed], true, None).expect("sent back");
        assert!(back.contains("sent submission") && back.contains("work/submit again"), "{back}");
        let done = super::next_after_submission(Some(id), &[Failed, Passed], false, handoff).expect("passed");
        assert!(done.contains("passed review") && done.contains("card is done"), "a pass outranks an earlier send-back: {done}");
        assert_eq!(super::next_after_submission(None, &[], true, None), None, "nothing submitted, nothing to say");
    }

    // what this catches (Kimi, 2026-09-28): the same patch resubmitted on a card minting a
    // second submission, including across a re-taken claim (her a0bc9421 then 5656d7eb
    // straddled a rebuild). Same publisher and artifact is the existing one; a changed
    // patch or another citizen is a new submission.
    #[test]
    fn the_same_patch_on_the_same_card_is_an_existing_submission() {
        let key = |id: u128, by: u128, hash: &str, base: &str| super::SubmissionKey {
            id: uuid::Uuid::from_u128(id),
            publisher: uuid::Uuid::from_u128(by),
            artifact_hash: hash.into(),
            base_sha: base.into(),
            instance: "matplotlib-26011".into(),
        };
        let earlier = || vec![key(1, 2, "dc597e", "b4se")];
        let first = Some(uuid::Uuid::from_u128(1));
        assert_eq!(super::same_submission(earlier(), &key(0, 2, "dc597e", "b4se")), first, "even on a new claim");
        assert_eq!(super::same_submission(earlier(), &key(0, 2, "ffff00", "b4se")), None, "a changed patch");
        assert_eq!(super::same_submission(earlier(), &key(0, 4, "dc597e", "b4se")), None, "another citizen");
        assert_eq!(super::same_submission(earlier(), &key(0, 2, "dc597e", "n3wb")), None, "same diff on a new base");
    }
    use std::path::Path;

    // what this catches (Kimi, 2026-09-28): a submit naming the wrong room refused with only
    // "no card matches ... among 176" while the card sat on another board she belongs to.
    // The refusal names the room that holds it; a card on no other board adds nothing.
    #[tokio::test]
    async fn a_submit_to_the_wrong_room_names_the_room_that_holds_the_card() {
        let home = tempfile::tempdir().expect("temp airc home");
        let airc = std::sync::Arc::new(
            airc_lib::Airc::open_with_wire_root_for_test(home.path(), home.path())
                .await
                .expect("a local airc scope opens without a daemon"),
        );
        let project = airc.join("cambriantech").await.expect("join the card's room");
        let repo = airc_lib::RepoId::new("github.com/CambrianTech/career-wrangler").expect("repo");
        let card = airc
            .create_work_card(airc_lib::CreateWorkCard::new(repo, "umbrella", airc_lib::Priority::P1))
            .await
            .expect("card");
        let turn_room = airc.join("academy").await.expect("join her turn's room");
        let short = card.as_uuid().simple().to_string()[..8].to_string();
        let hint = super::card_elsewhere(&airc, &short, turn_room.channel.as_uuid())
            .await
            .expect("the card is on another board");
        assert!(hint.contains("'cambriantech'"), "{hint}");
        assert_eq!(
            super::card_elsewhere(&airc, &short, project.channel.as_uuid()).await,
            None,
            "asking the right room has nothing to add"
        );
        use crate::sdk_codegen::CommandError;
        let e = super::with_hint(CommandError::Invalid("no card".into()), Some(hint));
        assert!(matches!(e, CommandError::Invalid(ref m) if m.contains("'cambriantech'")), "{e:?}");
    }

    // what this catches (Kimi on the 5090, 2026-10-06: three refusals in an hour for guessed
    // rooms 'swe-bench' and 'default' with the card id in hand): a card verb acts in the room
    // whose board holds the card, whatever room she named or none; a room she names is used
    // only for a card on no board she can see, and with neither the refusal says how to fix it.
    #[tokio::test]
    async fn a_card_verb_acts_in_the_cards_own_room_whatever_room_she_named() {
        let home = tempfile::tempdir().expect("temp airc home");
        let airc = std::sync::Arc::new(
            airc_lib::Airc::open_with_wire_root_for_test(home.path(), home.path())
                .await
                .expect("a local airc scope opens without a daemon"),
        );
        airc.join("bench-swe-bench-verified-1789170698").await.expect("join the card's room");
        let repo = airc_lib::RepoId::new("github.com/CambrianTech/career-wrangler").expect("repo");
        let card = airc
            .create_work_card(airc_lib::CreateWorkCard::new(repo, "a task", airc_lib::Priority::P1))
            .await
            .expect("card");
        airc.join("academy").await.expect("her pointer stands elsewhere");
        let short = card.as_uuid().simple().to_string()[..8].to_string();
        for named in [None, Some("swe-bench"), Some("default"), Some("academy"), Some("")] {
            let room = super::room_of_card(&airc, named, &short, "work/submit").await.expect("the card says its room");
            assert_eq!(room.name, "bench-swe-bench-verified-1789170698", "named {named:?}");
        }
        let elsewhere = super::room_of_card(&airc, Some("academy"), "0000dead", "work/submit")
            .await
            .expect("a card on no board she can see: the room she named");
        assert_eq!(elsewhere.name, "academy");
        // what this catches (Cormac on #4822): a handle that does not resolve, with no room named,
        // is answered by the resolver's own refusal, never a confident "on no board" it cannot know
        let e = super::room_of_card(&airc, None, "0000dead", "work/submit")
            .await
            .expect_err("a prefix matching no card and no room: the resolver's refusal");
        assert!(e.to_string().contains("0000dead") && !e.to_string().contains("room/join"), "{e}");
        let whole = uuid::Uuid::new_v4().to_string();
        let e = super::room_of_card(&airc, None, &whole, "work/submit")
            .await
            .expect_err("a whole id on no readable board and no room: nothing to act in");
        assert!(e.to_string().contains("room/join") && e.to_string().contains("room="), "{e}");
    }

    // what this catches (Kimi's verdict on #4825, 2026-10-06): work/review's card fields were
    // Uuid, so the 8-char handle the board shows her died in serde ("expected length 32 ...
    // found 8") before any resolver ran. Handles parse; resolution happens on the board.
    #[test]
    fn a_review_takes_the_board_handles_she_was_shown() {
        let p: super::WorkReviewParams = serde_json::from_value(serde_json::json!({
            "review_card_id": "3f2a91c0",
            "card_id": "0c4317e1",
            "outcome": "passed"
        }))
        .expect("short handles parse");
        assert_eq!(p.review_card_id, "3f2a91c0");
        assert_eq!(p.card_id.as_deref(), Some("0c4317e1"));
        assert!(p.room.is_none(), "the card decides the room");
    }

    // what this catches (Kimi, 2026-09-28): a citizen with a pushed commit in hand passes its
    // git sha as the artifact hash and gets a bare parse error. A git sha is named as one and
    // the refusal says to omit the field; a real 64-hex hash is never mistaken for one.
    #[test]
    fn a_git_sha_as_the_artifact_hash_is_named_and_told_to_omit() {
        let git = super::WorkArtifactReference {
            hash: "fb618e5cdb2848ef530e13c6905df8ced49c624f".into(),
            size_bytes: 1,
            mime: None,
        };
        let msg = git.into_artifact().expect_err("a git sha is not an artifact hash").to_string();
        assert!(msg.contains("git commit sha") && msg.contains("Omit artifact"), "{msg}");
        assert!(super::looks_like_git_sha("fb618e5"), "an abbreviation too");
        assert!(!super::looks_like_git_sha(&"a".repeat(64)), "a SHA-256 is not a git sha");
        let real = super::WorkArtifactReference { hash: "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08".into(), size_bytes: 4, mime: None };
        assert!(real.into_artifact().is_ok());
    }

    // what this catches (Astra, 2026-09-23): these two verbs typed their id params as
    // `Uuid`, so the 8-char handle the board SHOWS her was refused by serde — before the
    // handler, before any message could name the problem or the form it wanted. The
    // resolution must be the shared strict primitive against a SCOPED candidate set, so
    // that a handle expands, a collision is refused by name, and nothing is repaired.
    #[test]
    fn a_verb_accepts_the_handle_its_board_showed_and_refuses_an_ambiguous_one() {
        let a = uuid::Uuid::parse_str("d61513e4-a332-4cd3-a208-5bf0e8acd50d").expect("fixture id"); // expect: a literal id in a test
        let b = uuid::Uuid::parse_str("d61599ff-0000-4000-8000-000000000000").expect("fixture id"); // expect: a literal id in a test
        let other = uuid::Uuid::parse_str("7de5b691-0000-4000-8000-000000000000").expect("fixture id"); // expect: a literal id in a test

        // THE HANDLE SHE WAS SHOWN. This is the call that used to fail at deserialization.
        assert_eq!(resolve_shown("d61513e4", &[a, other], "card").expect("resolves"), a); // expect: asserted to resolve

        // A full UUID passes straight through and never consults the set — so an empty
        // board (an outage, a wrong room) cannot turn a correct id into a miss.
        assert_eq!(resolve_shown(&a.to_string(), &[], "card").expect("passes through"), a); // expect: asserted to pass

        // AMBIGUITY IS REFUSED, NOT GUESSED, and supplying more digits disambiguates —
        // the whole reason this uses the strict handle resolver and not the repairing one.
        let ambiguous = resolve_shown("d615", &[a, b], "card").expect_err("two cards share d615"); // expect_err: asserted ambiguous
        assert!(format!("{ambiguous:?}").contains("d615"), "the refusal names the prefix that collided");
        assert_eq!(resolve_shown("d61513e4", &[a, b], "card").expect("more digits decide"), a); // expect: asserted to resolve

        // A MISS IS A MISS. An id that matches nothing is refused against the named
        // population rather than silently reaching a lookup that reports the card absent.
        assert!(resolve_shown("deadbeef", &[a, other], "card").is_err());

        // Too short to disambiguate, and empty, are both named refusals — never a match.
        assert!(resolve_shown("d6", &[a, other], "card").is_err());
        assert!(resolve_shown("   ", &[a, other], "card").is_err());
    }

    // what this catches (Kimi's work/submit, 2026-09-21): "card X is absent from this room"
    // is true and useless — she asked for one room name, `resolve_room` turned it into a
    // Room, and if that landed elsewhere the sentence gives her no way to see the mismatch,
    // so the substrate's resolution failing reads as her being wrong about her own card.
    // The refusal must name the RESOLVER's answer, and carry the channel id beside it
    // because two rooms can share a name across scopes (Cormac's condition).
    #[test]
    fn a_room_is_named_by_resolver_answer_and_channel_never_by_name_alone() {
        let a = uuid::Uuid::from_u128(0xabcdef12_3456_7890_abcd_ef1234567890);
        let b = uuid::Uuid::from_u128(0x12345678_90ab_cdef_1234_567890abcdef);
        let seed = room_label("continuum", a);
        assert!(seed.contains("'continuum'"), "the resolved NAME is quoted: {seed}");
        assert!(seed.contains(&short8(a)), "and the channel id rides beside it: {seed}");
        // Two rooms sharing a name are distinguishable — the whole point of carrying the id.
        assert_ne!(room_label("continuum", a), room_label("continuum", b));
    }

    #[test]
    fn placeholders_are_never_artifacts_and_a_patch_hashes_to_its_sha256() {
        assert!(is_placeholder_hash(""));
        assert!(is_placeholder_hash(&"0".repeat(64)));
        assert!(is_placeholder_hash("<replace-with-string>"));
        assert!(!is_placeholder_hash("9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"));
        let a = artifact_of_patch("test");
        assert_eq!(a.hash, "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08");
        assert_eq!(a.size_bytes, 4);
        assert_eq!(a.mime.as_deref(), Some("text/x-patch"));
        // Regression: a published hash must still resolve after the checkout changes.
        use airc_blobs::{ContentHash, FsStore, MediaRef};
        let dir = tempfile::tempdir().unwrap();
        let store = FsStore::new(dir.path()).unwrap();
        let retained = super::retain_patch(&store, "test").unwrap();
        assert_eq!(retained.hash, a.hash);
        super::retain_patch(&store, "a later candidate").unwrap();
        let reference = MediaRef {
            hash: ContentHash::from_hex(&retained.hash).unwrap(),
            size_bytes: retained.size_bytes,
            mime: retained.mime,
        };
        assert_eq!(store.get_verified(&reference, 4).unwrap(), b"test");
    }

    #[test]
    fn a_benchmark_checkout_names_its_instance_and_a_worktree_names_the_card() {
        let card = uuid::Uuid::from_u128(7);
        assert_eq!(
            instance_of_checkout(Path::new("/x/peers/p/workspace/swe/sympy__sympy-23413"), card).as_deref(),
            Some("sympy__sympy-23413")
        );
        assert_eq!(
            instance_of_checkout(Path::new("/x/.airc/worktrees/1f58fc16"), card),
            Some(card.to_string())
        );
    }

    // what this catches (Kimi, 2026-09-26, submission 74920fe9): a repo worktree that
    // fast-forwarded to the upstream tip submitted 177 KB of other people's commits,
    // because the diff ran from the worktree's creation base. Only what no remote carries
    // is hers; a worktree that merged nothing keeps its creation base.
    #[test]
    fn a_submission_diffs_past_the_upstream_history_she_merged() {
        let dir = tempfile::tempdir().expect("tempdir");
        let run = |args: &[&str]| {
            std::process::Command::new("git").args(args).current_dir(dir.path()).output().expect("git")
        };
        let commit = |msg: &str| {
            assert!(run(&["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", msg]).status.success());
            String::from_utf8(run(&["rev-parse", "HEAD"]).stdout).expect("sha").trim().to_string()
        };
        assert!(run(&["init", "-q"]).status.success());
        let created = commit("created here");
        // nothing merged from upstream yet: the creation base stands
        assert_eq!(super::past_upstream(dir.path(), &created, None, None).expect("proven history"), created);
        let upstream = commit("someone else's merged work");
        assert!(run(&["update-ref", "refs/remotes/origin/canary", &upstream]).status.success());
        std::fs::write(dir.path().join("hers.rs"), "fn hers() {}\n").expect("write");
        assert!(run(&["add", "hers.rs"]).status.success());
        let _mine = commit("her change");
        assert_eq!(super::past_upstream(dir.path(), &created, None, None).expect("proven history"), upstream, "the base moves past what upstream carries");
        // her own branch pushed to the remote is NOT upstream (Cormac on #4442): the runtime
        // pushes after each act, and that ref sits at her own tip
        let branch = String::from_utf8(run(&["symbolic-ref", "--short", "HEAD"]).stdout).expect("branch").trim().to_string();
        assert!(run(&["update-ref", &format!("refs/remotes/origin/{branch}"), "HEAD"]).status.success());
        assert_eq!(super::past_upstream(dir.path(), &created, None, None).expect("proven history"), upstream, "her own pushed tip never becomes the base");
        // a remote that does not descend from the creation base is ignored
        assert!(run(&["update-ref", "refs/remotes/origin/stale", &created]).status.success());
        assert_eq!(super::past_upstream(dir.path(), &created, None, None).expect("proven history"), upstream);

        // A feature may track the integration branch rather than its own publishing
        // ref. Only recorded repository authority can exempt that tracked ref.
        let repo = airc_work::RepoId::new("CambrianTech/continuum").expect("repo");
        assert!(run(&["remote", "add", "origin", "https://github.com/CambrianTech/continuum.git"]).status.success());
        assert!(run(&["config", &format!("branch.{branch}.remote"), "origin"]).status.success());
        assert!(run(&["config", &format!("branch.{branch}.merge"), "refs/heads/canary"]).status.success());
        assert!(run(&["update-ref", airc_lib::work_worktree::CREATION_BASE_REF, &created]).status.success());
        let mut evidence = airc_lib::work_worktree::AllocationEvidence {
            repo: repo.clone(), requested_branch: branch.clone(), integration_base: Some("canary".into()),
            start_ref: "refs/remotes/origin/canary".into(), creation_sha: created.clone(),
        };
        let evidence_path = dir.path().join(".git/allocation-fixture.json");
        let record = |bytes: &[u8]| {
            std::fs::write(&evidence_path, bytes).expect("allocation fixture");
            let object = run(&["hash-object", "-w", evidence_path.to_str().expect("fixture path")]);
            assert!(object.status.success());
            let object = String::from_utf8(object.stdout).expect("object");
            assert!(run(&["update-ref", airc_lib::work_worktree::ALLOCATION_REF, object.trim()]).status.success());
        };
        record(&serde_json::to_vec(&evidence).expect("allocation"));
        assert_eq!(super::past_upstream(dir.path(), &created, Some(&repo), Some("https://github.com/CambrianTech/continuum")).expect("tracked integration"), upstream);
        let diff = run(&["diff", &upstream, "HEAD", "--", "hers.rs"]);
        assert!(String::from_utf8(diff.stdout).expect("own diff").contains("fn hers()"), "committed own work remains in the candidate");
        for remote in ["https://foreign.example/CambrianTech/continuum.git", "ssh://git@foreign.example/CambrianTech/continuum.git", "git@foreign.example:CambrianTech/continuum.git"] {
            assert!(run(&["remote", "set-url", "origin", remote]).status.success());
            assert_eq!(super::past_upstream(dir.path(), &created, Some(&repo), Some("https://github.com/CambrianTech/continuum")).expect("foreign host excluded"), created);
        }
        for remote in ["git@github.com:CambrianTech/continuum.git", "ssh://git@github.com/CambrianTech/continuum.git"] {
            assert!(run(&["remote", "set-url", "origin", remote]).status.success());
            assert_eq!(super::past_upstream(dir.path(), &created, Some(&repo), Some("https://github.com/CambrianTech/continuum")).expect("same GitHub repository"), upstream);
        }
        assert!(run(&["remote", "set-url", "origin", "git@enterprise.example:CambrianTech/continuum.git"]).status.success());
        assert_eq!(super::past_upstream(dir.path(), &created, Some(&repo), Some("https://enterprise.example/CambrianTech/continuum")).expect("configured enterprise authority"), upstream);
        assert!(run(&["remote", "set-url", "origin", "https://github.com/CambrianTech/continuum.git.git"]).status.success());
        assert_eq!(super::past_upstream(dir.path(), &created, Some(&repo), Some("https://github.com/CambrianTech/continuum")).expect("distinct repeated suffix identity"), created);
        assert!(run(&["remote", "set-url", "origin", "https://github.com/Foreign/fork.git"]).status.success());
        assert_eq!(super::past_upstream(dir.path(), &created, Some(&repo), Some("https://github.com/CambrianTech/continuum")).expect("foreign tracked ref excluded"), created);
        assert!(run(&["remote", "set-url", "origin", "https://github.com/CambrianTech/continuum.git"]).status.success());
        assert!(run(&["config", &format!("branch.{branch}.merge"), &format!("refs/heads/{branch}")]).status.success());
        assert_eq!(super::past_upstream(dir.path(), &created, Some(&repo), Some("https://github.com/CambrianTech/continuum")).expect("own publishing ref excluded"), upstream);
        evidence.repo = airc_work::RepoId::new("Foreign/fork").expect("foreign repo");
        record(&serde_json::to_vec(&evidence).expect("mismatched allocation"));
        assert!(super::past_upstream(dir.path(), &created, Some(&repo), Some("https://github.com/CambrianTech/continuum")).unwrap_err().to_string().contains("does not match"));
        record(b"{broken");
        assert!(super::past_upstream(dir.path(), &created, Some(&repo), Some("https://github.com/CambrianTech/continuum")).unwrap_err().to_string().contains("allocation evidence"));
        assert!(run(&["update-ref", "-d", airc_lib::work_worktree::ALLOCATION_REF]).status.success());
        // Legacy absence is not corruption. The same shared policy supplies the base;
        // repositories without a configured policy retain the previous selection.
        assert!(run(&["config", &format!("branch.{branch}.merge"), "refs/heads/canary"]).status.success());
        let legacy_expected = if airc_lib::work_repository::configured_base_branch(&repo).as_deref() == Some("canary") { &upstream } else { &created };
        assert_eq!(&super::past_upstream(dir.path(), &created, Some(&repo), Some("https://github.com/CambrianTech/continuum")).expect("legacy shared policy"), legacy_expected);
        let unknown = airc_work::RepoId::new("Unknown/unconfigured").expect("unknown repo");
        assert_eq!(super::past_upstream(dir.path(), &created, Some(&unknown), None).expect("unknown policy preserves selection"), created);
        assert!(run(&["config", "--unset", &format!("branch.{branch}.merge")]).status.success());
        assert!(run(&["config", "--unset", &format!("branch.{branch}.remote")]).status.success());
        std::fs::remove_file(evidence_path).expect("remove fixture bytes");

        // The creation object still exists, but a real Git shallow boundary hides its
        // ancestry. Refuse instead of silently publishing the whole upstream delta.
        let head_before = String::from_utf8(run(&["rev-parse", "HEAD"]).stdout).expect("head");
        let status_before = run(&["status", "--porcelain"]).stdout;
        let shallow = dir.path().join(".git/shallow");
        std::fs::write(&shallow, format!("{upstream}\n")).expect("shallow boundary");
        assert!(!run(&["merge-base", "--is-ancestor", &created, "HEAD"]).status.success());
        let refused = super::past_upstream(dir.path(), &created, None, None).expect_err("unproven automatic base");
        assert!(refused.to_string().contains("pass a verified base_sha"), "{refused}");
        assert_eq!(String::from_utf8(run(&["rev-parse", "HEAD"]).stdout).expect("head"), head_before);
        assert_eq!(run(&["status", "--porcelain"]).stdout, status_before);
        std::fs::remove_file(shallow).expect("restore fixture history");
        assert_eq!(super::past_upstream(dir.path(), &created, None, None).expect("restored ancestry"), upstream);
    }

    #[tokio::test]
    async fn a_worktree_with_no_base_is_refused_by_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let run = |args: &[&str]| {
            std::process::Command::new("git").args(args).current_dir(dir.path()).output().expect("git")
        };
        assert!(run(&["init", "-q"]).status.success());
        assert!(run(&["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "root"]).status.success());
        // A remote default branch is not proof of a legacy worktree's creation base.
        assert!(run(&["update-ref", "refs/remotes/origin/HEAD", "HEAD"]).status.success());
        let err = base_sha_of(&GitDiffBenchmark, dir.path(), "card", false).await.expect_err("no recorded creation base");
        assert!(err.to_string().contains("pass base_sha"), "{err}");
        // Regression for Kimi's result5734: submitting an owned staged checkout
        // requires no temporary acting root (including after focus loss/restart).
        let base = String::from_utf8(run(&["rev-parse", "HEAD"]).stdout).expect("sha").trim().to_string();
        std::fs::write(dir.path().join("solution.py"), "print('verified work')\n").expect("write patch");
        let add = run(&["add", "solution.py"]);
        assert!(add.status.success());
        let card = uuid::Uuid::new_v4();
        let derived = super::derive_submission(&GitDiffBenchmark, dir.path(), card, None, Some(base.clone()), None, &[]).await.expect("derive without ambient hands");
        assert_eq!(derived.instance, card.to_string());
        assert_eq!(derived.base_sha, base);
        let expected = String::from_utf8(run(&["diff", "--binary", "--full-index", "--no-ext-diff", "--no-textconv", "--no-color", "--src-prefix=a/", "--dst-prefix=b/", "HEAD", "--", "solution.py"]).stdout).expect("diff");
        assert_eq!(derived.artifact.hash, artifact_of_patch(&expected).hash);
        assert!(derived.artifact.size_bytes > 0);
        assert!(run(&["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "solution"]).status.success());
        let clean_base = String::from_utf8(run(&["rev-parse", "HEAD"]).stdout).expect("sha").trim().to_string();
        // Regression for Kimi's oversized submissions: advancing the remote default
        // must neither swallow committed work nor change the stored creation base.
        assert!(run(&["update-ref", airc_lib::work_worktree::CREATION_BASE_REF, &base, ""]).status.success());
        assert!(run(&["update-ref", "refs/remotes/origin/HEAD", "HEAD"]).status.success());
        let recorded = super::derive_submission(&GitDiffBenchmark, dir.path(), card, None, None, None, &[]).await.expect("recorded base includes committed work");
        assert_eq!(recorded.base_sha, base);
        assert_eq!(recorded.artifact.hash, derived.artifact.hash);
        assert_eq!(recorded.artifact.size_bytes, derived.artifact.size_bytes);
        let empty = super::derive_submission(&GitDiffBenchmark, dir.path(), card, None, Some(clean_base), None, &[]).await.err().expect("clean checkout refused");
        assert!(empty.to_string().contains("nothing to submit"));
        // Regression for Aris's loop (2026-10-31, investigation card 3a8a243a): each "no patch"
        // refusal names the same open way forward, and the empty-card hint does not send her
        // back to a submit that has nothing to publish.
        let empty_card = super::submissions_on_card(&[]);
        for said in [empty.to_string(), empty_card] {
            assert!(said.contains("work/note") && said.contains("work/state closed"), "{said}");
        }
        // Regression for 6dd8befa: a valid empty submission is not malformed
        // arguments. The real refusal reaches her without advice to retry its schema.
        let refusal = format!("work/submit: {empty}");
        assert_eq!(
            crate::cognition::tool_executor::command_executor::persona_tool_error("work_submit", refusal.clone()),
            refusal,
        );
    }

    // what this catches (Kimi, 2026-09-21): the two halves of the supplied-claim guard
    // shared one sentence — "submission requires your matching claim on this card" — so a
    // citizen who OWNED the card and HELD a live lease, but passed a STALE claim id from a
    // re-claim, was told only that something did not match. `claim_id` is documented
    // optional and therefore a fillable blank in her manual, and citizens fill those
    // (ab76eb848). Each half must now name what it saw, and the stale half must name the
    // way out that actually works — omitting claim_id, which makes the caller read the
    // live claim off the board so this guard passes.
    #[test]
    fn each_half_of_the_holder_guard_names_what_it_saw() {
        let me = airc_core::PeerId::new();
        let peer = airc_core::PeerId::new();
        let card = uuid::Uuid::new_v4();
        let mine = super::ClaimId::from_uuid(uuid::Uuid::new_v4());
        let stale = super::ClaimId::from_uuid(uuid::Uuid::new_v4());

        // The holder's own live claim passes.
        assert!(holder_guard(Some(me), me, Some(mine), mine, card).is_ok());

        // Someone else holds it: say WHO, and reuse the holder sentence.
        let held = holder_guard(Some(peer), me, Some(mine), mine, card).expect_err("not hers");
        let held = held.to_string();
        assert!(held.contains(&short8(peer.as_uuid())), "names the holder: {held}");
        assert!(held.contains("only the holder submits"), "{held}");

        // Hers, but the id she passed is stale: name BOTH ids and the way out.
        let bad = holder_guard(Some(me), me, Some(mine), stale, card).expect_err("stale id");
        let bad = bad.to_string();
        assert!(bad.contains(&short8(stale.as_uuid())), "names what she passed: {bad}");
        assert!(bad.contains(&short8(mine.as_uuid())), "names the live claim: {bad}");
        assert!(bad.contains("omit claim_id"), "names the corrective that works: {bad}");
        assert!(!bad.contains("only the holder submits"), "she IS the holder: {bad}");

        // An unclaimed card names THAT absence rather than printing an empty slot.
        let none = holder_guard(Some(me), me, None, stale, card).expect_err("no live claim");
        assert!(none.to_string().contains("unclaimed"), "{}", none);
    }
