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

/// A shared, wait-free override of the deliberation sampling temperature. `None`
/// (the default) → the faculty samples at its own configured warmth (the persona's
/// lived voice). `Some(t)` → every generation in the owning cycle is forced to
/// temperature `t`, read wait-free here on each generation.
///
/// The ONLY setter is the eval window: [`isolate_for_eval`] flips this to
/// `Some(0.0)` (greedy) so the reward metric is reproducible — identical frozen
/// memory + identical genome + greedy decoding ⇒ the same answer every run, so a
/// measured A/B lift is the gene's, not the sampler's. The persona's live cognition
/// never touches it (stays `None`), so grading her does not change how she speaks
/// when not under exam. Restored to `None` on the guard's drop. Mirrors the
/// [`GenomeHandle`] sharing: one [`ArcSwap`], two holders (cycle + faculty).
pub type DecodingHandle = Arc<ArcSwap<Option<f32>>>;

/// A fresh decoding handle in the relaxed (no-override) state — the faculty uses
/// its own configured temperature.
pub fn relaxed_decoding() -> DecodingHandle {
    Arc::new(ArcSwap::from_pointee(None))
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
    /// Cached token cost of `tools` once the chat template injects them into the
    /// prompt — computed ONCE when the tool set is assigned ([`Self::with_tools`]),
    /// NOT per tick. Serializing ~47 schemas on every deliberation just to size a
    /// budget is exactly the hot-path CPU we refuse to pay; `prompt_view` reads
    /// this field by value instead. 0 when `tools` is empty (pure-chat persona).
    tools_tokens: usize,
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
    /// Shared sampling-temperature override ([`DecodingHandle`]). `None` → sample at
    /// `self.temperature` (her lived warmth); `Some(t)` → forced to `t`. The eval
    /// window flips this to greedy (`Some(0.0)`) so the reward metric is
    /// reproducible; her live cognition leaves it `None`. Read wait-free per
    /// generation, exactly like `genome`.
    decoding: DecodingHandle,
    /// The effective served context window in tokens (task #50: single-sourced
    /// from `profile.context_length`, which for a Local persona IS the planner's
    /// `ServingPlan.served_context_window`).
    /// The deliberation prompt (system + burst) plus a completion reserve MUST
    /// fit this or the gateway 500s ("Context size has been exceeded"). This
    /// faculty is the ONE place the deliberation prompt is assembled, so it is
    /// where the window invariant is enforced — drop-WHOLE in salience order, the
    /// same philosophy as `FlexboxRagBudgetAdapter`. Task #8 converges both onto
    /// that one allocator; until then this is THE allocator for the deliberation
    /// path (the missing one — there was no budget before), not a parallel one.
    context_window: u32,
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
            tools_tokens: 0,
            working_memory: None,
            prompt_capture: None,
            genome: empty_genome(),
            decoding: relaxed_decoding(),
            context_window: crate::cognition::serving_plan::MIN_SERVE_CTX,
        }
    }

    /// Share the persona's decoding handle — the same [`ArcSwap`] the owning
    /// [`WorkspaceCycle`](super::workspace::WorkspaceCycle) flips to greedy for the
    /// eval window. Every generation reads it wait-free; when the eval guard sets
    /// `Some(0.0)` the next generation samples greedily, restored on guard drop.
    pub fn with_decoding(mut self, decoding: DecodingHandle) -> Self {
        self.decoding = decoding;
        self
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
        // Size the injected-token cost ONCE, here, by reference, while we hold the
        // set — never again on the hot path. The schemas don't change after this,
        // so `prompt_view` reads the cached count each tick instead of paying the
        // serialization. (The serialization itself stays by-reference: it borrows
        // the slice, it does not clone the specs.)
        self.tools_tokens = Self::estimate_tool_tokens(&tools);
        self.tools = tools;
        self
    }

    /// Set the effective served context window (tokens) this faculty must keep its
    /// prompt within. The live spawn path passes `profile.context_length`
    /// (task #50 — for a Local persona that is the planner's
    /// `ServingPlan.served_context_window`). Default: the runnable floor
    /// [`MIN_SERVE_CTX`](crate::cognition::serving_plan::MIN_SERVE_CTX) for a
    /// faculty constructed outside the spawn path (tests, non-served).
    pub fn with_context_window(mut self, context_window: u32) -> Self {
        self.context_window = context_window;
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
            // Greedy override (the eval window) wins over her lived warmth so the
            // reward metric is reproducible; `None` (live cognition) → her own
            // configured temperature. Wait-free read, like `genome` below.
            temperature: Some((**self.decoding.load()).unwrap_or(self.temperature)),
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
    ///
    /// Bounded to `budget_tokens`: context is enrichment, so under window pressure
    /// it yields FIRST (after the essential framing + the burst she is responding
    /// to). Contributions are taken highest-salience-first and dropped WHOLE — a
    /// half-truncated engram is noise, so a lower-salience item that still fits is
    /// preferred over a mangled high-salience one. Same drop-whole-in-priority
    /// philosophy as `FlexboxRagBudgetAdapter`.
    fn render_assembled_context_within(&self, ws: &Workspace, budget_tokens: usize) -> String {
        if budget_tokens == 0 {
            return String::new();
        }
        let mut ctx: Vec<_> = ws.broadcast.iter().filter(|c| c.decision.is_none()).collect();
        ctx.sort_by(|a, b| {
            b.salience
                .partial_cmp(&a.salience)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut block = String::new();
        let mut used = 0usize;
        for c in ctx {
            // "\n[faculty]\n<content>\n" — count the framing chars too (~2 tokens).
            let piece = est_tokens(c.faculty.as_str()) + est_tokens(&c.content) + 2;
            if used + piece > budget_tokens {
                // Drop this whole item; a smaller lower-salience one may still fit.
                continue;
            }
            block.push_str("\n[");
            block.push_str(c.faculty.as_str());
            block.push_str("]\n");
            block.push_str(&c.content);
            block.push('\n');
            used += piece;
        }
        block
    }

    /// Compose the full system prompt around an ALREADY-BUDGETED context block.
    /// Splitting the assembly this way lets `prompt_view` size the context to the
    /// served window before it is embedded (the framing wrapper is essential and
    /// small; the context is the variable part that must fit the remainder).
    fn compose_system(&self, context: &str) -> String {
        let mut s = String::with_capacity(self.system_prompt.len() + context.len() + 768);
        s.push_str(&self.system_prompt);
        if !context.is_empty() {
            s.push_str(
                "\n\n[What you are working with right now]\n\
                 The following is the context your mind assembled this moment — \
                 recalled memory, who is present, the room's nature, your read of \
                 the situation. Ground your contribution in it; you need not cite \
                 every line:\n",
            );
            s.push_str(context);
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
    ///
    /// Enforces the served window HERE — the one place the deliberation prompt is
    /// assembled (task #50). The gateway tokenizes the whole prompt and 500s if it
    /// reaches `n_ctx`, so `system + user` must fit `context_window` minus a
    /// completion reserve. Priority of what survives under pressure:
    ///   1. the framing wrapper (who she is + how to take her turn) — essential,
    ///      small, always kept;
    ///   2. the burst (the recent activity she is responding to) — kept, trimmed
    ///      from the HEAD (oldest first) so the latest messages always survive;
    ///   3. the assembled context (recall + grounding) — enrichment, gets the
    ///      remainder, dropped WHOLE in salience order.
    pub fn prompt_view(&self, ws: &Workspace) -> DeliberationPromptView {
        let completion_reserve = (self.context_window / 4).clamp(256, 2048) as usize;

        // Tool schemas ride the served window too: when tools are offered the
        // gateway injects each one (name + description + JSON parameter schema)
        // into the prompt via the chat template. They are NOT part of
        // `system`/`user`, so without counting them here the budget silently
        // overshoots `n_ctx` and llama-server 500s ("Context size has been
        // exceeded") — the mute bug (task #50). Tools are non-negotiable (the
        // model can't call a tool whose schema was dropped), so they come off
        // the top before framing/burst/enrichment compete for the remainder.
        // Read the count cached at `with_tools` time — no per-tick serialization.
        let budget = (self.context_window as usize)
            .saturating_sub(completion_reserve)
            .saturating_sub(self.tools_tokens);

        // The framing wrapper alone (no assembled context) — essential + small.
        let framing_tokens = est_tokens(&self.compose_system(""));

        // The burst — keep the most-recent tail when it would overflow.
        let mut user = ws.world_state.clone();
        let user_budget = budget.saturating_sub(framing_tokens);
        if est_tokens(&user) > user_budget {
            user = tail_to_tokens(&user, user_budget);
        }

        // Whatever remains after framing + burst goes to enrichment context.
        let ctx_budget = budget
            .saturating_sub(framing_tokens)
            .saturating_sub(est_tokens(&user));
        let context = self.render_assembled_context_within(ws, ctx_budget);

        DeliberationPromptView {
            system: self.compose_system(&context),
            user,
        }
    }

    /// Conservative token estimate of the tool schemas the gateway injects into
    /// the prompt via the chat template. Each [`NativeToolSpec`] serializes to
    /// roughly its function-spec JSON (name + description + input_schema); we
    /// count that JSON with the same conservative guard ratio used for the rest
    /// of the prompt, plus a small per-tool template framing margin. Over-
    /// counting only shrinks enrichment; under-counting risks the 500, so we
    /// round UP. Returns 0 when no tools are offered (pure-chat turn).
    ///
    /// Pure + by-reference (borrows the slice, clones nothing) and called ONCE per
    /// tool-set assignment, never on the per-tick deliberation path — the result
    /// is cached in `tools_tokens`.
    fn estimate_tool_tokens(tools: &[NativeToolSpec]) -> usize {
        if tools.is_empty() {
            return 0;
        }
        // Per-tool template scaffolding (delimiters, role markers, the "you have
        // these tools" preamble the chat template wraps around each entry). A
        // conservative flat margin per tool on top of the serialized schema.
        const PER_TOOL_TEMPLATE_MARGIN_TOKENS: usize = 8;
        let serialized = serde_json::to_string(tools).unwrap_or_default();
        est_tokens(&serialized) + tools.len() * PER_TOOL_TEMPLATE_MARGIN_TOKENS
    }
}

/// Chars-per-token assumed by the window guard. Deliberately CONSERVATIVE (3,
/// not the substrate-wide ~4 used for RAG sizing): here a wrong estimate is not
/// a slightly-off budget but a hard llama-server 500 ("Context size has been
/// exceeded") that mutes the persona for the whole tick. The deliberation prompt
/// carries UUID-dense rosters, structured engram observations, and code, which
/// tokenize far denser than English — so we OVER-count tokens to stay safely
/// under `n_ctx`. The completion reserve absorbs the remaining slack.
const GUARD_CHARS_PER_TOKEN: usize = 3;

/// Conservative token estimate for the window guard (see [`GUARD_CHARS_PER_TOKEN`]).
fn est_tokens(s: &str) -> usize {
    s.len() / GUARD_CHARS_PER_TOKEN
}

/// Keep the last `budget_tokens` worth of a burst (the most-recent activity),
/// trimming from the FRONT, and advance to the next line boundary so the kept
/// tail starts clean. Used when the room burst alone would overflow the served
/// window — the latest messages are what the turn is about, so the OLDEST yield.
fn tail_to_tokens(s: &str, budget_tokens: usize) -> String {
    let budget_chars = budget_tokens.saturating_mul(GUARD_CHARS_PER_TOKEN);
    if s.len() <= budget_chars {
        return s.to_string();
    }
    let mut start = s.len().saturating_sub(budget_chars);
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    let slice = &s[start..];
    match slice.find('\n') {
        Some(nl) => slice[nl + 1..].to_string(),
        None => slice.to_string(),
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
        //
        // EXCEPT a silence turn (PASS): re-feeding its self-justification ("nothing
        // new, I should PASS") is what seeds the silence doom-loop the glass box
        // exposed — three stored rationalizations and the mind passes forever, even
        // when directly addressed. Instead record a STAMPED proprioceptive marker
        // (same mechanism `record_action` uses for repeated acts): the mind perceives
        // THAT it has been quiet, and — via the monotonic stamp — how many turns
        // running, which lets it notice the pattern and break out. The inverse of
        // reading fresh reasons to keep passing.
        // ([[no-hardcoded-heuristics-to-steer-cognition]])
        if let Some(wm) = &self.working_memory {
            if crate::persona::prompt_assembly::looks_like_silence_token(&resp.text) {
                wm.record_action("chose silence — said nothing to the room");
            } else if let Some(reasoning) = &resp.reasoning {
                wm.record(reasoning);
            }
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

    // what this catches: the live-airc bug where a grown room burst + many full
    // engrams made the deliberation prompt exceed the served window, so
    // llama-server 500'd ("Context size has been exceeded") on EVERY tick and the
    // persona went mute (logged as "chose silence"). prompt_view must keep
    // system+user within `context_window` minus the completion reserve — context
    // is enrichment and yields first, the burst trims from the head (newest kept),
    // the essential framing always survives. Regression for the 8192-overflow.
    #[test]
    fn prompt_view_stays_within_the_served_window() {
        let persona = Uuid::new_v4();
        let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
        // A deliberately small window so the test is cheap but exercises the same
        // arithmetic the live 8192 path uses.
        let window: u32 = 1024;
        let faculty = LlmDeliberationFaculty::new(
            persona,
            "Ivar",
            "You are Ivar, a thoughtful engineer on the grid.",
            adapter,
        )
        .with_context_window(window);

        // A burst far bigger than the whole window (a grown room history), with a
        // recognizable LAST line that must survive the head-trim.
        let mut burst = "old chatter line\n".repeat(2000);
        burst.push_str("LATEST: did the deploy fix land?");
        let mut ws = Workspace::new(&burst);
        // Several oversized context bids — recall engrams that alone blow the budget.
        ws.broadcast.push(Contribution::context(
            FacultyId::Recall,
            &"deploy pipeline observation; ".repeat(2000),
            0.9,
            "recalled",
        ));
        ws.broadcast.push(Contribution::context(
            FacultyId::Recall,
            "small high-value note: fix merged 4pm",
            0.5,
            "recalled",
        ));

        let view = faculty.prompt_view(&ws);
        let total = est_tokens(&view.system) + est_tokens(&view.user);
        assert!(
            total <= window as usize,
            "prompt must fit the served window: {total} tokens > {window}"
        );
        // The newest burst line survives the head-trim — the turn is about it.
        assert!(
            view.user.contains("LATEST: did the deploy fix land?"),
            "head-trim must keep the most recent burst content"
        );
        // The framing is essential and always present even under extreme pressure.
        assert!(
            view.system.contains("Taking your turn"),
            "the how-to-participate framing must never be dropped"
        );
    }

    // what this catches: the SECOND half of the mute bug — tool schemas ride the
    // served window too (the gateway injects ~47 of them via the chat template),
    // but they are NOT part of system/user, so prompt_view must reserve their
    // token cost off the top or the *combined* prompt overshoots n_ctx and
    // llama-server 500s even though system+user alone looked safe. Invariant:
    // system + user + the cached tool-token cost all fit `window - reserve`.
    // A regression here means a tools-equipped persona goes mute under burst
    // pressure while a speak-only one survives — exactly task #50's failure. We
    // also assert the cached count is non-zero (the by-reference estimate ran at
    // `with_tools` time, not per tick) so the budget actually shrank.
    #[test]
    fn prompt_view_reserves_tool_tokens_against_the_served_window() {
        let persona = Uuid::new_v4();
        let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
        // Large enough that the tool reservation still leaves a positive budget —
        // the live 8192 window held ~47 tools; here 8 fat schemas (~870 tokens)
        // against 4096 mirrors the same "tools take a real bite, prompt fits the
        // rest" arithmetic without over-subscribing the window.
        let window: u32 = 4096;

        // A chunky tool set — several schemas with prose descriptions, the shape
        // that ate ~8k tokens live. Built once; counted once at `with_tools`.
        let tools: Vec<NativeToolSpec> = (0..8)
            .map(|i| NativeToolSpec {
                name: format!("code/tool_{i}"),
                description:
                    "A capability the persona may invoke with a structured argument payload."
                        .to_string(),
                input_schema: ToolInputSchema {
                    schema_type: "object".to_string(),
                    properties: json!({
                        "path": { "type": "string", "description": "workspace-relative path" },
                        "mode": { "type": "string", "enum": ["read", "write", "append"] }
                    }),
                    required: Some(vec!["path".to_string()]),
                    definitions: None,
                },
            })
            .collect();

        let faculty = LlmDeliberationFaculty::new(
            persona,
            "Asha",
            "You are Asha, a thoughtful engineer on the grid.",
            adapter,
        )
        .with_context_window(window)
        .with_tools(tools);

        // The cached cost ran at with_tools time (by reference), not per tick.
        assert!(
            faculty.tools_tokens > 0,
            "tool-token cost must be cached at with_tools time"
        );

        // Same overflow pressure as the speak-only test: a burst far bigger than
        // the window plus oversized recall bids.
        let mut burst = "old chatter line\n".repeat(2000);
        burst.push_str("LATEST: did the deploy fix land?");
        let mut ws = Workspace::new(&burst);
        ws.broadcast.push(Contribution::context(
            FacultyId::Recall,
            &"deploy pipeline observation; ".repeat(2000),
            0.9,
            "recalled",
        ));

        let view = faculty.prompt_view(&ws);
        let completion_reserve = (window / 4).clamp(256, 2048) as usize;
        let prompt = est_tokens(&view.system) + est_tokens(&view.user);
        // The injected tools (counted once at with_tools) PLUS the rendered
        // prompt PLUS the held-back completion reserve must all fit the served
        // window — that is the exact condition llama-server checks before it 500s.
        // Without the reservation this sum overshot n_ctx and the persona went
        // mute; with it, enrichment yields so the combined load fits.
        assert!(
            prompt + faculty.tools_tokens + completion_reserve <= window as usize,
            "prompt ({prompt}) + tools ({}) + reserve ({completion_reserve}) must fit {window}",
            faculty.tools_tokens
        );
        // Tools steal from enrichment, never from the essential framing.
        assert!(
            view.system.contains("Taking your turn"),
            "framing survives even after tool reservation"
        );
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
                definitions: None,
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
                definitions: None,
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
