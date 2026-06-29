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

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use std::fmt::Write as _;
use uuid::Uuid;

use super::persona_tools;
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
    /// The persona's authorized tool set, kept whole. Empty → the persona can only
    /// SPEAK. Non-empty → the persona can ACT. This set is NOT dumped into the
    /// prompt as full schemas (that flood — ~100 schemas, 4–5k tokens riding EVERY
    /// turn — is what starved the conversation AND overflowed `n_ctx`). Instead it
    /// feeds the compact CATALOG (`tool_catalog`, injected into the system prompt)
    /// and lets the act→observe path recognise a call by NAME. The model loads any
    /// one tool's full argument schema on demand via `commands/help`
    /// ([`describe_spec`](Self::describe_spec)) — progressive disclosure, the same
    /// shape Claude Code uses (deferred tools + a describe/search tool). An emitted
    /// call becomes a single [`Decision::Act`] verdict; the faculty does NOT run it
    /// — the act→observe driver ([`super::act_observe`]) does. Single-shot: one
    /// generation → one verdict (`Act` xor `Speak` xor `Pass`) per tick.
    tools: Vec<NativeToolSpec>,
    /// The compact tool catalog injected into the system prompt — tool names plus
    /// one-line summaries, grouped by category (see
    /// [`persona_tools::render_tool_catalog`]). Rebuilt ONCE whenever the tool set
    /// or the served window changes ([`Self::rebuild_tool_surface`]), never on the
    /// per-tick path; `compose_system` reads this string by reference. Empty when
    /// `tools` is empty (pure-chat persona). Two-tier (rich `name — summary`, or
    /// terse `category: names` if the window is tiny) so it ALWAYS fits.
    tool_catalog: String,
    /// The tools natively offered to the model each turn: the DISCOVERY PAIR —
    /// `commands/list` (filter/search the authorized surface → a small list of
    /// matching tools) + `commands/help` (load one named tool's full argument
    /// schema). Empty when the persona has no tools. Every other tool is dispatched
    /// by NAME through the JSON-in-prose path — `act_observe` maps any name back to
    /// its command, so a tool reached via `commands/list` still runs without being in
    /// this native array. This is what keeps the per-turn tool payload at TWO tiny
    /// schemas instead of the whole ~150-tool registry. Computed in
    /// [`Self::rebuild_tool_surface`].
    native_specs: Vec<NativeToolSpec>,
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
    /// Tools the persona has DEMANDED — looked up via `commands/help{name: X}` —
    /// and which are therefore offered NATIVELY (as callable function specs) on
    /// every subsequent turn, alongside the discovery pair. This is the missing
    /// "…then call it" half of progressive disclosure: native function-calling
    /// grammar-constrains an emitted tool-call to the OFFERED specs, so a tool the
    /// persona never saw natively is unreachable — she would loop forever asking
    /// `commands/help` for `code/run`, learn its schema, intend to run it ("Let me
    /// run this with code/run to verify"), and then be unable to emit the call
    /// (the glass box showed exactly this: 66 `commands/help{code/run}` lookups,
    /// 163 prose mentions of `code/run`, zero acts). Arming the looked-up tool is
    /// NOT output-steering — it makes the persona's OWN expressed intent
    /// executable; she still chooses whether to call it. Demand-driven and bounded
    /// (only tools she actually looks up), so the per-turn native payload stays
    /// small — never the ~150-schema dump that overflowed `n_ctx` and muted her.
    /// Interior-mutable because `deliberate(&self)` arms on the help turn and reads
    /// on the next; the lock is held briefly with no await across it.
    /// ([[persona-tool-loop-act-then-report]], [[no-hardcoded-heuristics-to-steer-cognition]])
    armed_tools: Mutex<HashSet<String>>,
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
            tool_catalog: String::new(),
            native_specs: Vec::new(),
            working_memory: None,
            prompt_capture: None,
            genome: empty_genome(),
            decoding: relaxed_decoding(),
            context_window: crate::cognition::serving_plan::MIN_SERVE_CTX,
            armed_tools: Mutex::new(HashSet::new()),
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

    /// Authorize a tool set — the persona can now ACT, not just speak. The full
    /// set is NOT dumped into the prompt; it feeds the compact catalog the persona
    /// browses, and the model loads any one tool's schema on demand via
    /// `commands/help`. An emitted call becomes a [`Decision::Act`] verdict the
    /// act→observe driver runs (the faculty itself never executes).
    pub fn with_tools(mut self, tools: Vec<NativeToolSpec>) -> Self {
        self.tools = tools;
        self.rebuild_tool_surface();
        self
    }

    /// Rebuild the prompt-facing tool surface — the compact catalog text and the
    /// `commands/help` native offering — from the authorized set + the served
    /// window. Called ONCE whenever either changes (tool assignment, window
    /// resize), NEVER on the per-tick path: `compose_system`/`prompt_view` read the
    /// cached `tool_catalog`/`describe_spec` by reference. The catalog gets HALF the
    /// window as its char budget (the other half is reserved for framing + burst +
    /// enrichment), two-tier-rendered so it always fits even at `MIN_SERVE_CTX`.
    fn rebuild_tool_surface(&mut self) {
        if self.tools.is_empty() {
            self.tool_catalog.clear();
            self.native_specs.clear();
            return;
        }
        let budget_chars =
            (self.context_window as usize / 2).saturating_mul(GUARD_CHARS_PER_TOKEN);
        self.tool_catalog = persona_tools::render_tool_catalog(&self.tools, budget_chars);
        self.native_specs = persona_tools::native_tool_specs();
    }

    /// Set the effective served context window (tokens) this faculty must keep its
    /// prompt within. The live spawn path passes `profile.context_length`
    /// (task #50 — for a Local persona that is the planner's
    /// `ServingPlan.served_context_window`). Default: the runnable floor
    /// [`MIN_SERVE_CTX`](crate::cognition::serving_plan::MIN_SERVE_CTX) for a
    /// faculty constructed outside the spawn path (tests, non-served).
    pub fn with_context_window(mut self, context_window: u32) -> Self {
        self.context_window = context_window;
        // The catalog's char budget scales with the window, so a window change can
        // flip the two-tier render (rich ↔ terse) — rebuild it.
        self.rebuild_tool_surface();
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
        .with_metrics(metrics_from(resp))
    }

    /// Inspect the emitted calls for a `commands/help{name: X}` lookup and ARM `X`
    /// — record it so the NEXT turn's offered native specs include `X`, making it
    /// callable. This is the demand-driven "…then call it" step: the persona has
    /// asked how to call a tool, which is the strongest possible signal she intends
    /// to use it; arming makes that intent emittable instead of silently dropped by
    /// the native-grammar lock. Pure plumbing — it changes WHAT she can call, never
    /// WHETHER she calls it. The help param is `name` (see [`CommandsHelp`]).
    fn arm_helped_tools(&self, calls: &[crate::ai::types::ToolCall]) {
        let mut newly = Vec::new();
        for c in calls {
            if c.name != super::persona_tools::TOOL_HELP_NAME {
                continue;
            }
            if let Some(target) = c.input.get("name").and_then(|v| v.as_str()) {
                let target = target.trim();
                // Don't re-arm the discovery pair itself, and never an empty name.
                if target.is_empty() || target == super::persona_tools::TOOL_HELP_NAME {
                    continue;
                }
                newly.push(target.to_string());
            }
        }
        if newly.is_empty() {
            return;
        }
        let mut armed = self.armed_tools.lock().unwrap();
        for t in newly {
            armed.insert(t);
        }
    }

    /// Turn the model's final text into a participation verdict. `salience` is
    /// the faculty's own confidence in its verdict — a placeholder for a model-
    /// derived signal (logprob / uncertainty), NOT a caste weight; it's how sure
    /// THIS mind is, which the arbiter integrates.
    fn verdict(&self, resp: &TextGenerationResponse) -> Contribution {
        let decision = decision_from_response(&resp.text);
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
        Contribution::verdict(decision, salience, reasoning).with_metrics(metrics_from(resp))
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
        // SELECTION (by salience): walk highest-salience-first and keep whole items
        // that fit the budget — a half-truncated engram is noise, so a smaller
        // lower-salience item that still fits is preferred over a mangled high one.
        let mut selected: Vec<&Contribution> = Vec::with_capacity(ctx.len());
        let mut used = 0usize;
        for c in ctx {
            // "\n[faculty]\n<content>\n" — count the framing chars too (~2 tokens).
            let piece = est_tokens(c.faculty.as_str()) + est_tokens(&c.content) + 2;
            if used + piece > budget_tokens {
                // Drop this whole item; a smaller lower-salience one may still fit.
                continue;
            }
            selected.push(c);
            used += piece;
        }
        // SERIALIZATION (by volatility): stable standing-framing FIRST (roster,
        // doctrine, map) so it lands in the cacheable KV-prefix region adjacent to
        // the static system prompt it resembles; volatile grounding (recall, working
        // memory) LAST, nearest the generation point. A stable sort preserves the
        // salience order WITHIN each tier, so attention ranking is untouched — this
        // is a pure emit-order choice that maximizes cross-turn prefix reuse AND puts
        // the live, actionable context closest to where the model writes. `false`
        // (stable) sorts before `true` (volatile). See [`Contribution::stable`].
        selected.sort_by_key(|c| u8::from(!c.stable));
        let mut block = String::new();
        for c in selected {
            block.push_str("\n[");
            block.push_str(c.faculty.as_str());
            block.push_str("]\n");
            block.push_str(&c.content);
            block.push('\n');
        }
        block
    }

    /// Compose the full system prompt around an ALREADY-BUDGETED context block.
    /// Splitting the assembly this way lets `prompt_view` size the context to the
    /// served window before it is embedded (the framing wrapper is essential and
    /// small; the context is the variable part that must fit the remainder).
    fn compose_system(&self, context: &str, directed: bool) -> String {
        let mut s = String::with_capacity(self.system_prompt.len() + context.len() + 768);
        s.push_str(&self.system_prompt);
        // NOTE ON ORDERING (KV-cache prefix reuse, measured 2026-06-23):
        // Everything pushed BEFORE the volatile `context` below forms a
        // byte-identical prefix across turns of the SAME directedness (identity +
        // how-to-take-your-turn + the 18.5KB tool catalog + silence affordance). The
        // silence affordance is the ONLY directedness-gated segment, and it sits LAST
        // among the static blocks — so toggling it (directed vs ambient) invalidates
        // only its own tail tokens, never the heavy identity/catalog prefix; and
        // within a run directedness is stable (the eval is always directed, live
        // ambient turns always undirected), so the cache holds in practice. The
        // llama.cpp server caches that prefix and re-prefills only the changed
        // tail, so the static ~5k tokens prefill ONCE per session instead of
        // ~27s every turn. The assembled `context` (recall + live situation) is
        // the ONLY volatile part, so it is appended LAST — which also puts the
        // live situation closest to the generation point (recency favors
        // instruction-following). Do NOT move volatile content above the catalog:
        // it breaks the cached prefix and every turn pays the full re-prefill.
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
             room's operating doctrine, if any) shape how you participate.{silence_tail}",
            name = self.persona_name,
            // Ambient turns carry the "you may stay silent" nudge; a turn DIRECTED at
            // her (a question put to her — eval exam, @mention, DM) drops it: silence
            // is for chatter she may let pass, not for ghosting a question asked of
            // her. See `Workspace::directed_at_self`.
            silence_tail = if directed {
                ""
            } else {
                " If you have nothing worth adding, stay silent."
            },
        );
        // Tools: a compact CATEGORY INDEX (category names + counts) plus how to
        // discover and use them. NOT every tool, NOT the schemas — both load on
        // demand: `commands/list` to search a category for tools, then `commands/help`
        // for one tool's call format (progressive disclosure, the Claude Code shape).
        // Only included when the persona has tools; pure-chat turns keep
        // say-your-piece. The `[Acting]` framing here states the truth WITHOUT a false
        // absolute: for many tasks the finished work IS the answer (write the function,
        // the prose, the design) — produce it directly; reach for a tool when the task
        // genuinely needs one (read a file, run code, search). Describing what you
        // WOULD do is not doing it — but neither is calling a tool you don't need.
        if !self.tools.is_empty() {
            s.push_str(
                "\n\n[Your tools]\n\
                 You can act, not just talk. Below is an INDEX of your tool \
                 categories with how many tools each holds — not the tools \
                 themselves. To find a tool: call `commands/list` with a `filter` \
                 (e.g. a category name or a keyword) to get the matching tools. To \
                 call one: first call `commands/help` with its exact name to see its \
                 arguments and an example, then call the tool. (`commands/list` and \
                 `commands/help` are offered to you directly; every other tool is \
                 called by name once you've found it.)\n",
            );
            s.push_str(&self.tool_catalog);
            s.push_str(
                "\n[Acting]\n\
                 Do the thing the task asks for. If the answer is something you can \
                 produce directly — a function, a piece of writing, a design — write \
                 the finished work now, in full. If it needs a tool — reading a file, \
                 running code, searching — call the tool THIS turn rather than \
                 describing what you would do; narrating a plan does not carry it out. \
                 After a tool runs you get the result back and can continue \
                 (e.g. help → call → read → run). Don't call a tool you don't need, \
                 and don't narrate one you do.",
            );
        }
        // Reuse the ONE silence contract — PASS = first-class choice to stay quiet —
        // but ONLY for an AMBIENT turn. When the turn is DIRECTED at her (a question
        // put to her: the eval exam, an @mention, a DM), the PASS escape is withheld:
        // a coder model offered "reply PASS and nothing reaches the room" takes that
        // exit even on a direct question (reproduced via glass-box replay — 0/13 on
        // the coder gym), and ghosting a question asked of you is not the same as
        // letting ambient chatter pass. She can still decline in her own words; she
        // just isn't handed the silent-PASS hatch. Withholding it is a FRAMING choice
        // over a structural addressing fact (`directed_at_self`), not a filter reading
        // her output (see [[no-hardcoded-heuristics-to-steer-cognition]]). The block
        // is the LAST static prefix segment, so toggling it costs only its own tokens
        // in KV-prefix terms — the identity/tools prefix above stays cacheable.
        if !directed {
            s.push_str(SILENCE_AFFORDANCE_BLOCK);
        }
        // VOLATILE TAIL — appended last so the static blocks above stay a stable,
        // cacheable prefix (see ORDERING note at the top of this fn). This is the
        // context the mind assembled THIS tick (recall + who's present + the
        // situation); it changes every turn, so it must come after all static
        // content or it poisons the KV-cache prefix.
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

        // The ONE natively-offered tool (`commands/help`) rides the served window
        // too: the gateway injects its function spec (name + description + schema)
        // via the chat template, outside `system`/`user`. Without counting it the
        // budget silently overshoots `n_ctx` and llama-server 400s ("exceeds context
        // size"). It is a SINGLE tiny schema (progressive disclosure — the rest of
        // the surface lives in the catalog inside `system`, already counted by
        // `framing_tokens`), so this is a few dozen tokens, not the 4–5k the old
        // full-registry dump cost. The catalog itself is part of `compose_system`,
        // so it is sized into the framing below — one accounting, not two.
        let budget = (self.context_window as usize)
            .saturating_sub(completion_reserve)
            .saturating_sub(self.describe_tool_tokens());

        // The framing wrapper alone (no assembled context) — essential + small.
        // Pass the SAME `directed` as the final compose below so the framing-token
        // estimate matches the prompt actually sent (directedness toggles the silence
        // block, which is a few dozen tokens).
        let framing_tokens = est_tokens(&self.compose_system("", ws.directed_at_self));

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
            system: self.compose_system(&context, ws.directed_at_self),
            user,
        }
    }

    /// Conservative token estimate of the ONE natively-offered tool spec
    /// (`commands/help`) the gateway injects via the chat template — its serialized
    /// function schema plus a small template framing margin. 0 when the persona has
    /// no tools (`describe_spec` is `None`). Counted with the same conservative
    /// guard ratio as the rest of the prompt (round UP — under-counting risks the
    /// 500). Cheap and pure; `describe_spec` is a single tiny schema, so this is a
    /// handful of tokens, not the old full-registry dump.
    fn describe_tool_tokens(&self) -> usize {
        const PER_TOOL_TEMPLATE_MARGIN_TOKENS: usize = 8;
        self.native_specs
            .iter()
            .map(|spec| {
                let serialized = serde_json::to_string(spec).unwrap_or_default();
                est_tokens(&serialized) + PER_TOOL_TEMPLATE_MARGIN_TOKENS
            })
            .sum()
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
        // Offer the DISCOVERY PAIR natively — `commands/list` (search/filter the
        // authorized surface → a small list) + `commands/help` (one named tool's call
        // format) — when the persona has a tool surface. The rest of the surface is
        // the compact CATEGORY INDEX inside the system prompt (progressive
        // disclosure): the persona searches a category with `commands/list`, loads a
        // tool's schema via `commands/help`, then calls it. A tool not in this native
        // array still DISPATCHES — `act_observe` resolves any call by name — and small
        // models emit those calls as JSON-in-prose, which the parse path below
        // handles. This keeps the per-turn tool payload at TWO tiny schemas instead of
        // the ~150-schema dump that overflowed `n_ctx` and muted her.
        // Offer the discovery pair PLUS any tool the persona has DEMANDED via
        // `commands/help` (armed below). Without the armed union she can look up
        // `code/run`, intend to call it, and be silently blocked by the native
        // grammar lock — the "…then call it" gap the glass box exposed. The union
        // is deduped (a tool already in the discovery pair is not re-added) and
        // bounded (only tools she actually looked up).
        let tools = {
            let mut offered = self.native_specs.clone();
            if !offered.is_empty() {
                let armed = self.armed_tools.lock().unwrap();
                for name in armed.iter() {
                    if offered.iter().any(|s| &s.name == name) {
                        continue;
                    }
                    // Fail-closed: only arm a tool that actually resolves in the
                    // registry (never fabricate a schema — [[fallbacks-are-illegal-fail-loud]]).
                    if let Some(spec) = persona_tools::spec_for_command(name) {
                        offered.push(spec);
                    }
                }
            }
            if offered.is_empty() {
                None
            } else {
                Some(offered)
            }
        };

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
                    // ARM any tool she just looked up via `commands/help{name: X}`,
                    // so the NEXT turn offers X natively and her stated intent to
                    // call it can actually be emitted (the broken "…then call it"
                    // half of progressive disclosure).
                    self.arm_helped_tools(&calls);
                    return Some(self.act_verdict(calls, &resp));
                }
            }
            if let Some(call) = crate::ai::json_in_prompt_tools::parse_tool_call(&resp.text) {
                self.arm_helped_tools(std::slice::from_ref(&call));
                return Some(self.act_verdict(vec![call], &resp));
            }
        }

        // No action chosen → the prose IS the verdict (PASS token → silence, else
        // Speak). The organism settles here.
        Some(self.verdict(&resp))
    }
}

/// Lift the speed/latency cost of one generation off the adapter response — the
/// measured wall-clock + the prompt/completion token counts. The brain stamps this
/// onto its verdict [`Contribution`] so latency and throughput leave the mind on
/// the same path as the decision, and the settle loop folds it into the per-task
/// total. Token counts are 0 when the gateway omitted `usage` (older endpoints);
/// `latency_ms` is always present (the adapter times every request).
fn metrics_from(resp: &TextGenerationResponse) -> crate::cognition::workspace::TurnMetrics {
    // The lane's PREFILL-vs-DECODE split (llama-server `timings`), when present:
    // cache_n/prompt_n is the KV-cache hit/miss, prompt_ms/predicted_ms the
    // wall-clock split that lets the harness see where Metal time actually goes.
    // Absent (cloud / older endpoints) → 0, and the breakdown rows read "n/a".
    let t = resp.timing.as_ref();
    crate::cognition::workspace::TurnMetrics {
        input_tokens: resp.usage.input_tokens,
        output_tokens: resp.usage.output_tokens,
        latency_ms: resp.response_time_ms,
        cached_tokens: t.map(|t| t.cached_tokens).unwrap_or(0),
        prefill_tokens: t.map(|t| t.prefill_tokens).unwrap_or(0),
        prefill_ms: t.map(|t| t.prefill_ms.round() as u64).unwrap_or(0),
        decode_ms: t.map(|t| t.decode_ms.round() as u64).unwrap_or(0),
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

    // what this catches: KV-prefix cache locality — session-stable standing framing
    // (roster/doctrine/map) must serialize BEFORE volatile grounding (recall) even
    // when the volatile bid scores HIGHER salience, so the stable sections sit in the
    // cacheable prefix region and don't re-prefill every turn. Without the stable-tier
    // sort, recall (0.9) would lead the block and push the roster (0.5) below it,
    // breaking the cross-turn prefix at the very first section. Regression for the
    // append-only/volatility-ordering speed work (commit 47c65891a + this one).
    #[test]
    fn stable_framing_serializes_before_higher_salience_volatile_grounding() {
        let persona = Uuid::new_v4();
        let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
        let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);

        let mut ws = Workspace::new("teammate: what's the plan?");
        // Volatile recall bids HIGH salience — would lead a pure salience-desc block.
        ws.broadcast.push(Contribution::context(
            FacultyId::Recall,
            "VOLATILE_RECALL: deploy was red, fixed 4pm",
            0.9,
            "recalled",
        ));
        // Stable standing-framing bids LOWER salience but is session-stable.
        ws.broadcast.push(
            Contribution::context(
                FacultyId::Custom("room-roster".to_string()),
                "STABLE_ROSTER: alice, bob, carol present",
                0.5,
                "framing",
            )
            .session_stable(),
        );

        // Generous budget so BOTH fit — this isolates ORDER, not truncation.
        let block = faculty.render_assembled_context_within(&ws, 4096);
        let roster_at = block.find("[room-roster]").expect("roster present");
        let recall_at = block.find("[recall]").expect("recall present");
        assert!(
            roster_at < recall_at,
            "stable framing must serialize before higher-salience volatile recall \
             (roster@{roster_at} should precede recall@{recall_at})\n{block}"
        );
    }

    // what this catches: progressive disclosure — the per-turn tool PAYLOAD is the
    // two-tool DISCOVERY PAIR (`commands/list` + `commands/help`), not the whole
    // authorized registry, and the system prompt carries only a CATEGORY INDEX, not
    // every tool. The old dump injected ~150 full schemas / one-liners (~4–5k tokens)
    // into EVERY turn, overflowing n_ctx → 400 "exceeds context size" → mute. Now the
    // surface is a tiny category index inside the system prompt + the two-tool native
    // offering, so even a huge tool set leaves system + user + the offered tools well
    // within the served window. Invariant: the category index (not tool names) rides
    // the system prompt, the native offering is exactly the discovery pair, and the
    // whole prompt + its tools + reserve fit the window. A regression means the dump
    // came back.
    #[test]
    fn tool_surface_is_a_category_index_plus_discovery_pair() {
        let persona = Uuid::new_v4();
        let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
        // A tool set whose FULL schemas would dwarf the window — the live shape
        // (whole authorized set ≫ whole slot), scaled down. With progressive
        // disclosure it no longer matters: only names+summaries ride the prompt.
        let window: u32 = 4096;
        let tools: Vec<NativeToolSpec> = (0..60)
            .map(|i| NativeToolSpec {
                name: format!("cat/command_{i}"),
                description:
                    "A registry command projected as a tool with a structured argument schema; \
                     its full parameter description rides the chat-template injection."
                        .to_string(),
                input_schema: ToolInputSchema {
                    schema_type: "object".to_string(),
                    properties: json!({
                        "path": { "type": "string", "description": "workspace-relative path" },
                        "mode": { "type": "string", "enum": ["read", "write", "append"] },
                        "limit": { "type": "integer", "description": "max rows returned" }
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

        // The native offering is exactly the DISCOVERY PAIR — `commands/list` (search)
        // + `commands/help` (call format) — regardless of how many tools are
        // authorized. Both resolve from the registry compiled into this build.
        let native: Vec<&str> = faculty.native_specs.iter().map(|s| s.name.as_str()).collect();
        assert!(
            native.contains(&persona_tools::TOOL_HELP_NAME),
            "commands/help must be offered natively: {native:?}"
        );
        assert!(
            native.contains(&"commands/list"),
            "commands/list (search) must be offered natively: {native:?}"
        );
        // Its injected cost is a handful of tokens, NOT the whole registry.
        assert!(
            faculty.describe_tool_tokens() < 512,
            "the discovery-pair tools must be tiny ({} tokens)",
            faculty.describe_tool_tokens()
        );

        // The CATEGORY INDEX rode into the system prompt — the persona drills in via
        // `commands/list` from there. The 60 tools all share category `cat`, so the
        // index is the single line `cat (60)` — NOT 60 tool names.
        let framing = faculty.compose_system("", false);
        assert!(
            framing.contains("[Your tools]") && framing.contains("cat (60)"),
            "the compact category index must be in the system prompt: {framing}"
        );
        assert!(
            !framing.contains("cat/command_0"),
            "individual tool names must NOT be dumped into the prompt (that was the bloat)"
        );

        // Under real burst pressure the whole prompt + the one tool + reserve fit
        // the served window — the exact condition llama-server checks before 400.
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
        let reserve = (window / 4).clamp(256, 2048) as usize;
        let prompt = est_tokens(&view.system) + est_tokens(&view.user);
        assert!(
            prompt + faculty.describe_tool_tokens() + reserve <= window as usize,
            "prompt ({prompt}) + describe tool ({}) + reserve ({reserve}) must fit {window}",
            faculty.describe_tool_tokens()
        );
        // The newest burst line survives, and the framing is intact.
        assert!(view.user.contains("LATEST: did the deploy fix land?"));
        assert!(view.system.contains("Taking your turn"));
    }

    // what this catches: the category index is small BY CONSTRUCTION — even a
    // 120-tool registry at the 2048-token serving floor (MIN_SERVE_CTX) renders to a
    // handful of `category (N)` entries, so the framing never alone blows past the
    // window and always leaves burst room. A regression (e.g. dumping tool names back
    // into the index) reintroduces the overflow at a tiny window.
    #[test]
    fn catalog_fits_the_minimum_serving_window() {
        let persona = Uuid::new_v4();
        let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
        let window = crate::cognition::serving_plan::MIN_SERVE_CTX; // 2048 floor
        let tools: Vec<NativeToolSpec> = (0..120)
            .map(|i| NativeToolSpec {
                name: format!("cat{}/command_{i}", i % 8),
                description: "A registry command with a one-line summary for the catalog."
                    .to_string(),
                input_schema: ToolInputSchema {
                    schema_type: "object".to_string(),
                    properties: json!({ "path": { "type": "string" } }),
                    required: Some(vec!["path".to_string()]),
                    definitions: None,
                },
            })
            .collect();
        let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter)
            .with_context_window(window)
            .with_tools(tools);

        // The catalog (inside framing) plus the completion reserve must leave room
        // for at least SOME burst — i.e. framing alone must not consume the window.
        let framing = est_tokens(&faculty.compose_system("", false));
        let reserve = (window / 4).clamp(256, 2048) as usize;
        assert!(
            framing + reserve < window as usize,
            "framing+catalog ({framing}) + reserve ({reserve}) must leave burst room in {window}"
        );
        // The index shows the 8 categories with their counts, NOT the 120 tool names
        // (tool names are reached via `commands/list`, not dumped into the prompt).
        assert!(
            faculty.tool_catalog.contains("cat0 (15)"),
            "category index must list categories with counts: {}",
            faculty.tool_catalog
        );
        assert!(
            !faculty.tool_catalog.contains("command_119"),
            "individual tool names must NOT be in the index (that was the bloat)"
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
            timing: None,
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

    // what this catches: the broken "…then call it" half of progressive disclosure.
    // When the persona looks up a tool via `commands/help{name: X}`, X must be ARMED
    // — offered NATIVELY on the next turn — or the native function-calling grammar
    // (locked to the discovery pair) makes X permanently uncallable. The glass box
    // showed exactly this failure: 66 `commands/help{code/run}` lookups, 163 prose
    // mentions of running it, zero acts. Regression here = `acts=0` returns: she can
    // discover a hand but never use it. Asserts (a) the lookup arms the tool, and
    // (b) the NEXT generation request actually carries the armed tool's spec.
    #[tokio::test]
    async fn looking_up_a_tool_arms_it_for_native_calling() {
        let persona = Uuid::new_v4();
        // The tool she demands MUST resolve in the registry for arming to offer its
        // real schema — `code/run` (her verify hand) is the canonical case.
        let demanded = "code/run";
        assert!(
            super::persona_tools::spec_for_command(demanded).is_some(),
            "precondition: {demanded} is a registered command"
        );
        let help_call = ToolCall {
            id: "h1".to_string(),
            name: super::persona_tools::TOOL_HELP_NAME.to_string(),
            input: json!({ "name": demanded }),
        };
        let adapter = Arc::new(ScriptedAdapter::new(vec![
            // Turn 1: she asks how to call code/run (native tool_use).
            make_response(FinishReason::ToolUse, "", Some(vec![help_call])),
            // Turn 2: a plain settle — we only inspect what tools turn 2 was OFFERED.
            make_response(FinishReason::Stop, "done", None),
        ]));
        // `with_tools(non-empty)` makes `native_specs` the discovery pair.
        let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter.clone())
            .with_tools(vec![read_tool()]);

        let ws = Workspace::new("solve: write add(a,b) in rust");
        let _ = faculty.contribute(&ws).await.expect("verdict");

        // (a) the lookup armed the demanded tool.
        assert!(
            faculty.armed_tools.lock().unwrap().contains(demanded),
            "looking up {demanded} via commands/help must arm it"
        );

        // (b) the NEXT turn offers it natively, so the grammar now permits the call.
        let _ = faculty.contribute(&ws).await.expect("verdict");
        let seen = adapter.seen.lock().unwrap();
        let turn2_tools = seen[1].tools.as_ref().expect("turn 2 offered tools");
        assert!(
            turn2_tools.iter().any(|s| s.name == demanded),
            "armed tool {demanded} must ride the next request's native specs (got: {:?})",
            turn2_tools.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        // The discovery pair is still there — arming UNIONS, never replaces.
        assert!(
            turn2_tools
                .iter()
                .any(|s| s.name == super::persona_tools::TOOL_HELP_NAME),
            "discovery pair survives the union"
        );
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

    // what this catches: the directedness gate on the silence affordance
    // (`Workspace::directed_at_self`). A DIRECTED turn — the eval exam, an @mention, a
    // DM — withholds the bare-PASS [Silence Option] escape so the persona does not
    // ghost a question put to her (the 0/13 coder-gym failure: a coder model takes the
    // "reply PASS, nothing reaches the room" exit on a directed question). An AMBIENT
    // turn keeps silence first-class. This is a FRAMING decision over a structural
    // addressing fact — her output is never filtered.
    #[test]
    fn directed_turn_withholds_the_silence_escape() {
        let adapter = Arc::new(ScriptedAdapter::new(vec![]));
        let faculty =
            LlmDeliberationFaculty::new(Uuid::new_v4(), "Asha", "You are Asha.", adapter);

        let ambient = faculty.prompt_view(&Workspace::new("just some room chatter"));
        assert!(
            ambient.system.contains("[Silence Option]"),
            "an ambient turn keeps the silence escape on the table"
        );
        assert!(
            ambient.system.contains("stay silent"),
            "an ambient turn keeps the soft 'stay silent' nudge"
        );

        let directed =
            faculty.prompt_view(&Workspace::new("answer me: what is 2+2?").directed(true));
        assert!(
            !directed.system.contains("[Silence Option]"),
            "a directed turn withholds the bare-PASS escape so a question is not ghosted"
        );
        assert!(
            !directed.system.contains("stay silent"),
            "and drops the soft 'stay silent' nudge on a directed turn too"
        );
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
