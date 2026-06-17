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
use std::fmt::Write as _;
use uuid::Uuid;

use super::tool_executor::{ToolExecutionContext, ToolExecutor};
use super::workspace::{Contribution, Decision, Faculty, FacultyId, Workspace};
use crate::ai::adapter::AIProviderAdapter;
use crate::ai::types::{
    ChatMessage, ContentPart, FinishReason, MessageContent, NativeToolSpec, TextGenerationRequest,
};
use crate::persona::prompt_assembly::{
    looks_like_silence_token, SILENCE_AFFORDANCE_BLOCK, SILENCE_TOKEN,
};

/// Default cap on tool-use rounds in one deliberation tick. A persona that ACTS
/// loops generate→tool→generate; the bound stops a model that never stops calling
/// tools from spinning the turn forever (a turn is bounded work, not an open agent).
const DEFAULT_MAX_TOOL_ITERATIONS: usize = 4;

/// Max chars of a tool result fed back to the model — keeps the re-prompt bounded.
const TOOL_RESULT_MAX_CHARS: usize = 8_000;

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
    if trimmed.is_empty() || looks_like_silence_token(trimmed) || starts_with_silence_token(trimmed)
    {
        Decision::Pass
    } else {
        Decision::Speak {
            text: trimmed.to_string(),
        }
    }
}

/// True if the response STARTS with the silence token (e.g. `"PASS — nothing to
/// add"`). Small models frequently emit `PASS` plus trailing prose despite the
/// "no other text" instruction; without this they'd literally speak the word
/// "PASS" into the room. The leading-token check treats that as the chosen
/// silence it is. (Accepted trade: a real message whose first word is literally
/// "pass" is silenced — vanishingly rare for a deliberation turn, and silence is
/// a first-class, low-cost outcome.)
fn starts_with_silence_token(text: &str) -> bool {
    let Some(first) = text.split_whitespace().next() else {
        return false;
    };
    let core = first.trim_end_matches(|c: char| !c.is_alphanumeric());
    core.eq_ignore_ascii_case(SILENCE_TOKEN)
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
    /// The persona's authorized tool set. Empty → the persona can only SPEAK
    /// (no `tools` passed to the model). Non-empty → the persona can ACT: the
    /// model may emit tool_calls the agent loop executes. Rust-origin contracts,
    /// the executor runs them ([[commands-are-kernel-level-and-compose]]).
    tools: Vec<NativeToolSpec>,
    /// Runs the tool calls the model emits. `None` → no acting even if `tools` is
    /// set (degrades to speak-only). Injected (trait): a TS-IPC impl in production,
    /// a mock in tests — the loop is impl-agnostic.
    tool_executor: Option<Arc<dyn ToolExecutor>>,
    /// Bound on tool-use rounds per tick ([`DEFAULT_MAX_TOOL_ITERATIONS`]).
    max_tool_iterations: usize,
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
            tools: Vec::new(),
            tool_executor: None,
            max_tool_iterations: DEFAULT_MAX_TOOL_ITERATIONS,
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

    /// Authorize a tool set — the persona can now ACT, not just speak. The model
    /// is offered these tools; emitted tool_calls run through the executor.
    pub fn with_tools(mut self, tools: Vec<NativeToolSpec>) -> Self {
        self.tools = tools;
        self
    }

    /// Inject the tool executor (the thing that runs the calls). Without it,
    /// `tools` are inert — the faculty stays speak-only.
    pub fn with_tool_executor(mut self, executor: Arc<dyn ToolExecutor>) -> Self {
        self.tool_executor = Some(executor);
        self
    }

    /// Build a generation request for the current message thread. Centralized so
    /// the agent loop's re-prompts and the first prompt share one shape.
    fn build_request(
        &self,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<NativeToolSpec>>,
        system_prompt: String,
    ) -> TextGenerationRequest {
        TextGenerationRequest {
            messages,
            system_prompt: Some(system_prompt),
            model: self.model.clone(),
            provider: None,
            temperature: Some(self.temperature),
            max_tokens: Some(self.max_tokens),
            top_p: None,
            top_k: None,
            repeat_penalty: None,
            stop_sequences: None,
            tools,
            tool_choice: None,
            response_format: None,
            active_adapters: None,
            request_id: None,
            user_id: None,
            room_id: None,
            purpose: Some("cognition/deliberation".to_string()),
            persona_id: Some(self.persona_id.to_string()),
        }
    }

    /// Context for tool execution this tick. session/context ids are nil for now
    /// (the Workspace cycle isn't session-scoped yet); they thread in when the
    /// recipe-executor passes the live session — a follow-up, flagged not faked.
    fn tool_context(&self) -> ToolExecutionContext {
        ToolExecutionContext {
            persona_id: self.persona_id,
            persona_name: self.persona_name.clone(),
            session_id: uuid::Uuid::nil(),
            context_id: uuid::Uuid::nil(),
            caller_context: serde_json::Value::Null,
            persona_config: super::tool_executor::PersonaMediaConfigLite {
                auto_load_media: false,
                supported_media_types: Vec::new(),
            },
        }
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
        let mut s = String::with_capacity(self.system_prompt.len() + 768);
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
        // Tell the reasoner it is taking a TURN in this activity, not analyzing a
        // transcript — otherwise small models outline the situation instead of
        // participating. The activity is NOT hardcoded (it is recipe-defined): the
        // room's operating doctrine in the context above specializes HOW to
        // participate (chat / coordination / game / code / art / …). This block
        // only says "respond as yourself, in your own voice, not as an analysis."
        let _ = write!(
            s,
            "\n\n[Taking your turn]\n\
             What follows (the user message) is the recent activity in this space — \
             messages from OTHER participants. You are {name}. Write ONLY your own \
             single next message, in first person, in your own voice, the way you \
             would actually say it. Do NOT write or invent anyone else's lines, do \
             NOT continue or replay the transcript, do NOT prefix your message with \
             your name, do NOT write an outline, analysis, or narration of what you \
             are doing — just say your piece. Let the context above (including the \
             room's operating doctrine, if any) shape how you participate. If you \
             have nothing worth adding, stay silent.",
            name = self.persona_name,
        );
        // Reuse the ONE silence contract — PASS = first-class choice to stay quiet.
        s.push_str(SILENCE_AFFORDANCE_BLOCK);
        s
    }

    /// The EXACT prompt this faculty sends the model this tick — the system
    /// prompt (identity + assembled RAG context + how-to-participate + silence
    /// affordance) and the user burst. Exposed so what the LLM sees is trivially
    /// introspectable at every turn (tests, replay, operator tooling). The RAG is
    /// the load-bearing input; it must never be opaque.
    pub fn prompt_view(&self, ws: &Workspace) -> DeliberationPromptView {
        DeliberationPromptView {
            system: self.build_system_prompt(ws),
            user: ws.world_state.clone(),
        }
    }
}

/// A snapshot of exactly what the deliberation faculty sends the model — the
/// glass box over the RAG/prompt. Print it, capture it, diff it across turns.
#[derive(Debug, Clone)]
pub struct DeliberationPromptView {
    pub system: String,
    pub user: String,
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
        let view = self.prompt_view(ws);
        // Introspection seam: emit EXACTLY what the model sees this tick. The RAG
        // is the load-bearing input — never opaque. Enable the `cognition` log
        // category for the persona to capture this per-turn (the existing
        // record/replay harness — recorder + RagCaptureSink + vdd::turn_replay —
        // is the durable path; this debug emit is the live tap).
        tracing::debug!(
            target: "cognition::deliberation",
            persona = %self.persona_name,
            system_prompt = %view.system,
            burst = %view.user,
            "deliberation prompt — what the model sees this turn"
        );
        // The growing message thread. Starts with the consolidated burst; the
        // agent loop appends the model's tool_use turns + their tool results, so a
        // persona that ACTS reasons over what its tools returned before it speaks.
        let mut messages = vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text(view.user),
            name: None,
        }];
        let tools = (!self.tools.is_empty()).then(|| self.tools.clone());
        // ACT only when the persona has BOTH authorized tools AND an executor;
        // otherwise it's speak-only (tools inert) — backward-compatible.
        let can_act = tools.is_some() && self.tool_executor.is_some();
        let mut iterations = 0usize;

        loop {
            let request = self.build_request(messages.clone(), tools.clone(), view.system.clone());
            let resp = match self.adapter.generate_text(request).await {
                Ok(r) => r,
                // Inference failed — abstain this tick (no fabricated Pass: a failed
                // model is not a chosen silence).
                Err(e) => {
                    tracing::warn!(
                        persona = %self.persona_name,
                        error = %e,
                        "deliberation inference failed; abstaining this tick"
                    );
                    return None;
                }
            };

            // ACT: the model called tools, we can run them, and we're under the
            // per-tick bound → execute, thread the results back, re-generate.
            if can_act
                && iterations < self.max_tool_iterations
                && matches!(resp.finish_reason, FinishReason::ToolUse)
            {
                let calls = resp.tool_calls.clone().unwrap_or_default();
                if !calls.is_empty() {
                    let executor = self.tool_executor.as_ref().expect("can_act implies executor");
                    // Append the model's tool_use turn (so the re-prompt has the
                    // assistant's calls, then the results — the agent transcript).
                    let assistant_parts = calls
                        .iter()
                        .map(|c| ContentPart::ToolUse {
                            id: c.id.clone(),
                            name: c.name.clone(),
                            input: c.input.clone(),
                        })
                        .collect::<Vec<_>>();
                    messages.push(ChatMessage {
                        role: "assistant".to_string(),
                        content: MessageContent::Parts(assistant_parts),
                        name: None,
                    });

                    match executor
                        .execute_native_batch(&calls, &self.tool_context(), TOOL_RESULT_MAX_CHARS)
                        .await
                    {
                        Ok(outcome) => {
                            let result_parts = outcome
                                .results
                                .iter()
                                .map(|r| ContentPart::ToolResult {
                                    tool_use_id: r.tool_use_id.clone(),
                                    content: r.content.clone(),
                                    is_error: None,
                                })
                                .collect::<Vec<_>>();
                            messages.push(ChatMessage {
                                role: "user".to_string(),
                                content: MessageContent::Parts(result_parts),
                                name: None,
                            });
                            iterations += 1;
                            continue; // re-generate now that the tools have answered
                        }
                        // Tool batch failed — don't fabricate; fall through to a
                        // final decision over whatever text the model already gave.
                        Err(e) => {
                            tracing::warn!(
                                persona = %self.persona_name,
                                error = %e,
                                "tool batch failed; falling through to text decision"
                            );
                        }
                    }
                }
            }

            // FINAL: the model's text is the verdict (speak / pass).
            let decision = decision_from_response(&resp.text);
            // The faculty's own confidence in its verdict. A placeholder for a
            // model-derived signal (logprob / uncertainty) — NOT a caste weight;
            // it's how sure THIS mind is, which the arbiter integrates.
            let (salience, reasoning) = match &decision {
                Decision::Pass => (0.5, format!("{} chose silence (PASS)", self.persona_name)),
                _ => (
                    0.85,
                    format!("{} deliberated over the assembled context", self.persona_name),
                ),
            };
            return Some(Contribution::verdict(decision, salience, reasoning));
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
        // Small models leak trailing prose after PASS — must still be silence,
        // not a message that literally says "PASS ...".
        assert_eq!(decision_from_response("PASS — nothing to add here"), Decision::Pass);
        assert_eq!(decision_from_response("PASS.\nI'll stay quiet"), Decision::Pass);
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
