#![cfg(test)]

    use super::*;
    use std::sync::Arc;
    use crate::ai::adapter::AIProviderAdapter;
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

    // what this catches (card 7e3e8070, reshaped by 3bd860ba): a write-less stretch on a held card
    // surfaces the advisory checkpoint (a note, a blocker, who can help; never a release demand),
    // and a hold whose start is unknown shows none rather than a guessed count. The count itself is
    // her action log's (`working_memory::tests::action_log`).
    #[test]
    pub(super) fn six_acts_without_a_write_gate_the_work_turn_and_an_edit_resets_it() {
        use crate::persona::work_burst::{held_work_burst_gated, WRITE_OR_RELEASE_AFTER_ACTS};
        let text = held_work_burst_gated(Some(WRITE_OR_RELEASE_AFTER_ACTS));
        assert!(text.contains("[progress checkpoint]"), "{text}");
        assert!(text.contains("PASS: blocked"), "{text}");
        // regression (Kimi, d33e928a, 2026-09-26): a citizen mid-diagnosis read past the
        // one act that records progress, a note; the checkpoint names it.
        assert!(text.contains("work/note"), "{text}");
        assert!(!held_work_burst_gated(Some(1)).contains("[progress checkpoint]"), "no checkpoint after an edit");
        assert!(!held_work_burst_gated(None).contains("[progress checkpoint]"), "an unknown hold start shows none");
        assert!(WRITE_OR_RELEASE_AFTER_ACTS >= 4, "the gate is not a hair trigger");
    }

    // what this catches: a work-turn stimulus that commands instead of offering (the pass and its
    // three reasons are hers, and the settle edge reads them), or one that lists her cards again
    // beside her held-work items (each card is its own item there, with what she can do on it).
    #[test]
    pub(super) fn the_work_turn_keeps_the_choice_hers_and_points_at_her_cards() {
        let burst = crate::persona::work_burst::held_work_burst_gated(None);
        assert!(burst.contains("[my work]"), "her cards are her held-work items: {burst}");
        assert!(!burst.contains("- card "), "no card is listed again in the stimulus: {burst}");
        assert!(burst.contains("PASS"), "the pass choice stays hers: {burst}");
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
        assert!(turns[0].content.contains("`commands_list`"), "the discovery verb as her list offers it");

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
