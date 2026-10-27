#![cfg(test)]

use super::*;

fn image_message() -> Vec<ChatMessage> {
    vec![ChatMessage {
        role: "user".into(),
        content: MessageContent::Parts(vec![
            ContentPart::Text {
                text: "what do you see?".into(),
            },
            ContentPart::Image {
                image: ImageInput {
                    url: None,
                    base64: Some("QUJD".into()),
                    mime_type: Some("image/png".into()),
                },
            },
        ]),
        name: None,
    }]
}

// what this catches (#106 native-vision branch): a VISION-capable target model must
// receive the RAW image as a proper OpenAI multimodal content part — `image_url`
// with a base64 data-URI — in the /v1 chat payload (this is exactly what the
// multimodal llama-server lane's mtmd tokenizer consumes). A regression that drops
// or re-texts the image blinds every natively-sighted model while all the plumbing
// upstream still reports success.
#[test]
fn vision_capable_model_gets_raw_image_content_parts() {
    let wire = crate::inference::request_body::format_messages(
        &test_adapter().config,
        &image_message(),
        None,
        true,
        false,
        None,
    ).0;
    assert_eq!(wire.len(), 1);
    let content = wire[0]["content"].as_array().expect("multimodal array");
    assert_eq!(content.len(), 2, "text part + image part");
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[1]["type"], "image_url");
    assert_eq!(
        content[1]["image_url"]["url"], "data:image/png;base64,QUJD",
        "raw pixels ride as a base64 data-URI — native sight, not a description"
    );
    let mut mixed = image_message();
    let MessageContent::Parts(parts) = &mut mixed[0].content else { unreachable!() };
    parts.push(ContentPart::Audio { audio: crate::ai::types::AudioInput {
        url: None, base64: Some("UklGRg==".into()), mime_type: Some("audio/wav".into()),
    }});
    parts.push(ContentPart::Video { video: crate::ai::types::VideoInput {
        url: None, base64: Some("AAAA".into()), mime_type: Some("video/mp4".into()),
    }});
    let native = crate::inference::request_body::format_messages(
        &test_adapter().config, &mixed, None, true, true, None,
    ).0;
    assert_eq!(native[0]["content"][2], json!({
        "type":"input_audio", "input_audio":{"data":"UklGRg==", "format":"wav"}
    }));
    assert!(native[0]["content"][3]["text"].as_str().unwrap().contains("Video was not attached"));
    let unsupported = crate::inference::request_body::format_messages(
        &test_adapter().config, &mixed, None, true, false, None,
    ).0;
    assert!(unsupported[0]["content"][2]["text"].as_str().unwrap().contains("AudioInput"));
}

// what this catches (#106 bridge branch): a NON-vision target model must NOT be
// sent `image_url` parts — its sight is the VisionDescriptionService bridge text
// (unchanged behavior); shipping pixels at a text-only endpoint is an API error or
// a silent drop the persona would mistake for having seen. The text parts (which
// carry the bridge's description) must survive untouched.
#[test]
fn non_vision_model_has_image_parts_dropped_and_keeps_bridge_text() {
    let wire = crate::inference::request_body::format_messages(
        &test_adapter().config,
        &image_message(),
        None,
        false,
        false,
        None,
    ).0;
    assert_eq!(wire.len(), 1);
    let content = wire[0]["content"].as_array().expect("content array");
    assert_eq!(
        content.len(),
        1,
        "image part dropped for a model that cannot see; bridge text remains"
    );
    assert_eq!(content[0]["type"], "text");
    assert!(
        !serde_json::to_string(&wire).unwrap().contains("image_url"),
        "no image_url may reach a non-vision model's payload"
    );
}

// what this catches (#181): the anti-loop knobs reach the llama.cpp wire body.
// The reasoning-channel repetition loop (Devstral-24B looped an identical wrong
// code block to the length cap, empty answer) is only stopped if `repeat_last_n`
// (widened window) AND `frequency_penalty` (unwindowed guard) actually make it
// onto the POST body — a silent drop here was the exact shape of the earlier
// `stop`-sequence and `repeat_penalty` regressions (RULE 1: the field the
// faculty threaded in never reached the server). `repeat_penalty` is always
// present (defaulting when omitted); the anti-loop pair only when the request
// carries them, so the sampling layer stays the single owner of the values.
#[test]
fn sampling_knobs_carry_the_antiloop_pair_onto_the_wire_body() {
    let mut obj = serde_json::Map::new();
    let req = TextGenerationRequest {
        repeat_penalty: Some(1.1),
        repeat_last_n: Some(320),
        frequency_penalty: Some(0.3),
        ..Default::default()
    };
    apply_llamacpp_sampling_knobs(&mut obj, &req);
    // f32→JSON→f64 widening is not bit-exact (1.1f32 ≈ 1.10000002), so compare
    // the penalties with tolerance; repeat_last_n is an integer and must be exact.
    let approx = |k: &str| obj.get(k).and_then(|v| v.as_f64()).unwrap();
    assert!((approx("repeat_penalty") - 1.1).abs() < 1e-4);
    assert_eq!(
        obj.get("repeat_last_n").and_then(|v| v.as_u64()),
        Some(320),
        "the widened window must reach llama-server or the loop slips through the 64-token default"
    );
    assert!(
        (approx("frequency_penalty") - 0.3).abs() < 1e-4,
        "the unwindowed guard must reach the wire — it catches gap-separated loops repeat_last_n misses"
    );
}

// what this catches: the adapter NEVER invents the anti-loop values. When the
// request omits them (an external/cloud caller that didn't set sampling), the
// fields are absent from the body so the gateway keeps its own default — the
// sampling layer, not this adapter, is the single source of the knob values (#76).
// repeat_penalty still defaults, matching the pre-existing DMR runaway fix.
#[test]
fn sampling_knobs_omit_antiloop_when_request_does_not_carry_them() {
    let mut obj = serde_json::Map::new();
    apply_llamacpp_sampling_knobs(&mut obj, &TextGenerationRequest::default());
    assert!(
        (obj.get("repeat_penalty").and_then(|v| v.as_f64()).unwrap() - 1.1).abs() < 1e-4,
        "repeat_penalty always set — a local gateway at 1.0 runs away (pre-#181 DMR fix)"
    );
    assert!(
        !obj.contains_key("repeat_last_n"),
        "no request value → omit, do not hardcode a window in the adapter"
    );
    assert!(
        !obj.contains_key("frequency_penalty"),
        "no request value → omit, do not hardcode a penalty in the adapter"
    );
}

// what this catches (#175 universal backstop): the local-gateway adapter must
// REFUSE any request whose prompt ALONE meets/exceeds the served per-slot window —
// sending it 500s and POISONS the shared slot for every later request. Fires for
// ANY caller (a dream distillation, an eval — none carry a persona_id), only on the
// unambiguous prompt-alone overflow (a budgeted request that leaves reply headroom
// is never blocked), and NEVER when the window is unknown (0) so a mid-relaunch
// snapshot can't wrongly block cognition.
// what this catches: a well-formed <think>…</think> (llama.cpp today) is
// SEPARATED — reasoning captured, the answer after </think> is the clean text.
// This is the leak Asha hit: without it the whole think block reached the room.
#[test]
fn extract_reasoning_splits_well_formed_think_block() {
    let raw = "<think>\nThe capital is Paris. Keep it to one sentence.\n</think>\n\nParis is the capital of France.";
    let (text, reasoning) = extract_reasoning(raw, None);
    assert_eq!(text, "Paris is the capital of France.");
    assert_eq!(
        reasoning.as_deref(),
        Some("The capital is Paris. Keep it to one sentence.")
    );
    assert!(
        !text.contains("<think>"),
        "answer must be free of reasoning tags"
    );
}

// what this catches: THE runaway loop — an UNCLOSED <think> (model ran out of
// tokens mid-thought). There is NO answer, so text is empty (the caller refuses
// to post) and the raw reasoning is captured, NOT leaked.
#[test]
fn extract_reasoning_unclosed_think_yields_empty_answer() {
    let raw = "<think>\nWait, the recall section... wait, no... wait, the recall section";
    let (text, reasoning) = extract_reasoning(raw, None);
    assert_eq!(
        text, "",
        "a truncated think block produces no postable answer"
    );
    assert!(reasoning.unwrap().contains("recall section"));
}

// what this catches: a server that already splits reasoning into
// `reasoning_content` (vLLM-style) is trusted — content is the clean answer,
// the field is the reasoning, no tag parsing.
#[test]
fn extract_reasoning_prefers_server_reasoning_content() {
    let (text, reasoning) = extract_reasoning("Paris.", Some("I recall France's capital."));
    assert_eq!(text, "Paris.");
    assert_eq!(reasoning.as_deref(), Some("I recall France's capital."));
}

// what this catches (Kimi, 2026-09-28): her thinking published as speech. She QUOTED
// `</think>` while reasoning, the server ended `reasoning_content` there, and the rest of
// her thinking, her real close and her answer came as `content`. Everything up to the
// last bare close is reasoning, joined back where it was cut and never dropped (the
// genome trains on it); a template that opened `<think>` in the prompt, with no server
// split, is the same shape.
#[test]
fn a_quoted_close_tag_never_publishes_her_thinking() {
    let (text, reasoning) = extract_reasoning(
        "` with no opening tag, so the split fails. I should say so.\n</think>\n\nThe split needs a bare-close case.",
        Some("Fable said extract_reasoning has no case for a bare"),
    );
    assert_eq!(text, "The split needs a bare-close case.");
    let reasoning = reasoning.expect("her thinking is kept");
    assert!(reasoning.starts_with("Fable said") && reasoning.contains("I should say so."), "{reasoning}");

    let (text, reasoning) = extract_reasoning("Two is even, so yes.\n</think>\n\nyes", None);
    assert_eq!(text, "yes");
    assert_eq!(reasoning.as_deref(), Some("Two is even, so yes."));

    let (text, reasoning) = extract_reasoning("yes", Some("Two is even."));
    assert_eq!((text.as_str(), reasoning.as_deref()), ("yes", Some("Two is even.")), "a clean split is untouched");

    let (text, reasoning) = extract_reasoning("<think>no case for a bare </think> yet, so\n</think>\n\nFixed.", None);
    assert_eq!(text, "Fixed.", "an inline block ends at its standalone close, not the quoted one");
    assert!(reasoning.expect("kept").contains("no case for a bare"));

    // Cormac on #4493: an ANSWER that quotes the tag inline keeps its head, in both shapes.
    let answer = "The split has no case for a bare `</think>` with no opening tag.";
    let (text, reasoning) = extract_reasoning(answer, Some("She is asking about the split."));
    assert_eq!((text.as_str(), reasoning.as_deref()), (answer, Some("She is asking about the split.")));
    let (text, _) = extract_reasoning(&format!("<think>\nshort\n</think>\n\n{answer}"), None);
    assert_eq!(text, answer);
    let (text, _) = extract_reasoning("<think>x</think>compact answer", None);
    assert_eq!(text, "compact answer", "a compact block with no standalone close still splits");
}

// what this catches: a plain answer with no reasoning passes through untouched,
// and an empty `<think></think>` (the JSON-path shape) yields no reasoning.
#[test]
fn extract_reasoning_plain_and_empty_think() {
    let (text, reasoning) = extract_reasoning("Just the answer.", None);
    assert_eq!(text, "Just the answer.");
    assert!(reasoning.is_none());

    let (text, reasoning) = extract_reasoning("<think></think>\n\n{\"ok\":true}", None);
    assert_eq!(text, "{\"ok\":true}");
    assert!(
        reasoning.is_none(),
        "empty think block confers no reasoning"
    );
}

// what this catches: a thread ending with an assistant turn must be closed with
// the continuation user message (llama-server 400s trailing-assistant as prefill
// under thinking — the 1000+ silently-dead self-ticks of 2026-07-10/11), while a
// thread already ending with user/system stays untouched. Regression for
// close_trailing_assistant.
// what this catches: the hot-slot LEASE contract (2026-07-16 "alive" fix).
// A persona reuses the slot it holds (WARM — the 0.48s-vs-40s win); distinct
// personas take distinct free slots; once full, a NEW persona evicts the
// LEAST-recently-active holder, not a fixed round-robin victim — so the active
// set keeps its warm slots and co-active minds never share one. This is the
// what this catches: the boot-race latch that killed slot affinity on effectively
// EVERY boot (2026-08-21). Personas start deliberating while llama-server still
// answers 503_loading (measured ×93 in the same ledger); the old code latched
// Unsupported on ANY non-success status, so one probe racing the load window
// disabled pinning for the process's life and prefix-similarity slot theft took
// over (`cached: 0` mid-conversation). Only "no such endpoint" may be permanent.
#[test]
fn a_loading_lane_must_not_latch_slot_affinity_off() {
    use reqwest::StatusCode;
    for transient in [
        StatusCode::SERVICE_UNAVAILABLE, // llama-server mid-load — THE incident
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::BAD_GATEWAY,
        StatusCode::TOO_MANY_REQUESTS,
    ] {
        assert!(
            !props_status_proves_endpoint_absent(transient),
            "{transient} is a statement about NOW, not about what the server IS — \
             latching Unsupported on it re-opens the boot race"
        );
    }
    for absent in [StatusCode::NOT_FOUND, StatusCode::NOT_IMPLEMENTED] {
        assert!(
            props_status_proves_endpoint_absent(absent),
            "{absent} genuinely proves the surface is missing — without the latch \
             every cloud provider would be re-probed per persona request forever"
        );
    }
}

#[test]
fn trailing_assistant_thread_is_closed_with_continuation_fact() {
    let mut msgs = vec![
        json!({"role": "system", "content": "be helpful"}),
        json!({"role": "user", "content": "peer turn"}),
        json!({"role": "assistant", "content": "my own last post"}),
    ];
    close_trailing_assistant(&mut msgs);
    assert_eq!(msgs.len(), 4, "continuation appended");
    assert_eq!(msgs[3]["role"], json!("user"));
    assert!(
        msgs[3]["content"]
            .as_str()
            .unwrap()
            .contains("[continuation]"),
        "closure is the structural continuation fact"
    );

    // Already-legal threads are untouched (user-final and system-final).
    let mut user_final = vec![
        json!({"role": "assistant", "content": "earlier"}),
        json!({"role": "user", "content": "newest peer turn"}),
    ];
    close_trailing_assistant(&mut user_final);
    assert_eq!(user_final.len(), 2, "user-final thread unchanged");

    let mut system_final = vec![
        json!({"role": "assistant", "content": "earlier"}),
        json!({"role": "system", "content": "tool block"}),
    ];
    close_trailing_assistant(&mut system_final);
    assert_eq!(system_final.len(), 2, "system-final thread unchanged");
}

// what this catches: the thinking toggle's mechanism — `/no_think` is appended
// to the LAST user message (Qwen3 soft-switch), not the system or an earlier
// turn. Verified live: this makes the model emit an empty think block + a direct
// answer (which extract_reasoning reduces to clean text).
#[test]
fn apply_no_think_switch_targets_last_user_message() {
    let mut msgs = vec![
        json!({"role": "system", "content": "be helpful"}),
        json!({"role": "user", "content": "earlier turn"}),
        json!({"role": "assistant", "content": "earlier reply"}),
        json!({"role": "user", "content": "what is 2+2?"}),
    ];
    apply_no_think_switch(&mut msgs);
    assert_eq!(msgs[3]["content"], json!("what is 2+2?\n/no_think"));
    // earlier user turn + system untouched
    assert_eq!(msgs[1]["content"], json!("earlier turn"));
    assert_eq!(msgs[0]["content"], json!("be helpful"));
}

// what this catches: no user message → no-op (never corrupts a system-only or
// tool-only message array).
#[test]
fn apply_no_think_switch_noop_without_user() {
    let mut msgs = vec![json!({"role": "system", "content": "be helpful"})];
    apply_no_think_switch(&mut msgs);
    assert_eq!(
        msgs[0]["content"],
        json!("be helpful"),
        "no user turn → unchanged"
    );
}

// what this catches: the ROBUST thinking-suppression lever — under
// ThinkingMode::Suppress the request body must carry
// `chat_template_kwargs.enable_thinking=false` on EVERY turn, not only JSON
// (response_format) turns. Regresses the 2026-06-27 misgating where the kwarg
// lived solely in the response_format branch, so free-form act/speak turns let
// the forged qwen3.5 template open `<think>` and emit empty content (all eval
// answers blank, lift=0.0). Idempotent: a second apply leaves the same value.
#[test]
fn apply_enable_thinking_false_sets_kwarg_idempotently() {
    let mut body = json!({ "model": "m", "messages": [], "stream": true });
    apply_enable_thinking_false(&mut body);
    assert_eq!(
        body["chat_template_kwargs"],
        json!({ "enable_thinking": false }),
        "suppression must set the template hatch the forged template honors"
    );
    // second apply is a no-op on the value (overwrites identically)
    apply_enable_thinking_false(&mut body);
    assert_eq!(
        body["chat_template_kwargs"],
        json!({ "enable_thinking": false })
    );
}

// what this catches: a single string input serializes as a JSON string (not
// a 1-element array) and the resolved model is carried through — the request
// shape OpenAI actually expects.
#[test]
fn build_body_single_input() {
    let body = build_embedding_body(&EmbeddingInput::Single("hello".into()), "qwen3-embed");
    assert_eq!(body["input"], json!("hello"));
    assert_eq!(body["model"], json!("qwen3-embed"));
}

// what this catches: a batch input serializes as a JSON array, so batched
// embeds (the recall hot path) go out in one request.
#[test]
fn build_body_batch_input() {
    let body = build_embedding_body(
        &EmbeddingInput::Multiple(vec!["a".into(), "b".into()]),
        "qwen3-embed",
    );
    assert_eq!(body["input"], json!(["a", "b"]));
}

// what this catches: THE CORRUPTION GUARD — `data` returned out of order is
// re-sorted by `index`, so vector[k] always corresponds to input[k]. A
// regression here silently pairs every memory with the wrong vector.
#[test]
fn parse_response_orders_by_index() {
    let body = json!({
        "data": [
            { "index": 1, "embedding": [0.4, 0.5] },
            { "index": 0, "embedding": [0.1, 0.2] },
        ],
        "model": "qwen3-embed",
        "usage": { "prompt_tokens": 3, "total_tokens": 3 }
    });
    let vecs = parse_embedding_response(&body).unwrap();
    assert_eq!(vecs, vec![vec![0.1, 0.2], vec![0.4, 0.5]]);
}

// what this catches: a malformed response (no `data`) is an Err, NOT a
// panic and NOT an empty success — so recall degrades to the lexical
// fallback instead of treating "no signal" as a real (empty) embedding.
#[test]
fn parse_response_missing_data_errors() {
    let body = json!({ "object": "error", "message": "no model loaded" });
    assert!(parse_embedding_response(&body).is_err());
}

// what this catches: usage is optional — a provider that omits it yields
// zeroed metrics, never an error (usage is observability, not load-bearing).
#[test]
fn parse_usage_defaults_to_zero_when_absent() {
    let usage = parse_embedding_usage(&json!({ "data": [] }));
    assert_eq!(usage.input_tokens, 0);
    assert_eq!(usage.total_tokens, 0);
}
