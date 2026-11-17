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
    // …plus the presence contract, which stands in her view as its own block
    // (cached once per epoch, not re-sent each act; a block leads a first start),
    // and the short live line naming it, still ask-last (the 2026-10-25 framing split).
    assert_eq!(
        roles,
        vec!["user", "user", "assistant", "user", "user", "user"],
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
    let assistant = &view.messages[2];
    assert_eq!(assistant.role, "assistant");
    assert_eq!(
        assistant.content_text(),
        "[occurred 1970-01-01T00:00:00.002Z] I propose using bart-large-cnn."
    );
    assert!(
        !assistant.content_text().contains("Asha:"),
        "the persona's own turn must not be self-prefixed: {assistant:?}"
    );
    assert!(view.messages[1]
        .content_text()
        .starts_with("[occurred 1970-01-01T00:00:00.001Z] Operator: "));
    // Perception facts are GROUNDING inserted BEFORE the final ask (the last user
    // turn), so the ask stays LAST where the model answers it — not the bracketed
    // meta, which it would otherwise parrot (2026-07-20 humaneval parrot fix). Here
    // the layout is [[Conversational Presence] (the standing contract, a block of her
    // view, leading a first start), Joel, Asha(assistant), [context]-fact, the live line,
    // Joel-ask]: the grounding sits at [3], after her view and before the ask, and the ask
    // is last.
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
        grounding_at: 0,
        history,
        stimulus: Some(stimulus.clone()),
        latest_result: Vec::new(), action_statement: None, pricing: PromptCalibration::default(),
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
    assert!(tight.receipt.dropped_tokens >= total - total / 2, "exactly what the budget needs, in whole messages: {:?}", tight.receipt);

    // A budget that holds only the stimulus: the whole history goes, and the
    // receipt says emptied (the turn Kimi could not see).
    let starved = faculty.fit_messages(prompt(history.clone()), required).expect("the stimulus fits");
    assert!(starved.receipt.emptied(), "{:?}", starved.receipt);
    assert_eq!(starved.receipt.dropped_tokens, total);
    assert_eq!(starved.receipt.history_budget, 0);
    assert!(starved.messages.iter().any(|m| m.content_text() == stimulus.content_text()),
        "the stimulus itself is never optional");
}

// what this catches: a fit that cuts mid-message, keeps more than its budget, opens on an answer
// without its question, or drops more than the budget needs. Her view's epoch owns the prefix's
// byte-stability (the quantized front drop that once did is gone: what this fits is this act's
// fresh tail or its refused prompt, never the next act's prefix), so the fit drops whole messages,
// oldest first, exactly as many as it must.
#[test]
fn fit_drops_whole_oldest_messages_and_opens_on_a_cluster_opener() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "T", "You are T.", adapter);
    let msgs: Vec<ChatMessage> = (0..40)
        .map(|i| ChatMessage::text("user", format!("m{i} {}", "word ".repeat(95))))
        .collect();
    let budget = 2000;
    // Twenty acts, each appending one message: every fit holds its budget and drops no message
    // the budget could have held.
    let mut last_fit: Vec<ChatMessage> = Vec::new();
    for n in 20..=40 {
        let fit = faculty
            .fit_messages(
                PromptMessages {
                    streams: Default::default(),
                    result_pointers: Vec::new(),
                    input_identity: [0; 32],
                    grounding_at: 0,
                    history: msgs[..n].to_vec(),
                    stimulus: None,
                    latest_result: Vec::new(), action_statement: None, pricing: PromptCalibration::default(),
                },
                budget,
            )
            .expect("whole history suffix fits")
            .messages;
        assert!(!fit.is_empty());
        let kept = PromptCalibration::default().estimate_all(&fit);
        assert!(kept <= budget, "act {n}: kept {kept} over its budget {budget}");
        let dropped = n - fit.len();
        if dropped > 0 {
            let one_more = LlmDeliberationFaculty::message_cost(&msgs[dropped - 1]);
            assert!(kept + one_more > budget, "act {n}: the oldest dropped message would have fit");
        }
        last_fit = fit;
    }
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
                grounding_at: 0,
                history: clustered,
                stimulus: None,
                latest_result: Vec::new(), action_statement: None, pricing: PromptCalibration::default(),
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
        PromptCalibration::default().estimate_all(&last_fit) <= budget,
        "fitted cost {} exceeds budget {budget}",
        PromptCalibration::default().estimate_all(&last_fit)
    );
    // A quantum larger than the optional history must not evict every
    // message on a quiet self-tick with no external stimulus or result.
    let ambient = || PromptMessages {
        streams: Default::default(),
        result_pointers: Vec::new(),
        input_identity: [0; 32],
        grounding_at: 0,
        history: vec![
            ChatMessage::text("user", "old situation ".repeat(30)),
            ChatMessage::text("user", "current situation ".repeat(20)),
        ],
        stimulus: None,
        latest_result: Vec::new(), action_statement: None, pricing: PromptCalibration::default(),
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
        input_identity: [0; 32], grounding_at: 0,
        history: vec![ChatMessage::text("user", "[work turn] fix the bug")],
        stimulus: Some(ChatMessage::text("user", "the ask")),
        latest_result: result.then(|| ChatMessage::text("user", "Full result of your most recent action")).into_iter().collect(), action_statement: None, pricing: PromptCalibration::default(),
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
    let expected = PromptCalibration::default().estimate_all(&history);
    let prompt = PromptMessages {
        streams: Default::default(),
        result_pointers: Vec::new(),
        input_identity: [0; 32], grounding_at: 0,
        history, stimulus: None, latest_result: Vec::new(), action_statement: None, pricing: PromptCalibration::default(),
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
        input_identity: [0; 32], grounding_at: 0,
        history: vec![
            ChatMessage::text("user", "Write the requested file. ".repeat(30)),
            ChatMessage::text("assistant", "calling code/write: completed.py ".repeat(30)),
            ChatMessage::text("user", "Full result of code/write: file written successfully."),
        ],
        stimulus: None, latest_result: Vec::new(), action_statement: None, pricing: PromptCalibration::default(),
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
// whole prompt after the system text), or sized outside the one allocation, or an ample grant
// buying more than her core hands (the rest are one commands/help away; Joel, 2026-11-04: "Core
// hands in prompt, rest on demand"). The next act's claim keeps the level her first view chose as
// its target, and the request carries exactly the selection the allocation bought.
/// Asha with tools assigned. Her surface is built from the live native registry
/// (`rebuild_tool_surface`), not these specs, whose role is only to give her a tool set.
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
fn wire(tools: &crate::cognition::tool_surface::SelectedSurface) -> String {
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

/// Her hands as the act's one allocation chooses them within `window`, beside her framing alone:
/// the path `prompt_view` takes, without the rest of a prompt.
fn tools_within(faculty: &LlmDeliberationFaculty, ws: &Workspace, window: u32) -> (crate::cognition::llm_deliberation_faculty::prompt::ToolsChoice, continuum_context::allocation::ActPlan) {
    use continuum_context::allocation::{allocate, id, ActClaims, Head, Span};
    use continuum_context::ladder::Leveled;
    let charge = |tokens: usize| tokens as u32;
    let offer = faculty.tools_offer(ws, &charge);
    let held_menu = faculty.tools_epoch.lock().as_ref().map(|held| held.menu().clone());
    let framing = faculty.framing_offer(ws, false, offer.names(), held_menu.as_ref(), &charge);
    let reply = continuum_context::flex::FlexClaim { source_id: id::REPLY.to_owned(), priority: 8, floor_tokens: 0, min_tokens: 0, max_tokens: 0, required: true };
    let act = allocate(
        window,
        ActClaims {
            framing: Head { held: framing.held.clone(), ..Head::free(framing.leveled.clone()) },
            required: Leveled::default(),
            newest: continuum_context::allocation::Newest::Absent,
            tools: offer.head(),
            grounding: Leveled::default(),
            last_move: Leveled::default(),
            history: Span::default(),
            reply,
            view: Vec::new(),
            price: continuum_context::price::ContextPrice::Unmeasured,
            trained: continuum_context::allocation::TrainedContext::Unreported,
        },
    );
    (faculty.granted_tools(ws, offer, &framing, &act, &charge), act)
}

#[test]
fn an_epoch_keeps_the_tool_level_its_allocation_bought() {
    let faculty = asha_with_hands();
    let ws = Workspace::new("what changed on the board?");
    let first = faculty.prompt_view_within(&ws, 200_000);
    assert!(first.capacity_error.is_none(), "{:?}", first.capacity_error);
    assert_eq!(first.tools.tokens, first.tools.demand_tokens(), "an ample window shows every verb of her hands whole");
    assert!(first.tools.tokens <= faculty.tool_surface().hands_tokens(), "her on-demand verbs stay out");
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
    let alone = faculty.tool_surface().select(&declared.room_affordances, &[]);
    assert_ne!(wire(&alone), wire(&first.tools), "precondition: these affordances alone choose another set");
    let second = faculty.prompt_view_within(&declared, 200_000);
    let third = faculty.prompt_view_within(&ws, 200_000);
    assert_eq!(view_epoch(&faculty), epoch, "precondition: no act turned the view over");
    assert_eq!(wire(&second.tools), wire(&first.tools), "an affordance that changes mid-epoch waits for the next start");
    assert_eq!(wire(&third.tools), wire(&first.tools), "the epoch's set holds across every act");
}

// what this catches: a view turnover that re-selects her tools (the M5, 2026-10-31 on 526bbe418:
// Aris's set flipped between 38 and 23 verbs at turnovers in one room, and all 13 cold prefills in
// two hours followed a head change). A turnover re-renders only what follows the head, so her set
// outlives it: the turnover act and the acts after it send the set she held, whatever their
// affordances choose.
#[test]
fn a_view_turnover_keeps_her_tool_set() {
    use continuum_context::source::Source;
    let faculty = asha_with_hands();
    let ws = Workspace::new("what changed on the board?");
    let declared = declaring_one_hand(&faculty);
    let first = faculty.prompt_view_within(&ws, 200_000);
    let alone = faculty.tool_surface().select(&declared.room_affordances, &[]);
    assert_ne!(wire(&alone), wire(&first.tools), "precondition: these affordances alone choose another set");
    let (choice, _) = tools_within(&faculty, &declared, 200_000);
    // The fit turns her view over in this act (what `fit_view` does when it outgrows its grant).
    let none: [&dyn Source<crate::cognition::context_streams::ContextMessage>; 0] = [];
    faculty.context_view.lock().as_mut().expect("a view").turn_over(&continuum_context::cost::Chars, Vec::new(), &none, |_| Vec::new()).expect("no sources, no grants");
    let (sent, _) = faculty.retain_tools(choice, false, true);
    assert_eq!(wire(&sent), wire(&first.tools), "the turnover act sends the set she held");
    let next = faculty.prompt_view_within(&declared, 200_000);
    assert_eq!(wire(&next.tools), wire(&first.tools), "and so does every act after it");
}

// what this catches: a held set sent past a window that can no longer hold it (an over-budget
// prompt the server refuses), the turn refused instead (Aiko on the M5, 2026-11-07: her core hands
// whole, 11,377 tokens, refused her turns on a 30,566 window), a verb dropped to make room (she is
// never handless), the loss taken quietly, or a collapse that never rises again (every downward
// move needs its upward one). A window too small for her held levels re-chooses them smaller
// (`delib.tools.epoch_level_lost`), every verb still sent and named; the head holds those levels,
// and at the next start a roomy window funds, her verbs rise whole again.
#[test]
fn a_window_that_cannot_hold_the_epochs_levels_collapses_them_and_they_rise_again() {
    let faculty = asha_with_hands();
    let ws = Workspace::new("what changed on the board?");
    let first = faculty.prompt_view_within(&ws, 200_000);
    let names = faculty.tool_surface().names_of(&first.tools);
    let (probe, _) = tools_within(&faculty, &ws, 200_000);
    assert_eq!(probe.sent.tokens, first.tools.tokens, "precondition: an ample window holds her verbs whole");
    let floor = {
        let charge = |tokens: usize| tokens as u32;
        let offer = faculty.tools_offer(&ws, &charge);
        let framing = faculty.framing_offer(&ws, false, offer.names(), None, &charge);
        offer.head().leveled.hard_limit() + framing.leveled.want()
    };
    let (small, act) = tools_within(&faculty, &ws, floor);
    assert!(act.lost(continuum_context::allocation::id::TOOLS), "the held whole levels were lost, loudly");
    assert!(act.irreducible_shortfall().is_none(), "nothing refuses: every verb fits at its pointer");
    assert!(small.sent.tokens < first.tools.tokens, "her verbs collapsed: {} of {}", small.sent.tokens, first.tools.tokens);
    assert_eq!(faculty.tool_surface().names_of(&small.sent), names, "every verb is still sent, in its place");
    let (sent, _) = faculty.retain_tools(small, false, false);
    let held = faculty.prompt_view_within(&ws, 200_000);
    assert_eq!(wire(&held.tools), wire(&sent), "the head holds its collapsed levels mid-epoch");
    let (roomy, _) = tools_within(&faculty, &ws, 200_000);
    assert!(roomy.next.is_some(), "a roomy window reports the rise for her head's next epoch");
    let (risen, _) = faculty.retain_tools(roomy, false, true);
    assert_eq!(wire(&risen), wire(&first.tools), "a funded start raises her verbs whole again");
}

// what this catches: a held card's transition rewriting her tool head mid-epoch. The served
// template (Qwen3.6-35B-A3B's, read from its GGUF on the M5) renders her tools first of all, so a
// set re-selected the moment her moves changed (`delib.tools.epoch_moves_changed`, Aiko on the M5,
// 2026-11-06 17:04Z and 19:04Z; her list went 39 -> 30 verbs at 19:21Z) re-prefilled every token
// behind the change. Mid-epoch her head holds and the verbs her new moves add ride the tail,
// callable at once; the act that starts her view's next epoch adopts the set her moves select.
#[test]
fn a_moves_change_waits_for_her_views_next_start_and_rides_the_tail_until_then() {
    use continuum_context::source::Source;
    use crate::cognition::work_preconditions::{fixture_card, Held, FIXTURE_NOW_MS};
    let faculty = asha_with_hands();
    // A room that declares one verb: her head is that verb and the discovery pair, smaller than her hands.
    let ws = declaring_one_hand(&faculty);
    let first = faculty.prompt_view_within(&ws, 200_000);
    assert!(first.capacity_error.is_none(), "{:?}", first.capacity_error);
    let epoch = view_epoch(&faculty);
    let surface = faculty.tool_surface();
    let head = surface.names_of(&first.tools);
    // A move her held card puts in her hands that her held head lacks, and that its own selection
    // takes. From the built surface (the live native registry), so the fixture tracks its names.
    let verb: &'static str = surface
        .names()
        .iter()
        .zip(surface.registry())
        .find(|(name, spec)| {
            surface.hands().iter().any(|hand| hand.name == spec.name)
                && !head.contains(name)
                && surface.names_of(&surface.select(&ws.room_affordances, std::slice::from_ref(*name))).contains(name)
        })
        .map(|(name, _)| &*Box::leak(name.clone().into_boxed_str())) // a held card's moves are static names
        .expect("precondition: a verb of her hands outside her head that a move selects");
    let her = airc_core::PeerId::from_uuid(Uuid::from_u128(1));
    let mut held = Held::of(fixture_card(airc_work::CardState::InProgress, Some(her), None, "fix the thing"), her, FIXTURE_NOW_MS, 1).expect("test: she holds it");
    held.in_hand = vec![verb];
    let moved = declaring_one_hand(&faculty).held_work(crate::cognition::workspace::HeldWork { holds: vec![held], ..Default::default() });
    let wire_name = surface.retained(&[verb.to_string()], first.tools.reason, 0).specs.expect("test: her surface has tools")[0].name.clone(); // JUSTIFIED: test fixture

    let second = faculty.prompt_view_within(&moved, 200_000);
    assert_eq!(view_epoch(&faculty), epoch, "precondition: no act turned the view over");
    assert_eq!(wire(&second.tools), wire(&first.tools), "her head holds mid-epoch");
    assert!(second.tools.tail.iter().any(|spec| spec.name == wire_name), "the move's verb rides the tail, callable now: {:?}", second.tools.tail);

    // The act that starts her view's next epoch adopts the set her moves select.
    let (choice, _) = tools_within(&faculty, &moved, 200_000);
    let none: [&dyn Source<crate::cognition::context_streams::ContextMessage>; 0] = [];
    faculty.context_view.lock().as_mut().expect("a view").turn_over(&continuum_context::cost::Chars, Vec::new(), &none, |_| Vec::new()).expect("no sources, no grants");
    let (adopted, _) = faculty.retain_tools(choice, false, true);
    assert!(surface.names_of(&adopted).iter().any(|name| name == verb), "the start adopts the set her moves select");
    let next = faculty.prompt_view_within(&moved, 200_000);
    assert_eq!(wire(&next.tools), wire(&adopted), "and holds it");
}

// what this catches (card 9cf06ade, slice 1): the request and contribute's per-act template clone
// deep-copying her tool head. A request built from her view carries the view's schemas by pointer,
// its clone shares them, and its wire JSON is byte-identical to the same schemas as an owned list.
// Not covered here (slice 2): `retained` and `at_levels` still build a fresh head each act.
#[test]
fn the_request_and_its_clone_carry_her_head_without_copying_it() {
    let faculty = asha_with_hands();
    let ws = Workspace::new("what changed on the board?");
    let view = faculty.prompt_view_within(&ws, 200_000);
    assert!(view.capacity_error.is_none(), "{:?}", view.capacity_error);
    let head = view.tools.specs.clone().expect("her hands are a surface"); // JUSTIFIED: test fixture
    let shared = Arc::clone(&head);
    let binding = faculty.binding.load_full();
    let reserve = view.completion_reserve;
    let request = faculty.build_request_within(&binding, reserve, view.messages, Some(shared), view.system, None, Some(ws.room_id), TurnKind::Pass);
    let template_clone = request.clone();
    let (Some(sent), Some(cloned)) = (request.tools.as_ref(), template_clone.tools.as_ref()) else {
        panic!("the request carries her head"); // JUSTIFIED: test fixture
    };
    assert!(Arc::ptr_eq(sent, &head), "the request points at the view's schemas");
    assert!(Arc::ptr_eq(cloned, &head), "the template's per-act clone shares them");
    let owned: Vec<NativeToolSpec> = head.to_vec();
    let (shared_wire, owned_wire) = (serde_json::to_string(sent).expect("schemas"), serde_json::to_string(&owned).expect("schemas"));
    assert_eq!(shared_wire, owned_wire, "the wire bytes are the schemas', whoever holds them");
}

/// A deploy between two acts: her checkpoint written and read back by a new process, whose
/// faculty is built on the restored memory with `surface` (`None`: the registry as it is).
fn after_a_restart(wm: &WorkingMemory, surface: Option<crate::cognition::tool_surface::ToolSurface>) -> (Arc<WorkingMemory>, LlmDeliberationFaculty) {
    let saved = serde_json::to_value(wm.snapshot()).expect("a checkpoint encodes");
    let restored = Arc::new(WorkingMemory::new(8));
    restored.restore(serde_json::from_value(saved).expect("its own checkpoint decodes"));
    let mut faculty = asha_with_hands().with_working_memory(Arc::clone(&restored));
    if let Some(surface) = surface {
        faculty.surface = surface;
    }
    (restored, faculty)
}

/// The live registry with one held verb's description changed: a deploy that changed its schema.
fn registry_changing(faculty: &LlmDeliberationFaculty, wire_name: &str) -> crate::cognition::tool_surface::ToolSurface {
    let mut offers = crate::cognition::persona_tools::native_offers();
    let offer = offers
        .iter_mut()
        .find(|offer| {
            let spec = offer.spec.clone();
            crate::ai::tool_dialect::to_wire_spec(spec).name == wire_name
        })
        .expect("the held verb is a native offer"); // JUSTIFIED: test fixture
    offer.spec.description.push_str(" Its contract changed in this deploy.");
    let tools = faculty.tools.clone();
    crate::cognition::tool_surface::ToolSurface::build(offers, tools, crate::ai::tool_dialect::to_wire_spec, |_| continuum_base::tool_contract::ContractRevision::FIRST)
}

fn schemas(specs: &[NativeToolSpec]) -> String {
    serde_json::to_string(specs).expect("schemas")
}

// what this catches (card 9cf06ade; the M5's #6102 deploy, 2026-11-17: `work_note`'s description
// changed, every resident's prefix broke at token 5,739 and Aiko's 119,972-token prompt was
// recomputed in 435 s): a restart re-reading her head from a registry whose schema changed. The
// schema-only replay: the same messages, one held verb's contract changed between the acts. Her
// head after the restart is byte-identical to the one her engine holds, her system turn too, and
// she is told the verb's contract moved before she acts.
#[test]
fn a_restart_sends_the_head_she_froze_and_tells_her_what_moved() {
    let wm = Arc::new(WorkingMemory::new(8));
    let ws = Workspace::new("what changed on the board?");
    let before = asha_with_hands().with_working_memory(Arc::clone(&wm));
    let first = before.prompt_view_within(&ws, 200_000);
    assert!(first.capacity_error.is_none(), "{:?}", first.capacity_error);
    let frozen = first.tools.specs.clone().expect("her hands are a surface"); // JUSTIFIED: test fixture
    let moved = frozen[0].name.clone();
    let canonical = crate::ai::tool_dialect::resolve_wire_name(&moved);
    let changed = registry_changing(&before, &moved);
    let (_, after) = after_a_restart(&wm, Some(changed));
    let second = after.prompt_view_within(&ws, 200_000);
    assert!(second.capacity_error.is_none(), "{:?}", second.capacity_error);
    let sent = second.tools.specs.clone().expect("her hands are a surface"); // JUSTIFIED: test fixture
    assert_eq!(schemas(&sent), schemas(&frozen), "her head is the bytes her engine holds, not the changed registry's");
    assert_eq!(second.system, first.system, "her system turn is unchanged");
    let told = |view: &DeliberationPromptView| view.messages.iter().filter(|m| m.content_text().contains(&format!("My tool `{canonical}` changed"))).count();
    assert_eq!(told(&first), 0, "nothing moved before the deploy");
    assert_eq!(told(&second), 1, "she is told before she acts, once");
}

// what this catches (Codex's review of 9cf06ade): a notice marked told when it was made rather than
// when a served prompt showed it, or told again on every wake and after every restart.
#[test]
fn a_contract_notice_is_told_once_from_what_was_served_and_survives_a_restart() {
    let wm = Arc::new(WorkingMemory::new(8));
    let ws = Workspace::new("what changed on the board?");
    let before = asha_with_hands().with_working_memory(Arc::clone(&wm));
    let first = before.prompt_view_within(&ws, 200_000);
    let frozen = first.tools.specs.clone().expect("her hands are a surface"); // JUSTIFIED: test fixture
    let moved = frozen[0].name.clone();
    let canonical = crate::ai::tool_dialect::resolve_wire_name(&moved);
    let notice = format!("My tool `{canonical}` changed");
    let (restored, after) = after_a_restart(&wm, Some(registry_changing(&before, &moved)));
    let second = after.prompt_view_within(&ws, 200_000);
    assert!(second.messages.iter().any(|m| m.content_text().contains(&notice)), "the notice is in her prompt");
    // Not served: the next wake still owes it.
    assert_eq!(restored.contract_changes().len(), 1, "made is not told");
    after.view_served(&second);
    let third = after.prompt_view_within(&ws, 200_000);
    assert!(restored.contract_changes().is_empty(), "served whole, it is told");
    assert_eq!(third.messages.iter().filter(|m| m.content_text().contains(&notice)).count(), 1, "kept in her view once, never appended again");
    let (_, later) = after_a_restart(&restored, Some(registry_changing(&before, &moved)));
    let fourth = later.prompt_view_within(&ws, 200_000);
    assert_eq!(schemas(fourth.tools.specs.as_deref().expect("her hands are a surface")), schemas(&frozen), "her head is still the bytes her engine holds"); // JUSTIFIED: test fixture
    assert!(later.working_memory.as_deref().is_some_and(|wm| wm.contract_changes().is_empty()), "what she was told survives the restart");
}

// what this catches (Codex's review of 9cf06ade): an undecodable head read as absent and silently
// replaced by the live tools, or its bytes lost; and a view checkpointed with no head beside it given
// the live tools silently, or its blocks dropped. Both wait for an explicit adoption, view kept.
#[test]
fn a_head_that_cannot_resume_refuses_her_act_keeps_her_view_and_lifts_on_adoption() {
    let wm = Arc::new(WorkingMemory::new(8));
    let ws = Workspace::new("what changed on the board?");
    let before = asha_with_hands().with_working_memory(Arc::clone(&wm));
    let first = before.prompt_view_within(&ws, 200_000);
    assert!(first.protocol_error.is_none(), "{:?}", first.protocol_error);
    let epoch = view_epoch(&before);
    use crate::cognition::hands_custody::{HandsCustody, SupersededPrior};
    let saved = wm.snapshot();
    assert!(matches!(saved.context_hands, HandsCustody::Held(_)), "her head is checkpointed beside her view");
    let unreadable = serde_json::json!({"generation": "not a number"});

    // A record that did not decode (its decode is `hands_custody`'s unit test), restored beside her view.
    let mut corrupt = saved.clone();
    let record = crate::cognition::hands_custody::CorruptHands { raw: unreadable, error: "invalid type".to_string(), original: None };
    let as_read = HandsCustody::Corrupt(record);
    corrupt.context_hands = as_read.clone();
    let broken = Arc::new(WorkingMemory::new(8));
    broken.restore(corrupt);
    let repairing = asha_with_hands().with_working_memory(Arc::clone(&broken));
    let refused = repairing.prompt_view_within(&ws, 200_000);
    assert!(refused.protocol_error.as_deref().is_some_and(|error| error.contains("does not decode") && error.contains("persona/hands/adopt")), "{:?}", refused.protocol_error);
    assert_eq!(broken.snapshot().context_hands, as_read, "it is checkpointed as it was");
    broken.request_hands_adoption("operator");
    let repaired = repairing.prompt_view_within(&ws, 200_000);
    assert!(repaired.protocol_error.is_none(), "an explicit adoption lifts it: {:?}", repaired.protocol_error);
    let kept = broken.snapshot().superseded_hands;
    let [replaced] = kept.as_slice() else {
        panic!("one adoption replaced one head: {kept:?}"); // JUSTIFIED: test fixture
    };
    let HandsCustody::Corrupt(record) = as_read else {
        panic!("the record restored is corrupt"); // JUSTIFIED: test fixture
    };
    assert_eq!(replaced.prior, SupersededPrior::Corrupt(record), "the record it replaced is kept, never erased");
    assert_eq!((replaced.by.as_str(), replaced.generation >= 1), ("operator", true), "with who asked and the generation that replaced it");

    // A checkpoint from before heads: its view, and no head beside it.
    let mut legacy = saved;
    legacy.context_hands = HandsCustody::Unrecorded;
    let old = Arc::new(WorkingMemory::new(8));
    old.restore(legacy);
    let resumed = asha_with_hands().with_working_memory(Arc::clone(&old));
    let waiting = resumed.prompt_view_within(&ws, 200_000);
    assert!(waiting.protocol_error.as_deref().is_some_and(|error| error.contains("no head beside it")), "{:?}", waiting.protocol_error);
    assert_eq!(view_epoch(&resumed), epoch, "her view is kept, its blocks too");
    old.request_hands_adoption("operator");
    let adopted = resumed.prompt_view_within(&ws, 200_000);
    assert!(adopted.protocol_error.is_none(), "an explicit adoption lifts it: {:?}", adopted.protocol_error);
    assert_eq!(view_epoch(&resumed), epoch, "and keeps her view");
}

// what this catches (Codex's review of 9cf06ade): a legacy head handed back warm on a guess, or a
// capture that differs adopted anyway. A view checkpointed before heads, with the request her engine
// was served recorded, adopts that request's head only where her accepted frontier (her sent view,
// through the render her acts send) and this act's system prompt, tool payload, model and messages
// match it byte for byte, over the custody the adoption names. A capture that differs is refused with
// where it differs and her refusal kept; the one that matches is adopted and recorded as such.
#[test]
fn a_head_is_adopted_from_the_capture_it_matches_and_refused_from_one_that_differs() {
    use crate::cognition::capture_adoption::{CaptureBinding, CapturedHead};
    use crate::cognition::hands_custody::HandsCustody;
    let wm = Arc::new(WorkingMemory::new(8));
    let ws = Workspace::new("what changed on the board?");
    let before = asha_with_hands().with_working_memory(Arc::clone(&wm));
    let first = before.prompt_view_within(&ws, 200_000);
    assert!(first.protocol_error.is_none(), "{:?}", first.protocol_error);
    // The request her engine was served: what her capture records.
    let binding = before.binding.load_full();
    let (messages, tools, system) = (first.messages.clone(), first.tools.specs.clone(), first.system.clone());
    let captured = before.build_request_within(&binding, first.completion_reserve, messages, tools, system, None, Some(ws.room_id), TurnKind::Pass);
    before.view_served(&first);

    // A checkpoint from before heads: her view, no head beside it.
    let mut legacy = wm.snapshot();
    legacy.context_hands = HandsCustody::Unrecorded;
    let restored = Arc::new(WorkingMemory::new(8));
    restored.restore(legacy);
    let after = asha_with_hands().with_working_memory(Arc::clone(&restored));
    assert!(after.prompt_view_within(&ws, 200_000).protocol_error.is_some(), "a legacy view waits for an adoption");
    let ask = |request: crate::ai::types::TextGenerationRequest| {
        let expected_custody_sha256 = restored.custody_receipt().expect("her custody digests").sha256;
        let binding = CaptureBinding { cursor: "the capture".to_string(), request_sha256: "its digest".to_string(), expected_custody_sha256 };
        restored.request_capture_adoption("operator", CapturedHead { binding, request: Arc::new(request) });
    };

    let mut differs = captured.clone();
    differs.system_prompt = differs.system_prompt.map(|system| format!("{system} "));
    ask(differs);
    let refused = after.prompt_view_within(&ws, 200_000);
    assert!(refused.protocol_error.as_deref().is_some_and(|error| error.contains("system prompt differs")), "{:?}", refused.protocol_error);
    assert!(restored.snapshot().superseded_hands.is_empty(), "nothing was adopted");

    ask(captured);
    let adopted = after.prompt_view_within(&ws, 200_000);
    assert!(adopted.protocol_error.is_none(), "the capture she was served adopts: {:?}", adopted.protocol_error);
    let kept = restored.snapshot().superseded_hands;
    assert!(kept.last().is_some_and(|replaced| replaced.from_capture.as_ref().is_some_and(|from| from.cursor == "the capture")), "recorded as adopted from it: {kept:?}");
    assert!(matches!(restored.snapshot().context_hands, HandsCustody::Held(_)), "her head is held");
}
