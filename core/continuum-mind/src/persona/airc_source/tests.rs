#![cfg(test)]
    use super::*;
    use airc_lib::AircError;
    use crate::cognition::channel_element::ChannelElementCache;
    use crate::ai::embedding::EmbeddingProvider;
    use crate::runtime::ready_buffer::DashMapReadyBuffer;
    use airc_core::{
        Body, ClientId, EventId, Headers, MentionTarget, PeerId, RoomId, TranscriptKind,
    };
    use std::sync::Mutex;
    use uuid::Uuid;

    fn persona() -> Uuid {
        Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap()
    }

    struct NoopEmbedder;
    #[async_trait]
    impl EmbeddingProvider for NoopEmbedder {
        fn id(&self) -> &str {
            "noop"
        }
        fn dim(&self) -> usize {
            1
        }
        async fn embed(&self, _text: &str) -> Vec<f32> {
            vec![0.0]
        }
    }

    struct StubReader {
        events: Vec<TranscriptEvent>,
        fail: Mutex<bool>,
    }
    impl StubReader {
        fn new(events: Vec<TranscriptEvent>) -> Self {
            Self {
                events,
                fail: Mutex::new(false),
            }
        }
        fn set_fail(&self, fail: bool) {
            *self.fail.lock().unwrap() = fail;
        }
    }
    #[async_trait]
    impl AircTranscriptReader for StubReader {
        async fn page_recent(&self, limit: usize) -> Result<Vec<TranscriptEvent>, AircError> {
            if *self.fail.lock().unwrap() {
                return Err(AircError::UnknownPeer(PeerId::new()));
            }
            Ok(self.events.iter().take(limit).cloned().collect())
        }
    }

    /// Reader that records the room scope it was paged with (#367).
    struct RoomRecordingReader {
        events: Vec<TranscriptEvent>,
        paged_room: Mutex<Option<Option<RoomId>>>,
    }
    #[async_trait]
    impl AircTranscriptReader for RoomRecordingReader {
        async fn page_recent(&self, limit: usize) -> Result<Vec<TranscriptEvent>, AircError> {
            Ok(self.events.iter().take(limit).cloned().collect())
        }
        async fn page_recent_in(
            &self,
            room: Option<RoomId>,
            limit: usize,
        ) -> Result<Vec<TranscriptEvent>, AircError> {
            *self.paged_room.lock().unwrap() = Some(room);
            self.page_recent(limit).await
        }
    }

    // what this catches: #367 — the perception page must be scoped to the TURN's
    // room, not the reader's current-room pointer. BigMama's find: a turn in any
    // room other than the persona's landing room paged the DEFAULT room's
    // transcript, so the digest AND the recall query built from it followed the
    // wrong conversation. deliver() must hand ctx.airc_room to the reader
    // (Some → that room; None → pointer-scoped page, the room-less fallback).
    #[tokio::test]
    async fn deliver_pages_the_turn_room_not_the_pointer() {
        let room = RoomId::new();
        let reader = Arc::new(RoomRecordingReader {
            events: vec![event_in(room, Some("hello"), 1)],
            paged_room: Mutex::new(None),
        });
        let (source, _) = isolated_source(reader.clone());
        source
            .deliver(&ctx_in(room))
            .await;
        assert_eq!(
            *reader.paged_room.lock().unwrap(),
            Some(Some(room)),
            "turn room must reach the reader's page scope"
        );

        // Room-less work (consolidation, dreams): the page is explicitly
        // pointer-scoped, not accidentally room-pinned.
        let ctx_no_room = RagContext::for_persona(persona(), 1_000_000);
        source
            .deliver(&ctx_no_room)
            .await;
        assert_eq!(*reader.paged_room.lock().unwrap(), Some(None));
    }

    /// Source over an ISOLATED digest substrate (own cache/buffer) so tests don't
    /// touch process globals. The cursor is the READER's — airc owns it in
    /// production, and a stub reader carries it here.
    fn isolated_source(
        reader: Arc<dyn AircTranscriptReader>,
    ) -> (AircRagSource, Arc<DigestBuffer>) {
        let cache = Arc::new(ChannelElementCache::new(Arc::new(NoopEmbedder)));
        let builder = Arc::new(ChannelDigestBuilder::new(cache));
        let buffer = Arc::new(DashMapReadyBuffer::new());
        let source = AircRagSource {
            persona_id: persona(),
            reader,
            builder,
            buffer: buffer.clone(),
            grounding: 0,
            fetch_limit: FETCH_LIMIT,
            // Isolated tests exercise the live window; hydration has its own test
            // (a stub DurableRoomHistory) and stays off here.
            history: None,
            last_digest: std::sync::Mutex::new(std::collections::HashMap::new()),
        };
        (source, buffer)
    }

    fn ctx_in(room: RoomId) -> RagContext {
        let mut c = RagContext::for_persona(persona(), 1_000_000);
        c.substrate.airc_room = Some(room);
        c
    }

    fn event_in(room: RoomId, text: Option<&str>, lamport: u64) -> TranscriptEvent {
        TranscriptEvent {
            event_id: EventId::new(),
            room_id: room,
            peer_id: PeerId::new(),
            client_id: ClientId::new(),
            kind: TranscriptKind::Message,
            occurred_at_ms: 1_000_000 + lamport,
            lamport,
            target: MentionTarget::Room(room),
            headers: Headers::default(),
            body: text.map(Body::text),
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        }
    }

    // what this catches: a fresh channel delivers its messages as the consolidated
    // digest window (the single context path), in chronological order.
    #[tokio::test]
    async fn delivers_channel_digest() {
        let room = RoomId::new();
        let reader = Arc::new(StubReader::new(vec![
            event_in(room, Some("hello"), 1),
            event_in(room, Some("world"), 2),
        ]));
        let (source, _) = isolated_source(reader);
        let delivery = source
            .deliver(&ctx_in(room))
            .await;
        assert_eq!(delivery.items.len(), 2);
        assert_eq!(delivery.items[0].content, "hello");
        assert_eq!(delivery.items[1].content, "world");
        assert_eq!(
            delivery.items[1]
                .metadata
                .get("unread")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    // what this catches: the 5-message world view (#259, glass-boxed
    // 2026-07-30: Asha's captures showed "[context] you can currently see the
    // last 3 messages" while her L1 budget could hold dozens — she then looped
    // generic-assistant filler because the real conversation was invisible).
    // what this catches: a budget sizing the conversation inside the source (FLOW plan §2d).
    // A caught-up persona is still handed the whole staged history, and a tiny budget
    // changes nothing: her one view decides what she sees, never a second budget here.
    #[tokio::test]
    async fn caught_up_persona_sees_budget_worth_of_history_not_five_messages() {
        let room = RoomId::new();
        let events: Vec<TranscriptEvent> = (1..=20)
            .map(|l| event_in(room, Some(&format!("turn number {l}")), l))
            .collect();
        let reader = Arc::new(StubReader::new(events));
        // Fully caught up — the STUB READER carries the cursor (airc's job live).
        let (source, _) = isolated_source(reader);

        let delivery = source
            .deliver(&ctx_in(room))
            .await;
        assert!(
            delivery.items.len() > DEFAULT_GROUNDING,
            "the window must reach past the {DEFAULT_GROUNDING}-message \
             recipe floor, got {}",
            delivery.items.len()
        );
        assert_eq!(
            delivery.items.len(),
            20,
            "all 20 turns are delivered, whatever the budget"
        );
    }

    // what this catches: a head cut in the source (#128/#146 trimmed long turns to a
    // per-turn cap before her view saw them, and NEVER CLIP THE NEWEST END). Every turn
    // arrives whole at any budget; how much of the conversation she sees is the view's
    // levels, and a turn it cannot fit leaves whole at a start, never as a cut head.
    #[tokio::test]
    async fn every_turn_arrives_whole_at_any_budget() {
        let room = RoomId::new();
        let long = |tag: &str| format!("{tag}: {}", "lorem ipsum dolor sit amet ".repeat(15));
        let texts: Vec<String> = std::iter::once(long("OPERATOR your card is 0b1a6230"))
            .chain((0..4).map(|i| long(&format!("peer essay {i}"))))
            .chain(std::iter::once(long("newest peer question")))
            .collect();
        let events = texts.iter().enumerate().map(|(i, t)| event_in(room, Some(t), i as u64 + 1)).collect();
        let (source, _) = isolated_source(Arc::new(StubReader::new(events)));
        let delivery = source.deliver(&ctx_in(room)).await;
        let seen: Vec<&str> = delivery.items.iter().map(|i| i.content.as_str()).collect();
        assert_eq!(seen, texts.iter().map(String::as_str).collect::<Vec<_>>(), "every turn, whole, in order");
    }

    fn receipt_from(room: RoomId, peer: PeerId, thought: &str, act: &str, lamport: u64) -> TranscriptEvent {
        let mut ev = event_in(room, Some(&format!("💭 {thought}\n⚙ {act}")), lamport);
        ev.peer_id = peer;
        ev
    }

    // what this catches (Cormac on #4806): a folded receipt line never relocates and a
    // run behind a chat line never changes again. Receipts fold per author per RUN,
    // a run ends at the next line anyone says, and the line sits at the run's start;
    // so another receipt from the same author after a chat line starts a NEW line at
    // the end and leaves every earlier byte of the window as it was, while the open
    // run at the tail (nothing said since) is the only thing that grows.
    #[test]
    fn a_receipt_run_ends_at_speech_and_an_earlier_run_never_changes() {
        let room = RoomId::new();
        let atlas = PeerId::new();
        let mut events = vec![event_in(room, Some("Kira: morning"), 1)];
        events.push(receipt_from(room, atlas, "thought one", "code/read a ✓", 2));
        events.push(receipt_from(room, atlas, "thought two", "code/read b ✓", 3));
        events.push(event_in(room, Some("Kira: how is it going"), 4));
        events.push(receipt_from(room, atlas, "thought three", "code/shell ls ✓", 5));
        let (source, _) = isolated_source(Arc::new(StubReader::new(Vec::new())));
        let render = |events: &Vec<TranscriptEvent>| {
            let digest = source.builder.build_from_events(persona(), room.as_uuid(), events.clone(), 0, 0);
            AircRagSource::digest_items(&digest, false).into_iter().map(|i| i.content).collect::<Vec<_>>()
        };
        let before = render(&events);
        assert_eq!(before.len(), 4, "{before:?}");
        assert!(before[1].starts_with("💭 thought two") && before[1].contains("2 act batches"), "the first run, folded, at its start: {:?}", before[1]);
        assert!(before[3].contains("thought three"), "the run after Kira's line is its own line: {:?}", before[3]);
        // Another receipt from Atlas: the open run at the tail grows; nothing before
        // it changes by a byte.
        events.push(receipt_from(room, atlas, "thought four", "code/shell cat ✓", 6));
        let after = render(&events);
        assert_eq!(after.len(), 4);
        assert_eq!(&after[..3], &before[..3], "every line before the open run is byte-identical");
        assert!(after[3].starts_with("💭 thought four") && after[3].contains("2 act batches"), "{:?}", after[3]);
    }

    // what this catches (Cormac on #4806, point 2): a receipts-only room is one open run
    // that grows as one folded line at its first receipt, so the window's head holds.
    #[test]
    fn an_open_run_at_the_window_start_stays_one_line_while_it_grows() {
        let room = RoomId::new();
        let atlas = PeerId::new();
        let events: Vec<TranscriptEvent> = (1..=4).map(|l| receipt_from(room, atlas, &format!("thought {l}"), "code/read a ✓", l)).collect();
        let (source, _) = isolated_source(Arc::new(StubReader::new(Vec::new())));
        let digest = source.builder.build_from_events(persona(), room.as_uuid(), events, 0, 0);
        let items = AircRagSource::digest_items(&digest, false);
        assert_eq!(items.len(), 1);
        assert!(items[0].content.starts_with("💭 thought 4") && items[0].content.contains("4 act batches"), "{:?}", items[0].content);
    }

    // what this catches: THE LOOP WAS THE WINDOW (2026-09-03) — a run of one
    // author's work receipts collapses to ONE unit (her newest thought + an act
    // tally) instead of N verbatim "I've been going in circles" lines; a chat
    // line breaks the run and stays verbatim; the newest receipt anchors the unit.
    #[tokio::test]
    async fn a_run_of_work_receipts_collapses_to_the_latest_thought_plus_a_tally() {
        let room = RoomId::new();
        let atlas = PeerId::new();
        let lorcan = PeerId::new();
        let mut events = vec![event_in(room, Some("OPERATOR: card 678b8f5c is yours"), 1)];
        for l in 2..=5 {
            events.push(receipt_from(room, atlas, &format!("thought {l}"), "code/shell ls ✓", l));
            // twelve workers interleave: another author's receipt between each of Atlas's
            events.push(receipt_from(room, lorcan, &format!("lorcan {l}"), "code/read x ✓", l + 100));
        }
        events.push(receipt_from(room, atlas, "thought six", "code/github/issue-create  ✗", 200));
        events.push(event_in(room, Some("Kira: Atlas, stop filing issues"), 201));
        let reader = Arc::new(StubReader::new(events));
        let (source, _) = isolated_source(reader);
        let delivery = source
            .deliver(&ctx_in(room))
            .await;
        assert_eq!(
            delivery.items.len(),
            4,
            "operator line + ONE unit per working author (Lorcan, Atlas) + Kira: {:?}",
            delivery.items.iter().map(|i| i.content.clone()).collect::<Vec<_>>()
        );
        // Runs sit where they STARTED (Atlas's at lamport 2, Lorcan's at 102), so a
        // later receipt never relocates a line; each still reads its newest thought.
        let lorcan_unit = &delivery.items[2].content;
        assert!(lorcan_unit.starts_with("💭 lorcan 5"), "Lorcan's newest leads: {lorcan_unit:?}");
        assert!(lorcan_unit.contains("code/read ✓×4"), "{lorcan_unit:?}");
        let run = &delivery.items[1].content;
        assert!(run.starts_with("💭 thought six"), "newest thought leads: {run:?}");
        assert!(run.contains("5 act batches"), "batch count: {run:?}");
        assert!(run.contains("code/shell ✓×4"), "tally: {run:?}");
        assert!(run.contains("code/github/issue-create ✗"), "tally: {run:?}");
        assert!(!run.contains("thought 2"), "older thoughts folded away: {run:?}");
        assert!(delivery.items[3].content.starts_with("Kira:"));
    }

    // what this catches: THE CARRIER (card 169eb543, 2026-09-15) — a roommate's line
    // that the speak gate would refuse (a bare `[code/run]` envelope, a `[wake]` framing
    // echo, a peer-voice line) never enters her burst, in EITHER packing mode. Six
    // citizens adopted one envelope over 20 hours by reading it in the room; the
    // store still holds thousands of them and other nodes keep posting them, so the
    // speak gate alone cannot end the contagion. Real speech and receipts are untouched.
    #[tokio::test]
    async fn a_line_the_speak_gate_would_refuse_never_enters_her_burst() {
        let room = RoomId::new();
        let events = vec![
            event_in(room, Some("Kira: the fix is in models.py:44"), 1),
            event_in(room, Some("[code/run]\n[invalid] command_run: unsupported lang"), 2),
            event_in(room, Some("[wake] You are Paige, awake on the continuum grid."), 3),
            event_in(
                room,
                Some("b6dcfc8e-98ab-4488-b469-d1441720621b: I understand the confusion."),
                4,
            ),
            event_in(
                room,
                Some("```python\n# Mark this room's activity concluded (o); code/shell({\"cmd\":\"# Mark o\"})\n```"),
                5,
            ),
            event_in(
                room,
                Some("Atlas: to run it you'd write code/shell({\"cmd\":\"ls\"}) — note the braces."),
                6,
            ),
        ];
        let (source, _) = isolated_source(Arc::new(StubReader::new(events.clone())));
        // Both packing modes share the seam: the working path (presence plane dropped)
        // and the conversational path (receipts collapsed per author).
        let digest = source.builder.build_from_events(persona(), room.as_uuid(), events, 0, 0);
        for working in [false, true] {
            let items = AircRagSource::digest_items(&digest, working);
            let seen: Vec<&str> = items.iter().map(|i| i.content.as_str()).collect();
            assert_eq!(seen.len(), 2, "working={working}: only the two real lines survive: {seen:?}");
            assert!(seen[0].starts_with("Kira:"), "{seen:?}");
            assert!(seen[1].starts_with("Atlas:"), "a line DISCUSSING an envelope is speech: {seen:?}");
        }
        // And the live path (the one every turn takes) delivers the same two.
        let delivery = source.deliver(&ctx_in(room)).await;
        assert_eq!(delivery.items.len(), 2, "{:?}", delivery.items.iter().map(|i| &i.content).collect::<Vec<_>>());
    }

    // what this catches: THE DEAF-PERSONA FIX — when the turn's ctx has no airc_room
    // (compose_for_turn sets None), the room is DERIVED from the transcript (page_recent
    // is room-scoped) so the persona still hears the conversation, instead of going
    // deaf. Regression guard for the slice-2 over-strict airc_room requirement.
    #[tokio::test]
    async fn no_room_scope_derives_room_from_transcript() {
        let room = RoomId::new();
        let reader = Arc::new(StubReader::new(vec![event_in(room, Some("hi"), 1)]));
        let (source, _) = isolated_source(reader);
        let ctx = RagContext::for_persona(persona(), 1_000_000); // airc_room = None
        let delivery = source.deliver(&ctx).await;
        assert_eq!(
            delivery.items.len(),
            1,
            "derives the room from the transcript, not deaf"
        );
        assert_eq!(delivery.items[0].content, "hi");
    }

    // what this catches: genuinely nothing — no room scope AND no transcript → empty.
    #[tokio::test]
    async fn no_room_no_transcript_delivers_empty() {
        let reader = Arc::new(StubReader::new(vec![]));
        let (source, _) = isolated_source(reader);
        let ctx = RagContext::for_persona(persona(), 1_000_000);
        let delivery = source.deliver(&ctx).await;
        assert!(delivery.items.is_empty());
    }

    // what this catches: a pre-staged digest in the buffer is served WITHOUT
    // rebuilding (the hot path peeks the region's snapshot). We seed the buffer with
    // a digest the reader could not have produced, and confirm it's what's served.
    // NOTE (#259): reuse requires the staged digest to hold at least what a rebuild would
    // (the room's events after any top-up); here both hold one, so the stage is served,
    // while an under-grounded stage is rebuilt wide instead (previous test).
    #[tokio::test]
    async fn serves_prestaged_digest_without_rebuild() {
        let room = RoomId::new();
        // Reader would return "live"; buffer holds a pre-staged "staged".
        let reader = Arc::new(StubReader::new(vec![event_in(room, Some("live"), 9)]));
        let (source, buffer) = isolated_source(reader);
        // Build a staged digest via a separate builder over the SAME-shape elements.
        let cache = Arc::new(ChannelElementCache::new(Arc::new(NoopEmbedder)));
        let staged_builder = ChannelDigestBuilder::new(cache);
        let staged_reader = StubReader::new(vec![event_in(room, Some("staged"), 1)]);
        let staged = staged_builder
            .build(persona(), room.as_uuid(), &staged_reader, 100, 0)
            .await
            .unwrap();
        buffer.publish((persona(), room.as_uuid()), Arc::new(staged));

        let delivery = source
            .deliver(&ctx_in(room))
            .await;
        assert_eq!(delivery.items.len(), 1);
        assert_eq!(
            delivery.items[0].content, "staged",
            "served the pre-staged digest, not a rebuild"
        );
    }

    // what this catches: cross-persona ctx is refused (defense in depth).
    #[tokio::test]
    async fn cross_persona_ctx_refused() {
        let room = RoomId::new();
        let reader = Arc::new(StubReader::new(vec![event_in(room, Some("secret"), 1)]));
        let (source, _) = isolated_source(reader);
        let mut other = RagContext::for_persona(Uuid::new_v4(), 1_000_000);
        other.substrate.airc_room = Some(room);
        let delivery = source
            .deliver(&other)
            .await;
        assert!(delivery.items.is_empty());
    }

    // what this catches: a reader error degrades to empty (cognition stays up), no
    // panic, no fallback to a raw path.
    #[tokio::test]
    async fn reader_error_delivers_empty() {
        let room = RoomId::new();
        let reader = Arc::new(StubReader::new(vec![event_in(room, Some("x"), 1)]));
        reader.set_fail(true);
        let (source, _) = isolated_source(reader);
        let delivery = source
            .deliver(&ctx_in(room))
            .await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.tokens_used, 0);
    }

    struct CursorReader {
        events: Vec<TranscriptEvent>,
        advanced: Mutex<Vec<u64>>,
    }
    #[async_trait]
    impl AircTranscriptReader for CursorReader {
        async fn page_recent(&self, limit: usize) -> Result<Vec<TranscriptEvent>, AircError> {
            Ok(self.events.iter().take(limit).cloned().collect())
        }
        async fn advance_read_cursor(&self, _persona: Uuid, _room: Uuid, event: &TranscriptEvent) -> Result<(), AircError> {
            self.advanced.lock().unwrap().push(event.lamport);
            Ok(())
        }
    }

    // what this catches: delivery marking her read (it did, before any prompt existed, so
    // a turn her view collapsed or an act that was refused still marked the room read, and
    // running rag_inspect moved a resident's real cursor). Delivery moves nothing; the
    // read receipt of a sent prompt moves it to exactly the event named, and an event the
    // last digest does not hold moves nothing.
    #[tokio::test]
    async fn delivery_marks_nothing_read_and_the_receipt_marks_the_named_event() {
        let room = RoomId::new();
        let events: Vec<TranscriptEvent> = (1..=3).map(|l| event_in(room, Some(&format!("line {l}")), l)).collect();
        let second = events[1].event_id.as_uuid();
        let reader = Arc::new(CursorReader { events, advanced: Mutex::new(Vec::new()) });
        let (source, _) = isolated_source(reader.clone());
        let delivery = source.deliver(&ctx_in(room)).await;
        assert_eq!(delivery.items.len(), 3);
        assert!(!delivery.truncated, "the digest has no continuation cursor");
        assert!(reader.advanced.lock().unwrap().is_empty(), "delivery is not reading");
        source.mark_read(room.as_uuid(), second).await;
        source.mark_read(room.as_uuid(), Uuid::new_v4()).await;
        assert_eq!(*reader.advanced.lock().unwrap(), vec![2], "read through exactly what her prompt rendered");
    }

    struct StubHistory {
        lines: Vec<crate::persona::durable_history::HydratedLine>,
    }
    #[async_trait]
    impl crate::persona::durable_history::DurableRoomHistory for StubHistory {
        async fn room_tail(
            &self,
            _room: Uuid,
            _limit: usize,
        ) -> Result<Vec<crate::persona::durable_history::HydratedLine>, String> {
            Ok(self.lines.clone())
        }
    }

    // what this catches: the #249 greeting-chorus mechanism. Post-reboot the
    // persona's airc runtime log holds ~1 live event while the room's real
    // history lives in the durable store — every mind saw ONE message and
    // mirrored it (glass-boxed live 2026-07-30). A shallow live window must be
    // topped up from the durable tail as GROUNDING (lamport 0 — never unread,
    // the #242 no-replay contract), deduped by content against live events.
    #[tokio::test]
    async fn shallow_live_window_tops_up_from_durable_history_as_grounding() {
        let room = RoomId::new();
        // ONE live event — the post-reboot shape.
        let live = event_in(room, Some("Hello everyone! I'm Benchy."), 5);
        // A durable copy has the SAME sender. Equal text from different peers
        // is a distinct message and must not be silently collapsed.
        let sender = live.peer_id.to_string();
        let reader = Arc::new(StubReader::new(vec![live]));
        let (source, _buffer) = isolated_source(reader);
        let mut source = source;
        source.grounding = 4; // want 4 lines of context; live has 1
        source.history = Some(Arc::new(StubHistory {
            lines: vec![
                crate::persona::durable_history::HydratedLine {
                    message_id: "m1".into(),
                    occurred_at_ms: 1791078791935,
                    sender_id: sender.to_string(),
                    media: Vec::new(),
                    text: "the wordstats tests are next".into(),
                },
                crate::persona::durable_history::HydratedLine {
                    message_id: "m2".into(),
                    occurred_at_ms: 1791078792935,
                    sender_id: sender.to_string(),
                    // Duplicate of the live event — must be deduped, not doubled.
                    media: Vec::new(),
                    text: "Hello everyone! I'm Benchy.".into(),
                },
                crate::persona::durable_history::HydratedLine {
                    message_id: "m3".into(),
                    occurred_at_ms: 1791078793935,
                    sender_id: sender.to_string(),
                    media: Vec::new(),
                    text: "Atlas claimed card 7cedd4cf".into(),
                },
            ],
        }));

        let delivery = source
            .deliver(&ctx_in(room))
            .await;
        let texts: Vec<&str> = delivery.items.iter().map(|i| i.content.as_str()).collect();
        assert!(
            texts.iter().any(|t| t.contains("wordstats tests")),
            "durable history must appear in the window; got: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("card 7cedd4cf")),
            "all non-duplicate durable lines hydrate; got: {texts:?}"
        );
        assert_eq!(
            texts.iter().filter(|t| t.contains("I'm Benchy")).count(),
            1,
            "the live event and its durable copy dedup to ONE line"
        );
        // Chronology: hydrated grounding (lamport 0) precedes the live event.
        let benchy_pos = texts.iter().position(|t| t.contains("I'm Benchy")).unwrap();
        let hist_pos = texts
            .iter()
            .position(|t| t.contains("wordstats tests"))
            .unwrap();
        assert!(
            hist_pos < benchy_pos,
            "hydrated history is PRIOR context, before the live tail"
        );
        // regression (Kimi, 2026-10-04): hydration dropped each line's time and rebuilt it
        // as 0, so history rendered "[occurrence time unknown]". It keeps the store's time;
        // lamport 0 alone keeps it on the grounding side (asserted above).
        let line = crate::persona::durable_history::HydratedLine {
            message_id: uuid::Uuid::new_v4().to_string(),
            sender_id: sender.to_string(),
            occurred_at_ms: 1791078791935,
            media: Vec::new(),
            text: "timed".into(),
        };
        let event = AircRagSource::hydrated_event(room.as_uuid(), uuid::Uuid::parse_str(&sender).expect("peer uuid"), &line);
        assert_eq!(event.occurred_at_ms, 1791078791935, "a hydrated line keeps its own time");
    }
