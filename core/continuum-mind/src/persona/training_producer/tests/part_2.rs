#![cfg(test)]
use super::*;

    #[test]
    pub(super) fn a_stamped_turn_keys_on_domain_role_a_failed_one_submits_nothing_unstamped_is_unchanged() {
        let classifier = DomainClassifier::new();
        let long = "Pool them behind one supervised task and hand out permits; \
                    a websocket per request will exhaust file descriptors long \
                    before it exhausts memory, and the reconnect storm is worse \
                    than the original load. Bound the pool and queue the overflow.";

        let unstamped = plan(&classifier, "How do I pool websockets?", long, None)
            .expect("test: a substantive turn plans a bucket");
        assert_eq!(
            unstamped.bucket_key(),
            unstamped.trait_kind,
            "an unstamped live turn must key on the bare domain exactly as before cc34ac0f"
        );

        let card = Uuid::from_u128(0xcc34ac0f);
        let stamp = OutcomeStamp {
            card_id: card,
            role: CreditRole::Owner,
            outcome: true,
        };
        let stamped = plan(&classifier, "How do I pool websockets?", long, Some(stamp))
            .expect("test: a stamped turn still clears the quality gate");
        assert_eq!(
            stamped.bucket_key(),
            format!("{}/owner", stamped.trait_kind),
            "a stamped turn keys on domain x role — there is no outcome segment, \
             because a failed turn never reaches a bucket at all"
        );

        // what this catches: the `/failed` bucket coming back. An earlier draft of
        // cc34ac0f filed a failed outcome under `domain/role/failed`, reasoning that
        // "learning what did not work is the other half of the curriculum". That is
        // true of a PREFERENCE corpus and false of this one: `TrainingExample` is
        // `{prompt, completion}` with no chosen/rejected anywhere in the tree, so
        // every bucket is an SFT target and a `/failed` bucket TRAINS THE FAILURE.
        // Until a preference-pair schema exists, a verified failure submits nothing.
        //
        // THE FIXTURE MUST CLEAR THE QUALITY GATE ON ITS OWN, or this test proves
        // nothing. `score_interaction_quality` weights `task_success` at 0.25 and
        // scores a failed outcome 0.2, so a failed turn scores `0.20 + 0.3*substance`
        // — and `long` above (237 chars → substance 0.7) lands at 0.41, BELOW
        // MIN_TRAINING_QUALITY 0.45. With that fixture the gate refuses the turn and
        // the evidence floor is never reached: deleting the floor entirely left this
        // assertion green. Found by mutation, not by reading.
        //
        // `substantial` is ≥500 chars → substance 0.9 → 0.47, which CLEARS the gate.
        // The positive control below proves it: same text, outcome `true`, plans a
        // bucket. So `None` for the failed case can only come from the floor.
        let substantial = "Pool them behind one supervised task and hand out permits; a websocket \
             per request will exhaust file descriptors long before it exhausts memory, and the \
             reconnect storm is worse than the original load. Bound the pool and queue the \
             overflow, then shed load at the queue rather than at accept() — a refused connection \
             the caller can retry is cheaper than a half-open socket nobody owns. Size the permit \
             count from the descriptor ceiling, not from a guess, and make the queue depth the \
             knob you actually tune under pressure.";
        assert!(
            substantial.len() >= 500,
            "fixture must clear the substance ladder's 500-char step, or the quality \
             gate refuses the turn and this test cannot see the floor at all"
        );

        // POSITIVE CONTROL: the same text with a PASSING verdict must still plan.
        // Without this, a fixture that happened to be gated out would make the
        // failed-case assertion vacuous — which is exactly the bug this block had.
        assert!(
            plan(
                &classifier,
                "How do I pool websockets?",
                substantial,
                Some(OutcomeStamp {
                    outcome: true,
                    role: CreditRole::Reviewer,
                    ..stamp
                }),
            )
            .is_some(),
            "control: this fixture clears the quality gate when the verdict PASSES, \
             so a None below is the evidence floor and not the gate"
        );

        assert!(
            plan(
                &classifier,
                "How do I pool websockets?",
                substantial,
                Some(OutcomeStamp {
                    outcome: false,
                    role: CreditRole::Reviewer,
                    ..stamp
                }),
            )
            .is_none(),
            "a turn credited to a FAILED card must produce no training example — \
             not a differently-named bucket"
        );

        // The verdict must reach the example itself, not just the key.
        let params = build_submit_params(Uuid::from_u128(7), "Cass", "qwen", &stamped, "live-turn");
        let meta = &params["examples"][0]["metadata"];
        assert_eq!(meta["cardId"], json!(card));
        assert_eq!(meta["role"], json!("owner"));
        assert_eq!(meta["outcome"], json!(true));
        assert_eq!(
            meta["domain"],
            json!(stamped.trait_kind),
            "metadata keeps the BARE domain so a stamped example stays queryable \
             alongside its unstamped siblings"
        );
        assert_eq!(
            params["traitKind"],
            json!(stamped.bucket_key()),
            "the widened bucket is what the example is filed under"
        );
        // The regression that would be invisible: the gym lookup keys on the BARE
        // domain. If it ever reads the widened bucket, every credited example loses
        // its eval set silently.
        //
        // THE OLD VERSION OF THIS CHECK NEVER GUARDED ANYTHING, and it took Astra's
        // review of 17e8c2f37 plus a precondition assert to prove it. It compared
        // `gym_for_trait(trait_kind).is_some()` against `params["evalSet"].is_some()`
        // — but `build_submit_params` inserts `evalSet` IFF that same lookup is Some,
        // and the fixture above classifies as `conversation`, WHICH HAS NO GYM. Both
        // sides were `false`, so it passed as `false == false`, identically whether
        // the production lookup used the bare domain or the widened bucket. The exact
        // regression it was written for could not have failed it.
        //
        // So this is re-anchored on a fixture that MAPS TO A REAL GYM. `code` is the
        // one trait with a committed eval set, and the path is asserted literally
        // rather than as `is_some()`: "there is some gym" is the weaker claim that let
        // the tautology hide.
        let code_reply = "Here is the fix: the bug is a null deref in the cargo build \
            script. Add an `if let Some(x) = opt` guard before the `.unwrap()`, return \
            an `Err` with the missing-field name, and the async function compiles and \
            the test passes against the typescript interface.";
        let code_stamped = plan(
            &classifier,
            "Why does my Rust function panic?",
            code_reply,
            Some(stamp),
        )
        .expect("a substantive code reply with a PASSING verdict must plan");
        assert_eq!(
            code_stamped.trait_kind, "code",
            "fixture must classify as `code`, or the eval-path assertions below go \
             vacuous the way the `conversation` fixture did"
        );
        assert_eq!(
            crate::cognition::gym::gym_for_trait(&code_stamped.trait_kind),
            Some("docs/genome/coder-eval.jsonl"),
            "the BARE domain must resolve to the coder gym — the exact path, not merely some path"
        );
        // The regression, stated positively: the WIDENED bucket has no gym at all, so
        // a lookup that keyed on it would drop the eval set. This is the assertion the
        // old `false == false` comparison was supposed to be making.
        assert_eq!(
            crate::cognition::gym::gym_for_trait(&code_stamped.bucket_key()),
            None,
            "the widened bucket ({}) must NOT resolve to a gym — which is exactly why \
             the lookup has to use the bare domain",
            code_stamped.bucket_key()
        );
        let code_params = build_submit_params(
            Uuid::from_u128(7),
            "Cass",
            "qwen",
            &code_stamped,
            "live-turn",
        );
        assert_eq!(
            code_params["evalSet"],
            json!("docs/genome/coder-eval.jsonl"),
            "the dispatched example must carry the BARE domain's eval set, so the L3 \
             sentinel can A/B the gene it produces"
        );
    }

    #[test]
    pub(super) fn substantive_turn_plans_a_bucket_trivial_turn_is_gated() {
        let classifier = DomainClassifier::new();

        // A trivial reply: below MIN_TRAINING_QUALITY → no plan.
        assert!(
            plan(&classifier, "thanks!", "ok", None).is_none(),
            "a one-word acknowledgement must be gated out of the training corpus"
        );

        // A substantive reply: clears the bar → a plan into a non-empty bucket.
        let long = "Here is how you implement connection pooling for websockets: \
            keep a fixed-size pool of live sockets, hand them out on acquire, return \
            them on release, and health-check idle ones on a timer so a dead socket \
            is replaced before a caller ever sees it. Cap the pool and queue waiters \
            so a burst cannot exhaust file descriptors.";
        let p = plan(&classifier, "How do I pool websockets?", long, None)
            .expect("a substantive reply must clear the quality gate");
        assert!(!p.trait_kind.is_empty(), "must be bucketed by a domain");
        assert_eq!(p.completion, long, "the reply is the training completion");
        assert!(
            p.quality >= MIN_TRAINING_QUALITY,
            "the carried quality must be the score that cleared the gate: {}",
            p.quality
        );
    }

    // what this catches: the recipe's {trait → gym} edge that lets the AUTOMATIC
    // loop close. A code turn classifies as the `code` trait, which resolves to a
    // committed gym — so the gene the producer dispatches carries an eval_set the
    // L3 sentinel can A/B and adopt. Before this map the producer omitted eval_set
    // and every auto-produced gene was unadoptable (only hand-dispatched jobs
    // closed the loop). The wiring inside `produce` stamps this same lookup onto
    // the submit params.
    #[test]
    pub(super) fn a_code_turn_auto_produces_a_measurable_gene() {
        let classifier = DomainClassifier::new();
        let code_reply = "Here is the fix: the bug is a null deref in the cargo build \
            script. Add an `if let Some(x) = opt` guard before the `.unwrap()`, return \
            an `Err` with the missing-field name, and the async function compiles and \
            the test passes against the typescript interface.";
        let p = plan(
            &classifier,
            "Why does my Rust function panic?",
            code_reply,
            None,
        )
        .expect("a substantive code reply must clear the quality gate");
        assert_eq!(
            p.trait_kind, "code",
            "a code turn must bucket as the code trait"
        );
        assert_eq!(
            crate::cognition::gym::gym_for_trait(&p.trait_kind),
            Some("docs/genome/coder-eval.jsonl"),
            "the code trait must resolve to a measuring gym so the gene is adoptable"
        );
    }

    // what this catches: the being-loop's THIRD axis reaching the live flywheel.
    // plan_received returns a plan UNCONDITIONALLY — no chat-quality gate, because a
    // deliberately-shared lesson's salience is its PROVENANCE (ReceivedSalience), not a
    // score built for live (stimulus→reply) turns. It still classifies a domain (bucket
    // non-empty) so the lesson lands in a gym that MEASURES it — the sentinel then adopts
    // the auto-produced gene only if it lifts the benchmark (#59). This is telepathy
    // reaching weights: a lesson one agent learns becomes another's trained capability.
    #[test]
    pub(super) fn received_lesson_skips_chat_quality_gate_but_stays_classified() {
        let classifier = DomainClassifier::new();

        // A terse lesson — the kind the live-turn quality gate is designed to drop —
        // still produces a plan, because plan_received returns SubmitPlan, not Option.
        let p = plan_received(&classifier, "airc", "the call room IS the airc room");
        assert_eq!(p.prompt, "airc", "the topic frames the lesson");
        assert_eq!(
            p.completion, "the call room IS the airc room",
            "the lesson is the trained-in completion"
        );
        assert_eq!(
            p.quality, 1.0,
            "provenance IS the quality signal — a deliberately shared lesson is not gated"
        );
        assert!(
            !p.trait_kind.is_empty(),
            "still classified into a bucket so it maps to a measuring gym"
        );
    }

    // what this catches: 41f4e3ef — a turn can change serving lanes; a scalar
    // summary must never attribute all its generations to the first model.
    #[test]
    pub(super) fn served_summary_requires_one_actual_lane_and_retains_request_identity() {
        use crate::ai::provenance::{GenerationOutcome, GenerationReceipt};
        let mut receipts = vec![receipt("failed-before-serving")];
        assert!(served_provenance(&receipts).is_none());
        receipts.push(GenerationReceipt {
            submitted_request_id: "submitted-a".into(),
            outcome: GenerationOutcome::Served {
                model: "actual-a".into(),
                provider: "provider-a".into(),
                provider_request_id: Some("provider-request-a".into()),
            },
            capture: None,
            genes: Vec::new(),
            gene_scales: Vec::new(),
            allocation: None,
        });
        receipts.push(receipt("failed-after-serving"));
        let homogeneous = served_provenance(&receipts).unwrap();
        assert_eq!(homogeneous.model, "actual-a");
        assert_eq!(homogeneous.request_id, "provider-request-a");
        receipts.push(GenerationReceipt {
            submitted_request_id: "submitted-b".into(),
            outcome: GenerationOutcome::Served {
                model: "actual-b".into(),
                provider: "provider-a".into(),
                provider_request_id: None,
            },
            capture: None,
            genes: Vec::new(),
            gene_scales: Vec::new(),
            allocation: None,
        });
        assert!(served_provenance(&receipts).is_none());
        let last = receipts.last_mut().unwrap();
        last.outcome = GenerationOutcome::Served {
            model: "actual-a".into(),
            provider: "provider-b".into(),
            provider_request_id: None,
        };
        assert!(served_provenance(&receipts).is_none());
        assert_eq!(
            served_provenance(&receipts[3..]).unwrap().request_id,
            "submitted-b"
        );
    }

    // what this catches: the ACTED chain rendering non-deterministically or dropping
    // the calls — the completion a passed card lifts into her curriculum.
    #[test]
    pub(super) fn the_acted_chain_renders_intent_then_every_call_in_order() {
        let calls = vec![
            crate::ai::types::ToolCall {
                id: "a".into(),
                name: "code/read".into(),
                input: json!({"path": "x.py"}),
            },
            crate::ai::types::ToolCall {
                id: "b".into(),
                name: "code/edit".into(),
                input: json!({"path": "x.py", "mode": "replace"}),
            },
        ];
        let c = acted_completion("  fix the qop quoting  ", &calls);
        assert_eq!(c, "fix the qop quoting\ncode/read({\"path\":\"x.py\"})\ncode/edit({\"mode\":\"replace\",\"path\":\"x.py\"})\n");
        assert_eq!(acted_completion("", &[]), "");
    }

    // what this catches: the chain dropping an act or reordering it — the completion a
    // passed card lifts must be the turn as she took it.
    #[test]
    pub(super) fn the_act_chain_keeps_every_act_in_order() {
        let c = |n: &str| crate::ai::types::ToolCall {
            id: n.into(),
            name: n.into(),
            input: json!({}),
        };
        let chain = acted_chain(&[
            ("look".into(), vec![c("code/read")]),
            ("fix".into(), vec![c("code/edit"), c("code/run")]),
        ]);
        assert_eq!(
            chain,
            "look\ncode/read({})\nfix\ncode/edit({})\ncode/run({})\n"
        );
    }

    // what this catches: Kimi's real loops (58 and 84 consecutive work turns on one card,
    // each repeating the last, zero acts) settling as 58 lessons; and the rule reaching
    // past the loop (Cormac on #4452): a single spoken turn and a verify re-read after an
    // edit are kept, because only a CONSECUTIVE repeat is the loop.
    #[test]
    pub(super) fn a_loop_across_turns_on_one_card_settles_once() {
        use crate::ai::types::{TextGenerationRequest, TextGenerationResponse};
        let call = |tool: Option<(&str, serde_json::Value)>| crate::genome::fine_tuning::LivedCall {
            capture: uuid::Uuid::new_v4().to_string(),
            request: TextGenerationRequest::default(),
            response: TextGenerationResponse {
                text: "thinking it over".into(),
                finish_reason: crate::ai::FinishReason::Stop,
                model: "m".into(),
                provider: "p".into(),
                usage: crate::ai::UsageMetrics::default(),
                response_time_ms: 0,
                request_id: "r".into(),
                content: None,
                tool_calls: tool.map(|(name, input)| vec![crate::ai::ToolCall { id: uuid::Uuid::new_v4().to_string(), name: name.into(), input }]),
                reasoning: None,
                routing: None,
                error: None,
                timing: None,
            },
        };
        let card = Uuid::new_v4();
        let row = |at: u64, lived: Option<Vec<crate::genome::fine_tuning::LivedCall>>| StagedCredit {
            id: Uuid::new_v4(),
            card_id: card,
            claim_id: None,
            owner: None,
            role: None,
            receipts: Vec::new(),
            served: None,
            prompt: "p".into(),
            completion: "c".into(),
            lived,
            staged_at_ms: at,
            coursework_set: None,
            task: None,
            rejected_at_ms: None,
        };
        let read = || call(Some(("code/read", json!({"path": "xarray/core/merge.py"}))));
        let edit = || call(Some(("code/edit", json!({"path": "xarray/core/merge.py"}))));
        let rows = vec![
            row(1, Some(vec![read()])),
            row(2, Some(vec![read()])),                // the loop: repeats the turn before
            row(3, Some(vec![call(None)])),            // a spoken turn: kept
            row(4, Some(vec![call(None)])),            // a no-action run collapses
            row(5, Some(vec![call(None)])),
            row(6, None),                              // unjudgeable: breaks the run
            row(7, Some(vec![read(), edit()])),
            row(8, Some(vec![read()])),                // the verify re-read after the edit: kept
        ];
        let verdicts: Vec<Option<&str>> = across_turns(&rows).into_iter().map(|(_, d)| d).collect();
        let r = Some("repeats_the_previous_turn");
        assert_eq!(verdicts, vec![None, r, None, r, r, None, None, None]);
    }

    // what this catches: a loop trained as lessons (Kimi's rule on card ad107e18: "61 act
    // batches under one thought, no progress"). A call that repeats the previous action
    // unchanged is dropped; the first of the run is kept, and a changed action is kept.
    #[test]
    pub(super) fn a_repeated_action_is_not_a_training_example() {
        use crate::ai::types::{TextGenerationRequest, TextGenerationResponse};
        let call = |text: &str, tool: Option<(&str, serde_json::Value)>| crate::genome::fine_tuning::LivedCall {
            capture: uuid::Uuid::new_v4().to_string(),
            request: TextGenerationRequest::default(),
            response: TextGenerationResponse {
                text: text.into(),
                finish_reason: crate::ai::FinishReason::Stop,
                model: "m".into(),
                provider: "p".into(),
                usage: crate::ai::UsageMetrics::default(),
                response_time_ms: 0,
                request_id: "r".into(),
                content: None,
                tool_calls: tool.map(|(name, input)| vec![crate::ai::ToolCall { id: uuid::Uuid::new_v4().to_string(), name: name.into(), input }]),
                reasoning: Some("let me organize the situation".into()),
                routing: None,
                error: None,
                timing: None,
            },
        };
        let read = || call("", Some(("code/read", json!({"path": "src/lib.rs"}))));
        let calls = vec![
            read(),
            read(),
            read(),
            call("", Some(("code/edit", json!({"path": "src/lib.rs"})))),
            call("done, tests pass", None),
            call("done, tests pass", None),
            read(),
        ];
        let kept = not_a_loop(&calls);
        let actions: Vec<&str> = kept
            .iter()
            .map(|c| c.response.tool_calls.as_ref().map(|t| t[0].name.as_str()).unwrap_or(c.response.text.as_str()))
            .collect();
        assert_eq!(actions, vec!["code/read", "code/edit", "done, tests pass", "code/read"]);
    }
