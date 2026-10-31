#![cfg(test)]

use super::*;

// what this catches: the live 2026-09-10 mute (a `code/read` result several times the window
// required whole: 836 consecutive refusals) WITHOUT its old cure, a head cut by a window share
// (`split_action_result`, deleted with RESULT_SHARE_DENOM: FOCUS-CONTRACT rule 4, a result is
// reduced by LEVEL in the one allocation, never cut). A legacy result (no native call ids) with
// a saved handle goes through the real `prompt_view`: never refused, and what she reads names the
// handle she can read the whole through. Native results are covered below (Aris).
#[tokio::test]
async fn a_huge_legacy_result_arrives_reduced_with_its_handle_never_refused() {
    use crate::ai::types::ResultCustody;
    use crate::cognition::working_memory::WorkingMemory;

    let window = 8_192u32;
    let wm = Arc::new(WorkingMemory::new(8));
    let huge: String = (0..6_000).map(|i| format!("line {i} of a very large tool result\n")).collect();
    wm.record_receipt_typed(&[], &huge, None, ResultCustody::Whole { handle: "legacy-evidence".into() });

    let faculty = LlmDeliberationFaculty::new(
        Uuid::new_v4(),
        "Sahar",
        "You are Sahar.",
        Arc::new(HeuristicInferenceAdapter::new()),
    )
    .with_context_window(window)
    .with_working_memory(Arc::clone(&wm));

    let view = faculty.prompt_view(&Workspace::new("what did that produce?"));
    assert!(view.capacity_error.is_none(), "one big result must not refuse the turn: {:?}", view.capacity_error);
    let sent: String = view.messages.iter().map(|m| m.content_text()).collect();
    assert!(sent.contains("legacy-evidence"), "she reads it at a level that names its handle");
    assert!(sent.len() < huge.len(), "reduced, not sent whole into a window it is several times");
}

/// Her NEWEST result over her window (prompt/newest_result.rs): Aris on the M5, 2026-10-26.
mod the_newest_result_over_her_window {
    use super::*;
    use crate::ai::types::{ContentPart, MessageContent, ResultCustody};
    use crate::cognition::act_observe::{ActStatus, Observation, ToolOutput, ToolVerb};
    use crate::cognition::working_memory::WorkingMemory;

    /// Her slot on the M5 (2026-10-26).
    // context-budget-exempt: the MEASURED served window in Aris's incident (serving/status on the M5,
    // 2026-10-26 22:22Z), a fixture input that reproduces it, not a budget the code sizes anything by.
    const ARIS_WINDOW: u32 = 187_136;
    const HANDLE: &str = "aris-evidence";

    /// Her measured density: her refused requests measured 192k-234k tokens, nearly all of it this
    /// result (340,832 chars, estimated 113,611 at chars/3). The lowest refusal, the most
    /// conservative reading of her density (the incident's own numbers).
    fn aris_calibration() -> PromptCalibration {
        PromptCalibration::observed(113_611, 192_000, None).expect("test: positive observation") // JUSTIFIED: test fixture
    }

    /// A `work/submission` answer the shape of hers: 578 staged rows of UUIDs, pretty JSON.
    fn evidence_answer(rows: usize) -> String {
        let row = |i: usize| {
            json!({
                "revision_id": Uuid::new_v4(), "claim_id": Uuid::new_v4(), "owner": Uuid::new_v4(),
                "staged_at_ms": 1_793_000_000_000u64 + i as u64, "generation_count": 1,
                "generation_request_ids": [Uuid::new_v4().to_string()],
                "matches_submission_claim": i % 289 != 7, "predates_submission": true,
            })
        };
        let rows: Vec<_> = (0..rows).map(row).collect();
        let patch = "+ a line of the submitted patch, as long as an ordinary diff line\n".repeat(1_500);
        serde_json::to_string_pretty(&json!({ "base_sha": "7fd84d40", "staged_evidence": rows, "patch": patch }))
            .expect("test: encode") // JUSTIFIED: test fixture
    }

    /// Her act and its saved result, recorded as the act seam does, and the faculty that renders it.
    fn faculty_after(content: &str) -> LlmDeliberationFaculty {
        faculty_on(content, Arc::new(HeuristicInferenceAdapter::new()))
    }

    /// The same, generating through `adapter`.
    fn faculty_on(content: &str, adapter: Arc<dyn crate::ai::adapter::AIProviderAdapter>) -> LlmDeliberationFaculty {
        faculty_at(content, adapter, ARIS_WINDOW)
    }

    /// The same, served at `window`.
    fn faculty_at(content: &str, adapter: Arc<dyn crate::ai::adapter::AIProviderAdapter>, window: u32) -> LlmDeliberationFaculty {
        let wm = Arc::new(WorkingMemory::new(8));
        let room = Uuid::new_v4();
        let call = ToolCall { id: "call-aris".into(), name: "work/submission".into(), input: json!({ "include_staged_evidence": true }) };
        wm.record_native_action_statement("Reading the staged evidence for my submission.", room, std::slice::from_ref(&call));
        let observation = Observation {
            call,
            output: ToolOutput {
                image: None,
                additional_images: None,
                result: crate::ai::types::ToolResult { tool_use_id: "call-aris".into(), content: content.into(), is_error: None, custody: Default::default() },
                verb: ToolVerb::classify("work/submission"),
                paths: Vec::new(),
                verdict: Default::default(),
            },
            status: ActStatus::Executed,
        };
        let rendered = observation.render_recency();
        wm.record_receipt_typed(&[observation], &rendered, Some(room), ResultCustody::Whole { handle: HANDLE.into() });
        LlmDeliberationFaculty::new(Uuid::new_v4(), "Aris", "You are Aris.", adapter)
            .with_context_window(window)
            .with_working_memory(wm)
    }

    /// Every tool result's content the view sends, with the ids it answers.
    fn tool_results(view: &DeliberationPromptView) -> Vec<(String, String)> {
        view.messages
            .iter()
            .filter_map(|m| match &m.content {
                MessageContent::Parts(parts) => Some(parts),
                MessageContent::Text(_) => None,
            })
            .flatten()
            .filter_map(|part| match part {
                ContentPart::ToolResult { tool_use_id, content, .. } => Some((tool_use_id.clone(), content.clone())),
                _ => None,
            })
            .collect()
    }

    // what this catches: Aris's stall. A newest result larger than her window was required whole,
    // so every request overflowed and she never acted again. It must render under her window at
    // its summary level, with the handle that reopens it and a line saying why, never dropped.
    #[test]
    fn aris_340k_result_renders_under_her_window_with_its_handle() {
        let whole = evidence_answer(578);
        assert!(whole.chars().count() >= 340_000, "the fixture is her shape: {}", whole.chars().count());
        let faculty = faculty_after(&whole);
        let ws = Workspace::new("what does the evidence say?");
        let view = faculty.prompt_view_with_visual_feedback(&ws, ARIS_WINDOW, aris_calibration(), None);
        assert!(view.capacity_error.is_none() && view.budget_fault.is_none(), "{:?} {:?}", view.capacity_error, view.budget_fault);
        let sent = aris_calibration().charge(view.estimated_prompt_tokens) + view.completion_reserve as usize;
        assert!(sent <= ARIS_WINDOW as usize, "{sent} tokens charged against her {ARIS_WINDOW} window");
        let results = tool_results(&view);
        assert_eq!(results.len(), 1, "the native call/result pair is intact");
        let (id, shown) = &results[0];
        assert_eq!(id, "call-aris");
        assert!(shown.contains("too large to show whole") && shown.contains(&format!("tool/output {HANDLE}")), "{}", &shown[..shown.len().min(600)]);
        assert!(shown.contains("lines between"), "it is her result's summary level, not only a pointer");
        assert!(shown.chars().count() * 4 <= whole.chars().count(), "a smaller level: {} of {}", shown.chars().count(), whole.chars().count());
    }

    // what this catches: Kimi on the windows 5090 (2026-10-28, refusal 1eef970d). Her view's
    // required sources (notices, open views) are never shed, but her newest result was priced
    // against the window less framing and tools alone: it fit there, stayed whole, and the view's
    // floor then took its room, so the fit refused required 33,719 > 30,243 before sending, with
    // no reduction offered. A result that fits the window alone but not beside her required view
    // comes in at its summary with its handle, and the turn goes.
    #[test]
    fn a_result_her_required_view_leaves_no_room_for_comes_in_reduced_not_refused() {
        // context-budget-exempt: a fixture window this test is run at (a result that fits it alone,
        // and required notices that leave it too little room), not a budget the code sizes anything by.
        const WINDOW: u32 = 60_000;
        let whole = "a line of a file she read, about as long as source lines run\n".repeat(1_700);
        let alone = faculty_at(&whole, Arc::new(HeuristicInferenceAdapter::new()), WINDOW);
        let ws = Workspace::new("what does the file say?");
        let view = alone.prompt_view_with_visual_feedback(&ws, WINDOW, PromptCalibration::default(), None);
        assert!(view.capacity_error.is_none(), "alone, the result fits whole: {:?}", view.capacity_error);
        assert_eq!(tool_results(&view), vec![("call-aris".to_owned(), whole.clone())], "and is untouched");
        let beside = faculty_at(&whole, Arc::new(HeuristicInferenceAdapter::new()), WINDOW);
        let memory = beside.working_memory.as_ref().expect("test: her memory"); // JUSTIFIED: test fixture
        for n in 0..60 {
            memory.pin_fact(&format!("held-{n}"), &format!("- a condition the substrate holds her to, number {n}: {}", "detail ".repeat(200)));
        }
        let view = beside.prompt_view_with_visual_feedback(&ws, WINDOW, PromptCalibration::default(), None);
        assert!(view.capacity_error.is_none(), "beside her required view the turn is not refused: {:?}", view.capacity_error);
        let results = tool_results(&view);
        assert_eq!(results.len(), 1, "the native call/result pair is intact");
        assert!(results[0].1.contains("too large to show whole") && results[0].1.contains(&format!("tool/output {HANDLE}")),
            "it comes in reduced with its handle: {}", &results[0].1[..results[0].1.len().min(400)]);
    }

    // what this catches: the collapse firing on a result that fits, which would cost her the
    // outcome of her own act on the very next turn.
    #[test]
    fn a_newest_result_that_fits_is_untouched() {
        let small = evidence_answer(3);
        let faculty = faculty_after(&small);
        let ws = Workspace::new("what does the evidence say?");
        let view = faculty.prompt_view_with_visual_feedback(&ws, ARIS_WINDOW, aris_calibration(), None);
        assert!(view.capacity_error.is_none());
        assert_eq!(tool_results(&view), vec![("call-aris".to_owned(), small)]);
    }

    /// Her refusal on the M5 (2026-10-27 04:29Z), exactly as the engine counted it.
    fn aris_refusal(requested: u32) -> crate::ai::inference_error::InferenceError {
        crate::ai::inference_error::InferenceError::ContextExceeded { requested, available: ARIS_WINDOW }
    }

    /// An adapter that refuses each of `refusals`, then answers PASS.
    fn refusing(refusals: &[u32]) -> Arc<ScriptedAdapter> {
        let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(FinishReason::Stop, "PASS", None)]));
        adapter.rejections.lock().expect("test: rejection queue").extend(refusals.iter().map(|&r| aris_refusal(r))); // JUSTIFIED: test fixture
        adapter
    }

    /// Her one result as a request carried it.
    fn sent_result(request: &TextGenerationRequest) -> String {
        request.messages.iter()
            .filter_map(|m| match &m.content { MessageContent::Parts(parts) => Some(parts), MessageContent::Text(_) => None })
            .flatten()
            .find_map(|part| match part { ContentPart::ToolResult { content, .. } => Some(content.clone()), _ => None })
            .expect("test: the request carries her result") // JUSTIFIED: test fixture
    }

    /// A request's cost in the fitter's own unit: framing, messages, tool schemas.
    fn estimate(request: &TextGenerationRequest) -> usize {
        LlmDeliberationFaculty::framing_cost(request.system_prompt.as_deref().unwrap_or_default()) // JUSTIFIED: a request with no system prompt costs none
            + LlmDeliberationFaculty::messages_cost(&request.messages)
            + request.tools.as_deref().map_or(0, crate::cognition::tool_surface::tool_surface_tokens_of) // map_or: no tools, no schemas
    }

    // what this catches: Aris's permanent stall after a restart (the M5, 2026-10-27 03:54-04:29Z).
    // With no measured density the bytes/3 estimate let her 340k-char result go whole; the
    // engine refused 214,671 > 187,136 and she parked, never using the density it measured.
    // The refusal must refit her view at that density, resend ONCE with the result at its
    // summary and handle, under her window, and the turn proceeds.
    #[tokio::test]
    async fn a_refusal_refits_her_newest_result_and_resends_once() {
        let whole = evidence_answer(578);
        let adapter = refusing(&[214_671]);
        let faculty = faculty_on(&whole, adapter.clone());
        let outcome = faculty.contribute(&Workspace::new("what does the evidence say?")).await.expect("test: a verdict"); // JUSTIFIED: test fixture
        assert!(outcome.fault.is_none() && outcome.capacity_refusal.is_none(), "{:?} {:?}", outcome.fault, outcome.capacity_refusal.is_some());
        let seen = adapter.seen.lock().expect("test: request log"); // JUSTIFIED: test fixture
        assert_eq!(seen.len(), 2, "the refused request and exactly one resend");
        assert_eq!(sent_result(&seen[0]), whole, "precondition: with no measured density the result went whole");
        let resent = sent_result(&seen[1]);
        assert!(resent.contains("too large to show whole") && resent.contains(&format!("tool/output {HANDLE}")), "{}", &resent[..resent.len().min(600)]);
        assert!(resent.contains("lines between"), "her result's summary level, not only a pointer");
        // Priced at the density the refusal measured, the resend and its reply fit her slot.
        let priced = (estimate(&seen[1]) as u128 * 214_671).div_ceil(estimate(&seen[0]) as u128) as usize;
        let reply = seen[1].max_tokens.expect("test: the resend carries its allowance") as usize; // JUSTIFIED: test fixture
        assert!(priced + reply <= ARIS_WINDOW as usize, "{priced} + {reply} over her {ARIS_WINDOW}");
    }

    // what this catches: a loop. A resend at her measured density that is refused too must park
    // on the refusal (as before #5505) and never be resent a second time.
    #[tokio::test]
    async fn a_refit_that_is_refused_too_parks_without_a_second_resend() {
        let adapter = refusing(&[214_671, 190_000]);
        let faculty = faculty_on(&evidence_answer(578), adapter.clone());
        let outcome = faculty.contribute(&Workspace::new("what does the evidence say?")).await.expect("test: a verdict"); // JUSTIFIED: test fixture
        assert!(outcome.fault.is_none(), "{:?}", outcome.fault);
        assert!(outcome.capacity_refusal.is_some(), "she parks on the refusal");
        assert_eq!(adapter.call_count(), 2, "one resend, never a second");
    }

    // what this catches: the refit firing on a request that fits, which would resend or reshape
    // a turn the engine accepted.
    #[tokio::test]
    async fn a_request_that_fits_never_takes_the_refit() {
        let small = evidence_answer(3);
        let adapter = refusing(&[]);
        let faculty = faculty_on(&small, adapter.clone());
        let outcome = faculty.contribute(&Workspace::new("what does the evidence say?")).await.expect("test: a verdict"); // JUSTIFIED: test fixture
        assert!(outcome.fault.is_none() && outcome.capacity_refusal.is_none());
        let seen = adapter.seen.lock().expect("test: request log"); // JUSTIFIED: test fixture
        assert_eq!(seen.len(), 1);
        assert_eq!(sent_result(&seen[0]), small);
    }

    // what this catches: #5520 not firing on the M5 (6417d15be, 05:20:57Z). Her refusal measured
    // 222,348 for 148,275 estimated: 1.50 averaged over the whole request, and at 1.50 her ~113.6k
    // result still fit the room the window left it, so it stayed whole and the resend path never
    // ran. This fixture holds HER ratio and her result's share of the estimate; the refusal's
    // excess, charged to the result, must bring it in at its summary and resend once.
    #[tokio::test]
    async fn her_averaged_density_still_refits_a_denser_result() {
        // Her refusal (05:20:57Z): the engine's count over our estimate of the same request.
        const MEASURED: u128 = 222_348;
        const ESTIMATED: u128 = 148_275;
        let turns: Vec<_> = (0..60)
            .map(|i| crate::cognition::workspace::BurstTurn::attributed(i % 2 == 1, if i % 2 == 1 { "Aris" } else { "Joel" }, format!("prose_turn_{i} ").repeat(110), Some(i)))
            .collect();
        let ws = Workspace::new(crate::cognition::workspace::Burst::from_turns(crate::identity::ActivityRoom::mint(), turns));
        let whole = evidence_answer(578);
        let adapter = refusing(&[]);
        let faculty = faculty_on(&whole, adapter.clone());
        // The preconditions read a twin: rendering a view advances its epoch, and the faculty
        // that acts must send exactly the view the refusal is scaled from.
        let twin = faculty_after(&whole);
        let first = twin.prompt_view_with_visual_feedback(&ws, ARIS_WINDOW, PromptCalibration::default(), None);
        assert!(!first.newest_result_reduced, "precondition: unmeasured, her result goes whole");
        let share = first.newest_result_tokens as f64 / first.estimated_prompt_tokens as f64;
        assert!((0.70..=0.85).contains(&share), "precondition: her result is ~77% of the estimate (113.6k of 148.3k), here {share:.2}");
        let requested = u32::try_from(first.estimated_prompt_tokens as u128 * MEASURED / ESTIMATED).expect("test: a u32 count"); // JUSTIFIED: test fixture
        assert!(requested > ARIS_WINDOW, "precondition: the engine refuses it ({requested})");
        // The 6417d15be miss, reproduced: at the averaged ratio her result still fits whole.
        let averaged = PromptCalibration::observed(first.estimated_prompt_tokens, requested, Some(first.input_identity)).expect("test: positive"); // JUSTIFIED: test fixture
        assert!(!faculty_after(&whole).prompt_view_with_visual_feedback(&ws, ARIS_WINDOW, averaged, None).newest_result_reduced, "precondition: the averaged ratio alone does not reduce it");
        adapter.rejections.lock().expect("test: rejection queue").push_back(aris_refusal(requested)); // JUSTIFIED: test fixture
        let outcome = faculty.contribute(&ws).await.expect("test: a verdict"); // JUSTIFIED: test fixture
        assert!(outcome.fault.is_none() && outcome.capacity_refusal.is_none(), "{:?} parked={}", outcome.fault, outcome.capacity_refusal.is_some());
        let seen = adapter.seen.lock().expect("test: request log"); // JUSTIFIED: test fixture
        assert_eq!(seen.len(), 2, "the refused request and exactly one resend");
        assert_eq!(estimate(&seen[0]), estimate(&first_request(&first, &seen[0])), "precondition: the refusal is scaled from the view she sent");
        let resent = sent_result(&seen[1]);
        assert!(resent.contains("too large to show whole") && resent.contains(&format!("tool/output {HANDLE}")), "{}", &resent[..resent.len().min(600)]);
    }

    /// `view` as the request `sent` carried it (its messages; the rest is the request's own).
    fn first_request(view: &DeliberationPromptView, sent: &TextGenerationRequest) -> TextGenerationRequest {
        let mut request = sent.clone();
        request.messages = view.messages.clone();
        request
    }
}
