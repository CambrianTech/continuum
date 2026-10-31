#![cfg(test)]

use super::*;

// what this catches: THE CLIP TAKING AWAY THE EVIDENCE OF WHAT SHE JUST DID.
//
// The chain, measured on the M5 2026-09-20: the conversation assembly is
// sophisticated, a flat cap lands on top of it (`delib.fill.latency_capped
// persona=Aiko window_budget=56227 cap=15000`), the drop walks up the history by
// COST until it reaches the run at the end — her act and its result — and she
// loses the record of her own last moves. She re-issues them:
// `persona.act.repeat_short_circuited` ("identical act already satisfied this
// turn"), then `persona.act.no_deliverable_yet`, then an hour with zero writes
// that reads as a lazy persona. A budget that binds must drop by recency-of-
// consequence, and her own last move is the LAST thing to go.
#[test]
fn a_binding_fill_budget_keeps_her_own_last_move_and_its_result() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "T", "You are T.", adapter);
    let mut history: Vec<ChatMessage> = (0..40)
        .map(|i| ChatMessage::text("user", format!("old{i} {}", "word ".repeat(95))))
        .collect();
    history.push(ChatMessage::text("user", format!("please write the file {}", "word ".repeat(40))));
    history.push(ChatMessage::text("assistant", format!("calling code/write {}", "word ".repeat(40))));
    history.push(ChatMessage::text("user", format!("Full result of code/write {}", "word ".repeat(60))));
    let prompt = || PromptMessages {
        streams: Default::default(),
        result_pointers: Vec::new(),
        input_identity: [0; 32],
        grounding_tokens: 0,
        grounding_at: 0,
        history: history.clone(),
        stimulus: None,
        latest_result: Vec::new(), action_statement: None,
    };

    // The run that must survive: her act, its result, and the ask that opened them.
    let evidence = prompt().recent_move_evidence();
    assert_eq!(evidence.messages, 3, "act + result + the turn that asked for it");
    assert!(evidence.tokens > 0);

    // A derived cap far below that evidence is RAISED to it — re-issuing an act
    // she already ran costs a whole extra turn, which is the opposite of what a
    // latency budget is for.
    let starved = 10usize;
    let budget = prompt().required_tokens() + starved.max(evidence.tokens);
    assert_eq!(budget, evidence.tokens, "the evidence floors the cap");

    let fitted = faculty
        .fit_messages(prompt(), budget)
        .expect("the floored budget fits")
        .messages;
    assert!(
        fitted.iter().any(|m| m.content_text().starts_with("Full result of code/write")),
        "the RESULT of her last act must survive a binding budget"
    );
    assert!(
        fitted.iter().any(|m| m.content_text().starts_with("calling code/write")),
        "…and the act itself"
    );
    assert!(
        fitted.iter().any(|m| m.content_text().starts_with("please write the file")),
        "…and the ask, so the act is never an answer without its question"
    );
    assert!(
        !fitted.iter().any(|m| m.content_text().starts_with("old0 ")),
        "the OLDEST turns are what a binding budget spends"
    );
    assert!(
        LlmDeliberationFaculty::messages_cost(&fitted) <= budget,
        "protecting her last move must never push the prompt over the window"
    );

    // When the budget cannot hold the act and its result at all, the turn
    // REFUSES rather than forgetting a completed act — see
    // `continuity_insufficient_capacity_refuses_instead_of_dropping_evidence`.
    let last_cost = LlmDeliberationFaculty::message_cost(
        history.last().expect("the history ends with her act's result"),
    );
    assert!(
        faculty.fit_messages(prompt(), last_cost).is_err(),
        "a budget too small for her completed act is a capacity error, not a \
                 silent forgetting"
    );
}

// what this catches: the LIMITS of the recent-move protection, pinned so this
// change cannot read as if recent-action loss is closed.
//
// The behaviour itself is specified by Astra's two regressions above
// (`continuity_followup_keeps_completed_action_evidence`,
// `continuity_insufficient_capacity_refuses_instead_of_dropping_evidence`); this
// test is deliberately NOT a parallel version of them. It pins the two edges the
// heuristic still has: a tool chain longer than the scan bound is protected only
// as a SUFFIX and says so, and a move further back than the bound is not
// protected at all. Source-owned causal grouping is what closes those.
#[test]
fn the_recent_move_protection_states_its_own_limits() {
    let msg = |role: &str, body: &str| ChatMessage::text(role, body.to_string());
    let of = |history: Vec<ChatMessage>| PromptMessages {
        streams: Default::default(),
        result_pointers: Vec::new(),
        input_identity: [0; 32],
        grounding_tokens: 0,
        grounding_at: 0,
        history,
        stimulus: None,
        latest_result: Vec::new(), action_statement: None,
    };

    // A MULTI-TOOL CHAIN LONGER THAN THE SCAN BOUND. Protected as a suffix, and
    // SAID to be one: no opener is added, because the message before the cut is
    // another link in the chain, not the turn that opened it.
    let mut chain = vec![msg("user", "do the whole migration")];
    for i in 0..5 {
        chain.push(msg("assistant", &format!("calling tool {i}")));
        chain.push(msg("user", &format!("Full result of tool {i}")));
    }
    let evidence = of(chain).recent_move_evidence();
    assert_eq!(
        evidence.messages,
        LlmDeliberationFaculty::CLUSTER_SCAN_MESSAGES,
        "a chain longer than the bound yields a bounded suffix"
    );
    assert!(
        evidence.truncated,
        "and it must SAY it is a suffix — a partial protection that reads as a \
                 whole one is how a limit becomes an invisible loss"
    );

    // A MOVE FURTHER BACK THAN THE SCAN BOUND IS NOT PROTECTED. Stated, not
    // hidden: this is the heuristic's edge, and the receipt carries a zero.
    let mut buried = vec![msg("assistant", "calling code/write")];
    for i in 0..LlmDeliberationFaculty::CLUSTER_SCAN_MESSAGES + 1 {
        buried.push(msg("user", &format!("unrelated room turn {i}")));
    }
    let none = of(buried).recent_move_evidence();
    assert_eq!(
        none,
        RecentMove::default(),
        "beyond the scan bound this protection does not reach — source-owned \
                 causal grouping is what closes that, not a longer sniff"
    );
    assert_eq!(
        none.fit_within(1_000),
        EvidenceFit::NoMove,
        "no move is not a cannot-fit — there is nothing to lose, and the two \
                 must never be conflated on the receipt"
    );

    // AN OVERSIZED RESULT IS A CONDENSE CASE, NOT A CAPACITY FAULT — the one
    // place this deliberately does NOT refuse. Refusing when a single message
    // cannot be carried whole does not preserve it; it mutes her. That exact
    // refusal is a measured outage: an unbounded required result produced 836
    // refusals and zero acts on 2026-09-10, and the substrate's answer was to cap
    // the result at its share of the window and demote the remainder to history —
    // which is the message that lands back here. Only the REDUCIBLE case refuses
    // (`continuity_insufficient_capacity_refuses_instead_of_dropping_evidence`,
    // where every message would individually have fit).
    let huge = of(vec![
        msg("user", "read the file"),
        msg("assistant", "calling code/read"),
        msg(
            "user",
            &format!("Full result of code/read {}", "x".repeat(60_000)),
        ),
    ]);
    let evidence = huge.recent_move_evidence();
    assert_eq!(
        evidence.fit_within(evidence.largest_message - 1),
        EvidenceFit::TooLargeToCarry,
        "a message larger than the whole budget cannot be carried by any policy \
                 — that wants condensing, not a fault that mutes the turn"
    );
    assert_eq!(
        evidence.fit_within(evidence.tokens),
        EvidenceFit::Protected,
        "and it is Protected the moment the budget can hold it"
    );
}

// what this catches: the rent ledger's spine (compression-ladder rung
// 2). The segment map must (1) open with the system run, (2) merge
// consecutive same-label messages into one run, (3) classify bracketed
// blocks and act results as grounding, and (4) let a cached-token
// frontier name the run it died in — the attribution every miss
// becomes and the number the dream curriculum reads.
#[test]
fn segment_map_names_the_run_a_reuse_frontier_dies_in() {
    let msgs = vec![
        ChatMessage::text("user", "Alice: how goes the fix?"),
        ChatMessage::text("assistant", "landed it, running tests"),
        ChatMessage::text(
            "user",
            "[occurred 2023-11-14T22:13:20.123Z] Alice: prior status",
        ),
        ChatMessage::text("assistant", "[occurrence time unknown] Earlier own report."),
        ChatMessage::text("user", "[recall]\n(my memories …)"),
        ChatMessage::text("user", "Full result of your most recent action (#3): ok"),
        ChatMessage::text("user", "Alice: and the grade?"),
    ];
    let map = segment_map("SYSTEM ".repeat(100).as_str(), &msgs);
    let labels: Vec<&str> = map.iter().map(|(l, _)| *l).collect();
    assert_eq!(
        labels,
        vec!["system", "history", "grounding", "history"],
        "runs merge by label in send order: {map:?}"
    );
    assert!(
        map.windows(2).all(|w| w[0].1 < w[1].1),
        "cumulative ends are strictly increasing: {map:?}"
    );
    // A frontier inside the grounding run attributes to grounding.
    let frontier = map[1].1 + 1; // just past history's end
    let broke = map
        .iter()
        .find(|(_, end)| *end > frontier)
        .map(|(l, _)| *l)
        .unwrap_or("fully-warm");
    assert_eq!(broke, "grounding");
    // A frontier past everything is fully warm.
    let past = map.last().unwrap().1 + 1;
    assert_eq!(
        map.iter().find(|(_, end)| *end > past).map(|(l, _)| *l),
        None,
        "reuse past every segment attributes to nothing — fully warm"
    );
}

// what this catches: e731576c — repetition remains observable without
// deleting distinct speech events or replacing them with authored words
// in the citizen's own voice.
#[test]
fn near_duplicate_own_turns_remain_visible_with_repetition_evidence() {
    use crate::cognition::workspace::Burst;
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(
        persona,
        "Casper",
        "You are Casper, a thoughtful engineer on the grid.",
        adapter,
    )
    .with_context_window(8192);

    // Byte-DISTINCT variants of one template (the live specimen's shape) —
    // all pairwise ≥ NEAR_DUP_JACCARD, none byte-equal.
    let v1 = "I apologize for the repetition. Let's focus on the task \"wordstats\". What approach would you like to take for this task?";
    let v2 = "I apologize for the repetition earlier. Let's focus on the task \"wordstats\". What approach would you like to take for this task?";
    let v3 = "I apologize for repeating myself earlier. Let's focus on the task \"wordstats\". What approach would you like to take for this task?";
    let turns = vec![
        BurstTurn::attributed(true, "Casper", v1, Some(1)),
        BurstTurn::attributed(
            false,
            "Anwen",
            "any specific topics you'd like to work on?",
            Some(2),
        ),
        BurstTurn::attributed(true, "Casper", v2, Some(3)),
        BurstTurn::attributed(false, "Atlas", "shall we outline the steps first?", Some(4)),
        BurstTurn::attributed(true, "Casper", v3, Some(5)),
    ];
    let ws = Workspace::new(Burst::from_turns(
        crate::identity::ActivityRoom::mint(),
        turns,
    ));
    let view = faculty.prompt_view(&ws);

    // All three actual statements survive in their original order.
    let apology_renders = view
        .messages
        .iter()
        .filter(|m| m.role == "assistant" && m.content_text().contains("I apologize"))
        .count();
    assert_eq!(
        apology_renders, 3,
        "similarity must not erase actual speech: {view:?}"
    );
    let own: Vec<_> = view
        .messages
        .iter()
        .filter(|message| message.role == "assistant")
        .map(|message| message.content_text())
        .collect();
    assert_eq!(
        own,
        [v1, v2, v3]
            .iter()
            .enumerate()
            .map(|(index, body)| format!(
                "[occurred 1970-01-01T00:00:00.{:03}Z] {body}",
                index * 2 + 1
            ))
            .collect::<Vec<_>>()
    );

    // The same raw history still supports the repetition observation.
    let all_user: String = view
        .messages
        .iter()
        .filter(|m| m.role == "user")
        .map(|m| m.content_text())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        all_user.contains("[repetition] 3 of your recent messages were nearly identical"),
        "raw-turn repetition fact must remain available: {all_user}"
    );
}

// what this catches: [[idle-is-self-directed-free-time]] Layer 1. A self-initiated
// turn must frame REST / turning-attention-elsewhere as a CO-EQUAL legitimate
// outcome ("not a failure to find something"), not only active pursuit — otherwise a
// quiet heartbeat reads as pressure to manufacture activity (the self-tick analogue
// of the polite-filler loop). It must stay NEUTRAL ("yours alone; nothing here is
// telling you which to pick") per [[no-hardcoded-heuristics-to-steer-cognition]], and
// active options must remain FIRST/primary so this never becomes the always-PASS
// doom-loop documented in SILENCE_AFFORDANCE_BLOCK. The block is gated on
// self_initiated — an inbound-driven (ambient/directed) turn must NOT carry it.
#[test]
fn self_initiated_turn_frames_rest_as_co_equal_and_stays_neutral() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter)
        .with_context_window(8192);

    // self_initiated = true, undirected.
    let own_time = faculty.compose_system(&BTreeSet::new(), false, true, None);
    assert!(
        own_time.contains("[Your own time]"),
        "self-initiated turn must carry the own-time framing: {own_time}"
    );
    // Rest / turning-elsewhere is named as legitimate, not a deficiency.
    assert!(
        own_time.contains("not a failure to find something"),
        "rest must be framed as a real choice, not a failure: {own_time}"
    );
    // Neutral — names the option, never scripts when to take it.
    assert!(
        own_time.contains("yours alone; nothing here is telling you which to pick"),
        "the choice must stay the persona's own, uncoached: {own_time}"
    );
    // Active options stay FIRST/primary — the pursue-your-thread framing appears
    // BEFORE the rest framing, so rest is the co-equal alternative, not the lead.
    let pursue = own_time
        .find("Pick up your own train of thought")
        .expect("active framing present");
    let rest = own_time
        .find("do not have to fill the moment")
        .expect("rest framing present");
    assert!(
        pursue < rest,
        "active options must lead; rest is the co-equal alternative: {own_time}"
    );

    // A non-self-initiated (inbound-driven) turn must NOT carry the own-time block.
    let ambient = faculty.compose_system(&BTreeSet::new(), false, false, None);
    assert!(
        !ambient.contains("[Your own time]"),
        "ambient/directed turns must not carry the own-time framing: {ambient}"
    );
}

// what this catches: the category index is small BY CONSTRUCTION — even a
// 120-tool registry at the 2048-token serving floor (MIN_SERVE_CTX) renders to a
// handful of `category (N)` entries, so the framing never alone blows past the
// window and always leaves burst room. A regression (e.g. dumping tool names back
// into the index) reintroduces the overflow at a tiny window.
#[test]
fn catalog_fits_the_minimum_serving_window() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let window = crate::inference::planning::MIN_SERVE_CTX; // 2048 floor
    let tools: Vec<NativeToolSpec> = (0..120)
        .map(|i| NativeToolSpec {
            name: format!("cat{}/command_{i}", i % 8),
            description: "A registry command with a one-line summary for the catalog."
                .to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: json!({ "path": { "type": "string" } }),
                required: Some(vec!["path".to_string()]),
                definitions: None,
            },
        })
        .collect();
    let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter)
        .with_context_window(window)
        .with_tools(tools);

    // The menu (inside framing) plus the completion reserve must leave room for at
    // least SOME burst — i.e. framing alone must not consume the window. With
    // nothing expanded (empty context) the menu is its smallest — a spine of
    // collapsed bookmarks — which is the floor case for this fit guard.
    let collapsed = faculty.compose_system(&BTreeSet::new(), false, false, None);
    let framing = estimate_guard_tokens(&collapsed);
    // The reply's floor — what the window always holds for it — derived from the
    // claim itself, never re-spelled. Above it the reply grows by priority.
    let reserve = reply_claim(TurnKind::Pass, None, 0, false, false, None).floor_tokens as usize;
    assert!(
        framing + reserve < window as usize,
        "framing+menu ({framing}) + reserve ({reserve}) must leave burst room in {window}"
    );
    // The SPINE names every category even when collapsed (`cat0 (15 — commands/list
    // --filter cat0)`) — she always sees the full map and can open any category on
    // demand. What the menu must NOT carry is the per-tool SUMMARY — that one-line
    // description × ~150 tools was the 18KB bloat.
    assert!(
        collapsed.contains("cat0 (15 — commands/list --filter cat0)"),
        "the spine must name each category as a collapsed bookmark: {collapsed}"
    );
    assert!(
        !collapsed.contains("one-line summary for the catalog"),
        "per-tool SUMMARIES must NOT be in the menu (that was the 18KB bloat)"
    );
    // And an EXPANDED category DOES list its verbs (the fix: she must SEE that a
    // tool exists to call it — glass-box: hidden names → 909 code-fences / 3 native
    // runs). Names ride the expansion, summaries never do.
    let opened = faculty.compose_system(
        &BTreeSet::from(["cat0".to_string()]),
        false,
        false,
        None,
    );
    // Bare verb names since the 2026-07-10 prompt diet — args live in
    // commands/help + the #1916 inline error-manual, not an 8k menu wall.
    assert!(
        opened.contains("cat0: command_0"),
        "an expanded category lists its verbs so she can see them: {opened}"
    );
}
