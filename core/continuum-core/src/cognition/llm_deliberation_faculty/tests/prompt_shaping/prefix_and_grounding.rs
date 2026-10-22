#![cfg(test)]

use super::*;

// what this catches: #266 KV-cache reuse at the CALLER. The per-turn presence
// framing (DIRECTED vs SILENCE) flips hard every turn; the raw prompt-captures
// caught it sitting in the system message at char ~7607, BEFORE the grounding
// context, so every flip shortened the reusable KV prefix to ~7.6k chars and
// re-prefilled the whole tail (0% KV reuse — the ~13min SWE solves). The fix moves
// the framing to render LAST in the system message, after the context, so the
// reusable prefix now extends THROUGH the grounding and the flip falls past it.
// This asserts the property at the live boundary: the two systems (directed vs
// undirected) share a common prefix that reaches into the grounding context, and
// each carries its own presence variant only in the diverging tail. Before the
// reorder this was RED — the systems diverged at the framing, char ~7607, ahead of
// the context. regression for #266
#[test]
fn directedness_flip_keeps_the_grounding_inside_the_reusable_prefix() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter)
        // Ample window so budget pressure never perturbs what renders — this
        // isolates the framing POSITION, not truncation.
        .with_context_window(8192);

    // Same room content + the same standing grounding (a session-stable roster
    // contribution renders the [What you are working with] block); only the
    // directedness dimension flips.
    let with_grounding = |directed: bool| {
        let mut ws = Workspace::new("the team is chatting about the plan");
        ws.broadcast.push(
            Contribution::context(
                FacultyId::Custom("room-roster".to_string()),
                "ROSTER: alice, bob present",
                0.5,
                "framing",
            )
            .session_stable(),
        );
        ws.attention = TurnAttention::for_message(directed, false);
        ws
    };
    let undirected = faculty.prompt_view(&with_grounding(false));
    let directed = faculty.prompt_view(&with_grounding(true));

    // 2026-09-01, the full fix: the system message is now the STABLE half
    // only, so the directedness flip cannot touch it AT ALL — the two
    // system messages are byte-identical, and the entire system prefix
    // (identity + tools + grounding) is reusable KV across the flip.
    assert_eq!(
        directed.system, undirected.system,
        "the system message is the stable half — a presence flip must not move a byte of it"
    );
    assert!(
        directed.system.contains("[What you are working with"),
        "grounding context stays in the (fully reusable) system prefix"
    );
    // The presence variants ride the conversation tail (facts phase,
    // before the ask) — directed carries its marker, undirected does not.
    let tail_has = |v: &DeliberationPromptView, needle: &str| {
        v.messages.iter().any(
            |m| matches!(&m.content, crate::ai::types::MessageContent::Text(t) if t.contains(needle)),
        )
    };
    assert!(
        tail_has(&directed, "This message names you")
            && !tail_has(&undirected, "This message names you"),
        "the directed presence variant renders as a conversation turn, never in the system prefix"
    );
}

// what this catches: run-18057-f1 — the just-executed act's FULL result must reach
// the next prompt EVEN WHEN NOTHING BID IT through the arbiter. The 0-byte SWE-bench
// patch happened because the result rode the working-memory faculty's single
// 0.5-salience contribution, which `arbiter.focus()` truncated whole under capacity
// pressure — she then generated blind to her own grep output. The fix reads the
// result DIRECTLY from working memory in the message builder and pins it as a
// trailing turn (#392). This test simulates the exact failure: an EMPTY broadcast
// (every faculty bid evicted) with a live result in working memory — and asserts the
// result still lands in the tail. A regression means the result went back through
// the evictable path. regression for #392 / run-18057-f1
#[tokio::test]
async fn pinned_act_result_reaches_the_prompt_even_with_an_empty_broadcast() {
    use crate::ai::types::MessageContent;
    use crate::ai::types::{ContentPart, ToolCall, ToolResult};
    use crate::cognition::act_observe::{ActStatus, Observation, ToolOutput, ToolVerb};
    use crate::cognition::working_memory::WorkingMemory;

    let home = tempfile::tempdir().unwrap();
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    crate::model_registry::init_global().unwrap();

    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());

    // A live working memory carrying the result of a just-run grep — the sympy-18057
    // shape: `code/search` returned the `_sympify` location she must edit next. Large
    // enough to clear the trail-head threshold so the pinned block surfaces.
    let wm = Arc::new(WorkingMemory::new(8));
    wm.set_served_window(16_384);
    let needle = "sympy/core/expr.py:123:        return self == sympify(other)  # _sympify HERE";
    let grep_result = format!(
        "code/search matches:\n{needle}\n{}",
        "context line\n".repeat(200)
    );
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(2, 2)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let image = crate::media::artifact::ImageArtifact::retain(
        &airc_blobs::FsStore::new(home.path().join("images")).unwrap(),
        png.get_ref(),
    )
    .unwrap();
    let mut other_png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(3, 2)
        .write_to(&mut other_png, image::ImageFormat::Png).unwrap();
    let other_image = crate::media::artifact::ImageArtifact::retain(
        &airc_blobs::FsStore::new(home.path().join("images")).unwrap(),
        other_png.get_ref(),
    ).unwrap();
    let ws = Workspace::new("fix the __eq__ comparison in sympy Expr");
    wm.record_native_action_statement("", ws.activity_room_id, &[ToolCall { id: "capture".into(), name: "perception/observe".into(), input: json!({}) }]);
    wm.record_receipt_typed(
        &[Observation {
            call: ToolCall {
                id: "capture".into(),
                name: "perception/observe".into(),
                input: json!({}),
            },
            output: ToolOutput {
                image: Some(image.clone()),
                additional_images: Some(vec![other_image.clone()]),
                result: ToolResult {
                    tool_use_id: "capture".into(),
                    content: grep_result.clone(),
                    is_error: None,
                    custody: crate::ai::types::ResultCustody::Unsaved,
                },
                verb: ToolVerb::classify("perception/observe"),
                paths: Vec::new(),
                verdict: Default::default(),
            },
            status: ActStatus::Executed,
        }],
        &grep_result,
        Some(ws.activity_room_id),
        crate::ai::types::ResultCustody::Unsaved,
    );

    let faculty = LlmDeliberationFaculty::new(persona, "Atlas", "You are Atlas.", adapter)
        .with_context_window(8192)
        .with_working_memory(wm.clone());

    // The deliberate failure condition: NOTHING in the broadcast. No working-memory
    // faculty bid survived attention — the exact state that dropped the grep result
    // to a 0-byte patch. The ask is still present.
    assert!(
        ws.broadcast.is_empty(),
        "the harness must reproduce the empty-broadcast eviction"
    );

    let view = faculty.prompt_view(&ws);
    assert!(view.capacity_error.is_none(), "{view:?}");
    assert!(view
        .user_text()
        .contains("fix the __eq__ comparison in sympy Expr"));
    let in_tail = view
        .messages
        .iter()
        .any(|m| matches!(&m.content, MessageContent::Parts(parts) if parts.iter().any(|p| matches!(p, ContentPart::ToolResult {content, ..} if content.contains(needle)))));
    assert!(
        in_tail,
        "the just-fetched result must reach the prompt independent of any faculty bid \
                         — this is the run-18057-f1 fix:\n{:#?}",
        view.messages
    );
    // And it must be a trailing USER turn (nearest generation), never the system prefix.
    assert!(
        !view.system.contains(needle),
        "the pinned result is trailing proprioception, not the cacheable system prefix"
    );

    // Regression: saved screenshot handles formerly reached the prompt,
    // but the model never received the bytes. Exercise the real CAS and
    // provider-specific metadata, without making an inference call.
    let binding = ModelBinding {
        adapter: Arc::new(
            crate::ai::openai_adapter::OpenAICompatibleAdapter::from_registry("llama-server"),
        ),
        model: Some("ggml-org/Qwen3.8-27B-GGUF".into()),
        context_window: 8192,
    };
    let visual = faculty
        .active_visual_feedback_at(&binding, home.path().join("images"))
        .await
        .unwrap()
        .unwrap();
    let ContentPart::Image { image: pixels } = &visual.parts[0].1 else {
        panic!("native model must get pixels")
    };
    use base64::Engine;
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(pixels.base64.as_ref().unwrap())
            .unwrap(),
        png.into_inner()
    );
    let seen = faculty.prompt_view_with_visual_feedback(
        &ws,
        8192,
        PromptCalibration::default(),
        Some(&visual),
    );
    assert!(seen.capacity_error.is_none());
    assert_ne!(seen.input_identity, view.input_identity);
    assert!(seen.messages.iter().any(|m| matches!(&m.content, MessageContent::Parts(parts) if parts.iter().any(|p| matches!(p, ContentPart::Image { .. })))));
    assert!(seen.protocol_error.is_none());
    // Follow the same message through request construction AND the
    // final provider serializer. A Parts value alone is not wire proof.
    let request = faculty.build_request_within(
        &binding, seen.completion_reserve, seen.messages.clone(), None,
        seen.system.clone(), None, Some(ws.room_id), faculty.turn_kind(&ws),
    );
    let wire = crate::inference::request_body::wire_messages(
        &request.messages, request.system_prompt.as_deref(), true, false, "llama-server",
    );
    let call_at = wire.iter().position(|m| m["tool_calls"][0]["id"] == "capture").unwrap();
    assert_eq!(wire[call_at]["role"], "assistant");
    assert_eq!(wire[call_at + 1]["role"], "tool");
    assert_eq!(wire[call_at + 1]["tool_call_id"], "capture");
    assert!(wire[call_at + 1]["content"].as_str().unwrap().contains(needle));
    let prompt = faculty.messages_unfitted(&ws, None);
    let required = prompt.required_tokens();
    // Preserve the conservative guard, including UTF-8 bytes: structured
    // traversal must not substitute RAG's chars/4 pricing policy.
    let guard_text = format!("{grep_result}日本語訳");
    assert_eq!(
        LlmDeliberationFaculty::message_cost(&ChatMessage::text("user", &guard_text)),
        est_tokens(&guard_text) + LlmDeliberationFaculty::PER_MESSAGE_TEMPLATE_TOKENS,
    );
    assert!(required >= est_tokens(&grep_result));
    assert!(faculty.fit_messages(prompt, required - 1).is_err(), "native pair cannot be split under pressure");
    let fitted = faculty.fit_messages(faculty.messages_unfitted(&ws, None), required).unwrap();
    assert!(fitted.messages.iter().any(|m| matches!(&m.content, MessageContent::Parts(parts) if parts.iter().any(|p| matches!(p, ContentPart::ToolUse {id, ..} if id == "capture")))));
    let urls: Vec<_> = wire.iter()
        .filter_map(|m| m["content"].as_array()).flatten()
        .filter_map(|p| p["image_url"]["url"].as_str()).collect();
    assert_eq!(urls.len(), 2, "all live-call participants must reach native model input");
    let (header, encoded) = urls[0].split_once(',').unwrap();
    assert_eq!(header, "data:image/png;base64");
    use sha2::Digest;
    let wire_bytes = base64::engine::general_purpose::STANDARD.decode(encoded).unwrap();
    assert_eq!(format!("{:x}", sha2::Sha256::digest(&wire_bytes)), image.hash);
    assert_eq!(wire_bytes.len() as u64, image.size_bytes);
    let (_, encoded) = urls[1].split_once(',').unwrap();
    let other_wire = base64::engine::general_purpose::STANDARD.decode(encoded).unwrap();
    assert_eq!(format!("{:x}", sha2::Sha256::digest(&other_wire)), other_image.hash);
    // Pre-upgrade checkpoints have typed pixels but no original native
    // call provenance. Keep their visual evidence without guessing calls.
    let mut legacy_snapshot = wm.snapshot();
    for entry in &mut legacy_snapshot.entries { entry.native_calls.clear(); }
    let legacy_memory = Arc::new(WorkingMemory::new(8));
    legacy_memory.restore(legacy_snapshot);
    let legacy_faculty = LlmDeliberationFaculty::new(persona, "Atlas", "You are Atlas.", Arc::new(HeuristicInferenceAdapter::new()))
        .with_context_window(8192).with_working_memory(legacy_memory);
    let legacy_view = legacy_faculty.prompt_view_with_visual_feedback(&ws, 8192, PromptCalibration::default(), Some(&visual));
    assert!(legacy_view.protocol_error.is_none());
    let legacy_wire = crate::inference::request_body::wire_messages(&legacy_view.messages, None, true, false, "llama-server");
    assert!(legacy_wire.iter().all(|m| m.get("tool_calls").is_none()));
    assert_eq!(legacy_wire.iter().filter_map(|m| m["content"].as_array()).flatten().filter(|p| p.get("image_url").is_some()).count(), 2);
    let missing = faculty.active_visual_feedback_at(&binding, home.path().join("absent-images"))
        .await.unwrap().unwrap();
    assert!(matches!(&missing.parts[0].1, ContentPart::Text { text } if text.contains("No pixels attached")));
    let failed = faculty.prompt_view_with_visual_feedback(&ws, 8192, PromptCalibration::default(), Some(&missing));
    assert_ne!(failed.input_identity, seen.input_identity);
    assert!(failed.user_text().contains("capture or inspect the image again"));
    // Preserve a real batch including error and historical skipped receipts.
    // Result content is producer-owned, including bounded spill pointers.
    let seq = wm.active_action_full().unwrap().0;
    let mut batch = wm.active_action_observations(seq);
    let mut error = batch[0].clone();
    error.call.id = "failed-call".into();
    error.call.name = "bash".into();
    error.call.input = json!({"command": "cat missing.rs"});
    let original_error_call = error.call.clone();
    crate::cognition::tool_dialect::normalize_call(&mut error.call);
    assert_eq!(error.call.name, "code/shell");
    error.output.image = None;
    error.output.additional_images = None;
    error.output.result.tool_use_id = error.call.id.clone();
    error.output.result.content = "file does not exist".into();
    error.output.result.is_error = Some(true);
    error.status = ActStatus::Errored { message: "file does not exist".into() };
    let mut skipped = error.clone();
    skipped.call.id = "skipped-call".into();
    skipped.output.result.tool_use_id = skipped.call.id.clone();
    skipped.output.result.content = "not executed; prior output available via tool/output saved-result".into();
    skipped.output.result.custody = crate::ai::types::ResultCustody::Pointer { handle: "saved-result".into() };
    skipped.status = ActStatus::AlreadySatisfied { repeat: 2 };
    batch.extend([error, skipped]);
    let mut native_calls: Vec<_> = batch.iter().map(|o| o.call.clone()).collect();
    native_calls[1] = original_error_call;
    wm.set_scope(Some("native-workspace".into()));
    wm.record_native_action_statement("", ws.activity_room_id, &native_calls);
    wm.record_receipt_typed(&batch, "batch receipt", Some(ws.activity_room_id), crate::ai::types::ResultCustody::Unsaved);
    let batch_prompt = faculty.messages_unfitted(&ws, None);
    assert!(batch_prompt.active_protocol_error().is_none());
    let required = batch_prompt.required_tokens();
    let fitted = faculty.fit_messages(batch_prompt, required).unwrap();
    let wire_batch = crate::inference::request_body::wire_messages(&fitted.messages, None, true, false, "llama-server");
    let at = wire_batch.iter().position(|m| m["tool_calls"][0]["id"] == "capture").unwrap();
    for (i, observation) in batch.iter().enumerate() {
        assert_eq!(wire_batch[at]["tool_calls"][i]["id"], native_calls[i].id);
        assert_eq!(wire_batch[at]["tool_calls"][i]["function"]["name"], native_calls[i].name);
        let arguments: serde_json::Value = serde_json::from_str(wire_batch[at]["tool_calls"][i]["function"]["arguments"].as_str().unwrap()).unwrap();
        assert_eq!(arguments, native_calls[i].input);
        assert_eq!(wire_batch[at + 1 + i]["tool_call_id"], observation.call.id);
        assert_eq!(wire_batch[at + 1 + i]["content"], observation.output.result.content);
    }
    let mut invalid = faculty.messages_unfitted(&ws, None);
    if let MessageContent::Parts(parts) = &mut invalid.latest_result[1].content {
        if let ContentPart::ToolResult { tool_use_id, .. } = &mut parts[0] {
            *tool_use_id = "unmatched".into();
        }
    }
    assert!(invalid.active_protocol_error().is_some());
    // Reproduce the mid-concern inventory loss: the result still has live
    // custody, but its body no longer fits beside the next native batch.
    let inventory = format!("inventory-13010\n{}", "source/location.rs:42 matched reference\n".repeat(400));
    // The claim here is FIT-TIME substitution: the ring must still hold the inventory
    // when the prompt is fitted. The ring's share is enforced at every receipt now
    // (a burst to pointers once the share is exceeded), so the window is sized to
    // hold this 16.8k-char inventory beside the next batch; a smaller window would
    // collapse it at the batch's receipt, which is the other mechanism, not this one.
    wm.set_served_window(65_536);
    let pin = crate::cognition::tool_executor::spill::LivePin::new();
    let custody = pin.retain(persona, &inventory);
    let handle = custody.handle().expect("saved inventory").to_owned();
    wm.record_receipt_typed(&[], &inventory, Some(ws.activity_room_id), custody);
    wm.record_native_action_statement("", ws.activity_room_id, &native_calls);
    wm.record_receipt_typed(&batch, "next native batch", Some(ws.activity_room_id), crate::ai::types::ResultCustody::Unsaved);
    // Her view carries the inventory whole while the window holds it; nothing collapses at
    // fit time any more (her view collapses at an epoch start, `prompt/epoch.rs`).
    let roomy = faculty.messages_unfitted(&ws, None);
    let full_cost = roomy.required_tokens() + LlmDeliberationFaculty::messages_cost(&roomy.history);
    let roomy = faculty.fit_messages(roomy, full_cost).unwrap();
    assert!(roomy.messages.iter().any(|m| m.content_text().contains(inventory.trim())));
    // Exact-refusal correction reuses saved alternatives and keeps the
    // entire required native batch, without re-reading working memory.
    let estimate = LlmDeliberationFaculty::messages_cost(&roomy.messages);
    let snapshot = DeliberationPromptView {
        refit: roomy.refit, system: String::new(), input_identity: [0; 32],
        estimated_prompt_tokens: estimate, completion_reserve: 0,
        capacity_error: None, protocol_error: None, segments: Vec::new(),
        messages: roomy.messages,
    };
    let corrected = faculty.refit_rejected_prompt(snapshot, estimate as u32 + 1, estimate as u32, 0).unwrap();
    assert!(corrected.estimated_prompt_tokens < estimate);
    assert!(corrected.messages.iter().any(|m| m.content_text().contains(&handle)));
    let corrected_wire = crate::inference::request_body::wire_messages(&corrected.messages, None, true, false, "llama-server");
    let call_at = corrected_wire.iter().position(|m| m["tool_calls"][0]["id"] == "capture").unwrap();
    for (i, observation) in batch.iter().enumerate() {
        assert_eq!(corrected_wire[call_at + 1 + i]["tool_call_id"], observation.call.id);
    }
    wm.set_scope(Some("another-workspace".into()));
    assert!(wm.active_action_full().is_none(), "native result cannot demote to foreign-root prose");
    let scoped = faculty.prompt_view_with_visual_feedback(
        &ws,
        8192,
        PromptCalibration::default(),
        Some(&visual),
    );
    assert!(scoped
        .messages
        .iter()
        .all(|m| !matches!(&m.content, MessageContent::Parts(_))));
    wm.set_scope(Some("native-workspace".into()));
    let returned = faculty.prompt_view_with_visual_feedback(
        &ws, 8192, PromptCalibration::default(), Some(&visual),
    );
    let returned_wire = crate::inference::request_body::wire_messages(&returned.messages, None, true, false, "llama-server");
    let at = returned_wire.iter().position(|m| m["tool_calls"][0]["id"] == "capture").unwrap();
    for (index, call) in native_calls.iter().enumerate() {
        assert_eq!(returned_wire[at]["tool_calls"][index]["id"], call.id);
        assert_eq!(returned_wire[at + 1 + index]["tool_call_id"], call.id);
    }
    wm.set_scope(None);
    wm.record_settlement("observed", None);
    assert!(faculty
        .active_visual_feedback(&binding)
        .await
        .unwrap()
        .is_none());
}
