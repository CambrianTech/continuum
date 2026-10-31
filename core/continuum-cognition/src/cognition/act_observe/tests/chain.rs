#![cfg(test)]

use super::*;
use crate::cognition::workspace::WorkspaceCycle;
use crate::cognition::workspace::SalienceArbiter;
use crate::ai::types::ToolCall;
use uuid::Uuid;
use std::sync::Arc;
use std::sync::Mutex;

// what this catches: the causal spine's first production wiring (task #447,
// CAUSAL-MEMORY-GRAPH.md slice A). Two acts driven through ONE chain must
// leave a `CausedBy` edge from the second act's engram to the first's —
// recorded at the write site, queryable through the admission store's graph.
// A fresh chain (a new turn) must NOT link back to the previous turn's acts.
// Regression guard for the zero-production-callers state EngramGraph shipped
// in: this pins that acts now populate the graph.
#[tokio::test]
async fn a_settle_chain_links_each_act_caused_by_its_predecessor() {
    let exec = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "ok\n".into(),
    });
    let adm = admission();
    let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter))
        .with_acting(body(exec.clone(), adm.clone()));
    let room = Uuid::new_v4();
    let chain = ActChain::new();

    // Act 1 — starts the chain: no predecessor, so no edge.
    acts_of(apply_act(&cycle, &[tool_call()], "start", room, &chain, None, None).await);
    let first = chain.prior().expect("first act admitted onto the chain");
    assert!(
        adm.engram_neighbors(&first).is_empty(),
        "the chain's first act has no predecessor to point at"
    );

    // Act 2 — a DIFFERENT call (the repeat guard short-circuits identical
    // batches; a chain is made of distinct steps).
    let second_call = ToolCall {
        id: "call-2".into(),
        name: "code/read".into(),
        input: serde_json::json!({ "file_path": "src/main.rs" }),
    };
    acts_of(apply_act(&cycle, &[second_call], "inspect", room, &chain, None, None).await);
    let second = chain.prior().expect("second act admitted onto the chain");
    assert_ne!(first, second, "two acts, two engrams");

    let edges = adm.engram_neighbors(&second);
    assert!(
        edges
            .iter()
            .any(|e| e.target == first
                && e.kind == crate::cognition::engram_graph::EdgeKind::CausedBy),
        "the second act must carry a CausedBy edge to its predecessor — \
         the `because` clause as structure, not prose; got {edges:?}"
    );

    // A NEW chain (next turn) must not reach back across the boundary.
    let next_turn = ActChain::new();
    assert!(
        next_turn.prior().is_none(),
        "a fresh chain starts empty — edges can never cross turns by construction"
    );
}

// what this catches: THE gap that made "which acts were done for this card"
// unanswerable. The chain used to start at `None`, so the FIRST act of every turn
// carried no CausedBy edge — and since a turn is triggered by a message or a
// work-card kickoff, that meant NO PATH IN THE GRAPH from a card to the work done
// for it. The card and its acts were causally disconnected, so every query showed
// "claimed, did nothing" no matter how much she actually did.
//
// Rooting the chain fixes it through the SAME write site — no new branch, no second
// rule — which is why this test asserts on the first act specifically.
#[tokio::test]
async fn a_chain_rooted_in_its_trigger_links_the_first_act_to_what_caused_the_turn() {
    let exec = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "ok\n".into(),
    });
    let adm = admission();
    let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter))
        .with_acting(body(exec.clone(), adm.clone()));
    let room = Uuid::new_v4();

    // The kickoff / inbound message that caused this turn to happen at all.
    let trigger = Uuid::new_v4();
    let chain = ActChain::rooted_in(&crate::cognition::workspace::Cause::Stimulus(trigger));

    acts_of(apply_act(&cycle, &[tool_call()], "start", room, &chain, None, None).await);
    let first = chain.prior().expect("first act admitted onto the chain");
    assert_ne!(first, trigger, "the act is its own engram, not the trigger");

    let edges = adm.engram_neighbors(&first);
    assert!(
        edges.iter().any(|e| e.target == trigger
            && e.kind == crate::cognition::engram_graph::EdgeKind::CausedBy),
        "the FIRST act must chain to the trigger that caused the turn — without \
         this edge there is no path from a work card to the acts done for it; \
         got {edges:?}"
    );
}

// what this catches: an unrooted chain silently gaining a phantom antecedent. A
// burst with no admitted trigger — an idle tick (`Ambient`), an eval fixture
// (`Synthetic`) — must produce a first act with NO edge rather than one pointing
// at something invented. Honest absence over a fabricated link, which is also what
// makes the `engram.chain.rooted` probe's ambient rows mean something.
#[tokio::test]
async fn an_unrooted_chain_leaves_its_first_act_honestly_unlinked() {
    let exec = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "ok\n".into(),
    });
    let adm = admission();
    let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter))
        .with_acting(body(exec.clone(), adm.clone()));
    let chain = ActChain::rooted_in(&crate::cognition::workspace::Cause::Ambient);

    acts_of(apply_act(&cycle, &[tool_call()], "start", Uuid::new_v4(), &chain, None, None).await);
    let first = chain.prior().expect("first act admitted");
    assert!(
        adm.engram_neighbors(&first).is_empty(),
        "no trigger means no edge — never a fabricated one"
    );
}
