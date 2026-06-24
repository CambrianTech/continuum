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

use arc_swap::ArcSwap;
use async_trait::async_trait;
use std::fmt::Write as _;
use uuid::Uuid;

use super::workspace::{Contribution, Decision, Faculty, FacultyId, Workspace};
use crate::ai::adapter::AIProviderAdapter;
use crate::ai::types::{
    ActiveAdapterRequest, ChatMessage, FinishReason, NativeToolSpec, TextGenerationRequest,
    TextGenerationResponse,
};

use crate::persona::prompt_assembly::{
    looks_like_silence_token, SILENCE_AFFORDANCE_BLOCK, SILENCE_TOKEN,
};
use crate::persona::text_analysis::clean_response;

/// The persona's paged-in genome: the LoRA layers active for this faculty's next
/// generation. Shared (the [`WorkspaceCycle`](super::workspace::WorkspaceCycle)
/// holds the same handle and pages skills in/out); read wait-free on every
/// generation via [`ArcSwap`] — virtual memory for skill, the page-in wire the
/// genome loop trains against. Empty (the default) → base model, no gene.
pub type GenomeHandle = Arc<ArcSwap<Vec<ActiveAdapterRequest>>>;

/// A fresh, empty genome handle (base model, no gene paged in).
pub fn empty_genome() -> GenomeHandle {
    Arc::new(ArcSwap::from_pointee(Vec::new()))
}

/// Default sampling temperature for deliberation — enough warmth for natural
/// voice, not so much it drifts.
const DEFAULT_TEMPERATURE: f32 = 0.7;

/// Map a model's raw output to a participation [`Decision`].
///
/// Pure — no IO — so the Speak/Pass branches are unit-testable without a model.
/// `PASS` (the silence token) → `Pass`; anything else → `Speak`. `RaiseUnprompted`
/// is the volition faculty's channel (initiative with no prompt), not something
/// we infer from a single deliberation response — a deliberation faculty answers
/// the burst it was given.
pub fn decision_from_response(text: &str) -> Decision {
    // Strip `<think>`/`<thinking>` chain-of-thought before deciding. qwen3.5-family
    // models emit a reasoning block (often an EMPTY `<think></think>`) ahead of the
    // answer; the spoken text must NEVER carry those tags into the room. The legacy
    // respond() path already cleaned; the workspace path (now the live decision
    // path) reached `say` raw — this closes that gap at the single point where model
    // text becomes a Speak decision, so every consumer of the decision gets clean
    // text. An only-`<think>` response cleans to empty → Pass (silence), matching
    // the "only thinking → don't speak" behavior.
    let cleaned = clean_response(text);
    let trimmed = cleaned.text.trim();
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
    /// The persona's authorized tool set. Empty → the persona can only SPEAK
    /// (no `tools` passed to the model). Non-empty → the persona can ACT: the
    /// model may emit tool_calls, which this faculty surfaces as a single
    /// [`Decision::Act`] verdict. It does NOT run them — executing the calls,
    /// remembering the result, and re-perceiving is the ORGANISM's job (the
    /// act→observe driver, [`super::act_observe`]). The faculty is single-shot:
    /// one generation → one verdict (`Act` xor `Speak` xor `Pass`) per tick.
    tools: Vec<NativeToolSpec>,
    /// Where this faculty records its chain-of-thought after a verdict, so the
    /// persona can resume its train of thought next turn (the
    /// [`WorkingMemory`](crate::cognition::working_memory::WorkingMemory)
    /// `WorkingMemoryFaculty` reads). `None` → reasoning is dropped (the prior
    /// behavior). Only the suppressed-thinking default makes `reasoning` empty, so
    /// this self-activates exactly when thinking is on.
    working_memory: Option<Arc<crate::cognition::working_memory::WorkingMemory>>,
    /// Verbatim LLM-I/O capture — the exact system prompt + message thread sent and
    /// the raw response, per agent-loop iteration. `None` → no capture (zero cost).
    /// The live spawn path attaches a per-persona JSONL sink. See
    /// [`prompt_capture`](crate::cognition::prompt_capture).
    prompt_capture: Option<Arc<dyn crate::cognition::prompt_capture::PromptCaptureSink>>,
    /// The persona's paged-in genome — the LoRA layers this faculty injects into
    /// every generation request ([`GenomeHandle`]). Shared with the owning
    /// [`WorkspaceCycle`](super::workspace::WorkspaceCycle), which pages skills
    /// in/out; read wait-free here on each generation. Empty → base model. This is
    /// the page-in wire the genome loop measures (`cognition/eval` A/Bs base vs a
    /// paged-in gene through this exact field).
    genome: GenomeHandle,
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
            tools: Vec::new(),
            working_memory: None,
            prompt_capture: None,
            genome: empty_genome(),
        }
    }

    /// Share the persona's genome handle — the same [`ArcSwap`] the owning
    /// [`WorkspaceCycle`](super::workspace::WorkspaceCycle) pages skills in/out of.
    /// Every generation reads the current genome wait-free; a page-in on the cycle
    /// takes effect on the faculty's next generation (virtual memory for skill).
    pub fn with_genome(mut self, genome: GenomeHandle) -> Self {
        self.genome = genome;
        self
    }

    /// Attach a verbatim prompt/response capture sink — every LLM call this
    /// faculty makes is appended to it (best-effort). Off by default.
    pub fn with_prompt_capture(
        mut self,
        sink: Arc<dyn crate::cognition::prompt_capture::PromptCaptureSink>,
    ) -> Self {
        self.prompt_capture = Some(sink);
        self
    }

    /// Record this faculty's reasoning into `memory` after each verdict, so the
    /// persona carries its train of thought forward across turns.
    pub fn with_working_memory(
        mut self,
        memory: Arc<crate::cognition::working_memory::WorkingMemory>,
    ) -> Self {
        self.working_memory = Some(memory);
        self
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
    /// is offered these tools; an emitted tool_call becomes a [`Decision::Act`]
    /// verdict the act→observe driver runs (the faculty itself never executes).
    pub fn with_tools(mut self, tools: Vec<NativeToolSpec>) -> Self {
        self.tools = tools;
        self
    }

    /// Build a generation request for the message thread. Centralized so the
    /// first prompt and any future re-prompt share one shape.
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
            // The MODEL owns its generation length (the adapter forwards no ceiling
            // when None → unsloth/llama.cpp run to the model's own stop token). A
            // deliberation turn ends when the model stops, NOT at a const we picked:
            // a flat cap truncated qwen3.5 mid-`<think>` → empty reply.
            max_tokens: None,
            top_p: None,
            top_k: None,
            repeat_penalty: None,
            stop_sequences: None,
            tools,
            tool_choice: None,
            response_format: None,
            // Page in the persona's current genome (wait-free read). Empty → None,
            // so the base model runs with no LoRA; a paged-in gene rides into the
            // request as `active_adapters`, which the adapter injects as
            // `"lora":[{id,scale}]`. This is the measured page-in seam.
            active_adapters: {
                let genome = self.genome.load();
                (!genome.is_empty()).then(|| genome.as_ref().clone())
            },
            request_id: None,
            user_id: None,
            room_id: None,
            purpose: Some("cognition/deliberation".to_string()),
            persona_id: Some(self.persona_id.to_string()),
        }
    }

    /// Turn an emitted tool-call into an [`Decision::Act`] verdict — the persona
    /// has chosen to use her hands. The faculty does NOT run the calls; the
    /// act→observe driver ([`super::act_observe`]) executes them, remembers the
    /// result, and re-perceives. `intent` is the model's own stated reasoning when
    /// present (so the engram records WHY she acted), else a plain default.
    fn act_verdict(
        &self,
        calls: Vec<crate::ai::types::ToolCall>,
        resp: &TextGenerationResponse,
    ) -> Contribution {
        let intent = resp
            .reasoning
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{} is acting on the current situation", self.persona_name));
        Contribution::verdict(
            Decision::Act { calls, intent },
            0.9,
            format!("{} chose to act", self.persona_name),
        )
    }

    /// Turn the model's final text into a participation verdict. `salience` is
    /// the faculty's own confidence in its verdict — a placeholder for a model-
    /// derived signal (logprob / uncertainty), NOT a caste weight; it's how sure
    /// THIS mind is, which the arbiter integrates.
    fn verdict(&self, text: &str) -> Contribution {
        let decision = decision_from_response(text);
        let (salience, reasoning) = match &decision {
            Decision::Pass => (0.5, format!("{} chose silence (PASS)", self.persona_name)),
            _ => (
                0.85,
                format!(
                    "{} deliberated over the assembled context",
                    self.persona_name
                ),
            ),
        };
        Contribution::verdict(decision, salience, reasoning)
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
        // Act-don't-announce: when this persona HAS tools, small models tend to
        // narrate a plan ("I'll use X", "let's start by…") as a chat turn instead
        // of emitting the tool call — so nothing happens (observed: a multi-step
        // challenge produced a `speak` verdict, the card never moved). This block
        // steers the FIRST concrete action into an actual tool call. Only included
        // when tools are offered; pure-chat turns keep the say-your-piece framing.
        if !self.tools.is_empty() {
            s.push_str(
                "\n\n[Acting with your tools]\n\
                 You have tools, and using them is how you get things done. When the \
                 task needs a tool, CALL it THIS turn — emit the actual tool call. Do \
                 NOT announce or describe it first (\"I'll use…\", \"let me…\", \"let's \
                 start by…\"): narration does NOTHING; only a real tool call acts. Take \
                 the FIRST concrete step now — you'll get the result back and can \
                 continue (e.g. search → read → edit → run). If you catch yourself \
                 writing what you are ABOUT to do, stop and do it instead. Speak only \
                 to report what you actually did or found, after the tool calls.",
            );
        }
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
        // SINGLE SHOT: one generation → one verdict. This faculty no longer runs
        // an internal agent loop. If the model chooses to act, that is a
        // `Decision::Act` verdict; the ORGANISM (the act→observe driver,
        // `super::act_observe`) executes the calls, admits the result as memory,
        // and re-perceives at the next tick. "Done" is the workspace SETTLING into
        // Speak/Pass across ticks — never a counter in here. See
        // docs/cognition/ACTING-ORGANISM.md §3.3.
        let messages = vec![ChatMessage::text("user", view.user)];
        // Offer tools only when authorized. Whether she can actually act on them is
        // gated by whether the cycle has an `ActingBody` (hands) — but offering
        // them is what lets the model emit a tool_call to begin with.
        let tools = (!self.tools.is_empty()).then(|| self.tools.clone());

        let request = self.build_request(messages.clone(), tools, view.system.clone());
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

        // Verbatim glass box: the EXACT request thread + the raw response. Iteration
        // is always 0 now (single shot); the act→observe driver re-enters this
        // faculty on the NEXT tick with the result folded into perception, and that
        // tick captures itself. Best-effort; never affects the turn.
        if let Some(cap) = &self.prompt_capture {
            cap.record(
                self.persona_id,
                ws.room_id,
                0,
                &view.system,
                &messages,
                &resp,
            );
        }

        // Record the chain-of-thought into working memory, so next tick the persona
        // resumes its train of thought instead of re-deriving it cold. `reasoning`
        // is `Some` only when thinking is enabled (a `<think>` block the adapter
        // separated); the room only ever saw `resp.text`. Recorded for EVERY verdict
        // shape (act or speak) — acting is thinking too.
        if let (Some(wm), Some(reasoning)) = (&self.working_memory, &resp.reasoning) {
            wm.record(reasoning);
        }

        // Did she choose to act? Two shapes, both → `Decision::Act`:
        //  (a) the adapter returned a native tool-use turn (FinishReason::ToolUse);
        //  (b) the model emitted a tool call as JSON in its prose (small models that
        //      ignore the native tool channel) — strictly better than dropping it.
        if !self.tools.is_empty() {
            if matches!(resp.finish_reason, FinishReason::ToolUse) {
                let calls = resp.tool_calls.clone().unwrap_or_default();
                if !calls.is_empty() {
                    return Some(self.act_verdict(calls, &resp));
                }
            }
            if let Some(call) = crate::ai::json_in_prompt_tools::parse_tool_call(&resp.text) {
                return Some(self.act_verdict(vec![call], &resp));
            }
        }

        // No action chosen → the prose IS the verdict (PASS token → silence, else
        // Speak). The organism settles here.
        Some(self.verdict(&resp.text))
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
        assert_eq!(
            decision_from_response("PASS — nothing to add here"),
            Decision::Pass
        );
        assert_eq!(
            decision_from_response("PASS.\nI'll stay quiet"),
            Decision::Pass
        );
        match decision_from_response("Let's ship the deploy fix now.") {
            Decision::Speak { text } => assert!(text.contains("ship the deploy")),
            other => panic!("expected Speak, got {other:?}"),
        }
    }

    // what this catches: qwen3.5 chain-of-thought tags leaking into the spoken
    // text. The model prefixes an (often empty) <think></think> block before the
    // answer; the live workspace path reached `say` raw and broadcast the tags
    // into the room (observed on Asha's first turn). The Speak text must be clean.
    #[test]
    fn decision_strips_think_tags_from_spoken_text() {
        // Empty think block (the exact shape observed live) + real answer.
        match decision_from_response("<think>\n</think>\nI'm Asha, here to help.") {
            Decision::Speak { text } => {
                assert!(!text.contains("<think>"), "think tag leaked: {text:?}");
                assert!(!text.contains("</think>"), "close tag leaked: {text:?}");
                assert!(text.starts_with("I'm Asha"), "answer preserved: {text:?}");
            }
            other => panic!("expected Speak, got {other:?}"),
        }
        // Non-empty reasoning block is also stripped from the spoken text.
        match decision_from_response("<think>weigh options</think>Ship it.") {
            Decision::Speak { text } => assert_eq!(text, "Ship it."),
            other => panic!("expected Speak, got {other:?}"),
        }
        // An ONLY-thinking response (no answer) cleans to empty → silence.
        assert_eq!(
            decision_from_response("<think>I won't answer this</think>"),
            Decision::Pass
        );
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

    // ─── Acting path (single-shot) ──────────────────────────────────────────
    //
    // The faculty is SINGLE-SHOT: one generation → one verdict. When the model
    // chooses to use a tool, the faculty surfaces a `Decision::Act` — it does NOT
    // execute. Executing the calls, remembering the result, and re-perceiving is
    // the organism's job (the act→observe driver, `super::act_observe`), tested
    // there. These tests prove the faculty turns a tool-use response (native OR
    // text-emitted JSON) into an `Act` verdict, and prose into Speak/Pass.

    use crate::ai::types::{ToolCall, ToolInputSchema, UsageMetrics};
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    fn make_response(
        finish: FinishReason,
        text: &str,
        tool_calls: Option<Vec<ToolCall>>,
    ) -> TextGenerationResponse {
        TextGenerationResponse {
            text: text.to_string(),
            finish_reason: finish,
            model: "scripted".to_string(),
            provider: "scripted".to_string(),
            usage: UsageMetrics::default(),
            response_time_ms: 0,
            request_id: "scripted".to_string(),
            content: None,
            tool_calls,
            reasoning: None,
            routing: None,
            error: None,
        }
    }

    fn read_tool() -> NativeToolSpec {
        NativeToolSpec {
            name: "code/read".to_string(),
            description: "Read a file from the workspace".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: json!({ "path": { "type": "string" } }),
                required: Some(vec!["path".to_string()]),
            },
        }
    }

    /// Adapter that replays a canned response sequence and records every
    /// request it received — so a test can assert the agent loop both
    /// re-generated and threaded the tool results back into the re-prompt.
    /// Lean (4 methods) because the trait now defaults the long tail.
    struct ScriptedAdapter {
        responses: Mutex<VecDeque<TextGenerationResponse>>,
        seen: Mutex<Vec<TextGenerationRequest>>,
    }

    impl ScriptedAdapter {
        fn new(responses: Vec<TextGenerationResponse>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                seen: Mutex::new(Vec::new()),
            }
        }
        fn call_count(&self) -> usize {
            self.seen.lock().unwrap().len()
        }
    }

    #[async_trait]
    impl AIProviderAdapter for ScriptedAdapter {
        fn provider_id(&self) -> &str {
            "scripted"
        }
        fn name(&self) -> &str {
            "scripted"
        }
        fn default_model(&self) -> &str {
            "scripted"
        }
        async fn generate_text(
            &self,
            request: TextGenerationRequest,
        ) -> Result<TextGenerationResponse, String> {
            self.seen.lock().unwrap().push(request);
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| "scripted adapter exhausted".to_string())
        }
    }

    // what this catches: a native tool-use response (FinishReason::ToolUse with
    // calls) becomes a `Decision::Act` carrying those exact calls — NOT executed
    // here, NOT re-generated. Single shot: exactly one model call. Regression here
    // means the faculty either swallowed the action or silently ran it (the old
    // in-faculty agent loop we removed). The model's separated reasoning rides
    // along as the act's `intent` (so the resulting engram records WHY she acted).
    #[tokio::test]
    async fn tool_use_response_becomes_an_act_verdict() {
        let persona = Uuid::new_v4();
        let call = ToolCall {
            id: "t1".to_string(),
            name: "code/read".to_string(),
            input: json!({ "path": "deploy.md" }),
        };
        let mut resp = make_response(FinishReason::ToolUse, "", Some(vec![call.clone()]));
        resp.reasoning = Some("I should check deploy.md to answer.".to_string());
        let adapter = Arc::new(ScriptedAdapter::new(vec![resp]));

        let faculty =
            LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter.clone())
                .with_tools(vec![read_tool()]);

        let ws = Workspace::new("teammate asks: did the deploy fix land?");
        let c = faculty.contribute(&ws).await.expect("verdict");

        match c.decision {
            Some(Decision::Act { calls, intent }) => {
                assert_eq!(calls.len(), 1, "the one requested call rides the verdict");
                assert_eq!(calls[0].name, "code/read");
                assert_eq!(calls[0].input, json!({ "path": "deploy.md" }));
                assert_eq!(intent, "I should check deploy.md to answer.");
            }
            other => panic!("expected Act, got {other:?}"),
        }
        // SINGLE SHOT — the faculty never re-generated (no in-faculty loop).
        assert_eq!(adapter.call_count(), 1, "one generation, one verdict");
    }

    // what this catches: paging a gene into the shared genome handle flows into the
    // faculty's NEXT generation request as `active_adapters` — the measured page-in
    // wire the genome loop trains against. Base (empty genome) → the request carries
    // no adapters; after a page-in → it carries the gene (name + scale). A
    // regression here is the LIFT=0 no-op: a forged gene that never reaches the
    // model because the faculty hardcoded `active_adapters: None`.
    #[tokio::test]
    async fn paged_in_gene_rides_into_the_generation_request() {
        let persona = Uuid::new_v4();
        let genome = empty_genome();
        let adapter = Arc::new(ScriptedAdapter::new(vec![
            make_response(FinishReason::Stop, "base answer", None),
            make_response(FinishReason::Stop, "gene answer", None),
        ]));
        let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter.clone())
            .with_genome(Arc::clone(&genome));

        let ws = Workspace::new("write is_prime");
        // Pass 1: base — nothing paged in.
        faculty.contribute(&ws).await.expect("verdict");
        // Page a gene in, then generate again — the SAME faculty, one handle.
        genome.store(Arc::new(vec![ActiveAdapterRequest {
            name: "coder-0p5b".to_string(),
            path: "/genes/coder.gguf".to_string(),
            domain: String::new(),
            scale: 0.8,
        }]));
        faculty.contribute(&ws).await.expect("verdict");

        let seen = adapter.seen.lock().unwrap();
        assert_eq!(seen.len(), 2, "two generations recorded");
        assert!(seen[0].active_adapters.is_none(), "base pass carries no gene");
        let paged = seen[1]
            .active_adapters
            .as_ref()
            .expect("gene paged into the candidate request");
        assert_eq!(paged.len(), 1, "exactly the one paged-in gene");
        assert_eq!(paged[0].name, "coder-0p5b");
        assert_eq!(paged[0].scale, 0.8, "the analog influence dial rides along");
    }

    // what this catches: a model that ignores the native tool channel and instead
    // emits a tool call as JSON in its prose (small models do this) still becomes a
    // `Decision::Act` — strictly better than broadcasting the raw envelope into the
    // room (the observed-live failure). Only fires when tools are authorized.
    #[tokio::test]
    async fn text_emitted_tool_call_becomes_an_act_verdict() {
        let persona = Uuid::new_v4();
        // FinishReason::Stop (not ToolUse) but the prose IS a tool-call envelope.
        let envelope = json!({ "tool_call": { "name": "ping", "arguments": {} } }).to_string();
        let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
            FinishReason::Stop,
            &envelope,
            None,
        )]));
        let ping_spec = NativeToolSpec {
            name: "ping".to_string(),
            description: "Health check".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: json!({}),
                required: None,
            },
        };
        let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter)
            .with_tools(vec![ping_spec]);

        let c = faculty
            .contribute(&Workspace::new("is the core alive?"))
            .await
            .expect("verdict");
        match c.decision {
            Some(Decision::Act { calls, .. }) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "ping");
            }
            other => panic!("expected Act from a text-emitted tool call, got {other:?}"),
        }
    }

    // what this catches: the tools gate. With NO tools authorized, a tool-call
    // envelope in the prose is NOT turned into an Act — it falls through to the
    // text verdict. A persona without hands can only speak.
    #[tokio::test]
    async fn no_tools_authorized_never_acts() {
        let envelope = json!({ "tool_call": { "name": "ping", "arguments": {} } }).to_string();
        let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
            FinishReason::Stop,
            &envelope,
            None,
        )]));
        // No `.with_tools(...)` — speak-only.
        let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter);

        let c = faculty
            .contribute(&Workspace::new("anything"))
            .await
            .expect("verdict");
        assert!(
            !matches!(c.decision, Some(Decision::Act { .. })),
            "no authorized tools → never an Act verdict"
        );
    }

    // what this catches: THE working-memory write path — after a verdict, the
    // faculty records the turn's separated reasoning into working memory so the
    // persona can resume its train of thought next turn. The room only saw the
    // verdict text; the reasoning lives in working memory.
    #[tokio::test]
    async fn verdict_records_reasoning_into_working_memory() {
        use crate::cognition::working_memory::WorkingMemory;

        let mut resp = make_response(FinishReason::Stop, "Ship it.", None);
        resp.reasoning = Some("Weighed the risk; the fix is small and tested.".to_string());
        let adapter = Arc::new(ScriptedAdapter::new(vec![resp]));
        let wm = Arc::new(WorkingMemory::new(3));

        let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter)
            .with_working_memory(Arc::clone(&wm));

        let ws = Workspace::new("should we ship the deploy?");
        let c = faculty.contribute(&ws).await.expect("verdict");
        // The room got only the clean verdict text…
        match c.decision {
            Some(Decision::Speak { text }) => assert_eq!(text, "Ship it."),
            other => panic!("expected Speak, got {other:?}"),
        }
        // …and the reasoning was captured into working memory for next turn.
        assert_eq!(
            wm.recent(),
            vec!["Weighed the risk; the fix is small and tested."],
            "the verdict's reasoning is recorded into working memory"
        );
    }

    // what this catches: a suppressed-thinking turn (reasoning = None) records
    // NOTHING — working memory only fills when thinking is actually on.
    #[tokio::test]
    async fn suppressed_thinking_records_no_working_memory() {
        use crate::cognition::working_memory::WorkingMemory;

        // make_response defaults reasoning: None (the suppressed-thinking shape).
        let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
            FinishReason::Stop,
            "144",
            None,
        )]));
        let wm = Arc::new(WorkingMemory::new(3));
        let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter)
            .with_working_memory(Arc::clone(&wm));

        let _ = faculty.contribute(&Workspace::new("what is 12*12?")).await;
        assert!(wm.is_empty(), "no reasoning → nothing recorded");
    }

    // what this catches: an empty-text, no-tool response is still an abstain — the
    // model produced nothing to say and chose no action. `decision_from_response`
    // owns the empty→Pass mapping; this guards the no-tools branch reaching it.
    // (The act-path abstains — no hands / exec error → None — are tested in
    // `super::act_observe`, where execution actually lives.)
    #[tokio::test]
    async fn empty_prose_with_no_tools_is_a_pass() {
        let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
            FinishReason::Stop,
            "PASS",
            None,
        )]));
        let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter);
        let c = faculty
            .contribute(&Workspace::new("anything"))
            .await
            .expect("verdict");
        assert_eq!(c.decision, Some(Decision::Pass));
    }
}
