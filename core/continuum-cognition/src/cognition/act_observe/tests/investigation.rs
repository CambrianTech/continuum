#![cfg(test)]

use super::*;
use crate::cognition::workspace::Decision;
use crate::cognition::workspace::Situation;
use crate::cognition::workspace::TurnFraming;
use crate::cognition::workspace::WorkspaceCycle;
use crate::cognition::workspace::Contribution;
use crate::cognition::workspace::Faculty;
use crate::cognition::workspace::FacultyId;
use crate::cognition::workspace::SalienceArbiter;
use crate::cognition::workspace::Workspace;
use crate::ai::types::ToolCall;
use uuid::Uuid;
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::Mutex;
use crate::cognition::working_memory::WorkingMemory;

/// A deliberation faculty that is a small *investigator*: its next move is a
/// pure function of what it has DISCOVERED so far (the observations folded
/// into perception), never a counter and never a magic flag. It needs two
/// facts to answer — where the program starts and what that entry calls —
/// so it acts to learn the first, acts again to learn the second, then
/// (seeing both in memory) synthesizes and speaks. The branch reads
/// accumulated MEMORY CONTENT, which is the whole point: the hands change
/// the mind. (A test stand-in for the model, exactly like `ActThenSpeak`;
/// it proves the cycle's cross-tick plumbing, not production cognition.)
struct Investigator(Arc<WorkingMemory>);
#[async_trait]
impl Faculty for Investigator {
    fn id(&self) -> FacultyId {
        FacultyId::Deliberation
    }
    fn reacts_to_broadcast(&self) -> bool {
        true
    }
    async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
        // Reads what it has DISCOVERED from assembled perception — the
        // working-memory render of its own recent acts (proprioception),
        // where each act-observation head carries the executor's result tokens
        // (ENTRY=…/CALLS=…). Not the raw burst, not the deleted fold.
        let perceived = crate::cognition::act_observe::test_support::perceived_with(ws, Some(&self.0));
        let after = |key: &str| -> Option<String> {
            perceived
                .split(key)
                .nth(1)
                .and_then(|s| s.split_whitespace().next())
                .map(String::from)
        };
        match (after("ENTRY="), after("CALLS=")) {
            (None, _) => Some(Contribution::verdict(
                Decision::Act {
                    calls: vec![probe("find where the program starts")],
                    intent: "find where the program starts".into(),
                },
                0.9,
                "I don't know the entry point yet — reach for it",
            )),
            (Some(_), None) => Some(Contribution::verdict(
                Decision::Act {
                    calls: vec![probe("see what the entry calls")],
                    intent: "find what the entry calls".into(),
                },
                0.9,
                "I know the entry; now I need what it calls",
            )),
            (Some(entry), Some(calls)) => Some(Contribution::verdict(
                Decision::Speak {
                    text: format!("the program starts in {entry} and calls {calls}"),
                },
                0.95,
                "synthesized both discoveries from memory — settling",
            )),
        }
    }
}

/// A probe tool call whose INPUT carries no `ENTRY=`/`CALLS=` token (only the
/// executor's *result* does), so the investigator never false-triggers on
/// its own intent.
fn probe(what: &'static str) -> ToolCall {
    ToolCall {
        id: "probe-1".into(),
        name: "code/search".into(),
        input: serde_json::json!({ "query": what }),
    }
}

// what this catches: THE HANDS CHANGE THE MIND. A multi-act investigation
// converges through MEMORY CONTENT — each act discovers a distinct fact, the
// result re-enters as an Episodic engram, and the NEXT decision is a function
// of what the persona now KNOWS (not an act counter, not a `[you just acted]`
// flag). She acts to find the entry point, observes it, acts to find what it
// calls, observes that, then — seeing BOTH discoveries in her assembled
// perception — synthesizes and speaks. This is the organic loop
// cognition→action→perception→cognition converging by judgment, the novel
// architecture that distinguishes the organism from a textbook agentic
// counter loop ([[persona-codes-blind-no-hands-no-organic-loop]]).
#[tokio::test]
async fn the_hands_change_the_mind_across_a_multi_act_investigation() {
    let exec = Arc::new(ScriptedExecutor::new(["ENTRY=main", "CALLS=boot"]));
    let adm = admission();
    let wm = Arc::new(WorkingMemory::new(3));
    let cycle = WorkspaceCycle::new(
        vec![
            Arc::new(Investigator(Arc::clone(&wm))),
        ],
        Arc::new(SalienceArbiter),
    )
    .with_acting(body_with_wm(exec.clone(), adm.clone(), Arc::clone(&wm)));

    let outcome = drive_to_settle(
        &cycle,
        "[eval]\npeer: where does the program start and what does it call?",
        8,
        TurnFraming::ambient(),
    )
    .await;

    assert_eq!(
        outcome.acts, 2,
        "two DISCOVERIES were needed to converge — multi-act, not one-shot"
    );
    let spoken = outcome
        .spoken
        .expect("the mind settled into a spoken synthesis, not an un-driven act");
    assert!(
        spoken.contains("main"),
        "the answer carries the FIRST discovery (entry point) — proof act 1 entered memory"
    );
    assert!(
        spoken.contains("boot"),
        "the answer carries the SECOND discovery (what it calls) — proof act 2 entered memory"
    );
    assert_eq!(
        adm.engram_count(),
        2,
        "each discovery became a durable memory the mind perceived next tick"
    );

    // THE 18057 TYPED-THREAD ASSERTION (Step 6). The just-executed act's RESULT
    // re-enters through the TYPED field — read `active_act().output.result.content`,
    // never a re-parse of the `[action #n]` prose — and the call correlates to its
    // result BY ID (`call.id == result.tool_use_id`), never by positional index. This
    // is act-grained proof the tool result threads back into the mind: the exact
    // structural channel run-18057-f1 lost the grep result through when it flowed as an
    // evictable perception bid, yielding the 0-byte patch. (The message-builder pinned
    // assistant-tool-use↔tool-result pair — the render side of this same typed value —
    // is the remaining Step 6 piece; it changes every live prompt and is deferred to a
    // live-validated follow-up rather than shipped blind.)
    let active = wm
        .active_act()
        .expect("the settled mind still holds its last typed act");
    assert_eq!(
        active.call.id, active.output.result.tool_use_id,
        "the tool result correlates to its call BY ID, not a positional index"
    );
    assert!(
        active.output.result.content.contains("boot"),
        "the last act's RESULT threads back through the TYPED field (not [action #n] prose): {}",
        active.output.result.content
    );
    let typed_acts = wm.recent_acts();
    assert_eq!(
        typed_acts.len(),
        2,
        "both discoveries are typed acts in the window, read off the typed channel"
    );
    assert!(
        typed_acts
            .iter()
            .all(|a| a.call.id == a.output.result.tool_use_id),
        "every discovery's result is id-correlated to its call"
    );
}

// what this catches: a previous call receipt is not permission to suppress the
// next read, retry or differently scoped discovery. Exercise the actual act seam.
#[tokio::test]
async fn repeated_reads_and_scoped_discovery_reach_the_executor() {
    let exec = Arc::new(ScriptedExecutor::new([
        "old", "edited", "new", "missing", "available",
        "schema one", "schema two", "tree one", "tree two",
    ]).with_error_at(4));
    let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter))
        .with_acting(body(exec.clone(), admission()));
    let room = Uuid::new_v4();
    let chain = ActChain::new();
    for (i, (name, input, expected)) in [
        ("code/read", serde_json::json!({"file_path":"x"}), "old"),
        ("code/edit", serde_json::json!({"file_path":"x"}), "edited"),
        ("code/read", serde_json::json!({"file_path":"x"}), "new"),
        ("code/read", serde_json::json!({"file_path":"y"}), "missing"),
        ("code/read", serde_json::json!({"file_path":"y"}), "available"),
        ("commands/help", serde_json::json!({"name":"code/read"}), "schema one"),
        ("commands/help", serde_json::json!({"name":"code/edit"}), "schema two"),
        ("code/tree", serde_json::json!({"path":"a"}), "tree one"),
        ("code/tree", serde_json::json!({"path":"b"}), "tree two"),
    ].into_iter().enumerate() {
        let call = ToolCall { id: format!("call-{i}"), name: name.into(), input };
        let acts = acts_of(apply_act(&cycle, &[call], "inspect", room, &chain, None, None).await);
        assert!(matches!(acts[0].status, ActStatus::Executed));
        assert!(acts[0].output.result.content.contains(expected));
        assert_eq!(acts[0].output.result.is_error, (i == 3).then_some(true));
    }
    assert!(exec.results.lock().unwrap().is_empty());
}

// what this catches: identical relative requests in different rooms retain
// their current execution scope and call/result correlation, never reuse history.
#[tokio::test]
async fn repeated_request_keeps_current_room_and_returns_fresh_evidence() {
    let exec = Arc::new(ScriptedExecutor::new(["room-a content", "room-b content"]));
    let wm = Arc::new(WorkingMemory::new(3));
    let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter))
        .with_acting(body_with_wm(exec.clone(), admission(), wm.clone()));
    let rooms = [Uuid::new_v4(), Uuid::new_v4()];
    for (i, room) in rooms.into_iter().enumerate() {
        let id = format!("read-{i}");
        let call = ToolCall {
            id: id.clone(),
            name: "code/read".into(),
            input: serde_json::json!({"file_path": "README.md"}),
        };
        let acts = acts_of(apply_act(&cycle, &[call], "inspect", room, &ActChain::new(), None, None).await);
        assert!(matches!(acts[0].status, ActStatus::Executed));
        assert_eq!(acts[0].output.result.tool_use_id, id);
        assert!(acts[0].output.result.content.contains(if i == 0 {
            "room-a content"
        } else {
            "room-b content"
        }));
    }
    assert_eq!(*exec.rooms.lock().unwrap(), rooms.to_vec());
    let feedback = wm.recent().join("\n");
    assert!(!feedback.contains("already in my working memory"));
    assert!(!feedback.contains("result has not changed"));
    assert!(exec.results.lock().unwrap().is_empty());
}

// what this catches: SETTLE IS A REST, NOT A HALT — the metronome does not
// crank to a halt after one answer. The SAME mind (same cycle, same body,
// same accumulating memory) settles concern A, then RE-AWAKENS on a fresh
// concern B and runs the act→observe→speak arc again. Her concern-A memory
// persists across the two drives (continuity of self), and she still engages
// B. Proves the organism keeps breathing across concerns: a settle is the
// judgment "the work is done for now," never a terminus.
#[tokio::test]
async fn it_settles_then_re_awakens_without_cranking_to_a_halt() {
    // Distinct per-concern observations: identical content would be a
    // content-addressed dedup no-op in memory (correct substrate behavior,
    // [[embeddings-are-per-content-computed-once-shared]]), which would mask
    // the continuity-of-self assertion below.
    let exec = Arc::new(ScriptedExecutor::new([
        "learned about A",
        "learned about B",
    ]));
    let adm = admission();
    // One living mind: the working-memory buffer accumulates ACROSS both
    // concern-drives (volatile continuity), so `ActThenSpeak` must re-awaken on
    // concern B by perceiving a NEW act stamp rather than mistaking concern A's
    // still-buffered proprioception for "already answered".
    let wm = Arc::new(WorkingMemory::new(3));
    let cycle = WorkspaceCycle::new(
        vec![
            Arc::new(ActThenSpeak::new().with_memory(Arc::clone(&wm))),
        ],
        Arc::new(SalienceArbiter),
    )
    .with_acting(body_with_wm(exec.clone(), adm.clone(), Arc::clone(&wm)));

    // Concern A: act → observe → settle on a Speak.
    let a = drive_to_settle(
        &cycle,
        "[eval]\npeer: concern A?",
        8,
        TurnFraming::ambient(),
    )
    .await;
    assert_eq!(a.acts, 1, "settled concern A after one act→observe");
    assert!(a.spoken.is_some(), "concern A got a spoken answer");
    assert_eq!(adm.engram_count(), 1, "concern A left exactly one memory");

    // Concern B on the SAME living mind — it must wake again, not stay halted.
    let b = drive_to_settle(
        &cycle,
        "[eval]\npeer: a totally different concern B?",
        8,
        TurnFraming::ambient(),
    )
    .await;
    assert_eq!(
        b.acts, 1,
        "the mind RE-AWAKENED and acted on the new concern — not stuck post-settle"
    );
    assert!(b.spoken.is_some(), "and settled concern B too");
    assert_eq!(
        adm.engram_count(),
        2,
        "continuity of self: concern-A memory persisted, concern B added its own"
    );
}

/// Deliberation faculty that Speaks a fixed text — for exercising the Speak arm.
struct SpeaksText(&'static str);
#[async_trait]
impl Faculty for SpeaksText {
    fn id(&self) -> FacultyId {
        FacultyId::Deliberation
    }
    fn reacts_to_broadcast(&self) -> bool {
        true
    }
    async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
        Some(Contribution::verdict(
            Decision::Speak {
                text: self.0.into(),
            },
            0.9,
            "speaks",
        ))
    }
}

// what this catches: the unfulfilled-promise backstop (#122, glass-boxed live
// 2026-07-09). A Speak that NARRATES action (first-person intent + fence) which
// no format lifted must leave an [unfulfilled] proprioception line in working
// memory — next tick she perceives her own unkept promise instead of believing
// the work happened. A plain prose Speak must leave no such line.
#[tokio::test]
async fn spoken_narrated_action_records_unfulfilled_promise() {
    let exec = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "ok".into(),
    });
    let promise = "I'll run this script to check:\n```python\nprint(2+2)\n```\nOutput soon!";
    let wm = Arc::new(WorkingMemory::new(4));
    let cycle = WorkspaceCycle::new(
        vec![Arc::new(SpeaksText(promise)) as Arc<dyn Faculty>],
        Arc::new(SalienceArbiter),
    )
    .with_acting(body_with_wm(exec.clone(), admission(), Arc::clone(&wm)));
    let (step, _, _) = settle_step(
        &cycle,
        "[eval]\npeer: can you check 2+2?",
        true,
        TurnFraming::ambient(),
        Situation::FreshContext,
        &ActChain::new(),
    )
    .await;
    assert!(matches!(step, SettleStep::Spoke(_)));
    assert!(
        wm.recent().iter().any(|l| l.contains("[unfulfilled]")),
        "narrated-but-unexecuted promise must enter proprioception: {:?}",
        wm.recent()
    );

    let wm2 = Arc::new(WorkingMemory::new(4));
    let cycle2 = WorkspaceCycle::new(
        vec![Arc::new(SpeaksText("the answer is 4, plainly.")) as Arc<dyn Faculty>],
        Arc::new(SalienceArbiter),
    )
    .with_acting(body_with_wm(exec, admission(), Arc::clone(&wm2)));
    let (step2, _, _) = settle_step(
        &cycle2,
        "[eval]\npeer: can you check 2+2?",
        true,
        TurnFraming::ambient(),
        Situation::FreshContext,
        &ActChain::new(),
    )
    .await;
    assert!(matches!(step2, SettleStep::Spoke(_)));
    assert!(
        !wm2.recent().iter().any(|l| l.contains("[unfulfilled]")),
        "plain prose must never trip the promise backstop"
    );
}

// what this catches (2026-09-17, #academy): a settled utterance that near-duplicates
// a PEER's line from the burst was recorded as an [echo] fact and STILL POSTED —
// 0d5c1ffa re-spoke cf6b4df6's whole line verbatim minutes apart, and the Intel
// tier's 64 "draft_peer_echo" rows in an hour refused nothing. The copy is a gate
// pass now (peer_echo / near_identical), the fact stays in her memory, and a
// substantial line of her own still speaks.
#[tokio::test]
async fn a_near_identical_copy_of_a_peers_line_is_a_pass_not_a_post() {
    use crate::cognition::workspace::{Burst, BurstTurn};
    let peer_line = "I'm not sure what you're trying to do here, but I can help you with any development question or challenge you might have today.";
    let room = crate::identity::ActivityRoom::from_uuid(Uuid::new_v4()).unwrap();
    let burst = || {
        Burst::from_turns(
            room,
            vec![BurstTurn::attributed(false, "cf6b4df6", peer_line, None)],
        )
    };
    let exec = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "ok".into(),
    });
    let wm = Arc::new(WorkingMemory::new(4));
    let cycle = WorkspaceCycle::new(
        vec![Arc::new(SpeaksText(peer_line)) as Arc<dyn Faculty>],
        Arc::new(SalienceArbiter),
    )
    .with_acting(body_with_wm(exec.clone(), admission(), Arc::clone(&wm)));
    let (step, _, _) = settle_step(&cycle, burst(), true, TurnFraming::ambient(), Situation::FreshContext, &ActChain::new()).await;
    match step {
        SettleStep::Passed { reason: Some(r) } => assert!(r.contains("peer_echo:near_identical"), "{r}"),
        other => panic!("a verbatim copy of a peer's line must not post: {other:?}"),
    }
    assert!(wm.recent().iter().any(|l| l.contains("[echo]")), "{:?}", wm.recent());

    let wm2 = Arc::new(WorkingMemory::new(4));
    let own = "The failing test is test_datetime_mean; the pivot's tzinfo is naive while the inputs are aware, so align it before subtracting.";
    let cycle2 = WorkspaceCycle::new(
        vec![Arc::new(SpeaksText(own)) as Arc<dyn Faculty>],
        Arc::new(SalienceArbiter),
    )
    .with_acting(body_with_wm(exec, admission(), Arc::clone(&wm2)));
    let (step2, _, _) = settle_step(&cycle2, burst(), true, TurnFraming::ambient(), Situation::FreshContext, &ActChain::new()).await;
    assert!(matches!(step2, SettleStep::Spoke(_)), "her own substantial line speaks: {step2:?}");
}

// what this catches: the CONFABULATION backstop (Joel 2026-07-11) — under a
// peer's verification pressure Atlas upgraded from stage directions to
// plausible fenced FILE CONTENTS no tool ever produced. A fenced Speak in a
// turn with zero acts, spoken while working memory already carries an
// outstanding [unfulfilled] promise, must record the [unverified] fact.
// Evidence-gated: the SAME fenced content with a clean memory (legitimate
// drafting — Asha sharing code) must record nothing.
#[tokio::test]
async fn fenced_content_over_unkept_promises_records_unverified_artifact() {
    // Atlas's live shape: the confabulated test-file contents.
    let confabulated = "1. **Simple Text File**: Contains a single line of text.\n\
                        ```\nThis is a simple text file for testing purposes.\n```";

    // With an outstanding promise in memory → [unverified].
    let exec = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "ok".into(),
    });
    let wm = Arc::new(WorkingMemory::new(4));
    wm.record_receipt(
        "[unfulfilled] I wrote a stage direction like [doing the task], \
         but a stage direction is words only — no tool ran, no file exists.",
    );
    let cycle = WorkspaceCycle::new(
        vec![Arc::new(SpeaksText(confabulated)) as Arc<dyn Faculty>],
        Arc::new(SalienceArbiter),
    )
    .with_acting(body_with_wm(exec.clone(), admission(), Arc::clone(&wm)));
    let (step, _, _) = settle_step(
        &cycle,
        "[eval]\npeer: please provide the content of the test files",
        true,
        TurnFraming::ambient(),
        Situation::FreshContext,
        &ActChain::new(),
    )
    .await;
    assert!(matches!(step, SettleStep::Spoke(_)));
    assert!(
        wm.recent().iter().any(|l| l.contains("[unverified]")),
        "fenced 'artifacts' over an unkept promise are composition, not \
         workspace truth: {:?}",
        wm.recent()
    );

    // Clean memory, same fenced content → legitimate drafting, no line.
    let exec2 = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "ok".into(),
    });
    let wm2 = Arc::new(WorkingMemory::new(4));
    let cycle2 = WorkspaceCycle::new(
        vec![Arc::new(SpeaksText(confabulated)) as Arc<dyn Faculty>],
        Arc::new(SalienceArbiter),
    )
    .with_acting(body_with_wm(exec2, admission(), Arc::clone(&wm2)));
    let (step2, _, _) = settle_step(
        &cycle2,
        "[eval]\npeer: could you draft example test data?",
        true,
        TurnFraming::ambient(),
        Situation::FreshContext,
        &ActChain::new(),
    )
    .await;
    assert!(matches!(step2, SettleStep::Spoke(_)));
    assert!(
        !wm2.recent().iter().any(|l| l.contains("[unverified]")),
        "drafting with a clean conscience is never taxed: {:?}",
        wm2.recent()
    );
}
