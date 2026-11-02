#![cfg(test)]

use super::*;
use crate::ai::types::ToolCall;
use crate::cognition::working_memory::WorkingMemory;
use crate::cognition::workspace::Contribution;
use crate::cognition::workspace::Decision;
use crate::cognition::workspace::Faculty;
use crate::cognition::workspace::FacultyId;
use crate::cognition::workspace::SalienceArbiter;
use crate::cognition::workspace::Situation;
use crate::cognition::workspace::TurnFraming;
use crate::cognition::workspace::Workspace;
use crate::cognition::workspace::WorkspaceCycle;
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::Mutex;
use uuid::Uuid;

/// what this catches: the no-deliverable nudge going back to ONE-SHOT. The first version
/// latched on a bool, and the probe trail proved the cost — `persona.settle.no_deliverable`
/// fired exactly once per SWE run and the persona then settled at 5-7 acts with a 30-act
/// budget unspent. The nudge must re-arm each time she ACTS, so a turn that keeps working
/// keeps being told the workspace is the deliverable; and it must NOT re-arm when she
/// speaks twice with no act between, so it can never become a spin.
#[test]
fn the_no_deliverable_nudge_rearms_on_each_act_but_never_twice_without_one() {
    // The gate's whole condition, isolated: `acts_at_last_nudge != Some(acts)`.
    let fires = |last: Option<usize>, acts: usize| last != Some(acts);

    // Never nudged yet at act 3 → fires.
    assert!(fires(None, 3), "first zero-deliverable Speak must nudge");
    // Nudged at 3, still at 3 (spoke again, acted zero times) → must NOT fire again.
    assert!(
        !fires(Some(3), 3),
        "a second Speak with no act in between must settle, not spin"
    );
    // Nudged at 3, she then acted (now 4) → re-arms.
    assert!(
        fires(Some(3), 4),
        "the nudge must re-arm once she has acted again — this is the bug that capped \
         her at one reminder per turn"
    );
}


// what this catches: the act→observe MOTION — the driver runs the act, folds
// the observation into the next perception, and the mind settles into a Speak
// that the external observer reads. acts==1, spoken is the settled answer.
#[tokio::test]
async fn drive_to_settle_acts_then_settles_on_speak() {
    let exec = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "4".into(),
    });
    let adm = admission();
    // Same buffer in the body (writer) and the perception-tier faculty
    // (reader): act → working memory → next-tick perception.
    let wm = Arc::new(WorkingMemory::new(3));
    let cycle = WorkspaceCycle::new(
        vec![
            Arc::new(ActThenSpeak::new().with_memory(Arc::clone(&wm))),
        ],
        Arc::new(SalienceArbiter),
    )
    .with_acting(body_with_wm(exec.clone(), adm.clone(), Arc::clone(&wm)));

    let outcome = drive_to_settle(
        &cycle,
        "[eval]\npeer: what is 2+2?",
        8,
        TurnFraming::ambient(),
    )
    .await;

    assert_eq!(outcome.acts, 1, "acted exactly once before settling");
    assert_eq!(outcome.spoken.as_deref(), Some("the answer is 4"));
    assert!(matches!(outcome.decision, Decision::Speak { .. }));
}

// Refusal after a real act retains its observed work and cannot replay input.
#[tokio::test]
async fn acted_then_capacity_refused_retains_work_without_replay() {
    struct ActThenRefuse(Arc<WorkingMemory>);
    #[async_trait]
    impl Faculty for ActThenRefuse {
        fn id(&self) -> FacultyId {
            FacultyId::Deliberation
        }
        fn reacts_to_broadcast(&self) -> bool {
            true
        }
        async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
            if latest_action_seq(&crate::cognition::act_observe::test_support::perceived_with(ws, Some(&self.0))) > 0 {
                Some(Contribution::prompt_capacity_refused(
                    crate::cognition::workspace::PromptCapacityError {
                        required_tokens: 8192,
                        budget_tokens: 4096,
                    },
                ))
            } else {
                let mut contribution = AlwaysAct.contribute(ws).await?;
                contribution.metrics = Some(crate::cognition::workspace::TurnMetrics {
                    input_tokens: 700,
                    output_tokens: 40,
                    latency_ms: 1200,
                    ..Default::default()
                });
                Some(contribution)
            }
        }
    }
    let wm = Arc::new(WorkingMemory::new(3));
    let exec = Arc::new(ScriptedExecutor::new(["real observed result"]));
    let cycle = WorkspaceCycle::new(
        vec![
            Arc::new(ActThenRefuse(Arc::clone(&wm))),
        ],
        Arc::new(SalienceArbiter),
    )
    .with_acting(body_with_wm(exec, admission(), wm));
    let outcome = drive_to_settle(&cycle, "perform the task", 8, TurnFraming::ambient()).await;
    assert_eq!(outcome.acts, 1);
    assert_eq!(outcome.turn_acts.len(), 1);
    assert_eq!(outcome.metrics.input_tokens, 700);
    assert_eq!(outcome.metrics.output_tokens, 40);
    assert_eq!(outcome.metrics.latency_ms, 1200);
    assert!(!outcome.deferred_before_dispatch);
    assert!(outcome.failure.is_none());
    assert!(matches!(
        SettleStep::from_settled(outcome).0,
        SettleStep::CapacityRefused {
            replay_input: false,
            ..
        }
    ));
}

// Capacity refusal is not a zero-generation lane wait. Exercise the actual
// full driver, including its retry loop, rather than only projecting a result.
#[tokio::test]
async fn capacity_refusal_leaves_the_driver_once_with_typed_custody() {
    struct Refusing(std::sync::atomic::AtomicUsize);
    #[async_trait]
    impl Faculty for Refusing {
        fn id(&self) -> FacultyId {
            FacultyId::Deliberation
        }
        fn reacts_to_broadcast(&self) -> bool {
            true
        }
        async fn contribute(&self, _: &Workspace) -> Option<Contribution> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Some(Contribution::prompt_capacity_refused(
                crate::cognition::workspace::PromptCapacityError {
                    required_tokens: 8192,
                    budget_tokens: 4096,
                },
            ))
        }
    }
    let faculty = Arc::new(Refusing(std::sync::atomic::AtomicUsize::new(0)));
    let cycle = WorkspaceCycle::new(vec![faculty.clone()], Arc::new(SalienceArbiter));
    let outcome =
        drive_to_settle(&cycle, "keep the complete input", 8, TurnFraming::ambient()).await;
    assert_eq!(faculty.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(outcome.failure.is_none());
    assert!(outcome.deferred_before_dispatch);
    assert_eq!(outcome.acts, 0);
    assert!(outcome.generation_receipts.is_empty());
    assert!(matches!(
        SettleStep::from_settled(outcome).0,
        SettleStep::CapacityRefused {
            replay_input: true,
            ..
        }
    ));
}

// what this catches: THE SETTLE ARTERY (the dominant SWE-bench killer, glass-boxed
// 2026-08-04 on sympy-21379: one `code/tree`, then a prose explanation of the bug —
// 0 patch bytes, 29 of 30 acts unspent, run over). When the CALLER declared the
// deliverable to be the workspace, a Speak that changed no file must not end the
// turn: she gets up to NARRATION_BUDGET more perceptions, each carrying the
// structural fact that her working memory holds no mutation (2026-09-05: one was
// too few — a plan, one act, a second plan ended every work turn). Bounded — the
// budget spends and it settles, so a determined Speak is never trapped in a loop.
#[tokio::test]
async fn a_zero_change_speak_reperceives_up_to_the_narration_budget_when_the_workspace_is_the_deliverable(
) {
    let speaker = CountingSpeaker::new();
    let wm = Arc::new(WorkingMemory::new(8));
    let exec = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "src/".into(),
    });
    let cycle = WorkspaceCycle::new(
        vec![
            Arc::clone(&speaker) as Arc<dyn Faculty>,
        ],
        Arc::new(SalienceArbiter),
    )
    .with_acting(body_with_wm(exec, admission(), Arc::clone(&wm)));

    let outcome = drive_to_settle(
        &cycle,
        "fix the bug in sympy/core/basic.py",
        8,
        TurnFraming::directed().on_workspace(),
    )
    .await;

    assert_eq!(
        speaker.generations(),
        super::super::settle::NARRATION_BUDGET + 1,
        "the zero-deliverable Speaks bought exactly the narration budget of perceptions — not zero, not a loop"
    );
    assert!(
        wm.recent().iter().any(|l| l.contains("[no-deliverable]")),
        "the structural fact reached working memory, where the next tick perceives it: {:?}",
        wm.recent()
    );
    assert!(
        matches!(outcome.decision, Decision::Speak { .. }),
        "she settles on the Speak after the budget — the decision stays hers"
    );
}

// what this catches: the blast radius. An ORDINARY turn (chat, an answer-graded
// task — the default `Deliverable::Answer`) is untouched: her first Speak settles
// it, exactly as before, and no [no-deliverable] fact is invented for a turn whose
// deliverable IS the utterance. The re-perception is opt-in by the caller that
// grades a diff, never a global change to how speech settles.
#[tokio::test]
async fn an_ordinary_turn_still_settles_on_the_first_speak() {
    let speaker = CountingSpeaker::new();
    let wm = Arc::new(WorkingMemory::new(8));
    let exec = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "ok".into(),
    });
    let cycle = WorkspaceCycle::new(
        vec![
            Arc::clone(&speaker) as Arc<dyn Faculty>,
        ],
        Arc::new(SalienceArbiter),
    )
    .with_acting(body_with_wm(exec, admission(), Arc::clone(&wm)));

    let outcome =
        drive_to_settle(&cycle, "what do you think?", 8, TurnFraming::directed()).await;

    assert_eq!(
        speaker.generations(),
        1,
        "one generation, settled — unchanged"
    );
    assert!(
        !wm.recent().iter().any(|l| l.contains("[no-deliverable]")),
        "no workspace-deliverable fact on a turn whose deliverable is the answer"
    );
    assert!(matches!(outcome.decision, Decision::Speak { .. }));
}

/// Emits a DIFFERENT read-class act every tick (fresh path per generation), so
/// every batch has a fresh loop signature — the exact shape the #206 identical-act
/// backstop can never bound. The instrument for the #390 discovery-saturation gate.
struct VaryingAct {
    ticks: Mutex<usize>,
}
#[async_trait]
impl Faculty for VaryingAct {
    fn id(&self) -> FacultyId {
        FacultyId::Deliberation
    }
    fn reacts_to_broadcast(&self) -> bool {
        true
    }
    async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
        let n = {
            let mut t = self.ticks.lock().unwrap();
            *t += 1;
            *t
        };
        Some(Contribution::verdict(
            Decision::Act {
                calls: vec![ToolCall {
                    id: format!("call-{n}"),
                    name: "code/read".into(),
                    input: serde_json::json!({ "path": format!("src/file_{n}.rs") }),
                }],
                intent: "keep reading".into(),
            },
            0.9,
            "reads forever, never writes",
        ))
    }
}

// what this catches (#390 discovery-saturation gate): a workspace-deliverable turn
// spending VARIED read acts — each with a fresh signature, so the #206 identical-act
// backstop never trips — must be stopped at HALF the act budget while nothing has
// mutated the workspace, so the remaining half survives for the empty-diff re-drive
// instead of being read away (glass-boxed live: 15 varied reads on the right file,
// patch_bytes 0, [no-deliverable] fact firing every act and changing nothing). The
// ambient control pins the scope: a turn whose deliverable is the UTTERANCE reads
// freely to the full budget — the gate exists only where the diff is the deliverable.
#[tokio::test]
async fn drive_to_settle_gates_discovery_at_half_budget_on_workspace_deliverable() {
    let exec = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "file contents...".into(),
    });
    let adm = admission();
    let cycle = WorkspaceCycle::new(
        vec![Arc::new(VaryingAct {
            ticks: Mutex::new(0),
        })],
        Arc::new(SalienceArbiter),
    )
    .with_acting(body(exec.clone(), adm.clone()));

    let outcome = drive_to_settle(
        &cycle,
        "fix the bug",
        20,
        TurnFraming::ambient().on_workspace(),
    )
    .await;

    assert_eq!(
        outcome.acts, 15,
        "discovery saturates at 3/4 of the budget (20*3/4) with zero mutations \
         — PREMISE CHANGED 2026-08-24: the /2 gate measured live as forcing \
         early settles on honest study phases (she 'quit' at 16/32 because \
         the gate quit for her); 3/4 still bounds a pure-read runaway"
    );
    assert!(
        matches!(outcome.decision, Decision::Act { .. }) && outcome.spoken.is_none(),
        "the never-write faculty returns un-driven — honest 'saturated, did not settle'"
    );

    // Ambient control: same never-writing faculty, deliverable is the utterance —
    // discovery is not gated and the full budget is hers.
    let exec2 = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "file contents...".into(),
    });
    let cycle2 = WorkspaceCycle::new(
        vec![Arc::new(VaryingAct {
            ticks: Mutex::new(0),
        })],
        Arc::new(SalienceArbiter),
    )
    .with_acting(body(exec2, admission()));
    let ambient = drive_to_settle(&cycle2, "look around", 20, TurnFraming::ambient()).await;
    assert_eq!(
        ambient.acts, 20,
        "a non-workspace turn reads to the full budget — the gate scopes to \
         workspace-deliverable turns only"
    );
}

// what this catches: the act-budget stopwatch actually reaching her MIND —
// regression for the 2026-08-23 framing gap where the facts were written
// (#2411) but eval framed every task as directed-speech, so they never
// fired for the DoD tasks they were built for. Pins: on a workspace-
// deliverable turn the [act-budget] contract fact lands in working memory;
// on an ordinary turn it does not (speech turns owe no stopwatch).
#[tokio::test]
async fn act_budget_facts_reach_working_memory_on_workspace_turns_only() {
    let exec = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "file contents...".into(),
    });
    let adm = admission();
    let the_body = body(exec.clone(), adm.clone());
    let wm = the_body.working_memory.clone();
    let cycle = WorkspaceCycle::new(
        vec![Arc::new(VaryingAct {
            ticks: Mutex::new(0),
        })],
        Arc::new(SalienceArbiter),
    )
    .with_acting(the_body);
    drive_to_settle(
        &cycle,
        "fix the bug",
        6,
        TurnFraming::ambient().on_workspace(),
    )
    .await;
    let entries = wm.recent_entries();
    let budget_facts: Vec<_> = entries
        .iter()
        .filter(|e| e.text.contains("[act-budget]"))
        .collect();
    // The turn-start contract fact fires at act 0 and is legitimately
    // EVICTED by the WM ring as acts accumulate (observed in this very
    // test: 6 acts on a capacity-8 ring pushed it out) — which is exactly
    // why the midpoint/two-remaining milestones RE-STATE the budget. The
    // guarantee worth pinning is therefore: at settle, working memory
    // holds at least one [act-budget] fact naming the REAL budget number.
    assert!(
        budget_facts
            .iter()
            .any(|e| e.text.contains("of 6 acts") || e.text.contains("my 6 acts")),
        "a budget fact naming the real budget must survive to settle; got: {:?}",
        budget_facts.iter().map(|e| &e.text).collect::<Vec<_>>()
    );

    // Control: an ordinary (speech-deliverable) turn records no stopwatch.
    let exec2 = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "file contents...".into(),
    });
    let body2 = body(exec2, admission());
    let wm2 = body2.working_memory.clone();
    let cycle2 = WorkspaceCycle::new(
        vec![Arc::new(VaryingAct {
            ticks: Mutex::new(0),
        })],
        Arc::new(SalienceArbiter),
    )
    .with_acting(body2);
    drive_to_settle(&cycle2, "look around", 6, TurnFraming::ambient()).await;
    assert!(
        !wm2.recent_entries()
            .iter()
            .any(|e| e.text.contains("[act-budget]")),
        "speech turns owe no stopwatch — the fact is scoped to workspace-deliverable turns"
    );
}

// what this catches: the grader's stopwatch. A mind that never settles is
// bounded by the EXTERNAL `max_acts` budget and the final un-driven Act is
// returned as unfinished — never a fabricated answer, and the budget is the
// observer's, not a cap in the persona's head (ACTING-ORGANISM §4).
#[tokio::test]
async fn drive_to_settle_returns_unsettled_act_when_budget_exhausted() {
    let exec = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "...".into(),
    });
    let adm = admission();
    let cycle = WorkspaceCycle::new(vec![Arc::new(AlwaysAct)], Arc::new(SalienceArbiter))
        .with_acting(body(exec.clone(), adm.clone()));

    let outcome = drive_to_settle(&cycle, "go", 2, TurnFraming::ambient()).await;

    assert_eq!(outcome.acts, 2, "spent exactly the observer's budget");
    assert!(
        outcome.spoken.is_none(),
        "did not settle → no spoken answer"
    );
    assert!(
        matches!(outcome.decision, Decision::Act { .. }),
        "returns the un-driven Act as honest 'did not finish'"
    );
}

// what this catches: changing results preserve productive iteration, while
// identical input AND output yields honestly after three repeated observations.
#[tokio::test]
async fn repeated_calls_respect_the_callers_budget() {
    for (results, expected_acts) in [
        (["unchanged"; 6], 4),
        (
            [
                "pending 1",
                "pending 2",
                "running",
                "written",
                "tested",
                "done",
            ],
            6,
        ),
    ] {
        let exec = Arc::new(ScriptedExecutor::new(results));
        let cycle = WorkspaceCycle::new(vec![Arc::new(AlwaysAct)], Arc::new(SalienceArbiter))
            .with_acting(body(exec.clone(), admission()));
        let outcome = drive_to_settle(&cycle, "go", 6, TurnFraming::ambient()).await;
        assert_eq!(outcome.acts, expected_acts);
        assert_eq!(exec.results.lock().unwrap().len(), 6 - expected_acts);
        assert!(matches!(outcome.decision, Decision::Act { .. }));
        assert!(outcome.spoken.is_none());
    }
}

// what this catches: fixed-point evidence ignores correlation IDs but never
// treats running or missing full results as a completed unchanged observation.
#[tokio::test]
async fn fixed_point_requires_available_finished_result_evidence() {
    use super::super::perception::batch_evidence;
    use crate::sdk_codegen::{ActVerdict, ToolVerdict};
    let exec = Arc::new(ScriptedExecutor::new(["result"]));
    let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter))
        .with_acting(body(exec, admission()));
    let mut call = tool_call();
    let mut acts = acts_of(apply_act(
        &cycle, &[call.clone()], "inspect", Uuid::new_v4(), &ActChain::new(),
        None,
        None,
        )
        .await,
    );
    let original = batch_evidence(&[call.clone()], &acts);
    assert!(original.is_some());
    call.id = "new-attempt".into();
    acts[0].call.id = call.id.clone();
    acts[0].output.result.tool_use_id = call.id.clone();
    assert!(original == batch_evidence(&[call.clone()], &acts));
    acts[0].output.result.content = "changed".into();
    assert!(original != batch_evidence(&[call.clone()], &acts));
    acts[0].output.verdict = ActVerdict::Declared(ToolVerdict::Running);
    assert!(batch_evidence(&[call.clone()], &acts).is_none());
    acts[0].output.verdict = ActVerdict::Unprojected;
    acts[0].output.result.custody = crate::ai::types::ResultCustody::Pointer {
        handle: "full-result".into(),
    };
    assert!(batch_evidence(&[call.clone()], &acts).is_none());
    assert!(batch_evidence(&[call], &[]).is_none());
}

// what this catches: the shared step's acting gate. `may_act = false` (how the
// eval driver paces ACTING past its budget) must return the decided Act WITHOUT
// executing it — the executor is never touched — so a deferred act can't run a
// tool the budget already forbade. `may_act = true` (the live path, always) runs
// it. This is the single seam that keeps live (one permitted act per tick) and
// eval (budget-gated acting) on the IDENTICAL per-step motion.
#[tokio::test]
async fn settle_step_defers_the_act_without_executing_when_may_act_is_false() {
    let exec = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "...".into(),
    });
    let adm = admission();
    let cycle = WorkspaceCycle::new(vec![Arc::new(AlwaysAct)], Arc::new(SalienceArbiter))
        .with_acting(body(exec.clone(), adm.clone()));

    let (deferred, _, _) = settle_step(
        &cycle,
        "go",
        false,
        TurnFraming::ambient(),
        Situation::FreshContext,
        &ActChain::new(),
    )
    .await;
    assert!(
        matches!(deferred, SettleStep::WouldAct { .. }),
        "may_act=false defers the act"
    );
    assert!(
        exec.seen_context.lock().unwrap().is_none(),
        "a deferred act NEVER touches the executor"
    );

    let (ran, _, _) = settle_step(
        &cycle,
        "go",
        true,
        TurnFraming::ambient(),
        Situation::FreshContext,
        &ActChain::new(),
    )
    .await;
    assert!(
        matches!(ran, SettleStep::Acted { .. }),
        "may_act=true runs it"
    );
    assert!(
        exec.seen_context.lock().unwrap().is_some(),
        "a permitted act DOES reach the executor"
    );
}

#[tokio::test]
async fn receipt_custody_failure_stops_settlement_without_retrying_inference() {
    use crate::ai::provenance::{GenerationOutcome, GenerationReceipt};
    use crate::cognition::act_observe::SettlementFailure;
    use crate::cognition::persona_workspace::{PersonaWorkspaceRegistry, TaskSinkError};
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Served {
        contribution: Contribution,
        calls: AtomicUsize,
    }
    #[async_trait]
    impl Faculty for Served {
        fn id(&self) -> FacultyId { FacultyId::Deliberation }
        fn reacts_to_broadcast(&self) -> bool { true }
        async fn contribute(&self, _: &Workspace) -> Option<Contribution> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Some(self.contribution.clone())
        }
    }
    let owner = Uuid::new_v4();
    let incarnation = Uuid::new_v4();
    let registry = Arc::new(PersonaWorkspaceRegistry::new());
    let mut contribution = Contribution::verdict(Decision::Speak { text: "must not be emitted".into() }, 1.0, "served");
    contribution.receipts.push(GenerationReceipt {
        submitted_request_id: "served-before-owner-loss".into(),
        outcome: GenerationOutcome::Served { model: "fixture".into(), provider: "fixture".into(), provider_request_id: None },
        capture: None, genes: Vec::new(), gene_scales: Vec::new(), allocation: None,
    });
    contribution.render_receipts.push(crate::cognition::render_receipt::RenderReceipt {
        owner, incarnation, request: "served-before-owner-loss".into(),
        view: continuum_context::view::Receipt { epoch: 1, items: Vec::new() },
    });
    let faculty = Arc::new(Served { contribution, calls: AtomicUsize::new(0) });
    let cycle = WorkspaceCycle::new(vec![faculty.clone()], Arc::new(SalienceArbiter))
        .with_render_custody(&registry, owner, incarnation);
    drop(registry);
    let outcome = drive_to_settle(&cycle, "observe the task".to_owned(), 2, TurnFraming::directed()).await;
    assert_eq!(faculty.calls.load(Ordering::SeqCst), 1, "persistence must not spend an inference retry");
    assert!(outcome.spoken.is_none());
    assert_eq!(outcome.acts, 0);
    assert!(!outcome.deferred_before_dispatch, "the generation really was served");
    assert!(matches!(&outcome.failure, Some(SettlementFailure::ReceiptPersistence(error))
        if matches!(error.as_ref(), TaskSinkError::OwnerGone(id) if *id == owner)));
    assert_eq!(outcome.generation_receipts.len(), 1);
    let (step, _, receipts) = SettleStep::from_settled(outcome);
    assert!(matches!(step, SettleStep::ReceiptPersistenceFailed { .. }));
    assert!(matches!(receipts[0].outcome, GenerationOutcome::Served { .. }));
}
