#![cfg(test)]
    use super::*;
    use airc_lib::AircError;
    use continuum_grounding::room_thread::said;
    use std::sync::Mutex;
    use uuid::Uuid;

    fn persona() -> Uuid {
        Uuid::from_u128(0xaaa)
    }

    /// A reader that keeps the cursor moves it was asked for and counts (and refuses) every page:
    /// the source must never page.
    #[derive(Default)]
    struct CursorReader {
        advanced: Mutex<Vec<u64>>,
        paged: std::sync::atomic::AtomicUsize,
    }
    #[async_trait]
    impl AircTranscriptReader for CursorReader {
        async fn page_recent(&self, _limit: usize) -> Result<Vec<TranscriptEvent>, AircError> {
            self.paged.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Err(AircError::NoCurrentRoom)
        }
        async fn advance_read_cursor(&self, _persona: Uuid, _room: Uuid, event: &TranscriptEvent) -> Result<(), AircError> {
            self.advanced.lock().unwrap().push(event.lamport);
            Ok(())
        }
    }

    fn source() -> (AircRagSource, Arc<CursorReader>) {
        let reader = Arc::new(CursorReader::default());
        (AircRagSource::new(persona(), reader.clone()), reader)
    }

    /// Her thread of `room` holding `lines` (sender, text), lamports 1.. in order.
    fn thread_of(room: Uuid, lines: &[(Uuid, String)]) -> Arc<RoomThread> {
        let lines = lines.iter().enumerate().map(|(i, (sender, text))| said(room, 1000 + i as u128, i as u64 + 1, *sender, text));
        Arc::new(RoomThread::new(room).with(lines, usize::MAX).unwrap_or_else(|| RoomThread::new(room)))
    }

    fn ctx_pinning(thread: &Arc<RoomThread>) -> RagContext {
        RagContext::for_persona_in_room(persona(), 1_000_000, thread.room).pinning(Some(Arc::clone(thread)))
    }

    fn peer(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn texts(delivery: &RagDelivery) -> Vec<String> {
        delivery.items.iter().map(|i| i.content.clone()).collect()
    }

    // what this catches: a budget sizing the conversation inside the source (FLOW plan §2d), and
    // a source reading anything but the revision the turn pinned: every line of the pinned thread
    // arrives, oldest first, and the reader is never paged.
    #[tokio::test]
    async fn the_pinned_thread_is_delivered_whole_and_airc_is_not_read() {
        let room = Uuid::from_u128(9);
        let lines: Vec<(Uuid, String)> = (1..=20).map(|n| (peer(2), format!("turn number {n}"))).collect();
        let thread = thread_of(room, &lines);
        let (source, reader) = source();
        let delivery = source.deliver(&ctx_pinning(&thread)).await;
        assert_eq!(texts(&delivery), lines.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>());
        assert!(!delivery.truncated);
        assert!(reader.advanced.lock().unwrap().is_empty(), "delivery is not reading");
        assert_eq!(reader.paged.load(std::sync::atomic::Ordering::Relaxed), 0, "nor paging airc");
    }

    // what this catches: a head cut in the source (#128/#146 trimmed long turns to a per-turn cap
    // before her view saw them; NEVER CLIP THE NEWEST END). Every turn arrives whole.
    #[tokio::test]
    async fn every_turn_arrives_whole_at_any_length() {
        let long = |tag: &str| format!("{tag}: {}", "lorem ipsum dolor sit amet ".repeat(15));
        let lines: Vec<(Uuid, String)> = std::iter::once(long("OPERATOR your card is 0b1a6230"))
            .chain((0..4).map(|i| long(&format!("peer essay {i}"))))
            .chain(std::iter::once(long("newest peer question")))
            .map(|t| (peer(2), t))
            .collect();
        let thread = thread_of(Uuid::from_u128(9), &lines);
        let delivery = source().0.deliver(&ctx_pinning(&thread)).await;
        assert_eq!(texts(&delivery), lines.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>(), "every turn, whole, in order");
    }

    fn receipt(thought: &str, act: &str) -> String {
        format!("💭 {thought}\n⚙ {act}")
    }

    fn render(lines: &[(Uuid, String)], working: bool) -> Vec<String> {
        AircRagSource::thread_items(&thread_of(Uuid::from_u128(9), lines), working).into_iter().map(|i| i.content).collect()
    }

    // what this catches (Cormac on #4806): a folded receipt line never relocates and a run behind a
    // chat line never changes again. Receipts fold per author per RUN, a run ends at the next line
    // anyone says, and the line sits at the run's start; only the open run at the tail grows.
    #[test]
    fn a_receipt_run_ends_at_speech_and_an_earlier_run_never_changes() {
        let (kira, atlas) = (peer(2), peer(3));
        let mut lines = vec![
            (kira, "Kira: morning".to_string()),
            (atlas, receipt("thought one", "code/read a ✓")),
            (atlas, receipt("thought two", "code/read b ✓")),
            (kira, "Kira: how is it going".to_string()),
            (atlas, receipt("thought three", "code/shell ls ✓")),
        ];
        let before = render(&lines, false);
        assert_eq!(before.len(), 4, "{before:?}");
        assert!(before[1].starts_with("💭 thought two") && before[1].contains("2 act batches"), "the first run, folded, at its start: {:?}", before[1]);
        assert!(before[3].contains("thought three"), "the run after Kira's line is its own line: {:?}", before[3]);
        lines.push((atlas, receipt("thought four", "code/shell cat ✓")));
        let after = render(&lines, false);
        assert_eq!(after.len(), 4);
        assert_eq!(&after[..3], &before[..3], "every line before the open run is byte-identical");
        assert!(after[3].starts_with("💭 thought four") && after[3].contains("2 act batches"), "{:?}", after[3]);
    }

    // what this catches (Cormac on #4806, point 2): a receipts-only room is one open run that grows
    // as one folded line at its first receipt, so the window's head holds.
    #[test]
    fn an_open_run_at_the_window_start_stays_one_line_while_it_grows() {
        let lines: Vec<(Uuid, String)> = (1..=4).map(|l| (peer(3), receipt(&format!("thought {l}"), "code/read a ✓"))).collect();
        let items = render(&lines, false);
        assert_eq!(items.len(), 1);
        assert!(items[0].starts_with("💭 thought 4") && items[0].contains("4 act batches"), "{:?}", items[0]);
    }

    // what this catches: THE LOOP WAS THE WINDOW (2026-09-03): interleaved runs of two authors'
    // receipts collapse to one unit each (newest thought + tally) where each run started; a chat
    // line stays verbatim.
    #[tokio::test]
    async fn a_run_of_work_receipts_collapses_to_the_latest_thought_plus_a_tally() {
        let (atlas, lorcan, kira) = (peer(3), peer(4), peer(2));
        let mut lines = vec![(kira, "OPERATOR: card 678b8f5c is yours".to_string())];
        for l in 2..=5 {
            lines.push((atlas, receipt(&format!("thought {l}"), "code/shell ls ✓")));
            lines.push((lorcan, receipt(&format!("lorcan {l}"), "code/read x ✓")));
        }
        lines.push((atlas, receipt("thought six", "code/github/issue-create  ✗")));
        lines.push((kira, "Kira: Atlas, stop filing issues".to_string()));
        let delivery = source().0.deliver(&ctx_pinning(&thread_of(Uuid::from_u128(9), &lines))).await;
        let seen = texts(&delivery);
        assert_eq!(seen.len(), 4, "operator line + ONE unit per working author + Kira: {seen:?}");
        assert!(seen[1].starts_with("💭 thought six") && seen[1].contains("5 act batches"), "{:?}", seen[1]);
        assert!(seen[1].contains("code/shell ✓×4") && seen[1].contains("code/github/issue-create ✗"), "{:?}", seen[1]);
        assert!(!seen[1].contains("thought 2"), "older thoughts folded away: {:?}", seen[1]);
        assert!(seen[2].starts_with("💭 lorcan 5") && seen[2].contains("code/read ✓×4"), "{:?}", seen[2]);
        assert!(seen[3].starts_with("Kira:"));
    }

    // what this catches: THE CARRIER (card 169eb543, 2026-09-15): a roommate's line the speak gate
    // would refuse never enters her burst, in either packing mode; real speech is untouched.
    #[test]
    fn a_line_the_speak_gate_would_refuse_never_enters_her_burst() {
        let lines: Vec<(Uuid, String)> = [
            "Kira: the fix is in models.py:44",
            "[code/run]\n[invalid] command_run: unsupported lang",
            "[wake] You are Paige, awake on the continuum grid.",
            "b6dcfc8e-98ab-4488-b469-d1441720621b: I understand the confusion.",
            "```python\n# Mark this room's activity concluded (o); code/shell({\"cmd\":\"# Mark o\"})\n```",
            "Atlas: to run it you'd write code/shell({\"cmd\":\"ls\"}) — note the braces.",
        ]
        .iter()
        .map(|t| (peer(2), t.to_string()))
        .collect();
        for working in [false, true] {
            let seen = render(&lines, working);
            assert_eq!(seen.len(), 2, "working={working}: only the two real lines survive: {seen:?}");
            assert!(seen[0].starts_with("Kira:") && seen[1].starts_with("Atlas:"), "{seen:?}");
        }
    }

    // what this catches: a context for another persona, a turn that pinned nothing, and a pinned
    // thread of another room than the turn's each deliver nothing (defense in depth: the gate a
    // node-wide copy once needed is now a comparison of two values).
    #[tokio::test]
    async fn another_persona_no_pin_or_another_rooms_thread_delivers_nothing() {
        let thread = thread_of(Uuid::from_u128(9), &[(peer(2), "secret".to_string())]);
        let (source, _) = source();
        let mut other = ctx_pinning(&thread);
        other.substrate.persona_id = Uuid::new_v4();
        assert!(source.deliver(&other).await.items.is_empty());
        let unpinned = RagContext::for_persona_in_room(persona(), 1_000_000, thread.room);
        assert!(source.deliver(&unpinned).await.items.is_empty());
        let elsewhere = RagContext::for_persona_in_room(persona(), 1_000_000, Uuid::from_u128(10)).pinning(Some(Arc::clone(&thread)));
        assert!(source.deliver(&elsewhere).await.items.is_empty());
    }

    // what this catches: the receipt writing anything but the airc event it is handed (airc's
    // cursor wants the event itself).
    #[tokio::test]
    async fn the_receipt_moves_her_cursor_to_the_event_it_names() {
        use crate::cognition::llm_deliberation_faculty::ReadReceipt;
        let thread = thread_of(Uuid::from_u128(9), &[(peer(2), "one".into()), (peer(2), "two".into())]);
        let (source, reader) = source();
        source.read_through(thread.room, &thread.items()[1].record).await;
        assert_eq!(*reader.advanced.lock().unwrap(), vec![2]);
    }
