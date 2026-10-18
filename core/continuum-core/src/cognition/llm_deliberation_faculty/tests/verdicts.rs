#![cfg(test)]

use super::*;

fn make_response(
    finish: FinishReason,
    text: &str,
    tool_calls: Option<Vec<ToolCall>>,
) -> TextGenerationResponse {
    TextGenerationResponse {
        text: text.to_string(),
        finish_reason: finish,
        model: "scripted".to_string(),
        provider: "scripted".to_string(),
        usage: UsageMetrics::default(),
        response_time_ms: 0,
        request_id: "scripted".to_string(),
        content: None,
        tool_calls,
        reasoning: None,
        routing: None,
        error: None,
        timing: None,
    }
}

fn read_tool() -> NativeToolSpec {
    NativeToolSpec {
        name: "code/read".to_string(),
        description: "Read a file from the workspace".to_string(),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: json!({ "path": { "type": "string" } }),
            required: Some(vec!["path".to_string()]),
            definitions: None,
        },
    }
}

/// Adapter that replays a canned response sequence and records every
/// request it received — so a test can assert the agent loop both
/// re-generated and threaded the tool results back into the re-prompt.
/// Lean (4 methods) because the trait now defaults the long tail.
struct ScriptedAdapter {
    responses: Mutex<VecDeque<TextGenerationResponse>>,
    seen: Mutex<Vec<TextGenerationRequest>>,
    rejections: Mutex<VecDeque<crate::ai::inference_error::InferenceError>>,
    live_window: Option<u32>,
    on_window_read: Mutex<Option<WindowReadHook>>,
    on_request: Mutex<Option<RequestHook>>,
}

type WindowReadHook = Box<dyn FnOnce() + Send>;
type RequestHook = Box<dyn FnMut(&TextGenerationRequest) -> Option<crate::ai::inference_error::InferenceError> + Send>;

impl ScriptedAdapter {
    fn new(responses: Vec<TextGenerationResponse>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            seen: Mutex::new(Vec::new()),
            rejections: Mutex::new(VecDeque::new()),
            live_window: None,
            on_window_read: Mutex::new(None),
            on_request: Mutex::new(None),
        }
    }
    fn call_count(&self) -> usize {
        self.seen.lock().unwrap().len()
    }
}

#[async_trait]
impl AIProviderAdapter for ScriptedAdapter {
    fn provider_id(&self) -> &str {
        "scripted"
    }
    fn name(&self) -> &str {
        "scripted"
    }
    fn default_model(&self) -> &str {
        "scripted"
    }
    fn live_served_window(&self) -> Option<u32> {
        // Inject the exact load → observation → publication interleaving
        // without scheduler timing or a second adapter implementation.
        let hook = self
            .on_window_read
            .lock()
            .expect("fixture hook lock")
            .take();
        if let Some(hook) = hook {
            hook();
        }
        self.live_window
    }
    async fn generate_stream_checked(
        &self,
        request: TextGenerationRequest,
        sink: tokio::sync::mpsc::UnboundedSender<crate::ai::adapter::GenerationChunk>,
    ) -> Result<TextGenerationResponse, crate::ai::inference_error::InferenceError>
    {
        let injected = self.on_request.lock().expect("fixture request hook")
            .as_mut().and_then(|hook| hook(&request));
        if let Some(error) = injected {
            self.seen.lock().expect("fixture request log").push(request);
            return Err(error);
        }
        let rejection = self
            .rejections
            .lock()
            .expect("fixture rejection queue")
            .pop_front();
        if let Some(error) = rejection {
            self.seen.lock().expect("fixture request log").push(request);
            return Err(error);
        }
        self.generate_stream(request, sink)
            .await
            .map_err(Into::into)
    }

    async fn generate_text(
        &self,
        request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String> {
        self.seen.lock().unwrap().push(request);
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| "scripted adapter exhausted".to_string())
    }
}

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
        let schemas = LlmDeliberationFaculty::tool_surface_tokens_of(
            request.tools.as_ref().expect("native tools retained"),
        );
        assert_eq!(schemas, faculty.select_tool_surface(&ws, expected).tokens);
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
                let excess = match attempt { 0 => 337, 1 => 43, _ => return None };
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
            registry.record_emission_in_memory(persona, 834, 0, crate::cognition::working_set::EmissionStop::Landed, tick);
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
                FacultyId::Custom(crate::persona::active_work_source::SOURCE_ID.into()),
                "The selected card, owner and teammate review remain required.",
                1.0,
                "current held activity",
            )
            .standing_grounding(),
        );
        ws.broadcast.push(Contribution::context(
            FacultyId::Custom("frozen-grounding".into()),
            "frozen_trailing_grounding", 1.0, "snapshot boundary",
        ).standing_grounding().trailing());
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
                crate::cognition::provenance::GenerationOutcome::Faulted {
                    model: None,
                    provider: None,
                    ..
                }
            ));
            assert_eq!(
                matches!(
                    outcome.receipts.last().unwrap().outcome,
                    crate::cognition::provenance::GenerationOutcome::Faulted { .. }
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
            assert!(second
                .system_prompt
                .as_deref()
                .expect("required system context")
                .contains("selected card, owner and teammate review"));
            let cost = |r: &TextGenerationRequest| {
                LlmDeliberationFaculty::framing_cost(
                    r.system_prompt.as_deref().expect("system"),
                ) + LlmDeliberationFaculty::messages_cost(&r.messages)
                    + LlmDeliberationFaculty::tool_surface_tokens_of(
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
    assert!(outcome.fault.is_some());
    assert_eq!(
        outcome.receipts.len(),
        1,
        "dispatch evidence exists without disk capture"
    );
    assert!(matches!(
        outcome.receipts[0].outcome,
        crate::cognition::provenance::GenerationOutcome::Faulted { .. }
    ));
    ws.now_ms = Some(180_000); // Clock and diagnostic churn do not replace the pending work.
    wm.record_fact(
        "The provider refused the previous prompt; the artifact remains pending.",
    );
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
    assert!(next.fault.is_some());
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

// what this catches: repeated rejection is bounded, the next tick uses
// observed density, and unrelated small prompts do not pay a fixed excess.
#[tokio::test]
async fn provider_context_feedback_bounds_replay_and_scales_next_turn() {
    use crate::ai::inference_error::InferenceError;
    use crate::cognition::working_set::WorkingSetRegistry;
    use crate::cognition::workspace::Burst;

    let home = tempfile::tempdir().expect("isolated demand storage");
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let persona = Uuid::new_v4();
    let window = 29_440;
    let adapter = Arc::new(ScriptedAdapter::new(vec![
        make_response(FinishReason::Stop, "PASS", None),
        make_response(FinishReason::Stop, "PASS", None),
    ]));
    adapter
        .rejections
        .lock()
        .expect("fixture rejection queue")
        .extend([
            InferenceError::ContextExceeded {
                requested: 29_722,
                available: window,
            },
            InferenceError::ContextExceeded {
                requested: 29_722,
                available: window,
            },
        ]);
    let registry = WorkingSetRegistry::new();
    for tick in 0..3 {
        registry.record_emission_in_memory(persona, 834, 0, crate::cognition::working_set::EmissionStop::Landed, tick);
    }
    let faculty =
        LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter.clone())
            .with_context_window(window)
            .with_tools(vec![read_tool()])
            .with_working_set(registry);
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
    turns.push(BurstTurn::attributed(
        false,
        "Peer",
        "Review the current artifact.",
        Some(161),
    ));
    let ws = Workspace::new(Burst::from_turns(
        crate::identity::ActivityRoom::mint(),
        turns,
    ));
    assert_eq!(
        faculty
            .messages_unfitted(&ws, None)
            .history
            .iter()
            .filter(|m| m.content_text().contains("distinct_history_"))
            .count(),
        160
    );
    let fault = faculty.contribute(&ws).await.expect("bounded refusal");
    assert!(fault.fault.is_some());
    assert_eq!(
        adapter.call_count(),
        2,
        "unchanged exact measurement terminates corrective replay"
    );
    let next = faculty.contribute(&ws).await.expect("next tick");
    assert!(next.fault.is_none(), "{:?}", next.fault);
    {
        let seen = adapter.seen.lock().expect("actual requests");
        assert_eq!(seen.len(), 3);
        assert!(
            LlmDeliberationFaculty::messages_cost(&seen[2].messages)
                < LlmDeliberationFaculty::messages_cost(&seen[1].messages),
            "the previously rejected prompt must not be sent unchanged next tick"
        );
    }
    let small = Workspace::in_room("What changed?", ws.room_id);
    let (effective, calibration) = faculty.prompt_fit(&faculty.binding.load());
    let small_view = faculty.prompt_view(&small);
    assert!(
        small_view.capacity_error.is_none(),
        "{:?}",
        small_view.capacity_error
    );
    assert!(
        calibration.charge(small_view.estimated_prompt_tokens)
            + small_view.completion_reserve as usize
            <= effective as usize
    );
    let outcome = faculty
        .contribute(&small)
        .await
        .expect("small unrelated turn");
    assert!(outcome.fault.is_none(), "{:?}", outcome.fault);
    assert_eq!(adapter.call_count(), 4);

    // Re-home to a distinct provider, even at the same declared capacity.
    let replacement = Arc::new(ScriptedAdapter::new(vec![make_response(
        FinishReason::Stop,
        "PASS",
        None,
    )]));
    faculty.binding.store(Arc::new(ModelBinding {
        adapter: replacement.clone(),
        model: None,
        context_window: window,
    }));
    let pristine = faculty.prompt_view_within(&ws, window);
    assert_eq!(
        faculty.prompt_view(&ws).estimated_prompt_tokens,
        pristine.estimated_prompt_tokens
    );
    assert!(faculty
        .contribute(&ws)
        .await
        .expect("new route")
        .fault
        .is_none());
    assert_eq!(replacement.call_count(), 1);
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
    // precondition: the fit binds (history was trimmed to the window), or a refit
    // has nothing to show and this test would pass for the wrong reason
    assert!(
        before.estimated_prompt_tokens as u64 + u64::from(before.completion_reserve)
            >= u64::from(window) * 9 / 10,
        "fixture must fill its window: {} + {} of {window}",
        before.estimated_prompt_tokens,
        before.completion_reserve
    );
    // the engine bills 25% more than the estimate, as Kimi's lane did
    let billed = (before.estimated_prompt_tokens as u32) * 5 / 4;
    let mut reply = make_response(FinishReason::Stop, "PASS", None);
    reply.usage.input_tokens = billed;
    adapter.responses.lock().expect("fixture queue").push_back(reply);
    assert!(faculty.contribute(&ws).await.expect("verdict").fault.is_none());

    let after = faculty.prompt_view(&ws);
    assert!(
        after.estimated_prompt_tokens < before.estimated_prompt_tokens,
        "the measured density must make room: {} then {}",
        before.estimated_prompt_tokens,
        after.estimated_prompt_tokens
    );
    let priced = (after.estimated_prompt_tokens as u64 * 5).div_ceil(4);
    assert!(
        priced + u64::from(after.completion_reserve) <= u64::from(window),
        "at the measured density the reply's reserve fits: {priced} + {} > {window}",
        after.completion_reserve
    );
}

// what this catches: f09424d4, Kimi's actual ask vanished while the
// newest one-line result became only board counts / a clipping notice.
// Inspect the REQUEST the adapter received, including the paid schemas
// and planned reply reserve, not an intermediate text helper.
#[tokio::test]
async fn prompt_preserves_stimulus_and_large_action_on_the_wire() {
    // Exercise every byte remainder at the grounding boundary: a floor
    // per name/body used to charge less than their concatenated bytes.
    for padding in 0..3 {
        use crate::cognition::working_memory::WorkingMemory;
        use crate::cognition::workspace::Burst;

        let window = 32_768u32;
        let persona = Uuid::new_v4();
        let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
            FinishReason::Stop,
            "PASS",
            None,
        )]));
        let wm = Arc::new(WorkingMemory::new(8));
        wm.set_served_window(window);
        let result = format!(
            "src/lib.rs:42: concrete_review_target\n{{\"payload\":\"{}\"}}\nclaimable_now: 29\ntotal_on_board: 43",
            "x".repeat(window as usize * 2),
        );
        wm.record_receipt(&result);
        let faculty =
            LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter.clone())
                .with_context_window(window)
                .with_tools(vec![read_tool()])
                .with_working_memory(wm);
        let room = crate::identity::ActivityRoom::from_uuid(Uuid::new_v4()).unwrap();
        // The STIMULUS carries the squeeze now. It used to be the oversized action
        // result that forced `completion_reserve` to yield, and a result can no
        // longer do that: it is capped at its share of the window with the
        // remainder demoted to history (an unbounded required result was muting
        // citizens outright — 836 refusals, zero acts, 2026-09-10). The stimulus
        // IS still irreducible and unbounded — she cannot answer a question she was
        // not given — so the reserve-yield property this test covers is exercised
        // through the one required item that legitimately has no ceiling.
        let stimulus: &str = Box::leak(
            format!(
                "Review PR #3858 against its actual source. {}",
                "supporting detail the reviewer sent inline ".repeat(1_200)
            )
            .into_boxed_str(),
        );
        let update = Arc::new(crate::persona::service_loop::IncomingMessage {
            work: None,
            media: Vec::new(),
            event_id: Uuid::new_v4(),
            lamport: 1,
            peer_id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            text: format!(
                "A colleague's additional evidence: {}",
                "retained evidence ".repeat(80)
            ),
        });
        let mut ws = Workspace::new(Burst::from_turns(
            room,
            vec![
                BurstTurn::attributed(
                    true,
                    "Ivar",
                    "I completed the previous independent activity.",
                    Some(0),
                ),
                BurstTurn::attributed(false, "Reviewer", stimulus, Some(1)),
                BurstTurn::attributed(true, "Ivar", "I fetched the board.", Some(2)),
                BurstTurn::perception("The board projection refreshed."),
            ],
        ));
        ws.workspace_deliverable = true;
        ws.room_updates = Arc::new(vec![Arc::clone(&update)]);
        ws.broadcast.push(
            Contribution::context(
                FacultyId::Custom(crate::persona::active_work_source::SOURCE_ID.into()),
                format!(
                    "Held activity context: continue independent source review.{}",
                    "x".repeat(padding)
                ),
                0.9,
                "held claims",
            )
            .standing_grounding(),
        );

        let unfitted = faculty.messages_unfitted(&ws, None);
        assert_eq!(
            unfitted
                .history
                .iter()
                .filter(|message| message.role == "assistant")
                .count(),
            2,
            "extracting the stimulus must not coalesce the self turns on opposite sides"
        );
        // The fixture's RESULT is big — that is what this test exercises. It used
        // to assert `required_tokens()` itself exceeded the prefill target, which
        // is now impossible BY DESIGN: an action result is capped at its share of
        // the window and the remainder is demoted to history (an unbounded required
        // result was muting citizens outright — 836 refusals, zero acts, 2026-09-10).
        // The fixture property is asserted where it actually lives now.
        assert!(
            LlmDeliberationFaculty::messages_cost(&[ChatMessage::text(
                "user",
                result.clone()
            )]) > crate::inference::prefill_rate::UNMEASURED_FILL_FLOOR_TOKENS,
            "fixture must present a genuinely oversized action result — measured \
             against the substrate's own unmeasured-box fill floor, since the \
             `30 × 500` product this used to compare with was the defect"
        );
        let view = faculty.prompt_view(&ws);
        assert!(view.capacity_error.is_none(), "{view:?}");
        // The reply and the prompt negotiate in ONE allocation now: the required
        // stimulus and result are never traded, the reply always keeps its floor,
        // and above it the reply grows by priority against optional history —
        // under the act's runaway bound. What the fixture must still prove is that
        // the oversized result rides the wire inside the window (asserted below).
        assert!(
            (LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS
                ..=LlmDeliberationFaculty::ACT_OUTPUT_CAP)
                .contains(&view.completion_reserve),
            "the reply keeps its floor and stays under the act's bound: {}",
            view.completion_reserve
        );
        let contribution = faculty
            .contribute(&ws)
            .await
            .expect("a verdict or named fault");
        assert!(contribution.fault.is_none(), "{:?}", contribution.fault);
        assert_eq!(adapter.call_count(), 1);
        let seen = adapter.seen.lock().unwrap();
        let request = &seen[0];
        assert!(
            request
                .messages
                .iter()
                .any(|message| message.content_text().ends_with(&update.text)),
            "room updates must survive alongside the original task and complete result"
        );
        assert!(request
            .messages
            .iter()
            .any(|message| message.content_text().contains(stimulus)));
        // WHAT IS GUARANTEED ON THE WIRE CHANGED HERE, deliberately.
        //
        // This used to assert the ENTIRE action result arrived byte-for-byte,
        // because the result was REQUIRED input and required input is never
        // dropped. That is precisely what muted citizens: one `code/read` of a
        // large file claimed the whole window as irreducible payload and
        // `fit_messages` refused every turn — measured 2026-09-10, required 18,629
        // against 15,688 available, 836 consecutive refusals, zero acts for an
        // entire benchmark round while she held the card.
        //
        // Now the result is capped at its share and the earlier part is DEMOTED to
        // ordinary history: rendered whenever the window allows, evicted by the
        // normal front-drop when it does not, and always re-readable through its
        // spill handle. So the honest guarantee is:
        //
        //   - the result's ENDING is always on the wire (it stays required), and
        //   - nothing is DISCARDED at the split — `split_action_result` is pinned
        //     byte-exact by `an_oversized_result_is_split_not_swallowed_whole`.
        //
        // A 16k-token single line on a 32k window cannot be both demoted and
        // retained; asserting otherwise would just be asserting the mute back.
        let wire_text: String = request
            .messages
            .iter()
            .map(|message| message.content_text().to_string())
            .collect();
        let ending = result
            .rsplit_once('\n')
            .map(|(_, last)| last)
            .filter(|last| !last.is_empty())
            .unwrap_or(result.as_str());
        assert!(
            request
                .messages
                .last()
                .unwrap()
                .content_text()
                .ends_with(ending),
            "the result's ENDING is the last thing she reads — she must always see                      what her own act produced, never only a footer"
        );
        assert!(
            wire_text.contains(ending),
            "and it is genuinely on the wire, not merely implied"
        );

        assert!(request
            .system_prompt
            .as_ref()
            .unwrap()
            .contains("Held activity context"));
        assert_eq!(
            request.max_tokens,
            Some(view.completion_reserve),
            "the wire carries the plan's grant verbatim"
        );
        let schema_tokens =
            LlmDeliberationFaculty::tool_surface_tokens_of(request.tools.as_ref().unwrap());
        assert_eq!(
            schema_tokens,
            faculty.select_tool_surface(&ws, window).tokens
        );
        let wire_tokens =
            LlmDeliberationFaculty::framing_cost(request.system_prompt.as_ref().unwrap())
                + LlmDeliberationFaculty::messages_cost(&request.messages)
                + schema_tokens
                + request.max_tokens.unwrap() as usize;
        assert!(
            wire_tokens <= window as usize,
            "wire {wire_tokens} > window {window}"
        );
    }
}

/// A writing activity: history worth trading, then the ask. Shared by the flex
/// reply tests below so each compares one workspace shape.
fn writing_workspace(room: crate::identity::ActivityRoom, history: usize, deliverable: bool) -> Workspace {
    use crate::cognition::workspace::Burst;
    let mut turns: Vec<_> = (0..history)
        .map(|i| {
            BurstTurn::attributed(
                i % 2 == 1,
                if i % 2 == 1 { "Benchy" } else { "Peer" },
                format!("distinct_history_{i} ").repeat(60),
                Some(i as u64),
            )
        })
        .collect();
    turns.push(BurstTurn::attributed(
        false,
        "Peer",
        "Write the migration file now: create the accounts table with its indexes.",
        Some(history as u64 + 1),
    ));
    let mut ws = Workspace::new(Burst::from_turns(room, turns));
    ws.workspace_deliverable = deliverable;
    ws
}

// what this catches: Benchy's first write cut mid-call (2026-10-18 02:10Z). Three
// read-only turns landed at 2,702; the reply's reserve was twice that peak, 5,404;
// the write cut mid-answer at 5,404 — and the retry was sized by the same carve-off.
// Now the cut's doubled answer is in her need and the pending cut raises the act to
// priority 10: on her 46,848 window the retry's REQUEST carries at least the doubled
// cut and never past the act's runaway bound, and prompt + reply fit the window.
#[tokio::test]
async fn a_writing_turn_after_a_cut_gets_its_reply_on_a_46848_window() {
    use crate::cognition::working_memory::WorkingMemory;
    use crate::cognition::working_set::{EmissionStop, WorkingSetRegistry};
    let home = tempfile::tempdir().expect("isolated fixture emission storage");
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let window = 46_848u32;
    let persona = Uuid::new_v4();
    let reg = WorkingSetRegistry::new();
    for tick in 1..=3 {
        reg.record_emission_in_memory(persona, 2_702, 0, EmissionStop::Landed, tick);
    }
    let cut = reg.record_emission_in_memory(persona, 5_404, 0, EmissionStop::CutMidAnswer, 4);
    assert_eq!(cut.peak_tokens, 10_808, "a cut mid-answer records the answer at double");
    let wm = Arc::new(WorkingMemory::new(8));
    wm.set_served_window(window);
    let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(FinishReason::Stop, "PASS", None)]));
    let faculty = LlmDeliberationFaculty::new(persona, "Benchy", "You are Benchy.", adapter.clone())
        .with_context_window(window)
        .with_tools(persona_tools::native_tool_specs())
        .with_working_set(reg)
        .with_working_memory(Arc::clone(&wm));
    let ws = writing_workspace(crate::identity::ActivityRoom::mint(), 120, true);
    assert_eq!(faculty.turn_kind(&ws), TurnKind::Act);
    wm.retain_truncated_emission(ws.activity_room_id, cut.peak_tokens);

    let contribution = faculty.contribute(&ws).await.expect("a verdict or named fault");
    assert!(contribution.fault.is_none(), "{:?}", contribution.fault);
    let seen = adapter.seen.lock().unwrap();
    let request = &seen[0];
    let reply = request.max_tokens.expect("the reply is bounded");
    assert!(
        (10_808..=LlmDeliberationFaculty::ACT_OUTPUT_CAP).contains(&reply),
        "the retried write gets at least the doubled cut, under the act's bound: {reply}"
    );
    let wire_tokens =
        LlmDeliberationFaculty::framing_cost(request.system_prompt.as_ref().unwrap())
            + LlmDeliberationFaculty::messages_cost(&request.messages)
            + LlmDeliberationFaculty::tool_surface_tokens_of(request.tools.as_ref().unwrap())
            + reply as usize;
    assert!(wire_tokens <= window as usize, "prompt + reply {wire_tokens} > window {window}");
    assert!(
        request.messages.iter().any(|m| m.content_text().contains("Write the migration file now")),
        "the ask rides the wire"
    );
}

// what this catches: the reply taking from REQUIRED input. A mind whose measured
// turn is far larger than a small seat (15,000 tokens of think + answer on an 8,192
// window) must still see her whole stimulus and her act's result: the reply's floor
// is capped by what required input leaves, and above it the reply only trades
// optional history. No capacity fault, the reply keeps a usable floor, and the
// request fits the window.
#[tokio::test]
async fn an_8192_window_keeps_required_content_under_a_large_measured_need() {
    use crate::cognition::working_memory::WorkingMemory;
    use crate::cognition::working_set::{EmissionStop, WorkingSetRegistry};
    let home = tempfile::tempdir().expect("isolated fixture emission storage");
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let window = 8_192u32;
    let persona = Uuid::new_v4();
    let reg = WorkingSetRegistry::new();
    for tick in 1..=3 {
        reg.record_emission_in_memory(persona, 12_000, 9_000, EmissionStop::Landed, tick);
    }
    let need = reg.need_of(persona).expect("measured");
    assert!(need.total() > window, "the fixture's need exceeds the whole seat");
    let wm = Arc::new(WorkingMemory::new(8));
    wm.set_served_window(window);
    let result = format!("src/schema.rs:7: accounts_table_defined\n{}", "column detail ".repeat(40));
    wm.record_receipt(&result);
    let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(FinishReason::Stop, "PASS", None)]));
    let faculty = LlmDeliberationFaculty::new(persona, "Benchy", "You are Benchy.", adapter.clone())
        .with_context_window(window)
        .with_tools(vec![read_tool()])
        .with_working_set(reg)
        .with_working_memory(wm);
    let ws = writing_workspace(crate::identity::ActivityRoom::mint(), 40, true);

    let view = faculty.prompt_view(&ws);
    assert!(view.capacity_error.is_none(), "{:?}", view.capacity_error);
    let contribution = faculty.contribute(&ws).await.expect("a verdict or named fault");
    assert!(contribution.fault.is_none(), "{:?}", contribution.fault);
    let seen = adapter.seen.lock().unwrap();
    let request = &seen[0];
    let wire_text: String = request.messages.iter().map(|m| m.content_text().to_string()).collect();
    assert!(wire_text.contains("Write the migration file now"), "the stimulus rides the wire");
    assert!(wire_text.contains("accounts_table_defined"), "the act's result rides the wire");
    let reply = request.max_tokens.expect("the reply is bounded");
    assert!(reply >= LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS, "a usable reply: {reply}");
    let wire_tokens =
        LlmDeliberationFaculty::framing_cost(request.system_prompt.as_ref().unwrap())
            + LlmDeliberationFaculty::messages_cost(&request.messages)
            + LlmDeliberationFaculty::tool_surface_tokens_of(request.tools.as_ref().unwrap())
            + reply as usize;
    assert!(wire_tokens <= window as usize, "prompt + reply {wire_tokens} > window {window}");
}

// what this catches: priority not deciding. One workspace, one measured want (a
// pending cut doubled), two turns: an ambient pass (priority 4) and an act after the
// cut (priority 10). The act's reply grows at history's expense; the required input
// is identical in both (never part of the trade); and the plan is deterministic —
// two runs of the act are byte-identical.
#[test]
fn reply_priority_trades_history_not_required_input() {
    use crate::cognition::working_memory::WorkingMemory;
    let window = 16_384u32;
    let persona = Uuid::new_v4();
    let wm = Arc::new(WorkingMemory::new(8));
    let faculty = LlmDeliberationFaculty::new(
        persona,
        "Benchy",
        "You are Benchy.",
        Arc::new(ScriptedAdapter::new(vec![])),
    )
    .with_context_window(window)
    .with_tools(vec![read_tool()])
    .with_working_memory(Arc::clone(&wm));
    let room = crate::identity::ActivityRoom::mint();
    let pass = writing_workspace(room, 200, false);
    let act = writing_workspace(room, 200, true);
    assert_eq!((faculty.turn_kind(&pass), faculty.turn_kind(&act)), (TurnKind::Pass, TurnKind::Act));
    wm.retain_truncated_emission(act.activity_room_id, 5_404);

    let required = |ws: &Workspace| faculty.messages_unfitted(ws, None).required_tokens();
    assert_eq!(required(&pass), required(&act), "the same required input on both turns");
    let history = |view: &DeliberationPromptView| {
        view.messages.iter().filter(|m| m.content_text().contains("distinct_history_")).count()
    };
    let pass_view = faculty.prompt_view_within(&pass, window);
    let act_view = faculty.prompt_view_within(&act, window);
    for view in [&pass_view, &act_view] {
        assert!(view.capacity_error.is_none(), "{:?}", view.capacity_error);
        assert!(
            view.messages.iter().any(|m| m.content_text().contains("Write the migration file now")),
            "required input is never traded"
        );
    }
    assert!(
        act_view.completion_reserve > pass_view.completion_reserve,
        "the act's reply outranks the ambient pass's: {} vs {}",
        act_view.completion_reserve,
        pass_view.completion_reserve
    );
    assert!(
        history(&act_view) < history(&pass_view),
        "history pays for the act's reply: {} vs {} messages",
        history(&act_view),
        history(&pass_view)
    );
    let again = faculty.prompt_view_within(&act, window);
    assert_eq!(
        format!("{:?} {:?} {}", again.system, again.messages, again.completion_reserve),
        format!("{:?} {:?} {}", act_view.system, act_view.messages, act_view.completion_reserve),
        "one plan for one input, byte for byte"
    );
}

// what this catches: b61ef8d2, saturated history bought a roster prefix
// using the room wall's reservation and silently dropped that wall. Inspect
// actual adapter requests, both KV placements, and source-order ties.
#[tokio::test]
async fn standing_activity_grounding_survives_history_on_the_wire() {
    use crate::cognition::working_memory::WorkingMemory;
    use crate::cognition::working_set::WorkingSetRegistry;
    use crate::cognition::workspace::Burst;

    let home = tempfile::tempdir().expect("isolated fixture demand and emission storage");
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    for (window, trailing, reverse, unit_size, history_count) in [
        (29_440, false, false, 500, 200),
        (29_440, true, true, 500, 200),
        (58_880, true, false, 500, 0),
        (29_440, false, true, 6_000, 200),
        (29_440, true, false, 6_000, 200),
    ] {
        let persona = Uuid::new_v4();
        let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
            FinishReason::Stop,
            "PASS",
            None,
        )]));
        let wm = Arc::new(WorkingMemory::new(8));
        wm.set_served_window(window);
        wm.record_receipt("latest complete tool receipt");
        // Optional history does not outrank the reply reserve. Give this
        // established citizen measured emissions so the large source
        // minimum leaves real history room, rather than assuming the
        // cold half-window prior must yield to optional conversation.
        let registry = WorkingSetRegistry::new();
        for now in 1..=3 {
            registry.record_emission_in_memory(persona, 1_000, 0, crate::cognition::working_set::EmissionStop::Landed, now);
        }
        let faculty =
            LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter.clone())
                .with_context_window(window)
                .with_tools(persona_tools::native_tool_specs())
                .with_working_memory(wm)
                .with_working_set(registry);
        let mut turns: Vec<_> = (0..history_count)
            .map(|i| {
                BurstTurn::attributed(
                    i % 2 == 1,
                    if i % 2 == 1 { "Ivar" } else { "Peer" },
                    format!("distinct_history_{i} ").repeat(100),
                    Some(i),
                )
            })
            .collect();
        turns.push(BurstTurn::attributed(
            false,
            "Peer",
            "Review the team's current activity.",
            Some(201),
        ));
        let room = crate::identity::ActivityRoom::from_uuid(Uuid::new_v4())
            .expect("fixture room is non-nil");
        let mut ws = Workspace::new(Burst::from_turns(room, turns));
        ws.workspace_deliverable = true;
        let parts: Vec<_> = (0..7)
            .map(|i| format!("peer_{i} role and activity: {}", "scope_".repeat(unit_size)))
            .collect();
        let roster_text = parts.join("\n");
        let roster = Contribution::context(
            FacultyId::Custom("room-roster".into()),
            roster_text.clone(),
            0.9,
            "colleagues",
        )
        .with_parts(parts.clone())
        .standing_grounding()
        .session_stable();
        let wall_text = "Current activity: review the substrate with your colleagues; source evidence is on this wall.";
        let wall = Contribution::context(
            FacultyId::Custom("room-wall".into()),
            wall_text,
            0.9,
            "activity",
        )
        .standing_grounding();
        // The requiredness contract is independent of cache placement.
        let (roster, wall) = if trailing {
            (roster.trailing(), wall.trailing())
        } else {
            (roster, wall.session_stable())
        };
        let full_roster_tokens = GroundingPiece::whole(&roster).tokens;
        let whole_roster_exceeds_window = full_roster_tokens > window as usize;
        let case = format!(
            "window={window}, trailing={trailing}, reverse={reverse}, units={unit_size}, history={history_count}, full_roster_tokens={full_roster_tokens}"
        );
        assert_eq!(
            whole_roster_exceeds_window,
            unit_size == 6_000,
            "fixture must distinguish a potentially fitting list from an oversized list: {case}"
        );
        ws.broadcast = if reverse {
            vec![wall, roster]
        } else {
            vec![roster, wall]
        };
        if history_count > 0 {
            let framing = faculty.compose_system_split(
                "",
                &faculty.expanded_categories(&ws),
                ws.directed_at_self(),
                ws.self_initiated,
                ws.now_ms,
                LlmDeliberationFaculty::holds_live_work(&ws),
            );
            let unfitted = faculty.messages_unfitted(&ws, Some(&framing.trailing));
            let history: Vec<_> = unfitted
                .history
                .iter()
                .filter(|m| m.content_text().contains("distinct_history_"))
                .collect();
            assert_eq!(
                history.len(),
                history_count as usize,
                "fixture history must survive near-duplicate filtering and coalescing"
            );
            // The standing minimum, unfitted — this fixture asserts the WHOLE
            // minimum fits its window, so it wants the plan itself, not a
            // budget-shed one.
            let minimum = GroundingPlan::minimum(&ws);
            let fixed = LlmDeliberationFaculty::framing_cost(&framing.stable)
                + faculty.select_tool_surface(&ws, window).tokens
                + minimum.system_reserve()
                + minimum.trailing_tokens()
                + unfitted.required_tokens()
                // The reply's floor as the plan sizes it — what it takes before history grows.
                + reply_claim(
                    faculty.turn_kind(&ws),
                    faculty.working_set.as_ref().and_then(|reg| reg.need_of(persona)),
                    0,
                    LlmDeliberationFaculty::holds_live_work(&ws),
                    ws.directed_at_self(),
                )
                .0
                .min_tokens as usize;
            let cluster = history
                .iter()
                .rev()
                .take(2)
                .map(|m| LlmDeliberationFaculty::message_cost(m))
                .sum::<usize>();
            assert!(fixed + cluster < window as usize,
                "fixture must admit a whole historical pair after its minimum and reply reserve: fixed={fixed}, pair={cluster}, window={window}, units={unit_size}");
            assert!(
                unfitted.tokens() > window as usize,
                "fixture must actually saturate history"
            );
        }
        let verdict = faculty
            .contribute(&ws)
            .await
            .expect("verdict or capacity fault");
        assert!(verdict.fault.is_none(), "{case}: {:?}", verdict.fault);
        assert_eq!(adapter.call_count(), 1);
        let seen = adapter.seen.lock().expect("recording fixture lock");
        let request = &seen[0];
        let system = request
            .system_prompt
            .as_deref()
            .expect("assembled system prompt");
        let contains = |text: &str| {
            system.contains(text)
                || request
                    .messages
                    .iter()
                    .any(|m| m.content_text().contains(text))
        };
        assert!(
            contains(wall_text),
            "the complete atomic wall must survive its equal-salience roster: {case}"
        );
        if history_count == 0 {
            assert!(
                contains(&roster_text),
                "a roomy model keeps all offered units: {case}"
            );
        } else {
            assert!(
                contains(&parts[0]),
                "keep the source's complete first unit: {case}"
            );
            // History is already bounded by the existing prefill cap.
            // A small list may therefore fit whole alongside retained
            // history; saturation alone does not require its omission.
            if whole_roster_exceeds_window {
                assert!(
                    !contains(&parts[6]),
                    "a roster larger than the entire window must retain only a prefix: {case}"
                );
            }
            if !contains(&roster_text) {
                assert!(
                    contains("more not shown (context budget)"),
                    "a partial list must disclose omitted source units: {case}"
                );
            }
            assert!(
                request
                    .messages
                    .iter()
                    .any(|m| m.content_text().contains("distinct_history_")),
                "optional history retains a share after reserving the minimum: {case}"
            );
        }
        assert_eq!(system.contains("[room-roster]"), !trailing);
        if trailing {
            let position = request
                .messages
                .iter()
                .position(|m| m.content_text().starts_with("[room-roster]"))
                .expect("reserved trailing roster");
            assert!(
                request.messages[position + 1..]
                    .iter()
                    .any(|m| m.content_text().contains("latest complete tool receipt")),
                "standing grounding retains its pre-result-ring placement"
            );
            assert_eq!(
                request
                    .messages
                    .iter()
                    .filter(|m| m.content_text().starts_with("[room-roster]"))
                    .count(),
                1
            );
        }
        assert!(contains("Review the team's current activity."));
        assert!(request
            .messages
            .last()
            .expect("required result")
            .content_text()
            .ends_with("latest complete tool receipt"));
        assert!(
            !contains("distinct_history_0 "),
            "optional history must yield"
        );
        let tools = request.tools.as_ref().expect("actual native surface");
        let tool_tokens = LlmDeliberationFaculty::tool_surface_tokens_of(tools);
        assert_eq!(tool_tokens, faculty.select_tool_surface(&ws, window).tokens);
        let wire = LlmDeliberationFaculty::framing_cost(system)
            + LlmDeliberationFaculty::messages_cost(&request.messages)
            + tool_tokens
            + request.max_tokens.expect("planned completion reserve") as usize;
        assert!(
            wire <= window as usize,
            "actual request {wire} exceeds its window: {case}"
        );
    }
}

// what this catches: an indivisible oversized stimulus, room update or active
// result must fault before inference, including self-ticks with no chat turns.
// It must still publish demand, so refusal cannot freeze a small window.
//
// An oversized STANDING GROUNDING unit is the deliberate exception, and cases 3-4
// pin it: enrichment sheds, the turn proceeds. This test asserted the opposite
// until 2026-09-09, which is how the mute survived — a green test defending the
// defect. Live that day: Kimi held `hard-rs/expr_eval` on a 28,160-token lane and
// took a `work_turn` every 15 s that produced `act=0 model_ms=0`, because 613
// tokens of standing grounding did not fit and the grounding error outranked every
// other check. She kept the card while refusing, so the round could never end.
// Payload requiredness is unchanged; only the enrichment's shortfall stopped being
// fatal.
#[tokio::test]
async fn prompt_capacity_fault_keeps_demand_and_never_calls_the_adapter() {
    use crate::cognition::working_memory::WorkingMemory;
    use crate::cognition::working_set::WorkingSetRegistry;

    let window = 8192u32;
    for oversized_part in 0..5 {
        let persona = Uuid::new_v4();
        // Parts 0-2 must never reach the adapter, so an EMPTY script proves it.
        // Parts 3-4 shed their enrichment and go on to think, so they need a real
        // response — reaching the model is the outcome those cases assert.
        let adapter = Arc::new(ScriptedAdapter::new(
            if oversized_part >= 3 || oversized_part == 1 {
                vec![make_response(FinishReason::Stop, "PASS", None)]
            } else {
                vec![]
            },
        ));
        let registry = WorkingSetRegistry::new();
        let wm = Arc::new(WorkingMemory::new(8));
        wm.set_served_window(window);
        let oversized = "payload ".repeat(window as usize);
        let mut ws = if oversized_part == 0 {
            Workspace::new(&oversized)
        } else if oversized_part == 1 {
            wm.record_receipt(&format!(
                "{oversized}\nclaimable_now: 29\ntotal_on_board: 43"
            ));
            let mut ws = Workspace::new("");
            ws.turns.clear();
            ws.self_initiated = true;
            ws
        } else if oversized_part >= 3 {
            let mut ws = Workspace::new("original task stays required");
            let grounding = Contribution::context(
                FacultyId::Custom("standing-activity".into()),
                oversized,
                0.9,
                "activity",
            )
            .standing_grounding();
            ws.broadcast.push(if oversized_part == 4 {
                grounding.trailing()
            } else {
                grounding.session_stable()
            });
            ws
        } else {
            wm.record_receipt("complete active result");
            let mut ws = Workspace::new("original task stays required");
            ws.room_updates = Arc::new(vec![Arc::new(
                crate::persona::service_loop::IncomingMessage {
                    work: None,
                    media: Vec::new(),
                    event_id: Uuid::new_v4(),
                    lamport: 1,
                    peer_id: Uuid::new_v4(),
                    room_id: Uuid::new_v4(),
                    text: oversized,
                },
            )]);
            ws
        };
        ws.now_ms = Some(1);
        let faculty =
            LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter.clone())
                .with_context_window(window)
                .with_working_memory(wm)
                .with_working_set(registry.clone());
        // Parts 3-4 shed their standing grounding (#3954) and part 1 caps its
        // oversized action result and demotes the rest — all three now THINK
        // instead of refusing. What still faults is a genuinely irreducible
        // payload: the stimulus (part 0) and a room update (part 2), neither of
        // which any bound may drop without answering a question she was never
        // shown.
        let bounded_case = oversized_part >= 3 || oversized_part == 1;
        let grounding_case = bounded_case;
        let view = faculty.prompt_view(&ws);
        if grounding_case {
            assert!(
                view.capacity_error.is_none(),
                "an unaffordable STANDING SOURCE sheds; it must not refuse the                          turn (part {oversized_part}): {:?}",
                view.capacity_error
            );
        } else {
            let error = view
                .capacity_error
                .expect("indivisible input exceeds the real window");
            assert!(error.required_tokens > error.budget_tokens);
        }
        let outcome = faculty
            .contribute(&ws)
            .await
            .expect("capacity is a named substrate fault");
        if grounding_case {
            assert!(
                outcome.fault.is_none(),
                "shedding enrichment is not a fault (part {oversized_part}): {:?}",
                outcome.fault
            );
            assert_eq!(
                adapter.call_count(),
                1,
                "she reached the model — that IS the fix (part {oversized_part})"
            );
        } else {
            assert!(outcome.fault.is_some());
            assert!(
                outcome.decision.is_none(),
                "capacity refusal is not a citizen choosing Pass"
            );
            assert_eq!(
                adapter.call_count(),
                0,
                "do not submit a footer or clipping notice"
            );
        }
        // Demand is published either way — a refusal must never freeze a small
        // window, and neither must a shed.
        assert!(registry.demand_of(persona).unwrap().peak_tokens > window);
    }
}

// what this catches: a native tool-use response (FinishReason::ToolUse with
// calls) becomes a `Decision::Act` carrying those exact calls — NOT executed
// here, NOT re-generated. Single shot: exactly one model call. Regression here
// means the faculty either swallowed the action or silently ran it (the old
// in-faculty agent loop we removed). The model's separated reasoning rides
// along as the act's `intent` (so the resulting engram records WHY she acted).
#[tokio::test]
async fn tool_use_response_becomes_an_act_verdict() {
    let persona = Uuid::new_v4();
    let room = Uuid::new_v4();
    let public = "I will remove the cursor, preserve truncation, then verify the callers.";
    let memory = Arc::new(crate::cognition::working_memory::WorkingMemory::new(16));
    memory.set_scope(Some("task-a".into()));
    let call = ToolCall {
        id: "t1".to_string(),
        name: "code/read".to_string(),
        input: json!({ "path": "deploy.md" }),
    };
    let mut resp = make_response(FinishReason::ToolUse, public, Some(vec![call.clone()]));
    resp.reasoning = Some("I should check deploy.md to answer.".to_string());
    let adapter = Arc::new(ScriptedAdapter::new(vec![resp]));

    // Real window: at the ctor default the framing alone starves msg_budget
    // to 0 — the exact state the delib.prompt.starved guard now refuses
    // (these two tests silently exercised the blank-prompt bug path for
    // months; the scripted adapter masked it).
    let faculty =
        LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter.clone())
            .with_tools(vec![read_tool()])
            .with_working_memory(memory.clone())
            .with_context_window(32_768);

    let mut ws = Workspace::new("teammate asks: did the deploy fix land?");
    ws.activity_room_id = room;
    let c = faculty.contribute(&ws).await.expect("verdict");

    match c.decision {
        Some(Decision::Act { calls, intent }) => {
            assert_eq!(calls.len(), 1, "the one requested call rides the verdict");
            assert_eq!(calls[0].name, "code/read");
            assert_eq!(calls[0].input, json!({ "path": "deploy.md" }));
            assert_eq!(intent, "I should check deploy.md to answer.");
        }
        other => panic!("expected Act, got {other:?}"),
    }
    // SINGLE SHOT — the faculty never re-generated (no in-faculty loop).
    assert_eq!(adapter.call_count(), 1, "one generation, one verdict");
    memory.record_receipt("code/read returned the cursor declaration and its callers");
    // No optional WM bid: seven competing faculties evicted it in the live
    // receipt. Authored action continuity must survive that same boundary.
    assert!(ws.broadcast.is_empty());
    let prompt = faculty.messages_unfitted(&ws, None);
    let required = prompt.required_tokens();
    let fitted = faculty
        .fit_messages(prompt, required)
        .expect("required continuity fits");
    assert!(fitted.messages.iter().any(|message| {
        message.role == "assistant" && message.content_text() == public
    }));
    assert!(fitted.messages.iter().any(|message| {
        message.content_text().contains("cursor declaration and its callers")
    }));
    ws.activity_room_id = Uuid::new_v4();
    assert!(faculty.messages_unfitted(&ws, None).action_statement.is_none(), "another room cannot inherit it");
    ws.activity_room_id = room;
    memory.set_scope(Some("task-b".into()));
    assert!(faculty.messages_unfitted(&ws, None).action_statement.is_none(), "another task cannot inherit it");
    memory.set_scope(Some("task-a".into()));
    memory.record_settlement("finished", Some(room));
    assert!(faculty.messages_unfitted(&ws, None).action_statement.is_none(), "settlement ends this decision");
}

// what this catches: paging a gene into the shared genome handle flows into the
// faculty's NEXT generation request as `active_adapters` — the measured page-in
// wire the genome loop trains against. Base (empty genome) → the request carries
// no adapters; after a page-in → it carries the gene (name + scale). A
// regression here is the LIFT=0 no-op: a forged gene that never reaches the
// model because the faculty hardcoded `active_adapters: None`.
#[tokio::test]
async fn paged_in_gene_rides_into_the_generation_request() {
    let persona = Uuid::new_v4();
    let genome = empty_genome();
    let adapter = Arc::new(ScriptedAdapter::new(vec![
        make_response(FinishReason::Stop, "base answer", None),
        make_response(FinishReason::Stop, "gene answer", None),
    ]));
    let faculty =
        LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter.clone())
            .with_genome(Arc::clone(&genome));

    let ws = Workspace::new("write is_prime");
    // Pass 1: base — nothing paged in.
    faculty.contribute(&ws).await.expect("verdict");
    // Page a gene in, then generate again — the SAME faculty, one handle.
    genome.store(Arc::new(vec![ActiveAdapterRequest {
        name: "coder-0p5b".to_string(),
        path: "/genes/coder.gguf".to_string(),
        domain: String::new(),
        scale: 0.8,
    }]));
    faculty.contribute(&ws).await.expect("verdict");

    let seen = adapter.seen.lock().unwrap();
    assert_eq!(seen.len(), 2, "two generations recorded");
    assert!(
        seen[0].active_adapters.is_none(),
        "base pass carries no gene"
    );
    let paged = seen[1]
        .active_adapters
        .as_ref()
        .expect("gene paged into the candidate request");
    assert_eq!(paged.len(), 1, "exactly the one paged-in gene");
    assert_eq!(paged[0].name, "coder-0p5b");
    assert_eq!(paged[0].scale, 0.8, "the analog influence dial rides along");
}

// what this catches: a model that ignores the native tool channel and instead
// emits a tool call as JSON in its prose (small models do this) still becomes a
// `Decision::Act` — strictly better than broadcasting the raw envelope into the
// room (the observed-live failure). Only fires when tools are authorized.
#[tokio::test]
async fn text_emitted_tool_call_becomes_an_act_verdict() {
    let persona = Uuid::new_v4();
    // FinishReason::Stop (not ToolUse) but the prose IS a tool-call envelope.
    let envelope = json!({ "tool_call": { "name": "ping", "arguments": {} } }).to_string();
    let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
        FinishReason::Stop,
        &envelope,
        None,
    )]));
    let ping_spec = NativeToolSpec {
        name: "ping".to_string(),
        description: "Health check".to_string(),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: json!({}),
            required: None,
            definitions: None,
        },
    };
    // Real window (see tool_use_response_becomes_an_act_verdict — same
    // starved-default story).
    let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter)
        .with_tools(vec![ping_spec])
        .with_context_window(32_768);

    let c = faculty
        .contribute(&Workspace::new("is the core alive?"))
        .await
        .expect("verdict");
    match c.decision {
        Some(Decision::Act { calls, .. }) => {
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].name, "ping");
        }
        other => panic!("expected Act from a text-emitted tool call, got {other:?}"),
    }
}

// what this catches: the reasoning-intent-no-lift class (#181 sibling,
// deepseek4 eval battery 2026-08-03) — a thinking model commits its tool
// call inside `reasoning_content` and leaves `content` empty; the verdict
// branch must lift the LAST parseable call from the reasoning tail instead
// of settling as an empty Speak. Also pins last-wins (earlier exploration
// in the same reasoning is not the final intention) + wire-dialect mapping.
#[tokio::test]
async fn empty_content_with_reasoning_tool_intent_lifts_the_final_call() {
    let persona = Uuid::new_v4();
    let reasoning = format!(
        "I could list the directory first: {}\nActually the task names the file, so I'll just read it: {}",
        json!({ "tool_call": { "name": "code/list", "arguments": { "path": "." } } }),
        json!({ "tool_call": { "name": "code/read", "arguments": { "path": "src/main.rs" } } }),
    );
    let mut resp = make_response(FinishReason::Stop, "", None);
    resp.reasoning = Some(reasoning);
    let adapter = Arc::new(ScriptedAdapter::new(vec![resp]));
    let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter)
        .with_tools(vec![read_tool()])
        .with_context_window(32_768);

    let c = faculty
        .contribute(&Workspace::new("what does main.rs contain?"))
        .await
        .expect("verdict");
    match c.decision {
        Some(Decision::Act { calls, .. }) => {
            assert_eq!(calls.len(), 1);
            assert_eq!(
                calls[0].name, "code/read",
                "the FINAL intention wins, not the discarded exploration"
            );
            assert_eq!(calls[0].input, json!({ "path": "src/main.rs" }));
        }
        other => panic!("expected Act lifted from the reasoning tail, got {other:?}"),
    }
}

// what this catches: the guard on the reasoning lift — a committed answer in
// the content channel (real prose OR an explicit PASS) must never be
// overridden by a tool call the model merely considered in its private
// reasoning. Only an EMPTY content channel consults the reasoning tail.
#[tokio::test]
async fn committed_content_is_never_overridden_by_reasoning_calls() {
    let persona = Uuid::new_v4();
    let reasoning = format!(
        "Maybe I should verify: {}. No — I already know the answer.",
        json!({ "tool_call": { "name": "code/read", "arguments": { "path": "src/main.rs" } } }),
    );
    let mut resp = make_response(FinishReason::Stop, "It prints hello world.", None);
    resp.reasoning = Some(reasoning);
    let adapter = Arc::new(ScriptedAdapter::new(vec![resp]));
    let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter)
        .with_tools(vec![read_tool()])
        .with_context_window(32_768);

    let c = faculty
        .contribute(&Workspace::new("what does main.rs contain?"))
        .await
        .expect("verdict");
    match c.decision {
        Some(Decision::Speak { .. }) => {}
        other => panic!("expected the spoken answer to stand, got {other:?}"),
    }
}

// what this catches (BigMama's 5090 receipt, 2026-10-06): a thought CUT at the
// output limit had the last call in its tail lifted and run (a half-formed code/run
// that failed). The same reasoning as the lift test above, but ended by Length:
// nothing is lifted; the thought-cut sentinel routes, so she decides.
#[tokio::test]
async fn a_thought_cut_at_the_limit_lifts_nothing_and_hands_her_the_decision() {
    let persona = Uuid::new_v4();
    let reasoning = format!(
        "I could list the directory first: {}\nActually the task names the file, so I'll just read it: {}",
        json!({ "tool_call": { "name": "code/list", "arguments": { "path": "." } } }),
        json!({ "tool_call": { "name": "code/read", "arguments": { "path": "src/main.rs" } } }),
    );
    let mut resp = make_response(FinishReason::Length, "", None);
    resp.reasoning = Some(reasoning);
    let adapter = Arc::new(ScriptedAdapter::new(vec![resp]));
    let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter)
        .with_tools(vec![read_tool()])
        .with_context_window(32_768);

    let c = faculty
        .contribute(&Workspace::new("what does main.rs contain?"))
        .await
        .expect("verdict");
    match c.decision {
        Some(Decision::Act { calls, .. }) => {
            assert_eq!(calls.len(), 1);
            assert_eq!(
                calls[0].name,
                crate::cognition::tool_executor::command_executor::THOUGHT_CUT_SENTINEL,
                "a cut thought is reported, never lifted into code/read"
            );
        }
        other => panic!("expected the thought-cut sentinel Act, got {other:?}"),
    }
}

// what this catches: the THINK-ONLY tail of the #181 arc — a thinking model
// that ends its generation INSIDE the reasoning channel (content empty,
// reasoning present, NO liftable call anywhere in the thinking) must not
// settle as an empty Speak. The verdict routes the think-only sentinel so
// the executor's teacher fires as an observation and drive_to_settle hands
// her another generation. Glass-boxed live 2026-08-15: a full bench round
// produced 17 inference turns and zero acts through exactly this hole.
#[tokio::test]
async fn think_only_turn_routes_the_teacher_sentinel_not_empty_speak() {
    let persona = Uuid::new_v4();
    let mut resp = make_response(FinishReason::Stop, "", None);
    resp.reasoning = Some(
        "These tasks are all tractable. I should start with the parser fix, \
         then the two string tasks. Let me plan the order carefully."
            .to_string(),
    );
    let adapter = Arc::new(ScriptedAdapter::new(vec![resp]));
    let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter)
        .with_tools(vec![read_tool()])
        .with_context_window(32_768);

    let c = faculty
        .contribute(&Workspace::new("solve the tasks on the board"))
        .await
        .expect("verdict");
    match c.decision {
        Some(Decision::Act { calls, intent }) => {
            assert_eq!(calls.len(), 1);
            assert_eq!(
                calls[0].name,
                crate::cognition::tool_executor::command_executor::THINK_ONLY_SENTINEL,
                "the think-only sentinel rides the verdict so the teacher fires"
            );
            assert!(
                intent.contains("tractable"),
                "her own reasoning is the act's intent — the engram records why"
            );
        }
        other => panic!("expected the think-only sentinel Act, got {other:?}"),
    }
}

// what this catches: #293 starvation at the VERDICT BRANCH — four resident
// personas (Asha/Atlas/Anwen/Benchy) on a NativeFunctionCalling lane looped
// for HOURS: the model emitted its call as a ```json TEXT fence with
// SIBLING args ({"function": "code/list", "path": "."} — args top-level
// beside the name, finish=Stop, tool_calls=None) and nothing lifted it, so
// every turn fell through to Speak. The verdict branch must lift the text
// call into an Act regardless of the lane's declared protocol.
#[tokio::test]
async fn native_lane_text_fence_with_sibling_args_still_becomes_an_act() {
    let persona = Uuid::new_v4();
    let text = "Let me look at the workspace first.\n```json\n{\"function\": \"code/list\", \"path\": \".\"}\n```";
    let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
        FinishReason::Stop,
        text,
        None, // NO native tool_calls — the exact starved shape
    )]));
    let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter)
        .with_tools(vec![read_tool()])
        .with_context_window(32_768);

    let c = faculty
        .contribute(&Workspace::new("what's in the workspace?"))
        .await
        .expect("verdict");
    match c.decision {
        Some(Decision::Act { calls, .. }) => {
            assert_eq!(calls.len(), 1);
            assert_eq!(
                calls[0].name, "code/list",
                "wire dialect keeps the canonical name"
            );
            assert_eq!(
                calls[0].input,
                json!({ "path": "." }),
                "siblings became the args"
            );
        }
        other => panic!("expected Act from the sibling-args text fence, got {other:?}"),
    }
}

// what this catches: NATIVE-CALLS-WIN precedence (#293's guard rail) — when a
// response carries BOTH structured tool_calls and a textual fence naming a
// different tool, the structured calls are the verdict; the text parse is a
// belt-and-suspenders fallback consulted only when the response carried none.
#[tokio::test]
async fn native_structured_calls_win_over_a_text_fence() {
    let persona = Uuid::new_v4();
    let native = ToolCall {
        id: "t1".to_string(),
        name: "code/read".to_string(),
        input: json!({ "path": "deploy.md" }),
    };
    let text = "```json\n{\"function\": \"code/list\", \"path\": \".\"}\n```";
    let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
        FinishReason::ToolUse,
        text,
        Some(vec![native]),
    )]));
    let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter)
        .with_tools(vec![read_tool()])
        .with_context_window(32_768);

    let c = faculty
        .contribute(&Workspace::new("did the deploy fix land?"))
        .await
        .expect("verdict");
    match c.decision {
        Some(Decision::Act { calls, .. }) => {
            assert_eq!(calls.len(), 1, "only the native call rides the verdict");
            assert_eq!(
                calls[0].name, "code/read",
                "native wins over the text fence"
            );
        }
        other => panic!("expected Act carrying the native call, got {other:?}"),
    }
}

// what this catches: the directedness gate on the silence affordance
// (`Workspace::directed_at_self`). The ambient participation default AND the
// bare-PASS escape both live in the ONE appended [Conversational Presence] block.
// A DIRECTED turn — the eval exam, an @mention, a DM — withholds that block so the
// persona is not handed a "reply PASS, nothing reaches the room" exit on a question
// put to her (the 0/13 coder-gym failure: a coder model takes that exit on a
// directed question). An AMBIENT turn includes it (silence stays first-class, the
// considered exception). The turn-taking block itself is posture-NEUTRAL — the old
// " If you have nothing worth adding, stay silent." nudge is gone, so neither turn
// carries it. This is a FRAMING decision over a structural addressing fact — her
// output is never filtered.
#[tokio::test]
async fn directed_turn_withholds_the_silence_escape() {
    use crate::ai::types::MessageContent;
    use crate::cognition::workspace::{SalienceArbiter, Situation, TurnFraming, WorkspaceCycle};
    use crate::inference::slots::SlotClass;
    use crate::persona::persona_identity::PersonaIdentity;
    let adapter = Arc::new(ScriptedAdapter::new(vec![]));
    let faculty =
        LlmDeliberationFaculty::new(Uuid::new_v4(), "Kimi", "You are Kimi.", adapter);
    // 2026-09-01: presence framing rides the conversation tail (the KV
    // split applied for real), so these assertions read the WHOLE
    // delivered prompt — system + turns — not the system string alone.
    let whole = |v: &DeliberationPromptView| {
        let mut s = v.system.clone();
        for m in &v.messages {
            if let MessageContent::Text(t) = &m.content {
                s.push('\n');
                s.push_str(t);
            }
        }
        s
    };

    let ambient = whole(&faculty.prompt_view(&Workspace::new("just some room chatter")));
    assert!(
        ambient.contains("[Conversational Presence]"),
        "an ambient turn carries the presence/PASS affordance block"
    );
    assert!(
        !ambient.contains("stay silent"),
        "the turn-taking block is posture-neutral — no 'stay silent' nudge"
    );

    let directed = whole(
        &faculty.prompt_view(&Workspace::new("answer me: what is 2+2?").directed(true)),
    );
    assert!(
        directed.contains("This message names you"),
        "a directed turn carries the DIRECTED presence variant (never ghost a \
         question; a pure pleasantry may rest — the natural spiral-break)"
    );
    assert!(
        !directed.contains("do not need to be addressed by name"),
        "a directed turn never carries the ambient block"
    );
    assert!(
        !directed.contains("stay silent"),
        "and carries no 'stay silent' nudge on a directed turn either"
    );

    // Regression for card157c095d: a human's opportunity (including a
    // stale peer-kind classification) became the false instruction
    // "This message names you". Exercise the real framing -> cycle ->
    // prompt seam on both admission and act-result re-perception.
    let identity = PersonaIdentity::new(faculty.persona_id, "Kimi");
    let cycle = WorkspaceCycle::new(vec![], Arc::new(SalienceArbiter), 4);
    for (text, human, expected_attention, expected_class) in [
        (
            "Joel here — which card do you hold?",
            true,
            TurnAttention::HumanOpportunity,
            SlotClass::Turn,
        ),
        (
            "Astra / Codex -> IntelMac: thanks for the 3898 review.",
            false,
            TurnAttention::Ambient,
            SlotClass::Background,
        ),
        (
            "Astra / Codex -> IntelMac: thanks for the 3898 review.",
            true,
            TurnAttention::HumanOpportunity,
            SlotClass::Turn,
        ),
        (
            "Kimi, please review the patch.",
            false,
            TurnAttention::Addressed,
            SlotClass::Turn,
        ),
        (
            "Kimi, please review the patch.",
            true,
            TurnAttention::Addressed,
            SlotClass::Turn,
        ),
    ] {
        let framing = TurnFraming::message(TurnAttention::for_message(
            identity.mentions(text),
            human,
        ));
        for situation in [Situation::FreshContext, Situation::PostAction] {
            let ws = cycle.run_situated(text.into(), framing, situation).await;
            assert_eq!(ws.attention, expected_attention, "{text}");
            assert_eq!(
                deliberation_slot_class(Some("cognition/deliberation"), ws.attention),
                expected_class,
                "human opportunity retains the foreground lane: {text}",
            );
            let delivered = whole(&faculty.prompt_view(&ws));
            assert_eq!(
                delivered.contains("This message names you"),
                expected_attention == TurnAttention::Addressed,
                "the actual prompt must not turn admission priority into addressing: {text}",
            );
            assert_eq!(
                delivered.contains("do not need to be addressed by name"),
                expected_attention != TurnAttention::Addressed,
                "unmentioned messages keep ordinary participation: {text}",
            );
        }
    }
}

// what this catches: the tools gate. With NO tools authorized, a tool-call
// envelope in the prose is NOT turned into an Act — it falls through to the
// text verdict. A persona without hands can only speak.
#[tokio::test]
async fn no_tools_authorized_never_acts() {
    let envelope = json!({ "tool_call": { "name": "ping", "arguments": {} } }).to_string();
    let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
        FinishReason::Stop,
        &envelope,
        None,
    )]));
    // No `.with_tools(...)` — speak-only.
    let faculty =
        LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter);

    let c = faculty
        .contribute(&Workspace::new("anything"))
        .await
        .expect("verdict");
    assert!(
        !matches!(c.decision, Some(Decision::Act { .. })),
        "no authorized tools → never an Act verdict"
    );
}

// what this catches: THE working-memory write path — after a verdict, the
// faculty records the turn's separated reasoning into working memory so the
// persona can resume its train of thought next turn. The room only saw the
// verdict text; the reasoning lives in working memory.
// what this catches: the steps-taken ledger (PERCEPTION-FACTS.md) —
// typed Receipt entries render as [steps taken this session]; facts
// NEVER appear as steps; an empty session renders the EXPLICIT
// zero-case (the shelter-hardened void) instead of nothing.
#[test]
fn steps_ledger_renders_receipts_and_explicit_zero_case() {
    use crate::cognition::working_memory::WorkingMemory;

    let adapter = Arc::new(ScriptedAdapter::new(vec![]));
    let wm = Arc::new(WorkingMemory::new(8));
    let faculty =
        LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter)
            .with_working_memory(Arc::clone(&wm));

    // Zero-case: explicit, never absent.
    let ws = Workspace::new("hello room");
    let view = faculty.prompt_view(&ws);
    let ledger = view
        .messages
        .iter()
        .filter_map(|m| match &m.content {
            crate::ai::types::MessageContent::Text(t)
                if t.starts_with("[steps taken this session]") =>
            {
                Some(t.clone())
            }
            _ => None,
        })
        .next()
        .expect("ledger section always present when WM is wired");
    assert!(ledger.contains("nothing has executed yet"), "{ledger}");

    // With a receipt + a fact: the receipt is a step, the fact is not.
    wm.record_receipt("I ran code/list({\"path\":\".\"}) Result: 4 files");
    wm.record_fact("[unfulfilled] I said I would run commands, but no tool ran");
    let view = faculty.prompt_view(&ws);
    let ledger = view
        .messages
        .iter()
        .filter_map(|m| match &m.content {
            crate::ai::types::MessageContent::Text(t)
                if t.starts_with("[steps taken this session]") =>
            {
                Some(t.clone())
            }
            _ => None,
        })
        .next()
        .expect("ledger present");
    assert!(ledger.contains("[action #1] I ran code/list"), "{ledger}");
    assert!(
        !ledger.contains("[unfulfilled]"),
        "facts are never steps: {ledger}"
    );
    assert!(!ledger.contains("nothing has executed yet"));
}

#[tokio::test]
async fn verdict_records_reasoning_into_working_memory() {
    use crate::cognition::working_memory::WorkingMemory;

    let mut resp = make_response(FinishReason::Stop, "Ship it.", None);
    resp.reasoning = Some("Weighed the risk; the fix is small and tested.".to_string());
    let adapter = Arc::new(ScriptedAdapter::new(vec![resp]));
    let wm = Arc::new(WorkingMemory::new(3));

    let faculty =
        LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter)
            .with_working_memory(Arc::clone(&wm));

    let ws = Workspace::new("should we ship the deploy?");
    let c = faculty.contribute(&ws).await.expect("verdict");
    // The room got only the clean verdict text…
    match c.decision {
        Some(Decision::Speak { text }) => assert_eq!(text, "Ship it."),
        other => panic!("expected Speak, got {other:?}"),
    }
    // …and the reasoning was captured into working memory for next turn.
    assert_eq!(
        wm.recent(),
        vec!["Weighed the risk; the fix is small and tested."],
        "the verdict's reasoning is recorded into working memory"
    );
}

// what this catches: a suppressed-thinking turn (reasoning = None) records
// NOTHING — working memory only fills when thinking is actually on.
#[tokio::test]
async fn suppressed_thinking_records_no_working_memory() {
    use crate::cognition::working_memory::WorkingMemory;

    // make_response defaults reasoning: None (the suppressed-thinking shape).
    let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
        FinishReason::Stop,
        "144",
        None,
    )]));
    let wm = Arc::new(WorkingMemory::new(3));
    let faculty =
        LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter)
            .with_working_memory(Arc::clone(&wm));

    let _ = faculty.contribute(&Workspace::new("what is 12*12?")).await;
    assert!(wm.is_empty(), "no reasoning → nothing recorded");
}

// what this catches: an empty-text, no-tool response is still an abstain — the
// model produced nothing to say and chose no action. `decision_from_response`
// owns the empty→Pass mapping; this guards the no-tools branch reaching it.
// (The act-path abstains — no hands / exec error → None — are tested in
// `super::act_observe`, where execution actually lives.)
#[tokio::test]
async fn empty_prose_with_no_tools_is_a_pass() {
    let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
        FinishReason::Stop,
        "PASS",
        None,
    )]));
    let faculty =
        LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter);
    let c = faculty
        .contribute(&Workspace::new("anything"))
        .await
        .expect("verdict");
    assert_eq!(c.decision, Some(Decision::pass()));
}
