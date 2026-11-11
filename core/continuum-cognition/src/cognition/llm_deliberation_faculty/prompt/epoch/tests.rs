//! Her view's source order ([`LlmDeliberationFaculty::view_sources`]): consecutive acts share
//! the whole view, and a turnover keeps the prefix up to the first source whose front changed.
#![cfg(test)]

use super::*;
use crate::cognition::context_streams::blocks::Grounding;
use crate::cognition::context_streams::conversation::{Conversation, Turn};
use crate::cognition::context_streams::Role;

// A source's average full-message density is not necessarily the density of its
// retained newest messages. Exercise the actual start and final message pricing.
#[test]
fn a_view_start_keeps_its_token_grant_with_uneven_message_sizes() {
    use continuum_context::flex::{AllocationState, FlexAllocation, SourceAllocation};
    let conversation = Conversation {
        turns: (0..40).map(|n| Turn {
            key: format!("density-{n}"),
            role: Role::User,
            line: if n < 20 { "long old message ".repeat(100) } else { "ok".to_owned() },
            pointer: None,
        }).collect(),
        ..Default::default()
    };
    let items = conversation.items();
    let full: usize = items.iter().map(|item| LlmDeliberationFaculty::message_cost(&item.full().to_chat())).sum();
    for granted in [32, 64, 128, 256, 512] {
        let plan = FlexAllocation {
            allocations: vec![SourceAllocation {
                source_id: format!("{VIEW_CLAIM}conversation"),
                allocated_tokens: granted,
                requested_floor: 0,
                requested_min: 0,
                requested_max: full as u32,
                required: false,
                state: AllocationState::Satisfied,
            }],
            unallocated: 0,
            escalation_needed: false,
            warnings: Vec::new(),
        };
        let sources: Vec<&dyn Source<ContextMessage>> = vec![&conversation];
        let (view, _) = View::start(&AllocationEstimate::uncalibrated(), Vec::new(), &sources, |claims| {
            start_grants(claims, &plan, granted as usize, None).expect("funded optional source")
        }).expect("matching source grants");
        let rendered: Vec<_> = view.render().map(ContextMessage::to_chat).collect();
        let actual = PromptCalibration::default().estimate_all(&rendered);
        assert!(actual <= granted as usize, "start rendered {actual} tokens against grant {granted}");
    }
}

fn block(faculty: &str, text: &str) -> Grounding {
    Grounding::atomic(uuid::Uuid::nil(), faculty, text)
}

fn turn(n: usize) -> Turn {
    Turn { key: format!("turn-{n}"), role: Role::User, line: format!("[occurred {n}] Operator: turn {n}"), pointer: None }
}

/// One act's sources: a room window of six turns starting at `first_turn`, a settled map, the
/// kanban (canonical order, as `messages_with_visual_feedback` sorts them), and a recall block
/// that says something new every act.
fn streams(first_turn: usize, recall: &str) -> Streams {
    Streams {
        conversation: Conversation { turns: (first_turn..first_turn + 6).map(turn).collect(), ..Default::default() },
        blocks: vec![
            block("workspace-map", "MAP: src/ tests/ docs/"),
            block("room-kanban", "BOARD: you hold 1 card; 3 claimable"),
            block("recall", recall),
        ],
        ..Default::default()
    }
}

/// The sources in the core's order, or in the order before 2026-10-24 (her conversation, her
/// actions, every block, her thoughts).
fn ordered<'a>(
    streams: &'a Streams,
    memory: &'a MemoryStreams<'a>,
    conversation_first: bool,
) -> Vec<&'a dyn Source<ContextMessage>> {
    if !conversation_first {
        return LlmDeliberationFaculty::view_sources(streams, Some(memory));
    }
    let mut sources: Vec<&dyn Source<ContextMessage>> = vec![&streams.conversation, &memory.actions];
    sources.extend(streams.blocks.iter().map(|block| block as &dyn Source<ContextMessage>));
    sources.push(&memory.thoughts);
    sources
}

fn texts(view: &View<ContextMessage>) -> Vec<String> {
    view.render().map(|message| message.text.clone()).collect()
}

fn shared(before: &[String], after: &[String]) -> usize {
    before.iter().zip(after).take_while(|(a, b)| a == b).count()
}

/// Two acts of one epoch, then a turnover that starts her growing sources at half (their
/// oldest items collapse, as `start_grants` reserves): what the second act's view shared with
/// the first's, how long the first was, and what the turnover kept of the view before it.
fn two_acts_and_a_turnover(conversation_first: bool) -> (usize, usize, usize) {
    let memory = WorkingMemory::new(8);
    memory.record_receipt("I ran code/list Result: 4 files");
    memory.record_settlement("listed", None);
    let memory_streams = MemoryStreams::of(&memory, uuid::Uuid::nil());
    let first = streams(0, "RECALL: the repro lives in tests/");
    let whole = |claims: &[Claim]| claims.iter().map(|claim| claim.full).collect();
    let (mut view, _) = View::start(&continuum_context::cost::Chars, Vec::new(), &ordered(&first, &memory_streams, conversation_first), whole)
        .expect("test: one grant per source");
    let before = texts(&view);

    // Her next act: another result, the room slid by one turn, recall says something new.
    memory.record_receipt("I ran code/read(main.rs) Result: 120 lines");
    memory.record_settlement("read it", None);
    let next = streams(1, "RECALL: main.rs calls the parser");
    let sources = ordered(&next, &memory_streams, conversation_first);
    view.advance(&sources);
    let advanced = texts(&view);

    let half_of_growing = |claims: &[Claim]| {
        let grounding = crate::cognition::context_streams::priority::GROUNDING;
        claims
            .iter()
            .map(|claim| if claim.priority >= grounding { claim.full } else { claim.full / 2 })
            .collect()
    };
    view.turn_over(&continuum_context::cost::Chars, Vec::new(), &sources, half_of_growing).expect("test: one grant per source");
    (shared(&before, &advanced), before.len(), shared(&advanced, &texts(&view)))
}

// what this catches: a growing section rendered at the head of her view. Within an epoch the
// next act's prompt holds the whole previous view and appends; at a turnover, the one rewrite
// an epoch costs, her conversation led and its oldest turn left, so the prompt broke at its
// first message and everything behind identity and tools re-prefilled (Benchy on the M5,
// 2026-10-24 00:32Z: `delib.prompt.common_prefix` index 2 after a turnover, 20% shared; Delia on
// the IntelMac, 10-24 00:14Z: index 2 at `[actions]`, 41%). A first start lays her blocks first;
// the turnover orders by measured motion, so a block that never changed (the map) leads it and
// recall, which changed, moves behind what held still.
#[test]
fn consecutive_acts_share_the_whole_view_and_a_turnover_keeps_the_settled_front() {
    let memory = WorkingMemory::new(8);
    let memory_streams = MemoryStreams::of(&memory, uuid::Uuid::nil());
    let now = streams(0, "RECALL: one");
    let names: Vec<&str> =
        LlmDeliberationFaculty::view_sources(&now, Some(&memory_streams)).iter().map(|s| s.name()).collect();
    assert_eq!(
        names,
        ["workspace-map", "room-kanban", "recall", "held-work", "actions", "conversation", "thoughts", "tasks", "open-views", "notices", "facts"],
        "a first start's tie: her grounding blocks, her held work, her actions and conversation, then what changes within an epoch, then what her situation shows now, right before the act's tail"
    );

    let (kept, length, survived) = two_acts_and_a_turnover(true);
    assert_eq!(kept, length, "precondition: within an epoch every order appends");
    assert_eq!(survived, 0, "precondition: her conversation first, a turnover keeps none of her view");

    let (kept, length, survived) = two_acts_and_a_turnover(false);
    assert_eq!(kept, length, "the next act's prompt holds the whole previous view, then its tail");
    // The map and the `[actions]` banner at least: the first change is inside her actions (an
    // act the turnover collapsed, or the act the epoch had appended at its end).
    assert!(survived >= 2, "a turnover, ordered by measured motion, keeps what never changed at its front: {survived}");
}

// what this catches: a view that fits turning over, cold, to raise a collapsed item a level
// (the M5, 2026-10-31: 32 of 91 turnovers in 3 h, each 101-111 s of prefill instead of 6-8 s).
// A restore is taken only on an act whose next append would turn the epoch over anyway, and
// only then is a whole start priced; an overflow always turns over.
#[test]
fn a_fitting_view_restores_only_when_the_next_act_would_turn_it_over() {
    let never = || -> Result<bool, ()> { panic!("a start is priced only when the next act would overflow") };
    assert_eq!(turnover_due(21_000, 32_000, Some(5_000), never), Ok(Turnover::Keep), "room for the next act: keep appending");
    assert_eq!(turnover_due(21_000, 32_000, None, never), Ok(Turnover::Keep), "nothing measured yet: keep appending");
    assert_eq!(turnover_due(30_000, 32_000, Some(5_000), || Ok::<_, ()>(true)), Ok(Turnover::Restore), "the next act overflows: restore now");
    assert_eq!(turnover_due(30_000, 32_000, Some(5_000), || Ok::<_, ()>(false)), Ok(Turnover::Keep), "nothing to restore: keep");
    assert_eq!(turnover_due(33_000, 32_000, Some(5_000), never), Ok(Turnover::Overflow), "over its grant: turn over");
    assert_eq!(turnover_due(30_000, 32_000, Some(5_000), || Err("mismatch")), Err("mismatch"), "a broken allocation refuses");
}

// what this catches: a turn sized before her window is known (granted nothing) turning her view
// over to an empty start, after which the next act re-appends every item her sources hold and
// turns over again (2026-11-02, every M5 resident at boot). With nothing allowed the view is kept.
#[test]
fn a_view_granted_nothing_is_kept_never_started_empty() {
    let due = turnover_due::<()>(33_165, 0, Some(10_000), || Ok(true)).expect("pure");
    assert_eq!(due, Turnover::Keep, "no grant is not an overflow");
    let due = turnover_due::<()>(33_165, 29_000, Some(10_000), || Ok(true)).expect("pure");
    assert_eq!(due, Turnover::Overflow, "a real grant it outgrew still turns over");
}

/// One growing source, priced in the allocation's tokens (her view's allocation estimate), whose one older item
/// is measured as never asked for again, so the depth collapses it and starts the source at its
/// base: her 1-token floor with her 6-token newest item whole.
fn depth_one_token_under_her_newest() -> (Vec<continuum_context::view::Claim>, continuum_context::flex::FlexAllocation) {
    use continuum_context::depth::Collapsible;
    use continuum_context::rereads::{Hazard, Tally};
    let claim = continuum_context::view::Claim {
        source: "results".to_owned(),
        priority: 0,
        floor: 1,
        full: 10,
        newest: 6,
        logged: 0,
        held: 0,
        instrumented: true,
        collapsible: vec![Collapsible { whole: 4, saves: 4, age: 1, hazard: Hazard::Measured { tally: Tally { exposure: 1_000.0, events: 0.0 }, prior: 1.0 } }],
    };
    let grant = continuum_context::flex::SourceAllocation {
        source_id: format!("{VIEW_CLAIM}results"),
        allocated_tokens: 10,
        requested_floor: 1,
        requested_min: 1,
        requested_max: 10,
        required: false,
        state: continuum_context::flex::AllocationState::Satisfied,
    };
    let plan = continuum_context::flex::FlexAllocation { allocations: vec![grant], unallocated: 0, escalation_needed: false, warnings: Vec::new() };
    (vec![claim], plan)
}

// what this catches: #5807's failure again, through the measured depth: her newest turn collapsed
// by a char (`reply_priority_grows_the_reply_not_at_history_or_required_input`, 2026-11-06). The
// depth priced her newest item in rounded tokens (chars times an average) and `StartSource::newest`
// in ceiled ones, so a depth that lowered her grant, or sized her start, to its own sum started her
// a token under her newest item whole. Priced by her allocation estimate both are one number; the depth's lowered
// grant still never falls under it.
#[test]
fn a_measured_depth_never_starts_her_under_her_newest_item() {
    let (claims, plan) = depth_one_token_under_her_newest();
    let grants = start_grants(&claims, &plan, 100, Some(1)).expect("an optional source is never refused");
    assert!(grants[0] >= claims[0].newest, "her newest item whole: granted {} of {} tokens", grants[0], claims[0].newest);
    // The lowered grant itself, at the fixture's tokens: the depth chose 5, her newest needs 6.
    let source = continuum_context::allocation::StartSource { priority: 0, fixed: false, required: false, floor: 1, grant: 10, newest: 6 };
    assert_eq!(depth_grant(5, &source), 6, "never below her newest item");
    assert_eq!(depth_grant(0, &continuum_context::allocation::StartSource { floor: 7, ..source }), 7, "never below its floor");
    assert_eq!(depth_grant(40, &source), 10, "never above the plan's grant");
}

/// A conversation of `old` long turns then `newest` short ones (`long` and `short` are their
/// lines), started under [`start_grants`] with `grant` tokens and a floor of 0 (a turn leaves her
/// view when collapsed), its want the actual messages' cost. What the start renders, charged by
/// the final prompt's own cost ([`PromptCalibration::estimate_all`]), and how many messages.
fn started_conversation(old: usize, long: &str, newest: usize, short: &str, grant: u32) -> (usize, usize) {
    let turns: Vec<Turn> = (0..old)
        .map(|n| (n, long))
        .chain((old..old + newest).map(|n| (n, short)))
        .map(|(n, line)| Turn { key: format!("turn-{n}"), role: Role::User, line: line.to_owned(), pointer: None })
        .collect();
    let conversation = Conversation { turns, ..Default::default() };
    let messages: Vec<ChatMessage> = conversation.items().iter().map(|item| item.full().to_chat()).collect();
    let newest_cost = messages.last().map_or(0, LlmDeliberationFaculty::message_cost); // map_or: the fixture always has turns
    assert!(newest_cost <= grant as usize, "precondition: her newest turn alone fits the grant");
    let plan = continuum_context::flex::FlexAllocation {
        allocations: vec![continuum_context::flex::SourceAllocation {
            source_id: format!("{VIEW_CLAIM}conversation"),
            allocated_tokens: grant,
            requested_floor: 0,
            requested_min: 0,
            requested_max: LlmDeliberationFaculty::clamp_u32(PromptCalibration::default().estimate_all(&messages)),
            required: false,
            state: continuum_context::flex::AllocationState::FloorOnly,
        }],
        unallocated: 0,
        escalation_needed: false,
        warnings: Vec::new(),
    };
    let sources: Vec<&dyn Source<ContextMessage>> = vec![&conversation];
    let sized = |claims: &[Claim]| start_grants(claims, &plan, grant as usize, None).expect("an optional source is never refused");
    let (view, _) = View::start(&AllocationEstimate::uncalibrated(), Vec::new(), &sources, sized).expect("test: one grant per source");
    let rendered: Vec<ChatMessage> = view.render().map(ContextMessage::to_chat).collect();
    (PromptCalibration::default().estimate_all(&rendered), rendered.len())
}

// what this catches: Codex's defect (`a_view_start_keeps_its_token_grant_with_uneven_message_sizes`
// above) on a base whose message cost carries no per-message template tokens (#5814: her density
// carries them), where his short turns cost nothing and pass. The density that still differs per
// message is the estimator's: it counts bytes, and a CJK character is three bytes, one estimated
// token, against an ASCII character's third. A start that converted its TOKEN grant into
// characters at the source's AVERAGE tokens per character bought characters enough for several
// dense newest turns whose own cost overran the grant (red before the allocation estimate: 36 tokens in 3
// messages against 32). The start prices each item with the cost the final prompt is charged with,
// so what it renders never exceeds its grant.
#[test]
fn a_view_start_keeps_its_token_grant_when_her_newest_turns_are_denser() {
    let long = "x".repeat(600);
    let (rendered, messages) = started_conversation(20, &long, 20, "了解しました。すぐ見ます", 32);
    assert!(messages >= 1, "her newest turn is kept whole: it fits");
    assert!(rendered <= 32, "the start rendered {rendered} tokens in {messages} messages against a 32-token grant");
}

// what this catches: the allocation estimate changing what a start keeps when every message costs alike: with
// uniform sizes the average was exact, and a start keeps the same newest turns it kept before.
#[test]
fn a_view_start_with_uniform_message_sizes_keeps_what_it_kept() {
    let line = "y".repeat(90);
    let (rendered, messages) = started_conversation(20, &line, 20, &line, 300);
    assert_eq!(messages, 5, "half the grant (150 tokens) at 30 tokens a turn: her newest five");
    assert!(rendered <= 300, "within its grant: {rendered}");
}

// what this catches: Codex's 100 > 32 on a base whose estimate counted message text only (#5814
// dropped the per-message template constant and measured nothing in its place). His conversation
// (20 long old turns, 20 "ok" turns): text alone prices an "ok" turn at 0 tokens, so a start
// priced that way keeps all twenty, while the recorded engine counts 6 tokens for each "ok" user
// turn in a conversation (`template::recorded_qwen36`): 120 against a 32-token grant. Her allocation
// estimate carries the engine-measured template share of a user turn, so the start keeps within
// every grant, as estimated and as the recorded engine counts those turns.
#[test]
fn a_view_start_keeps_its_token_grant_with_the_measured_template_overhead() {
    use continuum_context::flex::{AllocationState, FlexAllocation, SourceAllocation};
    let measured = PromptCalibration::default().with_template(super::super::template::TemplateEstimate::from_measurement(&super::super::template::recorded_qwen36()));
    // The recorded engine's count of one "ok" user turn in a conversation (the fixture's sixth probe).
    let engine_ok_turn = 6;
    let conversation = Conversation {
        turns: (0..40)
            .map(|n| Turn { key: format!("density-{n}"), role: Role::User, line: if n < 20 { "long old message ".repeat(100) } else { "ok".to_owned() }, pointer: None })
            .collect(),
        ..Default::default()
    };
    let sources: Vec<&dyn Source<ContextMessage>> = vec![&conversation];
    // What the final prompt prices a start's messages at: each at its allocation estimate.
    let counted = |rendered: &[ChatMessage]| measured.charge(rendered.iter().map(|message| measured.estimate(message)).sum());
    // A start priced by `cost`, its plan's want that cost's own claim, `granted` tokens.
    let start = |cost: AllocationEstimate, granted: u32| -> (usize, usize) {
        let full = continuum_context::view::claims(&cost, &sources)[0].full;
        let plan = FlexAllocation {
            allocations: vec![SourceAllocation {
                source_id: format!("{VIEW_CLAIM}conversation"),
                allocated_tokens: granted,
                requested_floor: 0,
                requested_min: 0,
                requested_max: LlmDeliberationFaculty::clamp_u32(full),
                required: false,
                state: AllocationState::Satisfied,
            }],
            unallocated: 0,
            escalation_needed: false,
            warnings: Vec::new(),
        };
        let (view, _) = View::start(&cost, Vec::new(), &sources, |claims| start_grants(claims, &plan, granted as usize, None).expect("funded optional source"))
            .expect("matching source grants");
        let rendered: Vec<ChatMessage> = view.render().map(ContextMessage::to_chat).collect();
        (counted(&rendered), rendered.len())
    };
    let (_, kept) = start(AllocationEstimate::uncalibrated(), 32);
    assert!(kept * engine_ok_turn > 32, "precondition: priced by text alone the start keeps {kept} turns, {} engine tokens against 32", kept * engine_ok_turn);
    for granted in [32, 64, 128, 256, 512] {
        let (estimated, kept) = start(AllocationEstimate(measured), granted);
        assert!(kept >= 1, "her newest turn is kept whole");
        assert!(estimated <= granted as usize, "start estimated at {estimated} tokens in {kept} turns against grant {granted}");
        assert!(kept * engine_ok_turn <= estimated, "the estimate errs high: {kept} turns the engine counts at {}", kept * engine_ok_turn);
    }
}

// what this catches: two prices for one message. Her view's start priced each block by her
// allocation estimate (text plus the measured template share) while the final fit and a refusal's
// refit priced text alone, so the fit read a start that kept its grant as under it, and the refit
// reduced against a different number than the start kept within. All three price through
// `PromptCalibration::estimate` now: a multi-role sequence (a user turn, a native call, its result,
// an assistant turn, a CJK user turn) costs the same to the view's start and to the final fit.
#[test]
fn the_final_fit_and_a_view_start_price_a_multi_role_sequence_identically() {
    use continuum_context::cost::Estimate;
    let pricing = PromptCalibration::default().with_template(super::super::template::TemplateEstimate::from_measurement(&super::super::template::recorded_qwen36()));
    let call = ChatMessage {
        role: "assistant".into(),
        content: crate::ai::types::MessageContent::Parts(vec![crate::ai::types::ContentPart::ToolUse { id: "c1".into(), name: "code_read".into(), input: serde_json::json!({ "path": "main.rs" }) }]),
        name: None,
    };
    let sequence = vec![
        ChatMessage::text("user", "Read main.rs please."),
        call,
        ChatMessage::tool_result("c1", "fn main() {\n    println!(\"hi\");\n}\n", false),
        ChatMessage::text("assistant", "It prints hi."),
        ChatMessage::text("user", "了解しました。次は？"),
    ];
    let view_price: usize = sequence
        .iter()
        .map(|message| AllocationEstimate(pricing).block(&ContextMessage::from_chat(message).expect("test: a message her view holds")))
        .sum();
    let faculty = LlmDeliberationFaculty::new(uuid::Uuid::new_v4(), "T", "You are T.", std::sync::Arc::new(crate::ai::heuristic_adapter::HeuristicInferenceAdapter::new()));
    let prompt = PromptMessages {
        streams: Default::default(),
        result_pointers: Vec::new(),
        input_identity: [0; 32],
        grounding_at: 0,
        history: sequence.clone(),
        stimulus: None,
        latest_result: Vec::new(),
        action_statement: None,
        pricing,
    };
    let fitted = faculty.fit_messages(prompt, view_price).expect("test: a budget of the sequence's own price holds it");
    assert_eq!(fitted.receipt.history_tokens, view_price, "the final fit prices the sequence as her view's start does");
    assert_eq!(fitted.receipt.dropped_messages, 0, "so a budget of the view's price keeps all of it");
    assert!(view_price > PromptCalibration::default().estimate_all(&sequence), "precondition: the measured template adds to text alone");
}
