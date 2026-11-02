#![cfg(test)]

use super::*;

// what this catches: 2e2a92f6 — small reductions were ignored by the live
// contribute path, while a local unwrap_or test incorrectly claimed coverage.
#[tokio::test]
async fn live_window_reductions_bound_actual_requests_and_persist() {
    use crate::cognition::workspace::Burst;

    for (live, expected) in [
        (Some(32_767), 32_767), // one-token reduction
        (Some(29_440), 29_440), // below the old shrink deadband
        (Some(28_672), 28_672), // exactly the old deadband
        (Some(16_384), 16_384), // large reduction
        (Some(36_864), 32_768), // growth at the deadband stays sticky
        (Some(36_865), 36_865), // material growth is adopted
        (None, 32_768),         // dedicated/cloud/not-ready: binding owns capacity
    ] {
        let mut scripted =
            ScriptedAdapter::new(vec![make_response(FinishReason::Stop, "PASS", None)]);
        scripted.live_window = live;
        let adapter = Arc::new(scripted);
        let binding = model_binding(adapter.clone(), Some("original-model".into()), 32_768);
        let faculty = LlmDeliberationFaculty::new(
            Uuid::new_v4(),
            "Reviewer",
            "You are Reviewer.",
            adapter.clone(),
        )
        .with_model_binding(binding.clone())
        .with_tools(vec![read_tool()]);
        let mut turns: Vec<_> = (0..180)
            .map(|i| {
                BurstTurn::attributed(
                    i % 2 == 1,
                    if i % 2 == 1 { "Reviewer" } else { "Peer" },
                    format!("history_{i} ").repeat(200),
                    Some(i),
                )
            })
            .collect();
        turns.push(BurstTurn::attributed(
            false,
            "Peer",
            "Review the current work.",
            Some(181),
        ));
        let ws = Workspace::new(Burst::from_turns(
            crate::identity::ActivityRoom::mint(),
            turns,
        ));
        assert!(faculty.messages_unfitted(&ws, None).tokens() > 36_865);
        let expected_view = faculty.prompt_view_within(&ws, expected);
        assert!(expected_view.capacity_error.is_none());
        let verdict = faculty
            .contribute(&ws)
            .await
            .expect("actual faculty verdict");
        assert!(verdict.fault.is_none(), "{:?}", verdict.fault);
        assert_eq!(binding.load().context_window, expected);
        let seen = adapter.seen.lock().expect("recorded request lock");
        assert_eq!(seen.len(), 1);
        let request = &seen[0];
        let schemas = crate::cognition::tool_surface::tool_surface_tokens_of(
            request.tools.as_ref().expect("native tools retained"),
        );
        assert_eq!(schemas, faculty.prompt_view(&ws).tools.tokens);
        assert_eq!(request.max_tokens, Some(expected_view.completion_reserve));
        let cost = LlmDeliberationFaculty::framing_cost(
            request.system_prompt.as_deref().expect("system framing"),
        ) + LlmDeliberationFaculty::messages_cost(&request.messages)
            + schemas
            + request.max_tokens.expect("planned reply") as usize;
        assert!(
            cost <= expected as usize,
            "request {cost} exceeds {expected}"
        );
        assert!(request
            .messages
            .iter()
            .any(|m| m.content_text().contains("Review the current work.")));
    }
}

// what this catches: 2e2a92f6 — the old adapter's observation must not
// overwrite a re-home OR a newer window on the very same adapter/model.
#[tokio::test]
async fn live_window_observation_cannot_overwrite_a_newer_binding() {
    for same_route in [false, true] {
        let mut scripted = ScriptedAdapter::new(vec![
            make_response(FinishReason::Stop, "PASS", None),
            make_response(FinishReason::Stop, "PASS", None),
        ]);
        scripted.live_window = Some(16_384);
        let old_adapter = Arc::new(scripted);
        let new_adapter = if same_route {
            old_adapter.clone()
        } else {
            Arc::new(ScriptedAdapter::new(vec![make_response(
                FinishReason::Stop,
                "PASS",
                None,
            )]))
        };
        let binding =
            model_binding(old_adapter.clone(), Some("original-model".into()), 32_768);
        let replacement = Arc::new(ModelBinding {
            adapter: new_adapter.clone(),
            model: Some(
                if same_route {
                    "original-model"
                } else {
                    "new-model"
                }
                .into(),
            ),
            context_window: 24_576,
        });
        let weak_binding = Arc::downgrade(&binding);
        let winning_binding = replacement.clone();
        *old_adapter
            .on_window_read
            .lock()
            .expect("fixture hook lock") = Some(Box::new(move || {
            weak_binding
                .upgrade()
                .expect("faculty retains binding")
                .store(winning_binding);
        }));
        let faculty = LlmDeliberationFaculty::new(
            Uuid::new_v4(),
            "Reviewer",
            "You are Reviewer.",
            old_adapter.clone(),
        )
        .with_model_binding(binding.clone())
        .with_tools(vec![read_tool()]);
        let ws = Workspace::new("Review the current work.");
        let verdict = faculty.contribute(&ws).await.expect("first verdict");
        assert!(verdict.fault.is_none());
        assert!(
            Arc::ptr_eq(&binding.load_full(), &replacement),
            "stale observation replaced the winning binding"
        );
        {
            let seen = old_adapter.seen.lock().expect("old route requests");
            assert_eq!(seen.len(), 1);
            assert_eq!(seen[0].model.as_deref(), Some("original-model"));
            assert_eq!(
                seen[0].max_tokens,
                Some(faculty.prompt_view_within(&ws, 16_384).completion_reserve)
            );
        }
        let verdict = faculty.contribute(&ws).await.expect("next verdict");
        assert!(verdict.fault.is_none());
        let seen = new_adapter.seen.lock().expect("winning route requests");
        let request = seen.last().expect("next turn uses winning route");
        assert_eq!(seen.len(), if same_route { 2 } else { 1 });
        assert_eq!(request.model, replacement.model);
        let next_window = if same_route { 16_384 } else { 24_576 };
        assert_eq!(binding.load().context_window, next_window);
        assert_eq!(
            request.max_tokens,
            Some(
                faculty
                    .prompt_view_within(&ws, next_window)
                    .completion_reserve
            )
        );
    }
}

// what this catches: aa5888a9 — the actual contribution path must
// repack an authoritative refusal, preserve required work, and capture
// distinct failed/successful requests without accepting partial errors.
#[tokio::test]
async fn provider_context_feedback_refits_and_captures_each_attempt() {
    use crate::ai::inference_error::InferenceError;
    use crate::cognition::prompt_capture::{self, CallStatus, JsonlPromptCaptureSink};
    use crate::cognition::workspace::Burst;

    for (failed_response, residual) in [(false, false), (true, false), (false, true)] {
        let home = tempfile::tempdir().expect("isolated capture and demand storage");
        let _native = crate::paths::NativeHomeOverride::install(home.path());
        let persona = Uuid::new_v4();
        let window = if residual { 51_712 } else { 29_440 };
        let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
            if failed_response {
                FinishReason::Error
            } else {
                FinishReason::Stop
            },
            if failed_response {
                "partial text must not become a verdict"
            } else {
                "PASS"
            },
            None,
        )]));
        if !residual { adapter.rejections.lock().expect("fixture rejection queue").push_back(
            InferenceError::from_http(400, r#"{"error":{"type":"exceed_context_size_error","n_prompt_tokens":29722,"n_ctx":29440}}"#),
        );
        }
        let wm = Arc::new(crate::cognition::working_memory::WorkingMemory::new(8));
        if residual {
            let changed_wm = wm.clone();
            let mut attempt = 0;
            *adapter.on_request.lock().unwrap() = Some(Box::new(move |request| {
                let excess = match attempt {
                    0 => 337,
                    1 => 43,
                    _ => return None,
                };
                if attempt == 0 {
                    changed_wm.record_receipt("arrived_after_frozen_quote");
                }
                attempt += 1;
                Some(InferenceError::ContextExceeded {
                    requested: window - request.max_tokens.expect("actual reserved output") + excess,
                    available: window,
                })
            }));
        }
        let registry = crate::cognition::working_set::WorkingSetRegistry::new();
        for tick in 0..3 {
            registry.record_emission_in_memory(
                persona,
                834,
                0,
                crate::cognition::working_set::EmissionStop::Landed,
                tick,
            );
        }
        let faculty =
            LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter.clone())
                .with_context_window(window)
                .with_tools(vec![read_tool()])
                .with_working_memory(wm.clone())
                .with_working_set(registry)
                .with_prompt_capture(Arc::new(
                    JsonlPromptCaptureSink::open(home.path(), persona)
                        .expect("capture fixture"),
                ));
        let mut turns: Vec<_> = (0..160)
            .map(|i| {
                BurstTurn::attributed(
                    i % 2 == 0,
                    if i % 2 == 0 { "Ivar" } else { "Peer" },
                    format!("distinct_history_{i} ").repeat(100),
                    Some(i),
                )
            })
            .collect();
        let stimulus = "Review the selected activity's immutable code artifact.";
        turns.push(BurstTurn::attributed(false, "Peer", stimulus, Some(161)));
        let mut ws = Workspace::new(Burst::from_turns(
            crate::identity::ActivityRoom::mint(),
            turns,
        ));
        ws.broadcast.push(
            Contribution::context(
                FacultyId::Custom(crate::cognition::faculty_ids::ACTIVE_WORK.into()),
                "The selected card, owner and teammate review remain required.",
                1.0,
                "current held activity",
            )
            .standing_grounding(),
        );
        ws.broadcast.push(Contribution::context(
            FacultyId::Custom("frozen-grounding".into()),
                "frozen_trailing_grounding",
                1.0,
                "snapshot boundary",
            )
            .standing_grounding()
            .trailing(),
        );
        let initial = faculty.prompt_view_within(&ws, window);
        assert!(initial.capacity_error.is_none());
        let unfitted = faculty.messages_unfitted(&ws, None);
        assert_eq!(
            unfitted
                .history
                .iter()
                .filter(|m| m.content_text().contains("distinct_history_"))
                .count(),
            160,
            "fixture history must remain distinct"
        );
        assert!(unfitted.tokens() > window as usize);
        let outcome = faculty.contribute(&ws).await.expect("real faculty outcome");
        assert_eq!(outcome.fault.is_some(), failed_response);
        {
            let seen = adapter.seen.lock().expect("actual requests");
            assert_eq!(seen.len(), if residual { 3 } else { 2 });
            assert!(seen.iter().all(|request| request.messages.iter()
                .all(|m| !m.content_text().contains("arrived_after_frozen_quote"))));
            for pair in seen.windows(2) {
                assert_ne!(pair[0].request_id, pair[1].request_id);
                let ground: Vec<_> = pair[1].messages.iter().enumerate()
                    .filter(|(_, m)| m.content_text().contains("frozen_trailing_grounding"))
                    .map(|(i, _)| i).collect();
                assert_eq!(ground.len(), 1);
                let ask = pair[1].messages.iter().position(|m| m.content_text().contains(stimulus)).unwrap();
                assert!(ground[0] < ask);
                assert_eq!(pair[0].system_prompt, pair[1].system_prompt);
                assert_eq!(pair[0].max_tokens, pair[1].max_tokens);
                assert_eq!(serde_json::to_value(&pair[0].tools).unwrap(), serde_json::to_value(&pair[1].tools).unwrap());
                assert!(LlmDeliberationFaculty::messages_cost(&pair[1].messages)
                    < LlmDeliberationFaculty::messages_cost(&pair[0].messages));
            }
            assert_eq!(outcome.receipts.len(), seen.len());
            for (receipt, request) in outcome.receipts.iter().zip(seen.iter()) {
                assert_eq!(
                    Some(&receipt.submitted_request_id),
                    request.request_id.as_ref()
                );
            }
            assert!(matches!(
                outcome.receipts[0].outcome,
                crate::ai::provenance::GenerationOutcome::Faulted {
                    model: None,
                    provider: None,
                    ..
                }
            ));
            assert_eq!(
                matches!(
                    outcome.receipts.last().unwrap().outcome,
                    crate::ai::provenance::GenerationOutcome::Faulted { .. }
                ),
                failed_response
            );
            let first = &seen[0];
            let second = seen.last().unwrap();
            assert_ne!(first.request_id, second.request_id);
            assert!(
                LlmDeliberationFaculty::messages_cost(&second.messages)
                    < LlmDeliberationFaculty::messages_cost(&first.messages)
            );
            assert!(second
                .messages
                .iter()
                .any(|m| m.content_text().contains(stimulus)));
            // The held card's context survives the re-fit as the assembled-context
            // message after her view (frozen with the grounding the re-fit keeps), never
            // the system message, which is hers in every room (2026-10-23).
            assert!(second
                .messages
                .iter()
                .any(|m| m.content_text().contains("selected card, owner and teammate review")));
            assert!(!second
                .system_prompt
                .as_deref()
                .expect("system framing")
                .contains("selected card, owner and teammate review"));
            let cost = |r: &TextGenerationRequest| {
                LlmDeliberationFaculty::framing_cost(r.system_prompt.as_deref().expect("system"))
                    + LlmDeliberationFaculty::messages_cost(&r.messages)
                    + crate::cognition::tool_surface::tool_surface_tokens_of(
                        r.tools.as_deref().expect("selected tools"),
                    )
            };
            let measured_first = if residual { window - first.max_tokens.unwrap() + 337 } else { 29_722 };
            let observed = PromptCalibration::observed(cost(first), measured_first, None)
                .expect("positive actual request observation");
            assert!(
                observed.charge(cost(second))
                    + second.max_tokens.expect("planned reserve") as usize
                    <= window as usize
            );
        }
        let page = prompt_capture::page(home.path(), persona, None, false, 10)
            .expect("actual capture lifecycle");
        let terminals: Vec<_> = page
            .entries
            .iter()
            .filter(|entry| !matches!(entry.status, CallStatus::Submitted))
            .collect();
        assert_eq!(terminals.len(), if residual { 3 } else { 2 });
        assert!(matches!(terminals[0].status, CallStatus::Failed));
        assert_eq!(
            matches!(terminals.last().unwrap().status, CallStatus::Failed),
            failed_response
        );
        assert_eq!(
            matches!(terminals.last().unwrap().status, CallStatus::Completed),
            !failed_response
        );
        assert_ne!(terminals[0].request_id, terminals[1].request_id);
    }
}

// what this catches: aa5888a9 — a minimal request that the provider
// rejects must fault rather than resend or cut its required stimulus.
#[tokio::test]
async fn provider_context_feedback_faults_when_required_input_cannot_fit() {
    let home = tempfile::tempdir().expect("isolated demand storage");
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let persona = Uuid::new_v4();
    let registry = crate::cognition::working_set::WorkingSetRegistry::new();
    let wm = Arc::new(crate::cognition::working_memory::WorkingMemory::new(8));
    wm.record_receipt("The original immutable artifact is still pending.");
    let window = 16_384;
    let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
        FinishReason::Stop,
        "PASS",
        None,
    )]));
    adapter
        .rejections
        .lock()
        .expect("fixture rejection queue")
        .push_back(
            crate::ai::inference_error::InferenceError::ContextExceeded {
                requested: window * 1_000,
                available: window,
            },
        );
    let faculty =
        LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter.clone())
            .with_context_window(window)
            .with_working_memory(wm.clone())
            .with_working_set(registry.clone());
    let mut ws = Workspace::new("This complete stimulus is irreducible.");
    ws.now_ms = Some(1_000);
    let outcome = faculty.contribute(&ws).await.expect("fault contribution");
    assert!(outcome.fault.is_none());
    assert!(outcome.capacity_refusal.is_some());
    assert_eq!(
        outcome.receipts.len(),
        1,
        "dispatch evidence exists without disk capture"
    );
    assert!(matches!(
        outcome.receipts[0].outcome,
        crate::ai::provenance::GenerationOutcome::Faulted { .. }
    ));
    // ...and it names the allocation its prompt was fitted under (FOCUS-CONTRACT rule 8): a
    // lived call that can't say which attention decision shaped it gives a learned focus policy
    // nothing to learn from. Every claim's grant is within what it asked for.
    let allocation = outcome.receipts[0].allocation.as_ref().expect("the call names its allocation");
    assert_eq!(allocation.window, window, "the window the one allocation ran over");
    assert!(!allocation.claims.is_empty());
    assert!(
        allocation.claims.iter().all(|c| c.grant <= c.want.max(c.floor)),
        "no claim granted past its want: {:?}",
        allocation.claims
    );
    ws.now_ms = Some(180_000); // Clock and diagnostic churn do not replace the pending work.
    wm.record_fact("The provider refused the previous prompt; the artifact remains pending.");
    ws.turns.push(BurstTurn::perception(
        "Provider diagnostic: context capacity exceeded.",
    ));
    ws.broadcast.push(Contribution::deliberation_fault(
        "Provider refused the previous prompt.",
    ));
    let next = faculty
        .contribute(&ws)
        .await
        .expect("retained capacity refusal");
    assert!(next.fault.is_none());
    assert!(next.capacity_refusal.is_some());
    assert!(
        next.receipts.is_empty(),
        "pre-dispatch refusal invents no generation"
    );
    assert!(
        registry
            .demand_of(persona)
            .expect("measured demand")
            .last_tokens
            > window,
        "the governor must see calibrated demand after the provider refusal"
    );
    assert_eq!(
        adapter.call_count(),
        1,
        "clock/diagnostic-only retry must preserve refusal"
    );
    let small = Workspace::new("What changed?");
    assert_eq!(
        small.room_id, ws.room_id,
        "changed payload must unlock within the same activity"
    );
    assert!(faculty.prompt_view(&small).capacity_error.is_none());
    let recovered = faculty
        .contribute(&small)
        .await
        .expect("changed small input");
    assert!(recovered.fault.is_none(), "{:?}", recovered.fault);
    let seen = adapter.seen.lock().expect("actual requests");
    assert_eq!(
        seen.len(),
        2,
        "only the original rejection and genuinely changed small input reach the adapter"
    );
    assert!(seen[0].messages.iter().any(|message| message
        .content_text()
        .contains("complete stimulus is irreducible")));
}

// what this catches (Kimi, 5090, 2026-10-06): a prompt the estimate undercounts but
// that still FITS the slot is never refused, so refusal-only calibration never
// learned it, and the reply's reserve went to the prompt: one turn in four ended at
// the slot ~950 tokens in. A reply's own engine count must refit the next prompt so
// that, priced at the measured density, the reserve fits again.
#[tokio::test]
async fn a_replys_engine_prompt_count_refits_the_next_prompt() {
    let persona = Uuid::new_v4();
    let window = 12_288u32;
    let turns: Vec<_> = (0..160)
        .map(|i| {
            BurstTurn::attributed(
                i % 2 == 0,
                if i % 2 == 0 { "Ivar" } else { "Peer" },
                format!("distinct_history_{i} ").repeat(100),
                Some(i),
            )
        })
        .collect();
    let ws = Workspace::new(crate::cognition::workspace::Burst::from_turns(
        crate::identity::ActivityRoom::mint(),
        turns,
    ));
    let adapter = Arc::new(ScriptedAdapter::new(vec![]));
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter.clone())
        .with_context_window(window);
    let before = faculty.prompt_view(&ws);
    // precondition: the window binds (her history was cut to fit it), or a refit has
    // nothing to show and this test would pass for the wrong reason
    let shown = |view: &DeliberationPromptView| {
        view.messages.iter().filter(|m| m.content_text().contains("distinct_history_")).count()
    };
    assert!(shown(&before) < 160, "fixture must bind its window: {} of 160 turns shown", shown(&before));
    // the engine bills 25% more than the estimate, as Kimi's lane did
    let billed = (before.estimated_prompt_tokens as u32) * 5 / 4;
    let mut reply = make_response(FinishReason::Stop, "PASS", None);
    reply.usage.input_tokens = billed;
    adapter.responses.lock().expect("fixture queue").push_back(reply);
    assert!(faculty.contribute(&ws).await.expect("verdict").fault.is_none());

    let after = faculty.prompt_view(&ws);
    // When the first prompt did not fit at the measured density, the next one is smaller.
    // (Her view may have left room already: an epoch starts at half what its grant buys.)
    let before_priced = (before.estimated_prompt_tokens as u64 * 5).div_ceil(4);
    if before_priced + u64::from(before.completion_reserve) > u64::from(window) {
        assert!(
            after.estimated_prompt_tokens < before.estimated_prompt_tokens,
            "the measured density must make room: {} then {}",
            before.estimated_prompt_tokens,
            after.estimated_prompt_tokens
        );
    }
    let priced = (after.estimated_prompt_tokens as u64 * 5).div_ceil(4);
    assert!(
        priced + u64::from(after.completion_reserve) <= u64::from(window),
        "at the measured density the reply's reserve fits: {priced} + {} > {window}",
        after.completion_reserve
    );
}

// The fitted view observed 4k; the same adapter grows before refusal is
// published. Recording readiness only at refusal would park on the new 8k.
#[tokio::test]
async fn capacity_growth_during_assembly_is_not_lost_when_refusal_is_published() {
    let scripted = ScriptedAdapter::new(vec![]);
    *scripted.window_sequence.lock().unwrap() = [4096, 8192].into();
    let adapter = Arc::new(scripted);
    let faculty = LlmDeliberationFaculty::new(
        Uuid::new_v4(), "Reviewer", "You are Reviewer.", adapter.clone(),
    ).with_context_window(4096).with_tools(vec![read_tool()]);
    let ws = Workspace::new("required original request ".repeat(10_000));
    let outcome = faculty.contribute(&ws).await.expect("typed refusal");
    let refusal = outcome.capacity_refusal.expect("input was not discarded to fit");
    let readiness = refusal.readiness.expect("live owner witness");
    assert_eq!(readiness.observed_window(), 4096);
    tokio::time::timeout(std::time::Duration::from_secs(1), readiness.wait())
        .await.expect("already published growth must not require another notification");
    assert_eq!(adapter.call_count(), 0, "refused assembly never generates");
}
