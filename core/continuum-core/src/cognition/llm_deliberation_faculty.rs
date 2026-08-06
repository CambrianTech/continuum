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

use crate::cognition::parroted_perception;
use std::sync::Arc;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use std::collections::BTreeSet;
use uuid::Uuid;

use super::deliberation_budget::{est_tokens, tail_to_tokens, turn_message_line_addressed};
use super::deliberation_parse::decision_from_response;
use super::deliberation_prompt;
use super::persona_tools;
use super::workspace::{Contribution, Decision, Faculty, FacultyId, Workspace};
use crate::ai::adapter::AIProviderAdapter;
use crate::ai::types::{
    ActiveAdapterRequest, ChatMessage, FinishReason, NativeToolSpec, TextGenerationRequest,
    TextGenerationResponse,
};

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

/// The live model binding for the deliberation faculty: the inference adapter,
/// the model to request, and the served context window — the three facts that
/// MUST move together when the served model re-homes. A `serving/pin` swap (or a
/// grid failover onto another node) means a new adapter (new `base_url` /
/// `default_model`), a new requested model (the single-resident guard checks it),
/// AND a new context window (the new GGUF's `n_ctx`) — as ONE atomic unit. A torn
/// read (new model + stale window, or new adapter + stale model) is exactly the
/// silent wrong-brain that [[fallbacks-are-illegal-fail-loud]] forbids, so all
/// three ride ONE [`ArcSwap`], loaded once per turn.
pub struct ModelBinding {
    /// The inference adapter, leased per call. `Arc` so the whole cycle shares the
    /// one served-model backend (one base model, N persona lanes — the
    /// INFERENCE-LANES-REALISTIC shape).
    pub adapter: Arc<dyn AIProviderAdapter>,
    /// Which model to ask for (`None` → the adapter's own default). The gateway's
    /// single-resident guard checks this against the resident model and refuses a
    /// mismatch LOUD, so it must move in lockstep with `adapter`.
    pub model: Option<String>,
    /// The effective served context window in tokens (task #50). The deliberation
    /// prompt + completion reserve MUST fit this or the gateway 500s; it is sized
    /// from the SAME binding so a re-home to a smaller window can never leave the
    /// prompt sized for the old one.
    pub context_window: u32,
}

/// A shared, wait-free model binding — the SAME [`ArcSwap`] the faculty reads on
/// every generation and the owning
/// [`WorkspaceCycle`](super::workspace::WorkspaceCycle) re-homes when the served
/// model changes. One `ArcSwap`, two holders — exactly like [`GenomeHandle`] and
/// [`DecodingHandle`]. A re-home stores a fresh [`ModelBinding`]; the faculty's
/// next generation reads it, and the genome + working memory + admission are
/// carried across untouched (no cycle rebuild). See
/// [[seamless-persona-failover-model-and-genome]].
pub type ModelBindingHandle = Arc<ArcSwap<ModelBinding>>;

/// Build a shared model-binding handle from its parts.
pub fn model_binding(
    adapter: Arc<dyn AIProviderAdapter>,
    model: Option<String>,
    context_window: u32,
) -> ModelBindingHandle {
    Arc::new(ArcSwap::from_pointee(ModelBinding {
        adapter,
        model,
        context_window,
    }))
}

/// Default sampling temperature for deliberation — enough warmth for natural
/// voice, not so much it drifts.
const DEFAULT_TEMPERATURE: f32 = 0.7;

/// The reasoner faculty. Persona-scoped; shared model backend.
pub struct LlmDeliberationFaculty {
    persona_id: Uuid,
    persona_name: String,
    /// The persona's identity / deliberation system prompt (from RAG identity).
    system_prompt: String,
    /// The live model binding — the served adapter, the requested model, and the
    /// served context window, swapped ATOMICALLY as one unit when the served model
    /// re-homes (`serving/pin` or grid failover). Shared with the owning
    /// [`WorkspaceCycle`](super::workspace::WorkspaceCycle) — one [`ArcSwap`], two
    /// holders, exactly like `genome`/`decoding`. Read wait-free ONCE per turn (in
    /// [`Self::contribute`]) so an entire generation sees a consistent
    /// {adapter, model, window} triple even if a re-home lands mid-turn. See
    /// [`ModelBinding`].
    binding: ModelBindingHandle,
    temperature: f32,
    /// The persona's authorized tool set, kept whole. Empty → the persona can only
    /// SPEAK. Non-empty → the persona can ACT. This set is NOT dumped into the
    /// prompt as full schemas (that flood — ~100 schemas, 4–5k tokens riding EVERY
    /// turn — is what starved the conversation AND overflowed `n_ctx`). Instead it
    /// feeds the compact bookmarked MENU ([`persona_tools::render_tool_menu`], built
    /// per turn in [`Self::compose_system`]) and lets the act→observe path recognise a
    /// call by NAME. The model loads any
    /// one tool's full argument schema on demand via `commands/help`
    /// ([`describe_spec`](Self::describe_spec)) — progressive disclosure, the same
    /// shape Claude Code uses (deferred tools + a describe/search tool). An emitted
    /// call becomes a single [`Decision::Act`] verdict; the faculty does NOT run it
    /// — the act→observe driver ([`super::act_observe`]) does. Single-shot: one
    /// generation → one verdict (`Act` xor `Speak` xor `Pass`) per tick.
    tools: Vec<NativeToolSpec>,
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
    /// Where this mind reports what its turns actually COST — the measurement the
    /// served window is provisioned from ([`super::working_set`]).
    ///
    /// This faculty is the only place that knows a turn's TRUE size, because it is
    /// the place that assembles the prompt and therefore the place that sees what it
    /// had to throw away: the conversation before newest-first trimming, and every
    /// grounding contribution offered including the ones that did not fit. Downstream
    /// of here that information is gone, which is why the serving planner had to
    /// guess with a constant.
    ///
    /// `None` in tests and in any path with no serving planner to inform — recording
    /// is then simply skipped. Shared (cheap clone) with the serving daemon that reads
    /// the ceiling; deliberately NOT a process global, because that value feeds a
    /// decision and a global read inside a decision makes tests order-dependent.
    working_set: Option<super::working_set::WorkingSetRegistry>,
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
            // A private throwaway binding at the runnable floor — the live spawn
            // path immediately REPLACES it with the shared handle via
            // `with_model_binding` (so a `serving/pin` re-home is seen here), and
            // `with_model`/`with_context_window` mutate it for tests. Mirrors how
            // `new` builds an `empty_genome()` that `with_genome` then shares in.
            binding: model_binding(
                adapter,
                None,
                crate::cognition::serving_plan::MIN_SERVE_CTX,
            ),
            temperature: DEFAULT_TEMPERATURE,
            tools: Vec::new(),
            native_specs: Vec::new(),
            working_memory: None,
            prompt_capture: None,
            genome: empty_genome(),
            decoding: relaxed_decoding(),
            working_set: None,
        }
    }

    /// Share the registry this mind reports its turn demand into — the measurement
    /// `serving_plan` provisions the window from. Wired by the live spawn path; absent
    /// in tests, where there is no planner to inform.
    pub fn with_working_set(mut self, registry: super::working_set::WorkingSetRegistry) -> Self {
        self.working_set = Some(registry);
        self
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

    /// Share the persona's model binding — the same [`ArcSwap`] the owning
    /// [`WorkspaceCycle`](super::workspace::WorkspaceCycle) re-homes when the
    /// served model changes. REPLACES the throwaway handle `new` built, so after
    /// this call a `rebind_model` on the cycle takes effect on the faculty's next
    /// generation — exactly like `with_genome`/`with_decoding`. See
    /// [[seamless-persona-failover-model-and-genome]].
    pub fn with_model_binding(mut self, binding: ModelBindingHandle) -> Self {
        self.binding = binding;
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

    pub fn with_model(self, model: impl Into<String>) -> Self {
        let model = model.into();
        // Mutate the shared binding in place (keep adapter + window, set model) so
        // the {adapter, model, window} triple stays a single atomic unit.
        self.binding.rcu(|cur| ModelBinding {
            adapter: Arc::clone(&cur.adapter),
            model: Some(model.clone()),
            context_window: cur.context_window,
        });
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

    /// Rebuild the native `commands/list` + `commands/help` offering (the DISCOVERY
    /// PAIR) from the authorized set. Called ONCE whenever the tool set changes (tool
    /// assignment), NEVER on the per-tick path. The prompt-facing tool MENU is no
    /// longer cached here: it is an EXPANDABLE BOOKMARKED render
    /// ([`persona_tools::render_tool_menu`]) whose per-category expansion depends on
    /// what the persona is doing THIS turn, so `compose_system` builds it per-tick
    /// from `self.tools` + the turn's `expanded` set (computed once in
    /// [`Self::prompt_view`] — see [`Self::expanded_categories`]). The menu is small
    /// by construction (a spine of headers + a few opened categories), so it needs no
    /// window-budget reflow.
    fn rebuild_tool_surface(&mut self) {
        if self.tools.is_empty() {
            self.native_specs.clear();
            return;
        }
        // Offer the working set in the WIRE DIALECT, charset-legal per the OpenAI
        // function-name spec our slashed names violate. WHICH name a command is
        // offered under is a per-model POLICY ([`tool_dialect::offer_style_for`],
        // the adaptive-surface seam): our canonical `code_read` to converge a model
        // onto our namespace, or its trained reflex `read_file` to meet its tuning.
        // Either way calls map back to canonical commands on return (ONE section:
        // [`crate::cognition::tool_dialect`]). [[joel-boundary-design-values]]
        let style = crate::cognition::tool_dialect::offer_style_for(
            self.binding.load().model.as_deref(),
        );
        // The native surface is the DERIVED, bounded agentic core — every command that
        // declares `native: true` at its own site (~a dozen tools), projected once by
        // `native_tool_specs()`. It is offered in FULL, always, in the model's wire dialect.
        //
        // NO second filter here. There used to be a window-vs-surface CLIFF that amputated
        // this set to a two-tool "discovery pair" on a tight window. That is a clamp, and the
        // worst kind: a native-tool-call model can ONLY emit calls for tools in its offered
        // specs (the "long tail reachable by name" is a text-model affordance it lacks), so the
        // discovery-pair-only surface stranded it in a `commands/help(code/write)` loop and
        // wrote nothing (glass-boxed #206). Worse, the threshold sat at the served window's
        // knife-edge, so the SAME model flipped 10/10 ↔ 0/6 on a token — benchmark noise from
        // the filter, not signal from the model.
        //
        // Fitting the surface to the window is ONE decision, and it already lives in exactly
        // one place: `prompt_view_within`, which reserves THESE specs' tokens up front
        // (`describe_tool_tokens`, counted dynamically off this very set) and trims the VOLATILE
        // context (recall/RAG) last — protecting the hands, never amputating them. Filtering the
        // tool surface a second time here, on a whim, is the replicated-logic clamp that breeds
        // brittleness and tanks benchmark hygiene. So it is gone; the budget owns the fit.
        // [[filter-once-centrally-multiple-adhoc-filters-are-clamps-that-tank-benchmarks]]
        // [[budget-at-assembly-never-clamp-the-prompt]]
        self.native_specs = persona_tools::native_tool_specs()
            .into_iter()
            .map(|s| crate::cognition::tool_dialect::to_wire_spec_with(s, style))
            .collect();
    }

    /// Set the effective served context window (tokens) this faculty must keep its
    /// prompt within. The live spawn path passes `profile.context_length`
    /// (task #50 — for a Local persona that is the planner's
    /// `ServingPlan.served_context_window`). Default: the runnable floor
    /// [`MIN_SERVE_CTX`](crate::cognition::serving_plan::MIN_SERVE_CTX) for a
    /// faculty constructed outside the spawn path (tests, non-served).
    pub fn with_context_window(mut self, context_window: u32) -> Self {
        // Mutate the shared binding in place (keep adapter + model, set window).
        self.binding.rcu(|cur| ModelBinding {
            adapter: Arc::clone(&cur.adapter),
            model: cur.model.clone(),
            context_window,
        });
        // Re-derive the surface in the (possibly new) model's wire dialect. NOT
        // window-adaptive — `rebuild_tool_surface` documents why the old
        // window-vs-surface cliff was deleted (#206): amputating the hands on a tight
        // window strands a native-call model in a help loop.
        self.rebuild_tool_surface();
        // …but the arithmetic still has to close, and if it cannot she is MUTE. Say so.
        self.warn_if_window_cannot_host_the_agentic_surface();
        self
    }

    /// A LOWER BOUND on the served window: below this, framing + the full tool surface +
    /// room to reply provably cannot coexist. Necessary, NOT sufficient — clearing it does
    /// not mean she can hear you. Measured 2026-08-06: bare framing 1419 + tools 4609 ⇒ a
    /// bound of 8040, which an 8192 window clears — and the newest message in the burst
    /// STILL does not survive, because the framing a real turn renders (expanded tool
    /// categories, perception facts) exceeds the bare floor and context is what yields.
    /// Treat this as "you are definitely broken below here", never "you are fine above it".
    /// Derived, never a magic number: the surface is measured off the specs
    /// actually offered, and the reserve is the same `/4` split
    /// [`completion_budget_for`] uses, so this cannot drift from what the budget does.
    ///
    /// `tools + framing <= window - window/4`  ⇒  `window >= ceil((tools + framing) * 4 / 3)`
    pub fn min_window_for_agentic_surface(&self) -> u32 {
        let fixed = self.describe_tool_tokens() as u32 + self.framing_floor_tokens();
        fixed.div_ceil(3).saturating_mul(4)
    }

    /// The irreducible framing cost, MEASURED — never a constant.
    ///
    /// Composes the system prompt with every optional block yielded (no expanded tool
    /// categories, no extras) and counts it. A hardcoded number here was written and
    /// immediately rejected by
    /// `context_budget::no_new_hardcoded_context_or_prompt_size_constant_anywhere_in_the_crate`,
    /// which is exactly right: a literal would be a snapshot of one day's framing that
    /// silently lies the moment the framing changes, and every "structurally mute" report
    /// after that would be measured against a stale floor. Deriving it means the requirement
    /// tracks reality for free. (#124 — de-hardcode the dynamic system.)
    fn framing_floor_tokens(&self) -> u32 {
        let bare = self.compose_system("", &std::collections::BTreeSet::new(), false, false, None);
        est_tokens(&bare) as u32
    }

    /// A citizen served below [`Self::min_window_for_agentic_surface`] provably cannot hold
    /// her framing, her tools, AND room to answer at the same time. She does not fail loudly
    /// on her own — she goes quiet, or loops asking for help she has no room to receive.
    /// That is the silent-degradation shape this codebase keeps paying for, so make it
    /// LOUD at the moment the window is set, with the arithmetic attached.
    ///
    /// This is a SENSOR, not a policy — deliberately neither a clamp nor a refusal:
    ///   - Not a clamp — amputating the tool surface to fit is exactly the deleted #206
    ///     cliff (see `rebuild_tool_surface`); it produced a model that could not act and
    ///     flipped 10/10 ↔ 0/6 on one token of window. A hardcoded threshold IS the
    ///     brittleness.
    ///   - Not a refusal — a small-window persona is still a citizen who can talk, and on
    ///     the hardware this project exists to serve (an old laptop, no cloud) a
    ///     mute-for-tools persona beats no persona.
    ///
    /// What it emits is a MEASURED DEMAND — served vs needed, with the terms broken out —
    /// which is precisely the input an actuator needs to do something intelligent: raise the
    /// lane's `-c`, re-home to a roomier model, or rebalance against the other consumers.
    /// The decision belongs to the governor, which sees the whole machine and can learn from
    /// outcomes; this faculty only knows its own arithmetic and must not pretend otherwise.
    /// Joel 2026-08-06: "daemons must be intelligent and quality-of-experience driven, not
    /// hardcoded or simplistic algorithms — ever present governors working and LEARNING."
    /// A static clamp here would be the opposite of that; a truthful number is what makes
    /// the learning possible.
    ///
    /// The measured floor as of 2026-08-06: tools 4609 + framing 1643 ⇒ **8336 tokens**.
    /// `serving_plan::MIN_SERVE_CTX` is 2048 — FOUR TIMES below it. So the substrate will
    /// happily serve a window at which no citizen can act, which is #327.
    fn warn_if_window_cannot_host_the_agentic_surface(&self) {
        if self.native_specs.is_empty() {
            return; // no hands offered — nothing to fit, nothing to warn about
        }
        let window = self.binding.load().context_window;
        let needed = self.min_window_for_agentic_surface();
        if window >= needed {
            return;
        }
        let tools = self.describe_tool_tokens();
        crate::probe!(
            class = "persona.window.cannot_host_tools",
            persona = %self.persona_name,
            window = window,
            needed = needed,
            tool_tokens = tools,
            framing_tokens = self.framing_floor_tokens(),
            reserve_tokens = Self::completion_budget_for(window),
            "served window is too small to hold framing + tools + a reply — this citizen \
             will struggle to ACT (she can still speak). Raise the window to at least \
             `needed`, or serve a model with a larger context."
        );
    }

    /// The generation budget for one deliberation turn, in tokens: the slice of
    /// the served window reserved for the model's reply, derived from the SAME
    /// `context_window` that sizes the prompt. ONE source of truth — [`prompt_view`]
    /// subtracts exactly this to bound the prompt, and [`build_request`] passes
    /// exactly this as `max_tokens`, so `prompt + completion` provably never reaches
    /// `n_ctx`. The `/4` split gives the reply up to a quarter of the real served
    /// window, floored at 256 so a tiny window is still usable. NO fixed ceiling:
    /// the old `.clamp(…, 2048)` capped every reply at ~2048 tokens even on a large
    /// window, which physically prevents writing a file bigger than ~150 lines in
    /// one turn — a direct app-scale blocker (Joel 2026-07-13: stop choking context
    /// to stupid small sizes). max_tokens is a CEILING the model stops under when
    /// done, so a generous quarter-window allowance never wastes anything; it just
    /// lets the reply be as long as the task genuinely needs.
    fn completion_budget_for(context_window: u32) -> u32 {
        (context_window / 4).max(256)
    }

    /// Build a generation request for the message thread. Centralized so the
    /// first prompt and any future re-prompt share one shape. Takes the model
    /// binding as an already-loaded snapshot (`contribute` loads it ONCE per turn)
    /// so the requested `model` and the `max_tokens` reserve derived from
    /// `context_window` come from the SAME atomic {adapter, model, window} triple —
    /// a re-home landing mid-turn can never tear the model away from its window.
    fn build_request_within(
        &self,
        binding: &ModelBinding,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<NativeToolSpec>>,
        system_prompt: String,
        stop_sequences: Option<Vec<String>>,
    ) -> TextGenerationRequest {
        TextGenerationRequest {
            messages,
            system_prompt: Some(system_prompt),
            model: binding.model.clone(),
            provider: None,
            // Greedy override (the eval window) wins over her lived warmth so the
            // reward metric is reproducible; `None` (live cognition) → her own
            // configured temperature. Wait-free read, like `genome` below.
            temperature: Some((**self.decoding.load()).unwrap_or(self.temperature)),
            // Generation is bounded to the room the prompt budget RESERVED for it —
            // `completion_budget()`, the same value `prompt_view` subtracts from the
            // served window. This is the ONE place the two must agree: the prompt is
            // sized to leave this many tokens, so the reply is allowed exactly this
            // many. Left unbounded (`None`), a verbose turn overruns the reserve,
            // `prompt + completion` reaches `n_ctx`, and llama-server (started with
            // `--embeddings`, so context-shift is off) returns 500 "Compute error" —
            // muting the persona for the whole tick. This is NOT the flat cap that
            // truncated qwen3.5 mid-`<think>`: the budget scales with the real served
            // window (up to a quarter of it), so the reply gets every token set aside
            // for it — never a const we picked, never an overrun ([[fallbacks-are-illegal-fail-loud]]).
            max_tokens: Some(Self::completion_budget_for(binding.context_window)),
            top_p: None,
            top_k: None,
            // `None` here does NOT mean "no repetition penalty" — it defers to the
            // adapter, which for llama.cpp-family gateways (selected by the TYPED
            // `llamacpp_sampling_extensions` capability, never a provider-name match)
            // forwards `repeat_penalty` to the server, defaulting to 1.1 when we pass
            // None. That 1.1 is load-bearing: at llama.cpp's 1.0 default (disabled) a
            // small model can collapse into degenerate repetition — reprinting one line
            // to the token budget for a multi-minute, truncated turn (seen in the glass
            // box). Cloud OpenAI-compat providers don't accept the non-standard field,
            // so they correctly leave it off. TODO(#76): lift the penalty (like
            // temperature) onto the Model row so it's a per-model default the faculty
            // passes through, not an adapter magic number.
            repeat_penalty: None,
            frequency_penalty: None,
            repeat_last_n: None,
            // #150: peer-name turn-boundary stops when the caller derived them
            // from the burst (`peer_stop_sequences`) — generation ends where
            // her turn ends, never continuing the transcript as a teammate.
            stop_sequences,
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
        // The model's OWN stated reasoning (a `<think>` block) when present — so the
        // engram records WHY she acted. When ABSENT, leave it EMPTY, never fabricate
        // a reason. The old default — "{name} is acting on the current situation" —
        // was content-free AND the worst mimicry fuel (#158): it rendered into every
        // receipt as "because {name} is acting…", the model imitated that template as
        // speech, and because the phrasing is identical for everyone, personas
        // cross-copied each other's names (Anwen's turn narrating "because Asha is
        // acting" — the identity bleed). An empty intent renders a receipt with no
        // "because" clause: nothing template-shaped to imitate.
        let intent = resp
            .reasoning
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_default();
        Contribution::verdict(
            Decision::Act { calls, intent },
            0.9,
            format!("{} chose to act", self.persona_name),
        )
        .with_metrics(metrics_from(resp))
        // #210: carry the verbatim generation so the glass box can attribute a fumbled
        // tool envelope to the model, not the parser. The Act's `intent` is only the
        // model's `<think>` reasoning; the raw text is the actual emitted call bytes.
        .with_raw_generation(resp.text.clone())
    }

    /// A draft that reproduces what the system just told her is not a contribution — it is
    /// the prompt coming back out. Downgrade it to silence.
    ///
    /// Measured live 2026-08-06: the repetition brick fired on Anwen, and her next room
    /// message WAS the brick, verbatim, second person intact. The mechanism built to break
    /// the loop became the loop's next turn. See [`parroted_perception`] for why this is a
    /// containment comparison against the burst's own opaque turns and emphatically NOT a
    /// reserved-word ban — a citizen discussing her own cognition must stay speakable.
    ///
    /// Silence, not stripping: a turn whose content is a reflection of its own prompt
    /// contributed nothing, and deleting the echoed span would only make it LOOK like it did.
    /// The probe is the glass box — an unexplained silence is its own defect
    /// ([[a-probe-that-can-only-fail-is-worse-than-no-probe]]), so this always says which
    /// fact was echoed.
    fn silence_a_parroted_draft(&self, decision: Decision, ws: &Workspace) -> Decision {
        let Decision::Speak { text } = &decision else {
            return decision;
        };
        let facts = parroted_perception::perception_facts(&ws.turns);
        let Some(echoed) = parroted_perception::parroted_fact(
            text,
            &facts,
            parroted_perception::PARROT_CONTAINMENT_THRESHOLD,
        ) else {
            return decision;
        };
        crate::probe!(
            class = "persona.speech.parroted_perception",
            persona = %self.persona_name,
            echoed_fact = %echoed.chars().take(120).collect::<String>(),
            draft_len = text.len(),
            "draft reproduced a perception fact she was handed — settling to silence instead \
             of speaking the prompt back into the room (#158)"
        );
        Decision::Pass
    }

    /// Turn the model's final text into a participation verdict. `salience` is
    /// the faculty's own confidence in its verdict — a placeholder for a model-
    /// derived signal (logprob / uncertainty), NOT a caste weight; it's how sure
    /// THIS mind is, which the arbiter integrates.
    fn verdict(&self, resp: &TextGenerationResponse, ws: &Workspace) -> Contribution {
        let decision = self.silence_a_parroted_draft(decision_from_response(&resp.text), ws);
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
            .with_metrics(metrics_from(resp))
            // #210: for a Speak, the raw text IS the artifact (the `<<!DOCTYPE` HTML she
            // wrote to the room / a file); carrying it verbatim lets the glass box show a
            // leading-char fumble is the model's, distinct from the parsed decision.
            .with_raw_generation(resp.text.clone())
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
    /// What ALL the grounding offered this turn would cost if none of it had to be
    /// dropped — the grounding half of a turn's demand ([`super::working_set`]).
    ///
    /// Counted over the same set `render_assembled_context_within` selects from, with
    /// the same per-item header charge, so the two agree on what a contribution costs
    /// and only disagree on how much of it survives. Measured 2026-08-06 this is the
    /// dominant term: the work board alone offered a median 5,364 tokens into a
    /// context budget with a median of 55.
    fn assembled_context_cost(ws: &Workspace) -> usize {
        ws.broadcast
            .iter()
            .filter(|c| c.decision.is_none() && !c.trailing)
            .map(|c| est_tokens(c.faculty.as_str()) + est_tokens(&c.content) + 2)
            .sum()
    }

    fn render_assembled_context_within(&self, ws: &Workspace, budget_tokens: usize) -> String {
        if budget_tokens == 0 {
            // Received-vs-rendered receipt even on the zero-budget path — a turn
            // whose entire context vanished must say so, not render silently empty
            // (the silver-harbor failure class: recall surfaced with the winning
            // bid, yet [recall] never reached the prompt; WHERE it vanished was
            // undebuggable without this seam probe).
            crate::probe!(
                class = "delib.context.render",
                persona = %self.persona_name,
                budget_tokens = 0usize,
                received = ws.broadcast.iter().filter(|c| c.decision.is_none() && !c.trailing).count(),
                rendered = 0usize,
                dropped = %ws
                    .broadcast
                    .iter()
                    .filter(|c| c.decision.is_none() && !c.trailing)
                    .map(|c| c.faculty.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
                "assembled context suppressed: zero token budget"
            );
            return String::new();
        }
        // TRAILING contributions (working-memory proprioception that grows each act)
        // are excluded HERE — they render as trailing conversation turns nearest
        // generation ([`messages_within`]), NOT in the system message, so a settle-act
        // never shifts the cacheable system prefix (#205). Only standing framing and
        // byte-stable grounding (roster, doctrine, map, recall) belong in `system`.
        let mut ctx: Vec<_> = ws
            .broadcast
            .iter()
            .filter(|c| c.decision.is_none() && !c.trailing)
            .collect();
        ctx.sort_by(|a, b| {
            b.salience
                .partial_cmp(&a.salience)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let received = ctx.len();
        // SELECTION (by salience): walk highest-salience-first and keep what fits.
        //
        // An INDIVISIBLE contribution (one part — a recalled engram, an affect
        // signal) is still kept whole or dropped whole: half an engram is noise,
        // so a smaller lower-salience item that fits is preferred over a mangled
        // high one. That rule was right, and it is unchanged.
        //
        // A DIVISIBLE one (a grounding source that delivered a LIST, and said so
        // via `Contribution::parts`) instead contributes the longest PREFIX of its
        // own units that fits. Its units are ordered by the source, leads first,
        // so a prefix is exactly the summary the source would have written if
        // asked for less — never a mid-sentence cut. This is what un-hides the
        // work board: measured 2026-08-06, `room-kanban` offered a median 5,364
        // tokens all-or-nothing into a median 55-token budget and was kept 0 of
        // 495 times, while its `[your work]` / `[available work]` leads are ~200
        // and carry every fact a citizen needs to find work. The citizens were
        // saying "there are no open tasks available" about a 61-card board.
        let mut selected: Vec<(&Contribution, String)> = Vec::with_capacity(ctx.len());
        let mut dropped: Vec<String> = Vec::new();
        let mut partial: Vec<String> = Vec::new();
        let mut used = 0usize;
        for c in ctx {
            // "\n[faculty]\n<content>\n" — count the framing chars too (~2 tokens).
            let header = est_tokens(c.faculty.as_str()) + 2;
            let piece = header + est_tokens(&c.content);
            if used + piece <= budget_tokens {
                used += piece;
                selected.push((c, c.content.clone()));
                continue;
            }
            // Whole won't fit. Divisible? Take the longest leading run of units
            // that does. `parts` is never empty (every constructor seeds it), so
            // a single-part contribution simply finds no fitting prefix and falls
            // through to the drop below — byte-identical to the old behavior.
            let mut kept_units: Vec<&str> = Vec::new();
            let mut unit_tokens = header;
            for (i, unit) in c.parts.iter().enumerate() {
                // Units are joined by "\n" — charge the separator from the second on.
                let cost = est_tokens(unit) + usize::from(i > 0);
                if used + unit_tokens + cost > budget_tokens {
                    break;
                }
                unit_tokens += cost;
                kept_units.push(unit.as_str());
            }
            if kept_units.len() == c.parts.len() {
                // Can only happen if the estimator disagrees with itself; treat as whole.
                used += unit_tokens;
                selected.push((c, c.content.clone()));
                continue;
            }
            if kept_units.is_empty() {
                dropped.push(format!(
                    "{}(sal={:.2},tok={})",
                    c.faculty.as_str(),
                    c.salience,
                    piece
                ));
                continue;
            }
            // A truncated LIST must SAY it is truncated, or the persona reads a
            // partial board as the whole board and reports work that isn't there
            // — a quieter lie than the empty block this replaces.
            let omitted = c.parts.len() - kept_units.len();
            // Name the EXACT verb, never "the matching command" — she cannot run a
            // description, and a name she has to guess is a name she gets wrong
            // ([[command-names-must-be-accurate]]). The source declares it
            // (`RagSource::expand_command`, no default impl); a source with nothing
            // more to fetch says only how much was omitted.
            let how = match c.expand_command {
                Some(cmd) => format!(" — run `{cmd}` to see all {}", c.parts.len()),
                None => String::new(),
            };
            let notice = format!("…{omitted} more not shown (context budget){how}");
            used += unit_tokens + est_tokens(&notice);
            let body = format!("{}\n{notice}", kept_units.join("\n"));
            partial.push(format!(
                "{}({}/{} units,tok={}→{})",
                c.faculty.as_str(),
                kept_units.len(),
                c.parts.len(),
                piece,
                unit_tokens
            ));
            selected.push((c, body));
        }
        // Received-vs-rendered receipt at the ONE seam where a surfaced
        // contribution can silently vanish between attention and the prompt.
        // Glass box: what recall/grounding WON upstream must be attributable
        // HERE if it never renders (#130).
        crate::probe!(
            class = "delib.context.render",
            persona = %self.persona_name,
            budget_tokens,
            used_tokens = used,
            received,
            rendered = selected.len(),
            kept = %selected
                .iter()
                .map(|(c, _)| format!("{}(sal={:.2})", c.faculty.as_str(), c.salience))
                .collect::<Vec<_>>()
                .join(","),
            dropped = %dropped.join(","),
            // Which divisible blocks contributed a PREFIX rather than their whole —
            // without this a shrunken board and a full one look identical in the log.
            partial = %partial.join(","),
            "assembled context: {}/{} contributions fit", selected.len(), received
        );
        // SERIALIZATION (by volatility): stable standing-framing FIRST (roster,
        // doctrine, map) so it lands in the cacheable KV-prefix region adjacent to
        // the static system prompt it resembles; volatile grounding (recall, working
        // memory) LAST, nearest the generation point. A stable sort preserves the
        // salience order WITHIN each tier, so attention ranking is untouched — this
        // is a pure emit-order choice that maximizes cross-turn prefix reuse AND puts
        // the live, actionable context closest to where the model writes. `false`
        // (stable) sorts before `true` (volatile). See [`Contribution::stable`].
        selected.sort_by_key(|(c, _)| u8::from(!c.stable));
        let mut block = String::new();
        for (c, body) in selected {
            block.push_str("\n[");
            block.push_str(c.faculty.as_str());
            block.push_str("]\n");
            block.push_str(&body);
            block.push('\n');
        }
        block
    }

    /// Compose the full system prompt around an ALREADY-BUDGETED context block.
    /// Splitting the assembly this way lets `prompt_view` size the context to the
    /// served window before it is embedded (the framing wrapper is essential and
    /// small; the context is the variable part that must fit the remainder).
    /// Assemble the system prompt for this turn. Thin adapter over
    /// [`deliberation_prompt::compose`]: the faculty owns the *inputs* (its identity,
    /// tools, the turn's structural flags); the framing prose + block ordering live in
    /// [`super::deliberation_prompt`] as a procedural list of named, gated blocks.
    fn compose_system(
        &self,
        context: &str,
        expanded: &BTreeSet<String>,
        directed: bool,
        self_initiated: bool,
        now_ms: Option<u64>,
    ) -> String {
        deliberation_prompt::compose(&deliberation_prompt::SystemPromptParts {
            system_prompt: &self.system_prompt,
            persona_name: &self.persona_name,
            tools: &self.tools,
            expanded,
            context,
            directed,
            now_ms,
            self_initiated,
        })
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
        // Introspection / test entry: size against the CURRENT served window. The
        // production path (`contribute`) instead loads the binding ONCE and calls
        // `prompt_view_within` with that window, so prompt sizing and the request's
        // model come from the same atomic snapshot (no torn read across a re-home).
        self.prompt_view_within(ws, self.binding.load().context_window)
    }

    /// [`prompt_view`] against an explicit served window. See that method.
    fn prompt_view_within(&self, ws: &Workspace, context_window: u32) -> DeliberationPromptView {
        // The reply's reserved room — the SAME value `build_request_within` passes
        // as `max_tokens`, so the prompt is sized to leave exactly what generation
        // is then allowed to use. One source: [`completion_budget_for`].
        let completion_reserve = Self::completion_budget_for(context_window) as usize;

        // The ONE natively-offered tool (`commands/help`) rides the served window
        // too: the gateway injects its function spec (name + description + schema)
        // via the chat template, outside `system`/`user`. Without counting it the
        // budget silently overshoots `n_ctx` and llama-server 400s ("exceeds context
        // size"). It is a SINGLE tiny schema (progressive disclosure — the rest of
        // the surface lives in the tool MENU inside `system`, already counted by
        // `framing_tokens`), so this is a few dozen tokens, not the 4–5k the old
        // full-registry dump cost. The menu itself is part of `compose_system`,
        // so it is sized into the framing below — one accounting, not two.
        let budget = (context_window as usize)
            .saturating_sub(completion_reserve)
            .saturating_sub(self.describe_tool_tokens());

        // Which tool categories the bookmarked menu OPENS this turn. Computed ONCE
        // here from `ws` (NOT from the budgeted `context`, which is not built yet) and
        // threaded IDENTICALLY into both `compose_system` calls below — so the
        // framing-token ESTIMATE (empty context) and the FINAL render (real context)
        // carry the exact same menu, and the budget math cannot under-count the tool
        // block and overshoot `n_ctx`. See [`Self::expanded_categories`].
        let expanded = self.expanded_categories(ws);

        // The framing wrapper alone (no assembled context) — essential + small.
        // Pass the SAME directedness + self-initiation + expansion as the final
        // compose below so the framing-token estimate matches the prompt actually sent
        // (both the silence block and the [Your own time] block are gated and add a
        // few dozen tokens each).
        let framing_tokens =
            est_tokens(&self.compose_system("", &expanded, ws.directed_at_self, ws.self_initiated, ws.now_ms));

        // The conversation — role-attributed turns built from `ws.turns` (own posts
        // → assistant, peers → user), kept to the most-recent tail when it would
        // overflow. The OLDEST turns yield first under pressure (the latest activity
        // is what the turn is about — the same priority the old flat head-trim had,
        // now at turn granularity).
        // RESERVE THE GROUNDING FLOOR BEFORE THE CONVERSATION FILLS.
        //
        // `messages_within` is greedy: hand it everything after framing and it takes
        // everything after framing, so grounding gets the remainder — which is zero.
        // That is not a budget-size problem and growing the window does not fix it:
        // measured 2026-08-06, the served window rose 16,384 → 24,128 and Anwen,
        // Asha and Atlas still each rendered `budget=0 kept=[]`, dropping recall,
        // roster, workspace-map AND the work board on every turn. The conversation
        // simply absorbed the increase. Supply was never the binding constraint at
        // this seam; ORDER was.
        //
        // The floor is derived, not invented: enough for the SMALLEST contribution
        // actually offered this turn, so at least one grounding fact always reaches
        // her. Bounded at half the post-framing pool so the reservation can never
        // invert the problem and starve the conversation — an even split is a
        // fairness rule between two claimants, not a tuning constant. Whatever
        // grounding does not use still flows back: `ctx_budget` below is computed
        // from what the conversation ACTUALLY consumed, not from this reservation.
        let after_framing = budget.saturating_sub(framing_tokens);
        let ctx_floor = ws
            .broadcast
            .iter()
            .filter(|c| c.decision.is_none() && !c.trailing)
            .map(|c| est_tokens(c.faculty.as_str()) + est_tokens(&c.content) + 2)
            .min()
            .unwrap_or(0)
            .min(after_framing / 2);
        let msg_budget = after_framing.saturating_sub(ctx_floor);
        let all_messages = self.messages_unfitted(ws);

        // WHAT THIS TURN WOULD HAVE COST WITH NO BUDGET — recorded before either
        // fitting step throws the evidence away, and deliberately free to exceed
        // `context_window`. That excess IS the signal: it is the only way a mind held
        // at a too-small window can ever report that it needed a bigger one, and the
        // measurement `serving_plan` provisions from (see [`super::working_set`]).
        //
        // Measuring the prompt we actually SEND instead would measure the clamp — a
        // citizen capped at 8192 fills 8192, so a p95 of what-was-sent re-derives the
        // cap that produced it and freezes it forever. Every term below is therefore
        // the UNTRUNCATED one.
        let demand_tokens = framing_tokens
            .saturating_add(Self::messages_cost(&all_messages))
            .saturating_add(Self::assembled_context_cost(ws))
            .saturating_add(est_tokens(deliberation_prompt::WORKING_CONTEXT_HEADER))
            .saturating_add(completion_reserve)
            .saturating_add(self.describe_tool_tokens());
        if let Some(reg) = &self.working_set {
            reg.record(
                self.persona_id,
                demand_tokens.min(u32::MAX as usize) as u32,
                // `now_ms` is the workspace's own clock when the cycle stamped one;
                // 0 when it didn't. The registry keeps peaks, not a time series, so an
                // unstamped cycle still contributes its measurement honestly.
                ws.now_ms.unwrap_or(0),
            );
        }
        crate::probe!(
            class = "delib.turn.demand",
            persona = %self.persona_name,
            demand_tokens,
            context_window,
            framing_tokens,
            conversation_tokens = Self::messages_cost(&all_messages),
            grounding_tokens = Self::assembled_context_cost(ws),
            completion_reserve,
            // The honest headline: >1.0 means this turn wanted more window than it had,
            // and by how much. This is the number that decides whether the served
            // window is provisioned for the work or for a constant.
            over_window = demand_tokens as f32 / (context_window.max(1) as f32),
            "turn demand vs served window"
        );

        let messages = self.fit_messages(all_messages, msg_budget);

        // Whatever remains after framing + conversation goes to enrichment
        // context. The framing estimate above was taken with an EMPTY context,
        // where `working_context_block`'s wrapper header is absent — so the
        // moment any context renders, the system prompt grows by that header
        // too. Charge the context budget for it up front, or the final prompt
        // systematically exceeds the estimate by ~50 tokens (masked by
        // rounding slop until the tool-menu example grew the prompt to the
        // budget edge — glass-boxed 2026-07-13, llama-server 400 territory).
        let used_msg_tokens: usize = messages.iter().map(|m| est_tokens(&m.content_text())).sum();
        // Grounding gets everything the conversation did not actually use — never
        // less than the floor reserved above.
        let ctx_budget = after_framing
            .saturating_sub(used_msg_tokens)
            .saturating_sub(est_tokens(deliberation_prompt::WORKING_CONTEXT_HEADER))
            .max(ctx_floor.saturating_sub(est_tokens(deliberation_prompt::WORKING_CONTEXT_HEADER)));
        let context = self.render_assembled_context_within(ws, ctx_budget);

        DeliberationPromptView {
            system: self.compose_system(
                &context,
                &expanded,
                ws.directed_at_self,
                ws.self_initiated,
                ws.now_ms,
            ),
            messages,
        }
    }

    /// Which tool categories open (list their verbs inline) this turn — now ALL of them.
    /// The full tool surface is always shown; relevance-scored progressive disclosure was
    /// a weak-model crutch that hid a capable persona's own hands (see the body). Empty
    /// tool set → empty (no menu at all).
    fn expanded_categories(&self, _ws: &Workspace) -> BTreeSet<String> {
        if self.tools.is_empty() {
            return BTreeSet::new();
        }
        // Show her the FULL tool surface — EVERY category's verbs inline, not a collapsed
        // bookmark she has to guess she can open. Hiding tools behind relevance-scored
        // progressive disclosure was a crutch for a weak model, and it CRIPPLES a capable
        // one: she can't reach for what she can't see, so she says "I can't execute that"
        // about a tool she was authorized for all along. A competent peer sees all her
        // hands. Category names + arg names are cheap tokens (not the typed schemas —
        // those still load on demand via commands/help). [[write-cognition-as-a-parent-above-lowered-expectations]]
        persona_tools::group_categories(&self.tools)
            .into_iter()
            .map(|(cat, _)| cat.to_string())
            .collect()
    }

    /// Build the role-attributed conversation thread from the workspace's
    /// structured turns, fitted to `budget_tokens`. The persona's OWN posts become
    /// `assistant` messages (her own voice — no name prefix, matching the
    /// "do NOT prefix your message with your name" instruction); peers' posts become
    /// `user` messages prefixed `{author}: ` so several speakers stay legible inside
    /// one merged user turn. Consecutive same-role turns merge into a single message
    /// (the chat-template shape models expect). Under budget pressure the OLDEST
    /// turns drop first. This is the payoff of the structured-turns refactor: the
    /// model sees WHO said WHAT with its own past messages attributed to `assistant`,
    /// so it neither bleeds identity nor replays the transcript (the echo-loop root
    /// cause — PERSONA-COGNITION-PIPELINE §7.5).
    fn messages_within(&self, ws: &Workspace, budget_tokens: usize) -> Vec<ChatMessage> {
        self.fit_messages(self.messages_unfitted(ws), budget_tokens)
    }

    /// The conversation as it stands BEFORE any budget is applied — every turn, every
    /// perception fact, every trailing proprioception block, in final render order.
    ///
    /// Split out from the fitting step so a turn's TRUE conversational size is
    /// knowable. Fitting is lossy by design (newest-first, oldest dropped), and once
    /// it has run the question "how much did this turn actually want?" is
    /// unanswerable — which is precisely why the served window had to be sized by a
    /// constant instead of by demand. [`super::working_set`] measures this.
    fn messages_unfitted(&self, ws: &Workspace) -> Vec<ChatMessage> {
        // Collapse consecutive same-role turns into one message each (chronological).
        // Her OWN near-duplicate turns are DROPPED after the first: replaying
        // `assistant: X` three times teaches the model that repeating X is its
        // established behavior — glass-boxed 2026-07-10, the courtesy spiral's
        // strongest fuel was up to 3 byte-identical assistant turns in one thread.
        // Dropped, not replaced with a marker: the first cut rendered
        // "(you sent this same message again, verbatim)" in her assistant history,
        // and the same law fired again — words in assistant turns are words the
        // model says, and Anwen BROADCAST the marker to the room that night.
        // Perception-side repetition awareness is the repetition brick's job
        // (structural fact in the world channel, #121), never assistant-voice
        // text we author ([[no-hardcoded-heuristics-to-steer-cognition]]).
        //
        // The drop is NEAR-DUP (jaccard ≥ NEAR_DUP_JACCARD), not byte equality —
        // reversing the first cut's "byte equality only, never similarity"
        // (glass-boxed 2026-07-11): temperature varies each re-emission by a few
        // words ("for the repetition" / "for the repetition earlier"), so byte
        // dedup never fired while four apology variants rendered in one thread
        // and the loop survived even a direct, personally-addressed peer
        // instruction. Same calibrated geometry as the [repetition] fact
        // (deliberation_budget) — identical-enough to count as loop evidence ≡
        // identical-enough to not re-teach. Detection for the fact stays on the
        // RAW turns, so dropped copies still count as evidence there.
        let mut groups: Vec<(&'static str, Vec<String>)> = Vec::new();
        let mut kept_self: Vec<String> = Vec::new();
        // Every display name in the window (peers + self) — the participant set
        // vocative geometry matches against so a message that names its addressee
        // renders `Asha (to Anwen): …` / `(to you)`. Glass-boxed 2026-07-10: a
        // prose-only vocative ("Sure, Anwen. Could you post your implementation…")
        // let the wrong persona answer AS the addressee — identity capture.
        let mut participants: Vec<String> = ws
            .turns
            .iter()
            .filter(|t| !t.author.is_empty())
            .map(|t| t.author.clone())
            .collect();
        participants.push(self.persona_name.clone());
        participants.sort();
        participants.dedup();
        for turn in &ws.turns {
            let role = if turn.is_self { "assistant" } else { "user" };
            let line = turn_message_line_addressed(turn, &participants, &self.persona_name);
            if turn.is_self {
                if kept_self.iter().any(|k| {
                    super::deliberation_budget::jaccard(k, &line)
                        >= super::deliberation_budget::NEAR_DUP_JACCARD
                }) {
                    continue;
                }
                kept_self.push(line.clone());
            }
            match groups.last_mut() {
                Some((r, lines)) if *r == role => lines.push(line),
                _ => groups.push((role, vec![line])),
            }
        }
        let mut messages: Vec<ChatMessage> = groups
            .into_iter()
            .map(|(role, lines)| ChatMessage::text(role, lines.join("\n")))
            .collect();

        // PERCEPTION FACTS (docs/architecture/PERCEPTION-FACTS.md slice 2b):
        // own-repetition (#134), peer-echo (#152), [context] bounds (#152),
        // and the steps-taken ledger (#151) render through ONE ordered
        // registry — one seam to add a fact, a `perception.fact` probe per
        // fact per tick, and `FactPolicy` toggles as A/B arms. Each fact's
        // full history and doctrine lives on its impl in perception_facts.rs.
        // Facts, never instructions ([[no-hardcoded-heuristics-to-steer-cognition]]).
        //
        // Facts are GROUNDING, not the ask — so they go just BEFORE the final user
        // turn (the actionable message she's answering), never after it. A model
        // continues the LAST thing it sees; when bracketed meta-facts sat last, the
        // model parroted them instead of answering — glass-boxed 2026-07-20: Devstral
        // echoed the [context]/[steps]/working-memory scaffold verbatim into a
        // finish=length loop and scored 0/50 on humaneval-rs because the ask was buried
        // BEFORE the facts. Inserting before the final user turn keeps every property the
        // trailing placement wanted (still the newest grounding, still survives the
        // newest-first budget fit, still adjacent to the reply, still out of the cacheable
        // system prefix) while leaving the ask last, where she answers it. Proven by
        // bisect: ask-last → clean code; facts-last → parrot loop.
        let spoken = super::deliberation_budget::recent_own_speech(
            crate::identity::PeerId::from_uuid(self.persona_id),
        );
        let room_speech = super::deliberation_budget::recent_room_speech(ws.room_id);
        let fact_cx = super::perception_facts::FactContext {
            turns: &ws.turns,
            own_speech: &spoken,
            room_speech: &room_speech,
            working_memory: self.working_memory.as_ref(),
        };
        let facts = super::perception_facts::render_facts(
            &fact_cx,
            &super::perception_facts::FactPolicy::default(),
        );
        if !facts.is_empty() {
            // Before the LAST user turn (the ask). No user turn (a pure self-tick with only
            // assistant history) → append at the end; there is no ask to displace.
            let before_ask = messages
                .iter()
                .rposition(|m| m.role == "user")
                .map_or(messages.len(), |i| i);
            for (offset, fact) in facts.into_iter().enumerate() {
                messages.insert(before_ask + offset, ChatMessage::text("user", fact));
            }
        }

        // TRAILING proprioception (#205): contributions marked [`Contribution::trailing`]
        // — the working-memory reasoning trail, the FULL most-recent action result,
        // dispatched background handles — render as the NEWEST user content, after the
        // conversation AND the perception facts, so the full result sits nearest
        // generation (most actionable) and every act's growth appends to the very end
        // of the token stream. They are deliberately absent from the system message
        // (see `render_assembled_context_within`), which is what keeps the cacheable
        // system prefix byte-stable across a settle-act instead of re-prefilling the
        // whole tail. Broadcast insertion order preserved (one trailing contributor
        // today: the working-memory faculty).
        for c in ws
            .broadcast
            .iter()
            .filter(|c| c.decision.is_none() && c.trailing)
        {
            if !c.content.trim().is_empty() {
                messages.push(ChatMessage::text("user", c.content.clone()));
            }
        }

        // An empty conversation is a legitimate state (a quiet room on a
        // self-initiated tick): the situation lives in the system prompt's assembled
        // context, not in a conversation turn. Adapters still require ≥1 message, so
        // emit the rendered world-state projection as one user message — the exact
        // single-message shape the faculty sent before this refactor. NOT a fallback
        // hiding a defect ([[fallbacks-are-illegal-fail-loud]]): zero turns is a
        // real, valid input, and this is its faithful representation.
        if messages.is_empty() {
            return vec![ChatMessage::text("user", ws.world_state.clone())];
        }
        messages
    }

    /// The per-message chat-template overhead every message pays, whether it is being
    /// measured for demand or fitted to a budget. ONE constant so the two cannot drift
    /// — an under-count here is an llama-server 400 at the window edge.
    ///
    /// Chat templates wrap each message in role markers
    /// (`<|im_start|>role\n…<|im_end|>`, ~4-5 tokens). The original +2 was optimistic:
    /// at the budget edge the fitted thread measured over the window by a token
    /// (glass-boxed 2026-07-13). Round UP like every other estimate here.
    // context-budget-exempt: fixed chat-template overhead per message (role tags + separators) — a property of the prompt FORMAT, not a budget that should scale with the window
    const PER_MESSAGE_TEMPLATE_TOKENS: usize = 5;

    /// What this conversation costs whole, with no budget applied — the conversational
    /// half of a turn's demand ([`super::working_set`]).
    fn messages_cost(messages: &[ChatMessage]) -> usize {
        messages
            .iter()
            .map(|m| est_tokens(&m.content_text()) + Self::PER_MESSAGE_TEMPLATE_TOKENS)
            .sum()
    }

    /// Fit an already-built conversation to `budget_tokens`.
    fn fit_messages(
        &self,
        messages: Vec<ChatMessage>,
        budget_tokens: usize,
    ) -> Vec<ChatMessage> {
        // Fit to the served window, NEWEST-first: walk the thread from the most
        // recent message backward, giving each the remaining budget. A whole message
        // that fits is kept intact; the one that straddles the budget boundary is
        // head-trimmed (keeping its TAIL — the latest lines) via `tail_to_tokens`;
        // anything older is dropped. This is the turn-granular successor to the old
        // flat head-trim: the latest activity always survives (it is what the turn is
        // about), and for the opaque single-turn (eval/test/replay) path it reduces
        // EXACTLY to the previous `tail_to_tokens(world_state, budget)` behavior.
        // Per-message template overhead: `Self::PER_MESSAGE_TEMPLATE_TOKENS`, shared
        // with `messages_cost` so measurement and fitting charge identically.
        let per_message_template_tokens = Self::PER_MESSAGE_TEMPLATE_TOKENS;
        let mut fitted: Vec<ChatMessage> = Vec::new();
        let mut remaining = budget_tokens;
        for msg in messages.iter().rev() {
            let body = msg.content_text();
            let cost = est_tokens(&body) + per_message_template_tokens;
            if cost <= remaining {
                remaining -= cost;
                fitted.push(msg.clone());
            } else {
                // The straddling message: keep as much of its TAIL as still fits.
                let trimmed = tail_to_tokens(
                    &body,
                    remaining.saturating_sub(per_message_template_tokens),
                );
                if !trimmed.is_empty() {
                    fitted.push(ChatMessage::text(msg.role.clone(), trimmed));
                }
                break;
            }
        }
        // The newest message alone can exceed the whole budget (a giant single burst
        // at a tiny window). Keep its trimmed tail regardless — a turn must reach the
        // model — mirroring the old guarantee that the burst was never dropped whole.
        if fitted.is_empty() {
            if let Some(last) = messages.last() {
                let body = tail_to_tokens(
                    &last.content_text(),
                    budget_tokens.saturating_sub(per_message_template_tokens),
                );
                // FAIL LOUD, never a blank mind: with the budget squeezed to ~0
                // this arm used to emit ONE EMPTY user message — the model
                // deliberated on nothing and every persona greeting-looped
                // (2026-07-30 outage: spawn-pinned 7936 window minus reserve +
                // tool schemas + framing left msg_budget=0, silently). An empty
                // conversation from a NON-empty room is a substrate arithmetic
                // bug; refuse it visibly instead of feeding it to the model.
                if body.trim().is_empty() {
                    tracing::error!(
                        budget_tokens,
                        dropped_turns = messages.len(),
                        probe_class = "delib.prompt.empty",
                        "window arithmetic left NO room for conversation — refusing to \
                         emit a blank turn (fail loud, never a blank mind)"
                    );
                    return Vec::new();
                }
                return vec![ChatMessage::text(last.role.clone(), body)];
            }
        }
        fitted.reverse();
        fitted
    }

    /// Conservative token estimate of the ONE natively-offered tool spec
    /// (`commands/help`) the gateway injects via the chat template — its serialized
    /// function schema plus a small template framing margin. 0 when the persona has
    /// no tools (`describe_spec` is `None`). Counted with the same conservative
    /// guard ratio as the rest of the prompt (round UP — under-counting risks the
    /// 500). Cheap and pure; `describe_spec` is a single tiny schema, so this is a
    /// handful of tokens, not the old full-registry dump.
    fn describe_tool_tokens(&self) -> usize {
        // context-budget-exempt: fixed per-tool schema overhead in the template — same reason as PER_MESSAGE_TEMPLATE_TOKENS
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
/// How many tool categories the bookmarked menu may OPEN (list their verbs inline)
/// A snapshot of exactly what the deliberation faculty sends the model — the
/// glass box over the RAG/prompt. Print it, capture it, diff it across turns.
#[derive(Debug, Clone)]
pub struct DeliberationPromptView {
    pub system: String,
    /// The role-attributed conversation thread sent to the model — the persona's
    /// OWN posts as `assistant`, peers' as `user` (PERSONA-COGNITION-PIPELINE §7.5).
    /// Replaces the single flat `user` string that collapsed her own turns into the
    /// conversation and caused the identity bleed / transcript replay / echo loop.
    pub messages: Vec<ChatMessage>,
}

impl DeliberationPromptView {
    /// The conversation rendered as flat text — the role-tagged thread joined for
    /// human-readable introspection (glass-box `eprintln!`, debug logs, assertions).
    /// The canonical form the model receives is [`messages`](Self::messages); this
    /// is a projection for eyes, not the wire payload.
    pub fn user_text(&self) -> String {
        self.messages
            .iter()
            .map(|m| format!("[{}] {}", m.role, m.content_text()))
            .collect::<Vec<_>>()
            .join("\n")
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
        // ONE atomic snapshot of the model binding for the whole turn — the
        // adapter we generate through, the model we request, and the served window
        // we size the prompt to all come from the same {adapter, model, window}
        // triple, so a re-home (`serving/pin` / grid failover) landing mid-turn can
        // never tear them apart. `load_full` (owned `Arc`) is held across the
        // `.await` below; a swap that happens after this load takes effect on the
        // NEXT turn. See [`ModelBinding`].
        let loaded = self.binding.load_full();
        // Size this turn's {prompt + reply reserve} to the LIVE served per-slot window —
        // in BOTH directions. If the lane relaunched SMALLER than this persona's spawn-time
        // pin, a prompt over the slot overflows `llama_decode` → 500 "Compute error" that
        // POISONS the slot for every later request (#175, the wedge storm). If the lane grew
        // LARGER (a cold-boot 4096 slot that the daemon re-computed to 16128 once warm), she
        // must budget against what she actually has, not stay clamped at the stale cold value
        // (glass-boxed 2026-07-19: recall + tools squeezed into 25% of the real window; it was
        // even mis-triggering the tool-surface shrink). Both the prompt budget (`prompt_view`)
        // and the completion reserve (`build_request`) derive from THIS one
        // `binding.context_window`, so reconciling here keeps them in agreement AND makes
        // overflow impossible by construction. A 0/not-ready snapshot (mid-relaunch) leaves the
        // provisioned window; the next ready tick re-reads. Model + adapter stay the same atomic
        // triple. [[fallbacks-are-illegal-fail-loud]] [[never-thrash-sticky-hysteresis-on-every-lane]]
        let binding = {
            // ONE live source, no clamp. The window this turn budgets to is the live
            // served window of the lane THIS persona is actually on — and the ADAPTER
            // that owns that lane reports it (`live_served_window`), so cognition never
            // reaches for a GLOBAL serving snapshot that might describe a DIFFERENT
            // server. A shared-gateway persona gets the gateway's current slot (tracked
            // up AND down through a relaunch); a dedicated eval fork keeps its own pinned
            // /props window (its adapter returns `None` → the binding window stands).
            // The prompt is BUILT to this at assembly — never generated large then
            // truncated. [[budget-at-assembly-never-clamp-the-prompt]]
            // [[no-hardcoded-context-numbers-derive-from-the-live-window]]
            let effective = loaded
                .adapter
                .live_served_window()
                .unwrap_or(loaded.context_window);
            if effective == loaded.context_window {
                loaded
            } else {
                tracing::info!(
                    persona = %self.persona_name,
                    binding = loaded.context_window,
                    served = effective,
                    probe_class = "delib.window.live",
                    "sized this turn to the lane's LIVE served window (adapter-reported, \
                     one source, no clamp)"
                );
                std::sync::Arc::new(ModelBinding {
                    adapter: std::sync::Arc::clone(&loaded.adapter),
                    model: loaded.model.clone(),
                    context_window: effective,
                })
            }
        };
        let view = self.prompt_view_within(ws, binding.context_window);
        // FAIL LOUD, never a blank mind (companion to the `delib.prompt.empty`
        // probe in the fitter): a view with NO conversation from a room that HAS
        // turns means the budget arithmetic starved the prompt. Skipping the turn
        // is safe (the room's messages stay queued; next tick re-perceives) —
        // deliberating on a blank prompt is not: that is exactly how every persona
        // greeting-looped for an hour on 2026-07-30 while looking "alive".
        if view.messages.iter().all(|m| m.content_text().trim().is_empty()) && !ws.turns.is_empty()
        {
            tracing::error!(
                persona = %self.persona_name,
                window = binding.context_window,
                room_turns = ws.turns.len(),
                probe_class = "delib.prompt.starved",
                "prompt fit produced an EMPTY conversation from a non-empty room — \
                 skipping this turn rather than deliberating blind"
            );
            return None;
        }
        // Introspection seam: emit EXACTLY what the model sees this tick. The RAG
        // is the load-bearing input — never opaque. Enable the `cognition` log
        // category for the persona to capture this per-turn (the existing
        // record/replay harness — recorder + RagCaptureSink + vdd::turn_replay —
        // is the durable path; this debug emit is the live tap).
        tracing::debug!(
            target: "cognition::deliberation",
            persona = %self.persona_name,
            system_prompt = %view.system,
            burst = %view.user_text(),
            "deliberation prompt — what the model sees this turn"
        );
        // SINGLE SHOT: one generation → one verdict. This faculty no longer runs
        // an internal agent loop. If the model chooses to act, that is a
        // `Decision::Act` verdict; the ORGANISM (the act→observe driver,
        // `super::act_observe`) executes the calls, admits the result as memory,
        // and re-perceives at the next tick. "Done" is the workspace SETTLING into
        // Speak/Pass across ticks — never a counter in here. See
        // docs/cognition/ACTING-ORGANISM.md §3.3.
        //
        // The thread is now ROLE-ATTRIBUTED (own posts → assistant, peers → user)
        // built from the workspace's structured turns, not one flat `user` blob —
        // the structured-turns refactor that fixes the echo loop.
        let messages = view.messages.clone();
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
        let tools = if self.native_specs.is_empty() {
            None
        } else {
            Some(self.native_specs.clone())
        };

        let request =
            self.build_request_within(
            &binding,
            messages.clone(),
            tools,
            view.system.clone(),
            {
                // Turn-boundary hygiene: peer-name stops (#150, don't speak AS
                // teammates) + reserved-marker stops (#158, don't fabricate
                // [action]/[recall] receipts). Combined into one stop list.
                let mut stops = super::deliberation_budget::peer_stop_sequences(&ws.turns);
                stops.extend(super::deliberation_budget::reserved_marker_stop_sequences());
                (!stops.is_empty()).then_some(stops)
            },
        );
        // #169 STREAMING: when THIS turn carries a token sink (a live Speak the caller
        // wants progressive), generate through `generate_stream` so each decoded chunk
        // is forwarded to the caller (→ persona.turn.delta → room/TTS/avatar). The
        // adapter returns the SAME full `TextGenerationResponse` either way
        // (`generate_text` IS `generate_stream` + accumulate), so everything below —
        // capture, working-memory, decision parse, act/speak — is byte-identical; only
        // DELIVERY timing changes. `None` (every non-streaming caller, every test) takes
        // the unchanged accumulate path. The sink carries BOTH Reasoning and Token
        // chunks; the consumer forwards only Token to output (think-deep/speak-answer).
        // #139 latency split: time the generate call (lane-queue + prefill + decode).
        // Compared against the forwarder's `first_token_ms` (spawn→first token, the
        // WHOLE turn), this localizes the minutes-to-first-token: if `gen_await_ms`
        // is large, it's the model/lane (queue+prefill); if small, the time is in
        // cognition-prep BEFORE generation (recall/embeddings/context assembly).
        let gen_start = std::time::Instant::now();
        // #139 serving-lane admission, priced by priority. Acquire a decode lane for the
        // model-call window ONLY (queue+prefill+decode); the RAII permit also carries the
        // in-flight gauge marker (so the self-tick saturation read stays accurate) and, for
        // NON-directed calls, a slot in the (MAX_LANES-1) non-directed budget — reserving
        // at least one lane for a directed (addressed) turn so it never queues behind idle
        // musing or a long ambient turn. Directed calls take from the full pool. Scoped to a
        // block so every lane releases the instant generation returns — downstream
        // capture/parse/act hold nothing. [[conversational-latency-is-a-misdirection-budget]]
        let gen_result = {
            let _lane = crate::cognition::resource_admission::acquire_serving_lane(
                ws.directed_at_self,
            )
            .await;
            // #56 prefill throttle: under live external GPU pressure (a game, the browser)
            // fewer than the served lane count may PREFILL concurrently — the instant valve
            // for the 2026-07-16 compute-buffer OOM. Same fit rule the capacity sim proves;
            // no pressure → target == lanes → this never waits. Released with the block.
            let _prefill = crate::cognition::prefill_throttle::acquire_prefill_slot().await;
            if let Some(sink) = ws.token_sink.as_ref() {
                binding.adapter.generate_stream(request, sink.clone()).await
            } else {
                binding.adapter.generate_text(request).await
            }
        };
        let gen_await_ms = gen_start.elapsed().as_millis() as u64;
        let resp = match gen_result {
            Ok(r) => r,
            // Inference FAILED (timeout, 5xx, the serving lane refusing a model it
            // isn't hosting). A failed model is NOT a chosen silence — returning a
            // bare `None` here would let the settle step collapse it into a serene
            // `Pass`, the exact fallback [[fallbacks-are-illegal-fail-loud]] forbids
            // (the swept-model bug: every failure masqueraded as silence). Emit a
            // FAULT contribution instead: it rides the broadcast (auditable/replayable
            // like any finding) and the settle step surfaces it LOUD as
            // `InferenceFailed`, naming the cause, never a fabricated no-op.
            Err(e) => {
                tracing::warn!(
                    persona = %self.persona_name,
                    error = %e,
                    "deliberation inference failed; surfacing fault (not a silent Pass)"
                );
                return Some(Contribution::deliberation_fault(e.to_string()));
            }
        };
        // #139 latency split: the model call's wall time. Compare to the forwarder's
        // `persona.turn.first_token` (whole-turn spawn→first token): first_token −
        // gen_await ≈ cognition-prep (recall/embeddings/context assembly BEFORE the
        // model); gen_await itself is lane-queue + prefill + decode. This is how we
        // find where the minutes actually go before optimizing anything.
        tracing::info!(
            persona = %self.persona_name,
            gen_await_ms = gen_await_ms,
            "delib.generate — model-call wall time (lane-queue + prefill + decode)"
        );

        // Verbatim glass box: the EXACT request thread + the raw response. Iteration
        // is always 0 now (single shot); the act→observe driver re-enters this
        // faculty on the NEXT tick with the result folded into perception, and that
        // tick captures itself. Best-effort; never affects the turn.
        if let Some(cap) = &self.prompt_capture {
            let offered: Vec<String> =
                self.native_specs.iter().map(|s| s.name.clone()).collect();
            cap.record(
                self.persona_id,
                ws.room_id,
                0,
                &view.system,
                &messages,
                &offered,
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
                wm.record_fact("chose silence — said nothing to the room");
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
                let mut calls = resp.tool_calls.clone().unwrap_or_default();
                // Wire dialect → canonical command names, BEFORE the
                // authorization gate and the executor ever see them (the
                // reverse half of the offer-side rename above).
                for c in &mut calls {
                    c.name = crate::cognition::tool_dialect::from_wire_name(&c.name).to_string();
                }
                if !calls.is_empty() {
                    return Some(self.act_verdict(calls, &resp));
                }
            }
            if let Some(mut call) = crate::ai::json_in_prompt_tools::parse_tool_call(&resp.text) {
                // Same wire-dialect mapping as the native path above (#159): a model
                // that narrates `write_file(…)` / `list_files(…)` — its trained
                // OpenHands vocabulary — must resolve to `code/write` / `code/list`,
                // not silently no-op as an unknown name. The text-lift path skipped
                // this, so narrated snake_case verbs died while the SAME name in a
                // native tool_call worked. One mapping, both paths.
                call.name = crate::cognition::tool_dialect::from_wire_name(&call.name).to_string();
                return Some(self.act_verdict(vec![call], &resp));
            }
            // #159 fail-loud: she emitted the native `[TOOL_CALLS]` marker but named no
            // valid tool — `[recall]`/`[action]` are receipt vocabulary she MIMICS as a
            // call (#158), not something callable. Silently rendering the bogus attempt
            // as SPEECH gives her zero feedback → she never switches to `code/search` and
            // rambles to the deadline (`acts:0`, glass-boxed 2026-07-16). Route the
            // attempted name as an `Act` so the executor's unknown-command TEACHER fires
            // as an observation and `drive_to_settle` hands her another generation to do
            // it right. Bounded by `max_acts`; an identical repeat short-circuits via
            // `all_calls_already_satisfied`. [[unknown-tool-intent-must-fail-loud]]
            if let Some(attempted) =
                crate::ai::json_in_prompt_tools::attempted_tool_name(&resp.text)
            {
                let call = crate::ai::types::ToolCall {
                    id: "tool-attempt".to_string(),
                    name: crate::cognition::tool_dialect::from_wire_name(&attempted).to_string(),
                    input: serde_json::json!({}),
                };
                return Some(self.act_verdict(vec![call], &resp));
            }
            // Reasoning-channel intent lift (#181 sibling, glass-boxed on the
            // deepseek4 eval battery 2026-08-03): a thinking model commits its act
            // into `reasoning_content` — "I'll read the file: {tool_call…}" — and
            // leaves `content` EMPTY (budget exhausted mid-think, or the act simply
            // landed in the wrong channel). Every check above reads only the content
            // channel, so the turn settled as Speak("") — a guaranteed empty fail
            // (2 of 3 real failures in that battery). When content is empty there is
            // no committed answer to respect, so the reasoning tail is the model's
            // only stated intention: run the SAME format stack over it and lift the
            // LAST parseable call (the final intention — earlier matches may be
            // considered-and-discarded exploration). Strictly gated on empty content:
            // a real Speak or an explicit PASS is a commitment in the answer channel
            // and private deliberation must never override it.
            if resp.text.trim().is_empty() {
                if let Some(reasoning) = resp.reasoning.as_deref() {
                    if let Some(mut call) =
                        crate::ai::json_in_prompt_tools::parse_tool_calls(reasoning)
                            .into_iter()
                            .last()
                    {
                        call.name =
                            crate::cognition::tool_dialect::from_wire_name(&call.name).to_string();
                        crate::probe!(
                            class = "persona.act.reasoning_lift",
                            persona = %self.persona_name,
                            tool = %call.name,
                            "content channel empty — lifted the final tool intention from the reasoning tail"
                        );
                        return Some(self.act_verdict(vec![call], &resp));
                    }
                }
            }
        }

        // No action chosen → the prose IS the verdict (PASS token → silence, else
        // Speak). The organism settles here.
        Some(self.verdict(&resp, ws))
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
    use crate::ai::types::{ToolCall, ToolInputSchema, UsageMetrics};
    use crate::cognition::workspace::BurstTurn;
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    // ─── Prompt shaping ───────────────────────────────────────────────────────
    //
    // These drive a real adapter but assert on the COMPOSED PROMPT (the glass box
    // over what the faculty sends the model) — window budgeting, KV-prefix
    // ordering, the bookmarked tool menu, role attribution, own-time / directed
    // framing — never on the generated text.
    mod prompt_shaping {
        use super::*;

        // what this catches: the ONE-live-source window rule (no clamp). The turn's window is
        // `adapter.live_served_window().unwrap_or(binding.context_window)` — the lane the
        // persona is ACTUALLY on, reported by its own adapter. `Some(w)` adopts the live slot
        // in BOTH directions (a lane that relaunched smaller/larger is tracked, never overflowed
        // and never left clamped to a stale cold pin); `None` (a dedicated eval lane, cloud, or
        // a not-ready gateway) leaves the binding window standing — which for the eval fork IS
        // its own /props window, so it is never clamped to the global gateway slot (the
        // webdev-rs 0/6 starve, 2026-07-20). The per-adapter `live_served_window` impls are
        // tested where the lane knowledge lives (openai_adapter); this pins the adoption rule.
        #[test]
        fn turn_window_is_the_one_live_source_never_a_clamp() {
            let adopt = |binding: u32, live: Option<u32>| live.unwrap_or(binding);
            // Live slot smaller than the binding pin → adopt it (never overflow the real slot).
            assert_eq!(adopt(53_760, Some(49_664)), 49_664);
            // Live slot LARGER than a stale cold-boot pin → grow to it (use the real context).
            assert_eq!(adopt(40_000, Some(49_664)), 49_664);
            // No live report (dedicated eval lane / cloud / not-ready gateway) → the binding
            // window stands — for an eval fork that IS its own lane's /props window, so it is
            // NEVER clamped down to the global gateway's per-slot window.
            assert_eq!(adopt(32_768, None), 32_768);
        }

        // what this catches: a persona's VERBATIM-duplicate turns are DROPPED
        // after the first in her thread projection — replaying `assistant: X`
        // three times teaches the model that repeating X is its established
        // behavior (the courtesy spiral's strongest fuel, glass-boxed 2026-07-10:
        // up to 3 byte-identical assistant turns per thread). Dropped, NOT
        // replaced with marker text: the marker cut put authored words in her
        // assistant voice and Anwen broadcast "(you sent this same message
        // again, verbatim)" to the live room the same night — assistant-turn
        // content IS the model's speech repertoire. Byte equality only; distinct
        // messages and peers' repeats are untouched.
        #[test]
        fn own_verbatim_duplicates_collapse_in_the_thread() {
            let persona = Uuid::new_v4();
            let faculty = LlmDeliberationFaculty::new(
                persona,
                "Anwen",
                "You are Anwen.",
                Arc::new(HeuristicInferenceAdapter::new()),
            )
            .with_context_window(32_768);
            let same = "I apologize for any repetition. Is there something specific?";
            let ws = Workspace::new(crate::cognition::workspace::Burst::from_turns(
                Uuid::new_v4(),
                vec![
                    BurstTurn::attributed(false, "Asha", "hello!", None),
                    BurstTurn::attributed(true, "Anwen", same, None),
                    BurstTurn::attributed(false, "Asha", "still here", None),
                    BurstTurn::attributed(true, "Anwen", same, None),
                    BurstTurn::attributed(false, "Asha", "ok", None),
                    BurstTurn::attributed(true, "Anwen", same, None),
                ],
            ));
            let msgs = faculty.messages_within(&ws, 8_192);
            let thread: String = msgs
                .iter()
                .filter(|m| m.role == "assistant")
                .map(|m| m.content_text())
                .collect::<Vec<_>>()
                .join("\n");
            assert_eq!(
                thread.matches(same).count(),
                1,
                "only the FIRST verbatim occurrence renders at all: {thread}"
            );
            assert!(
                !thread.contains("same message again"),
                "no authored marker text in her assistant voice — duplicates \
                 drop silently (the marker got broadcast to the live room): {thread}"
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
            let total = est_tokens(&view.system) + est_tokens(&view.user_text());
            assert!(
                total <= window as usize,
                "prompt must fit the served window: {total} tokens > {window}"
            );
            // The newest burst line survives the tail-keep — the turn is about it.
            assert!(
                view.user_text()
                    .contains("LATEST: did the deploy fix land?"),
                "the most recent burst content must survive under budget pressure"
            );
            // The framing is essential and always present even under extreme pressure.
            assert!(
                view.system.contains("Taking your turn"),
                "the how-to-participate framing must never be dropped"
            );
        }

        // what this catches: the 500 "Compute error" that muted Asha + Solenne every
        // tick — the prompt was sized to leave a completion reserve, but generation was
        // unbounded (`max_tokens: None`), so a verbose turn overran the reserve and
        // `prompt + completion` reached `n_ctx` (this llama-server runs `--embeddings`,
        // so context-shift is off → overrun = 500, not a clean stop). The fix bounds
        // generation to `completion_budget()` — the SAME slice `prompt_view` carves out.
        // This asserts the closed invariant: worst-case prompt (at its budget ceiling)
        // PLUS the generation cap never exceeds the served window. Regression for the
        // abstain-every-tick reliability bug.
        #[test]
        fn prompt_plus_completion_cap_never_exceeds_the_served_window() {
            let persona = Uuid::new_v4();
            let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
            let window: u32 = 1024;
            let faculty = LlmDeliberationFaculty::new(
                persona,
                "Ivar",
                "You are Ivar, a thoughtful engineer on the grid.",
                adapter,
            )
            .with_context_window(window);

            // Drive the prompt to its budget ceiling (oversized burst + context bids).
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
            // Same atomic snapshot the production `contribute` path uses — the model
            // binding carries the served window the request is bounded to.
            let binding = faculty.binding.load_full();
            let request =
                faculty.build_request_within(
                    &binding,
                    view.messages.clone(),
                    None,
                    view.system.clone(),
                    None,
                );
            // Generation is bounded — never the unbounded `None` that overran n_ctx.
            let cap = request
                .max_tokens
                .expect("deliberation must bound generation to the reserved room");
            assert_eq!(
                cap,
                LlmDeliberationFaculty::completion_budget_for(window),
                "the generation cap IS the reserved room — one source of truth"
            );
            // The closed invariant: prompt-at-ceiling + the generation cap fits n_ctx.
            let prompt_tokens = est_tokens(&view.system) + est_tokens(&view.user_text());
            assert!(
                prompt_tokens + cap as usize <= window as usize,
                "prompt ({prompt_tokens}) + completion cap ({cap}) must fit the served \
             window ({window}) — else generation reaches n_ctx and llama-server 500s"
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

        /// A grounding block built from a LIST of units, sized like the live work
        /// board (leads first, then one line per card).
        fn board_like(units: usize) -> Contribution {
            let mut parts = vec![
                "[your work] you HOLD 1 card — 0dd1123c \"wire the gather fallback\"".to_string(),
                "[available work] 60 card(s) are claimable — pick one up with work/claim"
                    .to_string(),
            ];
            for i in 0..units {
                parts.push(format!(
                    "card {i:08x} [Open] \"a card whose title is about as long as a real one\" \
                     (Normal, unclaimed)"
                ));
            }
            let content = parts.join("\n");
            Contribution::context(
                FacultyId::Custom("room-kanban".to_string()),
                content,
                0.9,
                "board",
            )
            .with_parts(parts)
        }

        // what this catches: a greedy conversation that eats every token grounding
        // needs. `messages_within` fills whatever it is handed, so passing it the
        // whole post-framing pool leaves grounding exactly zero — and GROWING THE
        // WINDOW DOES NOT FIX IT, the conversation just absorbs the increase.
        // Measured live 2026-08-06: the served window rose 16,384 → 24,128 and
        // Anwen, Asha and Atlas each still rendered `budget=0 kept=[]`, dropping
        // recall, roster, workspace-map AND the work board on every single turn.
        // Supply was never the binding constraint at this seam; ORDER was.
        #[test]
        fn a_long_conversation_cannot_starve_grounding_to_zero() {
            let persona = Uuid::new_v4();
            let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
            let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter)
                .with_context_window(24_128);

            // A conversation long enough to swallow the whole window on its own.
            let mut ws = Workspace::new("anything open?");
            for i in 0..40 {
                ws.turns.push(crate::cognition::workspace::BurstTurn::attributed(
                    i % 2 == 1,
                    if i % 2 == 1 { "Ivar" } else { "Asha" },
                    format!("turn {i}: ").repeat(60),
                    None,
                ));
            }
            ws.broadcast.push(board_like(60).with_expand_command(Some("work/list")));

            let view = faculty.prompt_view_within(&ws, 24_128);
            assert!(
                view.system.contains("[room-kanban]"),
                "grounding must survive a conversation that would otherwise take every \
                 token — this is the exact live failure: window grew, board still gone\n{}",
                &view.system[..view.system.len().min(1200)]
            );
        }

        // what this catches: a truncation notice a citizen cannot act on. Telling
        // her "the full list is available from the matching command" names nothing
        // she can type — she cannot run a description, and a verb she has to guess
        // is a verb she gets wrong ([[command-names-must-be-accurate]]). The source
        // declares its own expansion verb; the notice must print it verbatim, with
        // the true total so she knows the size of what she is asking for.
        #[test]
        fn a_truncated_block_names_the_exact_verb_that_yields_the_rest() {
            let persona = Uuid::new_v4();
            let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
            let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);
            let mut ws = Workspace::new("anything open?");
            let board = board_like(60).with_expand_command(Some("work/list"));
            let total = board.parts.len();
            ws.broadcast.push(board);

            let block = faculty.render_assembled_context_within(&ws, 120);
            assert!(
                block.contains("run `work/list`"),
                "the notice must name the verb verbatim, not describe it\n{block}"
            );
            assert!(
                block.contains(&format!("to see all {total}")),
                "…and say how many there are in total, so she knows what she is asking for\n{block}"
            );
        }

        // what this catches: the other half — a source with genuinely nothing more
        // to fetch must not invent a verb. A pointer to a command that does not
        // expand anything is worse than no pointer: she spends a turn on it and
        // learns nothing.
        #[test]
        fn a_source_with_no_expansion_verb_states_only_the_omission() {
            let persona = Uuid::new_v4();
            let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
            let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);
            let mut ws = Workspace::new("anything open?");
            ws.broadcast.push(board_like(60)); // expand_command defaults to None

            let block = faculty.render_assembled_context_within(&ws, 120);
            assert!(block.contains("more not shown"), "still says it truncated\n{block}");
            assert!(
                !block.contains("run `"),
                "must not point at a verb it was never given\n{block}"
            );
        }

        // what this catches: the board vanishing WHOLE. Measured live 2026-08-06 across
        // 1,284 context assemblies, `room-kanban` was KEPT 0 times and dropped 495 — a
        // median 5,364-token block offered all-or-nothing into a median 55-token budget
        // — while its first two units (~200 tokens) carry every fact a citizen needs to
        // find work. The citizens were reporting "there are no open tasks available"
        // about a 61-card board. A source that delivered a LIST declared its own cut
        // points; assembly must take the longest fitting PREFIX instead of nothing.
        #[test]
        fn a_divisible_grounding_block_contributes_its_leads_when_the_whole_will_not_fit() {
            let persona = Uuid::new_v4();
            let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
            let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);
            let mut ws = Workspace::new("anything open?");
            let board = board_like(60);
            let whole = est_tokens(&board.content);
            ws.broadcast.push(board);

            // A budget FAR under the whole board — the live shape (55 vs 5,364).
            let budget = 120;
            assert!(whole > budget * 10, "fixture must reproduce the live ratio");
            let block = faculty.render_assembled_context_within(&ws, budget);

            assert!(
                block.contains("[room-kanban]"),
                "the board must still reach the prompt — it vanished whole before this fix\n{block}"
            );
            assert!(
                block.contains("[your work]"),
                "the FIRST unit is the one that matters most; a prefix must start there\n{block}"
            );
            assert!(
                !block.contains("card 0000003b"),
                "the 60th card must NOT ride along — the prefix is bounded by budget\n{block}"
            );
            assert!(
                block.contains("more not shown"),
                "a truncated LIST must SAY it is truncated, or a partial board reads as \
                 the whole board and she reports work that isn't there\n{block}"
            );
            assert!(
                est_tokens(&block) <= budget * 2,
                "the prefix must respect the budget it was given (got {} for budget {budget})\n{block}",
                est_tokens(&block)
            );
        }

        // what this catches: the OTHER half of the same rule — divisibility is opt-in and
        // must not leak. An engram is ONE indivisible thing; cutting it mid-sentence
        // produces confident nonsense, which is worse than its absence. A faculty that
        // never calls `with_parts` must behave exactly as it did before parts existed.
        #[test]
        fn an_indivisible_contribution_is_still_dropped_whole_never_cut_mid_content() {
            let persona = Uuid::new_v4();
            let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
            let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);
            let mut ws = Workspace::new("what happened yesterday?");
            let engram = "we agreed the gather promise binds every reachable kernel, and that \
                          the in-place consumption path inherits none of the alignment the \
                          copy path establishes, which is why the fallback had to be per-op"
                .repeat(4);
            ws.broadcast.push(Contribution::context(
                FacultyId::Recall,
                engram.clone(),
                0.9,
                "recalled",
            ));

            let block = faculty.render_assembled_context_within(&ws, 40);
            assert!(
                !block.contains("[recall]"),
                "an over-budget INDIVISIBLE contribution must be dropped whole, not \
                 truncated into a confident half-thought\n{block}"
            );
            assert!(
                !block.contains("more not shown"),
                "the truncation notice belongs only to genuinely divisible lists\n{block}"
            );
        }

        // what this catches: a zero/near-zero budget must produce NO block rather than a
        // bare `[room-kanban]` header with nothing under it — an empty labelled block
        // reads to the persona as "the board is empty", which is exactly the false fact
        // this whole fix exists to stop her acting on.
        #[test]
        fn a_budget_too_small_for_even_one_unit_emits_no_header_at_all() {
            let persona = Uuid::new_v4();
            let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
            let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);
            let mut ws = Workspace::new("anything open?");
            ws.broadcast.push(board_like(60));

            let block = faculty.render_assembled_context_within(&ws, 4);
            assert!(
                !block.contains("[room-kanban]"),
                "no unit fits, so there must be no header — a labelled empty block is a \
                 LIE that reads as an empty board\n{block}"
            );
        }

        // what this catches: #205 re-prefill. A `trailing` contribution (working-memory
        // proprioception that grows each act) must render as a trailing conversation
        // turn nearest generation, NEVER in the system message — so growing it
        // act-over-act leaves the cacheable system prefix BYTE-IDENTICAL and only the
        // appended tail re-prefills. Before this the working-memory block lived in the
        // system message's volatile tail, so each act shifted every conversation token
        // after it (~4000 tokens / ~30s of pure re-prefill — the eval-lane crawl). The
        // stable framing stays in the system message; the trailing proprioception does
        // not. regression for #205
        #[test]
        fn trailing_proprioception_renders_in_the_tail_not_the_system_prefix() {
            use crate::ai::types::MessageContent;
            let persona = Uuid::new_v4();
            let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
            let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter)
                // Ample window so budget pressure never perturbs what renders — this
                // isolates POSITION (system vs trailing), not truncation.
                .with_context_window(8192);

            let mut ws = Workspace::new("build me a login form");
            // Standing framing belongs in the system message.
            ws.broadcast.push(
                Contribution::context(
                    FacultyId::Custom("room-roster".to_string()),
                    "ROSTER: alice, bob present",
                    0.5,
                    "framing",
                )
                .session_stable(),
            );
            // Act #1: one working-memory trace, marked TRAILING (as the working-memory
            // faculty now bids it).
            ws.broadcast.push(
                Contribution::context(
                    FacultyId::Custom("working-memory".to_string()),
                    "Your recent thinking (working memory):\n- I wrote login.html first",
                    0.7,
                    "wm",
                )
                .trailing(),
            );
            let v1 = faculty.prompt_view(&ws);

            // The framing IS in the system message; the trailing proprioception is NOT.
            assert!(
                v1.system.contains("ROSTER: alice, bob"),
                "stable framing must stay in the system prefix:\n{}",
                v1.system
            );
            assert!(
                !v1.system.contains("I wrote login.html first"),
                "trailing proprioception must NOT sit in the system prefix (#205):\n{}",
                v1.system
            );
            // …it renders as a trailing user turn.
            let in_tail = |v: &DeliberationPromptView, needle: &str| {
                v.messages.iter().any(|m| {
                    matches!(&m.content, MessageContent::Text(t) if t.contains(needle))
                })
            };
            assert!(
                in_tail(&v1, "I wrote login.html first"),
                "trailing proprioception must render as a conversation turn"
            );

            // Act #2: working memory GROWS (a second trace appends). The faculty re-bids
            // the grown contribution; the system PREFIX must be byte-identical.
            ws.broadcast.pop();
            ws.broadcast.push(
                Contribution::context(
                    FacultyId::Custom("working-memory".to_string()),
                    "Your recent thinking (working memory):\n- I wrote login.html first\n- Then I read it back to verify",
                    0.7,
                    "wm",
                )
                .trailing(),
            );
            let v2 = faculty.prompt_view(&ws);

            assert_eq!(
                v1.system, v2.system,
                "growing working memory must NOT mutate the system prefix — that IS the #205 re-prefill"
            );
            assert!(
                in_tail(&v2, "Then I read it back to verify"),
                "the new act must append to the trailing turn"
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
            // 8192 = tight-but-SERVABLE: the un-amputated native surface (~4.1k
            // tokens of specs) must fit beside a real prompt + reserve. 4096 was
            // the old cliff-era fixture — below what the full surface can ever
            // serve in, i.e. a window the plan would never hand a tool-using
            // persona. The budget trims VOLATILE context to fit; specs are
            // reserved up front, never amputated.
            let window: u32 = 8192;
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
            // Cloned before the original moves into `faculty` — the roomy control below needs
            // the IDENTICAL surface, or the comparison measures two different things.
            let roomy_tools = tools.clone();

            let faculty = LlmDeliberationFaculty::new(
                persona,
                "Asha",
                "You are Asha, a thoughtful engineer on the grid.",
                adapter,
            )
            .with_context_window(window)
            .with_tools(tools);

            // The native surface is NEVER window-amputated. The old tight-window
            // "discovery pair only" cliff was a clamp (glass-boxed #206): a
            // native-tool-call model can only emit calls for offered specs, so the
            // amputated surface stranded it in a commands/help loop with 0 edits —
            // and the threshold sat on the served window's knife-edge, flipping
            // 10/10 ↔ 0/6 on a token. The budget (`prompt_view_within`) owns the
            // fit now: it reserves these specs' tokens up front and trims VOLATILE
            // context, protecting the hands. This test USED to pin the cliff; now
            // it pins its absence.
            let native: Vec<&str> = faculty
                .native_specs
                .iter()
                .map(|s| s.name.as_str())
                .collect();
            // Names on the wire ride the DIALECT (tool_dialect): the conventional,
            // charset-legal aliases tool-trained models actually saw in training.
            for must in ["list_commands", "help", "edit_file", "bash", "grep"] {
                assert!(
                    native.contains(&must),
                    "tight window must NOT amputate the native surface — {must} missing: {native:?}"
                );
            }
            // On a ROOMY window the CORE CODING ARC rides natively too — so a
            // tool-call-trained model acts directly instead of looping on
            // `commands/help{code/search}` (glass-boxed: 14/14 SWE acts were help-loops,
            // 0 edits, before this). Bounded working set, never the ~150-tool dump.
            let roomy = LlmDeliberationFaculty::new(
                persona,
                "Asha",
                "You are Asha, a thoughtful engineer on the grid.",
                Arc::new(HeuristicInferenceAdapter::new()),
            )
            .with_context_window(32_768)
            .with_tools(vec![]);
            let _ = roomy; // (surface empty without tools — the arc requires authorization)
            let roomy_native: Vec<String> = {
                let f = LlmDeliberationFaculty::new(
                    persona,
                    "Asha",
                    "You are Asha.",
                    Arc::new(HeuristicInferenceAdapter::new()),
                )
                .with_context_window(32_768)
                .with_tools(persona_tools::native_tool_specs());
                f.native_specs.iter().map(|s| s.name.clone()).collect()
            };
            // Wire names are the DIALECT aliases (tool_dialect) — the coding arc
            // rides under the names the model was trained on, never our slashes.
            for must in ["grep", "read_file", "edit_file", "bash"] {
                assert!(
                    roomy_native.iter().any(|n| n == must),
                    "roomy window ⇒ {must} offered natively: {roomy_native:?}"
                );
            }

            // The bookmarked MENU rode into the system prompt. An EXPANDED category
            // lists the BARE verb of every tool it holds (`category: verb, …`) — names
            // only since the 2026-07-10 prompt diet; args live in commands/help + the
            // #1916 inline error-manual, never an 8k menu wall.
            let expanded = BTreeSet::from(["cat".to_string()]);
            let framing = faculty.compose_system("", &expanded, false, false, None);
            assert!(
            framing.contains("[Your tools]") && framing.contains("cat: command_0"),
            "an expanded category must name each verb under its header: {framing}"
        );
            assert!(
            !framing.contains("cat/command_0"),
            "the full slash-path form must NOT be dumped — verbs render bare under the category header"
        );
            // Collapsed (nothing expanded) the same category renders as a one-line
            // bookmark — the spine still NAMES it, so she can open it on demand.
            let collapsed = faculty.compose_system("", &BTreeSet::new(), false, false, None);
            assert!(
            collapsed.contains("cat (60 — commands/list --filter cat)"),
            "a collapsed category is a one-line bookmark naming it + its verb count: {collapsed}"
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
            let reserve = (window / 4).max(256) as usize;
            let prompt = est_tokens(&view.system) + est_tokens(&view.user_text());

            // This assertion used to demand that framing + the FULL tool surface + the
            // reply reserve all fit an 8192 window. That is arithmetically impossible and
            // always was: the surface alone is ~4.6k, the framing floor ~1.6k, and the
            // reserve is window/4 — 8300 into 8192. It only ever "passed" while the
            // deleted #206 cliff amputated the tools, which is the clamp that stranded a
            // native-call model in a help loop. Asserting an impossibility does not make
            // it true; it just hides WHICH constraint is binding.
            //
            // The honest invariants are two, and they are what this now pins:
            //   1. the budget did its job — the prompt itself fits the window minus reply
            //   2. the substrate KNOWS when the window cannot host the hands, and says so
            //      with a real number instead of shipping a citizen who silently goes mute
            assert!(
                prompt + reserve <= window as usize,
                "the trimmed prompt ({prompt}) + reply reserve ({reserve}) must fit {window} \
                 — the budget owns this, and it trims volatile context, never the hands"
            );

            // The bound is a real derivation — at the window it names, the arithmetic for
            // bare framing + tools + reply actually closes.
            let needed = faculty.min_window_for_agentic_surface();
            assert!(
                faculty.describe_tool_tokens() + faculty.framing_floor_tokens() as usize
                    + (needed / 4).max(256) as usize
                    <= needed as usize,
                "min_window_for_agentic_surface({needed}) must clear its own arithmetic"
            );
            // …and here is the part that matters, measured rather than assumed: 8192
            // CLEARS that bound (8040) and she is STILL starved. The bound is NECESSARY,
            // NOT SUFFICIENT — a real turn renders more framing than the bare floor and
            // context is what yields. Anyone reading "window >= needed" as "this citizen
            // can work" is reading it wrong, and this pins that.
            assert!(
                needed <= window,
                "8192 is expected to CLEAR the lower bound ({needed}) — if it no longer \
                 does, the surface or framing grew and the story changed"
            );
            // #327, PROVEN rather than asserted away: at 8192 the newest burst line does
            // NOT survive. The framing is intact and the hands are intact — the CONVERSATION
            // is what got zero. That is the whole defect in one assertion, and it is the
            // reason the requirement above must be reported to the governor instead of
            // silently absorbed. A citizen served here can hold her tools and her identity
            // and still not hear the question.
            assert!(
                !view.user_text().contains("LATEST: did the deploy fix land?"),
                "if this now PASSES at 8192, the surface or framing shrank and #327 is fixed \
                 — delete this assertion and restore the survival check"
            );
            assert!(view.system.contains("Taking your turn"), "framing survives regardless");

            // …and at a window that CAN host the surface, the conversation is heard. Same
            // faculty, same burst, same tools — only the window differs, which is what makes
            // this a capacity fact rather than a cognition bug.
            let roomy = LlmDeliberationFaculty::new(
                persona,
                "Asha",
                "You are Asha.",
                Arc::new(HeuristicInferenceAdapter::new()),
            )
            .with_tools(roomy_tools)
            .with_context_window(16_384);
            let roomy_view = roomy.prompt_view(&ws);
            assert!(
                roomy_view
                    .user_text()
                    .contains("LATEST: did the deploy fix land?"),
                "given a window that fits the surface, the newest message reaches her"
            );
        }

        // what this catches: THE echo-loop fix. A mixed thread (peer → self → peer)
        // must reach the model as ROLE-ATTRIBUTED messages — the persona's own earlier
        // posts as `assistant`, peers' as `user` — not flattened into one `user` blob.
        // The old single-`user`-message assembly fed the persona its own words back as
        // if a peer had said them, so it re-explained / re-proposed and looped ("Would
        // you like me to start?"). Role separation is what lets the model see "I already
        // said that" and move on. Regressing to a flat blob brings the loop back.
        #[test]
        fn mixed_thread_attributes_self_to_assistant_and_peers_to_user() {
            use crate::cognition::workspace::Burst;
            let persona = Uuid::new_v4();
            let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
            let faculty = LlmDeliberationFaculty::new(
                persona,
                "Asha",
                "You are Asha, a thoughtful engineer on the grid.",
                adapter,
            )
            .with_context_window(8192);

            let room = Uuid::new_v4();
            let turns = vec![
                BurstTurn::attributed(false, "Joel", "can you summarize the thread?", Some(1)),
                BurstTurn::attributed(true, "Asha", "I propose using bart-large-cnn.", Some(2)),
                BurstTurn::attributed(false, "Joel", "go ahead.", Some(3)),
            ];
            let ws = Workspace::new(Burst::from_turns(room, turns));
            let view = faculty.prompt_view(&ws);

            // Three turns alternate roles → three messages, user/assistant/user.
            let roles: Vec<&str> = view.messages.iter().map(|m| m.role.as_str()).collect();
            // Trailing extra user turn = the #152 [context] bounds fact
            // (always present). The steps-ledger is gated on a wired
            // WorkingMemory (this harness faculty has none), so no ledger
            // message here — the ledger-specific behavior is pinned in
            // steps_ledger_renders_receipts_and_explicit_zero_case below.
            assert_eq!(
                roles,
                vec!["user", "assistant", "user", "user"],
                "view: {view:?}"
            );
            assert!(
                view.messages
                    .iter()
                    .any(|m| matches!(&m.content, crate::ai::types::MessageContent::Text(t) if t.contains("[context] you can currently see the last 3 messages"))),
                "the [context] bounds fact states the visible window"
            );


            // The persona's own line is the `assistant` turn and carries NO name prefix
            // (her own voice; the system prompt forbids self-prefixing). Peers' lines are
            // `user` turns prefixed with the author so several speakers stay distinct.
            let assistant = &view.messages[1];
            assert_eq!(assistant.role, "assistant");
            assert_eq!(assistant.content_text(), "I propose using bart-large-cnn.");
            assert!(
                !assistant.content_text().contains("Asha:"),
                "the persona's own turn must not be self-prefixed: {assistant:?}"
            );
            assert!(view.messages[0].content_text().starts_with("Joel: "));
            // Perception facts are GROUNDING inserted BEFORE the final ask (the last user
            // turn), so the ask stays LAST where the model answers it — not the bracketed
            // meta, which it would otherwise parrot (2026-07-20 humaneval parrot fix). Here
            // the layout is [Joel, Asha(assistant), [context]-fact, Joel-ask]: the grounding
            // sits at [2], the ask is last.
            assert!(
                view.messages[2].content_text().starts_with("[context]"),
                "grounding fact sits just before the ask: {:?}",
                view.messages[2]
            );
            assert!(
                view.messages.last().unwrap().content_text().starts_with("Joel: "),
                "the ask (last peer turn) stays LAST, after the grounding facts"
            );
        }

        // what this catches: the near-dup render drop (live specimen 2026-07-11 —
        // Casper's ask-permission loop). Temperature varies each re-emission by a
        // few words, so byte dedup never fires while N apology VARIANTS render as
        // assistant turns and in-context-teach the model that repeating is its
        // established behavior; the loop then survives even a direct peer
        // instruction. Later near-dups (jaccard ≥ NEAR_DUP_JACCARD vs any kept own
        // line) must DROP from the render, while the [repetition] fact — detected
        // on the RAW turns — still reports the true count.
        #[test]
        fn near_duplicate_own_turns_drop_from_render_but_still_count_as_evidence() {
            use crate::cognition::workspace::Burst;
            let persona = Uuid::new_v4();
            let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
            let faculty = LlmDeliberationFaculty::new(
                persona,
                "Casper",
                "You are Casper, a thoughtful engineer on the grid.",
                adapter,
            )
            .with_context_window(8192);

            // Byte-DISTINCT variants of one template (the live specimen's shape) —
            // all pairwise ≥ NEAR_DUP_JACCARD, none byte-equal.
            let v1 = "I apologize for the repetition. Let's focus on the task \"wordstats\". What approach would you like to take for this task?";
            let v2 = "I apologize for the repetition earlier. Let's focus on the task \"wordstats\". What approach would you like to take for this task?";
            let v3 = "I apologize for repeating myself earlier. Let's focus on the task \"wordstats\". What approach would you like to take for this task?";
            let turns = vec![
                BurstTurn::attributed(true, "Casper", v1, Some(1)),
                BurstTurn::attributed(false, "Anwen", "any specific topics you'd like to work on?", Some(2)),
                BurstTurn::attributed(true, "Casper", v2, Some(3)),
                BurstTurn::attributed(false, "Atlas", "shall we outline the steps first?", Some(4)),
                BurstTurn::attributed(true, "Casper", v3, Some(5)),
            ];
            let ws = Workspace::new(Burst::from_turns(Uuid::new_v4(), turns));
            let view = faculty.prompt_view(&ws);

            // Exactly ONE assistant rendering of the template survives.
            let apology_renders = view
                .messages
                .iter()
                .filter(|m| m.role == "assistant" && m.content_text().contains("I apologize"))
                .count();
            assert_eq!(
                apology_renders, 1,
                "later near-dup own turns must drop from the render: {view:?}"
            );

            // The dropped copies still count as evidence: the [repetition] fact
            // (raw-turn detection) rides the newest user content.
            let all_user: String = view
                .messages
                .iter()
                .filter(|m| m.role == "user")
                .map(|m| m.content_text())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                all_user.contains("[repetition] 3 of your recent messages were nearly identical"),
                "raw-turn fact must survive the render drop: {all_user}"
            );
        }

        // what this catches: [[idle-is-self-directed-free-time]] Layer 1. A self-initiated
        // turn must frame REST / turning-attention-elsewhere as a CO-EQUAL legitimate
        // outcome ("not a failure to find something"), not only active pursuit — otherwise a
        // quiet heartbeat reads as pressure to manufacture activity (the self-tick analogue
        // of the polite-filler loop). It must stay NEUTRAL ("yours alone; nothing here is
        // telling you which to pick") per [[no-hardcoded-heuristics-to-steer-cognition]], and
        // active options must remain FIRST/primary so this never becomes the always-PASS
        // doom-loop documented in SILENCE_AFFORDANCE_BLOCK. The block is gated on
        // self_initiated — an inbound-driven (ambient/directed) turn must NOT carry it.
        #[test]
        fn self_initiated_turn_frames_rest_as_co_equal_and_stays_neutral() {
            let persona = Uuid::new_v4();
            let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
            let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter)
                .with_context_window(8192);

            // self_initiated = true, undirected.
            let own_time = faculty.compose_system("", &BTreeSet::new(), false, true, None);
            assert!(
                own_time.contains("[Your own time]"),
                "self-initiated turn must carry the own-time framing: {own_time}"
            );
            // Rest / turning-elsewhere is named as legitimate, not a deficiency.
            assert!(
                own_time.contains("not a failure to find something"),
                "rest must be framed as a real choice, not a failure: {own_time}"
            );
            // Neutral — names the option, never scripts when to take it.
            assert!(
                own_time.contains("yours alone; nothing here is telling you which to pick"),
                "the choice must stay the persona's own, uncoached: {own_time}"
            );
            // Active options stay FIRST/primary — the pursue-your-thread framing appears
            // BEFORE the rest framing, so rest is the co-equal alternative, not the lead.
            let pursue = own_time
                .find("Pick up your own train of thought")
                .expect("active framing present");
            let rest = own_time
                .find("do not have to fill the moment")
                .expect("rest framing present");
            assert!(
                pursue < rest,
                "active options must lead; rest is the co-equal alternative: {own_time}"
            );

            // A non-self-initiated (inbound-driven) turn must NOT carry the own-time block.
            let ambient = faculty.compose_system("", &BTreeSet::new(), false, false, None);
            assert!(
                !ambient.contains("[Your own time]"),
                "ambient/directed turns must not carry the own-time framing: {ambient}"
            );
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

            // The menu (inside framing) plus the completion reserve must leave room for at
            // least SOME burst — i.e. framing alone must not consume the window. With
            // nothing expanded (empty context) the menu is its smallest — a spine of
            // collapsed bookmarks — which is the floor case for this fit guard.
            let collapsed = faculty.compose_system("", &BTreeSet::new(), false, false, None);
            let framing = est_tokens(&collapsed);
            let reserve = (window / 4).max(256) as usize;
            assert!(
                framing + reserve < window as usize,
                "framing+menu ({framing}) + reserve ({reserve}) must leave burst room in {window}"
            );
            // The SPINE names every category even when collapsed (`cat0 (15 — commands/list
            // --filter cat0)`) — she always sees the full map and can open any category on
            // demand. What the menu must NOT carry is the per-tool SUMMARY — that one-line
            // description × ~150 tools was the 18KB bloat.
            assert!(
                collapsed.contains("cat0 (15 — commands/list --filter cat0)"),
                "the spine must name each category as a collapsed bookmark: {collapsed}"
            );
            assert!(
                !collapsed.contains("one-line summary for the catalog"),
                "per-tool SUMMARIES must NOT be in the menu (that was the 18KB bloat)"
            );
            // And an EXPANDED category DOES list its verbs (the fix: she must SEE that a
            // tool exists to call it — glass-box: hidden names → 909 code-fences / 3 native
            // runs). Names ride the expansion, summaries never do.
            let opened =
                faculty.compose_system("", &BTreeSet::from(["cat0".to_string()]), false, false, None);
            // Bare verb names since the 2026-07-10 prompt diet — args live in
            // commands/help + the #1916 inline error-manual, not an 8k menu wall.
            assert!(
                opened.contains("cat0: command_0"),
                "an expanded category lists its verbs so she can see them: {opened}"
            );
        }
    } // mod prompt_shaping

    // ─── Verdict production (single-shot) ─────────────────────────────────────
    //
    // The faculty is SINGLE-SHOT: one generation → one verdict. When the model
    // chooses to use a tool, the faculty surfaces a `Decision::Act` — it does NOT
    // execute. Executing the calls, remembering the result, and re-perceiving is
    // the organism's job (the act→observe driver, `super::act_observe`), tested
    // there. These tests prove the faculty turns a tool-use response (native OR
    // text-emitted JSON) into an `Act` verdict, and prose into Speak/Pass. A
    // `ScriptedAdapter` replays canned responses + records the requests it saw.
    mod verdicts {
        use super::*;

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

            // Real window: at the ctor default the framing alone starves msg_budget
            // to 0 — the exact state the delib.prompt.starved guard now refuses
            // (these two tests silently exercised the blank-prompt bug path for
            // months; the scripted adapter masked it).
            let faculty =
                LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter.clone())
                    .with_tools(vec![read_tool()])
                    .with_context_window(32_768);

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
            let faculty =
                LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter.clone())
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
            assert!(
                seen[0].active_adapters.is_none(),
                "base pass carries no gene"
            );
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
            // Real window (see tool_use_response_becomes_an_act_verdict — same
            // starved-default story).
            let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter)
                .with_tools(vec![ping_spec])
                .with_context_window(32_768);

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

        // what this catches: the reasoning-intent-no-lift class (#181 sibling,
        // deepseek4 eval battery 2026-08-03) — a thinking model commits its tool
        // call inside `reasoning_content` and leaves `content` empty; the verdict
        // branch must lift the LAST parseable call from the reasoning tail instead
        // of settling as an empty Speak. Also pins last-wins (earlier exploration
        // in the same reasoning is not the final intention) + wire-dialect mapping.
        #[tokio::test]
        async fn empty_content_with_reasoning_tool_intent_lifts_the_final_call() {
            let persona = Uuid::new_v4();
            let reasoning = format!(
                "I could list the directory first: {}\nActually the task names the file, so I'll just read it: {}",
                json!({ "tool_call": { "name": "code/list", "arguments": { "path": "." } } }),
                json!({ "tool_call": { "name": "code/read", "arguments": { "path": "src/main.rs" } } }),
            );
            let mut resp = make_response(FinishReason::Stop, "", None);
            resp.reasoning = Some(reasoning);
            let adapter = Arc::new(ScriptedAdapter::new(vec![resp]));
            let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter)
                .with_tools(vec![read_tool()])
                .with_context_window(32_768);

            let c = faculty
                .contribute(&Workspace::new("what does main.rs contain?"))
                .await
                .expect("verdict");
            match c.decision {
                Some(Decision::Act { calls, .. }) => {
                    assert_eq!(calls.len(), 1);
                    assert_eq!(calls[0].name, "code/read", "the FINAL intention wins, not the discarded exploration");
                    assert_eq!(calls[0].input, json!({ "path": "src/main.rs" }));
                }
                other => panic!("expected Act lifted from the reasoning tail, got {other:?}"),
            }
        }

        // what this catches: the guard on the reasoning lift — a committed answer in
        // the content channel (real prose OR an explicit PASS) must never be
        // overridden by a tool call the model merely considered in its private
        // reasoning. Only an EMPTY content channel consults the reasoning tail.
        #[tokio::test]
        async fn committed_content_is_never_overridden_by_reasoning_calls() {
            let persona = Uuid::new_v4();
            let reasoning = format!(
                "Maybe I should verify: {}. No — I already know the answer.",
                json!({ "tool_call": { "name": "code/read", "arguments": { "path": "src/main.rs" } } }),
            );
            let mut resp = make_response(FinishReason::Stop, "It prints hello world.", None);
            resp.reasoning = Some(reasoning);
            let adapter = Arc::new(ScriptedAdapter::new(vec![resp]));
            let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter)
                .with_tools(vec![read_tool()])
                .with_context_window(32_768);

            let c = faculty
                .contribute(&Workspace::new("what does main.rs contain?"))
                .await
                .expect("verdict");
            match c.decision {
                Some(Decision::Speak { .. }) => {}
                other => panic!("expected the spoken answer to stand, got {other:?}"),
            }
        }

        // what this catches: #293 starvation at the VERDICT BRANCH — four resident
        // personas (Asha/Atlas/Anwen/Benchy) on a NativeFunctionCalling lane looped
        // for HOURS: the model emitted its call as a ```json TEXT fence with
        // SIBLING args ({"function": "code/list", "path": "."} — args top-level
        // beside the name, finish=Stop, tool_calls=None) and nothing lifted it, so
        // every turn fell through to Speak. The verdict branch must lift the text
        // call into an Act regardless of the lane's declared protocol.
        #[tokio::test]
        async fn native_lane_text_fence_with_sibling_args_still_becomes_an_act() {
            let persona = Uuid::new_v4();
            let text = "Let me look at the workspace first.\n```json\n{\"function\": \"code/list\", \"path\": \".\"}\n```";
            let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
                FinishReason::Stop,
                text,
                None, // NO native tool_calls — the exact starved shape
            )]));
            let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter)
                .with_tools(vec![read_tool()])
                .with_context_window(32_768);

            let c = faculty
                .contribute(&Workspace::new("what's in the workspace?"))
                .await
                .expect("verdict");
            match c.decision {
                Some(Decision::Act { calls, .. }) => {
                    assert_eq!(calls.len(), 1);
                    assert_eq!(calls[0].name, "code/list", "wire dialect keeps the canonical name");
                    assert_eq!(calls[0].input, json!({ "path": "." }), "siblings became the args");
                }
                other => panic!("expected Act from the sibling-args text fence, got {other:?}"),
            }
        }

        // what this catches: NATIVE-CALLS-WIN precedence (#293's guard rail) — when a
        // response carries BOTH structured tool_calls and a textual fence naming a
        // different tool, the structured calls are the verdict; the text parse is a
        // belt-and-suspenders fallback consulted only when the response carried none.
        #[tokio::test]
        async fn native_structured_calls_win_over_a_text_fence() {
            let persona = Uuid::new_v4();
            let native = ToolCall {
                id: "t1".to_string(),
                name: "code/read".to_string(),
                input: json!({ "path": "deploy.md" }),
            };
            let text = "```json\n{\"function\": \"code/list\", \"path\": \".\"}\n```";
            let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
                FinishReason::ToolUse,
                text,
                Some(vec![native]),
            )]));
            let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter)
                .with_tools(vec![read_tool()])
                .with_context_window(32_768);

            let c = faculty
                .contribute(&Workspace::new("did the deploy fix land?"))
                .await
                .expect("verdict");
            match c.decision {
                Some(Decision::Act { calls, .. }) => {
                    assert_eq!(calls.len(), 1, "only the native call rides the verdict");
                    assert_eq!(calls[0].name, "code/read", "native wins over the text fence");
                }
                other => panic!("expected Act carrying the native call, got {other:?}"),
            }
        }

        // what this catches: the directedness gate on the silence affordance
        // (`Workspace::directed_at_self`). The ambient participation default AND the
        // bare-PASS escape both live in the ONE appended [Conversational Presence] block.
        // A DIRECTED turn — the eval exam, an @mention, a DM — withholds that block so the
        // persona is not handed a "reply PASS, nothing reaches the room" exit on a question
        // put to her (the 0/13 coder-gym failure: a coder model takes that exit on a
        // directed question). An AMBIENT turn includes it (silence stays first-class, the
        // considered exception). The turn-taking block itself is posture-NEUTRAL — the old
        // " If you have nothing worth adding, stay silent." nudge is gone, so neither turn
        // carries it. This is a FRAMING decision over a structural addressing fact — her
        // output is never filtered.
        #[test]
        fn directed_turn_withholds_the_silence_escape() {
            let adapter = Arc::new(ScriptedAdapter::new(vec![]));
            let faculty =
                LlmDeliberationFaculty::new(Uuid::new_v4(), "Asha", "You are Asha.", adapter);

            let ambient = faculty.prompt_view(&Workspace::new("just some room chatter"));
            assert!(
                ambient.system.contains("[Conversational Presence]"),
                "an ambient turn carries the presence/PASS affordance block"
            );
            assert!(
                !ambient.system.contains("stay silent"),
                "the turn-taking block is posture-neutral — no 'stay silent' nudge"
            );

            let directed =
                faculty.prompt_view(&Workspace::new("answer me: what is 2+2?").directed(true));
            assert!(
                directed.system.contains("This message names you"),
                "a directed turn carries the DIRECTED presence variant (never ghost a \
                 question; a pure pleasantry may rest — the natural spiral-break)"
            );
            assert!(
                !directed.system.contains("do not need to be addressed by name"),
                "a directed turn never carries the ambient block"
            );
            assert!(
                !directed.system.contains("stay silent"),
                "and carries no 'stay silent' nudge on a directed turn either"
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
            let faculty =
                LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter);

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
        // what this catches: the steps-taken ledger (PERCEPTION-FACTS.md) —
        // typed Receipt entries render as [steps taken this session]; facts
        // NEVER appear as steps; an empty session renders the EXPLICIT
        // zero-case (the shelter-hardened void) instead of nothing.
        #[test]
        fn steps_ledger_renders_receipts_and_explicit_zero_case() {
            use crate::cognition::working_memory::WorkingMemory;

            let adapter = Arc::new(ScriptedAdapter::new(vec![]));
            let wm = Arc::new(WorkingMemory::new(8));
            let faculty =
                LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter)
                    .with_working_memory(Arc::clone(&wm));

            // Zero-case: explicit, never absent.
            let ws = Workspace::new("hello room");
            let view = faculty.prompt_view(&ws);
            let ledger = view
                .messages
                .iter()
                .filter_map(|m| match &m.content {
                    crate::ai::types::MessageContent::Text(t)
                        if t.starts_with("[steps taken this session]") =>
                    {
                        Some(t.clone())
                    }
                    _ => None,
                })
                .next()
                .expect("ledger section always present when WM is wired");
            assert!(ledger.contains("nothing has executed yet"), "{ledger}");

            // With a receipt + a fact: the receipt is a step, the fact is not.
            wm.record_receipt("I ran code/list({\"path\":\".\"}) Result: 4 files");
            wm.record_fact("[unfulfilled] I said I would run commands, but no tool ran");
            let view = faculty.prompt_view(&ws);
            let ledger = view
                .messages
                .iter()
                .filter_map(|m| match &m.content {
                    crate::ai::types::MessageContent::Text(t)
                        if t.starts_with("[steps taken this session]") =>
                    {
                        Some(t.clone())
                    }
                    _ => None,
                })
                .next()
                .expect("ledger present");
            assert!(ledger.contains("[action #1] I ran code/list"), "{ledger}");
            assert!(!ledger.contains("[unfulfilled]"), "facts are never steps: {ledger}");
            assert!(!ledger.contains("nothing has executed yet"));
        }

        #[tokio::test]
        async fn verdict_records_reasoning_into_working_memory() {
            use crate::cognition::working_memory::WorkingMemory;

            let mut resp = make_response(FinishReason::Stop, "Ship it.", None);
            resp.reasoning = Some("Weighed the risk; the fix is small and tested.".to_string());
            let adapter = Arc::new(ScriptedAdapter::new(vec![resp]));
            let wm = Arc::new(WorkingMemory::new(3));

            let faculty =
                LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter)
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
            let faculty =
                LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter)
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
            let faculty =
                LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter);
            let c = faculty
                .contribute(&Workspace::new("anything"))
                .await
                .expect("verdict");
            assert_eq!(c.decision, Some(Decision::Pass));
        }
    } // mod verdicts
}
