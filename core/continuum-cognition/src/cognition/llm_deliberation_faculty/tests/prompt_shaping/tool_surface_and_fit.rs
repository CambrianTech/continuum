#![cfg(test)]

use super::*;

// what this catches: THE echo-loop fix. A mixed thread (peer → self → peer)
// must reach the model as ROLE-ATTRIBUTED messages — the persona's own earlier
// posts as `assistant`, peers' as `user` — not flattened into one `user` blob.
// The old single-`user`-message assembly fed the persona its own words back as
// if a peer had said them, so it re-explained / re-proposed and looped ("Would
// you like me to start?"). Role separation is what lets the model see "I already
// said that" and move on. Regressing to a flat blob brings the loop back.
#[test]
fn mixed_thread_attributes_self_to_assistant_and_peers_to_user() {
    use crate::cognition::workspace::Burst;
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(
        persona,
        "Asha",
        "You are Asha, a thoughtful engineer on the grid.",
        adapter,
    )
    .with_context_window(8192);

    let room = crate::identity::ActivityRoom::mint();
    let turns = vec![
        BurstTurn::attributed(false, "Operator", "can you summarize the thread?", Some(1)),
        BurstTurn::attributed(true, "Asha", "I propose using bart-large-cnn.", Some(2)),
        BurstTurn::attributed(false, "Operator", "go ahead.", Some(3)),
    ];
    let ws = Workspace::new(Burst::from_turns(room, turns));
    let view = faculty.prompt_view(&ws);

    // Three turns alternate roles → three messages, user/assistant/user.
    let roles: Vec<&str> = view.messages.iter().map(|m| m.role.as_str()).collect();
    // Trailing extra user turn = the #152 [context] bounds fact
    // (always present). The steps-ledger is gated on a wired
    // WorkingMemory (this harness faculty has none), so no ledger
    // message here — the ledger-specific behavior is pinned in
    // steps_ledger_renders_receipts_and_explicit_zero_case below.
    // …plus the presence contract, which now stands in her view as its own
    // block (cached once per epoch, not re-sent each act), and the short live
    // line naming it, still ask-last (the 2026-10-25 framing split).
    assert_eq!(
        roles,
        vec!["user", "assistant", "user", "user", "user", "user"],
        "view: {view:?}"
    );
    let text_of = |m: &crate::ai::types::ChatMessage| match &m.content {
        crate::ai::types::MessageContent::Text(t) => t.clone(),
        _ => String::new(),
    };
    assert!(
        view.messages.iter().any(|m| text_of(m).starts_with("[Conversational Presence]\n")),
        "the presence contract stands in her view as its block"
    );
    assert!(
        view.messages.iter().any(|m| text_of(m).contains("[In force this turn: Conversational Presence")),
        "the live line names the contract in force"
    );
    assert!(
        view.messages
            .iter()
            .any(|m| matches!(&m.content, crate::ai::types::MessageContent::Text(t) if t.contains("[context] 3 input turns were available before prompt fitting"))),
        "the [context] bounds fact states the visible window"
    );

    // The persona's own line is the `assistant` turn and carries NO name prefix
    // (her own voice; the system prompt forbids self-prefixing). Peers' lines are
    // `user` turns prefixed with the author so several speakers stay distinct.
    let assistant = &view.messages[1];
    assert_eq!(assistant.role, "assistant");
    assert_eq!(
        assistant.content_text(),
        "[occurred 1970-01-01T00:00:00.002Z] I propose using bart-large-cnn."
    );
    assert!(
        !assistant.content_text().contains("Asha:"),
        "the persona's own turn must not be self-prefixed: {assistant:?}"
    );
    assert!(view.messages[0]
        .content_text()
        .starts_with("[occurred 1970-01-01T00:00:00.001Z] Operator: "));
    // Perception facts are GROUNDING inserted BEFORE the final ask (the last user
    // turn), so the ask stays LAST where the model answers it — not the bracketed
    // meta, which it would otherwise parrot (2026-07-20 humaneval parrot fix). Here
    // the layout is [Joel, Asha(assistant), [Conversational Presence] (the standing
    // contract, in her view), [context]-fact, the live line, Joel-ask]: the grounding
    // sits at [3], after her view and before the ask, and the ask is last.
    assert!(
        view.messages[3].content_text().starts_with("[context]"),
        "grounding fact sits after her view and before the ask: {:?}",
        view.messages[3]
    );
    assert!(
        view.messages
            .last()
            .unwrap()
            .content_text()
            .starts_with("[occurred 1970-01-01T00:00:00.003Z] Operator: "),
        "the ask (last peer turn) stays LAST, after the grounding facts"
    );
}

#[test]
// what this catches (card d33e928a, Kimi 2026-09-26): the front-drop emptied a
// turn's whole optional history with no receipt — room messages, operator
// direction included, gone before compose and nothing said. The fit now accounts
// for what it kept and dropped, and says when it emptied the history.
fn fit_receipt_names_what_it_dropped_and_when_it_emptied_the_history() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "T", "You are T.", adapter);
    let history: Vec<ChatMessage> = (0..12)
        .map(|i| ChatMessage::text("user", format!("room {i} {}", "word ".repeat(95))))
        .collect();
    let stimulus = ChatMessage::text("user", "operator: do the thing ".repeat(40));
    let prompt = |history: Vec<ChatMessage>| PromptMessages {
        streams: Default::default(),
        result_pointers: Vec::new(),
        input_identity: [0; 32],
        grounding_tokens: 0,
        grounding_at: 0,
        history,
        stimulus: Some(stimulus.clone()),
        latest_result: Vec::new(), action_statement: None,
    };
    let required = prompt(Vec::new()).required_tokens();
    let total: usize = history.iter().map(LlmDeliberationFaculty::message_cost).sum();

    // Room for everything: nothing dropped, the receipt says so.
    let roomy = faculty.fit_messages(prompt(history.clone()), required + total).expect("fits");
    assert_eq!(roomy.receipt.dropped_messages, 0);
    assert_eq!(roomy.receipt.kept_messages, 12);
    assert!(!roomy.receipt.emptied());

    // A budget that holds the stimulus and a little history: some dropped, some kept.
    let tight = faculty.fit_messages(prompt(history.clone()), required + total / 2).expect("fits");
    assert!(tight.receipt.dropped_messages > 0 && tight.receipt.kept_messages > 0, "{:?}", tight.receipt);
    assert_eq!(tight.receipt.dropped_messages + tight.receipt.kept_messages, 12);
    assert!(tight.receipt.dropped_tokens >= (total / 2).saturating_sub(tight.receipt.quantum), "{:?}", tight.receipt);

    // A budget that holds only the stimulus: the whole history goes, and the
    // receipt says emptied (the turn Kimi could not see).
    let starved = faculty.fit_messages(prompt(history.clone()), required).expect("the stimulus fits");
    assert!(starved.receipt.emptied(), "{:?}", starved.receipt);
    assert_eq!(starved.receipt.dropped_tokens, total);
    assert_eq!(starved.receipt.history_budget, 0);
    assert!(starved.messages.iter().any(|m| m.content_text() == stimulus.content_text()),
        "the stimulus itself is never optional");
}

// what this catches: the fit's cut point IS the byte-stability of the
// whole conversation prefix (2026-09-01, wire-diffed: consecutive
// prompts diverging at ~1% depth because the exact-fit window start
// advanced every act). The front must stay byte-identical across small
// growth (a quantum of headroom), then advance in ONE whole-message
// jump — and the fitted suffix must always fit the budget.
#[test]
fn fit_front_advances_in_quanta_not_per_act() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "T", "You are T.", adapter);
    let msgs: Vec<ChatMessage> = (0..40)
        .map(|i| ChatMessage::text("user", format!("m{i} {}", "word ".repeat(95))))
        .collect();
    let budget = 2000;
    // Simulate 20 consecutive acts, each appending one ~160-token
    // message. Old behavior: the exact-fit start advanced EVERY act
    // (20 moves). Quantized: total growth ≈ 3.2k tokens ÷ the 512
    // quantum ⇒ at most ~7 boundary crossings, so most acts keep a
    // byte-identical front. (Counted, not position-pinned, so the
    // assertion cannot land on a quantum edge as token estimates
    // drift.)
    let mut moves = 0usize;
    let mut prev_first: Option<String> = None;
    let mut last_fit: Vec<ChatMessage> = Vec::new();
    for n in 20..=40 {
        let fit = faculty
            .fit_messages(
                PromptMessages {
                    streams: Default::default(),
                    result_pointers: Vec::new(),
                    input_identity: [0; 32],
                    grounding_tokens: 0,
                    grounding_at: 0,
                    history: msgs[..n].to_vec(),
                    stimulus: None,
                    latest_result: Vec::new(), action_statement: None,
                },
                budget,
            )
            .expect("whole history suffix fits")
            .messages;
        assert!(!fit.is_empty());
        let first = fit.first().map(|m| m.content_text());
        if prev_first.is_some() && first != prev_first {
            moves += 1;
        }
        prev_first = first;
        last_fit = fit;
    }
    assert!(
        (1..=8).contains(&moves),
        "front moved {moves} times across 20 acts — it must advance in \
                 QUANTA (a handful of jumps), never per act, and must still \
                 advance eventually (bounded memory)"
    );
    // Whole messages only at the front: the first fitted message is one
    // of the originals, never a mid-message shear.
    let first = last_fit.first().unwrap().content_text();
    assert!(
        msgs.iter().any(|m| m.content_text() == first),
        "front message must be a whole original message"
    );
    // CLUSTER GRANULARITY (Joel 9/1): the window must open on a clean
    // opener — when the quantized cut lands on an assistant answer or a
    // tool-result continuation, the front advances past the fragment.
    let mut clustered: Vec<ChatMessage> = Vec::new();
    for i in 0..30 {
        clustered.push(ChatMessage::text(
            "user",
            format!("q{i} {}", "word ".repeat(60)),
        ));
        clustered.push(ChatMessage::text(
            "assistant",
            format!("a{i} {}", "word ".repeat(60)),
        ));
    }
    let fit = faculty
        .fit_messages(
            PromptMessages {
                streams: Default::default(),
                result_pointers: Vec::new(),
                input_identity: [0; 32],
                grounding_tokens: 0,
                grounding_at: 0,
                history: clustered,
                stimulus: None,
                latest_result: Vec::new(), action_statement: None,
            },
            budget,
        )
        .expect("whole history suffix fits")
        .messages;
    assert_eq!(
        fit.first().unwrap().role,
        "user",
        "the window must never open on an answer without its question"
    );
    // The fitted suffix honors the budget.
    assert!(
        LlmDeliberationFaculty::messages_cost(&last_fit) <= budget,
        "fitted cost {} exceeds budget {budget}",
        LlmDeliberationFaculty::messages_cost(&last_fit)
    );
    // A quantum larger than the optional history must not evict every
    // message on a quiet self-tick with no external stimulus or result.
    let ambient = || PromptMessages {
        streams: Default::default(),
        result_pointers: Vec::new(),
        input_identity: [0; 32],
        grounding_tokens: 0,
        grounding_at: 0,
        history: vec![
            ChatMessage::text("user", "old situation ".repeat(30)),
            ChatMessage::text("user", "current situation ".repeat(20)),
        ],
        stimulus: None,
        latest_result: Vec::new(), action_statement: None,
    };
    let ambient_cost =
        LlmDeliberationFaculty::message_cost(ambient().history.last().unwrap());
    let fitted = faculty
        .fit_messages(ambient(), ambient_cost)
        .unwrap()
        .messages;
    assert_eq!(fitted.len(), 1);
    assert_eq!(fitted[0].content_text(), "current situation ".repeat(20));
    assert!(faculty.fit_messages(ambient(), ambient_cost - 1).is_err());
}

// what this catches: anything placed after the ask, where bracketed meta gets answered and
// parroted in its place (the 2026-07-20 humaneval fix), or behind the pinned result, which
// stays last.
#[test]
fn the_pinned_result_rides_last_and_nothing_rides_after_the_ask() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "T", "You are T.", adapter);
    let prompt = |result: bool| PromptMessages {
        streams: Default::default(),
        result_pointers: Vec::new(),
        input_identity: [0; 32], grounding_tokens: 0, grounding_at: 0,
        history: vec![ChatMessage::text("user", "[work turn] fix the bug")],
        stimulus: Some(ChatMessage::text("user", "the ask")),
        latest_result: result.then(|| ChatMessage::text("user", "Full result of your most recent action")).into_iter().collect(), action_statement: None,
    };
    let order = |result: bool| -> Vec<String> {
        faculty.fit_messages(prompt(result), 100_000).expect("test: fits").messages.iter()
            .map(|m| m.content_text().lines().next().unwrap_or("").to_string()).collect()
    };
    assert_eq!(order(true), vec![
        "[work turn] fix the bug", "the ask",
        "Full result of your most recent action",
    ]);
    assert_eq!(order(false), vec![
        "[work turn] fix the bug", "the ask",
    ]);
}

// Regression for #4290: a follow-up must not erase protection for the act it asks about.
#[test]
fn continuity_followup_keeps_completed_action_evidence() {
    let history = vec![
        ChatMessage::text("user", "Write the requested file."),
        ChatMessage::text("assistant", "calling code/write: completed.py"),
        ChatMessage::text("user", "Full result of code/write: file written successfully."),
        ChatMessage::text("user", "Now review the file you just wrote."),
    ];
    let expected = LlmDeliberationFaculty::messages_cost(&history);
    let prompt = PromptMessages {
        streams: Default::default(),
        result_pointers: Vec::new(),
        input_identity: [0; 32], grounding_tokens: 0, grounding_at: 0,
        history, stimulus: None, latest_result: Vec::new(), action_statement: None,
    };
    assert!(prompt.recent_move_evidence().tokens >= expected,
        "a new follow-up cannot make the preceding action and result disposable");
}

// Regression for #4290: insufficient capacity must refuse, not silently forget a completed act.
#[test]
fn continuity_insufficient_capacity_refuses_instead_of_dropping_evidence() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "T", "You are T.", adapter);
    let prompt = PromptMessages {
        streams: Default::default(),
        result_pointers: Vec::new(),
        input_identity: [0; 32], grounding_tokens: 0, grounding_at: 0,
        history: vec![
            ChatMessage::text("user", "Write the requested file. ".repeat(30)),
            ChatMessage::text("assistant", "calling code/write: completed.py ".repeat(30)),
            ChatMessage::text("user", "Full result of code/write: file written successfully."),
        ],
        stimulus: None, latest_result: Vec::new(), action_statement: None,
    };
    let insufficient = prompt.recent_move_evidence().tokens - 1;
    assert!(faculty.fit_messages(prompt, insufficient).is_err(),
        "required action/result continuity must not become optional when it exceeds capacity");
}

// what this catches: a faculty bound with `model: None` (the normal boot binding)
// reading the prefill-rate ladder under no name and taking the unmeasured floor
// while the served model's rate sat measured in the ladder (the M5, 2026-10-18).
#[test]
fn a_binding_without_a_model_name_carries_the_adapters_served_model() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let served = adapter.default_model().to_owned();
    assert!(!served.is_empty(), "the fixture adapter names a model");
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "T", "You are T.", adapter);
    assert_eq!(faculty.binding.load().model.as_deref(), Some(served.as_str()),
        "a bound faculty always names the model it is bound to");
}

// what this catches: her hands re-sized every act (a surface that moves mid-epoch rewrites the
// whole prompt after the system text), or sized outside the one allocation. Her first view
// buys her whole hands from an ample grant; the next act's claim keeps that level as its
// target with her core hands as the only floor, and the request carries exactly the
// selection the allocation bought.
/// Asha with tools assigned. Her surface is built from the live native registry
/// (`rebuild_tool_surface`), not these specs, whose role is only to give her a tool set; her
/// core hands are smaller than her whole hands there.
fn asha_with_hands() -> LlmDeliberationFaculty {
    let raw: Vec<NativeToolSpec> = ["code/read", "work/state", "chat/send", "commands/list", "room/join", "code/git/status", "work/submit", "web/fetch"]
        .iter()
        .map(|n| NativeToolSpec {
            name: (*n).to_string(),
            description: format!("{n}: does what its name says"),
            input_schema: crate::ai::types::ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: None,
                definitions: None,
            },
        })
        .collect();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    LlmDeliberationFaculty::new(Uuid::new_v4(), "Asha", "You are Asha.", adapter).with_tools(raw)
}

/// The schemas a surface puts on the wire, as bytes.
fn wire(tools: &crate::cognition::tool_surface::SelectedSurface<'_>) -> String {
    serde_json::to_string(&tools.specs).expect("schemas")
}

/// A workspace whose activity declares ONE verb of her hands (not the discovery pair or a focus
/// verb): its offer is that verb plus `commands/` and `focus/`, a strict subset of her hands, so it
/// fits any grant that held her hands and selects a different set. Taken from the built surface,
/// which is the live native registry, so the fixture tracks the registry's names.
fn declaring_one_hand(faculty: &LlmDeliberationFaculty) -> Workspace {
    let surface = faculty.tool_surface();
    let verb = surface
        .names()
        .iter()
        .zip(surface.registry())
        .find(|(name, spec)| {
            !name.starts_with("commands/") && !name.starts_with("focus/") && surface.hands().iter().any(|hand| hand.name == spec.name)
        })
        .map(|(name, _)| name.clone())
        .expect("her hands hold a verb besides the discovery pair and focus");
    Workspace::new("what changed on the board?").room_affordances(vec![verb])
}

fn view_epoch(faculty: &LlmDeliberationFaculty) -> Option<u64> {
    faculty.context_view.lock().as_ref().map(|view| view.epoch_id())
}

#[test]
fn an_epoch_keeps_the_tool_level_its_allocation_bought() {
    let faculty = asha_with_hands();
    let ws = Workspace::new("what changed on the board?");
    let first = faculty.prompt_view_within(&ws, 200_000);
    assert!(first.capacity_error.is_none(), "{:?}", first.capacity_error);
    let claim = faculty.tools_claim(&ws);
    assert!(claim.floor < claim.want, "core hands are smaller than her whole hands: {claim:?}");
    assert_eq!(first.tools.tokens, claim.want, "an ample grant buys her whole surface");
    assert_eq!(claim.level, first.tools.tokens, "the epoch keeps the level it chose as its target");
    assert!(first.tools_grant >= first.tools.tokens, "the selection stays within its grant");
    let second = faculty.prompt_view_within(&ws, 200_000);
    assert_eq!(
        serde_json::to_string(&second.tools.specs).expect("schemas"),
        serde_json::to_string(&first.tools.specs).expect("schemas"),
        "the same epoch sends byte-identical schemas"
    );
}

// what this catches: the set re-selected every act from that act's affordances while only the
// level was retained (Aiko on the M5, 2026-10-29: 19 of 89 requests changed the head, all in the
// tool list, as `git_diff` and `get_task` flipped in and out). Acts of one epoch whose room
// affordances differ send byte-identical tool lists.
#[test]
fn an_epoch_sends_one_tool_set_whatever_its_acts_affordances() {
    let faculty = asha_with_hands();
    let ws = Workspace::new("what changed on the board?");
    let declared = declaring_one_hand(&faculty);
    let first = faculty.prompt_view_within(&ws, 200_000);
    assert!(first.capacity_error.is_none(), "{:?}", first.capacity_error);
    let epoch = view_epoch(&faculty);
    let alone = faculty
        .tool_surface()
        .select_granted(&declared.room_affordances, first.tools_grant)
        .expect("precondition: the declared verb's offer fits the epoch's grant");
    assert_ne!(wire(&alone), wire(&first.tools), "precondition: these affordances alone choose another set");
    let second = faculty.prompt_view_within(&declared, 200_000);
    let third = faculty.prompt_view_within(&ws, 200_000);
    assert_eq!(view_epoch(&faculty), epoch, "precondition: no act turned the view over");
    assert_eq!(wire(&second.tools), wire(&first.tools), "an affordance that changes mid-epoch waits for the next start");
    assert_eq!(wire(&third.tools), wire(&first.tools), "the epoch's set holds across every act");
}

// what this catches: a retained set that never re-selects (a room's declared verbs reaching her
// only after a restart), or a turnover act that sends the old set while the new epoch retains
// another (the very next act would flip the head). The act that turns the view over sends what
// its own affordances choose, and the new epoch keeps that set against the acts after it.
#[test]
fn a_new_epoch_selects_from_the_affordances_of_the_act_that_started_it() {
    use continuum_context::source::Source;
    let faculty = asha_with_hands();
    let ws = Workspace::new("what changed on the board?");
    let declared = declaring_one_hand(&faculty);
    let first = faculty.prompt_view_within(&ws, 200_000);
    let grant = first.tools_grant;
    let alone = faculty
        .tool_surface()
        .select_granted(&declared.room_affordances, grant)
        .expect("precondition: the declared verb's offer fits the epoch's grant");
    assert_ne!(wire(&alone), wire(&first.tools), "precondition: these affordances alone choose another set");
    let (choice, refused) = faculty.granted_tools(&declared, faculty.tools_claim(&declared), grant);
    assert!(refused.is_none());
    assert_eq!(wire(&choice.sent), wire(&first.tools), "before the fit, the act charges the epoch's set");
    // The fit turns her view over in this act (what `fit_view` does when it outgrows its grant).
    let none: [&dyn Source<crate::cognition::context_streams::ContextMessage>; 0] = [];
    faculty.context_view.lock().as_mut().expect("a view").turn_over(Vec::new(), &none, |_| Vec::new()).expect("no sources, no grants");
    let epoch = view_epoch(&faculty);
    let sent = faculty.retain_tools(choice, false);
    assert_eq!(wire(&sent), wire(&alone), "the turnover act sends its own selection");
    let next = faculty.prompt_view_within(&ws, 200_000);
    assert_eq!(view_epoch(&faculty), epoch, "precondition: the next act stays in the new epoch");
    assert_eq!(wire(&next.tools), wire(&alone), "the new epoch keeps the set its start chose");
}

// what this catches: a retained set sent past a window that can no longer hold it (an over-budget
// prompt the server refuses), or the loss taken quietly. A grant below the epoch's level drops
// the set on the probed path (`delib.tools.epoch_level_lost`), sends what the grant buys, and the
// epoch then keeps that smaller set.
#[test]
fn a_window_that_cannot_hold_the_epochs_set_reselects_loudly() {
    use crate::cognition::tool_surface::SurfaceReason;
    let faculty = asha_with_hands();
    let ws = Workspace::new("what changed on the board?");
    let first = faculty.prompt_view_within(&ws, 200_000);
    let claim = faculty.tools_claim(&ws);
    assert_eq!(claim.level, first.tools.tokens, "precondition: the epoch holds her whole hands");
    assert!(claim.floor < claim.level, "precondition: core hands are smaller");
    let (choice, refused) = faculty.granted_tools(&ws, claim, claim.floor);
    assert!(refused.is_none(), "core hands fit their own floor");
    assert_eq!((choice.sent.reason, choice.sent.tokens), (SurfaceReason::CoreHandsForWindow, claim.floor));
    let sent = faculty.retain_tools(choice, false);
    assert_ne!(wire(&sent), wire(&first.tools), "the lost set is not sent");
    assert_eq!(faculty.tools_claim(&ws).level, claim.floor, "the epoch keeps the set the window could hold");
}
