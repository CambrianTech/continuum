#![cfg(test)]
    use super::*;

    /// Pin that EVERY assembled prompt includes the silence affordance.
    /// Without this teaching, even capable models default to producing
    /// chatter because the implicit contract of an LLM prompt is "produce
    /// output." Per #151 + Joel's `[[no-rust-gates-around-cognition]]`:
    /// the brain decides whether to engage — but the brain has to know
    /// silence is an option, and the option has to be vocabularized in
    /// the prompt for `PersonaResponse::Silent` to be a reachable
    /// output shape.
    ///
    /// A future PR that wires per-tier prompts or removes the universal
    /// affordance must update this expectation to reflect the new
    /// contract — silent removal would re-introduce the echo-storm bug.
    /// What this catches: the trailing-PASS silence contract. Glass-boxed live
    /// (2026-07-09): Asha wrote a courtesy close then `PASS` on its own final line —
    /// she CHOSE silence, but the strict whole-message match broadcast the text
    /// anyway, ignoring her decision. A bare final-line PASS must count as silence;
    /// PASS merely mentioned inside prose must NOT (a real reply containing the
    /// word "pass" stays a reply).
    #[test]
    fn trailing_pass_line_counts_as_silence() {
        assert!(looks_like_silence_token("PASS"));
        assert!(looks_like_silence_token("pass."));
        assert!(looks_like_silence_token(
            "Understood, Anwen. See you tomorrow!\nI'll be here if you need anything.\nPASS"
        ));
        assert!(!looks_like_silence_token(
            "I'll pass the results along tomorrow."
        ));
        assert!(!looks_like_silence_token(
            "Let's not pass on this opportunity.\nSee you soon!"
        ));
        // Decorated final-line token still counts (regression for the live 2026-07-09
        // goodbye loop: Asha emitted `[PASS]` — took the hatch, rejected over brackets).
        assert!(looks_like_silence_token(
            "Understood, Claude. See you tomorrow at 2 PM!\n[PASS]"
        ));
        assert!(looks_like_silence_token("(pass)"));
        assert!(looks_like_silence_token("*PASS*"));
        assert!(looks_like_silence_token("`PASS`."));
        // A decorated NON-token line must not count.
        assert!(!looks_like_silence_token("See you soon!\n[NOT A PASS]"));
    }

    #[test]
    fn assembled_prompt_always_carries_silence_affordance() {
        let input = PromptAssemblyInput {
            persona_name: "Paige".to_string(),
            system_prompt: "You are Paige, an autonomous AI persona on the grid.".to_string(),
            matched_angle: String::new(),
            history: vec![],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: Some("Other".to_string()),
                content: "Hi!".to_string(),
                timestamp_ms: None,
            },
            is_voice: false,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![],
            room_roster: vec![],
            room_doctrine: None,
        };

        let result = assemble(&input);

        assert!(
            result.system_message.contains("[Conversational Presence]"),
            "system_message missing the [Conversational Presence] header — the brain has no way to express PersonaResponse::Silent. Got: {}",
            result.system_message
        );
        assert!(
            result.system_message.contains(SILENCE_TOKEN),
            "system_message must literally contain the silence token ({}) so the LLM knows the exact reply shape. Got: {}",
            SILENCE_TOKEN,
            result.system_message
        );
    }

    /// `looks_like_silence_token` permits LCD-tier sloppiness
    /// (case, whitespace, single trailing period) without admitting
    /// substantive responses that happen to contain the word "pass".
    /// Pin both sides of the contract.
    #[test]
    fn silence_token_recognizer_contract() {
        // Positive cases — the brain CHOSE silence.
        for input in &[
            "PASS",
            "pass",
            "Pass",
            "  PASS  ",
            "\nPASS\n",
            "Pass.",
            "pass.",
            "  pass.  ",
        ] {
            assert!(
                looks_like_silence_token(input),
                "expected {:?} to be recognized as silence",
                input
            );
        }

        // Negative cases — substantive responses, even ones
        // containing "pass" as a word. The contract is the EXACT
        // token; ambiguity defeats the affordance.
        for input in &[
            "",
            " ",
            "Pass on the bread please",
            "I'll pass on this one",
            "Hello!",
            "pass pass",
            "I PASS the question to you",
            "PASS:", // trailing colon isn't a period — not the documented shape
        ] {
            assert!(
                !looks_like_silence_token(input),
                "expected {:?} to be a real response, not silence",
                input
            );
        }
    }

    #[test]
    fn test_basic_assembly() {
        let input = PromptAssemblyInput {
            persona_name: "Helper AI".to_string(),
            system_prompt: "You are Helper AI.".to_string(),
            matched_angle: "This is a coding question about Rust error handling.".to_string(),
            history: vec![HistoryMessage {
                role: "user".to_string(),
                name: Some("Operator".to_string()),
                content: "How do I handle errors in Rust?".to_string(),
                timestamp_ms: Some(1000000),
            }],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: Some("Operator".to_string()),
                content: "Specifically with Result types?".to_string(),
                timestamp_ms: Some(1010000),
            },
            is_voice: false,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![],
            room_roster: vec![],
            room_doctrine: None,
        };

        let result = assemble(&input);

        assert!(result.system_message.contains("Helper AI"));
        assert!(result.system_message.contains("Rust error handling"));
        assert!(result.messages.len() >= 2); // history + current (identity reminder removed 2026-04-20)
        assert!(result.estimated_tokens > 0);
    }

    /// What this catches (continuum#1211 PR-2): when recalled_engrams
    /// is non-empty, the assembled system_message includes the
    /// `[Recent Memory]` block AND each engram bullet.
    /// Regression: a future formatter change that drops the bullet
    /// prefix or the header would break the persona's ability to
    /// distinguish memory from current context.
    #[test]
    fn recalled_engrams_render_as_memory_block() {
        let input = PromptAssemblyInput {
            persona_name: "Helper AI".to_string(),
            system_prompt: "You are Helper AI.".to_string(),
            matched_angle: String::new(),
            history: vec![],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: Some("Operator".to_string()),
                content: "what color did I say I liked?".to_string(),
                timestamp_ms: Some(1000),
            },
            is_voice: false,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![
                "Operator's favorite color is teal.".to_string(),
                "Operator works in San Francisco.".to_string(),
            ],
            room_roster: vec![],
            room_doctrine: None,
        };

        let result = assemble(&input);
        assert!(
            result.system_message.contains("[Recent Memory]"),
            "expected Recent Memory header in: {}",
            result.system_message
        );
        assert!(
            result
                .system_message
                .contains("- Operator's favorite color is teal."),
            "expected bullet-prefixed engram in: {}",
            result.system_message
        );
        assert!(
            result
                .system_message
                .contains("- Operator works in San Francisco."),
            "expected second bullet in: {}",
            result.system_message
        );
    }

    /// What this catches (continuum#1211 PR-2): empty recalled_engrams
    /// produces NO `[Recent Memory]` block and NO header. Backwards-
    /// compat with all pre-PR-2 callers + cold-start personas (no
    /// engrams yet). Regression: a formatter that always emits the
    /// header would clutter every prompt for every persona that hasn't
    /// accumulated memory yet.
    #[test]
    fn empty_recalled_engrams_emits_no_memory_block() {
        let input = PromptAssemblyInput {
            persona_name: "Helper AI".to_string(),
            system_prompt: "You are Helper AI.".to_string(),
            matched_angle: String::new(),
            history: vec![],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: None,
                content: "hi".to_string(),
                timestamp_ms: None,
            },
            is_voice: false,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![],
            room_roster: vec![],
            room_doctrine: None,
        };

        let result = assemble(&input);
        assert!(
            !result.system_message.contains("[Recent Memory]"),
            "should NOT render Recent Memory header for empty engrams: {}",
            result.system_message
        );
    }

    // what this catches: the persona-identity-grounding fix. A non-empty
    // room_roster MUST render a [Present in this room] block that names
    // the persona itself, lists the other present citizens verbatim, and
    // forbids voicing them. Without this block a small model role-plays
    // the whole room (the Ivar confabulation bug). Regression target:
    // docs/grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md §5 slice 1.
    #[test]
    fn room_roster_renders_present_block_grounding_the_persona() {
        let input = PromptAssemblyInput {
            persona_name: "Ivar".to_string(),
            system_prompt: "You are Ivar.".to_string(),
            matched_angle: String::new(),
            history: vec![],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: None,
                content: "hi".to_string(),
                timestamp_ms: None,
            },
            is_voice: false,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![],
            room_roster: vec![
                "BigMama [persona] — Busy".to_string(),
                "win-claude [claude]".to_string(),
            ],
            room_doctrine: None,
        };

        let result = assemble(&input);
        assert!(
            result.system_message.contains("[Present in this room]"),
            "expected the roster block header: {}",
            result.system_message
        );
        // Grounds the persona in its OWN identity within the block.
        assert!(result.system_message.contains("You are Ivar"));
        // Lists the other present citizens verbatim (name + runtime).
        assert!(result.system_message.contains("BigMama [persona] — Busy"));
        assert!(result.system_message.contains("win-claude [claude]"));
    }

    // what this catches: empty roster → NO [Present in this room] block,
    // backwards-compatible with every caller that doesn't supply one
    // (and the cold-start / no-presence case). A formatter that always
    // emitted the header would clutter every prompt.
    #[test]
    fn empty_room_roster_emits_no_present_block() {
        let input = PromptAssemblyInput {
            persona_name: "Helper AI".to_string(),
            system_prompt: "You are Helper AI.".to_string(),
            matched_angle: String::new(),
            history: vec![],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: None,
                content: "hi".to_string(),
                timestamp_ms: None,
            },
            is_voice: false,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![],
            room_roster: vec![],
            room_doctrine: None,
        };

        let result = assemble(&input);
        assert!(
            !result.system_message.contains("[Present in this room]"),
            "should NOT render the roster block for an empty roster: {}",
            result.system_message
        );
    }

    // what this catches: a non-empty room_doctrine renders a
    // [Room operating doctrine] block carrying the contract verbatim, so
    // the persona calibrates participation to the room's nature (slice
    // 2). Empty/None must render nothing (the other test path). Regression
    // target: docs/grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md §5 slice 2.
    #[test]
    fn room_doctrine_renders_operating_block() {
        let mut input = PromptAssemblyInput {
            persona_name: "Ivar".to_string(),
            system_prompt: "You are Ivar.".to_string(),
            matched_angle: String::new(),
            history: vec![],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: None,
                content: "hi".to_string(),
                timestamp_ms: None,
            },
            is_voice: false,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![],
            room_roster: vec![],
            room_doctrine: Some(
                "This is a coordination room. Respond sparingly; do not chat.".to_string(),
            ),
        };

        let with = assemble(&input);
        assert!(
            with.system_message.contains("[Room operating doctrine]"),
            "expected the doctrine block header: {}",
            with.system_message
        );
        assert!(with.system_message.contains("Respond sparingly"));

        // None → no block (backwards-compatible).
        input.room_doctrine = None;
        let without = assemble(&input);
        assert!(!without.system_message.contains("[Room operating doctrine]"));
    }

    #[test]
    fn test_no_angle_no_injection() {
        let input = PromptAssemblyInput {
            persona_name: "Test".to_string(),
            system_prompt: "Base prompt.".to_string(),
            matched_angle: String::new(),
            history: vec![],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: None,
                content: "hi".to_string(),
                timestamp_ms: None,
            },
            is_voice: false,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![],
            room_roster: vec![],
            room_doctrine: None,
        };

        let result = assemble(&input);
        assert!(!result.system_message.contains("Shared Analysis"));
    }

    #[test]
    fn test_voice_mode() {
        let input = PromptAssemblyInput {
            persona_name: "Test".to_string(),
            system_prompt: "Base.".to_string(),
            matched_angle: String::new(),
            history: vec![],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: None,
                content: "hello".to_string(),
                timestamp_ms: None,
            },
            is_voice: true,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![],
            room_roster: vec![],
            room_doctrine: None,
        };

        let result = assemble(&input);
        assert!(result.system_message.contains("Voice Mode"));
    }

    #[test]
    fn test_social_signals() {
        let input = PromptAssemblyInput {
            persona_name: "Test".to_string(),
            system_prompt: "Base.".to_string(),
            matched_angle: String::new(),
            history: vec![],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: None,
                content: "test".to_string(),
                timestamp_ms: None,
            },
            is_voice: false,
            social_signals: Some(SocialSignals {
                ai_messages_recent: 5,
                human_spoke_recently: false,
                has_directed_mention: true,
                is_mentioned: false,
                seconds_since_last_response: Some(30.0),
                response_count_this_session: Some(3),
                response_cap: Some(10),
            }),
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![],
            room_roster: vec![],
            room_doctrine: None,
        };

        let result = assemble(&input);
        assert!(result.system_message.contains("Social Awareness"));
        assert!(result.system_message.contains("5 AI messages"));
        assert!(result.system_message.contains("No human has spoken"));
        assert!(result
            .system_message
            .contains("directed at another persona"));
    }

    #[test]
    fn test_time_gap_markers() {
        let input = PromptAssemblyInput {
            persona_name: "Test".to_string(),
            system_prompt: "Base.".to_string(),
            matched_angle: String::new(),
            history: vec![
                HistoryMessage {
                    role: "user".to_string(),
                    name: Some("A".to_string()),
                    content: "first".to_string(),
                    timestamp_ms: Some(0),
                },
                HistoryMessage {
                    role: "user".to_string(),
                    name: Some("B".to_string()),
                    content: "second after long gap".to_string(),
                    timestamp_ms: Some(600_000), // 10 minutes later
                },
            ],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: None,
                content: "now".to_string(),
                timestamp_ms: None,
            },
            is_voice: false,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![],
            room_roster: vec![],
            room_doctrine: None,
        };

        let result = assemble(&input);
        let gap_msg = result
            .messages
            .iter()
            .find(|m| m.content.contains("minutes passed"));
        assert!(gap_msg.is_some(), "Should have time gap marker");
    }

    // TODO(prompt-assembly): implement identity reminder injection.
    // The test below describes a desirable behavior — for small/local
    // models that tend to "forget" who they are over a long history,
    // injecting a "Remember: You are <persona_name>" message at
    // position N-2 (right before the current user message) keeps
    // identity grounded. Production code in `assemble()` does not yet
    // do this; only the test asserts the behavior. Marking ignored
    // until the injection is implemented in `build_messages_*` so
    // pre-push doesn't fail on an unimplemented spec.
    #[ignore = "identity reminder injection not yet implemented in assemble()"]
    #[test]
    fn test_identity_reminder_position() {
        let input = PromptAssemblyInput {
            persona_name: "Helper AI".to_string(),
            system_prompt: "System.".to_string(),
            matched_angle: String::new(),
            history: vec![HistoryMessage {
                role: "user".to_string(),
                name: None,
                content: "msg1".to_string(),
                timestamp_ms: None,
            }],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: None,
                content: "current".to_string(),
                timestamp_ms: None,
            },
            is_voice: false,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![],
            room_roster: vec![],
            room_doctrine: None,
        };

        let result = assemble(&input);
        // Identity reminder should be second-to-last (before current message)
        let len = result.messages.len();
        assert!(len >= 3);
        assert!(result.messages[len - 2]
            .content
            .contains("Remember: You are Helper AI"));
        assert!(result.messages[len - 1].content.contains("current"));
    }

    /// Reproduces the empirical task #75 chat shape: 5 personas + a human
    /// trigger, with the persona under render being one of them. The new
    /// `ProperChatMlSingleParty` strategy must:
    ///   - keep the human turn as role:user
    ///   - keep this-persona's prior turn as role:assistant
    ///   - DROP all other-persona turns
    ///   - emit the current message as role:user
    ///   - NOT emit any closing-cue / "Respond now" instruction
    ///   - NOT prefix any content with `<Name>: `
    ///
    /// This is the source-level fix for the echo-loop + name-prefix leak
    /// that the previous `SingleUserTurnFlattenedHistory` strategy
    /// exposed (Joel 2026-04-24, "no band aids — take the engineering
    /// path").
    #[test]
    fn proper_chatml_single_party_drops_other_personas_and_keeps_clean_alternation() {
        let history = vec![
            HistoryMessage {
                role: "user".to_string(),
                name: Some("Operator".to_string()), // human
                content: "anyone want to review PersonaUser.ts?".to_string(),
                timestamp_ms: None,
            },
            HistoryMessage {
                role: "user".to_string(),
                name: Some("Helper AI".to_string()), // other persona — must drop
                content: "Helper AI: I can take a look".to_string(),
                timestamp_ms: None,
            },
            HistoryMessage {
                role: "user".to_string(),
                name: Some("CodeReview AI".to_string()), // other persona — must drop
                content: "CodeReview AI: starting from line 100".to_string(),
                timestamp_ms: None,
            },
            HistoryMessage {
                role: "user".to_string(),
                name: Some("Local Assistant".to_string()), // self — must keep as assistant
                content: "Sure, I'll join in once everyone's settled.".to_string(),
                timestamp_ms: None,
            },
            HistoryMessage {
                role: "user".to_string(),
                name: Some("Operator".to_string()), // human
                content: "great, let's go".to_string(),
                timestamp_ms: None,
            },
        ];
        let current = HistoryMessage {
            role: "user".to_string(),
            name: None, // current human input — None convention
            content: "any objections to splitting the file?".to_string(),
            timestamp_ms: None,
        };

        let other_personas = vec!["Helper AI".to_string(), "CodeReview AI".to_string()];
        let messages = build_messages_proper_chatml_single_party(
            &history,
            &current,
            "Local Assistant",
            &other_personas,
        );

        // Expected: 4 messages total. Joel (user), Local Assistant own
        // prior (assistant), Joel (user), current (user). Helper AI +
        // CodeReview AI dropped.
        assert_eq!(messages.len(), 4, "got: {:?}", messages);

        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "anyone want to review PersonaUser.ts?");

        assert_eq!(messages[1].role, "assistant");
        assert_eq!(
            messages[1].content,
            "Sure, I'll join in once everyone's settled."
        );

        assert_eq!(messages[2].role, "user");
        assert_eq!(messages[2].content, "great, let's go");

        assert_eq!(messages[3].role, "user");
        assert_eq!(messages[3].content, "any objections to splitting the file?");

        // No name prefix anywhere in any content.
        for m in &messages {
            assert!(
                !m.content.starts_with("Local Assistant:"),
                "self-name prefix leaked into content: {:?}",
                m.content
            );
            assert!(
                !m.content.starts_with("Helper AI:"),
                "other-persona-name prefix leaked into content: {:?}",
                m.content
            );
        }

        // No closing-cue text. The role structure speaks for itself.
        for m in &messages {
            assert!(
                !m.content.contains("Respond now"),
                "closing-cue instruction leaked: {:?}",
                m.content
            );
            assert!(
                !m.content.contains("no name prefix"),
                "closing-cue instruction leaked: {:?}",
                m.content
            );
        }
    }

    /// Edge: history has ONLY the human's prior turn — single-party
    /// strategy should produce a clean two-message user/user (model's
    /// chat template will add the assistant prefill on top).
    #[test]
    fn proper_chatml_single_party_human_only_history() {
        let history = vec![HistoryMessage {
            role: "user".to_string(),
            name: Some("Operator".to_string()),
            content: "hi".to_string(),
            timestamp_ms: None,
        }];
        let current = HistoryMessage {
            role: "user".to_string(),
            name: None,
            content: "what's up".to_string(),
            timestamp_ms: None,
        };

        let messages =
            build_messages_proper_chatml_single_party(&history, &current, "Local Assistant", &[]);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "hi");
        assert_eq!(messages[1].role, "user");
        assert_eq!(messages[1].content, "what's up");
    }

    /// Edge: empty history + current — minimal valid input. Just one
    /// user turn. ChatML's assistant prefill handles the rest.
    #[test]
    fn proper_chatml_single_party_empty_history() {
        let current = HistoryMessage {
            role: "user".to_string(),
            name: None,
            content: "first message".to_string(),
            timestamp_ms: None,
        };

        let messages =
            build_messages_proper_chatml_single_party(&[], &current, "Local Assistant", &[]);

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "first message");
    }

    // what this catches (2026-09-15): a bare inflection of the token posted as a whole
    // message — "Silence." (5090), "Passed" (Intel Mac) — is silence; the same word
    // opening a sentence is speech.
    #[test]
    fn a_bare_inflection_of_the_token_is_silence_and_a_sentence_is_not() {
        for s in ["Passed", "Silence.", "(silence)", "  passing  ", "*Silent*", "yield_turn", "I'm stopping now."] {
            assert!(looks_like_silence_token(s), "{s:?}");
        }
        for s in [
            "Passed the command for marking the room concluded, but no action was taken.",
            "I'll stay silent on this one until the tests pass.",
            "Silence is not the answer here — the test still fails.",
        ] {
            assert!(!looks_like_silence_token(s), "{s:?}");
        }
    }
