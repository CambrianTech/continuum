#![cfg(test)]
use super::*;

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
        estimate_guard_tokens(&guard_text) + LlmDeliberationFaculty::PER_MESSAGE_TEMPLATE_TOKENS,
    );
    assert!(required >= estimate_guard_tokens(&grep_result));
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
    crate::ai::tool_dialect::normalize_call(&mut error.call);
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
        capacity_error: None, budget_fault: None, protocol_error: None, segments: Vec::new(),
        newest_result_reduced: false, newest_result_tokens: 0, allocation: None,
        messages: roomy.messages, tools: crate::cognition::tool_surface::SelectedSurface::none(), tools_grant: 0,
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

// what this catches: progressive disclosure — the per-turn tool PAYLOAD is the
// two-tool DISCOVERY PAIR (`commands/list` + `commands/help`), not the whole
// authorized registry, and the system prompt carries only a CATEGORY INDEX, not
// every tool. The old dump injected ~150 full schemas / one-liners (~4–5k tokens)
// into EVERY turn, overflowing n_ctx → 400 "exceeds context size" → mute. Now the
// surface is a tiny category index inside the system prompt + the two-tool native
// offering, so even a huge tool set leaves system + user + the offered tools well
// within the served window. Invariant: the category index (not tool names) rides
// the system prompt, the native offering is exactly the discovery pair, and the
// whole prompt + its tools + reserve fit the window. A regression means the dump
// came back.
#[test]
fn tool_surface_is_a_category_index_plus_discovery_pair() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    // A tool set whose FULL schemas would dwarf the window — the live shape
    // (whole authorized set ≫ whole slot), scaled down. With progressive
    // disclosure it no longer matters: only names+summaries ride the prompt.
    // 8192 = tight-but-SERVABLE: the un-amputated native surface (~4.1k
    // tokens of specs) must fit beside a real prompt + reserve. 4096 was
    // the old cliff-era fixture — below what the full surface can ever
    // serve in, i.e. a window the plan would never hand a tool-using
    // persona. The budget trims VOLATILE context to fit; specs are
    // reserved up front, never amputated.
    let window: u32 = 8192;
    let tools: Vec<NativeToolSpec> = (0..60)
        .map(|i| NativeToolSpec {
            name: format!("cat/command_{i}"),
            description:
                "A registry command projected as a tool with a structured argument schema; \
                     its full parameter description rides the chat-template injection."
                    .to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: json!({
                    "path": { "type": "string", "description": "workspace-relative path" },
                    "mode": { "type": "string", "enum": ["read", "write", "append"] },
                    "limit": { "type": "integer", "description": "max rows returned" }
                }),
                required: Some(vec!["path".to_string()]),
                definitions: None,
            },
        })
        .collect();
    // Cloned before the original moves into `faculty` — the roomy control below needs
    // the IDENTICAL surface, or the comparison measures two different things.
    let roomy_tools = tools.clone();

    let faculty = LlmDeliberationFaculty::new(
        persona,
        "Asha",
        "You are Asha, a thoughtful engineer on the grid.",
        adapter,
    )
    .with_context_window(window)
    .with_tools(tools);

    // The native surface is NEVER window-amputated. The old tight-window
    // "discovery pair only" cliff was a clamp (glass-boxed #206): a
    // native-tool-call model can only emit calls for offered specs, so the
    // amputated surface stranded it in a commands/help loop with 0 edits —
    // and the threshold sat on the served window's knife-edge, flipping
    // 10/10 ↔ 0/6 on a token. The budget (`prompt_view_within`) owns the
    // fit now: it reserves these specs' tokens up front and trims VOLATILE
    // context, protecting the hands. This test USED to pin the cliff; now
    // it pins its absence.
    let native: Vec<&str> = faculty
        .tool_surface().registry()
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    // Names on the wire ride the DIALECT (tool_dialect): the conventional,
    // charset-legal aliases tool-trained models actually saw in training.
    for must in ["list_commands", "help", "edit_file", "bash", "grep"] {
        assert!(
            native.contains(&must),
            "tight window must NOT amputate the native surface — {must} missing: {native:?}"
        );
    }
    // On a ROOMY window the CORE CODING ARC rides natively too — so a
    // tool-call-trained model acts directly instead of looping on
    // `commands/help{code/search}` (glass-boxed: 14/14 SWE acts were help-loops,
    // 0 edits, before this). Bounded working set, never the ~150-tool dump.
    let roomy = LlmDeliberationFaculty::new(
        persona,
        "Asha",
        "You are Asha, a thoughtful engineer on the grid.",
        Arc::new(HeuristicInferenceAdapter::new()),
    )
    .with_context_window(32_768)
    .with_tools(vec![]);
    let _ = roomy; // (surface empty without tools — the arc requires authorization)
    let roomy_native: Vec<String> = {
        let f = LlmDeliberationFaculty::new(
            persona,
            "Asha",
            "You are Asha.",
            Arc::new(HeuristicInferenceAdapter::new()),
        )
        .with_context_window(32_768)
        .with_tools(persona_tools::native_tool_specs());
        f.tool_surface().registry().iter().map(|s| s.name.clone()).collect()
    };
    // Wire names are the DIALECT aliases (tool_dialect) — the coding arc
    // rides under the names the model was trained on, never our slashes.
    for must in ["grep", "read_file", "edit_file", "bash"] {
        assert!(
            roomy_native.iter().any(|n| n == must),
            "roomy window ⇒ {must} offered natively: {roomy_native:?}"
        );
    }

    // The bookmarked MENU rode into the system prompt. An EXPANDED category
    // lists the BARE verb of every tool it holds (`category: verb, …`) — names
    // only since the 2026-07-10 prompt diet; args live in commands/help + the
    // #1916 inline error-manual, never an 8k menu wall.
    let expanded = BTreeSet::from(["cat".to_string()]);
    let framing = faculty.compose_system(&expanded, false, false, None);
    assert!(
        framing.contains("[Your tools]") && framing.contains("cat: command_0"),
        "an expanded category must name each verb under its header: {framing}"
    );
    assert!(
    !framing.contains("cat/command_0"),
    "the full slash-path form must NOT be dumped — verbs render bare under the category header"
);
    // Collapsed (nothing expanded) the same category renders as a one-line
    // bookmark — the spine still NAMES it, so she can open it on demand.
    let collapsed = faculty.compose_system(&BTreeSet::new(), false, false, None);
    assert!(
    collapsed.contains("cat (60 — commands/list --filter cat)"),
    "a collapsed category is a one-line bookmark naming it + its verb count: {collapsed}"
);

    // Under real burst pressure the whole prompt + the one tool + reserve fit
    // the served window — the exact condition llama-server checks before 400.
    let mut ws = history_with_stimulus("LATEST: did the deploy fix land?");
    ws.broadcast.push(Contribution::context(
        FacultyId::Recall,
        &"deploy pipeline observation; ".repeat(2000),
        0.9,
        "recalled",
    ));
    let view = faculty.prompt_view(&ws);
    let reserve = view.completion_reserve as usize;
    let prompt =
        estimate_guard_tokens(&view.system) + LlmDeliberationFaculty::messages_cost(&view.messages);

    // This assertion used to demand that framing + the FULL tool surface + the
    // reply reserve all fit an 8192 window. That is arithmetically impossible and
    // always was: the surface alone is ~4.6k, the framing floor ~1.6k, and the
    // reserve is window/4 — 8300 into 8192. It only ever "passed" while the
    // deleted #206 cliff amputated the tools, which is the clamp that stranded a
    // native-call model in a help loop. Asserting an impossibility does not make
    // it true; it just hides WHICH constraint is binding.
    //
    // The honest invariants are two, and they are what this now pins:
    //   1. the budget did its job — the prompt itself fits the window minus reply
    //   2. the substrate KNOWS when the window cannot host the hands, and says so
    //      with a real number instead of shipping a citizen who silently goes mute
    assert!(
        prompt + reserve <= window as usize,
        "the trimmed prompt ({prompt}) + reply reserve ({reserve}) must fit {window} \
                 — the budget owns this, and it trims volatile context, never the hands"
    );

    // The bound is a real derivation — at the window it names, the arithmetic for
    // bare framing + tools + reply actually closes.
    let needed = faculty.min_window_for_agentic_surface();
    assert!(
        faculty.describe_tool_tokens()
            + faculty.framing_floor_tokens() as usize
            + LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS as usize // the reply's flex floor — the smallest grant production gives a reply
            <= needed as usize,
        "min_window_for_agentic_surface({needed}) must clear its own arithmetic"
    );
    // …and here is the part that matters, measured rather than assumed. This
    // assertion USED to read `needed <= window` — "8192 clears the bound (8040)
    // and she is STILL starved" — and it went red on its own terms: the message
    // said "if it no longer does, the surface or framing grew and the story
    // changed", and it did. The bound moved 8040 → 9348 (+1308) as framing
    // accreted turn-fact by turn-fact (#151, #152, #144, #303 …), each one
    // individually justified.
    //
    // So the story is now STRONGER, not broken: at 8192 an agentic citizen no
    // longer fits AT ALL — the necessary condition itself fails, before we even
    // get to "necessary but not sufficient". An 8k-window model cannot host this
    // surface, full stop. That is #333 (the surface is paid twice) stated as a
    // number instead of a complaint.
    //
    // Pinned as a CEILING rather than relaxed to pass: this is the ratchet that
    // makes the next +1308 fail loudly instead of accruing silently. Lower it
    // when the surface actually shrinks; never raise it to make a red go green
    // without saying what grew and why.
    //
    // 9400 → 9600, and what grew, stated plainly as the ratchet demands: the
    // `yield_turn` VERDICT VERB (+144 tokens, 9348 → 9544). It is the structured
    // channel for choosing silence, which `Decision::Pass` never had — the
    // absence is what forced months of prose phrase-matching, and Joel killed the
    // last of that on 2026-08-07 ("string matches for semantic understanding is
    // not good for reliability"). Its schema is argument-free; the 144 tokens are
    // almost entirely the DESCRIPTION, which is the part that teaches her the
    // silence is free and the announcement is the noise.
    //
    // Judged worth it against #333 rather than waved through: a single avoided
    // pass-cascade costs the room more than 144 tokens (the 2026-08-01 one ran
    // ~30 minutes across every peer), and this replaces matcher code that could
    // only ever be beaten by the next phrasing.
    //
    // 9600 → 9800, and what grew, stated plainly as the ratchet demands: PR #2162
    // grew the native payload 20 → 23 schemas (+212 tokens, 9544 → 9756) and said
    // so in its own commit message — the #339 lifecycle write-back verbs
    // (REACHABLE-VERIFIED: they existed and had never declared NATIVE, so a
    // native-call citizen could claim work she could never update or close) plus
    // `room/members` (pinned into the native-surface test after #358 proved a
    // correct roster read). Each verb closes a measured live defect; shrinking
    // here would undo verified fixes. What #2162 did NOT do was re-pin this
    // ceiling, so canary sat red for a day and every branch inherited it —
    // which is itself the ratchet working: the growth got named here instead
    // of accruing silently.
    // 9800 → 10350, stated plainly as the ratchet demands: three NATIVE room
    // membership verbs — `room/list`, `room/join`, `room/leave` (+492 tokens,
    // 9756 → 10292).
    //
    // Why they earn it: nothing in continuum could put a citizen in a second
    // room AT ALL. Her rooms were whatever bootstrap seeded, forever — the last
    // of four narrowings behind operator messages being structurally invisible
    // to personas (measured on two machines: every citizen subscribed to exactly
    // one room while the operator held several, so the daemon's own channel
    // filter correctly dropped everything). Perception and reply were fixed
    // first; without membership those fixes have nothing to act on. And they are
    // HERS to call, not only an operator's to apply to her, which is the whole
    // difference between a citizen and a managed resource.
    //
    // SHRUNK FIRST, then re-pinned — the ratchet's first branch before its
    // second. The initial draft cost 688 tokens; trimming verbose DESCRIPTIONs
    // and dropping alias overflow (4→2, 3→2) recovered 196 of them. What is left
    // is the irreducible cost of three discoverable verbs. Descriptions were cut
    // to the point where more cutting would recreate #358 — a citizen who cannot
    // find the verb reaches for the wrong one and reads her own looping as having
    // nothing to contribute.
    // 10350 → 11300, stated plainly as the ratchet demands, and SHRUNK FIRST
    // as its own first branch requires — this is the first re-pin where the
    // shrink was MEASURED rather than eyeballed.
    //
    // WHAT GREW: three NATIVE activity verbs — `activity/spawn`, `activity/archive`,
    // `activity/protect` (+1,273 gross). They are not new commands; they routed
    // all along and had simply never shipped a DESCRIPTOR, so `activity/spawn` —
    // the verb that mints every room, benchmark rooms included — was absent from
    // `commands/list`, the ACL, codegen and every citizen's tool surface. Same
    // defect class as #339, now impossible: `ModuleRegistry::register` panics on
    // a constructor with no descriptor.
    //
    // WHAT SHRANK: 728 tokens, from making the schema projection stop shipping
    // MAINTAINER RATIONALE to citizens. schemars lifts `///` verbatim, so one
    // doc comment served two readers with opposite needs — a citizen deciding
    // whether to spawn a room was billed for `schemars(with = "String") describes
    // the WIRE (a uuid string, per #[serde(transparent)])…`. The projection now
    // keeps the LEAD PARAGRAPH (see `persona_tools::lead_paragraph_descriptions`);
    // rationale stays in the file where it belongs. Not a length cap — truncating
    // the ANSWER is #358's shape.
    //
    // NET 11,972 → 11,244. Design + full measurement:
    // docs/cognition/TOOL-DISCLOSURE-LADDER.md. That doc also retracts the
    // "the schemas are 3-5x bloated" reading this ceiling invited: measured, the
    // 26-verb surface is 6,935 tokens, mean 266/verb — leaner per verb than the
    // frontier agent runtimes it was being unfavourably compared to. The surface
    // is not obese; it is paid TWICE (#333) and it is charged against a 16k lane.
    // The next real shrink is structural — rung 2 of the ladder becoming the
    // TURN's working set instead of a static list — after which this ceiling
    // guards the SPINE and stops taxing capability.
    // The guard is about SURFACE GROWTH, so it measures the SURFACE. It used to assert
    // on `min_window_for_agentic_surface()`, which was then `fixed * D/(D-1)` — the
    // surface AND the reserve policy multiplied together (it is now `fixed + the
    // reply floor`, still a reply term this guard does not own). Changing the reserve split therefore
    // moved a ceiling whose own message says "framing/tools grew again. Shrink the
    // surface", and the D=4→2 experiment tripped it at 16,864 with the surface
    // completely unchanged. Two concerns in one assertion; the reserve half was never
    // this guard's business. `fixed` is tools + bare framing and nothing else, so this
    // now moves if and only if the surface actually grows.
    // 8500 → 8650, stated plainly as the ratchet demands, and SHRUNK FIRST:
    // one NATIVE verb — `perception/hot-edit` (+191 tokens, 8432 → 8623). The
    // TWEAK half of the design loop (render → observe → hot-edit → re-grade,
    // DESIGN-BENCH-VISUAL-CRAFT.md): apply a CSS patch to a live page and
    // re-observe, no deployment — offered beside `perception/observe` because a
    // citizen who can see a page but not iterate on it styles blind. The first
    // draft cost 238 tokens; trimming the DESCRIPTION to its contract (full
    // accumulated CSS, replace-wholesale layer, delta meaning) recovered 47.
    // What is left is the irreducible cost of one discoverable verb.
    //
    // 8650 → 9400, stated plainly (2026-08-25): two NATIVE web verbs —
    // `web/search` + `web/fetch` (+727 tokens even after trimming both
    // DESCRIPTIONs to one line each; the rest is their irreducible param
    // schemas). Web FORAGING is a deliberate capability-parity add (Joel: "make
    // sure we have it" — the operator uses web lookup constantly on SWE/task
    // work, and a native-call model like Ornith can ONLY emit calls for tools in
    // its offered specs, so catalog-only web was unreachable to her hands). The
    // surface already exceeded the 8192 tight-test window before this (8623 >
    // 8192) — the budget trims VOLATILE context (recall/RAG) to fit and reserves
    // specs up front, so this raises the reserved floor, it does not introduce a
    // new overflow. Ornith serves at 166k where this ceiling is irrelevant; the
    // tight-window LCD persona (which does not do web research) pays with less
    // recall room, a conscious trade. If a third addition wants in, SHRINK first.
    //
    // 9400 → 10150, stated plainly (2026-08-25): three NATIVE GitHub-collaboration
    // verbs — `code/github/pr-create` + `pr-comment` + `issue-create` (+748 tokens,
    // their irreducible param schemas). This is the executor→TEAMMATE layer (Joel:
    // "friendly in how code and GitHub work are managed"): she had LOCAL git but
    // could not open/comment a PR or file an issue, and a native-call model can only
    // emit calls for OFFERED tools, so catalog-only collaboration verbs were
    // unusable by her hands. Same trade as the web add: Ornith serves at 166k where
    // this is irrelevant; the tight-window LCD persona pays with less recall room.
    // The shrink-first debt is now REAL at 10k — the next capability MUST shrink the
    // surface (a category-index/discovery split for the rarely-used verbs, #333), or
    // the LCD persona gets a reduced surface; do not keep bumping this.
    //
    // 10150 → 10400, stated plainly (2026-08-26): ONE native vision verb —
    // `vision/look` (+191 tokens, its param schema). This is a citizen's EYES
    // as an act: workspace images (screenshots, charts, benchmark PNGs) were
    // structurally invisible — cognition/vision-describe is Internal, and a
    // native-call model can only emit calls for OFFERED tools. Sight is a
    // sensory-parity capability (every persona sees, per the sensory
    // architecture), and the vision-qa benchmark grades exactly this loop.
    // Same conscious trade as the web/github adds: the tight-window LCD
    // persona pays with less recall room; Ornith at 166k doesn't notice.
    // If another addition wants in, SHRINK first (#333).
    //
    // 10400 → 10700, stated plainly (2026-09-01): TWO native activity verbs —
    // `activity/recipes` + `activity/invite` (+~565 tokens, their param
    // schemas). A citizen with an idea could already SPAWN a room
    // (activity/spawn has been native since #274) but could neither
    // discover what recipes exist nor staff the room she made — spawn →
    // invite → say-what-it's-about is ONE flow (Joel, 2026-08-31:
    // starting an activity must be "easy and common", and citizens mint
    // their own activities per [[activities-are-self-hosting]]). Same
    // conscious trade as vision/look above.
    // 10700 -> 12000, stated plainly (#4102): three native reviewed-work
    // verbs, work/submit + work/review + work/submission. Their compact,
    // documented schemas cost 1286 guard tokens: exact artifact identity,
    // submission/review claims, and evidence must be callable by native
    // models. The full catalog plus framing is 11974 (10688 without
    // these three). This is a test receipt, not a runtime budget increase;
    // per-turn selection and whole-request window accounting stay intact.
    // 12000 -> 12100, stated plainly (2026-09-17, the reviewer's twin of #4148):
    // work/review gained `evidence_text` — the one field a reviewer can actually
    // fill (what she ran and saw) in place of an evidence hash nothing in her
    // world produced — and work/submit's derivable fields turned optional.
    // Net +27 guard tokens on the full catalog (12027).
    // 2026-09-17, #4158: work/list, work/similar and work/duplicates gained `room`
    // (boards are PER ROOM; a citizen seated in a triage room saw an EMPTY board).
    // First cut cost +117 guard tokens and tripped this ceiling AND the 8192
    // survival check below by TWO tokens; the docs were made terse instead of
    // moving either number. The surface is within the ceiling as it stood.
    // 12100 -> 13200, stated plainly (#4532, Joel 2026-09-28: "give them more not
    // less"): a capable citizen's work turn now carries the hands a Claude or Codex
    // session has — code/shell-poll, code/git/add, code/git/push,
    // code/github/pr-create, code/github/pr-comment — as natives. Measured 13145
    // on the full catalog (+1045 over the 12100 surface as merged). The citizen's
    // own terminal-level hands are the point of the change, not framing growth;
    // per-turn selection and whole-request window accounting stay intact.
    // 13200 -> 13500, stated plainly (2026-09-28, Joel: making cards toward an
    // activity's goals is every participant's work): work/create joined the native
    // surface (repo, title, body, priority, and a REQUIRED room). Measured 13499 on
    // CI before its docs were trimmed. Before it a citizen could claim and move cards
    // but not create one, so a project stayed one card.
    // 13500 -> 13610 (measured on CI over work/create), stated plainly (2026-09-28, Joel:
    // citizens need to drive and screenshot their own sites, as Playwright automation
    // does): perception/interact
    // joined the native surface (session, target, viewport, actions, selector), the
    // DRIVE half of the observe loop (card 3569675f).
    // 13610 -> 13624 (measured on canary ac65b92c7), stated plainly (2026-09-28, Joel:
    // a citizen's confusion is a tool-feedback defect): #4557 rewrote work/submit's
    // artifact help to lead with "Omit: computed from your checkout. Not a git sha."
    // after Kimi stalled supplying a hash the verb computes. The words are the fix.
    // 13624 -> 15119 (CI, PR #4603): provided perception commands now expose
    // typed required inputs and nested action schemas instead of Null. This is
    // actual tool demand, including target/session recovery descriptions; keep
    // accounting for it rather than hiding schemas from the persona or budget.
    // 15119 -> 15179 (CI, PR #4615): work/submit adds the optional
    // generation_request_id selector and its exclusivity help. Kimi's
    // inspected staging UUID expired before publication; this stable
    // causal selector adds 60 measured guard tokens. Account for the
    // real schema; retain whole-request budgeting and this growth guard.
    // 15179 -> 15731 (CI, PR #4626): screenshot now exposes its existing
    // selector, encoding, dimensions and delivery parameters instead of
    // an empty schema. The real typed contract adds 552 measured guard
    // tokens; this test ceiling is not a runtime context-budget change.
    // 15731 -> 17087 (PR #4758): focus/continue, focus/nudge and focus/mute join
    // the offer, her self-determination verbs (Joel 2026-10-05: agency; her loop
    // is her own). ~450 guard tokens each, in line with the ~330 average of the
    // other hands: a continuation carries a note, a room, an expectation and a
    // verdict, each described. Real demand, accounted for, not hidden.
    // 17087 -> 17150 (PR #4782): focus/continue gains `private`, the act that
    // takes her into her mind room (PRIVACY-OF-THOUGHT.md). Its text was cut
    // to one line each (it measured 17230 first); the 63 left are the param's
    // schema itself, the feature. Measured, not guessed.
    // 17150 -> 17238 (PR #5059, 2026-10-15): code/edit now advertises
    // its required typed four-operation contract instead of an untyped,
    // optional operation that generated an invalid path-only retry. Hosted
    // run 38738554029 on 6aa0cbe21 measured 17238; deleting duplicate legacy
    // fallback fields first saved 280 tokens from 17518. Retain the remaining
    // 88 for the callable contract, with no slack or production budget change.
    // 17238 -> 17298 (PR #5112, 2026-10-16): work/list gains `limit` (default 40,
    // claimable first) so a board of 776 cards is never one 198k-char line again;
    // one optional field and its one-line doc, measured locally at 17298.
    // 17298 -> 17335 (goal 10 slice 2, card ecd5dff3, 2026-10-24): commands/help gains
    // `query`, the way a mind finds a verb by what she wants to do once her learned head no
    // longer lists every verb. Its text was cut to one line (it measured 17427 first); the
    // 37 left are the param itself, the feature. Measured locally at 17335.
    // 17335 -> 17486 (#5504, Aris's 340k-char evidence answer, 2026-10-27): work/submission gains
    // evidence_rows / evidence_offset / evidence_limit, so include_staged_evidence answers with a
    // summary and pages rows on request instead of one 340,832-char result (Aris: 578 rows, ~183k
    // tokens of a 187,136 slot, 0 acts for a day). The docs were cut to a few words each (it
    // measured 17650 first); the 151 left are the three fields themselves, the same trade as
    // #5112's work/list limit. Measured locally at 17486.
    const AGENTIC_SURFACE_CEILING: u32 = 17486;
    let surface = faculty.describe_tool_tokens() as u32 + faculty.framing_floor_tokens();
    println!("agentic surface: {surface} guard tokens; ceiling {AGENTIC_SURFACE_CEILING}");
    assert!(
        surface <= AGENTIC_SURFACE_CEILING,
        "the agentic surface is now {surface} tokens (ceiling \
                 {AGENTIC_SURFACE_CEILING}) — framing/tools grew. Shrink the surface (#333) \
                 or state plainly what was added and re-pin the ceiling"
    );
    assert!(
        needed > window,
        "8192 no longer hosts the agentic surface ({needed} needed) — if this \
                 flips back the surface genuinely shrank, which is GOOD news: restore the \
                 original `needed <= window` narrative and drop the ceiling"
    );
    // #327, and the assertion here is now the SURVIVAL check rather than its
    // inverse. It used to read `!contains(…)` — "at 8192 the newest burst line
    // does NOT survive; the framing is intact and the hands are intact, the
    // CONVERSATION is what got zero" — with a note to restore the survival check
    // if it ever flipped. It flipped. Card dec1a7ff flipped it.
    //
    // Note WHAT changed, because the old note guessed wrong about the cause: the
    // surface did NOT shrink and the framing did NOT shrink. The ceiling
    // assertion above still passes and `needed > window` still holds, both
    // measuring the same surface as before. What changed is that the budget
    // stopped RESERVING for a surface it does not send. This turn narrows to her
    // hands, and the message budget used to subtract the whole registry anyway —
    // so the conversation was trimmed to fit room that was never occupied, and
    // the newest line fell off the end of a window that had space for it.
    //
    // That is the difference between "this window is too small for a citizen"
    // and "we were spending her window on tools we withheld". Only the second
    // was ever true here.
    assert!(
        view.user_text()
            .contains("LATEST: did the deploy fix land?"),
        "the newest burst line must survive once the budget prices the surface it \
                 actually sends — if this regresses, the accounting is double-counting the \
                 withheld registry again (card dec1a7ff); selected={}, estimated={}, reserve={}, capacity={:?}\n{}",
        faculty.prompt_view(&ws).tools.tokens,
        view.estimated_prompt_tokens,
        view.completion_reserve,
        view.capacity_error,
        &view.user_text()[..view.user_text().len().min(600)]
    );
    assert!(
        view.system.contains("Taking your turn"),
        "framing survives regardless"
    );

    // …and at a window that CAN host the surface, the conversation is heard. Same
    // faculty, same burst, same tools — only the window differs, which is what makes
    // this a capacity fact rather than a cognition bug.
    let roomy = LlmDeliberationFaculty::new(
        persona,
        "Asha",
        "You are Asha.",
        Arc::new(HeuristicInferenceAdapter::new()),
    )
    .with_tools(roomy_tools)
    .with_context_window(16_384);
    let roomy_view = roomy.prompt_view(&ws);
    assert!(
        roomy_view
            .user_text()
            .contains("LATEST: did the deploy fix land?"),
        "given a window that fits the surface, the newest message reaches her"
    );
}
