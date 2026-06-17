//! LlmDeliberationFaculty — the reasoner, backed by real inference.
//!
//! This is the deliberation-tier faculty (`reacts_to_broadcast() == true`): it
//! runs in phase 2 of the staged cycle, over the **assembled context** the
//! perception faculties (recall, world-model, affect, roster, doctrine) won into
//! the broadcast in phase 1. It calls a real `AIProviderAdapter` — the same trait
//! the live persona path uses — and turns the model's output into a participation
//! [`Decision`].
//!
//! ## It is NOT a gate
//!
//! Per [[no-rust-gates-around-cognition]] and §1.1 of
//! PERSONA-BRAIN-ARCHITECTURE.md, this faculty does not decide *whether the
//! persona is allowed* to think. It thinks, and its thought's *result* is the
//! Decision. Silence (`Pass`) is the model choosing the silence affordance
//! (`PASS` token, reusing `prompt_assembly::SILENCE_AFFORDANCE_BLOCK` +
//! `looks_like_silence_token` — one silence contract, not a second one) — the
//! persona's own judgment, never an `@`-trigger or a caste rule.
//!
//! ## Backend-agnostic
//!
//! It holds an `Arc<dyn AIProviderAdapter>` — the shared model backend, leased
//! per call (the cbar session-mutex lease pattern). In a bring-up harness that's
//! `HeuristicInferenceAdapter` (deterministic, no GPU) or a real
//! `LlamaCppAdapter` (a live local model). The faculty does not care which — the
//! brain is unchanged when the backend swaps.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use super::workspace::{Contribution, Decision, Faculty, FacultyId, Workspace};
use crate::ai::adapter::AIProviderAdapter;
use crate::ai::types::{ChatMessage, MessageContent, TextGenerationRequest};
use crate::persona::prompt_assembly::{looks_like_silence_token, SILENCE_AFFORDANCE_BLOCK};

/// Default sampling temperature for deliberation — enough warmth for natural
/// voice, not so much it drifts.
const DEFAULT_TEMPERATURE: f32 = 0.7;
/// Default response cap. Deliberation is a turn, not an essay.
const DEFAULT_MAX_TOKENS: u32 = 512;

/// Map a model's raw output to a participation [`Decision`].
///
/// Pure — no IO — so the Speak/Pass branches are unit-testable without a model.
/// `PASS` (the silence token) → `Pass`; anything else → `Speak`. `RaiseUnprompted`
/// is the volition faculty's channel (initiative with no prompt), not something
/// we infer from a single deliberation response — a deliberation faculty answers
/// the burst it was given.
pub fn decision_from_response(text: &str) -> Decision {
    let trimmed = text.trim();
    if trimmed.is_empty() || looks_like_silence_token(trimmed) {
        Decision::Pass
    } else {
        Decision::Speak {
            text: trimmed.to_string(),
        }
    }
}

/// The reasoner faculty. Persona-scoped; shared model backend.
pub struct LlmDeliberationFaculty {
    persona_id: Uuid,
    persona_name: String,
    /// The persona's identity / deliberation system prompt (from RAG identity).
    system_prompt: String,
    /// Shared model backend, leased per call.
    adapter: Arc<dyn AIProviderAdapter>,
    /// Which model to ask for (None → the adapter's default).
    model: Option<String>,
    temperature: f32,
    max_tokens: u32,
}

impl LlmDeliberationFaculty {
    pub fn new(
        persona_id: Uuid,
        persona_name: impl Into<String>,
        system_prompt: impl Into<String>,
        adapter: Arc<dyn AIProviderAdapter>,
    ) -> Self {
        Self {
            persona_id,
            persona_name: persona_name.into(),
            system_prompt: system_prompt.into(),
            adapter,
            model: None,
            temperature: DEFAULT_TEMPERATURE,
            max_tokens: DEFAULT_MAX_TOKENS,
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// Render the assembled context (the phase-1 winners that hold the workspace)
    /// into a system-prompt block, so the reasoner conditions on what recall /
    /// world-model / affect surfaced. Only *context* contributions are included
    /// (the verdict isn't in the broadcast yet at phase 2).
    fn render_assembled_context(&self, ws: &Workspace) -> String {
        let mut block = String::new();
        for c in ws.broadcast.iter().filter(|c| c.decision.is_none()) {
            block.push_str("\n[");
            block.push_str(c.faculty.as_str());
            block.push_str("]\n");
            block.push_str(&c.content);
            block.push('\n');
        }
        block
    }

    fn build_system_prompt(&self, ws: &Workspace) -> String {
        let mut s = String::with_capacity(self.system_prompt.len() + 512);
        s.push_str(&self.system_prompt);
        let context = self.render_assembled_context(ws);
        if !context.is_empty() {
            s.push_str(
                "\n\n[What you are working with right now]\n\
                 The following is the context your mind assembled this moment — \
                 recalled memory, who is present, the room's nature, your read of \
                 the situation. Ground your contribution in it; you need not cite \
                 every line:\n",
            );
            s.push_str(&context);
        }
        // Reuse the ONE silence contract — PASS = first-class choice to stay quiet.
        s.push_str(SILENCE_AFFORDANCE_BLOCK);
        s
    }
}

#[async_trait]
impl Faculty for LlmDeliberationFaculty {
    fn id(&self) -> FacultyId {
        FacultyId::Deliberation
    }

    // Deliberation tier: reacts to the assembled broadcast (phase 2), so the
    // Decision is conditioned on the context recall/world-model/affect surfaced.
    fn reacts_to_broadcast(&self) -> bool {
        true
    }

    async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
        let request = TextGenerationRequest {
            messages: vec![ChatMessage {
                role: "user".to_string(),
                // The consolidated burst — the "catch up on the thread" the
                // persona is reasoning over this tick.
                content: MessageContent::Text(ws.world_state.clone()),
                name: None,
            }],
            system_prompt: Some(self.build_system_prompt(ws)),
            model: self.model.clone(),
            provider: None,
            temperature: Some(self.temperature),
            max_tokens: Some(self.max_tokens),
            top_p: None,
            top_k: None,
            repeat_penalty: None,
            stop_sequences: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            active_adapters: None,
            request_id: None,
            user_id: None,
            room_id: None,
            purpose: Some("cognition/deliberation".to_string()),
            persona_id: Some(self.persona_id.to_string()),
        };

        match self.adapter.generate_text(request).await {
            Ok(resp) => {
                let decision = decision_from_response(&resp.text);
                // The faculty's own confidence in its verdict. A placeholder for a
                // model-derived signal (logprob / uncertainty) — NOT a caste
                // weight; it's how sure THIS mind is, which the arbiter integrates.
                let (salience, reasoning) = match &decision {
                    Decision::Pass => (0.5, format!("{} chose silence (PASS)", self.persona_name)),
                    _ => (
                        0.85,
                        format!("{} deliberated over the assembled context", self.persona_name),
                    ),
                };
                Some(Contribution::verdict(decision, salience, reasoning))
            }
            // Inference failed — abstain this tick (the trace shows no verdict
            // bid; the caller treats absence as effective silence). We do NOT
            // fabricate a Pass: a failed model is not a chosen silence.
            Err(e) => {
                tracing::warn!(
                    persona = %self.persona_name,
                    error = %e,
                    "deliberation inference failed; abstaining this tick"
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;

    // what this catches: the PASS silence token maps to Decision::Pass (with or
    // without trailing punctuation); real content maps to Speak. One silence
    // contract, reused from prompt_assembly.
    #[test]
    fn decision_parsing_maps_pass_and_speak() {
        assert_eq!(decision_from_response("PASS"), Decision::Pass);
        assert_eq!(decision_from_response("  PASS.  "), Decision::Pass);
        assert_eq!(decision_from_response(""), Decision::Pass);
        match decision_from_response("Let's ship the deploy fix now.") {
            Decision::Speak { text } => assert!(text.contains("ship the deploy")),
            other => panic!("expected Speak, got {other:?}"),
        }
    }

    // what this catches: end-to-end through a REAL adapter (the deterministic
    // heuristic stand-in) — the faculty calls inference and produces a verdict
    // Contribution. Proves the faculty wires to the AIProviderAdapter trait the
    // live path uses; swap in LlamaCppAdapter for a live persona, unchanged.
    #[tokio::test]
    async fn deliberates_through_a_real_adapter() {
        let persona = Uuid::new_v4();
        let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
        let faculty = LlmDeliberationFaculty::new(
            persona,
            "Ivar",
            "You are Ivar, a thoughtful engineer on the grid.",
            adapter,
        );

        assert!(faculty.reacts_to_broadcast(), "deliberation is phase 2");

        // A workspace with a phase-1 recall context bid already broadcast.
        let mut ws = Workspace::new("teammate asks: where did we land on the deploy?");
        ws.broadcast.push(Contribution::context(
            FacultyId::Recall,
            "deploy pipeline was red; fix was merged at 4pm",
            0.8,
            "recalled",
        ));

        let c = faculty
            .contribute(&ws)
            .await
            .expect("deliberation should bid a verdict when inference succeeds");
        assert_eq!(c.faculty, FacultyId::Deliberation);
        assert!(
            c.decision.is_some(),
            "deliberation carries the participation verdict"
        );
        // The heuristic adapter acks (never emits PASS), so this is a Speak.
        assert!(matches!(c.decision, Some(Decision::Speak { .. })));
    }
}
