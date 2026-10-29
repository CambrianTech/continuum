#![cfg(test)]
use super::*;

    /// what this catches: a citizen's card landing somewhere other than the room she
    /// named (the old "current room" default put project cards in #general), or under
    /// an identity that is not hers — work/create is how an activity's participants
    /// break its goals into slices, so both the board and the author must be right.
    #[tokio::test]
    pub(super) async fn a_citizens_card_lands_in_the_named_room_created_by_her() {
        let home = tempfile::tempdir().expect("temp airc home");
        let airc = Arc::new(
            Airc::open_with_wire_root_for_test(home.path(), home.path())
                .await
                .expect("a local airc scope opens without a daemon"),
        );
        let project = airc.join("career-wrangler").await.expect("join the project room");
        let lobby = airc.join("general").await.expect("join the lobby; focus moves here");

        let made = WorkCreate::create(
            &airc,
            WorkCreateParams {
                room: "career-wrangler".to_string(),
                repo: Some("github.com/CambrianTech/career-wrangler".to_string()),
                title: "job list page".to_string(),
                body: None,
                priority: Some(CardPriority::P1),
            },
        )
        .await
        .expect("card created");

        let id = WorkCardId::from_uuid(Uuid::parse_str(&made.card_id).expect("card_id is a uuid"));
        let horizon = board_horizon(&airc).await.expect("subscribed boards");
        let (room, card) = horizon
            .boards
            .iter()
            .find_map(|(r, b)| b.card(id).map(|c| (r, c)))
            .expect("the created card is on a subscribed board");
        assert_eq!(room.channel, project.channel, "the named room, not the focused lobby");
        assert_ne!(room.channel, lobby.channel);
        assert_eq!(card.created_by, airc.peer_id(), "authored under her own identity");
        assert_eq!(card.priority, Priority::P1);

        // A blank room is refused, never read as "the current room".
        let blank = WorkCreate::create(
            &airc,
            WorkCreateParams {
                room: "  ".to_string(),
                repo: Some("github.com/CambrianTech/career-wrangler".to_string()),
                title: "should not land".to_string(),
                body: None,
                priority: None,
            },
        )
        .await;
        assert!(matches!(blank, Err(CommandError::Invalid(_))), "{blank:?}");

        // A blank repo is refused, never inferred.
        let blank_repo = WorkCreate::create(
            &airc,
            WorkCreateParams {
                room: "career-wrangler".to_string(),
                repo: Some("  ".to_string()),
                title: "blank".to_string(),
                body: None,
                priority: None,
            },
        )
        .await;
        assert!(
            matches!(&blank_repo, Err(CommandError::Invalid(m)) if m.contains("repo is blank")),
            "{blank_repo:?}"
        );

        // No repo named and no card held: refused with the fix, never a default repo.
        let unnamed = WorkCreate::create(
            &airc,
            WorkCreateParams {
                room: "career-wrangler".to_string(),
                repo: None,
                title: "orphan".to_string(),
                body: None,
                priority: None,
            },
        )
        .await;
        assert!(
            matches!(&unnamed, Err(CommandError::Invalid(m)) if m.starts_with("work/create: add repo to this call")),
            "the refusal leads with HER action: {unnamed:?}"
        );

        // The default path (Cormac on #4571): holding a card and naming no repo files the new
        // card against the HELD card's repo, never some other project's. A claim is made from
        // the card's own room.
        let held = WorkCardId::from_uuid(Uuid::parse_str(&made.card_id).expect("card_id is a uuid"));
        airc.join("career-wrangler").await.expect("stand in the card's room to claim it");
        airc.claim_work_card_with_origin(
            ClaimWorkCard { card_id: held, ttl_ms: 600_000 },
            airc_work::ClaimOrigin::Explicit,
        )
        .await
        .expect("she claims the slice card");
        let slice = WorkCreate::create(
            &airc,
            WorkCreateParams {
                room: "career-wrangler".to_string(),
                repo: None,
                title: "slice 2".to_string(),
                body: None,
                priority: None,
            },
        )
        .await
        .expect("a held card supplies the repo");
        let slice_id = WorkCardId::from_uuid(Uuid::parse_str(&slice.card_id).expect("uuid"));
        let horizon = board_horizon(&airc).await.expect("boards");
        let filed = horizon
            .boards
            .iter()
            .find_map(|(_, b)| b.card(slice_id))
            .expect("the new card is on a board");
        assert_eq!(filed.repo.to_string(), card.repo.to_string(), "the held card's repo, not another project's");
    }

    /// what this catches (Kimi on #4825, 2026-10-06): a reviewer's pass on a card whose PR
    /// is still open closed the card, and `airc work merge` refuses a Closed card, so the
    /// merge needed a hand move back to Review. A pass leaves a PR-linked card in Review
    /// for its merge, and a card with no PR still closes on the pass.
    #[tokio::test]
    pub(super) async fn a_pass_leaves_a_card_with_an_open_pr_mergeable_and_closes_one_without() {
        let home = tempfile::tempdir().expect("temp airc home");
        let airc = Arc::new(
            Airc::open_with_wire_root_for_test(home.path(), home.path())
                .await
                .expect("a local airc scope opens without a daemon"),
        );
        let room = airc.join("continuum").await.expect("join the project room");
        let repo = RepoId::new("github.com/CambrianTech/continuum").expect("repo id");
        let mut outcomes = Vec::new();
        for with_pr in [true, false] {
            let parent = airc
                .create_work_card_in(&room, CreateWorkCard::new(repo.clone(), "a fix", Priority::P1))
                .await
                .expect("parent card created");
            if with_pr {
                airc.link_card_pull_request_in(
                    &room,
                    airc_lib::LinkCardPullRequest {
                        card_id: parent,
                        pull_request: airc_work::model::PullRequestRef {
                            repo: repo.clone(),
                            number: 4825,
                            head: airc_lib::BranchName::new("fix/a-fix").expect("branch"),
                            base: airc_lib::BranchName::new("canary").expect("branch"),
                        },
                    },
                )
                .await
                .expect("the PR links and the card moves to Review");
            } else {
                raw_advance(&airc, parent, CardState::Review, "test").await.expect("to review");
            }
            let review = airc
                .create_work_card_in(
                    &room,
                    CreateWorkCard::new(repo.clone(), "review: a fix", Priority::P1).reviewing(parent),
                )
                .await
                .expect("review card created");
            let landed = advance_card_state_effective(&airc, review, CardState::Closed, "test", None)
                .await
                .expect("the reviewer's pass lands");
            let (_, card) = card_in_subscribed_rooms(&airc, parent).await.expect("parent on the board");
            outcomes.push((with_pr, landed, card.state));
        }
        assert_eq!(
            outcomes,
            [
                (true, CardState::Review, CardState::Review),
                (false, CardState::Closed, CardState::Closed),
            ],
            "a PR-linked card waits for its merge; a card without one closes on the pass"
        );
    }

    /// what this catches (card 57d51f97): a reviewer's send-back retires its own round
    /// and returns the parent to work — no fall-through rewrites the review card to
    /// Blocked or opens a nested round. Regression for the `return Ok(next)` restore in
    /// the verdict branch of advance.rs.
    #[tokio::test]
    pub(super) async fn a_send_back_closes_the_round_and_returns_the_parent_to_work() {
        let home = tempfile::tempdir().expect("temp airc home");
        let airc = Arc::new(
            Airc::open_with_wire_root_for_test(home.path(), home.path())
                .await
                .expect("a local airc scope opens without a daemon"),
        );
        let room = airc.join("continuum").await.expect("join the project room");
        let repo = RepoId::new("github.com/CambrianTech/continuum").expect("repo id");
        let parent = airc
            .create_work_card_in(&room, CreateWorkCard::new(repo.clone(), "a fix", Priority::P1))
            .await
            .expect("parent card created");
        raw_advance(&airc, parent, CardState::Review, "test").await.expect("to review");
        let review = airc
            .create_work_card_in(
                &room,
                CreateWorkCard::new(repo.clone(), "review: a fix", Priority::P1).reviewing(parent),
            )
            .await
            .expect("review card created");
        let landed = advance_card_state_effective(&airc, review, CardState::Blocked, "test", None)
            .await
            .expect("the reviewer's send-back lands");
        assert_eq!(landed, CardState::InProgress, "send-back returns the parent to work");
        let (_, pcard) = card_in_subscribed_rooms(&airc, parent).await.expect("parent on the board");
        assert_eq!(pcard.state, CardState::InProgress, "the parent is back in progress for revision");
        let (_, rcard) = card_in_subscribed_rooms(&airc, review).await.expect("round on the board");
        assert_eq!(
            rcard.state,
            CardState::Closed,
            "the retired round stays closed — not rewritten to Blocked, no nested round opened"
        );
    }

    /// what this catches: card 29621b9f — resolving a subscribed room's card id
    /// succeeded, then work/get looked only on the current board and refused it.
    /// Exercise the actual read path after subscribing without moving focus, and
    /// prove that a known card in a parted room remains outside the read boundary.
    #[tokio::test]
    pub(super) async fn work_get_reads_subscribed_cards_without_changing_focus() {
        let home = tempfile::tempdir().expect("temp airc home");
        let airc = Arc::new(
            Airc::open_with_wire_root_for_test(home.path(), home.path())
                .await
                .expect("a local airc scope opens without a daemon"),
        );
        let academy = airc.join("academy").await.expect("join the academy");
        let repo = RepoId::new("github.com/CambrianTech/continuum").expect("repo id");
        let mut request = CreateWorkCard::new(
            repo.clone(),
            "serve-time pin match gap",
            Priority::P1,
        );
        request.body = Some("Review the serving match against the actual source.".to_string());
        let card = airc
            .create_work_card(request)
            .await
            .expect("card created on the academy board");
        let current_room = airc.join("bench-run-1").await.expect("join the run room");
        let local_card = airc
            .create_work_card(CreateWorkCard::new(
                repo,
                "local task",
                Priority::P2,
            ))
            .await
            .expect("card created on the current board");
        airc.part_channel(Some("academy"))
            .await
            .expect("part the academy");

        let prefix = card.as_uuid().simple().to_string()[..8].to_string();
        let full_id = card.as_uuid().to_string();
        let before = airc
            .subscription_set()
            .await
            .expect("subscriptions before refused reads");
        assert!(matches!(
            WorkGet::read_card(&airc, &full_id, Uuid::nil()).await,
            Err(CommandError::NotFound(_))
        ));
        assert!(matches!(
            WorkGet::read_card(&airc, &prefix, Uuid::nil()).await,
            Err(CommandError::Invalid(_))
        ));
        assert_eq!(
            airc.subscription_set()
                .await
                .expect("subscriptions after refused reads"),
            before,
            "knowing a card id must not rejoin its room"
        );

        // This is room/join's production behavior: subscribe, preserving focus.
        airc.subscribe_room("academy")
            .await
            .expect("subscribe to the card's room");
        assert_eq!(
            airc.current_room().await.expect("current room").channel,
            current_room.channel
        );
        let before = airc
            .subscription_set()
            .await
            .expect("subscriptions before reads");
        let resolved = resolve_card_id(&airc, &prefix)
            .await
            .expect("the academy card resolves by prefix from the run room");
        assert_eq!(resolved, card);
        for id in [&full_id, &prefix] {
            let read = WorkGet::read_card(&airc, id, Uuid::nil())
                .await
                .expect("read the subscribed card");
            assert_eq!(read.id, prefix);
            assert_eq!(read.title, "serve-time pin match gap");
            assert_eq!(read.room_id, academy.channel);
            assert!(read.claimable);
            assert!(read.lease.is_none());
            assert_eq!(
                read.body.as_deref(),
                Some("Review the serving match against the actual source.")
            );
            assert_eq!(read.state, "open");
        }
        // Regression: a stale owner must not look like live contention in work/get.
        // Use the real board shape and the shared projection's clock, not a sleep.
        let horizon = board_horizon(&airc).await.expect("subscribed boards");
        // Regression44c5612e: a known UUID must not read unrelated later rooms;
        // prefix resolution still uses the complete horizon above.
        let (first_room, first_board) = horizon.boards.first().expect("first subscribed board");
        let first_card = if first_board.card(card).is_some() { card } else { local_card };
        assert!(first_board.card(first_card).is_some());
        assert!(horizon.boards.len() > 1);
        let located = board_horizon_for_card(&airc, Some(first_card)).await.expect("exact card horizon");
        assert_eq!(located.boards.len(), 1, "stop at the containing board");
        assert_eq!(located.boards[0].0.channel, first_room.channel);
        let missing = board_horizon_for_card(&airc, Some(WorkCardId::from_uuid(Uuid::new_v4())))
            .await.expect("missing exact card horizon");
        assert_eq!(missing.boards.len(), horizon.boards.len(), "a miss still searches every subscribed room");
        let boards = &horizon.boards;
        let (room, source) = boards
            .iter()
            .find_map(|(r, b)| b.card(card).map(|c| (r, c)))
            .expect("academy card");
        let mut claimed = source.clone();
        claimed.owner = Some(airc_core::PeerId::new());
        claimed.claim_id = Some(airc_work::ClaimId::from_uuid(Uuid::new_v4()));
        claimed.claim_expires_at_ms = Some(100);
        claimed.last_heartbeat_at_ms = Some(50);
        let held = WorkGet::receipt(room, &claimed, None, 99, Uuid::nil());
        // Joel, 2026-10-06: a live hold is takeable by anyone; `lease: held` is the fact she
        // judges a takeover by, `claimable` is only what work/claim will accept.
        assert!(held.claimable);
        // regression for card 5d447195: the lease reaches her as time left, not only as
        // an epoch she converts by hand; a lapsed or absent claim has none.
        assert_eq!(held.lease_remaining_secs, Some(0), "1 ms left rounds down, still held");
        assert_eq!(lease_remaining_secs(Some(1_790_644_494_881), 1_790_644_404_881), Some(90));
        assert_eq!(lease_remaining_secs(Some(100), 100), None, "lapsed at the edge");
        assert_eq!(lease_remaining_secs(None, 5), None);
        assert_eq!(held.lease.as_deref(), Some("held"));
        let lapsed = WorkGet::receipt(room, &claimed, None, 100, Uuid::nil());
        assert!(lapsed.claimable);
        assert_eq!(lapsed.lease.as_deref(), Some("expired"));
        assert_eq!(
            lapsed.owner, held.owner,
            "historical owner is preserved, not treated as an active hold"
        );
        assert_eq!(lapsed.claim_expires_at_ms, Some(100));
        assert_eq!(lapsed.last_heartbeat_at_ms, Some(50));
        assert_eq!(lapsed.observed_at_ms, 100);
        assert!(!lapsed.is_self);
        let own = WorkGet::receipt(
            room,
            &claimed,
            None,
            100,
            claimed.owner.expect("owner").as_uuid(),
        );
        assert!(
            own.is_self,
            "an expired claim still identifies its own holder"
        );
        assert_eq!(own.room, academy.name);
        claimed.state = CardState::Review;
        assert!(!WorkGet::receipt(room, &claimed, None, 100, Uuid::nil()).claimable);
        for id in [
            local_card.as_uuid().to_string(),
            short8(local_card.as_uuid()),
        ] {
            let read = WorkGet::read_card(&airc, &id, Uuid::nil())
                .await
                .expect("read the current-room card");
            assert_eq!(read.id, short8(local_card.as_uuid()));
            assert_eq!(read.title, "local task");
            assert!(read.body.is_none());
        }
        assert!(matches!(
            WorkGet::read_card(&airc, &Uuid::nil().to_string(), Uuid::nil()).await,
            Err(CommandError::NotFound(_))
        ));
        assert_eq!(
            airc.subscription_set()
                .await
                .expect("subscriptions after reads"),
            before,
            "successful and unknown-card reads must preserve subscriptions and focus"
        );
        // Regression9f1de8dd: an author can read exact signed review words without
        // holding a reviewer claim or calling the verdict-publishing verb.
        let claim = airc.claim_work_card(ClaimWorkCard { card_id: local_card, ttl_ms: 600_000 }).await.unwrap();
        let artifact = airc_work::SubmissionArtifact {
            hash: airc_blobs::ContentHash::from_bytes(b"patch"), size_bytes: 5, mime: Some("text/x-patch".into()),
        };
        let submitted = airc.submit_work_in(&current_room, airc_lib::SubmitWork {
            submission_id: airc_work::SubmissionId::new(), card_id: local_card, claim_id: claim,
            instance: "ordinary".into(), base_sha: airc_work::GitObjectId::new("a".repeat(40)).unwrap(),
            artifact: artifact.clone(),
        }).await.unwrap();
        let review_card = airc.create_work_card(CreateWorkCard::new(
            RepoId::new("github.com/CambrianTech/continuum").unwrap(), "review", Priority::P2,
        ).reviewing(local_card)).await.unwrap();
        let review_claim = airc.claim_work_card(ClaimWorkCard { card_id: review_card, ttl_ms: 600_000 }).await.unwrap();
        let words = "Resolve the selected branch, not checkout HEAD.";
        let review = airc.review_work_submission_in(&current_room, airc_lib::ReviewWorkSubmission {
            review_id: airc_work::WorkReviewId::new(), card_id: local_card,
            submission_id: submitted.submission_id, artifact,
            review_card_id: review_card, review_claim_id: review_claim,
            outcome: airc_work::WorkReviewOutcome::Failed,
            evidence: airc_work::SubmissionArtifact { hash: airc_blobs::ContentHash::from_bytes(words.as_bytes()), size_bytes: words.len() as u64, mime: Some("text/plain".into()) },
            evidence_text: Some(words.into()),
        }).await.unwrap();
        for target in [local_card, review_card] {
            let read = WorkGet::read_card(&airc, &target.to_string(), Uuid::nil()).await.unwrap();
            assert_eq!(read.reviews.len(), 1);
            assert_eq!(read.reviews[0].review_id, review.review_id.as_uuid());
            assert_eq!(read.reviews[0].reviewer, review.reviewer.as_uuid());
            assert_eq!(read.reviews[0].evidence_text.as_deref(), Some(words));
        }

    }

    // what this catches: a done that carries nothing (clean tree, HEAD at the
    // staged commit) reading as reviewable — the review-as-release-valve seen on
    // 2026-09-06/07. A dirty tree or a commit newer than staging is work; an
    // unreadable checkout is named, never assumed either way.
    #[test]
    pub(super) fn a_done_without_a_write_is_no_change_and_an_unreadable_checkout_is_named() {
        assert_eq!(
            change_verdict("", Some(100), Some(200)),
            ChangeVerdict::NoChange
        );
        assert_eq!(
            change_verdict(" M a.py\n", Some(100), Some(200)),
            ChangeVerdict::Changed
        );
        assert_eq!(
            change_verdict("", Some(300), Some(200)),
            ChangeVerdict::Changed
        );
        assert!(matches!(
            change_verdict("", None, Some(200)),
            ChangeVerdict::CouldNotLook(_)
        ));
        assert!(matches!(
            change_verdict("", Some(1), None),
            ChangeVerdict::CouldNotLook(_)
        ));
    }

    // what this catches: the agentic-surface ratchet (#333), with a name on it and in the
    // file that owns the field. `origin` is INTERNAL provenance — every `Automatic` setter
    // (`persona/work_focus.rs`, `commands/benchmark.rs`) builds `airc_work::ClaimOrigin`
    // directly and not one goes through these params — so projecting it spent catalog
    // tokens in every citizen's turn on a value she could only ever send wrong. Added
    // 2026-09-22, it moved the measured agentic surface 12027 -> 12210 past its 12100
    // ceiling and painted three PRs red off one shared base, none of which had touched
    // cognition. Re-adding it must fail HERE, in one crate's fast test, rather than in a
    // cognition ratchet nobody reading work.rs would think to connect to this field.
    #[test]
    pub(super) fn the_claim_origin_is_provenance_not_a_citizens_parameter() {
        let schema = serde_json::to_value(schemars::schema_for!(WorkClaimParams))
            .expect("the params schema projects");
        let props = schema
            .get("properties")
            .and_then(|p| p.as_object())
            .expect("an object schema with properties");
        assert!(
            props.contains_key("card_id"),
            "the citizen's real parameters still project: {props:?}"
        );
        assert!(
            !props.contains_key("origin"),
            "`origin` is internal provenance and must NOT ride the projected catalog — \
             this is the growth that broke the 12100 agentic-surface ceiling: {props:?}"
        );
        assert!(
            !schema.to_string().contains("WorkClaimOrigin"),
            "skipping the field must drop its enum DEFINITION too — the definition is \
             half the tokens: {schema}"
        );

        // The wire contract is unchanged: an omitted field is still an EXPLICIT claim,
        // which is what every citizen-issued `work/claim` is.
        let p: WorkClaimParams = serde_json::from_value(serde_json::json!({
            "card_id": "31c241e2-0000-4000-8000-000000000000"
        }))
        .expect("origin stays optional on the wire");
        assert!(matches!(p.origin, WorkClaimOrigin::Explicit));
    }

// A PASS CLOSES THE ROUNDS IT SUPERSEDES AND LEAVES NEWER ONES OPEN. Review cards are per
// round: when a pass lands on one of them, the parent closes on that round's submission and
// every OTHER open review card whose candidate is at or behind it reviews an artifact that
// just got approved — left alone they stay claimable P1s forever (the stale accumulation this
// card exists to end). A round whose candidate is NEWER than the accepted one still has a
// live question on it and must survive. The sweep resolves each candidate through its typed
// submission binding, falling back to the newest submission for legacy untyped rounds.
#[tokio::test]
async fn a_pass_closes_superseded_rounds_and_leaves_newer_ones_open() {
    let home = tempfile::tempdir().expect("temp airc home");
    let airc = Arc::new(
        Airc::open_with_wire_root_for_test(home.path(), home.path())
            .await
            .expect("airc scope opens without a daemon"),
    );
    let room = airc.join("continuum").await.expect("join the project room");
    let repo = RepoId::new("github.com/CambrianTech/continuum").expect("repo id");

    // Two rounds of work on one parent: submit S1, open review round 1; send it back and
    // resubmit as S2 (reusing the same claim — nothing in a submission requires the latest
    // claim), open review round 2. The board then holds submissions newest-first [S2, S1].
    let parent = airc
        .create_work_card_in(&room, CreateWorkCard::new(repo.clone(), "a fix", Priority::P1))
        .await
        .expect("parent card created");

    let claim = airc
        .claim_work_card(ClaimWorkCard { card_id: parent, ttl_ms: 600_000 })
        .await
        .expect("the round's work is claimed");
    let artifact = airc_work::SubmissionArtifact {
        hash: airc_blobs::ContentHash::from_bytes(b"patch"),
        size_bytes: 5,
        mime: Some("text/x-patch".into()),
    };

    let s1 = airc
        .submit_work_in(
            &room,
            airc_lib::SubmitWork {
                submission_id: airc_work::SubmissionId::new(),
                card_id: parent,
                claim_id: claim,
                instance: "ordinary".into(),
                base_sha: airc_work::GitObjectId::new("a".repeat(40)).unwrap(),
                artifact: artifact.clone(),
            },
        )
        .await
        .expect("round 1 submitted");
    let r1 = airc
        .ensure_submission_review_in(
            &room,
            parent,
            s1.submission_id,
            CreateWorkCard::new(repo.clone(), "review: round 1", Priority::P1),
        )
        .await
        .expect("round 1 gets its review card");

    let s2 = airc
        .submit_work_in(
            &room,
            airc_lib::SubmitWork {
                submission_id: airc_work::SubmissionId::new(),
                card_id: parent,
                claim_id: claim,
                instance: "ordinary".into(),
                base_sha: airc_work::GitObjectId::new("b".repeat(40)).unwrap(),
                artifact,
            },
        )
        .await
        .expect("round 2 resubmitted on the same claim");
    let r2 = airc
        .ensure_submission_review_in(
            &room,
            parent,
            s2.submission_id,
            CreateWorkCard::new(repo.clone(), "review: round 2", Priority::P1),
        )
        .await
        .expect("round 2 gets its review card");
    assert_ne!(r1, r2, "each round has its own deterministic review card");

    // The reviewer passes the NEWER round: accepted candidate is S2 (index 0), so R1's
    // candidate S1 sits behind it and must be closed by the sweep — while the verdict
    // itself lands on R2, closing it too. No stale claimable P1s survive a pass.
    let landed = advance_card_state_effective(&airc, r2, CardState::Closed, "test", None)
        .await
        .expect("a pass on the newest round lands");
    assert_eq!(
        landed,
        CardState::Closed,
        "the effective state is the parent's: no PR, so it closes on the pass"
    );
    let (_, pcard) = card_in_subscribed_rooms(&airc, parent).await.expect("parent on the board");
    assert_eq!(pcard.state, CardState::Closed, "the parent closed on the pass");
    let (_, r1card) = card_in_subscribed_rooms(&airc, r1)
        .await
        .expect("round 1 on the board");
    assert_eq!(
        r1card.state,
        CardState::Closed,
        "the superseded round was swept — no stale claimable P1 survives a pass"
    );
    let (_, r2card) = card_in_subscribed_rooms(&airc, r2)
        .await
        .expect("round 2 on the board");
    assert_eq!(
        r2card.state,
        CardState::Closed,
        "the verdict's own round stays closed — not rewritten by the fall-through this fix removes"
    );
}

// A PASS DOES NOT CLOSE ROUNDS THAT REVIEW NEWER WORK. The sweep must tie closing to the
// accepted submission (blocking #3 of 99c64d55): a round whose bound candidate is newer
// than what just passed still has a live question on it, and closing it would discard a
// judgment of work that postdates the pass — exactly the over-closing this fix exists to
// prevent. Passing the OLDER round here settles S1 (index 1) while R2 reviews S2 (index
// 0): only the verdict's own card closes behind it; R2 must survive.
#[tokio::test]
async fn a_pass_leaves_rounds_reviewing_newer_work_open() {
    let home = tempfile::tempdir().expect("temp airc home");
    let airc = Arc::new(
        Airc::open_with_wire_root_for_test(home.path(), home.path())
            .await
            .expect("airc scope opens without a daemon"),
    );
    let room = airc.join("continuum").await.expect("join the project room");
    let repo = RepoId::new("github.com/CambrianTech/continuum").expect("repo id");

    // The same two-round board as its sibling test: submissions newest-first [S2, S1],
    // typed review rounds R1 on S1 and R2 on S2.
    let parent = airc
        .create_work_card_in(&room, CreateWorkCard::new(repo.clone(), "a fix", Priority::P1))
        .await
        .expect("parent card created");

    let claim = airc
        .claim_work_card(ClaimWorkCard { card_id: parent, ttl_ms: 600_000 })
        .await
        .expect("the round's work is claimed");
    let artifact = airc_work::SubmissionArtifact {
        hash: airc_blobs::ContentHash::from_bytes(b"patch"),
        size_bytes: 5,
        mime: Some("text/x-patch".into()),
    };

    let s1 = airc
        .submit_work_in(
            &room,
            airc_lib::SubmitWork {
                submission_id: airc_work::SubmissionId::new(),
                card_id: parent,
                claim_id: claim,
                instance: "ordinary".into(),
                base_sha: airc_work::GitObjectId::new("a".repeat(40)).unwrap(),
                artifact: artifact.clone(),
            },
        )
        .await
        .expect("round 1 submitted");
    let r1 = airc
        .ensure_submission_review_in(
            &room,
            parent,
            s1.submission_id,
            CreateWorkCard::new(repo.clone(), "review: round 1", Priority::P1),
        )
        .await
        .expect("round 1 gets its review card");

    let s2 = airc
        .submit_work_in(
            &room,
            airc_lib::SubmitWork {
                submission_id: airc_work::SubmissionId::new(),
                card_id: parent,
                claim_id: claim,
                instance: "ordinary".into(),
                base_sha: airc_work::GitObjectId::new("b".repeat(40)).unwrap(),
                artifact,
            },
        )
        .await
        .expect("round 2 resubmitted on the same claim");
    let r2 = airc
        .ensure_submission_review_in(
            &room,
            parent,
            s2.submission_id,
            CreateWorkCard::new(repo.clone(), "review: round 2", Priority::P1),
        )
        .await
        .expect("round 2 gets its review card");
    assert_ne!(r1, r2, "each round has its own deterministic review card");

    // The reviewer passes the OLDER round: accepted candidate is S1 (index 1). R2's
    // candidate S2 sits AHEAD of it — newer work than what just passed — so the sweep
    // must leave R2 open for its own verdict.
    let landed = advance_card_state_effective(&airc, r1, CardState::Closed, "test", None)
        .await
        .expect("a pass on the older round lands");
    assert_eq!(
        landed,
        CardState::Closed,
        "the effective state is the parent's: no PR, so it closes on the pass"
    );
    let (_, r1card) = card_in_subscribed_rooms(&airc, r1)
        .await
        .expect("round 1 on the board");
    assert_eq!(r1card.state, CardState::Closed, "the verdict's own round closed");
    let (_, r2card) = card_in_subscribed_rooms(&airc, r2)
        .await
        .expect("round 2 on the board");
    assert_ne!(
        r2card.state,
        CardState::Closed,
        "the round reviewing newer work stays open — a pass must not close it"
    );
}