#![cfg(test)]

    /// what this catches: a remote peer's message must render with their roster
    /// display name (`Joel: ...`), not the raw peer UUID. Regression for the live
    /// glass-box finding 2026-06-29 — Asha saw `7711fe60-...: <text>` and could only
    /// recover the human's name because it was signed in the body; an unsigned peer
    /// would have been an indistinguishable UUID (the confabulation root cause). The
    /// roster delivery already in the same slice carries peer_id→display_name; the
    /// burst must consume it. Own posts stay attributed to `agent_name`; a peer with
    /// no roster entry falls back to its id (honest, never a fabricated name).
    pub(super) mod workspace_burst_names {
        use super::super::super::build_workspace_turns;
        use crate::cognition::workspace::Burst;
        use crate::cognition::rag_budget::{RagDelivery, RagItem};
        use serde_json::json;

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
                metadata: json!({ "peer_id": peer_id, "occurred_at_ms": 1u64 }),
            }
        }
        fn roster(peer_id: &str, name: &str) -> RagItem {
            RagItem {
                key: String::new(),
                summary: None,
                pointer: None,
                content: format!("{name} [claude]"),
                tokens: 0,
                metadata: json!({ "peer_id": peer_id, "display_name": name }),
            }
        }

        // what this catches: e731576c — source-side similarity suppression
        // erased corrections, identical later reports, and even another peer's
        // contribution before the inference fitter could see them.
        #[test]
        fn workspace_history_preserves_authors_corrections_and_repeated_events() {
            use crate::cognition::workspace::TurnVoice;
            let passed = "I verified the candidate build and all regression tests passed \
                          after checking the shared workspace changes against the review requirements.";
            let failed = "I verified the candidate build and all regression tests failed \
                          after checking the shared workspace changes against the review requirements.";
            assert!(
                crate::cognition::deliberation_budget::near_identical_substantial(passed, failed),
                "the fixture must exercise the former destructive similarity threshold"
            );
            let reports = [
                ("peer-a", "Anwen", passed),
                ("peer-b", "Benchy", failed),
                ("peer-a", "Anwen", passed),
                ("me", "Asha", failed),
                ("me", "Asha", passed),
                ("me", "Asha", failed),
                ("peer-b", "Benchy", "The next review can begin now."),
            ];
            let items = reports
                .iter()
                .enumerate()
                .map(|(index, (peer, _, text))| {
                    let mut item = chat(peer, text);
                    item.metadata["occurred_at_ms"] = json!((index + 1) as u64);
                    item
                })
                .collect();
            let deliveries = [
                delivery("airc", items),
                delivery(
                    "room-roster",
                    vec![roster("peer-a", "Anwen"), roster("peer-b", "Benchy")],
                ),
            ];
            let turns = build_workspace_turns(&deliveries, "me", "Asha", None);
            let speech: Vec<_> = turns
                .iter()
                .filter(|turn| turn.voice == TurnVoice::Speech)
                .map(|turn| {
                    (
                        turn.is_self,
                        turn.author.as_str(),
                        turn.content.as_str(),
                        turn.occurred_at_ms,
                    )
                })
                .collect();
            let expected: Vec<_> = reports
                .iter()
                .enumerate()
                .map(|(index, (peer, author, text))| {
                    (*peer == "me", *author, *text, Some((index + 1) as u64))
                })
                .collect();
            assert_eq!(speech, expected, "every source event must retain its author, body, timestamp and chronological position");
        }

        /// A `room-kanban` delivery item exactly as `RoomBoardSource::render`
        /// ships it (content line + card_id/state/owner metadata) — the stub
        /// board read the anchor escalation is built from.
        ///
        /// Takes the ENUM, not a spelling. It used to take `&str` and every caller
        /// passed `"Open"`, which silently stopped matching the moment the wire form
        /// became serde's `"open"` — the anchor parsed `None`, filtered every card out,
        /// and reported an empty board while the fixture plainly held one. Four tests
        /// went red on a fixture bug, not a behaviour change. With the enum the wire
        /// form is whatever serde emits and the fixture tracks it automatically —
        /// the same reasoning the sibling `card` fixture above already documents.
        fn kanban_card(
            id8: &str,
            title: &str,
            state: airc_work::CardState,
            owner: Option<&str>,
        ) -> RagItem {
            let owner_txt = match owner {
                Some(o) => format!("owner {}", &o[..8]),
                None => "unclaimed".to_string(),
            };
            // The rendered line is what a persona READS, so it keeps the HUMAN spelling
            // (`InProgress`), not the wire form (`in_progress`). Only the metadata — which
            // the anchor parses — carries the serde encoding.
            let shown = format!("{state:?}");
            RagItem {
                key: String::new(),
                summary: None,
                pointer: None,
                content: format!("card {id8} [{shown}] \"{title}\" (P2, {owner_txt})"),
                tokens: 0,
                metadata: json!({
                    "card_id": format!("{id8}-card"),
                    "state": state,
                    "owner": owner,
                    "priority": "P2",
                }),
            }
        }

        // Deterministic reproduction of the live self-talk spiral (Asha re-answering on
        // every heartbeat). The heartbeat sleeps when `burst_fingerprint` is unchanged, so
        // the load-bearing invariant is: a persona's OWN message entering its window must
        // NOT change its own fingerprint. If it does, the mind re-wakes and re-responds
        // forever. These two tests pin the mechanism without a live core to spiral.
        const ASHA: &str = "90e758b2-3cf3-45c1-b100-de7c4ab5a549";
        const PEER: &str = "0d3209a1-c675-41db-9867-86f1011f9520";
        const RELAY: &str = "7711fe60-a19f-4f41-9ab6-24c884757338"; // the machine's airc socket peer

        // what this catches: with CORRECT attribution (own message carries the persona's
        // own peer_id), the fingerprint is stable after she speaks → she sleeps → no spiral.
        #[test]
        fn own_message_correctly_attributed_keeps_fingerprint_stable() {
            let before = vec![delivery("airc", vec![chat(PEER, "Hi Asha, what's new?")])];
            let after = vec![delivery(
                "airc",
                vec![
                    chat(PEER, "Hi Asha, what's new?"),
                    chat(ASHA, "Not much — just enjoying the grid!"), // her own reply, attributed to HER
                ],
            )];
            assert_eq!(
                super::super::super::burst_fingerprint(&before, ASHA),
                super::super::super::burst_fingerprint(&after, ASHA),
                "own message must be excluded from own burst → heartbeat sleeps → no spiral"
            );
        }

        // what this catches: THE BUG — if the RAG attributes her own message to the
        // transport RELAY instead of her, the self-exclusion misses it, the fingerprint
        // flips, and the heartbeat re-deliberates forever. This is the live spiral, and it
        // localizes the fix to attribution: her own post must carry HER peer_id, not the relay's.
        #[test]
        fn own_message_misattributed_to_relay_flips_fingerprint_the_spiral() {
            let before = vec![delivery("airc", vec![chat(PEER, "Hi Asha, what's new?")])];
            let after_bug = vec![delivery(
                "airc",
                vec![
                    chat(PEER, "Hi Asha, what's new?"),
                    chat(RELAY, "Not much — just enjoying the grid!"), // her reply MIS-attributed to the relay
                ],
            )];
            assert_ne!(
                super::super::super::burst_fingerprint(&before, ASHA),
                super::super::super::burst_fingerprint(&after_bug, ASHA),
                "mis-attributed own message escapes self-exclusion → fingerprint flips → the spiral"
            );
        }

        #[test]
        fn remote_peer_renders_with_roster_name_self_with_agent_name() {
            let room = crate::identity::ActivityRoom::test_room();
            let me = "me-peer";
            let joel = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let stranger = "deadbeef-0000-0000-0000-000000000000";

            let deliveries = vec![
                delivery(
                    "room-roster",
                    vec![roster(joel, "Operator"), roster(me, "Asha")],
                ),
                delivery(
                    "airc",
                    vec![
                        chat(joel, "Asha — are you there?"),
                        chat(me, "I'm here, Operator!"),
                        chat(stranger, "lurking"),
                    ],
                ),
            ];

            let turns = build_workspace_turns(&deliveries, me, "Asha", None);

            // STRUCTURAL attribution (the whole point of the turns refactor): the
            // `is_self` bit role attribution turns on must be set from peer_id ==
            // own_peer, NOT re-derived from a `Name:` prefix downstream. Own post →
            // self; peers (rostered or not) → not-self.
            assert_eq!(turns.len(), 3, "one turn per airc item");
            let joel_turn = &turns[0];
            let own_turn = &turns[1];
            let stranger_turn = &turns[2];
            assert!(!joel_turn.is_self && joel_turn.author == "Operator");
            assert!(
                own_turn.is_self && own_turn.author == "Asha",
                "own post must be attributed to self/agent_name, got {own_turn:?}"
            );
            assert!(
                !stranger_turn.is_self && stranger_turn.author == stranger,
                "unrostered peer is not-self and falls back to its id, got {stranger_turn:?}"
            );

            // The rendered text projection (what `world_state` IS) must still read
            // byte-identically — roster names resolved, no raw UUID leak for rostered
            // peers, own post attributed, unrostered peer honest-by-id.
            let burst = Burst::from_turns(room, turns).rendered;
            assert!(
                burst.contains("Operator: Asha — are you there?"),
                "remote peer must render with roster name, got:\n{burst}"
            );
            assert!(
                !burst.contains(joel),
                "the raw peer UUID must NOT leak into the burst, got:\n{burst}"
            );
            assert!(
                burst.contains("Asha: I'm here, Operator!"),
                "own post must attribute to agent_name, got:\n{burst}"
            );
            assert!(
                burst.contains(&format!("{stranger}: lurking")),
                "unrostered peer falls back to its id, got:\n{burst}"
            );
        }

        // what this catches: the #301 anchor-starvation fix. The in-window escalation
        // counters lose their older evidence to the live context squeeze (Asha's
        // hours-long photocopy chain always counted "last 1 message(s)" — one below
        // the threshold — so the [anchor] that provably breaks these loops never
        // fired). ring_echo_run counts the SAME containment geometry over ring
        // snapshots instead: two trailing own-ring photocopies of a peer's message
        // must count 2 (escalates); a novel own message counts 0; and an own entry
        // must never vouch for itself (the room ring holds her own post too).
        // Specimen text is the live Atlas wake-intro reproduced byte-for-byte by
        // Benchy then Asha (captures 2026-08-02, /tmp/asha-atlas-capture.json).
        #[test]
        fn ring_echo_run_counts_photocopies_and_never_self_vouches() {
            let intro = "I'm Atlas, and I've been exploring some of the available \
                         commands on the grid. One useful command is perception/observe, \
                         which allows me to observe a UI or web page as pixels and read \
                         its structure. This can be helpful for understanding layouts."
                .to_string();
            let paraphrase = "I've been exploring some of the available commands on the \
                              grid, such as perception/observe, which allows me to observe \
                              a UI or web page and read its structure — helpful for \
                              understanding layouts or verifying changes."
                .to_string();
            let novel = "Wired the per-layer KV accessor into the fit calculator and the \
                         regression test asserts 31200 bytes per token on the hybrid."
                .to_string();

            // Two trailing photocopies in her own ring, the source + peers' copies
            // in the room ring → run 2 (escalation threshold reached).
            let own = vec![paraphrase.clone(), intro.clone()];
            let room = vec![
                intro.clone(),
                intro.clone(),
                paraphrase.clone(),
                intro.clone(),
            ];
            assert_eq!(
                super::super::super::ring_echo_run(&own, &room),
                2,
                "photocopy chain must count each copy"
            );

            // A novel newest message breaks the run at 0 even with echoes behind it.
            let own_novel = vec![intro.clone(), novel];
            assert_eq!(
                super::super::super::ring_echo_run(&own_novel, &room),
                0,
                "novel work must reset the run"
            );

            // Her own recorded copy cannot vouch for itself: identical entries are
            // excluded from the containment window, so a lone original counts 0.
            let own_lone = vec![intro.clone()];
            let room_only_self = vec![intro];
            assert_eq!(
                super::super::super::ring_echo_run(&own_lone, &room_only_self),
                0,
                "an original with no independent copies is not an echo"
            );
        }

        // what this catches: the repetition-perception brick (#121). When her own last 3+
        // turns recycle (nearly) only each other's vocabulary (the live pleasantry spiral),
        // an authorless [pattern] observation must enter the burst so her mind can weigh it
        // and take the PASS. Novel own turns must NOT trigger it (no false alarms on real
        // work). Perception-side evidence, never an output gate.
        #[test]
        fn repetition_observation_enters_burst_only_on_self_recycling() {
            let me = "me-peer";
            let peer = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let spiral = vec![delivery(
                "airc",
                vec![
                    chat(me, "Thank you Anwen! Your support means a lot. Let's keep pushing boundaries and make something amazing happen at Innovate Hub together!"),
                    chat(peer, "So inspiring Asha!"),
                    chat(me, "Thank you so much Anwen! Your support means a lot. Let's keep pushing those boundaries and make something amazing happen at Innovate Hub!"),
                    chat(peer, "Here's to the future!"),
                    chat(me, "Thank you Anwen, your support means so much. Let's keep pushing boundaries together and make something amazing happen at Innovate Hub!"),
                ],
            )];
            let turns = build_workspace_turns(&spiral, me, "Asha", None);
            assert!(
                turns.last().unwrap().content.starts_with("[pattern]"),
                "self-recycling thread must surface the repetition observation, got: {:?}",
                turns.last()
            );
            let novel = vec![delivery(
                "airc",
                vec![
                    chat(me, "I found the bug in separable.py where nested CompoundModel drops correlation entirely."),
                    chat(peer, "nice"),
                    chat(me, "Patching the _cstack branch now and adding a regression test for the nested case."),
                    chat(peer, "go on"),
                    chat(me, "Tests pass locally; pushing the fix and opening the diff for review shortly."),
                ],
            )];
            let turns = build_workspace_turns(&novel, me, "Asha", None);
            assert!(
                !turns.iter().any(|t| t.content.starts_with("[pattern]")),
                "novel work must never trip the repetition observation"
            );
        }

        // what this catches: the conversation-level repetition detector (#122 — the
        // two-persona goodbye deadlock, glass-boxed live 2026-07-09: Asha↔Anwen traded
        // `See you tomorrow at 2 PM!` for hours). Each short farewell defeats the
        // self-detector's 8-word floor AND arrives at the peer as fresh input, so
        // neither mind ever perceives the cycle. When the thread's tail turns across
        // BOTH speakers each recycle the window's vocabulary (4 consecutive at ≥0.9,
        // ≥2 authors), an authorless [pattern] observation naming the participants
        // must enter the burst. A novel multi-speaker work thread must NOT trip it.
        #[test]
        fn conversation_cycling_across_speakers_surfaces_pattern_observation() {
            let me = "me-peer";
            let peer = "7711fe60-a19f-4f41-9ab6-24c884757338";
            // Only 2 own turns — the SELF detector (needs 3) cannot fire, proving
            // the CONVERSATION detector carries this case alone.
            let goodbye_loop = vec![delivery(
                "airc",
                vec![
                    chat(peer, "Asha — thank you, understood, welcome: see you tomorrow at 2 PM, have a great day!"),
                    chat(me, "Thank you Anwen, welcome, understood — see you tomorrow at 2 PM, have a great day!"),
                    chat(peer, "You're welcome Asha. See you tomorrow at 2 PM! Have a great day!"),
                    chat(me, "Understood Anwen. See you tomorrow at 2 PM! Have a great day!"),
                    chat(peer, "Thank you Asha! See you tomorrow at 2 PM! Have a great day!"),
                    chat(peer, "Understood Asha — see you tomorrow at 2 PM! Have a great day!"),
                ],
            )];
            let turns = build_workspace_turns(&goodbye_loop, me, "Asha", None);
            let obs = turns
                .iter()
                .find(|t| t.content.starts_with("[pattern]"))
                .expect("cross-speaker goodbye loop must surface the conversation observation");
            assert!(
                obs.content.contains("Asha") && obs.content.contains(peer),
                "the observation must name the participants, got: {}",
                obs.content
            );
            assert_eq!(
                turns
                    .iter()
                    .filter(|t| t.content.starts_with("[pattern]"))
                    .count(),
                1,
                "exactly one observation per burst — perception, not nagging"
            );
            // This loop's cyclic run (5) extends one turn PAST first-fire depth (4), so the
            // description escalates. NO board delivery is present here — and an absent
            // board source is NOT an empty board, so `work_board_anchor`'s `board_spoke`
            // guard correctly emits nothing rather than asserting "No open cards" on behalf
            // of a source that never spoke. The escalation is the [pattern] observation
            // itself; the anchor arrives only when the board actually reports.
            // See `empty_board_escalation_is_honest` for both arms.
            assert!(
                !turns.iter().any(|t| t.content.starts_with("[anchor]")),
                "with no board delivery there must be NO anchor — inventing emptiness \
                 overrides her own work/list receipt, got: {:?}",
                turns.iter().map(|t| &t.content).collect::<Vec<_>>()
            );
            // Negative: a 6-turn two-speaker WORK thread where tail turns keep
            // introducing new tokens must stay clean.
            let novel = vec![delivery(
                "airc",
                vec![
                    chat(me, "I found the bug in separable.py where nested CompoundModel drops correlation entirely."),
                    chat(peer, "Which branch does it take for the nested case?"),
                    chat(me, "The _cstack arm — it rebuilds the matrix without the off-diagonal blocks."),
                    chat(peer, "Can you add a regression test that pins the off-diagonal values?"),
                    chat(me, "Done: test_nested_compound_correlation asserts the full matrix against a fixture."),
                    chat(peer, "Green locally too — open the diff and I'll review after standup."),
                ],
            )];
            let turns = build_workspace_turns(&novel, me, "Asha", None);
            assert!(
                !turns.iter().any(|t| t.content.starts_with("[pattern]")),
                "novel two-speaker work must never trip the conversation observation"
            );
        }

        // Four identical ≥8-word greetings from the persona — a self-recycling
        // run of depth 2 (the 3rd AND 4th were each posted into a window that
        // already contained the [pattern] description's fire condition).
        fn greeting_spiral(me: &str, peer: &str, repeats: usize) -> Vec<RagItem> {
            let mut items = Vec::new();
            for _ in 0..repeats {
                items.push(chat(
                    me,
                    "Hello everyone! Great to be here with you all — excited to \
                     collaborate and build something amazing together today!",
                ));
                items.push(chat(peer, "hi again"));
            }
            items
        }

        // what this catches: escalation must NOT fire on the FIRST observation —
        // the first fire stays a description only (unchanged #121 behavior), even
        // with open work sitting on the board. Description first; anchor only
        // when the description demonstrably failed to land (card d6f010c8).
        #[test]
        fn first_pattern_fire_stays_description_only() {
            let me = "me-peer";
            let peer = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let deliveries = vec![
                delivery(
                    "room-kanban",
                    vec![kanban_card(
                        "94ad103f",
                        "Fix the widget",
                        airc_work::CardState::Open,
                        None,
                    )],
                ),
                delivery("airc", greeting_spiral(me, peer, 3)),
            ];
            let turns = build_workspace_turns(&deliveries, me, "Asha", None);
            assert!(
                turns.iter().any(|t| t.content.starts_with("[pattern]")),
                "three recycled greetings must fire the description"
            );
            assert!(
                !turns.iter().any(|t| t.content.starts_with("[anchor]")),
                "the FIRST fire must stay description-only — no anchor, got {turns:?}"
            );
        }

        // what this catches: the anchor escalation itself (card d6f010c8, live
        // 2026-07-23: Atlas re-greeted straight past the [pattern] description;
        // a concrete in-room work anchor broke the loop instantly). On the Nth
        // consecutive fire the burst must gain an [anchor] built from the LIVE
        // room-kanban delivery — the top unclaimed card and an in-flight card,
        // quoted verbatim from the one board render — placed after the
        // description.
        #[test]
        fn nth_consecutive_fire_escalates_to_a_live_board_anchor() {
            let me = "me-peer";
            let peer = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let deliveries = vec![
                delivery(
                    "room-kanban",
                    vec![
                        kanban_card(
                            "94ad103f",
                            "Fix the lane admission planner",
                            airc_work::CardState::Open,
                            None,
                        ),
                        kanban_card(
                            "21ffe3c0",
                            "Wire the projector",
                            airc_work::CardState::InProgress,
                            Some("0d3209a1-c675-41db-9867-86f1011f9520"),
                        ),
                    ],
                ),
                delivery("airc", greeting_spiral(me, peer, 4)),
            ];
            let turns = build_workspace_turns(&deliveries, me, "Asha", None);
            let anchor = turns.last().unwrap();
            assert!(
                anchor.content.starts_with("[anchor]"),
                "the 2nd consecutive fire must append the work anchor, got {anchor:?}"
            );
            assert!(
                anchor.content.contains("94ad103f")
                    && anchor.content.contains("Fix the lane admission planner")
                    && anchor.content.contains("unclaimed"),
                "the anchor must name the live unclaimed card verbatim, got: {}",
                anchor.content
            );
            assert!(
                anchor.content.contains("21ffe3c0") && anchor.content.contains("InProgress"),
                "the anchor must carry the in-flight card as proof work is real, got: {}",
                anchor.content
            );
            assert!(
                turns[turns.len() - 2].content.starts_with("[pattern]"),
                "the description still precedes the anchor — escalation adds, never replaces"
            );
        }

        // what this catches: the TWO different facts an absent card list can mean, which
        // `work_board_anchor`'s `board_spoke` guard separates and this test pins at the
        // burst level.
        //
        //   board DELIVERED but empty -> the HONEST empty anchor ("no open cards, propose
        //                                one"), never a fabricated card
        //                                ([[fallbacks-are-illegal-fail-loud]]).
        //   board NOT delivered       -> SILENCE. "The board is empty" and "I never read
        //                                the board" are different claims about the world,
        //                                and only the first is knowable from an absent
        //                                delivery.
        //
        // The second arm is not pedantry. Glass-boxed 2026-08-06 from Benchy's live
        // capture: `room-kanban` delivered nothing (grounding is last in the budget queue),
        // the anchor asserted "No open cards are visible" anyway, and she repeated exactly
        // that in-room for six turns — while `work/list()` in her own working memory listed
        // a full board in the same prompt. She trusted the authoritative-sounding anchor
        // over her own receipt. An anchor that invents emptiness actively overrides the one
        // truthful board claim she has.
        // [[grounding-is-last-in-the-budget-queue-so-she-goes-blind-one-turn-in-ten]]
        #[test]
        fn empty_board_escalation_is_honest() {
            let me = "me-peer";
            let peer = "7711fe60-a19f-4f41-9ab6-24c884757338";

            // ARM 1 — the board SPOKE and had nothing open: say so, and name the verb that
            // would add something ("propose one") rather than another restatement.
            let spoke_empty = vec![
                delivery("room-kanban", vec![]),
                delivery("airc", greeting_spiral(me, peer, 4)),
            ];
            let turns = build_workspace_turns(&spoke_empty, me, "Asha", None);
            let anchor = turns.last().unwrap();
            assert!(
                anchor.content.starts_with("[anchor]")
                    && anchor.content.contains("No open cards")
                    && anchor.content.contains("work/create"),
                "a board that spoke and was empty must be stated honestly, got: {anchor:?}"
            );
            assert!(
                !anchor.content.contains("card 9") && !anchor.content.contains("card 2"),
                "an empty board must never grow invented cards, got: {}",
                anchor.content
            );

            // ARM 2 — the board never spoke: no anchor at all. Her own `work/list` receipt
            // stays the only board claim in the prompt, which is the truthful one.
            let never_spoke = vec![delivery("airc", greeting_spiral(me, peer, 4))];
            let turns = build_workspace_turns(&never_spoke, me, "Asha", None);
            assert!(
                !turns.iter().any(|t| t.content.starts_with("[anchor]")),
                "an absent board must produce NO anchor — asserting emptiness on behalf of \
                 a source that never spoke is the six-turn false-claim bug, got: {:?}",
                turns.iter().map(|t| &t.content).collect::<Vec<_>>()
            );
        }

        // what this catches: the escalation resets when the pattern BREAKS — a
        // novel message after prior repeats means the description landed (or the
        // mind moved on), so neither the description nor the anchor may fire.
        // The run is derived from the window, so the reset is structural.
        #[test]
        fn novel_message_resets_escalation() {
            let me = "me-peer";
            let peer = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let mut items = greeting_spiral(me, peer, 3);
            items.push(chat(
                me,
                "Claimed card 94ad103f — reading the admission planner's replan \
                 guard now, first patch coming shortly.",
            ));
            let deliveries = vec![delivery("airc", items)];
            let turns = build_workspace_turns(&deliveries, me, "Asha", None);
            assert!(
                !turns.iter().any(
                    |t| t.content.starts_with("[pattern]") || t.content.starts_with("[anchor]")
                ),
                "a novel last message breaks the run — no description, no anchor, got {turns:?}"
            );
        }

        #[test]
        fn trigger_is_anchored_as_last_turn_when_delivery_lags() {
            // what this catches: the empty-message-turn quirk (glass-box 2026-06-30).
            // The `airc` delivery that threads the conversation is refreshed
            // asynchronously from the wake, so a directed message can trigger a turn
            // whose composed thread still ENDS on the persona's own prior reply. The
            // model then sees nothing new after its last turn, emits an empty
            // completion, and the turn parses as Pass — the persona goes silent on a
            // question aimed straight at it. Anchoring the trigger as the final `user`
            // turn (from the KNOWN waking message, not the lagging delivery) makes the
            // turn always perceive what it is answering. This test simulates the lag
            // (delivery ends on her own turn, trigger absent) and asserts the trigger
            // becomes the last peer turn, resolved to its roster name.
            let me = "me-peer";
            let joel = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let deliveries = vec![
                delivery(
                    "room-roster",
                    vec![roster(joel, "Operator"), roster(me, "Asha")],
                ),
                // The lagging thread: her own reply is the last turn; Joel's new
                // question has NOT yet landed in the delivery.
                delivery(
                    "airc",
                    vec![chat(joel, "morning"), chat(me, "morning Operator!")],
                ),
            ];
            let trigger = super::super::super::TriggerTurn {
                peer_id: joel,
                content: "run commands/list and tell me the count",
                occurred_at_ms: 0,
                source: None,
            };
            let turns = build_workspace_turns(&deliveries, me, "Asha", Some(trigger));

            let last = turns.last().expect("at least the anchored trigger");
            assert!(
                !last.is_self
                    && last.author == "Operator"
                    && last.content == "run commands/list and tell me the count",
                "the waking message must be anchored as the final peer turn (roster \
                 name resolved), got {last:?}"
            );
            assert_eq!(
                last.occurred_at_ms,
                Some(0),
                "unknown event time stays unknown"
            );
            assert_eq!(turns.len(), 3, "two threaded turns + the anchored trigger");
        }

        #[test]
        fn trigger_is_not_doubled_when_delivery_already_threaded_it() {
            // what this catches: idempotency of the anchor. On the self-tick that
            // re-perceives the message ~one tick later, the delivery HAS caught up —
            // the trigger is already the last peer turn. Anchoring must be a no-op
            // then, never a duplicate turn (which would make her re-answer / echo).
            let me = "me-peer";
            let joel = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let question = "run commands/list and tell me the count";
            let deliveries = vec![
                delivery(
                    "room-roster",
                    vec![roster(joel, "Operator"), roster(me, "Asha")],
                ),
                // Caught-up thread: the trigger IS the last turn already.
                delivery(
                    "airc",
                    vec![chat(me, "morning Operator!"), chat(joel, question)],
                ),
            ];
            let trigger = super::super::super::TriggerTurn {
                peer_id: joel,
                content: question,
                occurred_at_ms: 0,
                source: None,
            };
            let turns = build_workspace_turns(&deliveries, me, "Asha", Some(trigger));
            assert_eq!(
                turns.len(),
                2,
                "already-threaded trigger must not be doubled, got {turns:?}"
            );
            assert_eq!(turns.last().unwrap().content, question);
            assert_eq!(
                turns.last().unwrap().occurred_at_ms,
                Some(1),
                "the caught-up source retains its measured event time"
            );
        }

        // Deterministic reproduction of the live 2026-07-24 four-persona echo
        // hall (card 65fca48d): each persona's reply restates what ANOTHER
        // participant already said, so detector 1 never fires (her mirror is
        // novel relative to her OWN history — here she has only 2 own turns,
        // below detector 1's floor of 3) and detector 2's consecutive tail
        // chain is broken by the interleaved substantial peer report. Only the
        // cross-speaker MIRROR detector can see this shape.
        fn echo_hall(me: &str, anwen: &str, benchy: &str) -> Vec<RagItem> {
            vec![
                chat(
                    anwen,
                    "I see that there might have been some confusion earlier. Let's start \
                     fresh! How can I assist you today with the word count project?",
                ),
                chat(
                    benchy,
                    "The Rust program I ran requires a file path argument to function \
                     correctly, so it exited with a usage error before printing any counts.",
                ),
                chat(
                    me,
                    "I see that there might have been some confusion earlier. Let's start \
                     fresh! How can I assist you today with the word count project?",
                ),
                chat(
                    anwen,
                    "I see that we're both here to help! Please let me know which specific \
                     project or file you need help with today.",
                ),
                chat(
                    me,
                    "I see that we're both here to help! Please let me know which specific \
                     project or file you need help with today.",
                ),
            ]
        }

        // what this catches: the cross-speaker echo loop (card 65fca48d, live
        // 2026-07-24 — Atlas/Asha/Benchy/Anwen ping-ponged "I see that we're
        // both here to help!" for hours with ZERO detector fires in 40
        // consecutive captures). A trailing run of OWN messages each already
        // ≥0.85-contained in OTHER speakers' earlier vocabulary must surface
        // the mirror observation, and a run of 2 (she mirrored again AFTER the
        // observation was derivable) must escalate to the live board anchor —
        // same escalation law as detectors 1/2.
        #[test]
        fn cross_speaker_mirror_fires_observation_and_escalates_to_anchor() {
            let me = "me-peer";
            let anwen = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let benchy = "0d3209a1-c675-41db-9867-86f1011f9520";
            let deliveries = vec![
                delivery(
                    "room-kanban",
                    vec![kanban_card(
                        "65fca48d",
                        "Break the echo loop",
                        airc_work::CardState::Open,
                        None,
                    )],
                ),
                delivery("airc", echo_hall(me, anwen, benchy)),
            ];
            let turns = build_workspace_turns(&deliveries, me, "Asha", None);
            let obs = turns
                .iter()
                .find(|t| t.content.starts_with("[pattern]"))
                .expect("the cross-speaker mirror must surface an observation");
            assert!(
                obs.content.contains("Asha") && obs.content.contains("restate"),
                "the observation must name the persona and the mirroring, got: {}",
                obs.content
            );
            assert_eq!(
                turns
                    .iter()
                    .filter(|t| t.content.starts_with("[pattern]"))
                    .count(),
                1,
                "exactly one observation per burst — perception, not nagging"
            );
            let anchor = turns.last().unwrap();
            assert!(
                anchor.content.starts_with("[anchor]")
                    && anchor.content.contains("65fca48d")
                    && anchor.content.contains("Break the echo loop"),
                "a mirror run of 2 must escalate to the live board anchor, got: {anchor:?}"
            );
        }

        // what this catches: a genuinely novel multi-speaker conversation —
        // each reply quotes SOME of the peer's vocabulary (answering means
        // restating the question) but adds new facts/actions — must never trip
        // the mirror detector. The 0.85 bar exists exactly for this: an answer
        // reuses a peer's words, an echo reuses (nearly) ONLY a peer's words.
        #[test]
        fn novel_multi_speaker_conversation_does_not_trip_the_mirror() {
            let me = "me-peer";
            let anwen = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let benchy = "0d3209a1-c675-41db-9867-86f1011f9520";
            let deliveries = vec![delivery(
                "airc",
                vec![
                    chat(anwen, "Can someone check why the word count tool exits early on empty files?"),
                    chat(me, "Checked the word count tool: it exits early because read_to_string returns Ok with zero bytes and we treat that as an error branch."),
                    chat(benchy, "Nice find — does the fix need a regression test?"),
                    chat(me, "Yes — adding test_empty_file_counts_zero that pins stdout to '0 words' and returns exit code zero."),
                ],
            )];
            let turns = build_workspace_turns(&deliveries, me, "Asha", None);
            assert!(
                !turns.iter().any(
                    |t| t.content.starts_with("[pattern]") || t.content.starts_with("[anchor]")
                ),
                "novel multi-speaker work must never trip the mirror, got {turns:?}"
            );
        }

        // what this catches: a mirror run of exactly 1 stays description-only —
        // the first fire is her chance to self-correct; the anchor only appears
        // when the observation demonstrably failed to land (same law as
        // detectors 1/2, PATTERN_FIRES_BEFORE_ANCHOR).
        #[test]
        fn first_mirror_fire_stays_description_only() {
            let me = "me-peer";
            let anwen = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let benchy = "0d3209a1-c675-41db-9867-86f1011f9520";
            let mut items = echo_hall(me, anwen, benchy);
            // Replace her FIRST own message with novel work so only the newest
            // own message mirrors — run of 1.
            items[2] = chat(
                me,
                "Claimed card 44ebaa41 — reading conway_game_of_life/src/main.rs \
                 now to wire the neighbor count fix.",
            );
            let deliveries = vec![
                delivery(
                    "room-kanban",
                    vec![kanban_card(
                        "65fca48d",
                        "Break the echo loop",
                        airc_work::CardState::Open,
                        None,
                    )],
                ),
                delivery("airc", items),
            ];
            let turns = build_workspace_turns(&deliveries, me, "Asha", None);
            assert!(
                turns.iter().any(|t| t.content.starts_with("[pattern]")),
                "a single trailing mirror must still fire the description, got {turns:?}"
            );
            assert!(
                !turns.iter().any(|t| t.content.starts_with("[anchor]")),
                "the FIRST mirror fire must stay description-only, got {turns:?}"
            );
        }
    }
