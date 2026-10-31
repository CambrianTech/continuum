#![cfg(test)]

    use super::*;
    use std::sync::Arc;
    use crate::ai::adapter::AIProviderAdapter;
    use crate::persona::work_burst::{held_work_burst, own_recent_thoughts};
    use crate::ai::HeuristicInferenceAdapter;
    use crate::persona::instance_manager::PersonaInstanceInfo;
    use crate::persona::airc_citizen::StubAircCitizen;
    use crate::persona::identity_provider::PersonaIdentitySource;
    use crate::experience::role::RoleId;
    use crate::cognition::conversation::scripted_conversation::ScriptedConversation;
    use crate::persona::scripted_conversation::ScriptedCitizen;
    use crate::persona::supervisor::HostedPersona;
    use std::path::PathBuf;

    // What this catches: a paused generator must not hold a short answer until
    // another token arrives; the same live rail must filter reasoning and close in order.
    #[tokio::test]
    pub(super) async fn token_forwarder_flushes_during_pause_and_closes_in_order() {
        use crate::ai::adapter::GenerationChunk;
        use crate::persona::stream_rail;
        use std::time::Duration;

        async fn next_for_room(
            rx: &mut tokio::sync::broadcast::Receiver<stream_rail::StreamDelta>,
            room: &str,
        ) -> stream_rail::StreamDelta {
            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let delta = rx.recv().await.expect("live rail remains available");
                    if delta.room_id == room {
                        return delta;
                    }
                }
            })
            .await
            .expect("buffered output arrives without further input or channel closure")
        }

        let room = Uuid::new_v4().to_string();
        let mut output = stream_rail::subscribe();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let owner = spawn_token_forwarder(
            rx, None, "stream-test".into(), Some(room.clone()), Some("sender".into()),
        );
        let start = next_for_room(&mut output, &room).await;
        assert!(start.token.is_empty() && !start.done);
        tx.send(GenerationChunk::Reasoning("private fixture".into())).expect("owner alive");
        tx.send(GenerationChunk::Token("First".into())).expect("owner alive");
        // Keep tx open and send nothing else: this is the formerly stuck path.
        let first = next_for_room(&mut output, &room).await;
        assert_eq!(first.token, "First");
        assert_eq!(first.seq, 0);
        assert!(!first.done);
        tx.send(GenerationChunk::Token("Second".into())).expect("owner alive");
        tx.send(GenerationChunk::Token("Third".into())).expect("owner alive");
        drop(tx);
        owner.await.expect("forwarder closes cleanly");
        assert_eq!(start.stream_id, first.stream_id);
        let mut tail = String::new();
        let mut expected_seq = 1;
        loop {
            let delta = next_for_room(&mut output, &room).await;
            assert_eq!(delta.stream_id, first.stream_id);
            assert_eq!(delta.seq, expected_seq);
            if delta.done {
                assert!(delta.token.is_empty());
                break;
            }
            tail.push_str(&delta.token);
            expected_seq += 1;
        }
        assert_eq!(tail, "SecondThird");
    }

    // what this catches: a turn in her mind room typing onto the room rail (sink 7 of
    // PRIVACY-OF-THOUGHT.md). The forwarder drains her tokens and closes, and not one
    // frame for her mind room reaches the rail.
    #[tokio::test]
    pub(super) async fn a_turn_in_her_mind_room_streams_no_tokens() {
        use crate::ai::adapter::GenerationChunk;
        use crate::persona::stream_rail;
        let her = Uuid::new_v4();
        let mind = crate::cognition::mind_room::mind_room_id(her).to_string();
        let mut output = stream_rail::subscribe();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let owner = spawn_token_forwarder(rx, None, "Kimi".into(), Some(mind.clone()), Some(her.to_string()));
        tx.send(GenerationChunk::Token("a private thought".into())).expect("owner alive");
        drop(tx);
        owner.await.expect("forwarder drains and closes");
        let leaked = tokio::time::timeout(std::time::Duration::from_millis(600), async {
            loop {
                match output.recv().await {
                    Ok(delta) if delta.room_id == mind => return true,
                    Ok(_) => continue,
                    Err(_) => return false,
                }
            }
        })
        .await
        .unwrap_or(false);
        assert!(!leaked, "her mind room: no frame on the rail");
    }

    // What this catches (e731576c): publication, not a later work-turn return,
    // owns the speech ring; failures and other rooms cannot become its history.
    #[tokio::test]
    pub(super) async fn successful_publication_records_speech_once_in_the_actual_room() {
        use crate::cognition::deliberation_budget::recent_own_speech;
        use crate::cognition::conversation::scripted_conversation::ScriptedConversation;

        let peer = crate::identity::PeerId::from_uuid(Uuid::new_v4());
        let room = Uuid::new_v4();
        let other_room = Uuid::new_v4();
        let conversation = ScriptedConversation::new();
        publish_own_speech(&conversation, peer, room, "The tests passed.")
            .await
            .expect("scripted publication succeeds");
        assert_eq!(recent_own_speech(peer, room), ["The tests passed."]);
        assert!(recent_own_speech(peer, other_room).is_empty());

        let refused = ScriptedConversation::new().with_say_failure("room unavailable");
        assert_eq!(
            publish_own_speech(&refused, peer, room, "This was never delivered.").await,
            Err("room unavailable".to_string())
        );
        assert!(refused.said_in().is_empty());
        assert_eq!(recent_own_speech(peer, room), ["The tests passed."]);

        publish_own_speech(&conversation, peer, other_room, "Other activity report.")
            .await
            .expect("scripted publication succeeds");
        publish_own_speech(&conversation, peer, room, "The tests failed.")
            .await
            .expect("scripted correction succeeds");
        assert_eq!(
            recent_own_speech(peer, room),
            ["The tests passed.", "The tests failed."]
        );
        assert_eq!(
            recent_own_speech(peer, other_room),
            ["Other activity report."]
        );
        assert_eq!(
            conversation.said_in(),
            vec![
                (room, "The tests passed.".to_string()),
                (other_room, "Other activity report.".to_string()),
                (room, "The tests failed.".to_string()),
            ]
        );
    }

    // what this catches: the resume block carries HER newest thoughts only, oldest
    // first, clipped — never another citizen's line, never a receipt.
    // what this catches (glass-boxed 2026-09-06, Atlas on django-16899): the newest
    // thought head-clipped to its orientation — "Let me carefully parse where I am…" —
    // so every turn re-ran git status and the conclusion she had reached never
    // reached her next turn. The newest thought's TAIL is the resume point.
    #[test]
    pub(super) fn the_newest_thought_resumes_from_its_conclusion_not_its_orientation() {
        let me = Uuid::new_v4();
        let row = |ms, text: &str| crate::persona::durable_history::RoomRow {
            id: Uuid::new_v4(),
            sender: me,
            occurred_at_ms: ms,
            media: Vec::new(),
            text: text.to_string(),
        };
        let long = format!(
            "💭 Let me carefully parse where I actually am right now. {} So the fix is: include the index in the label at checks.py:1040.",
            "First, the ground truth from my own board and workspace. ".repeat(6)
        );
        let rows = vec![row(1, "💭 earlier orientation"), row(2, &long)];
        let state = own_recent_thoughts(&rows, me, 2, 80);
        assert_eq!(state.len(), 2);
        assert!(
            state[1].ends_with("include the index in the label at checks.py:1040."),
            "{state:?}"
        );
        assert!(
            !state[1].contains("carefully parse"),
            "the orientation is what got dropped: {state:?}"
        );
    }

    // what this catches (card 7e3e8070, reshaped by 3bd860ba): six acts on a held card
    // with no file change surface the advisory checkpoint (a note, a blocker, who can
    // help; never a release demand); an edit receipt resets the count and it stays away.
    #[test]
    pub(super) fn six_acts_without_a_write_gate_the_work_turn_and_an_edit_resets_it() {
        use crate::persona::work_burst::{acts_since_last_write, held_work_burst_gated, CardProgress, WRITE_OR_RELEASE_AFTER_ACTS};
        let me = Uuid::new_v4();
        let row = |ms, text: &str| crate::persona::durable_history::RoomRow {
            id: Uuid::new_v4(),
            sender: me,
            occurred_at_ms: ms,
            media: Vec::new(),
            text: text.to_string(),
        };
        let looping = vec![
            row(1, "💭 reading ⚙ code/read a.py ✓ ⚙ code/search x ✓"),
            row(2, "💭 checking ⚙ code/run pytest ✓ ⚙ code/git/status ✓"),
            row(3, "💭 more ⚙ code/read b.py ✓ ⚙ code/shell ls ✓"),
        ];
        assert_eq!(acts_since_last_write(&looping, me), 6);
        let text = held_work_burst_gated(
            &[],
            &[],
            acts_since_last_write(&looping, me),
            &CardProgress::default(),
        );
        assert!(text.contains("[progress checkpoint]"), "{text}");
        assert!(text.contains("PASS: blocked"), "{text}");
        // regression (Kimi, d33e928a, 2026-09-26): a citizen mid-diagnosis read past the
        // one act that records progress, a note; the checkpoint names it.
        assert!(text.contains("work/note"), "{text}");

        let mut edited = looping.clone();
        edited.push(row(
            4,
            "💭 fixing ⚙ code/write witness_report.txt ✓ ⚙ code/run pytest ✓",
        ));
        assert_eq!(
            acts_since_last_write(&edited, me),
            1,
            "an edit resets the count"
        );
        // regression for the 2026-09-11 pull→release loop: a claim (the pull) after a
        // release starts a fresh count, so the new hold opens without the checkpoint.
        let mut reclaimed = looping.clone();
        reclaimed.push(row(4, "💭 next ⚙ work/release abcd ✓"));
        reclaimed.push(row(5, "💭 pulled ⚙ work/claim ef01 ✓ ⚙ code/read c.py ✓"));
        assert_eq!(
            acts_since_last_write(&reclaimed, me),
            1,
            "a claim is a hold boundary: only the act after it counts"
        );
        let text = held_work_burst_gated(
            &[],
            &[],
            acts_since_last_write(&edited, me),
            &CardProgress::default(),
        );
        assert!(
            !text.contains("[progress checkpoint]"),
            "no checkpoint after an edit: {text}"
        );
        assert!(
            WRITE_OR_RELEASE_AFTER_ACTS >= 4,
            "the gate is not a hair trigger"
        );
    }

    // what this catches (card 516738b4): a work turn that re-reads what it read.
    // Her ⚙ receipts already say which files she read and what she ran; the block
    // must lead with them, de-duplicated, newest last, and say nothing on a fresh card.
    #[test]
    pub(super) fn a_held_card_carries_what_she_already_read_and_ran() {
        use crate::persona::work_burst::{card_progress, held_work_burst_gated, progress_line, CardProgress};
        let me = Uuid::new_v4();
        let row = |ms, text: &str| crate::persona::durable_history::RoomRow {
            id: Uuid::new_v4(),
            sender: me,
            occurred_at_ms: ms,
            media: Vec::new(),
            text: text.to_string(),
        };
        let rows = vec![
            row(2, "💭 looking ⚙ code/read django/forms/fields.py ✓ ⚙ code/search MultiValueField ✓"),
            row(1, "💭 first ⚙ work/get de33e1d6 ✓"),
            row(3, "💭 again ⚙ code/read django/forms/fields.py ✓ ⚙ code/run pytest tests/forms_tests -k Multi ✗"),
            row(4, "💭 fix ⚙ code/edit django/forms/fields.py ✓ ⚙ code/run pytest tests/forms_tests -k Multi ✓"),
        ];
        let p = card_progress(&rows, me);
        assert_eq!(p.acts, 7);
        assert_eq!(p.writes, 1);
        assert_eq!(
            p.read,
            vec!["django/forms/fields.py".to_string()],
            "paths only, de-duplicated"
        );
        assert_eq!(
            p.searched,
            vec!["MultiValueField".to_string()],
            "a search term is not a read"
        );
        // Successful writes survive as concrete receipts; failed edits and
        // another citizen's writes must not become her accomplishments.
        assert_eq!(p.wrote, vec!["code/edit django/forms/fields.py"]);
        let mut write_rows = rows.clone();
        write_rows.push(row(5, "⚙ code/write failed.rs ✗"));
        write_rows.push(row(6, "⚙ code/edit django/forms/fields.py ✓"));
        write_rows.push(row(7, "⚙ work/note de33e1d6 ✓"));
        let mut others = row(8, "⚙ code/write someone-elses.rs ✓");
        others.sender = Uuid::new_v4();
        write_rows.push(others);
        let writes = card_progress(&write_rows, me);
        assert_eq!(writes.wrote, vec!["code/edit django/forms/fields.py", "work/note de33e1d6"]);
        let receipt = progress_line(&writes);
        assert!(receipt.contains("Successful write receipts: code/edit django/forms/fields.py; work/note de33e1d6."));
        assert!(!receipt.contains("failed.rs"));
        assert!(!receipt.contains("someone-elses.rs"));
        assert_eq!(p.ran.len(), 2);
        assert!(p.ran[1].ends_with('✓'), "{:?}", p.ran);
        let block = held_work_burst_gated(&[], &[], 0, &p);
        assert!(
            block.contains("[progress] Recent work receipts:"),
            "{block}"
        );
        assert!(
            block.contains("Already read: django/forms/fields.py."),
            "{block}"
        );
        assert!(
            block.contains("Already searched: MultiValueField."),
            "{block}"
        );
        assert!(
            block.find("[progress]").unwrap() < block.find("Your workspace holds").unwrap(),
            "the note leads"
        );
        assert!(
            progress_line(&CardProgress::default()).is_empty(),
            "a fresh card carries no note"
        );
        assert!(
            !held_work_burst_gated(&[], &[], 0, &CardProgress::default()).contains("[progress]")
        );
    }

    #[test]
    pub(super) fn her_last_thoughts_lead_the_work_turn_oldest_first() {
        use crate::persona::durable_history::RoomRow;
        let me = Uuid::new_v4();
        let other = Uuid::new_v4();
        let row = |sender, ms, text: &str| RoomRow {
            id: Uuid::new_v4(),
            sender,
            occurred_at_ms: ms,
            media: Vec::new(),
            text: text.to_string(),
        };
        let rows = vec![
            row(
                me,
                3,
                "💭 lines 107-152: the bug is the total_degree branch",
            ),
            row(other, 4, "💭 not hers"),
            row(me, 1, "💭 let me look at itermonomials"),
            row(me, 2, "⚙ code/read ✓"),
            row(
                me,
                5,
                "💭 let me refocus and actually do work on this card now, really",
            ),
        ];
        let state = own_recent_thoughts(&rows, me, 2, 40);
        assert_eq!(state.len(), 2);
        assert!(state[0].starts_with("💭 lines 107-152"), "{state:?}");
        // The newest thought keeps its TAIL — the conclusion — not its opening.
        assert!(
            state[1].starts_with('…') && state[1].ends_with("on this card now, really"),
            "newest keeps its conclusion: {state:?}"
        );
        let text = held_work_burst(&[], &state);
        assert!(text.contains("resume from them"));
        assert!(text.contains("lines 107-152"));
    }

    #[test]
    pub(super) fn held_work_burst_names_cards_and_keeps_the_choice_hers() {
        use airc_work::{CardState, Priority, RepoId, WorkCardId};
        let card = airc_lib::WorkCard {
            card_id: WorkCardId::new(),
            repo: RepoId::new("acme/continuum").expect("valid repo id in fixture"),
            title: "PROJECT [swe] psf__requests-2148".to_string(),
            body: None,
            priority: Priority::P1,
            lane_id: None,
            state: CardState::InProgress,
            owner: None,
            claim_provenance: None,
            claim_id: None,
            claim_expires_at_ms: None,
            last_heartbeat_at_ms: None,
            pull_request: None,
            created_by: airc_core::PeerId::new(),
            created_at_ms: 1_000_000,
            updated_at_ms: 1_000_000,
            reviews: None,
            submissions: Vec::new(),
            last_submission_rejection: None,
        };
        let id8: String = airc_core::shown_id::shown_form(card.card_id.as_uuid());
        let burst = held_work_burst(&[&card], &[]);
        assert!(burst.contains(&id8), "short id must appear: {burst}");
        assert!(
            burst.contains("psf__requests-2148"),
            "title must appear: {burst}"
        );
        // The choice stays hers — she PASSES to conclude, and the pass carries a
        // reason (done/blocked/nothing) the deterministic settle edge reads. A
        // burst that loses the pass-clause becomes a command; one that loses the
        // reason words leaves the edge unable to tell done from blocked.
        assert!(
            burst.contains("PASS"),
            "the pass choice stays hers: {burst}"
        );
        assert!(
            burst.contains("done") && burst.contains("blocked") && burst.contains("nothing"),
            "the pass must name its three reasons for the settle edge: {burst}"
        );
    }

    // what this catches: a turn composed WITHOUT its room — the defect that made every
    // room-scoped grounding source (room-kanban, room-roster, room-doctrine, room-board)
    // abstain at once, so a citizen perceived no board, no roster, no doctrine and no wall
    // and correctly reported "there are no open tasks available".
    //
    // MEASURED 2026-08-06 from the live probe file (`rag.room_gate.abstain`): 504 of 564
    // abstains carried `turn_room = 00000000-0000-0000-0000-000000000000` — 89% of turns —
    // while `bound_room` was CORRECT. Not a room mismatch: the turn simply carried no room.
    //
    // #127 built the gate, the `for_persona_in_room` constructor, the probe AND the doc
    // comment, then closed as completed while THIS caller still passed no room. One source
    // (`persona/airc_source.rs`) papered over it locally by deriving the room from transcript
    // events and left a comment saying so, which is exactly why the other four starved
    // unnoticed. A grep-level guard is the right shape because the regression is a CALLER
    // forgetting, not a logic error any unit test would reach.
    #[test]
    pub(super) fn every_live_compose_for_turn_stamps_the_room_never_none() {
        let src = [include_str!("../burst_fingerprint.rs"), include_str!("../serve_persona_loop_inner.rs")].concat();
        let calls: Vec<&str> = src
            .match_indices("compose_for_turn(")
            .map(|(i, _)| &src[i..(i + 160).min(src.len())])
            .collect();
        assert!(
            !calls.is_empty(),
            "no compose_for_turn call found — did the live turn path move? \
             This guard must follow it."
        );
        for c in &calls {
            assert!(
                !c.contains(", None)"),
                "a LIVE compose_for_turn passes no room: every room-scoped source will \
                 abstain and she goes blind to board/roster/doctrine/wall at once (#331). \
                 Pass Some(turn_room) or Some(ctx.identity.default_room).\nsite: {c}"
            );
        }
    }

    // what this catches: THE six-turn loop, glass-boxed from Benchy's live prompt capture
    // on 2026-08-06. `room-kanban` did not deliver that turn (grounding sits last in the
    // budget queue), and the anchor rendered that absence as the assertion "No open cards
    // are visible on this room's board right now". She then said exactly that in-room, six
    // times — while `work/list()` in her OWN working memory, in the SAME prompt, listed a
    // full board. She trusted the authoritative anchor over her own receipt.
    //
    // "The board is empty" and "I never read the board" are different facts, and only one
    // is knowable from an absent source. The anchor must be SILENT when the source did not
    // speak, so her own tool receipt stays the only board claim in the prompt.
    // regression for the 2026-08-06 anchor/receipt contradiction
    #[test]
    pub(super) fn an_absent_board_source_yields_no_anchor_at_all() {
        use crate::cognition::rag_budget::{RagDelivery, RagItem};

        let delivery = |source_id: &str, items: Vec<RagItem>| RagDelivery {
            source_id: source_id.to_string(),
            items,
            tokens_used: 0,
            truncated: false,
        };
        // The ENUM, not a spelling. If the wire form ever changes, this fixture changes
        // with it automatically instead of quietly testing a string that no longer occurs.
        let card = |state: airc_work::CardState| RagItem {
            key: String::new(),
            summary: None,
            pointer: None,
            content: "#00f2a380 self-heal #2: receive-binding re-derive".to_string(),
            tokens: 8,
            metadata: serde_json::json!({ "card_id": "00f2a380", "state": state }),
        };

        // The live shape: other sources delivered, the board source did NOT.
        let without_board = vec![delivery("conversation", vec![])];
        assert_eq!(
            work_board_anchor(&without_board),
            "",
            "a source that never spoke must not be quoted as evidence of an empty board"
        );

        // And the guard must keep that silence out of perception entirely — a blank
        // opaque turn is its own kind of noise.
        let mut turns: Vec<crate::cognition::workspace::BurstTurn> = Vec::new();
        push_work_board_anchor(&mut turns, &without_board);
        assert!(
            turns.is_empty(),
            "no anchor means no turn, not an empty one"
        );

        // The board source SPOKE and the board really is empty → the honest-empty line is
        // still correct and must survive. Silencing that would trade one lie for another.
        let empty_board = vec![delivery("room-kanban", vec![])];
        assert!(
            work_board_anchor(&empty_board).contains("No open cards are visible"),
            "a board that was READ and is empty is a real fact worth stating"
        );

        // Cards present → the anchor names real work, as before.
        let with_cards = vec![delivery(
            "room-kanban",
            vec![card(airc_work::CardState::Claimed)],
        )];
        let anchor = work_board_anchor(&with_cards);
        assert!(
            anchor.contains("Open work exists"),
            "delivered cards must still produce the concrete anchor: {anchor}"
        );
    }

    // what this catches: #147 — a wake with NO conversation must carry the
    // orientation briefing, not a void (the void gets filled with imagined
    // history / generic-assistant masks, observed all morning 2026-07-12).
    // A wake WITH conversation must NOT carry it (orientation is for voids).
    #[test]
    pub(super) fn empty_wake_carries_the_briefing_and_populated_wake_does_not() {
        let turns = build_workspace_turns(&[], "peer-1", "Asha", None);
        assert_eq!(turns.len(), 1);
        assert!(
            turns[0].content.starts_with("[wake] You are Asha"),
            "got: {}",
            turns[0].content
        );
        assert!(turns[0].content.contains("list_commands"));

        let delivery = crate::cognition::rag_budget::RagDelivery {
            source_id: "airc".to_string(),
            items: vec![crate::cognition::rag_budget::RagItem {
                key: String::new(),
                summary: None,
                pointer: None,
                content: "hello there".to_string(),
                tokens: 0,
                metadata: serde_json::json!({ "peer_id": "peer-2" }),
            }],
            tokens_used: 0,
            truncated: false,
        };
        let turns = build_workspace_turns(&[delivery], "peer-1", "Asha", None);
        assert!(
            !turns.iter().any(|t| t.content.starts_with("[wake]")),
            "populated wake must not carry the briefing"
        );
    }

    // what this catches: #125 slice 1 (Joel 2026-08-03: "should never be a mind
    // from scratch — the whole point is the opposite"). An empty wake with LIVE
    // claims must lead with the THREAD (her held cards) BEFORE the room
    // description, and a lost-claim fact must ride the briefing as the thread's
    // honest tail — never a room-first orientation that reduces her life to
    // verb-filler.
    #[test]
    pub(super) fn empty_wake_leads_with_her_work_thread_before_the_room() {
        let work = crate::cognition::rag_budget::RagDelivery {
            source_id: "active-work".to_string(),
            items: vec![
                crate::cognition::rag_budget::RagItem {
                    key: String::new(),
                    summary: None,
                    pointer: None,
                    content: "card 20fe404a \"macOS install acceptance checks\" (P1, owner YOU)"
                        .to_string(),
                    tokens: 0,
                    metadata: serde_json::json!({}),
                },
                crate::cognition::rag_budget::RagItem {
                    key: String::new(),
                    summary: None,
                    pointer: None,
                    content: "[work] Your claim on card 33a0e899 \"conway\" is no longer held by you (lease expired or released)."
                        .to_string(),
                    tokens: 0,
                    metadata: serde_json::json!({ "fact": "claim_lost" }),
                },
            ],
            tokens_used: 0,
            truncated: false,
        };
        let turns = build_workspace_turns(&[work], "peer-1", "Asha", None);
        assert_eq!(turns.len(), 1);
        let c = &turns[0].content;
        let thread = c.find("mid-work").expect("thread present");
        let lost = c.find("no longer held").expect("lost-claim tail present");
        let room = c.find("Nothing has been said").expect("room line present");
        assert!(thread < room, "thread must LEAD the room description: {c}");
        assert!(
            lost < room,
            "lost-claim tail rides before the room line: {c}"
        );
        assert!(
            !c.contains("No work of yours is on record"),
            "the no-thread line must not appear when a thread exists: {c}"
        );
    }

    /// Drive-coupling guard for the never-stop heartbeat's wake gate.
    /// what this catches: `burst_fingerprint` must treat the persona's OWN active
    /// work as interior DRIVE — a new/changed work card moves the fingerprint so the
    /// tick fires to advance it — WITHOUT reopening the self-talk flood (own CHAT
    /// posts stay excluded; observed live 2026-06-22 as 19 self-talk cycles). This
    /// is the AUTONOMOUS-PROJECT-LOOP fix: the external-only gate was backwards for
    /// thought. [[alignment-through-mutual-self-interest]].
    pub(super) mod burst_fingerprint_drive {
        use super::super::burst_fingerprint;
        use crate::cognition::rag_budget::{RagDelivery, RagItem};
        use serde_json::json;

        const ME: &str = "me-peer";

        fn delivery(source_id: &str, items: Vec<RagItem>) -> RagDelivery {
            RagDelivery {
                source_id: source_id.to_string(),
                items,
                tokens_used: 0,
                truncated: false,
            }
        }
        fn chat(peer_id: &str, content: &str) -> RagItem {
            RagItem {
                key: String::new(),
                summary: None,
                pointer: None,
                content: content.to_string(),
                tokens: 0,
                metadata: json!({ "peer_id": peer_id }),
            }
        }
        fn card(content: &str) -> RagItem {
            RagItem {
                key: String::new(),
                summary: None,
                pointer: None,
                content: content.to_string(),
                tokens: 0,
                metadata: json!({}),
            }
        }

        #[test]
        fn own_chat_inert_but_own_work_wakes_the_heartbeat() {
            let base = vec![delivery("airc", vec![chat("other", "hi")])];
            let fp0 = burst_fingerprint(&base, ME);

            // My own chat post → excluded → fingerprint unchanged (no self-talk spiral).
            let with_my_chat = vec![delivery(
                "airc",
                vec![chat("other", "hi"), chat(ME, "I'll look into it")],
            )];
            assert_eq!(
                fp0,
                burst_fingerprint(&with_my_chat, ME),
                "own chat must not move the fingerprint (else speech re-triggers self-talk)"
            );

            // My active work card appears → fingerprint MUST change (interior drive).
            let with_my_work = vec![
                delivery("airc", vec![chat("other", "hi")]),
                delivery(
                    "active-work",
                    vec![card("card abc [InProgress] \"impl X\"")],
                ),
            ];
            assert_ne!(
                fp0,
                burst_fingerprint(&with_my_work, ME),
                "own active work must wake the heartbeat to advance it"
            );
        }

        #[test]
        fn work_progress_refires_the_next_slice() {
            // A card's state change is real progress → must re-fire so she continues.
            let in_progress = vec![delivery(
                "active-work",
                vec![card("card abc [InProgress] \"impl X\"")],
            )];
            let done = vec![delivery(
                "active-work",
                vec![card("card abc [Done] \"impl X\"")],
            )];
            assert_ne!(
                burst_fingerprint(&in_progress, ME),
                burst_fingerprint(&done, ME),
                "card state change (progress) must move the fingerprint to continue the loop"
            );
        }
    }

mod part_2;
mod part_3;
use part_3::*;
mod part_4;
