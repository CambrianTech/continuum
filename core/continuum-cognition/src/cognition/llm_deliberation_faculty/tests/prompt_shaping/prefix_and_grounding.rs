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
    // (tools + identity) is reusable KV across the flip.
    assert_eq!(
        directed.system, undirected.system,
        "the system message is the stable half — a presence flip must not move a byte of it"
    );
    let tail_has = |v: &DeliberationPromptView, needle: &str| {
        v.messages.iter().any(
            |m| matches!(&m.content, crate::ai::types::MessageContent::Text(t) if t.contains(needle)),
        )
    };
    // The grounding is the room's: it rides after her view, never the system message
    // (2026-10-23), and the presence flip still cannot reach it.
    assert!(
        !directed.system.contains("ROSTER: alice") && tail_has(&directed, "ROSTER: alice"),
        "grounding renders in the messages, not the system prefix"
    );
    // The presence variants ride the conversation tail (facts phase,
    // before the ask) — directed carries its marker, undirected does not.
    assert!(
        tail_has(&directed, "This message names you")
            && !tail_has(&undirected, "This message names you"),
        "the directed presence variant renders as a conversation turn, never in the system prefix"
    );
}


// what this catches: a room's grounding back in the system message. One mind acts in two
// rooms; on the M5 (2026-10-23) her KV pages were keyed per room because the system message
// closed with the room's assembled context, so a room switch rewrote the start of her
// prompt and her warm page could not serve the other room. Her system message must be
// byte-identical across rooms, the second room's prompt must open with everything her
// first act's view held (the engine reuses it whole), and each room's grounding must still
// reach her when she acts there: its doctrine as a block of her view, a context no source
// marked as a view block in the tail after it.
#[test]
fn two_rooms_share_her_head_and_each_room_grounds_her_when_she_acts_there() -> Result<(), String> {
    use crate::cognition::workspace::Burst;
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter)
        // Ample window: no turnover, no history dropped; this isolates placement.
        .with_context_window(16_384);
    let turns = vec![
        BurstTurn::attributed(false, "Operator", "we shipped the fix yesterday", Some(1_700_000_000_000)),
        BurstTurn::attributed(true, "Ivar", "I saw it land.", Some(1_700_000_060_000)),
        BurstTurn::attributed(false, "Operator", "what's next on the board?", Some(1_700_000_120_000)),
    ];
    let act_in = |room_uuid: Uuid, turns: Vec<BurstTurn>, doctrine: &str, roster: &str| -> Result<DeliberationPromptView, String> {
        let room = crate::identity::ActivityRoom::from_uuid(room_uuid).map_err(|_| "a non-nil room".to_string())?;
        let mut ws = Workspace::new(Burst::from_turns(room, turns));
        ws.now_ms = Some(1_700_000_180_000);
        // Marked exactly as `RagSourceFaculty::contribute` marks a standing source's bid.
        ws.broadcast.push(
            Contribution::context(FacultyId::Custom("room-doctrine".to_string()), doctrine, 0.9, "framing")
                .with_parts(vec![doctrine.to_string()])
                .standing_grounding()
                .session_stable()
                .trailing(),
        );
        // A context no producer marked as a view block: it must still stay out of the head.
        ws.broadcast.push(
            Contribution::context(FacultyId::Custom("room-roster".to_string()), roster, 0.9, "framing")
                .session_stable(),
        );
        Ok(faculty.prompt_view(&ws))
    };
    let a = act_in(Uuid::new_v4(), turns.clone(), "DOCTRINE-A: review before merge", "ROSTER-A: alice")?;
    let mut later = turns;
    later.push(BurstTurn::attributed(false, "Operator", "and in this room?", Some(1_700_000_240_000)));
    let b = act_in(Uuid::new_v4(), later, "DOCTRINE-B: ship the demo", "ROSTER-B: bob")?;

    // Her head is hers: identical across the two rooms, and no room's bytes in it.
    assert_eq!(a.system, b.system, "the system message is a function of the persona alone");
    for needle in ["DOCTRINE-A", "DOCTRINE-B", "ROSTER-A", "ROSTER-B", "[What you are working with"] {
        assert!(!a.system.contains(needle), "`{needle}` in the system message:\n{}", a.system);
    }
    // Her view leads both prompts, and the second room's opens with all of the first's.
    let texts = |v: &DeliberationPromptView| v.messages.iter().map(|m| m.content_text()).collect::<Vec<_>>();
    let (ta, tb) = (texts(&a), texts(&b));
    let view_a = a.refit.grounding_at;
    assert!(view_a > 0, "her view leads the messages:\n{ta:#?}");
    assert_eq!(tb.get(..view_a), ta.get(..view_a), "room B's prompt must open with room A's whole view");

    // Each room still grounds her where she acts: its doctrine is the newest version of the
    // block in her view, and its unmarked context rides the tail after the view.
    let newest_doctrine = |t: &[String]| t.iter().rev().find(|m| m.starts_with("[room-doctrine]")).cloned();
    assert!(newest_doctrine(&ta).is_some_and(|m| m.contains("DOCTRINE-A")), "{ta:#?}");
    assert!(newest_doctrine(&tb).is_some_and(|m| m.contains("DOCTRINE-B")), "{tb:#?}");
    let tail_holds = |t: &[String], needle: &str| t[view_a.min(t.len())..].iter().any(|m| m.contains(needle));
    assert!(tail_holds(&ta, "ROSTER-A"), "room A's context reaches her in room A:\n{ta:#?}");
    assert!(tail_holds(&tb, "ROSTER-B") && !tail_holds(&tb, "ROSTER-A"), "room B's context, not A's:\n{tb:#?}");
    Ok(())
}
