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
    ChatMessage, FinishReason, NativeToolSpec, TextGenerationRequest, TextGenerationResponse,
};
use crate::persona::text_analysis::clean_response;
use crate::persona::prompt_assembly::{
    looks_like_silence_token, SILENCE_AFFORDANCE_BLOCK, SILENCE_TOKEN,
};

/// Default cap on tool-use rounds in one deliberation tick. A persona that ACTS
/// loops generate→tool→generate; the bound stops a model that never stops calling
/// tools from spinning the turn forever (a turn is bounded work, not an open agent).
const DEFAULT_MAX_TOOL_ITERATIONS: usize = 4;

/// Max chars of a tool result fed back to the model — keeps the re-prompt bounded.
const TOOL_RESULT_MAX_CHARS: usize = 8_000;

/// True when EVERY call in `calls` was already executed this turn (same name +
/// args, so it would return the same result). A non-productive repeat: the model
/// is re-acting instead of reporting what it already found. The agent loop uses
/// this to stop acting and force a terminal report turn — otherwise a small model
/// burns its whole tool budget re-issuing one call and then falls silent (observed
/// live: `ping` called 4× against an identical result, then empty text → no reply).
/// Empty `calls` is NOT a repeat (there is nothing being re-issued).
fn all_calls_already_ran(
    calls: &[crate::ai::types::ToolCall],
    acted: &[(String, serde_json::Value)],
) -> bool {
    !calls.is_empty()
        && calls
            .iter()
            .all(|c| acted.iter().any(|(n, i)| n == &c.name && i == &c.input))
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
    /// model may emit tool_calls the agent loop executes. Rust-origin contracts,
    /// the executor runs them ([[commands-are-kernel-level-and-compose]]).
    tools: Vec<NativeToolSpec>,
    /// Runs the tool calls the model emits. `None` → no acting even if `tools` is
    /// set (degrades to speak-only). Injected (trait): a TS-IPC impl in production,
    /// a mock in tests — the loop is impl-agnostic.
    tool_executor: Option<Arc<dyn ToolExecutor>>,
    /// Bound on tool-use rounds per tick ([`DEFAULT_MAX_TOOL_ITERATIONS`]).
    max_tool_iterations: usize,
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
            tool_executor: None,
            max_tool_iterations: DEFAULT_MAX_TOOL_ITERATIONS,
            working_memory: None,
            prompt_capture: None,
        }
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
            active_adapters: None,
            request_id: None,
            user_id: None,
            room_id: None,
            purpose: Some("cognition/deliberation".to_string()),
            persona_id: Some(self.persona_id.to_string()),
        }
    }

    /// Context for tool execution this tick. `context_id` is the room the turn
    /// acts within (`ws.room_id`, the third ID tier) — so the persona's hands
    /// scope every command to the SAME room the turn is about, never a phantom
    /// `nil` room (the `scoped(nil)` bug). `session_id` stays nil deliberately:
    /// it is the EPHEMERAL connection instance and is NEVER load-bearing for
    /// where a tool action lands (per IDENTITY-SCOPE-PEER-LIVENESS-MODEL.md A.5,
    /// session never feeds trust or scope); it threads in only if/when a stable
    /// session token is needed, separate from context.
    fn tool_context(&self, ws: &Workspace) -> ToolExecutionContext {
        ToolExecutionContext {
            persona_id: self.persona_id,
            persona_name: self.persona_name.clone(),
            session_id: uuid::Uuid::nil(),
            context_id: ws.room_id,
            caller_context: serde_json::Value::Null,
            persona_config: super::tool_executor::PersonaMediaConfigLite {
                auto_load_media: false,
                supported_media_types: Vec::new(),
            },
        }
    }

    /// One acting round. The persona can act only when it has BOTH authorized
    /// tools AND an executor to run them (otherwise tools are inert — speak-
    /// only). When the model's response is a non-empty tool-use turn, execute
    /// the batch and thread the agent transcript (the assistant tool_use turn,
    /// then the user tool_results turn) into `messages`, returning `true` so the
    /// caller re-generates over what the tools returned.
    ///
    /// Returns `false` when there is nothing to act on — the response's text is
    /// the verdict. A tool batch that *errors* also returns `false`: we fall
    /// through to a decision over whatever text the model already produced and
    /// never fabricate an outcome ([[no-fallbacks-ever]]).
    async fn try_tool_round(
        &self,
        resp: &TextGenerationResponse,
        messages: &mut Vec<ChatMessage>,
        ws: &Workspace,
    ) -> bool {
        if self.tools.is_empty() {
            return false;
        }
        let Some(executor) = self.tool_executor.as_ref() else {
            return false;
        };
        if !matches!(resp.finish_reason, FinishReason::ToolUse) {
            return false;
        }
        let calls = resp.tool_calls.clone().unwrap_or_default();
        if calls.is_empty() {
            return false;
        }

        // Echo the assistant's tool_use turn so the re-prompt holds the calls,
        // then the results — the agent transcript the model reasons over next.
        messages.push(ChatMessage::assistant_tool_use(&calls));
        match executor
            .execute_native_batch(&calls, &self.tool_context(ws), TOOL_RESULT_MAX_CHARS)
            .await
        {
            Ok(outcome) => {
                messages.push(ChatMessage::tool_results(&outcome.results));
                true
            }
            Err(e) => {
                tracing::warn!(
                    persona = %self.persona_name,
                    error = %e,
                    "tool batch failed; falling through to text decision"
                );
                false
            }
        }
    }

    /// Terminal REPORT turn for the agent loop. The persona has ACTED (tool results
    /// are threaded into `messages`) but produced no prose — a small model offered
    /// tools tends to re-emit a call every round instead of transitioning to an
    /// answer. Withhold tools (no JSON-call affordance to re-trigger) and ask it to
    /// report what it found, forcing synthesis over the gathered results instead of
    /// falling silent. NOT a fallback: this is the loop's report phase, reached only
    /// after real tool execution produced real results.
    async fn synthesize_answer(
        &self,
        messages: &[ChatMessage],
        system_prompt: &str,
    ) -> Option<TextGenerationResponse> {
        let mut thread = messages.to_vec();
        thread.push(ChatMessage::text(
            "user",
            "You have finished using tools. Reply to the room now in your own words: \
             say what you found from the tool results above. Do NOT call another tool.",
        ));
        let request = self.build_request(thread, None, system_prompt.to_string());
        match self.adapter.generate_text(request).await {
            Ok(r) => Some(r),
            Err(e) => {
                tracing::warn!(
                    persona = %self.persona_name,
                    error = %e,
                    "synthesize-answer turn failed; abstaining"
                );
                None
            }
        }
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
                format!("{} deliberated over the assembled context", self.persona_name),
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
        // The growing message thread. Starts with the consolidated burst; each
        // acting round appends the model's tool_use turn + its results, so a
        // persona that ACTS reasons over what its tools returned before it speaks.
        let mut messages = vec![ChatMessage::text("user", view.user)];
        // Offer tools only when authorized; whether the persona can actually ACT
        // on them is gated in `try_tool_round` (it also needs an executor).
        let tools = (!self.tools.is_empty()).then(|| self.tools.clone());

        // Agent loop: generate → maybe act → re-generate, bounded per tick. A
        // round that acts threads its tool results in and loops; once the model
        // stops calling tools (or we hit the bound), the response text is the
        // verdict. A turn is bounded work, not an open-ended agent.
        let mut iterations = 0usize;
        // Tool calls already executed THIS turn (name + args). Used to detect a
        // model that re-emits a call it has already run — a stuck "act" loop that
        // never transitions to reporting (observed live: a small model called
        // `ping` 4× against the same result, then emitted empty text → silent).
        // A repeat means stop acting and report what was found.
        let mut acted_calls: Vec<(String, serde_json::Value)> = Vec::new();
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

            // Verbatim glass box: the EXACT request thread (system + messages sent
            // this round) and the raw response. Per agent-loop iteration, so tool
            // rounds are captured too. Best-effort; never affects the turn.
            if let Some(cap) = &self.prompt_capture {
                cap.record(self.persona_id, ws.room_id, iterations, &view.system, &messages, &resp);
            }

            let wants_tools = matches!(resp.finish_reason, FinishReason::ToolUse);

            // A non-productive repeat: every call in this response was already run
            // this turn with the same args (so it would return the same result).
            // The model is re-acting instead of reporting — stop acting and let the
            // terminal report turn below force a written answer.
            let repeat = wants_tools
                && resp
                    .tool_calls
                    .as_ref()
                    .is_some_and(|calls| all_calls_already_ran(calls, &acted_calls));

            if wants_tools && !repeat && iterations < self.max_tool_iterations {
                if self.try_tool_round(&resp, &mut messages, ws).await {
                    // Record ONLY calls that actually executed (results now threaded
                    // in), so `acted_calls` means "we have real results" — used both
                    // to detect a later identical repeat AND to gate the terminal
                    // report turn. A call that could NOT run (no executor) must NOT
                    // count, or an inert tool call would masquerade as gathered work.
                    if let Some(calls) = resp.tool_calls.as_ref() {
                        for c in calls {
                            acted_calls.push((c.name.clone(), c.input.clone()));
                        }
                    }
                    iterations += 1;
                    continue; // tools answered → re-generate over their results
                }
            }

            // Reached only when we are NOT continuing the act loop. Three shapes:
            //
            //  (a) `wants_tools` AND we already executed real tool calls this turn
            //      (`acted_calls` non-empty): the model is now repeating or capped
            //      and re-emitting the JSON envelope instead of reporting. Its text
            //      is `{"tool_call":...}`, NOT a spoken answer — emitting it verbatim
            //      would broadcast JSON into the room (observed live). Run a terminal
            //      REPORT turn with tools withheld so the model must synthesize prose
            //      over the results it gathered; if even that comes back empty / still
            //      a tool call, abstain (never fabricate a PASS).
            //
            //  (b) `wants_tools` but NOTHING executed (`acted_calls` empty — no
            //      executor, so the tool call is inert): honor any REAL verdict text
            //      the model gave alongside the inert call (e.g. "PASS" → silence); if
            //      it left only the tool-call JSON or empty text, abstain.
            //
            //  (c) `!wants_tools` — the model produced prose. That IS the verdict.
            if wants_tools && !acted_calls.is_empty() {
                if let Some(answer) = self.synthesize_answer(&messages, &view.system).await {
                    let clean = answer.text.trim();
                    // Guard: a habit-driven model may STILL emit the tool-call envelope
                    // even with tools withheld. Never broadcast that — only real prose
                    // counts as the report.
                    let is_tool_json =
                        crate::ai::json_in_prompt_tools::parse_tool_call(&answer.text).is_some();
                    if !clean.is_empty() && !is_tool_json {
                        if let (Some(wm), Some(reasoning)) =
                            (&self.working_memory, &answer.reasoning)
                        {
                            wm.record(reasoning);
                        }
                        return Some(self.verdict(&answer.text));
                    }
                }
                tracing::warn!(
                    persona = %self.persona_name,
                    iterations,
                    max = self.max_tool_iterations,
                    acted_calls = acted_calls.len(),
                    "deliberation: model kept acting and produced no report prose even \
                     after the terminal synthesize turn; abstaining this tick (no \
                     fabricated PASS, no broadcasting raw tool-call JSON)"
                );
                return None;
            }

            // `wants_tools` but nothing executed (inert tool call), OR the model's
            // text IS only the tool-call JSON: that JSON is not a spoken answer.
            // Abstain rather than broadcast it. A REAL verdict alongside the inert
            // call (non-empty, non-JSON text) falls through below to be honored.
            if wants_tools {
                let is_tool_json =
                    crate::ai::json_in_prompt_tools::parse_tool_call(&resp.text).is_some();
                if resp.text.trim().is_empty() || is_tool_json {
                    tracing::warn!(
                        persona = %self.persona_name,
                        iterations,
                        "deliberation: model wanted a tool it could not run and left no \
                         verdict text; abstaining this tick (no fabricated PASS)"
                    );
                    return None;
                }
            }

            // Record the chain-of-thought that produced THIS verdict into working
            // memory, so next turn the persona resumes its train of thought instead
            // of re-deriving it cold. `reasoning` is `Some` only when thinking is
            // enabled (the adapter separated a `<think>` block); suppressed turns
            // record nothing. The room only ever saw `resp.text` — reasoning lives
            // in working memory + the harness, never the wire.
            if let (Some(wm), Some(reasoning)) = (&self.working_memory, &resp.reasoning) {
                wm.record(reasoning);
            }
            return Some(self.verdict(&resp.text));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;

    // what this catches: the agent-loop repeat guard that stops a persona from
    // burning its whole tool budget re-issuing one call and then falling silent
    // (regression for the live "ping ×4 → empty → no reply" loop, 2026-06-22). A
    // call counts as a repeat ONLY when the same name AND the same args already
    // ran this turn; a new name, new args, or a fresh (empty-`acted`) call is NOT
    // a repeat and must still be allowed to execute.
    #[test]
    fn repeat_guard_flags_only_an_already_run_call() {
        let call = |name: &str, args: serde_json::Value| ToolCall {
            id: "x".into(),
            name: name.into(),
            input: args,
        };
        let ran = vec![("ping".to_string(), serde_json::json!({}))];

        // exact repeat (same name + same args) → stop acting
        assert!(all_calls_already_ran(&[call("ping", serde_json::json!({}))], &ran));
        // same name, DIFFERENT args → not a repeat (could return something new)
        assert!(!all_calls_already_ran(
            &[call("ping", serde_json::json!({"message": "hi"}))],
            &ran
        ));
        // different tool entirely → not a repeat
        assert!(!all_calls_already_ran(&[call("code/read", serde_json::json!({}))], &ran));
        // first time we have run nothing → never a repeat (must execute)
        assert!(!all_calls_already_ran(&[call("ping", serde_json::json!({}))], &[]));
        // a batch is a repeat only if EVERY call already ran (one fresh call ⇒ act)
        assert!(!all_calls_already_ran(
            &[call("ping", serde_json::json!({})), call("code/read", serde_json::json!({}))],
            &ran
        ));
        // empty call set is not a repeat (nothing is being re-issued)
        assert!(!all_calls_already_ran(&[], &ran));
    }

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

    // ─── Acting path (the agent loop) ───────────────────────────────────────
    //
    // These prove the speaks→ACTS capability end-to-end with deterministic
    // doubles: a scripted adapter (canned response sequence) + a recording
    // executor (captures the calls it ran). The doubles are now LEAN — the
    // AIProviderAdapter default impls mean ScriptedAdapter is 4 methods, which
    // is exactly the friction that previously made this test heavy enough to
    // defer (task #15).

    use super::super::tool_executor::{NativeBatchOutcome, ParsedToolBatch, ToolError, ToolOutcome};
    use crate::ai::types::{
        ContentPart, MessageContent, ToolCall, ToolInputSchema, ToolResult as NativeToolResult,
        UsageMetrics,
    };
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

    /// Executor that records the calls it ran and returns one canned result
    /// per call. `parse_response`/`store_outcome` are unreachable: the agent
    /// loop consumes native `tool_calls` and never parses or stores here.
    struct RecordingExecutor {
        calls: Mutex<Vec<ToolCall>>,
        /// The contextId (room) the executor was handed — proves tool calls are
        /// scoped to the turn's room, not a phantom nil.
        seen_context: Mutex<Option<Uuid>>,
        result_content: String,
    }

    #[async_trait]
    impl ToolExecutor for RecordingExecutor {
        async fn execute_native_batch(
            &self,
            calls: &[ToolCall],
            ctx: &ToolExecutionContext,
            _max_result_chars: usize,
        ) -> Result<NativeBatchOutcome, ToolError> {
            *self.seen_context.lock().unwrap() = Some(ctx.context_id);
            self.calls.lock().unwrap().extend_from_slice(calls);
            let results = calls
                .iter()
                .map(|c| NativeToolResult {
                    tool_use_id: c.id.clone(),
                    content: self.result_content.clone(),
                    is_error: None,
                })
                .collect();
            Ok(NativeBatchOutcome {
                results,
                media: Vec::new(),
                stored_ids: Vec::new(),
            })
        }
        async fn parse_response(
            &self,
            _response_text: &str,
            _model_family: Option<&str>,
        ) -> Result<ParsedToolBatch, ToolError> {
            unreachable!("agent loop consumes native tool_calls, never parse_response")
        }
        async fn store_outcome(
            &self,
            _outcome: &ToolOutcome,
            _context: &ToolExecutionContext,
        ) -> Result<uuid::Uuid, ToolError> {
            unreachable!("deliberation loop does not store outcomes")
        }
    }

    // what this catches: the full speaks→ACTS loop. Model asks for a tool →
    // the executor runs it → the result is threaded back → the model
    // re-generates → its post-tool text becomes the Speak verdict. Regression
    // here means a persona that can't actually act on what its tools returned.
    #[tokio::test]
    async fn persona_acts_then_speaks_threading_tool_results() {
        let persona = Uuid::new_v4();
        let call = ToolCall {
            id: "t1".to_string(),
            name: "code/read".to_string(),
            input: json!({ "path": "deploy.md" }),
        };
        let adapter = Arc::new(ScriptedAdapter::new(vec![
            // Turn 1: the model calls a tool.
            make_response(FinishReason::ToolUse, "", Some(vec![call])),
            // Turn 2 (after the tool answers): the model speaks its conclusion.
            make_response(
                FinishReason::Stop,
                "Per deploy.md, the fix merged at 4pm — we're green.",
                None,
            ),
        ]));
        let executor = Arc::new(RecordingExecutor {
            calls: Mutex::new(Vec::new()),
            seen_context: Mutex::new(None),
            result_content: "deploy.md: fix merged 4pm, pipeline green".to_string(),
        });

        let faculty =
            LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter.clone())
                .with_tools(vec![read_tool()])
                .with_tool_executor(executor.clone());

        let ws = Workspace::new("teammate asks: did the deploy fix land?");
        let c = faculty.contribute(&ws).await.expect("verdict");

        // Spoke the model's FINAL (post-tool) text — proves it re-generated.
        match c.decision {
            Some(Decision::Speak { text }) => assert!(
                text.contains("merged at 4pm"),
                "expected the post-tool conclusion, got: {text}"
            ),
            other => panic!("expected Speak with post-tool text, got {other:?}"),
        }
        // Ran exactly the one authorized tool the model asked for.
        {
            let ran = executor.calls.lock().unwrap();
            assert_eq!(ran.len(), 1, "exactly one tool call executed");
            assert_eq!(ran[0].name, "code/read");
        }
        // Two model calls: initial (→tool_use) + re-prompt (→text).
        assert_eq!(adapter.call_count(), 2, "loop acted then re-generated");
        // The re-prompt carried the tool RESULT back to the model (the agent
        // transcript: assistant tool_use, then user tool_result).
        let seen = adapter.seen.lock().unwrap();
        let reprompt = &seen[1];
        let threaded = reprompt.messages.iter().any(|m| {
            matches!(&m.content, MessageContent::Parts(parts)
                if parts.iter().any(|p| matches!(p,
                    ContentPart::ToolResult { content, .. } if content.contains("pipeline green"))))
        });
        assert!(
            threaded,
            "re-prompt must thread the tool result back to the model"
        );
    }

    // what this catches: the never-stops-acting regression (live, 2026-06-22). A
    // small model re-issued the SAME tool call every round (ping ×4) against an
    // identical result, exhausted its budget, then emitted empty text → it fell
    // silent (or worse, broadcast the raw {"tool_call":…} JSON). The loop must (1)
    // detect the non-productive repeat and stop acting, then (2) run a terminal
    // REPORT turn with tools WITHHELD so the model synthesizes a prose answer over
    // the result it already gathered — never silence, never raw JSON in the room.
    #[tokio::test]
    async fn repeated_tool_call_triggers_terminal_report_not_silence() {
        let persona = Uuid::new_v4();
        let ping = || ToolCall {
            id: "p".to_string(),
            name: "ping".to_string(),
            input: json!({}),
        };
        let adapter = Arc::new(ScriptedAdapter::new(vec![
            // Round 1: call ping (executes, result threaded in).
            make_response(FinishReason::ToolUse, "", Some(vec![ping()])),
            // Round 2: call ping AGAIN, identical → detected as a repeat, stop acting.
            make_response(FinishReason::ToolUse, "", Some(vec![ping()])),
            // Terminal REPORT turn (tools withheld): the model finally speaks.
            make_response(FinishReason::Stop, "The core is alive; round-trip 0ms.", None),
        ]));
        let executor = Arc::new(RecordingExecutor {
            calls: Mutex::new(Vec::new()),
            seen_context: Mutex::new(None),
            result_content: "{\"ok\":true,\"roundTripMs\":0}".to_string(),
        });
        let ping_spec = NativeToolSpec {
            name: "ping".to_string(),
            description: "Health check".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: json!({}),
                required: None,
            },
        };

        let faculty =
            LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter.clone())
                .with_tools(vec![ping_spec])
                .with_tool_executor(executor.clone());

        let ws = Workspace::new("teammate asks: is the core alive?");
        let c = faculty.contribute(&ws).await.expect("verdict");

        // The persona SPOKE a prose report — not silence, not the raw tool JSON.
        match c.decision {
            Some(Decision::Speak { text }) => {
                assert!(text.contains("alive"), "expected the prose report, got: {text}");
                assert!(
                    crate::ai::json_in_prompt_tools::parse_tool_call(&text).is_none(),
                    "must never broadcast a raw tool-call envelope: {text}"
                );
            }
            other => panic!("expected a spoken report after the repeat, got {other:?}"),
        }
        // ping executed exactly ONCE — the identical repeat was NOT re-run.
        assert_eq!(executor.calls.lock().unwrap().len(), 1, "the repeat must not re-execute");
        // Three model calls: act, repeat (caught), terminal report. The terminal
        // report turn must withhold tools so the model can't re-call.
        assert_eq!(adapter.call_count(), 3, "act + repeat + terminal report");
        let seen = adapter.seen.lock().unwrap();
        assert!(
            seen[2].tools.as_ref().map_or(true, |t| t.is_empty()),
            "terminal report turn must withhold tools"
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

    // what this catches: the nil-scope bug — tool calls MUST be scoped to the
    // room the turn is for (ws.room_id = contextId), not Uuid::nil() (a phantom
    // room). Regression here = a persona's hands act in the wrong room, which is
    // both a correctness bug and a security one (commands land outside the
    // authorized context). See IDENTITY-SCOPE-PEER-LIVENESS-MODEL.md A.6 step 3.
    #[tokio::test]
    async fn tool_calls_are_scoped_to_the_turns_room() {
        let persona = Uuid::new_v4();
        let room = Uuid::new_v4();
        let call = ToolCall {
            id: "t1".to_string(),
            name: "code/read".to_string(),
            input: json!({ "path": "deploy.md" }),
        };
        let adapter = Arc::new(ScriptedAdapter::new(vec![
            make_response(FinishReason::ToolUse, "", Some(vec![call])),
            make_response(FinishReason::Stop, "done", None),
        ]));
        let executor = Arc::new(RecordingExecutor {
            calls: Mutex::new(Vec::new()),
            seen_context: Mutex::new(None),
            result_content: "ok".to_string(),
        });
        let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter)
            .with_tools(vec![read_tool()])
            .with_tool_executor(executor.clone());

        // Run the turn IN a specific room — the contextId the persona acts within.
        let ws = Workspace::in_room("teammate asks: status?", room);
        faculty.contribute(&ws).await.expect("verdict");

        let seen = *executor.seen_context.lock().unwrap();
        assert_eq!(
            seen,
            Some(room),
            "tool execution must be scoped to the turn's room (contextId), not nil"
        );
        assert_ne!(seen, Some(Uuid::nil()), "must not be the phantom nil room");
    }

    // what this catches: the can_act gate. Tools authorized but NO executor
    // wired → the persona stays speak-only (tools inert), backward-compatible.
    // The loop must NOT attempt a tool round: a single generate, decision taken
    // from that one response. Guards against tools accidentally acting without
    // an executor present.
    #[tokio::test]
    async fn tools_without_executor_stay_speak_only() {
        let persona = Uuid::new_v4();
        let call = ToolCall {
            id: "t1".to_string(),
            name: "code/read".to_string(),
            input: json!({}),
        };
        let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
            FinishReason::ToolUse,
            "PASS",
            Some(vec![call]),
        )]));
        // Tools authorized, but no executor injected.
        let faculty =
            LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter.clone())
                .with_tools(vec![read_tool()]);

        let ws = Workspace::new("anything");
        let c = faculty.contribute(&ws).await.expect("verdict");

        assert_eq!(c.decision, Some(Decision::Pass));
        assert_eq!(
            adapter.call_count(),
            1,
            "no executor → no tool round, single generate"
        );
    }

    // what this catches: the fabricated-silence regression. A model that wants
    // to act but CAN'T (no executor / tool-iteration cap) and leaves NO verdict
    // text must ABSTAIN, not be turned into a chosen PASS. Before the fix,
    // decision_from_response("") → Pass silently converted a cut-off model into
    // "chose silence" — violating the no-fallbacks doctrine (silence is the
    // model's choice, never a substrate artifact).
    #[tokio::test]
    async fn tool_want_with_empty_text_and_no_executor_abstains() {
        let persona = Uuid::new_v4();
        let call = ToolCall {
            id: "t1".to_string(),
            name: "code/read".to_string(),
            input: json!({}),
        };
        // Model wants a tool but emits NO verdict text — and no executor exists.
        let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
            FinishReason::ToolUse,
            "",
            Some(vec![call]),
        )]));
        let faculty =
            LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter.clone())
                .with_tools(vec![read_tool()]);

        let ws = Workspace::new("anything");
        // Abstain (no contribution) — NOT a fabricated Pass.
        assert!(
            faculty.contribute(&ws).await.is_none(),
            "model cut off with no text must abstain, not fabricate a PASS"
        );
        assert_eq!(adapter.call_count(), 1, "single generate, no tool round");
    }
}
