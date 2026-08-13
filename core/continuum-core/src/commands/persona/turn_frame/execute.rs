//! `persona/turn-execute` — drive a persona's ONE live turn in one IPC hop (typed,
//! dep-holding), the deterministic third doorbell to the single turn primitive.
//!
//! Where [`drain`](super::drain) stops at the replay-stable turn frame, this command
//! carries the drained messages all the way through a real turn — but NOT through a
//! second, parallel inference path. It drives [`settle_step`] on the persona's ONE live
//! [`WorkspaceCycle`], the exact step the airc heartbeat (`service_loop`) and the eval
//! grader drive:
//!
//! ```text
//!   drain inbox (deterministic command-side doorbell)
//!     -> synthesize the airc-shaped burst (same build_workspace_turns formatter)
//!     -> resolve the persona's live WorkspaceCycle (persona_workspace::global())
//!     -> settle_step(may_act = true, directed) — full cognition, faculties inject
//!        recall + grounding INSIDE the step
//!     -> bundle { replayRecord, inferenceResponse: <SettleStep + TurnMetrics> }
//! ```
//!
//! Why: this command used to build its OWN prompt from an adapter-less shadow cognition
//! and dispatch `inference/llm/request` through the module registry — a parallel path
//! with no system grounding that returned a stub. That is deleted. "Full cognition at all
//! times" means there is exactly ONE brain per persona and exactly ONE turn primitive;
//! a command-driven turn perceives its world byte-identically to a lived one and runs the
//! same Workspace (recall, grounding, deliberation, tools). The inbox stays only as the
//! deterministic message queue this turn is FOR (fed by `cognition/enqueue-message`).
//!
//! Captures the owning [`CognitionModule`](crate::modules::cognition::CognitionModule)'s
//! shared [`CognitionState`] — the per-persona inbox lives on it. The live brain is
//! resolved from the process-global workspace registry, not from module state. Assembled
//! by [`command_objects`](super::command_objects), called from `CognitionModule::commands`.
//!
//! Fail-loud notes: a persona with messages to turn but NO live `WorkspaceCycle` is not
//! hosted — `CommandError::Invalid` naming the cause, never an answer from a shadow
//! cognition. An **empty** drain (or an absent inbox) is a legitimate no-op: it returns
//! `{ replayRecord: null, inferenceResponse: null }` BEFORE resolving the brain, never an
//! error.
//!
//! [`settle_step`]: crate::cognition::act_observe::settle_step
//! [`WorkspaceCycle`]: crate::cognition::workspace::WorkspaceCycle
//!
//! `access: Internal` — substrate cognition IPC the host persona loop drives, not a
//! remote-callable persona toolbelt verb.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use crate::logging::TimingGuard;
use crate::modules::cognition::{record_drained_turn_frame, CognitionState};
use crate::persona::turn_frame::{PersonaTurnFrame, PersonaTurnFrameReplayRecord};
use crate::sdk_codegen::CommandError;

/// Default frame window (ms) when the caller omits `windowMs`. Transplanted from the arm.
fn default_window_ms() -> u64 {
    80
}

/// Default max messages a single drain pulls when the caller omits `maxItems`. From the arm.
fn default_max_items() -> u64 {
    16
}

/// Params for `persona/turn-execute`: which persona to turn and the drain frame bounds.
/// Everything but `personaId` falls back to the substrate defaults, so the minimal call
/// is `{ personaId }`.
///
/// Note: there is deliberately NO composition or generation-budget param. Genome
/// composition is paged in by the live `WorkspaceCycle`'s own faculties during the turn,
/// and generation length is owned by the model's adapter (tasks #45/#46 — no hardcoded
/// caps). A turn-execute caller seeds the messages; the persona's ONE brain owns how it
/// thinks and how long it speaks.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/TurnExecuteParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct TurnExecuteParams {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    #[serde(default = "default_window_ms")]
    #[ts(type = "number")]
    pub window_ms: u64,
    #[serde(default = "default_max_items")]
    #[ts(type = "number")]
    pub max_items: u64,
}

/// The bundled outcome of one persona turn: the replay-stable turn frame plus the live
/// turn's outcome. Both are `null` on an empty drain (no-op). `inferenceResponse` is the
/// [`settle_step_to_json`] projection of the [`SettleStep`] the live `WorkspaceCycle`
/// produced (`{ outcome, text?|intent?|calls?, metrics? }`) — its shape is owned by this
/// command, but rides as `unknown` on the wire so the field stays open as the outcome
/// vocabulary grows.
///
/// [`SettleStep`]: crate::cognition::act_observe::SettleStep
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/TurnExecuteResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct TurnExecuteResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub replay_record: Option<PersonaTurnFrameReplayRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub inference_response: Option<Value>,
}

crate::action_command! {
    /// Drive a persona's ONE live turn in one hop: drain the inbox into a replay-stable
    /// frame, synthesize the airc-shaped burst, and drive `settle_step` on the persona's
    /// live `WorkspaceCycle` (the same turn primitive the airc heartbeat and the eval
    /// grader drive — full cognition, no parallel inference path). Bundles
    /// `{ replayRecord, inferenceResponse }` where `inferenceResponse` is the settle-step
    /// outcome + live token/latency metrics. Returns the null pair when the drain window
    /// was empty (no-op). Substrate cognition IPC the host persona loop drives; not a
    /// persona toolbelt verb.
    pub struct TurnExecute { state: Arc<CognitionState> }
    name: "persona/turn-execute",
    access: Internal,
    params: TurnExecuteParams,
    output: TurnExecuteResult,
    run(this, _ctx, params) => {
        let _timer = TimingGuard::new("module", "persona_turn_execute");

        let max_items = usize::try_from(params.max_items)
            .map_err(|_| CommandError::Invalid(format!("max_items too large: {}", params.max_items)))?;

        // Drain the persona's inbox into an OWNED frame, then DROP the DashMap guard
        // before any await. The inbox is the deterministic command-side doorbell (fed by
        // `cognition/enqueue-message`); it carries only the WHO/WHEN/WHAT of the messages
        // this turn is FOR. It is NOT a second cognition — the RESPONSE comes from the
        // persona's ONE live brain below. Holding the `Ref` across `settle_step().await`
        // would be a lock-across-await (task #85); we extract owned data and release it.
        let raw_frame = {
            let Some(persona) = this.state.personas.get(&params.persona_id) else {
                // No inbox for this persona ⇒ nothing to turn ⇒ idle no-op (the null
                // pair), exactly like an empty drain. A tick with nothing to do is the
                // routine case, never a caller error.
                return Ok(TurnExecuteResult { replay_record: None, inference_response: None });
            };
            persona.inbox.drain_frame(params.window_ms, max_items)
        };
        record_drained_turn_frame(&raw_frame);

        // Empty drain: the null pair, NOT an Err. Idle ticks are routine.
        let inbox_frame = match raw_frame {
            Some(f) => f,
            None => {
                return Ok(TurnExecuteResult { replay_record: None, inference_response: None });
            }
        };

        // Keep the replay record for observability — the same deterministic frame
        // artifact the recorder captures — but DERIVE the turn from the live brain, not
        // from a separately-built prompt + a second inference dispatch (the parallel path
        // this command used to take, which built an adapter-less shadow prompt with no
        // system grounding and returned a stub).
        let turn_frame = PersonaTurnFrame::from_inbox_frame(inbox_frame.clone());
        let replay_record = turn_frame.replay_record();
        if let Some(ref rec) = replay_record {
            crate::persona::recorder::record_turn_frame_replay(rec);
        }

        // The room this turn is FOR — carried by the drained frame (the messages' room).
        let room = inbox_frame.room_id;

        // Synthesize ONE airc-shaped delivery from the drained messages — the exact
        // envelope `build_workspace_turns` reads (source_id "airc", per-item peer_id +
        // occurred_at_ms). This is the SAME burst formatter the live heartbeat
        // (service_loop) and the eval fork use, so a command-driven turn perceives its
        // world byte-identically to a lived one. The grounding tier (recall / roster /
        // doctrine) is injected by the live cycle's OWN faculties inside settle_step — we
        // compose only the message delivery here (matching eval.rs; the #8 convergence
        // note), so there is no double grounding and no second allocator.
        let items = inbox_frame
            .messages
            .iter()
            .map(|m| crate::persona::rag_budget::RagItem {
                content: m.content.clone(),
                tokens: 0,
                metadata: serde_json::json!({
                    "peer_id": m.sender_id.to_string(),
                    "occurred_at_ms": m.timestamp,
                }),
            })
            .collect::<Vec<_>>();
        let delivery = crate::persona::rag_budget::RagDelivery {
            source_id: "airc".to_string(),
            items,
            tokens_used: 0,
            continuation: None,
            resolution_used: crate::persona::rag_budget::ResolutionPreference::Raw,
        };
        // own_peer attributes the persona's OWN past posts (is_self → assistant role); a
        // command-seeded turn carries only inbound peer messages, so it's inert here but
        // kept honest. agent_name comes from the live workspace roster (airc owns names);
        // absent ⇒ empty (never a fabricated name, [[fallbacks-are-illegal-fail-loud]]).
        let own_peer = params.persona_id.to_string();
        let agent_name = crate::cognition::persona_workspace::global()
            .roster()
            .into_iter()
            .find(|(id, _)| *id == params.persona_id)
            .and_then(|(_, name)| name)
            .unwrap_or_default();
        let burst = crate::cognition::workspace::Burst::from_turns(
            room,
            crate::persona::service_loop::build_workspace_turns(
                std::slice::from_ref(&delivery),
                &own_peer,
                &agent_name,
                // The turn_frame command synthesizes the whole burst from its
                // supplied frame — the delivery IS the intended thread; no
                // out-of-band waking message to anchor.
                None,
            ),
        );

        // Resolve the persona's ONE live brain — the WorkspaceCycle the service_loop and
        // the SubstrateGovernor already drive (registered at spawn). Absent ⇒ the persona
        // is not hosted; fail loud naming the cause. We NEVER answer from an adapter-less
        // shadow cognition — that parallel path is exactly what this migration removes.
        let cycle = crate::cognition::persona_workspace::global()
            .get(&params.persona_id)
            .ok_or_else(|| CommandError::Invalid(format!(
                "persona/turn-execute: no live WorkspaceCycle for {} — the persona is not \
                 hosted. Spawn it before driving a turn; refusing to answer from a shadow \
                 cognition with no adapter.",
                params.persona_id
            )))?;

        // ONE turn primitive: settle_step on the LIVE cycle — the SAME step the airc
        // heartbeat (service_loop) and the eval grader drive; three doorbells, one brain,
        // full cognition (the cycle's faculties inject recall + grounding INSIDE the
        // step). `may_act = true`: the turn permits its one act. `directed = true`: a
        // command-seeded message is put TO the persona, so we withhold the silent-PASS
        // hatch — the same exam-is-directed measurement control the eval driver documents
        // (a structural harness fact fed to the mind, never a filter on her output).
        let (step, metrics) = crate::cognition::act_observe::settle_step(
            &cycle,
            burst,
            room,
            true,
            crate::cognition::workspace::TurnFraming::message(true),
            // One-shot directed tick: a fresh ask, so fuller grounding.
            crate::cognition::workspace::Situation::FreshContext,
        )
        .await;

        Ok(TurnExecuteResult {
            replay_record,
            inference_response: Some(settle_step_to_json(&step, metrics.as_ref())),
        })
    }
}

/// Project a [`SettleStep`](crate::cognition::act_observe::SettleStep) outcome (+ optional
/// [`TurnMetrics`](crate::cognition::workspace::TurnMetrics)) into the command's JSON
/// result cell. The ONE place `persona/turn-execute`'s outcome shape is defined, so the
/// sweep harness and any other caller read a stable
/// `{ outcome, text?|intent?|calls?, metrics? }` — where `metrics` carries the live
/// generation cost (tokens + latency) the model reported for THIS turn.
fn settle_step_to_json(
    step: &crate::cognition::act_observe::SettleStep,
    metrics: Option<&crate::cognition::workspace::TurnMetrics>,
) -> Value {
    use crate::cognition::act_observe::SettleStep;
    let mut base = match step {
        SettleStep::Spoke(text) => serde_json::json!({ "outcome": "spoke", "text": text }),
        SettleStep::Acted { calls, intent } => serde_json::json!({
            "outcome": "acted", "intent": intent, "calls": calls.len(),
        }),
        SettleStep::WouldAct { calls, intent } => serde_json::json!({
            "outcome": "wouldAct", "intent": intent, "calls": calls.len(),
        }),
        SettleStep::ActUnfulfilled { calls, intent } => serde_json::json!({
            "outcome": "actUnfulfilled", "intent": intent, "calls": calls.len(),
        }),
        SettleStep::Passed => serde_json::json!({ "outcome": "passed" }),
        // The model call FAILED — surface it LOUD and NAMED, never as a serene
        // `passed` ([[fallbacks-are-illegal-fail-loud]]). The sweep harness reads
        // this to tell an infra failure (timeout / 5xx / a serving lane refusing an
        // unhosted model) apart from a chosen silence.
        SettleStep::InferenceFailed { error } => serde_json::json!({
            "outcome": "inferenceFailed", "error": error,
        }),
    };
    if let Some(m) = metrics {
        base["metrics"] = serde_json::json!({
            "inputTokens": m.input_tokens,
            "outputTokens": m.output_tokens,
            "latencyMs": m.latency_ms,
            "tokensPerSecond": m.tokens_per_second(),
        });
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::act_observe::SettleStep;
    use crate::cognition::workspace::TurnMetrics;
    use crate::persona::{InboxMessage, Modality, PersonaCognition, SenderType};
    use crate::rag::RagEngine;
    use crate::sdk_codegen::{AccessLevel, ActionCommand, Ctx};

    // Build a TurnExecute command over a fresh CognitionState carrying one live persona
    // inbox. NOTE: this seeds only the command-side inbox (the deterministic doorbell) —
    // it deliberately does NOT register a live WorkspaceCycle in persona_workspace::global()
    // (that needs a real adapter + model). So a turn WITH messages here exercises the
    // not-hosted fail-loud path; the real settle_step outcome is validated live via `uu`.
    fn command_with_persona(persona_id: Uuid) -> (TurnExecute, Arc<CognitionState>) {
        let rag_engine = Arc::new(RagEngine::new());
        let state = Arc::new(CognitionState::new(rag_engine.clone()));
        state.personas.insert(
            persona_id,
            PersonaCognition::new(persona_id, "Test Persona".to_string(), rag_engine),
        );
        (
            TurnExecute {
                state: state.clone(),
            },
            state,
        )
    }

    fn enqueue_message(state: &CognitionState, persona_id: Uuid, content: &str, timestamp: u64) {
        let persona = state
            .personas
            .get(&persona_id)
            .expect("test persona exists");
        persona.inbox.enqueue(InboxMessage {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            sender_name: "Operator".to_string(),
            sender_type: SenderType::Human,
            content: content.to_string(),
            timestamp,
            priority: 0.9,
            source_modality: Some(Modality::Chat),
            voice_session_id: None,
        });
    }

    fn params(persona_id: Uuid) -> TurnExecuteParams {
        serde_json::from_value(serde_json::json!({ "personaId": persona_id.to_string() }))
            .expect("minimal params deserialize")
    }

    // what this catches: the name + access contract. turn-execute is host-driven
    // substrate IPC (the persona loop turns its own inbox), so it stays Internal —
    // registered and grid-routable, never a remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(TurnExecute::NAME, "persona/turn-execute");
        assert_eq!(TurnExecute::ACCESS, AccessLevel::Internal);
    }

    // what this catches: the frame-bound defaults survive an absent-field payload — a
    // `{ personaId }` call still gets window_ms=80 / max_items=16, not a deserialize
    // error. Guards the serde(default) wiring. (There is deliberately no generation-budget
    // or composition param anymore — the live cycle + adapter own those.)
    #[test]
    fn defaults_fill_absent_turn_bounds() {
        let p = params(Uuid::nil());
        assert_eq!(p.window_ms, 80);
        assert_eq!(p.max_items, 16);
    }

    // what this catches: an absent persona (no inbox) is a no-op, NOT an error. A tick for
    // a persona this core doesn't host has nothing to drain, so it returns the null pair
    // BEFORE touching the brain — idle ticks are routine, never a caller error.
    #[tokio::test]
    async fn absent_persona_is_noop_not_error() {
        let rag_engine = Arc::new(RagEngine::new());
        let state = Arc::new(CognitionState::new(rag_engine));
        let cmd = TurnExecute { state };

        let out = cmd
            .run(&Ctx::default(), params(Uuid::new_v4()))
            .await
            .expect("absent persona is a no-op, not an error");
        assert!(out.replay_record.is_none(), "no inbox → null replayRecord");
        assert!(
            out.inference_response.is_none(),
            "no inbox → null inferenceResponse"
        );
    }

    // what this catches: an empty inbox short-circuits to the null pair BEFORE resolving
    // the live brain — a no-op, not an error (idle ticks are routine).
    #[tokio::test]
    async fn empty_drain_returns_null_bundle() {
        let persona_id = Uuid::new_v4();
        let (cmd, _state) = command_with_persona(persona_id);

        let out = cmd
            .run(&Ctx::default(), params(persona_id))
            .await
            .expect("empty drain is a no-op, not an error");
        assert!(
            out.replay_record.is_none(),
            "empty drain → null replayRecord"
        );
        assert!(
            out.inference_response.is_none(),
            "empty drain → null inferenceResponse (brain never resolved)"
        );
    }

    // what this catches: a persona WITH messages to turn but NO live WorkspaceCycle fails
    // loud (CommandError::Invalid naming "not hosted") — it NEVER answers from an
    // adapter-less shadow cognition. This is the whole point of the unification: the
    // response comes from the ONE live brain or not at all. The random persona_id is
    // guaranteed absent from the process-global workspace registry, so global().get() is
    // None regardless of what other tests registered.
    #[tokio::test]
    async fn messages_without_live_cycle_fail_loud_not_hosted() {
        let persona_id = Uuid::new_v4();
        let (cmd, state) = command_with_persona(persona_id);
        enqueue_message(&state, persona_id, "what is 17 * 23?", 20_000);

        let err = cmd
            .run(&Ctx::default(), params(persona_id))
            .await
            .expect_err("a turn with no live brain must fail loud, not answer from a shadow");
        match err {
            CommandError::Invalid(msg) => {
                assert!(msg.contains("not hosted"), "got: {msg}");
                assert!(
                    msg.contains(&persona_id.to_string()),
                    "must name the persona: {msg}"
                );
            }
            other => panic!("expected Invalid naming not-hosted, got {other:?}"),
        }
    }

    // what this catches: the settle_step → JSON projection is the ONE stable outcome
    // shape. A Spoke carries `outcome:"spoke"` + the text; a Passed carries just the
    // outcome; and when metrics are present the live token/latency numbers ride under
    // `metrics` (the speed half of the scoreboard the sweep reads). Guards the wire
    // vocabulary any turn-execute caller depends on without needing a live cycle.
    #[test]
    fn settle_step_json_projection_is_the_contract() {
        let metrics = TurnMetrics {
            input_tokens: 128,
            output_tokens: 7,
            latency_ms: 500,
            ..Default::default()
        };
        let spoke = settle_step_to_json(
            &SettleStep::Spoke("17 * 23 = 391".to_string()),
            Some(&metrics),
        );
        assert_eq!(spoke["outcome"], "spoke");
        assert_eq!(spoke["text"], "17 * 23 = 391");
        assert_eq!(spoke["metrics"]["inputTokens"], 128);
        assert_eq!(spoke["metrics"]["outputTokens"], 7);
        assert_eq!(spoke["metrics"]["latencyMs"], 500);
        assert_eq!(spoke["metrics"]["tokensPerSecond"], 14.0);

        // No metrics ⇒ no metrics key (never a fabricated zero-cost row).
        let passed = settle_step_to_json(&SettleStep::Passed, None);
        assert_eq!(passed["outcome"], "passed");
        assert!(
            passed.get("metrics").is_none(),
            "absent metrics must not synthesize a row"
        );

        // A FAILED model call projects a distinct, NAMED `inferenceFailed` outcome —
        // never a serene `passed`. This is what lets the sweep harness tell an infra
        // fault (timeout / 5xx / a lane refusing an unhosted model) apart from a
        // chosen silence ([[fallbacks-are-illegal-fail-loud]]).
        let failed = settle_step_to_json(
            &SettleStep::InferenceFailed {
                error: "serving lane refused unhosted model".to_string(),
            },
            None,
        );
        assert_eq!(failed["outcome"], "inferenceFailed");
        assert_eq!(failed["error"], "serving lane refused unhosted model");
        assert_ne!(
            failed["outcome"], "passed",
            "an inference failure must never read as a chosen silence"
        );
    }
}
