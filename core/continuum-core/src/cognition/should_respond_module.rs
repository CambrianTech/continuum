//! `ai/should-respond` — the kernel command that runs a persona's mind.
//!
//! Per IntelMac's recipe-executor design (and [[commands-are-kernel-level-and-
//! compose]]): participation is **one command, N lane-routed handlers**. This is
//! the handler for a continuum-native persona — it resolves the persona's
//! `WorkspaceCycle` from the [`PersonaWorkspaceRegistry`], runs it over the
//! consolidated burst, and returns the [`Decision`] (the kebab-case wire enum
//! external ACP bridges also produce). The recipe-pipeline walker dispatches this
//! as one step; the persona service loop calls it to decide whether to speak.
//!
//! It is NOT a gate. It runs the mind and returns the mind's verdict. Silence
//! (`Pass`) is the persona's own judgment (the deliberation faculty chose the
//! PASS affordance), never a caste/mention rule. No workspace registered for the
//! persona → an error, not a silent drop: a persona with no mind is a bug to
//! surface, not a reason to fake silence.

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use super::persona_workspace::PersonaWorkspaceRegistry;
use super::workspace::Decision;
use crate::runtime::service_module::{CommandResult, ModuleConfig, ModulePriority, ServiceModule};
use crate::runtime::ModuleContext;

/// The kernel command name. One decision, one place.
pub const SHOULD_RESPOND_COMMAND: &str = "ai/should-respond";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShouldRespondParams {
    /// Which persona's mind to run.
    persona_id: Uuid,
    /// The consolidated burst — the "catch up on the thread" world-state the
    /// persona reasons over this tick (recent room transcript, consolidated).
    burst: String,
}

/// ServiceModule that registers `ai/should-respond` against the persona-scoped
/// workspace registry. The registry is populated at persona spawn (one cycle per
/// persona — the "one soul, many rooms" invariant); this module just resolves +
/// runs it.
pub struct ShouldRespondModule {
    registry: Arc<PersonaWorkspaceRegistry>,
}

impl ShouldRespondModule {
    pub fn new(registry: Arc<PersonaWorkspaceRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl ServiceModule for ShouldRespondModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "ai-should-respond",
            // Cognition-class scheduling (sub-10ms target dispatch; the inference
            // itself is leased inside the deliberation faculty off this path).
            priority: ModulePriority::High,
            command_prefixes: &["ai/should-respond"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            // Per-persona cycles serialize their own model lease; the module
            // itself is unbounded (many personas can be deciding concurrently).
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            SHOULD_RESPOND_COMMAND => {
                let p: ShouldRespondParams = serde_json::from_value(params)
                    .map_err(|e| format!("{SHOULD_RESPOND_COMMAND} params: {e}"))?;
                let cycle = self.registry.get(&p.persona_id).ok_or_else(|| {
                    format!(
                        "no workspace cycle registered for persona {} — its mind was not assembled at spawn",
                        p.persona_id
                    )
                })?;
                // Run the persona's continuous mind over the burst. The decision
                // is the OUTPUT of cognition; `None` (nothing won attention
                // strongly enough to externalize) is effective silence = Pass.
                let workspace = cycle.run(p.burst).await;
                let decision = workspace.decision().cloned().unwrap_or(Decision::Pass);
                CommandResult::json(&decision)
            }
            other => Err(format!("unknown command: {other}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    use crate::cognition::persona_workspace::PersonaBrainConfig;
    use crate::persona::admission_state::AdmissionState;
    use crate::persona::engram::{ChatMessageRef, Engram, EngramKind, EngramOrigin, TrustState};
    use crate::persona::recall_metadata::{RecallMetadata, RecallMetadataRegistry};
    use crate::runtime::service_module::CommandResult;

    fn seed_admission(now_ms: u64) -> Arc<AdmissionState> {
        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta.clone()));
        let id = Uuid::new_v4();
        let engram = Engram {
            context_id: None,
            id,
            kind: EngramKind::Episodic,
            content: "we agreed to ship the deploy fix after review".to_string(),
            origin: EngramOrigin::Chat(ChatMessageRef {
                message_id: Uuid::new_v4(),
                room_id: Uuid::new_v4(),
                sender_id: Uuid::new_v4(),
                posted_at_ms: now_ms,
                content_hash: "h".to_string(),
            }),
            recall_keys: Vec::new(),
            admitted_at_ms: now_ms,
            trust_state_at_admission: TrustState::ApprovedPeer,
            admission_trace_id: None,
        };
        state.push_for_test(engram);
        recall_meta.admit(
            id,
            RecallMetadata {
                salience: 0.7,
                access_count: 0,
                last_accessed_ms: 0,
                protected_until_ms: 0,
                last_decayed_ms: now_ms,
            },
        );
        state
    }

    fn registry_with_ivar(persona: Uuid) -> Arc<PersonaWorkspaceRegistry> {
        let registry = Arc::new(PersonaWorkspaceRegistry::new());
        registry.get_or_build(PersonaBrainConfig {
            quiesced: None, // not a hosted mind (fork/module view) — never lease-preempted
            persona_id: persona,
            persona_name: "Ivar".to_string(),
            system_prompt: "You are Ivar, an engineer on the grid.".to_string(),
            admission: seed_admission(1_000_000_000),
            adapter: Arc::new(HeuristicInferenceAdapter::new()),
            capacity: None,
            grounding_sources: Vec::new(),
            embedder: None,
            tool_executor: None,
            context_window: crate::cognition::serving_plan::MIN_SERVE_CTX,
            // Harness: synchronous perception (deferral is a live-path concern).
            defer_recall: false,
            defer_grounding: false,
            suppress_recall: false,
        });
        registry
    }

    // what this catches: ai/should-respond resolves a persona's mind, runs it, and
    // returns a Decision as JSON (the wire enum). This is the kernel command the
    // recipe walker + persona loop dispatch — proven end-to-end against a real
    // adapter. Swap LlamaCppAdapter and the same command drives a live persona.
    #[tokio::test]
    async fn should_respond_runs_the_mind_and_returns_a_decision() {
        let persona = Uuid::new_v4();
        let module = ShouldRespondModule::new(registry_with_ivar(persona));
        let params = serde_json::json!({
            "personaId": persona.to_string(),
            "burst": "teammate: where did we land on the deploy fix?",
        });
        let result = module
            .handle_command(SHOULD_RESPOND_COMMAND, params)
            .await
            .expect("ai/should-respond should succeed");
        let json = match result {
            CommandResult::Json(v) => v,
            other => panic!("expected Json result, got {other:?}"),
        };
        // The wire shape is the Decision enum (serde tag = "kind").
        let decision: Decision =
            serde_json::from_value(json).expect("result must deserialize to a Decision");
        assert!(matches!(decision, Decision::Speak { .. }));
    }

    // what this catches: an unregistered persona is a surfaced ERROR, never a
    // faked Pass — a persona with no assembled mind is a bug, not silence.
    #[tokio::test]
    async fn unregistered_persona_errors_not_silently_passes() {
        let module = ShouldRespondModule::new(Arc::new(PersonaWorkspaceRegistry::new()));
        let params = serde_json::json!({
            "personaId": Uuid::new_v4().to_string(),
            "burst": "anyone there?",
        });
        let err = module
            .handle_command(SHOULD_RESPOND_COMMAND, params)
            .await
            .expect_err("missing workspace must error");
        assert!(err.contains("no workspace cycle"));
    }
}
