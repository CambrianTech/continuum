#![cfg(test)]

use super::*;

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
// what this catches: a stream that reports no usage (zero tokens) read as an empty
// generation. The class comes from what the response carries: an answer is landed and
// reasoning alone ended in her think, whatever the token counts (Codex on #5616).
#[tokio::test]
async fn a_response_without_usage_is_counted_by_what_it_carries() {
    let mut answer = make_response(FinishReason::Stop, "The parser is fine.", None);
    answer.usage.output_tokens = 0;
    let mut think = make_response(FinishReason::Stop, "", None);
    think.reasoning = Some("The parser looks fine, but I have not read the tests yet".to_string());
    think.usage.output_tokens = 0;
    // Codex's case on #5616: cut at the limit inside an answer, usage missing. The policy and the
    // counter read one classification, so this is a cut in the answer, never a cut think.
    let mut cut = make_response(FinishReason::Length, "The parser is fine, but the", None);
    cut.usage.output_tokens = 0;
    for (resp, landed) in [(answer, 1), (think, 0), (cut, 2)] {
        let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "Aris", "You are Aris.", Arc::new(ScriptedAdapter::new(vec![resp])))
            .with_context_window(32_768);
        let before = crate::cognition::health_ledger::read_stops();
        faculty.contribute(&Workspace::new("is the parser right?")).await.expect("verdict");
        let after = crate::cognition::health_ledger::read_stops();
        if landed == 2 {
            assert!(after.cut_answer > before.cut_answer, "a cut answer with no usage is a cut answer: {before:?} -> {after:?}");
        } else if landed == 1 {
            assert!(after.landed > before.landed, "an answer with no usage is landed: {before:?} -> {after:?}");
        } else {
            assert!(after.think_ended > before.think_ended, "reasoning with no usage ended in her think: {before:?} -> {after:?}");
        }
    }
}

// what this catches: a turn cancelled while its read receipt is suspended losing the stop of a
// generation that already completed. The stop is counted before any await after the
// generation (Codex on #5616): here the receipt never returns, the turn is dropped inside it,
// and the cut think is still on the hour's ledger.
#[tokio::test]
async fn a_turn_cancelled_at_its_read_receipt_still_counts_its_stop() {
    struct NeverReturns(Arc<tokio::sync::Notify>);
    #[async_trait::async_trait]
    impl crate::cognition::llm_deliberation_faculty::ReadReceipt for NeverReturns {
        async fn read_through(&self, _room: Uuid, _event: Uuid) {
            self.0.notify_one();
            std::future::pending::<()>().await;
        }
    }
    let entered = Arc::new(tokio::sync::Notify::new());
    let mut resp = make_response(FinishReason::Length, "", None);
    resp.reasoning = Some("I should read the parser first, then compare it with".to_string());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "Aris", "You are Aris.", Arc::new(ScriptedAdapter::new(vec![resp])))
        .with_tools(vec![read_tool()])
        .with_context_window(32_768)
        .with_read_receipt(Arc::new(NeverReturns(entered.clone())));
    let mut ws = Workspace::new("fix the parser");
    let source = crate::cognition::workspace::TurnSource { room: Uuid::new_v4(), event: Uuid::new_v4(), lamport: 1 };
    ws.turns = vec![crate::cognition::workspace::BurstTurn::attributed(false, "Joel", "fix the parser", None).from_event(source)];

    let before = crate::cognition::health_ledger::read_stops();
    tokio::select! {
        _ = faculty.contribute(&ws) => panic!("the receipt never returns, so the turn cannot finish"),
        _ = entered.notified() => {}
    }
    let after = crate::cognition::health_ledger::read_stops();
    assert!(after.cut_think > before.cut_think, "the stop was counted before the receipt: {before:?} -> {after:?}");
}

// what this catches: a generation cut at the output limit inside her reasoning, counted
// as a cut think on the hour line, once, even though a fault path handles it and even with
// no working set. Regression: Aris's three 963-token cuts inside thinking read "think-only 0"
// (the M5, 2026-10-30), because the count sat in one branch the cut never reached.
#[tokio::test]
async fn a_length_cut_inside_reasoning_is_counted_as_a_cut_think() {
    let persona = Uuid::new_v4();
    let mut resp = make_response(FinishReason::Length, "", None);
    resp.reasoning = Some("I should read the parser first, then compare it with".to_string());
    let adapter = Arc::new(ScriptedAdapter::new(vec![resp]));
    let faculty = LlmDeliberationFaculty::new(persona, "Aris", "You are Aris.", adapter)
        .with_tools(vec![read_tool()])
        .with_context_window(32_768);

    let before = crate::cognition::health_ledger::read_stops();
    faculty.contribute(&Workspace::new("fix the parser")).await.expect("verdict");
    let after = crate::cognition::health_ledger::read_stops();
    // The ledger is process-wide and other tests run beside this one, so the claim is a
    // rise, not an absolute count.
    assert!(after.cut_think > before.cut_think, "the cut think is on the hour's ledger: {before:?} -> {after:?}");
}

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
    use crate::cognition::conversation::persona_identity::PersonaIdentity;
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
// what this catches: her action stream not reaching the prompt she is sent. With working
// memory wired, the view carries the explicit nothing-has-executed void before any act, and
// a settled act as its own item afterwards; a fact is never rendered as an act. (The stream's
// honest states themselves: `context_streams::tests`.)
#[test]
fn her_action_stream_reaches_the_prompt() {
    use crate::cognition::working_memory::WorkingMemory;

    let adapter = Arc::new(ScriptedAdapter::new(vec![]));
    let wm = Arc::new(WorkingMemory::new(8));
    let faculty =
        LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter)
            .with_working_memory(Arc::clone(&wm));
    let ws = Workspace::new("hello room");
    let texts = |view: &DeliberationPromptView| -> Vec<String> {
        view.messages.iter().map(|m| m.content_text()).collect()
    };

    let before = texts(&faculty.prompt_view(&ws));
    assert!(before.iter().any(|t| t.contains("nothing has executed yet")), "{before:?}");

    wm.record_receipt("I ran code/list({\"path\":\".\"}) Result: 4 files");
    wm.record_fact("[unfulfilled] I said I would run commands, but no tool ran");
    wm.record_settlement("listed", None);
    let after = texts(&faculty.prompt_view(&ws));
    assert!(after.iter().any(|t| t.starts_with("[result #1") && t.contains("I ran code/list")), "{after:?}");
    assert!(!after.iter().any(|t| t.starts_with("[result") && t.contains("[unfulfilled]")), "facts are never acts");
    // Her view is a timeline: within an epoch the void said before her first act stays where
    // it was said, superseded by what came after it; a turnover renders only what is true now.
    // Either way it never stands after her act unanswered.
    let act = after.iter().position(|t| t.starts_with("[result #1"));
    if let Some(void) = after.iter().position(|t| t.contains("nothing has executed yet")) {
        assert!(Some(void) < act, "{after:?}");
        assert!(after.iter().any(|t| t == "(that was before my first act; my acts follow)"), "{after:?}");
    }
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
