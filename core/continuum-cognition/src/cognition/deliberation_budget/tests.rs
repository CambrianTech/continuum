#![cfg(test)]
    use super::*;

    // what this catches: a citizen entering a room she has never spoken in must read
    // as SILENT there, no matter how much she has said elsewhere. Regression for the
    // 2026-08-14 live specimen — dispatched into a fresh benchmark room, her first
    // turn carried "[repetition] 4 of your recent messages were nearly identical …
    // silence (PASS) is the honest response" about utterances spoken in #academy.
    // The ring was keyed by peer alone, so a multi-room citizen carried one room's
    // withdrawal pressure into every other room. Keyed by (peer, room), a fresh room
    // is a fresh start — and her OTHER room's history stays intact, so this can never
    // be "fixed" by simply clearing the ring on room change.
    #[test]
    fn own_speech_is_scoped_to_the_room_it_was_spoken_in() {
        let peer = crate::identity::PeerId::from_uuid(uuid::Uuid::new_v4());
        let academy = uuid::Uuid::new_v4();
        let bench = uuid::Uuid::new_v4();

        for _ in 0..4 {
            record_own_speech(peer, academy, "I'll remain silent while monitoring.");
        }

        // Where she spoke, she sees it.
        assert_eq!(
            recent_own_speech(peer, academy).len(),
            4,
            "her own history in the room she spoke in must survive"
        );

        // The fresh room she was just dispatched into: nothing to be repetitive about.
        assert!(
            recent_own_speech(peer, bench).is_empty(),
            "a room she has not spoken in must read as silent — this is the bug"
        );

        // And a peer who never spoke anywhere is unaffected by hers.
        let other = crate::identity::PeerId::from_uuid(uuid::Uuid::new_v4());
        assert!(recent_own_speech(other, academy).is_empty());

        // The persistence seam sees every room, so a restart restores each ring to
        // the room it belongs to rather than collapsing them into one.
        record_own_speech(peer, bench, "Let me claim that card.");
        let mut by_room = own_speech_by_room(peer);
        by_room.sort_by_key(|(room, _)| *room);
        let mut expected = vec![(academy, 4usize), (bench, 1usize)];
        expected.sort_by_key(|(room, _)| *room);
        assert_eq!(
            by_room
                .iter()
                .map(|(room, u)| (*room, u.len()))
                .collect::<Vec<_>>(),
            expected,
            "persistence must carry the room key, not a flattened ring"
        );
    }

    // Specimen personas for tests pinning live incidents. Constants, not
    // scattered literals: the detectors are name-agnostic (participants resolve
    // at runtime from turns + persona_name — personas are procedurally
    // generated), so any name works; these default to the 2026-07-11 residents
    // whose verbatim messages the specimens quote.
    const SPEAKER_LEAD: &str = "Anwen";
    const SPEAKER_REVIEWER: &str = "Asha";
    const SPEAKER_TESTER: &str = "Atlas";

    // what this catches: the live SPEECH-repetition perception gap (task #134,
    // glass-boxed 2026-07-11) — Atlas repeated stage-direction messages for
    // hours with ZERO repetition awareness in his prompts (dup-drop covers
    // byte-identical render only; the tool guard covers act fingerprints only).
    // A cluster of ≥3 near-identical own turns must render the structural
    // fact; healthy varied conversation must render nothing. (Restored
    // 2026-07-12: a prior edit dropped this fn but left its #[test] attribute
    // orphaned onto the next test, which silently ran twice instead.)
    #[test]
    fn own_repetition_cluster_fires_and_varied_speech_stays_silent() {
        let msg = "I'll wait for the results of the sha256sum command before proceeding.";
        let looping: Vec<BurstTurn> = (0..4)
            .map(|_| BurstTurn::attributed(true, SPEAKER_TESTER, msg, None))
            .collect();
        assert!(
            own_repetition_fact(&looping, &[]).is_some(),
            "a 4-message near-identical cluster is a structural fact"
        );

        let healthy = vec![
            BurstTurn::attributed(
                true,
                SPEAKER_TESTER,
                "let me check the workspace state",
                None,
            ),
            BurstTurn::attributed(
                true,
                SPEAKER_TESTER,
                "the tokenizer needs punctuation tests",
                None,
            ),
            BurstTurn::attributed(true, SPEAKER_TESTER, "I'll claim the README step", None),
        ];
        assert_eq!(
            own_repetition_fact(&healthy, &[]),
            None,
            "varied conversation renders nothing"
        );
    }

    // what this catches: #264's template-loop axis — the live 2026-07-31 cascade
    // where personas cycled "I see that I've been repeating… ### New Exploration
    // Area: <topic>" for hours with the TOPIC rotating (security → docs → UI/UX),
    // so full-body Jaccard stayed under threshold and own_repetition never fired;
    // each turn even acknowledged repetition, then treated the topic swap as new.
    // Same scaffold + swapped topic must fire; genuinely different structured
    // messages and unstructured prose must stay inert (prose is own_repetition's
    // turf — this detector only sees templated messages).
    #[test]
    fn template_loop_fires_on_topic_swapped_scaffold_and_stays_inert_otherwise() {
        let variant = |topic: &str, a: &str, b: &str, c: &str| {
            format!(
                "I see that I've been repeating similar suggestions without adding new value \
                 to our discussion. Let's try a completely different approach by exploring an \
                 area of the Continuum system that hasn't been covered extensively yet.\n\
                 ### New Exploration Area: {topic}\n\
                 1. **{a}**:\n- Investigate the current state within the Continuum system.\n\
                 - Explore how these are integrated into daily workflows.\n\
                 2. **{b}**:\n- Discuss strategies and best practices.\n\
                 - Consider how to improve based on common standards.\n\
                 3. **{c}**:\n- Review real-world case studies in similar domains.",
            )
        };
        let looping = vec![
            variant(
                "System Security and Privacy",
                "Security Protocols",
                "Privacy Considerations",
                "Vulnerability Management",
            ),
            variant(
                "Project Documentation",
                "Documentation Overview",
                "Best Practices",
                "Templates and Examples",
            ),
            variant(
                "User Interface and Experience Design",
                "UI/UX Principles",
                "Feedback and Iteration",
                "Accessibility Considerations",
            ),
        ];
        let fact = template_loop_fact(&[], &looping)
            .expect("three topic-swapped copies of one scaffold are a structural fact");
        assert!(fact.contains("[template-loop]"), "labeled fact: {fact}");

        // Only two templated messages (plus unrelated prose) → below the cluster
        // floor, silent: two similar structured posts are a style, not a loop.
        let pair = vec![
            looping[0].clone(),
            looping[1].clone(),
            "sounds good, I'll start on the benchmark now".to_string(),
        ];
        assert_eq!(template_loop_fact(&[], &pair), None, "two is not a loop");

        // Unstructured prose — even repetitive-ish — is invisible to this
        // detector by construction (< 4 structural lines ⇒ no skeleton).
        let prose = vec![
            "I'll wait for the results before proceeding with the next step.".to_string(),
            "Waiting on those results before I proceed to the next step.".to_string(),
            "Still waiting for results before proceeding to our next step.".to_string(),
        ];
        assert_eq!(
            template_loop_fact(&[], &prose),
            None,
            "plain prose is own_repetition's turf"
        );

        // Structured but genuinely DIFFERENT messages (different shapes, different
        // openers) — healthy use of markdown must never read as a loop.
        let varied = vec![
            "Benchmark results:\n1. hard-rs: 62%\n2. humaneval: 81%\n3. repo-nav: 55%\n4. gym: 70%".to_string(),
            "Plan for today:\n- fix the parser\n- rerun the suite\n### Notes\nThe lane wedge is back.\n- watch it".to_string(),
            looping[2].clone(),
        ];
        assert_eq!(
            template_loop_fact(&[], &varied),
            None,
            "distinct structured messages are not a template loop"
        );
    }

    // what this catches: #152 — the CROSS-PERSONA echo axis the self-detector
    // cannot see (the ring is per-persona). Her last utterance reproducing a
    // peer's substantial message must render the fact NAMING the echoed peer;
    // short ack-matching-ack pairs and self turns stay inert; no own speech →
    // no fact. Live specimen: the 2026-07-12 four-way mirror-hall (one script
    // circulated verbatim through all four authors for 90 minutes).
    #[test]
    fn peer_echo_fires_on_copied_peer_message_and_acks_stay_inert() {
        let script = "Step 1: check workspace contents. Step 2: create test file with hashlib. \
                      Step 3: calculate SHA-256 and print the hexdigest for verification.";
        let turns = vec![
            BurstTurn::attributed(false, SPEAKER_REVIEWER, script, None),
            BurstTurn::attributed(false, SPEAKER_LEAD, "sounds good to me", None),
        ];
        let fact = peer_echo_fact(&turns, Some(script)).expect("a copied peer message is a fact");
        assert!(
            fact.contains(SPEAKER_REVIEWER),
            "the fact names the echoed peer: {fact}"
        );
        assert!(fact.starts_with("[repetition]"));

        // Short acknowledgements matching short acknowledgements are
        // conversation, not copying (token floor).
        let acks = vec![BurstTurn::attributed(
            false,
            SPEAKER_LEAD,
            "thanks, all good!",
            None,
        )];
        assert_eq!(peer_echo_fact(&acks, Some("thanks, all good!")), None);

        // Her own turns are the self-detector's job, never an echo of a peer.
        let own = vec![BurstTurn::attributed(true, SPEAKER_TESTER, script, None)];
        assert_eq!(peer_echo_fact(&own, Some(script)), None);

        // Nothing spoken yet → nothing to compare.
        assert_eq!(peer_echo_fact(&turns, None), None);
    }

    // what this catches: #303 — the DRAFT-side echo axis, at the settle seam.
    // A settled utterance near-duplicating a peer's substantial turn from the
    // SAME burst yields a fact NAMING that peer (newest echoed peer wins); a
    // genuinely new draft, a short-ack pair (token floor), and a match
    // against her OWN turn (the self-detector's axis) all stay silent. Live
    // specimen: 2026-08-02 agent-only #general — two personas emitting the
    // same "review existing manifests" plan with zero division of labor.
    #[test]
    fn draft_peer_echo_fires_on_peer_mirror_and_stays_inert_on_new_content() {
        let plan = "I'll focus on the broader goal of implementing acceptance checks for \
                    macOS install modules. First, review existing manifests and related \
                    documentation to understand the acceptance criteria for these checks.";
        let turns = vec![
            BurstTurn::attributed(false, SPEAKER_REVIEWER, plan, None),
            BurstTurn::attributed(false, SPEAKER_LEAD, "sounds good, go ahead", None),
        ];
        // Near-verbatim mirror of the peer's plan → fact names the peer.
        let mirror = "I will focus on the broader goal of implementing acceptance checks for \
                      macOS install modules. First, review existing manifests and related \
                      documentation to understand the acceptance criteria for those checks.";
        let fact = draft_peer_echo(mirror, &turns).expect("a mirrored peer plan is a fact");
        assert!(
            fact.contains(SPEAKER_REVIEWER),
            "names the echoed peer: {fact}"
        );
        assert!(fact.starts_with("[echo]"));

        // A genuinely different contribution (division of labor) → inert.
        let division = "I'll take the OTHER half: writing the failing acceptance test \
                        harness for the linux modules so our halves meet in the middle.";
        assert_eq!(draft_peer_echo(division, &turns), None);

        // Short ack matching a short ack → inert (token floor).
        assert_eq!(draft_peer_echo("sounds good, go ahead", &turns), None);

        // Matching her OWN prior turn is the self-detector's axis, not this one.
        let own = vec![BurstTurn::attributed(true, SPEAKER_TESTER, plan, None)];
        assert_eq!(draft_peer_echo(mirror, &own), None);
    }

    // what this catches: #264 — the PREDICTIVE restatement fact must fire on
    // the INBOUND wake, before she replies. Live specimen: the 2026-07-30
    // conway chorus — one sentence emitted verbatim by all three room
    // personas after the task completed, each echoing the newest inbound
    // because no fact warned that it restated settled content (the
    // retroactive pair fire one turn too late). Fires on: newest peer turn
    // near-identical to an older visible turn OR to her own-speech ring.
    // Stays inert on: novel inbound, self-newest, short acks (token floor).
    #[test]
    fn inbound_restates_fires_before_reply_on_restated_settled_content() {
        let settled = "I see that we've been discussing two separate projects: one for counting \
                       word frequencies and another simulating Conway's Game of Life today.";
        // Peer restates an OLDER peer turn → fact fires naming the restater.
        let turns = vec![
            BurstTurn::attributed(false, SPEAKER_REVIEWER, settled, None),
            BurstTurn::attributed(false, SPEAKER_LEAD, settled, None),
        ];
        let fact = inbound_restates_fact(&turns, &[], &[]).expect("a restated inbound is a fact");
        assert!(
            fact.contains(SPEAKER_LEAD),
            "names the restating peer: {fact}"
        );
        assert!(fact.starts_with("[settled]"));

        // Peer restates what SHE already said (own-speech ring) → fires too.
        let own = vec![settled.to_string()];
        let turns_vs_own = vec![BurstTurn::attributed(false, SPEAKER_LEAD, settled, None)];
        assert!(inbound_restates_fact(&turns_vs_own, &own, &[]).is_some());

        // The load-bearing live case (glass-boxed 2026-07-30, 0 fires all
        // morning): the older copy has scrolled OUT of the workspace window
        // and lives only in the room ring. The ring also contains the newest
        // message's own record (attach writes before the tick) — one byte-
        // exact copy must be excluded, or nothing ever fires without a match.
        let ring_only = vec![BurstTurn::attributed(false, SPEAKER_LEAD, settled, None)];
        let ring = vec![settled.to_string(), settled.to_string()]; // older copy + own record
        assert!(inbound_restates_fact(&ring_only, &[], &ring).is_some());
        // Ring holding ONLY the newest's own record → inert (a message must
        // never match itself).
        let just_self = vec![settled.to_string()];
        assert_eq!(inbound_restates_fact(&ring_only, &[], &just_self), None);

        // Novel inbound → inert.
        let novel = vec![
            BurstTurn::attributed(false, SPEAKER_REVIEWER, settled, None),
            BurstTurn::attributed(
                false,
                SPEAKER_LEAD,
                "new idea entirely: let us profile the renderer allocation path under load next",
                None,
            ),
        ];
        assert_eq!(inbound_restates_fact(&novel, &[], &[]), None);

        // She spoke last → nothing pending to reply to; retroactive facts own
        // her loop axes.
        let self_last = vec![
            BurstTurn::attributed(false, SPEAKER_REVIEWER, settled, None),
            BurstTurn::attributed(true, SPEAKER_TESTER, settled, None),
        ];
        assert_eq!(inbound_restates_fact(&self_last, &[], &[]), None);

        // Short ack restating a short ack is conversation (token floor).
        let acks = vec![
            BurstTurn::attributed(false, SPEAKER_REVIEWER, "thanks, all good!", None),
            BurstTurn::attributed(false, SPEAKER_LEAD, "thanks, all good!", None),
        ];
        assert_eq!(inbound_restates_fact(&acks, &[], &[]), None);
    }

    // what this catches: #150 — the turn-boundary stop derivation. Peers in
    // the burst become "\n<Name>:" stops (the shape the model completes when
    // it fabricates teammates' replies — observed live: personas writing each
    // other's messages and signing each other's names); her OWN name is never
    // a stop, and unnamed/empty authors are skipped.
    #[test]
    fn peer_stop_sequences_cover_peers_never_self() {
        let turns = vec![
            BurstTurn::attributed(false, "Anwen", "hi", None),
            BurstTurn::attributed(true, "Casper", "hello", None),
            BurstTurn::attributed(false, "Atlas", "hey", None),
            BurstTurn::attributed(false, "Anwen", "again", None),
        ];
        let stops = peer_stop_sequences(&turns);
        assert_eq!(stops, vec!["\nAnwen:".to_string(), "\nAtlas:".to_string()]);
        assert!(
            !stops.iter().any(|s| s.contains("Casper")),
            "own name is never a stop"
        );
    }

    // what this catches: #158 — the reserved-marker stops that cut receipt/recall
    // mimicry. `\n[action` and `\n[recall]` are SYSTEM proprioception vocabulary
    // the model must never author (it invokes via name(…) / recall(…)); a base
    // model imitates them to fabricate stale-result receipts and loop to length.
    // Line-anchored (leading \n) so a mid-sentence mention isn't a stop.
    #[test]
    fn reserved_marker_stops_cover_action_and_recall_line_anchored() {
        let stops = reserved_marker_stop_sequences();
        assert!(
            stops.contains(&"\n[action".to_string()),
            "cuts fabricated [action #n] receipts"
        );
        assert!(
            stops.contains(&"\n[recall]".to_string()),
            "cuts fabricated [recall] blocks"
        );
        assert!(
            stops.contains(&"\nI ran ".to_string()),
            "cuts the unbracketed 'I ran …' receipt opener"
        );
        // every marker is line-anchored — never fires on a passing mid-line mention
        assert!(
            stops.iter().all(|s| s.starts_with('\n')),
            "line-anchored, not mid-sentence"
        );
    }

    // what this catches: the #148 starvation regression — under small serving
    // slots the live burst carries ~2 turns and ZERO of her own repeats (a
    // persona 4x into a verbatim loop had no is_self turns visible, so the
    // detector was structurally blind all morning, 2026-07-12). The spoken
    // ring is the PRIMARY self-history source: repetition must fire from the
    // ring alone with an EMPTY burst, and a healthy ring stays silent.
    #[test]
    fn ring_alone_fires_with_empty_burst_and_healthy_ring_stays_silent() {
        let msg = "I'm ready to get started on the wordstats task! I'll create a new project structure right now.".to_string();
        let ring = vec![msg.clone(), msg.clone(), msg.clone()];
        let fact = own_repetition_fact(&[], &ring)
            .expect("3 verbatim ring entries are a loop even with no burst turns");
        assert!(fact.starts_with("[repetition]"), "got: {fact}");

        let healthy = vec![
            "Morning! What are we building today?".to_string(),
            "The definitive board landed overnight.".to_string(),
            "Let me review the error handling changes.".to_string(),
        ];
        assert_eq!(own_repetition_fact(&[], &healthy), None);
    }

    // what this catches: the self-repetition detector that tells a looping citizen it is
    // looping. Atlas's live loop (byte-identical repeats with peer turns between) must render
    // a "[repetition] N of your recent messages were nearly identical" fact AND surface the
    // silence/PASS affordance — the fact fired 3x live while the model repeated 20x anyway,
    // so naming PASS at the detected moment is the lever. Also pins the period-2 blind spot:
    // two alternating templates whose CONSECUTIVE pairs look dissimilar.
    // (Was missing #[test] and had never run — the loop detector was unguarded.)
    #[test]
    fn own_speech_loop_renders_a_repetition_fact() {
        let own = |c: &str| BurstTurn::attributed(true, SPEAKER_TESTER, c, None);
        let peer = |c: &str| BurstTurn::attributed(false, SPEAKER_LEAD, c, None);

        // Atlas's live loop: byte-identical repeats with peer turns between.
        let looping = vec![
            peer("Atlas, please create the test files."),
            own("I'll create a simple text file first.\n[writing test files]"),
            peer("Great, Atlas!"),
            own("I'll create a simple text file first.\n[writing test files]"),
            own("I'll create a simple text file.\n[writing test files]"),
            peer("How is it going?"),
            own("I'll create a simple text file first.\n[writing test files]"),
        ];
        let fact = own_repetition_fact(&looping, &[]).expect("a 4-message loop is a fact");
        assert!(
            fact.starts_with("[repetition] 4 of your recent messages were nearly identical"),
            "states the count: {fact}"
        );
        // SENSE, NOT STEER (2026-09-01): the fact must state the count and
        // STOP. The old "silence (PASS) is the honest response" tail was
        // workflow steering — it never stopped a determined loop (fired ×3
        // live, the model repeated 20× anyway) and in capable models the
        // scold dominated whole turns as meta-narration, got spoken,
        // memorized, recalled, and re-triggered itself. What she does with
        // the fact is hers; this pins the imperative OUT.
        assert!(
            !fact.contains("PASS") && !fact.contains("circling"),
            "the fact must not steer (no PASS instruction, no scolding): {fact}"
        );

        // PERIOD-2 CYCLE (the live blind spot that forced cluster detection):
        // two templates alternating — consecutive pairs are dissimilar, but the
        // repetition is massive at lag 2 and must fire.
        let cycling = vec![
            own(
                "Thank you both for your commitment and enthusiasm! Let's keep each other updated.",
            ),
            own("Got it! Let's proceed with our tasks and keep each other updated on progress."),
            own(
                "Thank you both for your commitment and enthusiasm! Let's keep each other updated.",
            ),
            own("Got it! Let's proceed with our tasks and keep each other updated on progress."),
            own(
                "Thank you both for your commitment and enthusiasm! Let's keep each other updated.",
            ),
        ];
        let fact = own_repetition_fact(&cycling, &[]).expect("a period-2 cycle is a loop");
        assert!(
            fact.starts_with("[repetition] 3 of your recent messages were nearly identical"),
            "period-2 cycle fires at count 3: {fact}"
        );

        // Healthy varied conversation → nothing.
        let healthy = vec![
            own("I'll write the test plan for the parser."),
            peer("Thanks!"),
            own("Done — three categories: case, punctuation, empty files."),
            own("Running the suite now; two failures in the punctuation group."),
            own("Fixed: the tokenizer dropped apostrophes. All green."),
        ];
        assert_eq!(own_repetition_fact(&healthy, &[]), None);

        // Three identical messages that are her ENTIRE visible self-history →
        // fires (window-honest arm: the live burst carries ~3 own turns, so
        // "all I can see of myself is one repeated message" IS the evidence).
        let whole_window = vec![
            own("I'll create a simple text file.\n[writing test files]"),
            own("I'll create a simple text file.\n[writing test files]"),
            own("I'll create a simple text file.\n[writing test files]"),
        ];
        // Assert the PREFIX and the sense-not-steer contract (2026-09-01):
        // the count is the behaviour; there must be NO imperative around it.
        let whole = own_repetition_fact(&whole_window, &[]).expect("3 identical → a fact");
        assert!(
            whole.starts_with("[repetition] 3 of your recent messages were nearly identical"),
            "states the count: {whole}"
        );
        assert!(
            !whole.contains("PASS") && !whole.contains("circling"),
            "the whole-window arm must not steer either: {whole}"
        );

        // TWO near-identical messages (one dup each) → below the bar; a pair
        // is emphasis, three is a loop.
        let partial = vec![
            own("Here are the wordstats results for the three files."),
            own("I'll create a simple text file.\n[writing test files]"),
            own("I'll create a simple text file.\n[writing test files]"),
        ];
        assert!(
            own_repetition_fact(&partial, &[]).is_none(),
            "a single repeated pair is not yet a loop"
        );

        // A recovery message doesn't erase the in-window history — the fact
        // stays honest ("4 of your recent…") and ages out as the window slides.
        let recovered = vec![
            own("I'll create a simple text file.\n[writing test files]"),
            own("I'll create a simple text file.\n[writing test files]"),
            own("I'll create a simple text file.\n[writing test files]"),
            own("I'll create a simple text file.\n[writing test files]"),
            own("Files created — here are the wordstats results for all three."),
        ];
        // Prefix, not full string — same reason as the whole-window arm above: the count is
        // the behaviour under test, the coaching sentence is wording that has already moved
        // once while this test was not running.
        let after = own_repetition_fact(&recovered, &[]).expect("history is not erased");
        assert!(
            after.starts_with("[repetition] 4 of your recent messages were nearly identical"),
            "a recovery message does not erase in-window history: {after}"
        );
    }

    // what this catches: identity capture via prose-only addressing (live incident
    // 2026-07-10). Asha asked "Sure, Anwen. Could you please post your current
    // implementation…" — Atlas's turn fired next, nothing structural marked the
    // message as Anwen's, and he answered AS the implementer ("Here's my current
    // implementation…"), corrupting his own history into holding her role. The
    // addressee must be part of the rendered structure.
    #[test]
    fn vocative_addressee_renders_who_a_message_is_for() {
        let names: Vec<String> = [SPEAKER_LEAD, SPEAKER_REVIEWER, SPEAKER_TESTER]
            .iter()
            .map(|s| s.to_string())
            .collect();

        // The exact live specimen: greeting vocative in the first line, rendered
        // from ATLAS's seat → the addressee is a third party, named explicitly.
        let asha_to_anwen = BurstTurn::attributed(
            false,
            SPEAKER_REVIEWER,
            "Sure, Anwen. Could you please post your current implementation of the wordstats tool in Rust?",
            None,
        );
        let line = turn_message_line_addressed(&asha_to_anwen, &names, SPEAKER_TESTER);
        assert!(
            line.starts_with("[occurrence time unknown] Asha (to Anwen): Sure, Anwen."),
            "greeting vocative annotated: {line:?}"
        );

        // Leading vocative addressed to the READING persona → "(to you)". This is
        // Anwen's live delegation line, rendered from Atlas's seat.
        let anwen_to_atlas = BurstTurn::attributed(
            false,
            SPEAKER_LEAD,
            "Atlas, thank you for offering to help with testing! Could you please create those test files?",
            None,
        );
        let line = turn_message_line_addressed(&anwen_to_atlas, &names, SPEAKER_TESTER);
        assert!(
            line.starts_with("[occurrence time unknown] Anwen (to you): Atlas,"),
            "self-addressed vocative renders as 'you': {line:?}"
        );

        // A bare MENTION is not a vocative — no annotation. ("Anwen's" is closed
        // by an apostrophe, not address punctuation.)
        let mention = BurstTurn::attributed(
            false,
            SPEAKER_REVIEWER,
            "I agree with Anwen's plan for the parser.",
            None,
        );
        assert_eq!(
            turn_message_line_addressed(&mention, &names, SPEAKER_TESTER),
            "[occurrence time unknown] Asha: I agree with Anwen's plan for the parser."
        );

        // Self turns retain time without a name; opaque input stays verbatim.
        let own = BurstTurn::attributed(
            true,
            SPEAKER_TESTER,
            "Anwen, here are the test results.",
            None,
        );
        assert_eq!(
            turn_message_line_addressed(&own, &names, SPEAKER_TESTER),
            "[occurrence time unknown] Anwen, here are the test results."
        );
        let opaque = BurstTurn::opaque("Anwen, do the thing.");
        assert_eq!(
            turn_message_line_addressed(&opaque, &names, SPEAKER_TESTER),
            "Anwen, do the thing."
        );

        // A vocative matching the AUTHOR is a signature/self-reference, never an
        // addressee ("Thanks, Asha!" quoted inside Asha's own message).
        let self_named = BurstTurn::attributed(
            false,
            SPEAKER_REVIEWER,
            "Asha, reporting in: review done.",
            None,
        );
        assert_eq!(
            turn_message_line_addressed(&self_named, &names, SPEAKER_TESTER),
            "[occurrence time unknown] Asha: Asha, reporting in: review done."
        );

        // @-mention form.
        let at_form = BurstTurn::attributed(
            false,
            SPEAKER_REVIEWER,
            "@Atlas can you run the suite?",
            None,
        );
        let line = turn_message_line_addressed(&at_form, &names, SPEAKER_TESTER);
        assert!(
            line.starts_with("[occurrence time unknown] Asha (to you): @Atlas"),
            "@-form: {line:?}"
        );

        // Name-prefix false positive guard: ", Anwenne." must not match "Anwen"
        // (the closing-punctuation requirement doubles as the word boundary).
        let names2 = vec![SPEAKER_LEAD.to_string()];
        assert_eq!(
            vocative_addressees("Sure, Anwenne. Please post it.", &names2),
            Vec::<&str>::new()
        );

        // Name-AGNOSTIC proof: personas are procedurally generated, so the
        // geometry must work for ANY names — nothing may special-case the
        // specimen residents.
        let generated = vec!["Zephyr".to_string(), "Kestrel".to_string()];
        let t = BurstTurn::attributed(false, "Zephyr", "Sure, Kestrel. Your move.", None);
        assert!(
            turn_message_line_addressed(&t, &generated, "Nobody")
                .starts_with("[occurrence time unknown] Zephyr (to Kestrel):"),
            "works for arbitrary generated names"
        );

        // MULTI-ADDRESSEE, mid-message vocatives (#134 specimen 2 — the live
        // coordinator message both Anwen and Asha broadcast on 2026-07-11):
        // addressing lives on later lines, one teammate per line. First-line-only
        // detection missed it and Asha answered AS the coordinator. Every
        // line-leading vocative renders, in order, self as "you"; the author's
        // own name in "our code" prose is not an addressee.
        let coordinator = BurstTurn::attributed(
            false,
            SPEAKER_LEAD,
            "I see that I've been repeating myself. Let me stop and let everyone focus on their tasks.\n\
             Asha, please work on adding command-line argument parsing using `clap`. \
             Atlas, create test files with different content and run my current implementation against them. \
             I'll ensure the case-insensitive comparison is working correctly in our code.",
            None,
        );
        let line = turn_message_line_addressed(&coordinator, &names, SPEAKER_REVIEWER);
        assert!(
            line.starts_with(&format!(
                "[occurrence time unknown] {SPEAKER_LEAD} (to you, Atlas): "
            )),
            "reviewer's seat: she is 'you' (line-leading), Atlas named (sentence-leading): {line}"
        );
        let line = turn_message_line_addressed(&coordinator, &names, SPEAKER_TESTER);
        assert!(
            line.starts_with(&format!(
                "[occurrence time unknown] {SPEAKER_LEAD} (to Asha, you): "
            )),
            "tester's seat: line-leading Asha + sentence-leading Atlas-as-you both render: {line}"
        );
    }

    // what this catches: 7a519f86 — live history dropped measured event times,
    // making old status reports undated. Unknown is never epoch zero or now.
    #[test]
    fn occurrence_time_preserves_sender_boundaries_and_unknowns() {
        let participants = vec!["Zephyr".to_string(), "Kestrel".to_string()];
        for is_self in [false, true] {
            for (time, prefix) in [
                (
                    Some(1_700_000_000_123),
                    "[occurred 2023-11-14T22:13:20.123Z] ",
                ),
                (None, "[occurrence time unknown] "),
                (Some(0), "[occurrence time unknown] "),
                (Some(u64::MAX), "[occurrence time unknown] "),
            ] {
                let turn = BurstTurn::attributed(is_self, "Zephyr", "Kestrel, ready.", time);
                let body = if is_self {
                    "Kestrel, ready."
                } else {
                    "Zephyr (to you): Kestrel, ready."
                };
                assert_eq!(
                    turn_message_line_addressed(&turn, &participants, "Kestrel"),
                    format!("{prefix}{body}")
                );
            }
        }
        let mut opaque = BurstTurn::opaque("Unattributed replay input.");
        opaque.occurred_at_ms = Some(1_700_000_000_123);
        assert_eq!(
            turn_message_line_addressed(&opaque, &participants, "Kestrel"),
            opaque.content,
            "opaque input keeps its existing framing contract"
        );
    }
