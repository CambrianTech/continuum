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

