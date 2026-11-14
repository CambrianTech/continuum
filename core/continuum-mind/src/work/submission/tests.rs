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

    // what this catches: `latest` refused as "not a usable submission handle" (19 calls on the M5
    // in six hours, 2026-11-07). It is a value of the field and its default, in the schema she
    // sees; an id still decodes as an id; a word that is neither does not decode.
    #[test]
    fn latest_is_a_value_of_the_submission_handle_and_its_default() -> Result<(), Box<dyn std::error::Error>> {
        use super::SubmissionHandle;
        let read = |v: serde_json::Value| serde_json::from_value::<super::WorkSubmissionParams>(v);
        assert_eq!(read(serde_json::json!({ "card_id": "3a783ea1" }))?.submission_id, SubmissionHandle::Latest);
        assert_eq!(read(serde_json::json!({ "card_id": "3a783ea1", "submission_id": "latest" }))?.submission_id, SubmissionHandle::Latest);
        assert!(matches!(read(serde_json::json!({ "card_id": "3a783ea1", "submission_id": "f8896e64" }))?.submission_id, SubmissionHandle::Id(_)));
        assert!(read(serde_json::json!({ "card_id": "3a783ea1", "submission_id": "auto" })).is_err(), "neither latest nor an id");
        let schema = serde_json::to_value(schemars::schema_for!(super::WorkSubmissionParams))?;
        let field = &schema["properties"]["submission_id"];
        assert_eq!(field["default"], "latest", "{field}");
        assert_eq!(field["anyOf"][0]["enum"], serde_json::json!(["latest"]), "{field}");
        assert!(!schema["required"].as_array().ok_or("required")?.contains(&serde_json::json!("submission_id")), "{schema}");
        Ok(())
    }

    /// The M5's board as the refused calls in `replayed_refusals.json` met it (2026-11-07): Aiko's
    /// card 3a783ea1 with her submission f8896e64 and its review card 58855348 (Atlas); card
    /// 48c587c5 with submission 4ed430f0 and its review card 0caefac4 (Atlas); and cards with no
    /// submission yet (Benchy's be12a016 among them). Built through the real projection.
    fn replayed_board() -> Result<airc_work::WorkBoardProjection, Box<dyn std::error::Error>> {
        use airc_core::PeerId;
        use airc_work::{CardCreated, ClaimId, ClaimOrigin, GitObjectId, Priority, RepoId, SubmissionId, WorkCardClaimed, WorkCardId, WorkEvent, WorkSubmission};
        let id = |s: &str| uuid::Uuid::parse_str(s);
        let author = PeerId::from_uuid(id("75ece751-73ec-449c-be9f-66c2e502d9ab")?);
        let reviewer = PeerId::from_uuid(id("e5f4141d-1d95-4d62-8c19-97b5f8320837")?);
        let artifact: airc_work::SubmissionArtifact = serde_json::from_value(serde_json::json!({ "hash": "a".repeat(64), "size_bytes": 5 }))?;
        let mut board = airc_work::WorkBoardProjection::new();
        let mut events = Vec::new();
        let mut card = |card: &str, reviews: Option<&str>, owner: PeerId, submission: Option<&str>| -> Result<(), Box<dyn std::error::Error>> {
            let card_id = WorkCardId::from_uuid(id(card)?);
            let claim_id = ClaimId::new();
            events.push(WorkEvent::CardCreated(CardCreated {
                card_id, repo: RepoId::new("CambrianTech/continuum")?, title: card.into(), body: None, priority: Priority::P2,
                lane_id: None, created_by: owner, created_at_ms: 1, reviews: reviews.map(id).transpose()?.map(WorkCardId::from_uuid), origin: None,
            }));
            events.push(WorkEvent::CardClaimed(WorkCardClaimed {
                selected_at_ms: None, card_id, claim_id, owner, ttl_ms: 60_000, claimed_at_ms: 2, origin: ClaimOrigin::Explicit,
            }));
            if let Some(submission) = submission {
                events.push(WorkEvent::WorkSubmitted(WorkSubmission {
                    submission_id: SubmissionId::from_uuid(id(submission)?), card_id, claim_id, instance: card.into(),
                    base_sha: GitObjectId::new("b".repeat(40))?, head: airc_work::SubmissionHead::At(GitObjectId::new("c".repeat(40))?), artifact: artifact.clone(), publisher: owner, submitted_at_ms: 3,
                }));
            }
            Ok(())
        };
        card("3a783ea1-a40a-4278-b2f5-69026de6d889", None, author, Some("f8896e64-e9cd-4f9e-91cf-4774ca68fe90"))?;
        card("58855348-f6fa-56d7-9d4e-23bec0a81690", Some("3a783ea1-a40a-4278-b2f5-69026de6d889"), reviewer, None)?;
        card("48c587c5-91fe-4941-97c7-53af8d51690f", None, author, Some("4ed430f0-411f-4599-b7c8-16e6cf946832"))?;
        card("0caefac4-012e-5861-8371-ae2dfa8bee7c", Some("48c587c5-91fe-4941-97c7-53af8d51690f"), reviewer, None)?;
        for empty in ["be12a016-ec75-4042-8aa9-02b9f795f242", "d6ec19d7-8b8f-47a4-bec8-ba4d2a6157d2", "e657ea8e-4a74-4fc1-a0ca-d76a8358f0d6", "02c26ccd-0000-4000-8000-000000000001", "4d34acb9-0000-4000-8000-000000000002"] {
            card(empty, None, author, None)?;
        }
        for event in &events {
            board.apply(event)?;
        }
        Ok(board)
    }

    // what this catches: the refusal loops in the four residents' captures (the M5, 2026-11-07,
    // six hours; the calls are copied verbatim into `replayed_refusals.json`). Replayed through
    // the one handle resolver: every `latest` (or omitted) read resolves or is the card's typed
    // "none yet"; every read on a card with no submission is "none yet", refused with the call that
    // moves her on (`work/submit` on her own card); a review card reads the submission it reviews
    // (Atlas named the parent's f8896e64 on review card 58855348); what still misses is a guessed
    // id, refused by type with `work/submission` at its default as the next call.
    #[test]
    fn the_m5s_refused_submission_reads_replay_into_answers() -> Result<(), Box<dyn std::error::Error>> {
        use super::{SubmissionHandle, SubmissionRead, SubmissionRefusal, UnreadSubmission};
        use airc_core::shown_id::Shown;
        let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!("replayed_refusals.json"))?;
        let board = replayed_board()?;
        let cards: Vec<airc_work::WorkCardId> = board.snapshot().cards.iter().map(|c| c.card_id).collect();
        let (mut found, mut none_yet, mut guessed, mut undecodable, mut off_board) = (0, 0, 0, 0, 0);
        for row in rows.iter().filter(|r| r["verb"] == "work/submission") {
            let Ok(card_id) = Shown::<airc_work::WorkCardId>::try_from(row["card_id"].as_str().ok_or("card_id")?)?.resolve(cards.iter().copied()) else {
                off_board += 1; // a card this board never held: refused before any handle is read
                continue;
            };
            let card = board.card(card_id).ok_or("resolved on this board")?;
            let mut call = serde_json::json!({ "card_id": row["card_id"] });
            if let Some(handle) = row.get("submission_id") {
                call["submission_id"] = handle.clone();
            }
            let Ok(params) = serde_json::from_value::<super::WorkSubmissionParams>(call) else {
                undecodable += 1; // `auto`: neither latest nor an id, refused at decode by the schema's own type
                continue;
            };
            match params.submission_id.read(&board, card) {
                Ok(SubmissionRead::Found { submission, on }) => {
                    found += 1;
                    assert_eq!(submission.card_id, on.card_id, "{row}");
                }
                Ok(SubmissionRead::NoneYet { on }) => {
                    none_yet += 1;
                    assert!(on.submissions.is_empty(), "{row}");
                }
                Err(UnreadSubmission::Handle(_)) => {
                    guessed += 1;
                    assert_ne!(params.submission_id, SubmissionHandle::Latest, "latest never misses: {row}");
                }
                Err(other) => return Err(format!("{row}: {other}").into()),
            }
        }
        // 64 refused reads in the fixture, every one refused before. Now 36 answer (20 found, 16
        // the card's typed "none yet" with the call that moves her on); 21 guessed ids name no
        // submission on the card that carries them and are refused with `work/submission` at its
        // default as the next call; 2 `auto` never decode; 5 named cards outside this board.
        assert_eq!((found, none_yet, guessed, undecodable, off_board), (20, 16, 21, 2, 5));

        // The "none yet" refusal on her own card names work/submit; on a card she does not hold
        // (a reviewer waiting on the author) it names work/get; a guessed id names work/submission.
        let next = |e: crate::sdk_codegen::CommandError| match e {
            crate::sdk_codegen::CommandError::Refused(refusal) => refusal.next().calls().next().map(|call| call.verb().canonical()),
            _ => None,
        };
        let empty = board.card(airc_work::WorkCardId::from_uuid(uuid::Uuid::parse_str("be12a016-ec75-4042-8aa9-02b9f795f242")?)).ok_or("empty card")?;
        let author = empty.owner.ok_or("owned")?;
        let reviewer = airc_core::PeerId::from_uuid(uuid::Uuid::parse_str("e5f4141d-1d95-4d62-8c19-97b5f8320837")?);
        let at = airc_work::TAKEOVER_ON_RECORD_SINCE_MS;
        let none = || SubmissionRefusal::NoneYet { card: empty.card_id };
        assert_eq!(next(super::submission_refused(none(), &SubmissionHandle::Latest, empty.card_id, empty, author, at)), Some("work/submit"));
        assert_eq!(next(super::submission_refused(none(), &SubmissionHandle::Latest, empty.card_id, empty, reviewer, at)), Some("work/get"));
        let named = SubmissionHandle::Id(Shown::try_from(airc_work::SubmissionId::new().as_uuid().to_string().as_str())?);
        let missed = UnreadSubmission::NotOnCard { submission: airc_work::SubmissionId::new(), card: empty.card_id };
        assert_eq!(next(super::submission_refused(SubmissionRefusal::Unread(missed), &named, empty.card_id, empty, author, at)), Some("work/submission"));
        Ok(())
    }

    // what this catches (Cormac, review 1851ee8b of #5821): a refusal whose next call is the read
    // that was just refused, so taking it loops her. Every cause, with each handle that can reach
    // it, is walked: the next call is never `work/submission` with the handle she sent. A parent
    // absent from this board, or the newest refused by the review binding, sends her to `work/get`
    // on the parent; an id that named nothing sends her to the newest.
    #[test]
    fn a_submission_refusal_never_suggests_the_read_it_refused() -> Result<(), Box<dyn std::error::Error>> {
        use super::{SubmissionHandle, SubmissionRefusal, UnreadSubmission};
        use airc_core::shown_id::Shown;
        use airc_work::{SubmissionId, WorkCardId, WorkReviewRejectionReason};
        use crate::cognition::work_preconditions::{fixture_card, FIXTURE_NOW_MS};
        let her = airc_core::PeerId::from_uuid(uuid::Uuid::from_u128(1));
        let parent = WorkCardId::from_u128(7);
        let review = fixture_card(airc_work::CardState::Claimed, Some(her), Some(parent), "review of a card on another board");
        let named = SubmissionHandle::Id(Shown::try_from(SubmissionId::from_u128(0xabcd).as_uuid().to_string().as_str())?);
        let wire = |handle: &SubmissionHandle| serde_json::to_value(handle);
        let cases = [
            (UnreadSubmission::ParentAbsent(parent), SubmissionHandle::Latest, "work/get"),
            (UnreadSubmission::ParentAbsent(parent), named, "work/get"),
            (UnreadSubmission::Review { parent, reason: WorkReviewRejectionReason::UnknownSubmission }, SubmissionHandle::Latest, "work/get"),
            (UnreadSubmission::Review { parent, reason: WorkReviewRejectionReason::WrongReviewCard }, named, "work/submission"),
            (UnreadSubmission::NotOnCard { submission: SubmissionId::from_u128(0xabcd), card: review.card_id }, named, "work/submission"),
        ];
        for (cause, asked, expected) in cases {
            let label = format!("{cause:?} asked {asked:?}");
            let refused = super::submission_refused(SubmissionRefusal::Unread(cause), &asked, review.card_id, &review, her, FIXTURE_NOW_MS);
            let crate::sdk_codegen::CommandError::Refused(refusal) = refused else { return Err(format!("{label}: not a typed refusal").into()) };
            let call = refusal.next().calls().next().ok_or(format!("{label}: no next call"))?;
            assert_eq!(call.verb().canonical(), expected, "{label}");
            let repeats = call.verb().canonical() == "work/submission"
                && call.args().get("submission_id").cloned().unwrap_or(serde_json::json!(super::LATEST)) == wire(&asked)?; // unwrap_or: an omitted handle is `latest`, its default
            assert!(!repeats, "{label}: the next call is the refused read");
            if expected == "work/get" {
                assert_eq!(call.args().get("card_id"), Some(&serde_json::json!(parent.to_string())), "{label}: the parent");
            }
        }
        Ok(())
    }

    // what this catches: `work/review` called on her own implementation card (22 refusals in the
    // same captures, "not a review card (it reviews nothing)"). The verb is not offered there: it
    // declares `OnDemand`, so it is not among her core hands, and her held card puts it in hand
    // only when that card reviews something. Every card those calls named, held by her in any
    // working state, leaves it out of hand; a review card she holds puts it in.
    #[test]
    fn the_review_verb_is_never_offered_on_her_own_card() -> Result<(), Box<dyn std::error::Error>> {
        use crate::cognition::work_preconditions::{fixture_card, in_hand, FIXTURE_NOW_MS};
        use crate::sdk_codegen::ActionCommand;
        use airc_work::CardState;
        use continuum_base::tool_disclosure::Disclosure;
        assert_eq!(<super::WorkReview as ActionCommand>::DISCLOSURE, Disclosure::OnDemand);
        assert_eq!(<super::WorkSubmission as ActionCommand>::DISCLOSURE, Disclosure::OnDemand);
        let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!("replayed_refusals.json"))?;
        let her = airc_core::PeerId::from_uuid(uuid::Uuid::from_u128(1));
        let reviewed = rows.iter().filter(|r| r["verb"] == "work/review").count();
        assert_eq!(reviewed, 22, "the replayed calls");
        for state in [CardState::Open, CardState::Claimed, CardState::InProgress, CardState::Review, CardState::Closed] {
            let own = fixture_card(state, Some(her), None, "her own card");
            assert!(!in_hand(&own, her, FIXTURE_NOW_MS).contains(&"work/review"), "{state:?}");
        }
        let review = fixture_card(CardState::Claimed, Some(her), Some(airc_work::WorkCardId::from_u128(7)), "a review card");
        assert!(in_hand(&review, her, FIXTURE_NOW_MS).contains(&"work/review"));
        Ok(())
    }

    // what this catches (2026-09-17): a submit the citizen could not satisfy by hand —
    // the manual's zero hash is a placeholder and never an artifact; the artifact is
    // the SHA-256 of her patch; a benchmark checkout names its instance from its path
    // and a repo worktree names the card; a worktree with no base to diff against is a
    // named refusal, never a diff against HEAD.
    use super::{artifact_of_patch, base_sha_of, instance_of_checkout, is_placeholder_hash, resolve_shown, room_label, short8};

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
        let bench = super::next_after_submission(Some(id), &[], true, None, None).expect("submitted");
        assert!(bench.contains(&short8(id)) && bench.contains("test oracle grades it now") && bench.contains("verdict in the room"), "{bench}");
        assert!(bench.contains("marking the card done") && !bench.contains("No state"), "never contradicts the bench card's own text: {bench}");
        assert!(bench.contains("failed grade: revise and work/submit again"), "{bench}");
        let handoff = Some(airc_lib::WorkCardId::new());
        let pr = airc_work::PullRequestRef {
            repo: airc_work::RepoId::new("CambrianTech/continuum").expect("a valid repo id"),
            number: 5900,
            head: airc_work::BranchName::new("e5c80c7f/her-fix").expect("a valid branch"),
            base: airc_work::BranchName::new("canary").expect("a valid branch"),
        };
        let reviewed = super::next_after_submission(Some(id), &[], false, handoff, Some(&pr)).expect("submitted");
        assert!(reviewed.contains("Await the review") && reviewed.contains("Do not submit again unless it is sent back"), "{reviewed}");
        assert!(reviewed.contains("CambrianTech/continuum#5900"), "the pull request carrying it is named: {reviewed}");
        assert_eq!(super::next_after_submission(Some(id), &[Unknown], false, handoff, Some(&pr)), Some(reviewed.clone()), "an unknown verdict is still waiting");
        assert!(super::next_after_submission(Some(id), &[], false, None, None).unwrap().contains("no review handoff is recorded"));
        let back = super::next_after_submission(Some(id), &[Failed], true, None, None).expect("sent back");
        assert!(back.contains("sent submission") && back.contains("work/submit again"), "{back}");
        let merging = super::next_after_submission(Some(id), &[Failed, Passed], false, handoff, Some(&pr)).expect("passed");
        assert!(merging.contains("passed review") && merging.contains("CambrianTech/continuum#5900") && !merging.contains("card is done"), "a pass outranks an earlier send-back, and lands through its merge: {merging}");
        // what this catches (break 2): a pass with no PR linked told her the card was done while
        // its patch landed nowhere; it names the move that opens and links the PR instead.
        let unlanded = super::next_after_submission(Some(id), &[Passed], false, handoff, None).expect("passed");
        assert!(!unlanded.contains("card is done") && unlanded.contains("no pull request is linked") && unlanded.contains("work/submit"), "{unlanded}");
        let graded = super::next_after_submission(Some(id), &[Passed], true, None, None).expect("passed");
        assert!(graded.contains("card is done"), "an oracle-graded card finishes on its grade: {graded}");
        assert_eq!(super::next_after_submission(None, &[], true, None, None), None, "nothing submitted, nothing to say");
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
        // the way out without naming a command in the sentence (`commands_in_refusal_text`): name its room
        assert!(e.to_string().contains("room=") && !e.to_string().contains("room/join"), "{e}");
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

    // what this catches (Cormac's review of #5775 at 79e9dec89; the 2026-07-13 measurement
    // that 28% of live work/claim calls carried a full UUID with one character added or
    // dropped, e.g. `d7cfe47e0-8e39-...`): work/submit's `claim_id` stays a String on the
    // `id_resolve` path it had on canary, scoped to THIS card's one claim. Its full and
    // shown forms resolve. The corrupted full id is pinned as REFUSED, because that path is
    // `resolve_handle`, which never repaired (the repairing `id_resolve::resolve` is not
    // what work/submit called): card 484b000b adds the 8-hex-group form to `Shown` and
    // flips the last assertion when it migrates this field.
    #[test]
    fn a_submit_claim_id_keeps_its_canary_path_full_and_shown_resolve(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let claim = uuid::Uuid::parse_str("d7cfe47e-8e39-4c1a-9b2f-0a1b2c3d4e5f")?;
        let p: super::WorkSubmitParams = serde_json::from_value(serde_json::json!({
            "card_id": "0c4317e1", "claim_id": "d7cfe47e"
        }))?;
        assert_eq!(p.claim_id.as_deref(), Some("d7cfe47e"), "the claim handle decodes as she typed it");
        let resolve = |raw: &str| resolve_shown(raw, &[claim], "claim");
        assert!(matches!(resolve("d7cfe47e"), Ok(id) if id == claim), "the shown form");
        assert!(matches!(resolve(&claim.to_string()), Ok(id) if id == claim), "the full form");
        assert!(
            resolve("d7cfe47e0-8e39-4c1a-9b2f-0a1b2c3d4e5f").is_err(),
            "canary's strict path refuses a one-char-corrupted full id; 484b000b flips this"
        );
        Ok(())
    }

    /// A card holding `submissions`, the one candidate set `submission_named` reads.
    fn card_with_submissions(
        submissions: &[airc_work::SubmissionId],
    ) -> Result<airc_work::WorkCard, Box<dyn std::error::Error>> {
        use airc_core::PeerId;
        use airc_work::{CardState, ClaimId, GitObjectId, Priority, RepoId, WorkCard, WorkCardId, WorkSubmission};
        let card_id = WorkCardId::new();
        let claim_id = ClaimId::new();
        let artifact: airc_work::SubmissionArtifact = serde_json::from_value(serde_json::json!({
            "hash": "a".repeat(64), "size_bytes": 5, "mime": "text/x-diff"
        }))?;
        let base_sha = GitObjectId::new("b".repeat(40))?;
        let publisher = PeerId::new();
        Ok(WorkCard {
            card_id,
            repo: RepoId::new("CambrianTech/continuum")?,
            title: "a card".to_string(),
            body: None,
            priority: Priority::P2,
            lane_id: None,
            state: CardState::Claimed,
            owner: Some(publisher),
            claim_provenance: None,
            claim_id: Some(claim_id),
            claim_expires_at_ms: None,
            last_heartbeat_at_ms: None,
            pull_request: None,
            created_by: publisher,
            created_at_ms: 0,
            updated_at_ms: 0,
            reviews: None,
            submissions: submissions
                .iter()
                .map(|&submission_id| WorkSubmission {
                    submission_id,
                    card_id,
                    claim_id,
                    instance: "ordinary".into(),
                    base_sha: base_sha.clone(),
                    head: airc_work::SubmissionHead::Unrecorded,
                    artifact: artifact.clone(),
                    publisher,
                    submitted_at_ms: 10,
                })
                .collect(),
            last_submission_rejection: None,
        })
    }

    // what this catches (Benchy, M5 2026-11-04 17:51:56Z): `persona.act.refused
    // command="work/submit" reason=work/submit: [arguments] CommandRequest deserialization
    // failed: UUID parsing failed: invalid length: expected length 32 for simple f...`.
    // work/submit's submission_id (and work/review's review_id, submission_id and
    // review_claim_id) were raw `Uuid`, so the id she was SHOWN died in serde before any
    // handler could resolve it. They decode as airc's `Shown<T>` and resolve against the one
    // card the verb read: a short id resolves on both verbs, a full id passes through, a
    // prefix two submissions share is the typed `Ambiguous` refusal naming both, and junk is
    // refused at decode with a sentence that names the accepted form.
    #[test]
    fn a_shown_submission_id_resolves_on_its_card_and_a_shared_prefix_is_refused_by_type(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use airc_core::shown_id::ShownIdError;
        use airc_work::SubmissionId;
        let a = SubmissionId::from_uuid(uuid::Uuid::parse_str("d61513e4-a332-4cd3-a208-5bf0e8acd50d")?);
        let b = SubmissionId::from_uuid(uuid::Uuid::parse_str("d61599ff-0000-4000-8000-000000000000")?);
        let card = card_with_submissions(&[a, b])?;

        // work/submit: the short id she was shown decodes and names her submission.
        let submit: super::WorkSubmitParams = serde_json::from_value(serde_json::json!({
            "card_id": "0c4317e1", "submission_id": "d61513e4", "claim_id": "136f8174"
        }))?;
        let named = submit.submission_id.ok_or("the submission id she sent")?;
        assert_eq!(super::submission_named(&named, Some(&card)), Ok(a));

        // work/review: every id she was shown decodes; the submission resolves the same way.
        let review: super::WorkReviewParams = serde_json::from_value(serde_json::json!({
            "review_card_id": "3f2a91c0", "outcome": "passed", "card_id": "0c4317e1",
            "submission_id": "d61599ff", "review_id": "9a0b1c2d", "review_claim_id": "136f8174"
        }))?;
        let super::SubmissionHandle::Id(named) = review.submission_id else { return Err("the submission id she sent".into()) };
        assert_eq!(super::submission_named(&named, Some(&card)), Ok(b));

        // A full id still works, and passes without its candidates (a new id she mints).
        let fresh = uuid::Uuid::new_v4();
        let full: super::WorkSubmitParams = serde_json::from_value(serde_json::json!({
            "card_id": "0c4317e1", "submission_id": fresh.to_string()
        }))?;
        let named = full.submission_id.ok_or("the submission id she sent")?;
        assert_eq!(super::submission_named(&named, None), Ok(SubmissionId::from_uuid(fresh)));

        // A prefix both submissions share is refused by TYPE, naming both, never guessed.
        let shared: super::WorkSubmitParams = serde_json::from_value(serde_json::json!({
            "card_id": "0c4317e1", "submission_id": "d615"
        }))?;
        let named = shared.submission_id.ok_or("the submission id she sent")?;
        let refused = super::submission_named(&named, Some(&card));
        assert!(
            matches!(&refused, Err(ShownIdError::Ambiguous { matches, .. }) if matches.len() == 2),
            "{refused:?}"
        );

        // Junk never decodes, and what she reads names the form that is accepted.
        let junk = serde_json::from_value::<super::WorkSubmitParams>(serde_json::json!({
            "card_id": "0c4317e1", "submission_id": "placeholder"
        }));
        let what_she_reads = junk.err().ok_or("junk must not decode")?.to_string();
        assert!(what_she_reads.contains("give the full id or at least"), "{what_she_reads}");
        Ok(())
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
        // A submission is a commit (card c3504a71: the merge gate needs its head): an uncommitted
        // change is refused with staging it and committing it as her next moves, typed.
        let uncommitted = super::derive_submission(&GitDiffBenchmark, dir.path(), card, None, Some(base.clone()), None, &[]).await.err().expect("an uncommitted change is refused");
        assert_eq!(next_commands(&uncommitted), ["code/git/add", "code/git/commit"], "{uncommitted}");
        assert!(run(&["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "solution"]).status.success());
        let clean_base = String::from_utf8(run(&["rev-parse", "HEAD"]).stdout).expect("sha").trim().to_string();
        let derived = super::derive_submission(&GitDiffBenchmark, dir.path(), card, None, Some(base.clone()), None, &[]).await.expect("derive without ambient hands");
        assert_eq!(derived.instance, card.to_string());
        assert_eq!(derived.base_sha, base);
        assert_eq!(derived.head, clean_base, "the submission records the commit its patch ends at");
        let expected = String::from_utf8(run(&["diff", "--binary", "--full-index", "--no-ext-diff", "--no-textconv", "--no-color", "--src-prefix=a/", "--dst-prefix=b/", &base, "HEAD", "--", "solution.py"]).stdout).expect("diff");
        assert_eq!(derived.artifact.hash, artifact_of_patch(&expected).hash);
        assert!(derived.artifact.size_bytes > 0);
        // A commit carrying a path a submission never publishes is refused by name: the gate merges
        // the commit, so a patch that dropped it would land it unreviewed (Fable on #5824).
        std::fs::create_dir_all(dir.path().join(".continuum")).expect("substrate dir");
        std::fs::write(dir.path().join(".continuum/state.json"), "{}").expect("substrate file");
        assert!(run(&["add", "-f", ".continuum/state.json"]).status.success());
        assert!(run(&["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "substrate"]).status.success());
        let carried = super::derive_submission(&GitDiffBenchmark, dir.path(), card, None, Some(base.clone()), None, &[]).await.err().expect("a commit carrying substrate is refused");
        assert!(carried.to_string().contains(".continuum/state.json"), "{carried}");
        assert!(run(&["reset", "-q", "--hard", &clean_base]).status.success());
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
        // Regression for Aris's loop (2026-10-31, investigation card 3a8a243a): a "no patch"
        // refusal names the open way forward, so it never sends her back to work/submission. And
        // for Aiko's (the M5, 2026-11-13, review card 149f1391: 8 refused submits while the sentence
        // named `work/note` and `work/state`, offered on her list as `ledger` and `update_task`): the
        // moves are typed references to the commands, rendered under the names her list offers.
        assert_eq!(next_commands(&empty), ["work/note", "work/state"], "{empty}");
        let said = empty.to_string();
        let crate::sdk_codegen::CommandError::Refused(refused) = &empty else { panic!("a typed refusal: {empty}") };
        for call in refused.next().calls() {
            assert!(said.contains(&call.call()), "the call renders as offered: {said}");
            assert!(!said.contains(call.verb().canonical()), "never by its canonical name in prose: {said}");
        }
        // Regression for 6dd8befa: a valid empty submission is not malformed
        // arguments. The real refusal reaches her without advice to retry its schema.
        let refusal = format!("work/submit: {empty}");
        assert_eq!(
            crate::cognition::tool_executor::command_executor::persona_tool_error("work_submit", refusal.clone()),
            refusal,
        );
    }

    // what this catches (Kimi, 2026-09-21; Codex on the precondition redesign): work/submit's
    // decision at its mutation boundary, through the command's own error mapping. Someone else's
    // card names the holder; a stale claim id from a re-claim names the live claim and the way out
    // that works (omit claim_id), with the card as it is NOW and her moves on it now; and a card
    // that moved to Review since she acted still takes her revised submission.
    #[test]
    fn the_submit_decision_at_the_boundary_names_what_it_saw_and_her_moves_now() {
        use crate::cognition::work_preconditions::{decide, fixture_card, submit, ClaimArg, Request, FIXTURE_NOW_MS};
        use airc_work::CardState;
        let me = airc_core::PeerId::from_uuid(uuid::Uuid::from_u128(1));
        let peer = airc_core::PeerId::from_uuid(uuid::Uuid::from_u128(2));
        let at = FIXTURE_NOW_MS;
        let any = Request::any(at);
        let theirs = fixture_card(CardState::InProgress, Some(peer), None, "x");
        let held = crate::work::refused(decide(submit(&theirs, me, &any), &theirs, me, at).expect_err("not hers")).to_string();
        assert!(held.contains(&short8(peer.as_uuid())), "names the holder: {held}");
        // Since she read it, the card moved to Review and her claim was re-taken.
        let now = fixture_card(CardState::Review, Some(me), None, "x");
        let stale = super::ClaimId::from_uuid(uuid::Uuid::from_u128(0x57a1e));
        let named = Request { at_ms: at, claim: ClaimArg::Named(stale), seen_revision: None };
        let bad = crate::work::refused(decide(submit(&now, me, &named), &now, me, at).expect_err("stale id")).to_string();
        let live = now.claim_id.expect("an owned fixture holds a claim"); // expect: fixture_card claims an owned card
        assert!(bad.contains(&short8(live.as_uuid())) && bad.contains("omit claim_id"), "names the live claim and the way out: {bad}");
        assert!(bad.contains("review") && bad.contains("work/submit"), "the card now, and her moves now: {bad}");
        assert_eq!(decide(submit(&now, me, &any), &now, me, at), Ok(live), "a revised submission in Review is publishable");
    }

    // what this catches: Benchy on the M5, 2026-11-04, five refused submits of the same patch.
    // An identical submission (same publisher, patch hash, base and instance) answers as success
    // naming the existing one; a changed patch is a new submission (the precondition above lets it
    // publish in Review).
    #[test]
    fn an_identical_submission_is_answered_with_the_existing_one_and_a_changed_patch_is_new() {
        let key = |hash: &str| super::SubmissionKey {
            id: uuid::Uuid::from_u128(1),
            publisher: uuid::Uuid::from_u128(2),
            artifact_hash: hash.into(),
            base_sha: "b4se".into(),
            instance: "card".into(),
        };
        assert_eq!(super::same_submission([key("dc597e")], &key("dc597e")), Some(uuid::Uuid::from_u128(1)));
        assert_eq!(super::same_submission([key("dc597e")], &key("ffff00")), None, "a changed patch submits");
        let existing = airc_work::WorkSubmission {
            submission_id: airc_work::SubmissionId::from_u128(1),
            card_id: airc_work::WorkCardId::from_u128(3),
            claim_id: super::ClaimId::from_uuid(uuid::Uuid::from_u128(4)),
            instance: "card".into(),
            base_sha: airc_work::GitObjectId::new("a".repeat(40)).expect("a full sha"), // expect: 40 hex is a valid sha
            head: airc_work::SubmissionHead::Unrecorded,
            artifact: artifact_of_patch("diff --git a/x b/x\n").into_artifact().expect("a patch reference"), // expect: a sha-256 reference
            publisher: airc_core::PeerId::from_uuid(uuid::Uuid::from_u128(2)),
            submitted_at_ms: 7,
        };
        let answer = super::already_submitted(&existing, uuid::Uuid::from_u128(5), &[], false, None);
        assert_eq!((answer.outcome, answer.submission_id), (super::SubmitOutcome::AlreadySubmitted, uuid::Uuid::from_u128(1)));
        assert!(answer.next.contains("already your submission"), "{}", answer.next);
    }

/// The commands a refusal's next moves name, by canonical name, first first.
fn next_commands(error: &crate::sdk_codegen::CommandError) -> Vec<&'static str> {
    match error {
        crate::sdk_codegen::CommandError::Refused(refused) => refused.next().calls().map(|call| call.verb().canonical()).collect(),
        other => panic!("a typed refusal with its next moves, not {other}"),
    }
}
