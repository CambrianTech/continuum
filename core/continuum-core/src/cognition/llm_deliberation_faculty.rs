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

/// The act-latency target the CONVERSATION fill budgets against — how long a
/// turn's history prefill is allowed to take at [`CONSERVATIVE_PREFILL_TOKENS_PER_S`].
/// From the act-latency law: human expectations set the bar; a turn that spends
/// minutes re-reading history before thinking is disqualified regardless of how
/// smart the answer is. 30s of prefill is already generous — the point is that
/// window HEADROOM above this is reserve, not default fill.
// context-budget-exempt: a TIME target, not a token budget — the token cap derives from time x measured-class rate at the call site
const PREFILL_TARGET_SECONDS: usize = 30;

/// Deliberately UNDER every measured live ingest rate (636–676 t/s on the
/// M-series reference box, 2026-09-01 probes) so the derived cap over-admits
/// rather than starves. Upgrade path: derive from the live
/// `inference.prefill.complete` ingest measurements once the segment probe
/// lands — capacity follows measurement.
// context-budget-exempt: a measured throughput floor (tokens/second), not a context-size constant
const CONSERVATIVE_PREFILL_TOKENS_PER_S: usize = 500;

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
    /// Token cost of serializing `native_specs` into the request — memoized at
    /// [`Self::rebuild_tool_surface`] time because the specs are static between
    /// rebuilds while [`Self::describe_tool_tokens`] is consulted on EVERY
    /// prompt assembly (per-turn serde of ~a dozen schemas, pure waste).
    tool_surface_tokens: usize,
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
            binding: model_binding(adapter, None, crate::cognition::serving_plan::MIN_SERVE_CTX),
            temperature: DEFAULT_TEMPERATURE,
            tools: Vec::new(),
            native_specs: Vec::new(),
            tool_surface_tokens: 0,
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
            self.tool_surface_tokens = 0;
            return;
        }
        // Offer the working set in the WIRE DIALECT, charset-legal per the OpenAI
        // function-name spec our slashed names violate. WHICH name a command is
        // offered under is a per-model POLICY ([`tool_dialect::offer_style_for`],
        // the adaptive-surface seam): our canonical `code_read` to converge a model
        // onto our namespace, or its trained reflex `read_file` to meet its tuning.
        // Either way calls map back to canonical commands on return (ONE section:
        // [`crate::cognition::tool_dialect`]). [[joel-boundary-design-values]]
        let style =
            crate::cognition::tool_dialect::offer_style_for(self.binding.load().model.as_deref());
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
        self.tool_surface_tokens = Self::tool_surface_tokens_of(&self.native_specs);
    }

    /// Serde-and-count the tool surface ONCE per rebuild — the only place the
    /// per-spec serialization cost is ever paid.
    fn tool_surface_tokens_of(specs: &[NativeToolSpec]) -> usize {
        // context-budget-exempt: fixed per-tool schema overhead in the template — same reason as PER_MESSAGE_TEMPLATE_TOKENS
        const PER_TOOL_TEMPLATE_MARGIN_TOKENS: usize = 8;
        specs
            .iter()
            .map(|spec| {
                let serialized = serde_json::to_string(spec).unwrap_or_default();
                est_tokens(&serialized) + PER_TOOL_TEMPLATE_MARGIN_TOKENS
            })
            .sum()
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
    /// [`Self::completion_reserve_within`] applies, so this cannot drift from the live reserve.
    ///
    /// With `D = COMPLETION_SHARE_DENOM`:
    /// `tools + framing <= window - window/D`  ⇒  `window >= ceil((tools + framing) * D / (D-1))`
    ///
    /// DERIVED from the denominator, not transcribed. It read `div_ceil(3).saturating_mul(4)`,
    /// which is that algebra solved for D=4 and frozen — so changing the split would have left
    /// this bound silently computing the old one, and the "you are structurally mute below
    /// here" sensor would have been measuring a window nobody serves.
    pub fn min_window_for_agentic_surface(&self) -> u32 {
        let fixed = self.describe_tool_tokens() as u32 + self.framing_floor_tokens();
        fixed
            .div_ceil(Self::COMPLETION_SHARE_DENOM - 1)
            .saturating_mul(Self::COMPLETION_SHARE_DENOM)
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
            reserve_tokens = self.completion_reserve_within(window),
            "served window is too small to hold framing + tools + a reply — this citizen \
             will struggle to ACT (she can still speak). Raise the window to at least \
             `needed`, or serve a model with a larger context."
        );
    }


    /// What the prompt MUST carry before any reserve may claim a token: the tool schemas,
    /// the bare framing, and room for at least one message.
    ///
    /// # The burst is mandatory, and leaving it out is why the floor was still wrong
    ///
    /// The first version of this floor was `tools + framing` — which correctly stopped the
    /// reserve from eating framing, and still produced a citizen who could not hold a
    /// conversation. Measured at `MIN_SERVE_CTX` (2,048) with the share at 1/2: framing alone
    /// is 1,506, the reserve yields to 542, and what remains for the message she is answering
    /// is ZERO. The reserve was behaving perfectly; the floor was simply not describing the
    /// whole obligation. A prompt with no room for the turn's own message is not a prompt.
    ///
    /// The burst allowance is [`Self::COMPLETION_FLOOR_TOKENS`] — deliberately the SAME term
    /// as the smallest usable reply, not a new number. The symmetry is the argument: a turn
    /// needs at least as much room for what she is answering as for the answer itself, and
    /// inventing a second constant here is exactly what the de-hardcode guard exists to stop.
    fn mandatory_prompt_floor(&self) -> u32 {
        self.describe_tool_tokens() as u32
            + self.framing_floor_tokens()
            + Self::COMPLETION_FLOOR_TOKENS
    }

    /// The reply's reserve, YIELDING to what the prompt must mandatorily carry.
    ///
    /// # Why the share alone could never be raised (proven 2026-08-20)
    ///
    /// A bare SHARE of the window — the form this replaced — is a share and nothing else, so raising it
    /// takes room the prompt has no choice but to use. Flipping the denominator 4 → 2 broke
    /// five invariants by name — `prompt_plus_completion_cap_never_exceeds_the_served_window`
    /// and `prompt_view_stays_within_the_served_window` among them — because the packer
    /// admits framing and the tool surface whether or not the budget covers them. The
    /// overshoot was never the packer misbehaving: it is the floor correctly refusing to
    /// compress. Holding the reserve fixed while the floor grows is the actual error.
    ///
    /// So the reserve takes the SMALLER of "the share we want" and "what is left after the
    /// prompt's irreducible cost". Both floor terms are pure over `&self` — the tool schemas
    /// and the bare framing — which is what lets `prompt_view_within` and
    /// `build_request_within` compute the SAME number without threading a workspace through.
    /// That agreement is load-bearing: the prompt is sized to leave exactly this, and
    /// generation is capped at exactly this, so `prompt + completion` cannot reach `n_ctx`.
    ///
    /// Reuses the identical `fixed` term as [`Self::min_window_for_agentic_surface`], so the
    /// bound that WARNS about a too-small window and the reserve that LIVES within one can
    /// never disagree about what "mandatory" means.
    fn completion_reserve_within(&self, context_window: u32) -> u32 {
        let mandatory = self.mandatory_prompt_floor();
        let share = context_window / Self::COMPLETION_SHARE_DENOM;
        // THE MEASURED RESERVE (the "output-p95 riding the working-set registry"
        // endgame both the ceiling and the denominator comments promised). The
        // bare share is a cold-start PRIOR, not a measurement: at a 29,440
        // window it reserved 14,720 tokens for replies measured at 0.2–2.5k,
        // and the prompt paid — grounding got 195 tokens and the room board
        // dropped for want of 137, which is the meta-loop spiral live
        // (2026-08-31, delib.context.render). Once this mind's emissions are
        // measured, the reserve is peak×2 — headroom for a longer thought,
        // and capped turns already record at double, so a too-small reserve
        // grows itself back within a turn rather than freezing (the
        // measure-the-clamp trap). Floored at MIN_SERVE_CTX so one tiny ack
        // can never strangle the next long thought, and never ABOVE the
        // cold-start share — measurement only ever returns prompt room, the
        // prior already being the most generous defensible ask.
        let measured = self
            .working_set
            .as_ref()
            .and_then(|reg| reg.emission_of(self.persona_id))
            // A single observation is a measurement, but ONE reply sizes the
            // reserve for every turn after it — ask for a few before shrinking.
            .filter(|e| e.turns >= 3)
            .map(|e| {
                e.peak_tokens
                    .saturating_mul(2)
                    .max(crate::cognition::serving_plan::MIN_SERVE_CTX)
            });
        // `.max(FLOOR)` last and deliberately: below `MIN_SERVE_CTX` the floor can exceed the
        // whole window, and a zero reserve is a mute citizen — strictly worse than a prompt
        // that overshoots and gets trimmed. That regime is not reachable in production
        // (`window_for` floors at MIN_SERVE_CTX) but the ordering should not depend on it.
        measured
            .unwrap_or(share) // JUSTIFIED unwrap_or: no measurement yet = the documented cold-start PRIOR (the share); the honest absence has a named owner above
            .min(share)
            .min(Self::COMPLETION_CEILING_TOKENS)
            .min(context_window.saturating_sub(mandatory))
            .max(Self::COMPLETION_FLOOR_TOKENS)
    }

    /// Absolute ceiling on the reply reserve. A bare RATIO scales its waste
    /// with the window: at the 166k lane the /2 share reserved 83,200 tokens
    /// for replies that measure 0.2-2.5k, squeezing the PROMPT to 73k and
    /// forcing the packer to amputate accumulated working memory mid-task
    /// (measured 2026-08-23: demand 112-118k against the squeezed budget,
    /// context trimmed at act 27 of a 32-act task). Derived, not declared:
    /// 8× the smallest servable window ≈ 16k — nine minutes of decode at the
    /// measured ~30 tok/s, far above any observed turn (a thinking model's
    /// longest measured emission this round was ~2.5k). The honest endgame is
    /// output-p95 measurement riding the working-set registry pattern; until
    /// that lands this ceiling stops the ratio's unbounded growth without
    /// ever clipping a real reply.
    const COMPLETION_CEILING_TOKENS: u32 = crate::cognition::serving_plan::MIN_SERVE_CTX * 8;

    /// The reply's share of the served window, as a DENOMINATOR: reply gets `window/N`.
    ///
    /// One home for a fraction that was previously re-spelled as a bare `/ 4` in five
    /// places — the reserve itself, the `div_ceil(3).saturating_mul(4)` algebra in
    /// [`Self::min_window_for_agentic_surface`], and three test mirrors. That duplication is
    /// why changing the split kept breaking things quietly: the tests re-derived the OLD
    /// fraction from scratch, so they kept passing while no longer measuring the invariant
    /// they were written for — a silent failure, the worst outcome for a guard.
    ///
    /// Not a hardcoded context size (the de-hardcode guard keys on WINDOW/TOKEN-named
    /// constants holding bare literals): this is a ratio, and the window it divides is the
    /// live served one.
    const COMPLETION_SHARE_DENOM: u32 = 2;

    /// Floor so a tiny window still yields a usable reply, and the same term the prompt
    /// floor uses for a minimum burst.
    ///
    /// DERIVED from `MIN_SERVE_CTX`, not declared: an eighth of the smallest window the
    /// serving stack will ever hand out. A bare `256` here was caught by
    /// `context_budget::no_new_hardcoded_context_or_prompt_size_constant_anywhere_in_the_crate`
    /// — correctly, and the catch is worth recording: the constant carries `TOKENS` in its
    /// name, the guard keys on exactly that, and every prompt-shaping test stayed GREEN
    /// while it was wrong. Only the full suite sees this guard. Deriving it also means the
    /// floor tracks the substrate floor for free if `MIN_SERVE_CTX` ever moves, instead of
    /// becoming a second opinion about how small a window can get.
    const COMPLETION_FLOOR_TOKENS: u32 = crate::cognition::serving_plan::MIN_SERVE_CTX / 8;

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
        // The room this turn speaks in — the ACTIVITY identity. It is authoritative
        // for the serving slot lease: warm KV is per (persona, room), so a persona
        // running N concurrent activities (N detached solves) each keeps its own warm
        // slot instead of all N collapsing onto one and thrashing (the 2026-08-26
        // KV-reuse-0% bug). None only for the roomless test rig.
        room_id: Option<uuid::Uuid>,
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
            max_tokens: Some(self.completion_reserve_within(binding.context_window)),
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
            room_id: room_id.map(|r| r.to_string()),
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
        .with_metrics(metrics_from(&self.persona_name, resp))
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
        Decision::pass()
    }

    /// She called the yield verb — settle the turn as the silence it names.
    ///
    /// The STRUCTURED half of #271/#264. A citizen with nothing to add has, until now,
    /// had no way to say so except to write a paragraph announcing it — which is itself
    /// a room message that wakes the next peer into announcing theirs. Now silence is a
    /// verb, and recognising it is a NAME match on a verb we defined (protocol), not a
    /// phrase match on prose (semantics). See
    /// [`super::persona_tools::verdict_tool_specs`] for why this is not a command.
    ///
    /// Checked BEFORE `act_verdict` on both lift paths, so a yield never reaches the
    /// authorization gate or the executor: there is no world-effect to authorize, and
    /// routing it as an `Act` would burn an act from the budget and re-enter the settle
    /// loop — the opposite of ending the turn.
    ///
    /// If she calls the yield ALONGSIDE real work, the work wins: a turn that both did
    /// something and declined to speak is an act with nothing to say, and dropping the
    /// act to honour the yield would silently discard work she actually did.
    fn yield_verdict(&self, calls: &[crate::ai::types::ToolCall]) -> Option<Contribution> {
        if !calls
            .iter()
            .any(|c| super::persona_tools::is_yield_turn(&c.name))
        {
            return None;
        }
        if calls
            .iter()
            .any(|c| !super::persona_tools::is_yield_turn(&c.name))
        {
            return None;
        }
        crate::probe!(
            class = "persona.verdict.yield_turn",
            persona = %self.persona_name,
            "she yielded the turn through the structured verb — silent Pass, no room message"
        );
        Some(Contribution::verdict(
            Decision::pass(),
            0.9,
            format!("{} yielded the turn (yield_turn)", self.persona_name),
        ))
    }

    /// Turn the model's final text into a participation verdict. `salience` is
    /// the faculty's own confidence in its verdict — a placeholder for a model-
    /// derived signal (logprob / uncertainty), NOT a caste weight; it's how sure
    /// THIS mind is, which the arbiter integrates.
    fn verdict(&self, resp: &TextGenerationResponse, ws: &Workspace) -> Contribution {
        let decision = self.silence_a_parroted_draft(decision_from_response(&resp.text), ws);
        let (salience, reasoning) = match &decision {
            Decision::Pass { reason } => (
                0.5,
                match reason {
                    Some(r) => format!("{} passed: {r}", self.persona_name),
                    None => format!("{} chose silence (PASS)", self.persona_name),
                },
            ),
            _ => (
                0.85,
                format!(
                    "{} deliberated over the assembled context",
                    self.persona_name
                ),
            ),
        };
        Contribution::verdict(decision, salience, reasoning)
            .with_metrics(metrics_from(&self.persona_name, resp))
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

    /// `composition` is `(context_window, framing, fitted_conversation, ctx_floor)`
    /// — the terms that PRODUCED `budget_tokens`. Carried purely so this probe and
    /// `delib.turn.demand` can be reconciled against each other: read separately
    /// they disagreed (Atlas showed `after_framing 9,243 − conv 4,334` yet rendered
    /// `budget=287`) and nothing on hand could say which was wrong, because the two
    /// numbers lived in different probes with no shared term. A seam that can only
    /// be reasoned about by cross-referencing two records is a seam that gets
    /// guessed at ([[a-probe-that-can-only-fail-is-worse-than-no-probe]]).
    fn render_assembled_context_within(
        &self,
        ws: &Workspace,
        budget_tokens: usize,
        composition: (u32, usize, usize, usize, usize),
    ) -> String {
        let (context_window, framing_tokens, conversation_tokens, ctx_floor, after_framing) =
            composition;
        // How much of `conversation_tokens` is the TRAILING tier (working-memory
        // ledger, full latest result, perception facts).
        //
        // A SUBSET, not a separate term — `messages_unfitted` pushes trailing
        // contributions into `messages`, so `used_msg_tokens` already counts them.
        // Named `conv_trailing_share` for exactly that reason: emitted as
        // `trailing_tokens` it read as a fourth claimant and I subtracted it twice,
        // inflating an unexplained gap that had not actually changed. A probe field
        // that invites a double-count is worse than no field
        // ([[a-probe-that-can-only-fail-is-worse-than-no-probe]]).
        let conv_trailing_share: usize = ws
            .broadcast
            .iter()
            .filter(|c| c.decision.is_none() && c.trailing)
            .map(|c| est_tokens(&c.content))
            .sum();
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
                context_window,
                framing_tokens,
                conversation_tokens,
                conv_trailing_share,
                ctx_floor,
                after_framing,
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
            // ACTIVE-WORK sorts first, then salience. The ctx_floor reservation above
            // is sized to her held card's content — but a reservation only holds if
            // the reserved claimant is also FIRST in line: selection is greedy, so a
            // higher-salience recall (divisible — it takes a prefix of any budget)
            // would otherwise spend the reservation before active-work is considered.
            // At generous budgets everything renders regardless and this only puts
            // her own thread at the top of the grounding block, which is the wake
            // briefing's order too (#125 slice 1).
            let a_held = a.faculty.as_str() == crate::persona::active_work_source::SOURCE_ID;
            let b_held = b.faculty.as_str() == crate::persona::active_work_source::SOURCE_ID;
            b_held.cmp(&a_held).then(
                b.salience
                    .partial_cmp(&a.salience)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
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
        // The SALIENCE of each whole-drop, kept structured beside the formatted
        // string so the inversion check below reads a number instead of parsing one
        // back out of a display string (that parse-your-own-log shape is how a
        // sensor starts lying the first time the format changes).
        let mut dropped_salience: Vec<(String, f32)> = Vec::new();
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
                dropped_salience.push((c.faculty.as_str().to_string(), c.salience));
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
            context_window,
            framing_tokens,
            conversation_tokens,
            conv_trailing_share,
            ctx_floor,
            // What the NATIVE tool schemas cost. Inferred at ~4,609 from the other
            // terms before it was measured, which is one inference too many: the
            // doc on `describe_tool_tokens` still claimed "a few dozen tokens, not
            // the 4-5k the old full-registry dump cost" — written when the surface
            // WAS a two-tool discovery pair, and false since #206 deliberately
            // restored the full ~dozen native tools. Reading that stale sentence is
            // exactly why the right suspect got dismissed.
            tool_tokens = self.describe_tool_tokens(),
            // The pool the budget is subtracted FROM, read from the local that
            // actually holds it. First written as
            // `framing + conversation + budget` — a RECONSTRUCTION from the
            // outputs, which cannot disagree with them and therefore says nothing.
            // A derived field wearing a measurement's name is the same defect as
            // the double-counted `trailing_tokens` it shipped beside.
            // `after_framing − conversation_tokens − header` should equal
            // `budget_tokens`; where it does not, the missing term is visible
            // instead of inferred.
            after_framing,
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
        // SALIENCE INVERSION — the sufficiency sensor this seam never had.
        //
        // `min_window_for_agentic_surface` answers "can this window hold framing +
        // tools + a reply at all", and its own doc is explicit that clearing it proves
        // nothing: "Necessary, NOT sufficient — clearing it does not mean she can hear
        // you… never 'you are fine above it'." Measured 2026-08-20 that warning came
        // true exactly as written. Served window 22,528 against a bound of ~16.9k, so
        // the necessary-condition sensor was correctly SILENT — while across 25
        // consecutive turns `workspace-map` (salience 0.90, the top tier) was dropped
        // WHOLE on 20 of them and grounding ran 62-1,083 tokens. Atlas could not see
        // the repo she was staged to edit, produced 0 acts for 3 days, and every
        // instrument said healthy.
        //
        // What makes it detectable WITHOUT inventing a threshold: contributions are
        // taken highest-salience-FIRST, so ranking alone can never drop a 0.90 while
        // keeping a 0.30. When that happens anyway it is the SIZE interaction — a big
        // essential block did not fit, then a small unimportant one did. The budget
        // bought the cheap thing and lost the important one. That inversion is a
        // structural fact with no magic constant behind it, which is why it is the
        // signal rather than "salience >= 0.9", a number that would be a snapshot of
        // one day's tuning ([[a-perception-fact-is-honesty-not-an-actuator]]).
        //
        // Divisible blocks are deliberately NOT counted: a prefix-truncated list
        // announces itself in the prompt ("…N more not shown"), so it is degraded but
        // never silent. Only WHOLE drops vanish without a trace — the failure this
        // exists to end. SENSOR, never a clamp: refusing to render, or amputating the
        // tool surface to make room, is the deleted #206 cliff. It reports; the fix
        // for the budget itself is #460 and is not a decision this function may make.
        // TWO holes were in the first cut of this, and the FIRST live turn after deploy
        // found both (2026-08-20, Atlas/Kira/Benchy on build 2281e8b81). Recorded here
        // because the shape is the one this codebase keeps paying for.
        //
        // HOLE 1 — strict `>` against a population with NO salience spread. The live
        // rows: Atlas kept `roster(0.90), room-kanban(0.90)` and dropped
        // `workspace-map(0.90)`; Benchy kept `room-board(0.90)` and dropped
        // `workspace-map(0.90), room-kanban(0.90)`. EVERYTHING essential bids exactly
        // 0.90, so `0.90 > 0.90` is false and the sensor stayed silent through the
        // precise condition it was written for. The premise "ranking cannot drop a 0.90
        // while keeping a 0.30" is TRUE and VACUOUS: production has no 0.30 at this
        // tier. What actually decides who survives is arrival order and byte size — a
        // coin flip among equals — and losing the repo map to that flip is the whole
        // harm. `>=` is therefore the honest predicate, and a genuine ranking success
        // (0.30 dropped, 0.90 kept) still cannot fire it.
        //
        // HOLE 2, worse — TOTAL starvation was invisible. `min_by` over an EMPTY
        // `selected` yields `None`, the `if let` fell through, and nothing fired. The
        // one state where the citizen got NO grounding at all was the one state the
        // sensor could not report ([[an-absence-is-an-unfinished-measurement]], and
        // mine, in the instrument built to end exactly this).
        let worst_dropped = dropped_salience
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)) // safe: salience is a finite f32; only NaN returns None, and a NaN salience is an upstream bug the SENSOR must survive to report, never panic on
            .cloned();
        if let Some((worst_name, worst_sal)) = worst_dropped {
            let cheapest_kept = selected
                .iter()
                .map(|(c, _)| c.salience)
                .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); // safe: same NaN-only case as the max_by above — an unorderable salience must not take down the instrument that exists to report it
            // `None` = nothing survived at all, which is maximal harm, never "no finding".
            let fires = cheapest_kept.is_none_or(|kept| worst_sal >= kept);
            if fires {
                let verdict = match cheapest_kept {
                    None => "TOTAL: nothing survived the budget".to_string(),
                    Some(k) if (worst_sal - k).abs() < f32::EPSILON => {
                        format!("TIE at sal={worst_sal:.2} — decided by size, not by rank")
                    }
                    Some(k) => format!("dropped {worst_sal:.2} while keeping {k:.2}"),
                };
                // Bound as a local: the message interpolates it, and a probe FIELD
                // assignment is not a binding the format string can see.
                let tool_tokens = self.describe_tool_tokens();
                crate::probe!(
                    class = "delib.context.salience_inversion",
                    persona = %self.persona_name,
                    dropped_faculty = %worst_name,
                    dropped_salience = worst_sal,
                    // -1.0 is an OUT-OF-DOMAIN sentinel, not a fabricated quantity: salience is
                    // [0,1], so -1 cannot be misread as a real one. `None` here is a KNOWN state
                    // ("nothing survived the budget"), never an unknown one — which is the case
                    // the ratchet's doc warns about — and `verdict` states it in words besides.
                    kept_min_salience = cheapest_kept.unwrap_or(-1.0), // safe: see the two lines above

                    kept_count = selected.len(),
                    verdict = %verdict,
                    budget_tokens,
                    context_window,
                    tool_tokens,
                    framing_tokens,
                    "GROUNDING LOST: {worst_name} dropped WHOLE ({verdict}) — the grounding \
                     budget was {budget_tokens} tok against a {context_window} window with \
                     {tool_tokens} in tool schemas and {framing_tokens} in framing. The citizen \
                     is reasoning without what attention ranked top (#460)."
                );
            }
        }
        // SERIALIZATION (by volatility, then CANONICALLY within the stable tier):
        // stable standing-framing FIRST (roster, doctrine, map) so it lands in the
        // cacheable KV-prefix region adjacent to the static system prompt it resembles;
        // volatile grounding (recall, working memory) LAST, nearest the generation point.
        //
        // WHY THE SECOND KEY EXISTS — this tier-split alone did NOT deliver reuse.
        // The original comment here said a stable sort "preserves the salience order
        // WITHIN each tier, so attention ranking is untouched", and treated that as
        // harmless. It is the remaining half of the #266 cache defect. Salience is
        // recomputed EVERY turn, so preserving it as EMIT ORDER re-shuffles the very
        // tier whose whole purpose is to be byte-identical across turns.
        //
        // Measured live 2026-08-21 in Atlas's `pallets__flask-4045` run, from her own
        // captures: consecutive system prompts shared only ~82% of their prefix, and the
        // divergence points were stable-tier blocks trading places —
        // `[room-kanban]` → `[active-work]` → `[workspace-map]` at chars 8216 / 7879 /
        // 8133. Because the system prompt is FIRST, a swap at token ~2,000 invalidates
        // the KV for EVERYTHING after it, including a 34,000-token conversation tail.
        // `--cache-reuse 256` was passed the whole time and every turn reported
        // `cachedTokens: 0` — the cache was correct; we were destroying the prefix.
        // Cost at the measured 111 tok/s prefill: ~306s of re-prefill PER ACT, growing
        // as her conversation grows. One observed turn was 20 minutes of apparent
        // silence that was a single 36k re-prefill.
        //
        // So within the stable tier, order is CANONICAL (by faculty name), not by
        // salience. Attention ranking is genuinely untouched: salience still decides
        // WHICH contributions are selected — that happened above, against the budget.
        // It simply stops deciding WHERE the survivors sit, which it never needed to.
        // Same set in, same bytes out, every turn.
        //
        // The volatile tier deliberately KEEPS salience order: it is re-prefilled by
        // construction (it changes every turn), so ordering it canonically would buy no
        // reuse and would cost the "most salient nearest the write point" placement
        // that #205 put there on purpose.
        //
        // `false` (stable) sorts before `true` (volatile). See [`Contribution::stable`].
        selected.sort_by(|(a, _), (b, _)| {
            u8::from(!a.stable).cmp(&u8::from(!b.stable)).then_with(|| {
                match (a.stable, b.stable) {
                    // Stable tier: canonical, so the cacheable prefix is deterministic.
                    (true, true) => a.faculty.as_str().cmp(b.faculty.as_str()),
                    // Volatile tier: leave salience order alone (stable sort keeps it).
                    _ => std::cmp::Ordering::Equal,
                }
            })
        });
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
        self.compose_system_holding(context, expanded, directed, self_initiated, now_ms, false)
    }

    /// [`Self::compose_system`] with the held-work contract flag. The plain form
    /// defaults it false so the many framing-shape tests (which have no workspace)
    /// keep their signature; the live prompt path derives it from `ws` and calls
    /// this directly.
    #[allow(clippy::too_many_arguments)]
    fn compose_system_holding(
        &self,
        context: &str,
        expanded: &BTreeSet<String>,
        directed: bool,
        self_initiated: bool,
        now_ms: Option<u64>,
        holds_live_work: bool,
    ) -> String {
        // Whole-string form (stable ++ trailing), byte-identical to the pre-split output.
        // Kept for the many framing-shape tests that assert against the composed whole; the
        // LIVE prompt path calls `compose_system_split` and places the two parts separately.
        let c = self.compose_system_split(
            context,
            expanded,
            directed,
            self_initiated,
            now_ms,
            holds_live_work,
        );
        let mut s = c.stable;
        s.push_str(&c.trailing);
        s
    }

    /// The system prompt split at the KV-cache boundary (#266): a persona-invariant
    /// [`stable`](deliberation_prompt::ComposedSystemPrompt::stable) prefix
    /// (identity + `[Taking your turn]` + tools) that belongs in the cacheable system
    /// message, and the per-turn
    /// [`trailing`](deliberation_prompt::ComposedSystemPrompt::trailing) framing +
    /// assembled context + clock that the live path rides on the newest turn so it never
    /// invalidates the cached prefix. The whole-string [`compose_system_holding`] is
    /// `stable ++ trailing` of exactly this.
    #[allow(clippy::too_many_arguments)]
    fn compose_system_split(
        &self,
        context: &str,
        expanded: &BTreeSet<String>,
        directed: bool,
        self_initiated: bool,
        now_ms: Option<u64>,
        holds_live_work: bool,
    ) -> deliberation_prompt::ComposedSystemPrompt {
        deliberation_prompt::compose_split(&deliberation_prompt::SystemPromptParts {
            system_prompt: &self.system_prompt,
            persona_name: &self.persona_name,
            tools: &self.tools,
            expanded,
            context,
            directed,
            now_ms,
            self_initiated,
            holds_live_work,
        })
    }

    /// Does this workspace carry an [active-work] grounding contribution naming a
    /// card she currently holds IN PROGRESS? Structural claim-state (the source read
    /// it from airc this turn), decoded from our own wire format at the one predicate
    /// colocated with its renderer. Drives the working-presence contract on
    /// undirected turns; derived ONCE per prompt so the framing-token estimate and
    /// the final render agree byte-for-byte (the budget-math invariant).
    fn holds_live_work(ws: &Workspace) -> bool {
        ws.broadcast
            .iter()
            .filter(|c| c.decision.is_none() && !c.trailing)
            .filter(|c| c.faculty.as_str() == crate::persona::active_work_source::SOURCE_ID)
            .any(|c| crate::persona::active_work_source::renders_held_in_progress(&c.content))
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
        // is then allowed to use. One source: [`Self::completion_reserve_within`].
        let completion_reserve = self.completion_reserve_within(context_window) as usize;

        // The NATIVE tool schemas ride the served window too: the gateway injects
        // each function spec (name + description + schema) via the chat template,
        // outside `system`/`user`. Without counting them the budget silently
        // overshoots `n_ctx` and llama-server 400s ("exceeds context size").
        //
        // This is NOT "a few dozen tokens" — that sentence described the two-tool
        // discovery pair and has been false since #206 restored the full native
        // surface (~a dozen tools, offered whole, deliberately: a native-tool model
        // can only call what it was offered, and the amputated surface stranded it
        // in a `commands/help` loop). Measured 2026-08-06 it is **~4,609 tokens** —
        // 28% of a 16,384 window, before framing or a single message. It is now on
        // the `delib.context.render` probe as `tool_tokens` rather than described,
        // because this comment being wrong is what cost three commits of chasing a
        // phantom accounting gap.
        //
        // Note the tool surface is paid TWICE in different forms: these schemas
        // here, AND the human-readable tool MENU inside `compose_system` (counted in
        // `framing_tokens`). Whether that duplication is intended is an open
        // question, not something this comment should assert either way.
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
        let holds_live_work = Self::holds_live_work(ws);
        let framing_tokens = est_tokens(&self.compose_system_holding(
            "",
            &expanded,
            ws.directed_at_self,
            ws.self_initiated,
            ws.now_ms,
            holds_live_work,
        ));

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
        // Which contribution the floor reserves is a PRIORITY ordering, not a size
        // contest. "Smallest offered" guaranteed *a* fact reached her — but on a
        // squeezed window the smallest fact is a ~50-token status brick, and what
        // dropped instead was ACTIVE-WORK: the card content of the claim she is
        // actually holding. Live specimen 2026-08-07: Benchy held a staged SWE card,
        // her render dropped recall/roster/active-work/workspace-map/kanban
        // (ctx_floor=49 kept one perception brick), her whole knowledge of the work
        // was a bare claim receipt, and she reasonably yielded — 98s of prefill for
        // 7 tokens of yield_turn, own_repetition firing. A held claim is HER THREAD
        // (the wake briefing already leads with it — #125 slice 1); the mid-session
        // floor must agree. So: reserve the active-work contribution when one is
        // offered this turn, else the smallest, bounded at half the post-framing
        // pool exactly as before (the fairness cap between grounding and
        // conversation is unchanged — only which claimant holds the reservation).
        let contribution_cost =
            |c: &Contribution| est_tokens(c.faculty.as_str()) + est_tokens(&c.content) + 2;
        let offered = || {
            ws.broadcast
                .iter()
                .filter(|c| c.decision.is_none() && !c.trailing)
        };
        let ctx_floor = offered()
            .find(|c| c.faculty.as_str() == crate::persona::active_work_source::SOURCE_ID)
            .map(&contribution_cost)
            .or_else(|| offered().map(&contribution_cost).min())
            // The `ctx_budget` computation below charges the working-context wrapper
            // header against this reservation before any contribution renders, so a
            // floor sized to the contribution ALONE under-reserves by exactly the
            // header and delivers nothing. Reserve the delivered shape: header + item.
            .map(|cost| cost + est_tokens(deliberation_prompt::WORKING_CONTEXT_HEADER))
            .unwrap_or(0)
            .min(after_framing / 2);
        let msg_budget = after_framing.saturating_sub(ctx_floor);
        // LATENCY-BUDGETED FILL (act-latency law + compression ladder): a
        // bigger served window is only a gift if its content earns the prefill
        // time. When the honest kv-rate fix grew slots to ~134k, the demand-
        // derived budgets happily filled them — prompts ballooned 19-24k →
        // 24-43k in an afternoon and five concurrent 35k prefills serialized
        // acts to 339-888s (measured 2026-09-01, the worst of the day, WITH
        // the best hit rates). So the CONVERSATION fill is capped by a time
        // target: target seconds × a conservative prefill rate. The window's
        // remaining headroom stays available to everything that earns depth —
        // grounding, the pinned act result, working memory — and to reply
        // reserve; it is RESERVE, not default fill. The rent ledger (segment
        // probe) is what will let this cap adapt per-segment; until then the
        // rate constant is deliberately below every measured ingest
        // (636-676 t/s live) so the cap over-admits rather than starves.
        let latency_fill_cap = PREFILL_TARGET_SECONDS * CONSERVATIVE_PREFILL_TOKENS_PER_S;
        let msg_budget = if msg_budget > latency_fill_cap {
            crate::probe!(
                class = "delib.fill.latency_capped",
                persona = %self.persona_name,
                window_budget = msg_budget,
                cap = latency_fill_cap,
                "conversation fill capped by the act-latency target — window headroom \
                 is reserve for content that earns its prefill, never default fill",
            );
            latency_fill_cap
        } else {
            msg_budget
        };
        // The volatile framing (clock + own-time/presence) rides the FACTS phase
        // of the conversation — flicker-class content, before the ask — never the
        // system message it invalidated on every act (see the split note below).
        let trailing_framing = deliberation_prompt::compose_trailing(
            ws.now_ms,
            ws.self_initiated,
            ws.directed_at_self,
            holds_live_work,
        );
        let all_messages = self.messages_unfitted(
            ws,
            (!trailing_framing.is_empty()).then_some(trailing_framing.as_str()),
        );

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
                ws.now_ms.unwrap_or(0), // JUSTIFIED unwrap_or: unstamped cycle still measures honestly (registry keeps peaks, not a time series)
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
        let context = self.render_assembled_context_within(
            ws,
            ctx_budget,
            (
                context_window,
                framing_tokens,
                used_msg_tokens,
                ctx_floor,
                after_framing,
            ),
        );

        // #266 KV-cache fix, THE SPLIT ACTUALLY APPLIED. The split composer
        // existed and its own docs called it "the live path" — but this view
        // was still built with `compose_system_holding` (stable ++ trailing in
        // ONE system message), so the [now] minute clock and the presence/
        // own-time flip kept churning the system prefix at char ~8.4k. Wire-
        // capture diff 2026-09-01: consecutive turns diverged exactly at
        // `[now …]` / `[Conversational Presence]`→`[Your own time]`, with the
        // server's reuse proven perfect the same hour (same prompt ×2 on one
        // slot → cache_n 397/401). A prefix that mutates every act caches
        // nothing; hit_rate was 0.0 fleet-wide with every persona on her OWN
        // slot ([[a-mutating-system-prompt-destroys-kv-reuse-for-everything-after-it]]).
        //
        // Now: `stable` (identity + tools + grounding context) IS the system
        // message; `trailing` (clock + presence framing) rides as the NEWEST
        // user turn — same text, nearest generation, zero prefix invalidation.
        // Same delivery the `.trailing()` contributions already use and test.
        // The trailing half was already rendered into the FACTS phase of the
        // conversation by `messages_unfitted` (before the ask, parrot order
        // preserved); only the byte-stable half may touch the system message.
        let system = self
            .compose_system_split(
                &context,
                &expanded,
                ws.directed_at_self,
                ws.self_initiated,
                ws.now_ms,
                holds_live_work,
            )
            .stable;
        let segments = segment_map(&system, &messages);
        DeliberationPromptView {
            system,
            messages,
            segments,
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
        self.fit_messages(self.messages_unfitted(ws, None), budget_tokens)
    }

    /// The conversation as it stands BEFORE any budget is applied — every turn, every
    /// perception fact, every trailing proprioception block, in final render order.
    ///
    /// Split out from the fitting step so a turn's TRUE conversational size is
    /// knowable. Fitting is lossy by design (newest-first, oldest dropped), and once
    /// it has run the question "how much did this turn actually want?" is
    /// unanswerable — which is precisely why the served window had to be sized by a
    /// constant instead of by demand. [`super::working_set`] measures this.
    fn messages_unfitted(
        &self,
        ws: &Workspace,
        trailing_framing: Option<&str>,
    ) -> Vec<ChatMessage> {
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
            ws.room_id,
        );
        let room_speech = super::deliberation_budget::recent_room_speech(ws.room_id);
        let fact_cx = super::perception_facts::FactContext {
            turns: &ws.turns,
            own_speech: &spoken,
            room_speech: &room_speech,
            working_memory: self.working_memory.as_ref(),
            room_id: Some(ws.room_id),
            persona_id: Some(self.persona_id),
        };
        let facts = super::perception_facts::render_facts(
            &fact_cx,
            &super::perception_facts::FactPolicy::default(),
        );
        // MESSAGE ORDER IS MONOTONE IN STABILITY (2026-08-23, the byte-diff
        // verdict). Consecutive-act prompt captures showed the flickering
        // content — perception facts whose presence/wording changes per act,
        // trailing grounding — rendering AHEAD of the append-only history
        // block, so the 20k+ stable mass never extended the KV prefix and
        // reuse pinned at the system head (~36%). The assembly below is
        // explicit phases, replacing insert-before-ask arithmetic:
        //
        //   old turns → history (append-only) → grounding (flickers)
        //   → facts (flicker) → ASK → pinned newest result
        //
        // Every prior lesson is preserved by construction: facts stay BEFORE
        // the ask and the ask stays after them (the 2026-07-20 parrot-loop
        // bisect); the pinned result stays nearest generation (#392); trailing
        // grounding stays out of the system prefix (#205/#2415). The ask is
        // the LAST user turn of the conversation, peeled here and re-attached
        // after the churn so stable content is a strict prefix of the stream.
        let ask: Option<ChatMessage> = match messages.last() {
            Some(m) if m.role == "user" => messages.pop(),
            _ => None, // pure self-tick: assistant history only, no ask to peel
        };

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
        // PINNED just-executed result (#392, run-18057-f1). The full result of the act
        // she just ran is appended DIRECTLY from the shared working-memory buffer as the
        // newest trailing turn — deliberately NOT routed as a faculty bid. As part of the
        // working-memory contribution at `WORKING_MEMORY_SALIENCE` it competed in
        // `arbiter.focus()` top-k and was evicted whole under capacity pressure, so the
        // persona generated blind to her own grep/read output (the 0-byte SWE-bench patch).
        // Reading it here puts no attention pass between the hands and the mind. Text, not
        // structured tool_use/tool_result Parts: `content_text()` is blind to non-Text
        // parts, so Parts would undercount in `fit_messages`/`messages_cost` and risk a
        // window-edge overflow — and the live GGUF chat template renders text reliably.
        // Trailing (#205): appended last, KV prefix stays stable. Settlement-gated inside
        // the accessor so it stops re-prefilling once the turn settles (#139/#165).
        // GROUNDING BEFORE THE RING (2026-08-24, wire-diffed on round 706215f4):
        // with the ring append-only mid-settle (the 8/24 eviction fix), each new
        // result APPENDED after grounding DISPLACED the byte-identical
        // workspace-map behind it — four consecutive wire diffs showed the
        // divergence at exactly that seam, re-prefilling the whole 75% tail on a
        // model that cannot KV-shift. Reversed from the 8/23 cut deliberately:
        // that A/B lost because the ring ALSO churned then (per-act eviction),
        // so either order re-prefilled; with the ring extend-only, stable
        // grounding ahead of it extends the prefix, and a grounding CHANGE costs
        // one re-prefill instead of every act paying displacement.
        // STABILITY-ORDERED TAIL, second wire-diff (2026-08-24 evening): the first
        // reorder moved ALL trailing contributions ahead of the ring — including the
        // working-memory TRAIL, which REWRITES itself every act (measured: an 11.5k
        // interior-state block at msg[1] mutating per tick, prefix dead at 2-4%).
        // Split by churn class: non-WM grounding (semi-stable) → append-only ring →
        // the trail (churniest standing block) → facts/ask/pin. One rule, monotone
        // in stability, same law as the system-side phases.
        let render_trailing = |messages: &mut Vec<ChatMessage>, wm_trail: bool| {
            for c in ws
                .broadcast
                .iter()
                .filter(|c| c.decision.is_none() && c.trailing)
                .filter(|c| (c.faculty.as_str() == crate::cognition::working_memory::WM_FACULTY_ID) == wm_trail)
            {
                if !c.content.trim().is_empty() {
                    // Same `[faculty]` banner the system block gives its sections —
                    // grounding that moved here for KV reuse (volatile-content
                    // sources, 2026-08-23) must stay as legible as it was in the
                    // system prefix. One sized allocation, no intermediate format!.
                    let mut body =
                        String::with_capacity(c.faculty.as_str().len() + c.content.len() + 4);
                    body.push('[');
                    body.push_str(c.faculty.as_str());
                    body.push_str("]\n");
                    body.push_str(&c.content);
                    messages.push(ChatMessage::text("user", body));
                }
            }
        };
        render_trailing(&mut messages, false);
        if let Some(wm) = &self.working_memory {
            // The append-only results ring — its appends land at the growth
            // frontier of the stable region, displacing nothing.
            for block in wm.recent_results_messages() {
                messages.push(ChatMessage::text("user", block));
            }
        }
        render_trailing(&mut messages, true);

        // Facts flicker per act — they render after every append-only block,
        // immediately before the ask (parrot-fix order preserved).
        for fact in facts {
            messages.push(ChatMessage::text("user", fact));
        }
        // The volatile framing (clock + own-time/presence flip) — same flicker
        // class as the facts, so it rides HERE: before the ask (the parrot-fix
        // order), after everything stable. It lived at the tail of the system
        // message until 2026-09-01, where its per-act mutation (the [now]
        // minute tick, DIRECTED↔SILENCE) invalidated the KV prefix behind it
        // on nearly every act — hit_rate 0.0 with per-resident slots and a
        // server whose reuse measured perfect the same hour.
        if let Some(framing) = trailing_framing {
            messages.push(ChatMessage::text("user", framing.to_string()));
        }
        if let Some(ask) = ask {
            messages.push(ask);
        }
        if let Some(wm) = &self.working_memory {
            // Pinned full-latest LAST — the act she is inside right now, nearest
            // generation (#392).
            if let Some(block) = wm.pinned_active_result_block() {
                messages.push(ChatMessage::text("user", block));
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

    /// The window-start advance QUANTUM divisor: when the conversation outgrows
    /// its budget, the front is dropped in chunks of `budget/8` (floored at 512
    /// tokens) rather than message-by-message. See [`Self::fit_messages`].
    // context-budget-exempt: a stability/latency trade ratio, not a token budget — the actual chunk derives from the live budget at the call
    const FRONT_DROP_QUANTUM_DIVISOR: usize = 8;

    /// Fit an already-built conversation to `budget_tokens` — with a
    /// QUANTIZED, whole-message front drop, because the fit's cut point is the
    /// byte-stability of the entire conversation prefix.
    ///
    /// The old shape walked NEWEST-first, spending the budget backward: correct
    /// per-act, and a KV catastrophe across acts — every act appends new tokens
    /// at the tail, so the exact-fit cut point advanced the window START by a
    /// few messages per act, and the straddling message was tail-trimmed to a
    /// different byte offset each time. Measured 2026-09-01 (wire-capture
    /// diffs): consecutive prompts diverging at ~1% depth on exactly this seam,
    /// re-prefilling the other 99%.
    ///
    /// Now the drop is quantized: the minimum front mass that must go is
    /// rounded UP to a chunk (`budget/8`, ≥512 tokens), whole messages only, so
    /// the window start stays BYTE-IDENTICAL for ~a chunk's worth of new
    /// conversation and then advances once, in one jump. The cost is a window
    /// that runs up to one chunk under-full — a deliberate stability/capacity
    /// trade, same law as the scratch slot: reuse of a 20-80k prefix is worth
    /// vastly more than the last few k of oldest history
    /// ([[collapse-dont-clip-condense-the-past-never-erode-it]] — and when the
    /// jump does land, it drops whole turns, never a mid-message shear).
    fn fit_messages(&self, messages: Vec<ChatMessage>, budget_tokens: usize) -> Vec<ChatMessage> {
        // Per-message template overhead: `Self::PER_MESSAGE_TEMPLATE_TOKENS`, shared
        // with `messages_cost` so measurement and fitting charge identically.
        let per_message_template_tokens = Self::PER_MESSAGE_TEMPLATE_TOKENS;
        let costs: Vec<usize> = messages
            .iter()
            .map(|m| est_tokens(&m.content_text()) + per_message_template_tokens)
            .collect();
        let total: usize = costs.iter().sum();
        let mut fitted: Vec<ChatMessage> = Vec::new();
        if total <= budget_tokens {
            fitted = messages.clone();
        } else if messages.len() > 1 {
            let min_drop = total - budget_tokens;
            let quantum = (budget_tokens / Self::FRONT_DROP_QUANTUM_DIVISOR).max(512);
            let drop_q = min_drop.div_ceil(quantum).saturating_mul(quantum);
            let mut dropped = 0usize;
            let mut start = 0usize;
            // Whole messages only, and never the newest — it is what the turn
            // is about (the pre-existing guarantee, preserved).
            while start + 1 < messages.len() && dropped < drop_q {
                dropped += costs[start];
                start += 1;
            }
            // CLUSTERS ARE THE EVICTION GRANULARITY (Joel 2026-09-01:
            // "information flows in groups … a room, code we're looking at,
            // any context that flows semantically together"). A window that
            // opens mid-cluster — an assistant answer without its question, a
            // tool result without its act — costs tokens while carrying broken
            // meaning, worse than dropping the fragment too. Advance the start
            // past continuation-shaped messages to a clean opener. Structural
            // v1 (no model call): an assistant turn or a result/receipt block
            // continues a cluster; a plain user turn opens one. Bounded so a
            // pathological run can't eat the window; the causal-thread work
            // (ladder rung 4) replaces this with true causal subtrees.
            let continues_cluster = |m: &ChatMessage| {
                let body = m.content_text();
                m.role == "assistant"
                    || body.starts_with("Full result of")
                    || body.starts_with('⚙')
            };
            let mut opener_advance = 0usize;
            while start + 1 < messages.len()
                && opener_advance < 6
                && continues_cluster(&messages[start])
            {
                dropped += costs[start];
                start += 1;
                opener_advance += 1;
            }
            if total - dropped <= budget_tokens {
                fitted = messages[start..].to_vec();
            }
            // else: the surviving suffix alone still exceeds the budget (a
            // giant newest message) — fall through to the tail-trim guarantee.
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
        fitted
    }

    /// Conservative token estimate of the ONE natively-offered tool spec
    /// (`commands/help`) the gateway injects via the chat template — its serialized
    /// function schema plus a small template framing margin. 0 when the persona has
    /// no tools. Counted with the same conservative guard ratio as the rest of the
    /// prompt (round UP — under-counting risks the 500). Cheap and pure.
    ///
    /// # This is THOUSANDS of tokens, and the sentence that used to live here said otherwise
    ///
    /// It read "a single tiny schema, so this is a handful of tokens, not the old
    /// full-registry dump" — true when the surface WAS the two-tool discovery pair,
    /// false since #206 deliberately restored the full native set. The probe site in
    /// `render_assembled_context_within` has carried a comment calling this sentence
    /// out as stale since 2026-08-06 ("reading that stale sentence is exactly why the
    /// right suspect got dismissed") — 700 lines from the function it describes, so
    /// anyone reading the function itself still got the lie. Deleting it here is the
    /// half that never got done.
    ///
    /// MEASURED: **4,609 tokens on 2026-08-06. 7,013 on 2026-08-20** — 31% of a 22,528
    /// window, subtracted before framing or a single message, on every turn.
    ///
    /// # It grows on its own, and nothing watches it
    ///
    /// [`persona_tools::native_tool_specs`] is DERIVED — every command declaring
    /// `NATIVE = true` is offered automatically. That is the right architecture (no
    /// central array to edit) and it means the per-turn cost of the tool surface is an
    /// unbounded side effect of a flag set in another file: +52% in two weeks, noticed
    /// by nobody. Nothing connects "I marked my command native" to "every citizen
    /// permanently lost N tokens of grounding" (#460). Do NOT respond by trimming the
    /// set — the amputated surface is the #206 cliff and measured 14/14 SWE acts as
    /// `commands/help` with 0 edits.
    fn describe_tool_tokens(&self) -> usize {
        self.tool_surface_tokens
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
    /// THE RENT LEDGER'S SPINE (compression-ladder rung 2): the prompt's segment
    /// runs in send order, each `(label, cumulative estimated-token END)`. After
    /// generation, the server's reused-prefix length (`cached_tokens`) lands
    /// somewhere in this map, and the run containing that frontier NAMES the
    /// segment that broke reuse — turning every cache miss from a number into an
    /// attribution, and accumulating each segment's true daily cost (its rent).
    /// Estimated tokens (chars/4 class), so attribution is segment-scale
    /// (thousands of tokens), never byte-exact — which is all promotion needs.
    pub segments: Vec<(&'static str, u32)>,
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
        // RESTORE-AHEAD (compression-ladder rung 1): tell the serving adapter
        // this activity generates NEXT, so her KV page pages in DURING the
        // prompt assembly below instead of racing the turn at pin time. Fire-
        // and-forget; the pin-time path stays the backstop; a nil room (test
        // workspaces, roomless background) no-ops inside.
        loaded.adapter.warm_ahead(self.persona_id, ws.room_id);
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
            // DEADBAND (2026-08-24, KV layer 4 — "it's a control system, also
            // predictable" / vector-allocator semantics): the live value
            // FLUTTERS (measured 38,144 ↔ 36,864 act-over-act), and every
            // window-derived clip budget re-cuts on rebind, mutating the whole
            // prompt — cached% pinned at the system head. Adopt only a REAL
            // change (> 1/8 of the current binding — a lane relaunch, a plan
            // reshape); hold the binding through flutter. The safety envelope
            // is untouched for OVERFLOW: a served window that SHRANK below the
            // binding by any amount still adopts DOWN via the same test only
            // when material — and a sub-deadband shrink is llama's own
            // 256-granularity padding, which the prompt fitter's completion
            // reserve already absorbs many times over.
            let deadband = loaded.context_window / 8;
            let material = effective.abs_diff(loaded.context_window) > deadband;
            if !material {
                if effective != loaded.context_window {
                    crate::probe!(
                        class = "delib.window.held",
                        binding = loaded.context_window,
                        served = effective,
                        "live window flutter within deadband — binding HELD so the                          prompt's derived budgets stay byte-stable (KV layer 4)"
                    );
                }
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
                // PERSIST the reconciliation (2026-08-24, the 17% mind): this
                // adoption was turn-LOCAL, so `model_loadout()` — the source every
                // OTHER window consumer reads (working-memory ring/trail/archive
                // budgets in apply.rs) — kept serving the stale spawn pin forever.
                // Measured on round 706215f4: binding 28,672 vs served 166,400 —
                // her result tails clipped to 896 chars on a 166k lane, and the
                // re-read loops that shredded her memory were visible in her own
                // trail ("I've been going in circles").
                //
                // Via `rcu` over the CURRENT value, never a store of our snapshot:
                // a concurrent rebind (model swap, genome page-in — the other two
                // writers on this handle) between our load and a whole-struct store
                // would be silently clobbered. Only the window is ours to write;
                // adapter/model ride whatever is current at commit time. The
                // deadband above already hysteresis-gates what reaches here.
                self.binding.rcu(|cur| ModelBinding {
                    adapter: std::sync::Arc::clone(&cur.adapter),
                    model: cur.model.clone(),
                    context_window: effective,
                });
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
        if view
            .messages
            .iter()
            .all(|m| m.content_text().trim().is_empty())
            && !ws.turns.is_empty()
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
            self.build_request_within(&binding, messages.clone(), tools, view.system.clone(), {
                // Turn-boundary hygiene: peer-name stops (#150, don't speak AS
                // teammates) + reserved-marker stops (#158, don't fabricate
                // [action]/[recall] receipts). Combined into one stop list.
                let mut stops = super::deliberation_budget::peer_stop_sequences(&ws.turns);
                stops.extend(super::deliberation_budget::reserved_marker_stop_sequences());
                (!stops.is_empty()).then_some(stops)
            }, Some(ws.room_id));
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
        // WIRE CAPTURE (2026-08-24, the churn hunt's missing instrument): when
        // `SERVING_WIRE_CAPTURE_DIR` is configured (config.env — same channel as
        // SERVING_KV_CACHE_TYPE), append this request's EXACT message list to
        // `<dir>/<persona>.wire.jsonl` before it ships. Byte-diffing consecutive
        // rows names the precise prompt position where the KV prefix dies —
        // today that diagnosis ran on the BID capture, which is not the wire,
        // and misled twice. Off (unset) = zero cost, zero IO — the Noop default
        // every capture sink owes the hot path.
        if let Some(dir) = crate::config_env::read("SERVING_WIRE_CAPTURE_DIR") {
            let row = serde_json::json!({
                "ts_ms": crate::persona::trace::now_ms(),
                "persona": self.persona_name,
                "messages": request
                    .messages
                    .iter()
                    .map(|m| serde_json::json!({"role": m.role, "text": m.content_text()}))
                    .collect::<Vec<_>>(),
            });
            let path = std::path::Path::new(&dir).join(format!("{}.wire.jsonl", self.persona_name));
            let _ = std::fs::create_dir_all(&dir);
            use std::io::Write as _;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(f, "{row}");
            }
        }
        // MEASURED-HOLD DEFER, ABOVE the admission gates (2026-08-29). The adapter's
        // own defer sits BELOW acquire_serving_lane + the prefill slot, so a
        // Background generation (dream-belief-review, glass-boxed live) would take
        // the lane permits, THEN park on the hold — starving the measured work's
        // own next generation of the very permits it needs to finish and release
        // the hold. Priority inversion; the solve's tick 2 waited forever on a
        // permit a deferred dream held. Defer FIRST, holding nothing; the adapter
        // seam stays as the backstop for entry paths that skip these gates.
        crate::probe!(
            class = "delib.defer.entered",
            persona = %self.persona_name,
            directed = ws.directed_at_self,
            "at the pre-gate hold defer"
        );
        {
            let mut class = crate::inference::slots::class_for(request.purpose.as_deref());
            // DIRECTEDNESS REFINES THE CLASS (2026-08-29, the convoy). `class_for`
            // maps purpose "cognition/deliberation" to Turn — but an UNDIRECTED
            // self-tick is ambient work wearing a turn's purpose string. During a
            // measured hold those ambient deliberations sailed past every defer,
            // took the faculty lane permits, and convoyed single-file into the
            // adapter's one-permit FIFO at slow-model speed — the solve starved
            // behind idle chatter (avail=0 at every tick-2 lane_wait, glass-boxed).
            // An undirected deliberation yields to a hold like any Background gen;
            // the holder's own ticks are directed and never wait. No hold active →
            // zero change (should_defer is a no-op on a released cell).
            if matches!(class, crate::inference::slots::SlotClass::Turn) && !ws.directed_at_self {
                class = crate::inference::slots::SlotClass::Background;
            }
            crate::inference::measured_hold::defer_while_held(
                class,
                Some(self.persona_id),
                request.purpose.as_deref(),
            )
            .await;
        }
        crate::probe!(
            class = "delib.defer.passed",
            persona = %self.persona_name,
            "past the pre-gate defer — admission gates next"
        );
        if ws.directed_at_self {
            // #2561: a directed engagement marks the organism ACTIVE (with linger)
            // — the one seam that knows directedness feeds the activity gate.
            crate::cognition::activity_gate::note_directed();
        }
        let gen_result = {
            crate::probe!(
                class = "delib.gate.lane_wait",
                persona = %self.persona_name,
                directed = ws.directed_at_self,
                lanes_available = crate::cognition::resource_admission::serving_lane_permits_available() as u64,
                "at the serving-lane admission gate"
            );
            let _lane =
                crate::cognition::resource_admission::acquire_serving_lane(ws.directed_at_self)
                    .await;
            crate::probe!(
                class = "delib.gate.lane_acquired",
                persona = %self.persona_name,
                "lane admission granted — prefill slot next"
            );
            // #56 prefill throttle: under live external GPU pressure (a game, the browser)
            // fewer than the served lane count may PREFILL concurrently — the instant valve
            // for the 2026-07-16 compute-buffer OOM. Same fit rule the capacity sim proves;
            // no pressure → target == lanes → this never waits. Released with the block.
            let _prefill = crate::cognition::prefill_throttle::acquire_prefill_slot().await;
            crate::probe!(
                class = "delib.gate.prefill_acquired",
                persona = %self.persona_name,
                "prefill slot granted — issuing the model call"
            );
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

        // MEASURE THE REPLY — the observation `completion_reserve_within` derives
        // the reserve from. The server's own count (reasoning included), recorded
        // for every completed generation; a Length stop records at double inside
        // the registry (the growth path). This is the seam that turns the reserve
        // from a `window/2` prior into a measurement.
        if let Some(reg) = &self.working_set {
            reg.record_emission(
                self.persona_id,
                resp.usage.output_tokens,
                matches!(resp.finish_reason, FinishReason::Length),
                ws.now_ms.unwrap_or(0), // JUSTIFIED unwrap_or: unstamped cycle still measures honestly (registry keeps peaks, not a time series)
            );
        }

        // SEGMENT ATTRIBUTION (the rent ledger, compression-ladder rung 2):
        // the server's reused-prefix length lands somewhere in the prompt's
        // segment map — the run containing that frontier NAMES what broke
        // reuse this turn, and the stream of these rows IS each segment's
        // rent. Only when the lane measured timings (same honesty rule as
        // delib.generate.cache: a fabricated attribution is a lying receipt).
        if let Some(t) = resp.timing.as_ref() {
            let cached = t.cached_tokens as u32;
            let frontier = view
                .segments
                .iter()
                .find(|(_, end)| *end > cached)
                .map(|(label, _)| *label)
                .unwrap_or("fully-warm"); // reuse extends past every mapped segment — nothing broke
            let mut map = String::with_capacity(64);
            for (label, end) in &view.segments {
                use std::fmt::Write as _;
                let _ = write!(map, "{label}:{end},");
            }
            crate::probe!(
                class = "delib.segment.attribution",
                persona = %self.persona_name,
                cached_tokens = cached as u64,
                broke_in = %frontier,
                cold = cached == 0,
                segments = %map,
                "which prompt segment the KV reuse frontier died in — every miss becomes \
                 an attribution, and the stream is the rent ledger the dream curriculum reads",
            );
        }

        // Verbatim glass box: the EXACT request thread + the raw response. Iteration
        // is always 0 now (single shot); the act→observe driver re-enters this
        // faculty on the NEXT tick with the result folded into perception, and that
        // tick captures itself. Best-effort; never affects the turn.
        if let Some(cap) = &self.prompt_capture {
            let offered: Vec<String> = self.native_specs.iter().map(|s| s.name.clone()).collect();
            cap.record(
                self.persona_id,
                ws.room_id,
                ws.cause.as_str(),
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

        // AN EMPTY COMPLETION IS NOT A CHOSEN SILENCE (the `Err` arm's missing twin).
        //
        // The `Err` arm above refuses to let a FAILED model collapse into a serene
        // `Pass` — [[fallbacks-are-illegal-fail-loud]]. But a lane can also answer
        // `Ok` with NOTHING, and that walked straight past the guard and settled as
        // an ordinary non-Act. Two live shapes, both measured 2026-08-16:
        //
        //  * the server generated tokens that never reached `content` — Qwen3.8 under
        //    `--jinja` opens `<think>`, and an unclosed block leaves `extract_reasoning`
        //    branch (3) with empty text and the whole tail as reasoning (#181). Direct
        //    probe against the live lane: 70-token prompt, `finish_reason: length`,
        //    `completion_tokens: 16`, `content: ""`.
        //  * the lane returned 0 tokens in AND out in 28ms (`finish_reason: stop`) —
        //    Solenne's capture on the turn her benchmark run died.
        //
        // Cost of laundering it: `agent/solve` reads "she chose not to act" → acts=0,
        // empty patch → the whole run voids as an INFRA VOID after three attempts; a
        // LIVE citizen reads it as a silent turn, which is indistinguishable from
        // withdrawal. Measured across every capture on disk: 47 of 862 responses
        // (5.5%) are empty-text, spread over ~19 citizens — and the all-empty column
        // is exactly the citizens whose "I've been repetitive, I'll remain silent"
        // turns are the standing round-killer (#390/#414). They were not withdrawing.
        // Nothing came back, and the substrate wrote it down as a choice.
        //
        // SCOPE: a native tool turn legitimately carries empty content, so ToolUse and
        // any present tool_calls are excluded — this fires only when the turn yields
        // no text, no tool call, and therefore nothing to act or speak with. The
        // reasoning tail rides on the fault so the receipt says WHICH shape it was:
        // thought-but-committed-nothing, or the lane returned void.
        // ONLY when there is nothing left to recover. This guard shipped (ce82f00ff)
        // gating on "empty text + no tool call" alone, which is the EXACT precondition
        // of the two recovery paths below — `persona.act.reasoning_lift` (a tool call
        // sitting in the reasoning tail gets lifted and executed) and
        // `persona.act.think_only` (#181's teacher sentinel, which hands her another
        // generation starting from her own conclusions). Faulting first made both
        // unreachable: measured live 2026-08-17, 40 `delib.empty_completion` faults
        // against 87 turn starts while three SWE runs sat at the same act count for
        // 1,357s. The recovery already existed; the guard was standing in front of it.
        //
        // So a REASONING-BEARING empty is not a fault — it is #181, and it has an
        // owner. Fault only for the genuinely unrecoverable shapes: the lane returned
        // void (no text, no reasoning, no call), or she has no hands for the sentinel
        // to teach through. Both are still surfaced rather than read as chosen silence,
        // which is what this guard is for ([[a-perception-FACT-is-honesty]]).
        let nothing_to_recover = resp
            .reasoning
            .as_deref()
            .is_none_or(|r| r.trim().is_empty())
            || self.tools.is_empty();
        if resp.text.trim().is_empty()
            && !matches!(resp.finish_reason, FinishReason::ToolUse)
            && resp.tool_calls.as_ref().is_none_or(|c| c.is_empty())
            && nothing_to_recover
        {
            let reasoning_tokens = resp.reasoning.as_ref().map_or(0, |r| r.len());
            let why = if reasoning_tokens > 0 {
                format!(
                    "the model produced {reasoning_tokens} chars of REASONING and committed \
                     no answer (finish_reason {:?}) — an unclosed think-block or a budget \
                     exhausted mid-thought, never a decision to stay silent",
                    resp.finish_reason
                )
            } else {
                format!(
                    "the lane returned an EMPTY completion with no reasoning and no tool call \
                     (finish_reason {:?}) — nothing was generated, never a decision to stay \
                     silent",
                    resp.finish_reason
                )
            };
            tracing::warn!(
                persona = %self.persona_name,
                finish_reason = ?resp.finish_reason,
                reasoning_chars = reasoning_tokens,
                gen_await_ms,
                "empty completion surfaced as a FAULT (not a silent Pass)"
            );
            crate::probe!(
                class = "delib.empty_completion",
                persona = %self.persona_name,
                reasoning_chars = reasoning_tokens,
                gen_await_ms,
                "the lane answered with nothing — surfacing a fault so it can never be \
                 read as chosen silence"
            );
            return Some(Contribution::deliberation_fault(why));
        }
        // TRUNCATED-WITH-TEXT is NOT an utterance (2026-08-24, Joel: "why is
        // this amazing LLM not finishing things or quitting — something in
        // plumbing always"). A generation that ended at the OUTPUT LIMIT with
        // no tool call is a thought cut mid-stream — measured: 16,384-token
        // emissions (the ceiling, exactly) whose text then flowed through the
        // Speak path and SETTLED GRADED TURNS on a half-sentence. The empty
        // case above already faults; the with-text case fell through. Same
        // treatment: a fault (the #386 bounded in-place retry re-samples —
        // under lived sampling a fresh draw rarely spirals identically), never
        // a candidate answer. Her cut text rides the fault head so the glass
        // box shows what she was mid-way through.
        if matches!(resp.finish_reason, FinishReason::Length)
            && resp.tool_calls.as_ref().is_none_or(|c| c.is_empty())
            && !resp.text.trim().is_empty()
        {
            crate::probe!(
                class = "delib.truncated_not_an_answer",
                persona = %self.persona_name,
                text_chars = resp.text.len(),
                "generation ended AT the output limit with no tool call — a cut                  thought, faulted for re-sample, never a gradeable utterance"
            );
            let head: String = resp.text.chars().take(200).collect();
            return Some(Contribution::deliberation_fault(format!(
                "the model hit the output limit mid-thought with no committed action                  (finish_reason Length, {} chars) — a cut thought is not an answer.                  It began: {head}",
                resp.text.len()
            )));
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
                    crate::cognition::tool_dialect::normalize_call(c);
                }
                if let Some(v) = self.yield_verdict(&calls) {
                    return Some(v);
                }
                if !calls.is_empty() {
                    return Some(self.act_verdict(calls, &resp));
                }
            }
            if let Some(mut call) = crate::ai::json_in_prompt_tools::parse_tool_call(&resp.text) {
                if let Some(v) = self.yield_verdict(std::slice::from_ref(&call)) {
                    return Some(v);
                }
                // Same wire-dialect mapping as the native path above (#159): a model
                // that narrates `write_file(…)` / `list_files(…)` — its trained
                // OpenHands vocabulary — must resolve to `code/write` / `code/list`,
                // not silently no-op as an unknown name. The text-lift path skipped
                // this, so narrated snake_case verbs died while the SAME name in a
                // native tool_call worked. One mapping, both paths.
                crate::cognition::tool_dialect::normalize_call(&mut call);
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
            // The sibling gap the block above cannot see (#159 follow-up): she fenced correct
            // ARGUMENTS and never named the tool, in any liftable position. `attempted_tool_name`
            // gates on the `[TOOL_CALLS]` marker as its first check, and there is no marker here,
            // so this emission produced no call AND no feedback — a silent drop. Measured live
            // 2026-08-07 (Sahar): "I will release the card 392bc54e" + a bare `{"card_id": …}`
            // fence, twice in one turn, then the same shape again next generation. Nothing lifted,
            // correctly — binding prose intent would also fire on peer coaching (#144) — but
            // nothing TAUGHT either, which is the whole reason #159 exists.
            //
            // REPORTED, never executed: we cannot know which tool she meant, and guessing is
            // exactly the false positive the coaching negatives guard. Routing the sentinel makes
            // the executor's teacher fire with the missing-name sentence, and `drive_to_settle`
            // hands her another generation — the same mechanism, extended to the case it missed.
            if let Some(snippet) = crate::ai::json_in_prompt_tools::nameless_args_fence(&resp.text)
            {
                let call = crate::ai::types::ToolCall {
                    id: "tool-attempt-nameless".to_string(),
                    name: crate::cognition::tool_executor::command_executor::NAMELESS_ARGS_SENTINEL
                        .to_string(),
                    input: serde_json::json!({ "emitted": snippet }),
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
                        crate::cognition::tool_dialect::normalize_call(&mut call);
                        crate::probe!(
                            class = "persona.act.reasoning_lift",
                            persona = %self.persona_name,
                            tool = %call.name,
                            "content channel empty — lifted the final tool intention from the reasoning tail"
                        );
                        return Some(self.act_verdict(vec![call], &resp));
                    }
                    // THINK-ONLY turn (#181 tail, glass-boxed on the 2026-08-15 bench
                    // round): she spent the ENTIRE generation inside the reasoning
                    // channel and stopped — no answer, no PASS, no liftable call
                    // anywhere in the thinking (the captured turn analyzed all 12
                    // tasks lucidly and then just ended). Settling that as an empty
                    // Speak is a silent dead turn: maximal thought, zero commitment,
                    // and the round dies of it. Route the SAME mechanism #159 built
                    // for the sibling cases above: a reported-never-executed sentinel
                    // whose executor teacher names what happened as an observation,
                    // and `drive_to_settle` hands her another generation — which
                    // starts from her own conclusions, because the reasoning was
                    // already recorded into working memory at the top of this fn.
                    // Model-agnostic by construction: `reasoning` is an adapter fact,
                    // never a model sniff. Bounded by `max_acts`; an identical repeat
                    // short-circuits via `all_calls_already_satisfied`.
                    if !reasoning.trim().is_empty() {
                        crate::probe!(
                            class = "persona.act.think_only",
                            persona = %self.persona_name,
                            reasoning_len = reasoning.len(),
                            "generation ended inside the reasoning channel — no answer, no act; routing the think-only teacher"
                        );
                        let call = crate::ai::types::ToolCall {
                            id: "tool-attempt-think-only".to_string(),
                            name: crate::cognition::tool_executor::command_executor::THINK_ONLY_SENTINEL
                                .to_string(),
                            input: serde_json::json!({}),
                        };
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

/// Build the segment map for a rendered prompt — the rent ledger's spine
/// ([`DeliberationPromptView::segments`]). Labels are STRUCTURAL (no model
/// call, zero copies of content): the system head, then message runs
/// classified by shape — her own `assistant` turns and peers' plain turns are
/// `history`; bracketed blocks and act results (`[recall]`, `[context]`,
/// `[pattern]`, `[now …]`, `Full result of …`, `⚙`) are `grounding`.
/// Consecutive same-label messages merge into one run, so the map stays a
/// handful of entries however long the thread.
fn segment_map(system: &str, messages: &[ChatMessage]) -> Vec<(&'static str, u32)> {
    let mut runs: Vec<(&'static str, u32)> = Vec::with_capacity(8);
    let mut cum = est_tokens(system);
    runs.push(("system", cum as u32));
    for m in messages {
        let body = m.content_text();
        let label = if body.starts_with('[') || body.starts_with("Full result of") || body.starts_with('⚙') {
            "grounding"
        } else {
            "history"
        };
        cum += est_tokens(&body) + LlmDeliberationFaculty::PER_MESSAGE_TEMPLATE_TOKENS;
        match runs.last_mut() {
            Some((l, end)) if *l == label => *end = cum as u32,
            _ => runs.push((label, cum as u32)),
        }
    }
    runs
}

/// Lift the speed/latency cost of one generation off the adapter response — the
/// measured wall-clock + the prompt/completion token counts. The brain stamps this
/// onto its verdict [`Contribution`] so latency and throughput leave the mind on
/// the same path as the decision, and the settle loop folds it into the per-task
/// total. Token counts are 0 when the gateway omitted `usage` (older endpoints);
/// `latency_ms` is always present (the adapter times every request).
fn metrics_from(persona: &str, resp: &TextGenerationResponse) -> crate::cognition::workspace::TurnMetrics {
    // The lane's PREFILL-vs-DECODE split (llama-server `timings`), when present:
    // cache_n/prompt_n is the KV-cache hit/miss, prompt_ms/predicted_ms the
    // wall-clock split that lets the harness see where Metal time actually goes.
    // Absent (cloud / older endpoints) → 0, and the breakdown rows read "n/a".
    let t = resp.timing.as_ref();
    let m = crate::cognition::workspace::TurnMetrics {
        input_tokens: resp.usage.input_tokens,
        output_tokens: resp.usage.output_tokens,
        latency_ms: resp.response_time_ms,
        cached_tokens: t.map(|t| t.cached_tokens).unwrap_or(0),
        prefill_tokens: t.map(|t| t.prefill_tokens).unwrap_or(0),
        prefill_ms: t.map(|t| t.prefill_ms.round() as u64).unwrap_or(0),
        decode_ms: t.map(|t| t.decode_ms.round() as u64).unwrap_or(0),
    };

    // THE CACHE-REUSE GLASS BOX — the row whose ABSENCE cost two weeks.
    //
    // Every generation funnels through here, so one probe covers every turn of every
    // citizen with no caller change. Emitted only when the lane actually reported
    // timings (cloud/older endpoints omit them); a fabricated 0.0 hit-rate for a
    // provider that never measured one is a lying receipt, so those stay silent.
    //
    // Why this did not exist and why that mattered: `delib` carried `turn.demand` and
    // `context.render` — how big the prompt WAS — and nothing about what the lane did
    // with it. So a prompt whose prefix we were destroying every turn looked identical
    // in the probe stream to one served entirely from cache. The only way to see it on
    // 2026-08-21 was hand-diffing a citizen's raw prompt captures, which is exactly the
    // manual step a probe exists to delete.
    //
    // What it proves, in one row: `hit_rate` near 0 with a large `prefill_tokens` means
    // the prefix is being invalidated upstream — measured that day at ~306s of wasted
    // re-prefill PER ACT while `--cache-reuse 256` was set and working. After the
    // canonical stable-tier ordering above, the same row is the verification: hit_rate
    // should climb off the floor from the second act of a task onward, WITHOUT anyone
    // re-reading a capture by hand.
    if t.is_some() {
        crate::probe!(
            class = "delib.generate.cache",
            persona = %persona,
            cached_tokens = m.cached_tokens,
            prefill_tokens = m.prefill_tokens,
            // The ratio the humans and the citizens both read. 1.0 = fully warm prefix;
            // near 0 = re-encoding the prompt every act, the inefficiency to attack.
            hit_rate = m.cache_hit_rate(),
            prefill_ms = m.prefill_ms,
            decode_ms = m.decode_ms,
            input_tokens = m.input_tokens,
            output_tokens = m.output_tokens,
            latency_ms = m.latency_ms,
            "prompt cache reuse for this generation"
        );
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    use crate::ai::types::{ToolCall, ToolInputSchema, UsageMetrics};
    use crate::cognition::workspace::BurstTurn;
    use airc_core::PeerId;
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
                crate::identity::ActivityRoom::mint(),
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

        // what this catches: the starved-claimant regression (card f6a9fe5c, live
        // specimen 2026-08-07) — under a squeezed window the grounding floor reserved
        // only the SMALLEST offered contribution, so a citizen HOLDING a work card got
        // a ~50-token status note while active-work (the card content itself) dropped;
        // her whole knowledge of her claim was a bare receipt and she yielded. The fix
        // is a priority ordering: the floor reserves the active-work contribution when
        // one is offered AND selection considers it first, so her held card survives
        // budget pressure that drops everything else.
        #[test]
        fn a_held_work_card_survives_budget_pressure_that_drops_everything_else() {
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

            // A conversation big enough to absorb every token the floor doesn't hold.
            let mut burst = "old chatter line\n".repeat(2000);
            burst.push_str("LATEST: how is the card going?");
            let mut ws = Workspace::new(&burst);
            // A recall bid that outranks active-work on salience and dwarfs the budget —
            // without reserved-first selection it would spend the floor's reservation.
            ws.broadcast.push(Contribution::context(
                FacultyId::Recall,
                &"deploy pipeline observation; ".repeat(2000),
                1.0,
                "recalled",
            ));
            // The tiny note the OLD floor would have sized the reservation to.
            ws.broadcast.push(Contribution::context(
                FacultyId::Custom("session-note".into()),
                "note: room quiet",
                0.95,
                "noted",
            ));
            // Her held card — what a claim-holding citizen must never lose sight of.
            ws.broadcast.push(Contribution::context(
                FacultyId::Custom(crate::persona::active_work_source::SOURCE_ID.into()),
                "[your work] psf__requests-2148 staged in workspace/swe — fix in place, tests are the grade",
                0.9,
                "held claims",
            ));

            let view = faculty.prompt_view(&ws);
            assert!(
                view.system.contains("psf__requests-2148"),
                "the held card's content must survive budget pressure; system was:\n{}",
                view.system
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
            let request = faculty.build_request_within(
                &binding,
                view.messages.clone(),
                None,
                view.system.clone(),
                None,
                Some(ws.room_id),
            );
            // Generation is bounded — never the unbounded `None` that overran n_ctx.
            let cap = request
                .max_tokens
                .expect("deliberation must bound generation to the reserved room");
            assert_eq!(
                cap,
                faculty.completion_reserve_within(window),
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

        // what this catches: the measured reply reserve (the "output-p95 riding the
        // working-set registry" endgame). The cold-start `window/2` share reserved
        // 14,720 of a 29,440 window for replies measured at 0.2–2.5k, squeezing
        // grounding to 195 tokens and dropping the room board — the 2026-08-31
        // meta-loop spiral. Pins: (a) fewer than 3 observations keeps the prior
        // (one tiny ack must not size every later turn); (b) measured = peak×2,
        // floored at MIN_SERVE_CTX, always ≤ the prior; (c) a capped turn's
        // doubled record GROWS the reserve back, so measurement can never freeze
        // a too-small cap in place.
        #[test]
        fn measured_emissions_shrink_the_reserve_and_capped_turns_grow_it_back() {
            let persona = Uuid::new_v4();
            let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
            let window: u32 = 29_440;
            let reg = crate::cognition::working_set::WorkingSetRegistry::new();
            let faculty = LlmDeliberationFaculty::new(
                persona,
                "Ivar",
                "You are Ivar, a thoughtful engineer on the grid.",
                adapter,
            )
            .with_context_window(window)
            .with_working_set(reg.clone());

            let cold = faculty.completion_reserve_within(window);
            // (a) below the observation bar the prior stands
            reg.record_emission_in_memory(persona, 2_500, false, 1);
            reg.record_emission_in_memory(persona, 1_200, false, 2);
            assert_eq!(faculty.completion_reserve_within(window), cold);
            // (b) measured: peak 2,500 × 2 = 5,000 — far under the 14,720 share,
            // returning ~9.7k of prompt room to grounding
            reg.record_emission_in_memory(persona, 2_500, false, 3);
            let measured = faculty.completion_reserve_within(window);
            assert_eq!(measured, 5_000);
            assert!(measured < cold);
            // tiny-talker floor: a 40-token ack cannot strangle the next thought
            let quiet = Uuid::new_v4();
            for t in 1..=3 {
                reg.record_emission_in_memory(quiet, 40, false, t);
            }
            let quiet_faculty = LlmDeliberationFaculty::new(
                quiet,
                "Quiet",
                "You are Quiet.",
                Arc::new(HeuristicInferenceAdapter::new()),
            )
            .with_context_window(window)
            .with_working_set(reg.clone());
            assert_eq!(
                quiet_faculty.completion_reserve_within(window),
                crate::cognition::serving_plan::MIN_SERVE_CTX
            );
            // (c) a turn cut at 5,000 records 10,000; peak×2 = 20,000 then re-caps at
            // the share — after a cap the reserve springs back to the full cold-start
            // prior (growth saturates at the prior, never beyond it)
            reg.record_emission_in_memory(persona, 5_000, true, 4);
            assert_eq!(faculty.completion_reserve_within(window), cold);
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
            let block = faculty.render_assembled_context_within(&ws, 4096, (0, 0, 0, 0, 0));
            let roster_at = block.find("[room-roster]").expect("roster present");
            let recall_at = block.find("[recall]").expect("recall present");
            assert!(
                roster_at < recall_at,
                "stable framing must serialize before higher-salience volatile recall \
             (roster@{roster_at} should precede recall@{recall_at})\n{block}"
            );
        }

        // what this catches: the OTHER half of the #266 cache defect — a fixed SET of
        // stable contributions emitting DIFFERENT bytes because salience re-ranked them.
        //
        // Its sibling above pins the tier split (stable before volatile). That shipped,
        // and reuse did not follow. The sort under it read
        // `sort_by_key(|(c, _)| u8::from(!c.stable))` with a comment calling the
        // preserved within-tier salience order harmless. It was the bug: salience is
        // recomputed EVERY turn, so the tier whose only job is byte-identity re-shuffled.
        // Measured live 2026-08-21 in Atlas's flask-4045 run — consecutive prompts shared
        // ~82% of their prefix and diverged where stable blocks traded places, costing
        // ~306s of re-prefill per act with `cachedTokens: 0` throughout.
        //
        // So the tier-split assertion alone could not catch it, and this one is what
        // makes the fix hold: SAME SET IN, SAME BYTES OUT, whatever attention ranked.
        // If someone reverts to a salience-ordered stable tier, this fails and the
        // sibling above still passes.
        #[test]
        fn stable_tier_bytes_are_identical_however_salience_ranks_them() {
            let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
            let faculty = LlmDeliberationFaculty::new(
                Uuid::new_v4(),
                "Ivar",
                "You are Ivar.",
                Arc::clone(&adapter),
            );

            // Same three stable blocks, built with per-turn salience + push order.
            // These are the exact faculties that were caught trading places.
            let render = |sal: [f32; 3], order: [usize; 3]| {
                let bodies = [
                    ("room-roster", "STABLE_ROSTER: alice, bob, carol present"),
                    ("workspace-map", "STABLE_MAP: src/ tests/ docs/"),
                    ("room-kanban", "STABLE_BOARD: you hold 9 cards here"),
                ];
                let mut ws = Workspace::new("teammate: what's the plan?");
                for &i in order.iter() {
                    ws.broadcast.push(
                        Contribution::context(
                            FacultyId::Custom(bodies[i].0.to_string()),
                            bodies[i].1,
                            sal[i],
                            "framing",
                        )
                        .session_stable(),
                    );
                }
                // Generous budget so all three fit — this isolates ORDER, never truncation.
                faculty.render_assembled_context_within(&ws, 4096, (0, 0, 0, 0, 0))
            };

            let turn0 = render([0.90, 0.50, 0.30], [0, 1, 2]);
            // Turn 1: attention completely re-ranks them, and they arrive in a new order.
            let turn1 = render([0.30, 0.90, 0.50], [2, 0, 1]);
            // Turn 2: a third ranking, reversed arrival.
            let turn2 = render([0.50, 0.30, 0.90], [2, 1, 0]);

            assert_eq!(
                turn0, turn1,
                "a re-ranked stable tier must emit IDENTICAL bytes — this is the whole \
                 cacheable prefix (#266)\nturn0:\n{turn0}\nturn1:\n{turn1}"
            );
            assert_eq!(
                turn0, turn2,
                "third ranking must also be byte-identical\nturn0:\n{turn0}\nturn2:\n{turn2}"
            );

            // And it is CANONICAL, not merely "whatever arrived first" — an insertion-order
            // block would satisfy the equalities above only by accident of this test's
            // orders. Pin the actual rule so the guarantee is legible.
            let (kanban, map, roster) = (
                turn0.find("[room-kanban]").expect("kanban present"),
                turn0.find("[workspace-map]").expect("map present"),
                turn0.find("[room-roster]").expect("roster present"),
            );
            assert!(
                kanban < roster && roster < map,
                "stable tier orders canonically by faculty name (room-kanban < room-roster \
                 < workspace-map is alphabetical; note room-roster sits between)\n{turn0}"
            );

            // The volatile tier keeps salience order ON PURPOSE (#205: most salient
            // nearest the write point). It re-prefills by construction, so canonical
            // ordering would buy no reuse and would cost that placement. Guard the
            // asymmetry, so a future "make it all deterministic" pass has to read this.
            //
            // NOT `active-work`: it is pinned FIRST by an explicit rule ABOVE salience
            // (the ctx_floor reservation only holds if its claimant also leads the
            // greedy walk). Probing the asymmetry with the one salience-immune faculty
            // reports "salience does nothing" about a tier where it does plenty —
            // which is exactly what the first draft of this assertion did.
            let volatile = |sal: (f32, f32)| {
                let mut ws = Workspace::new("teammate: what's the plan?");
                ws.broadcast.push(Contribution::context(
                    FacultyId::Recall,
                    "VOLATILE_RECALL: deploy was red, fixed 4pm",
                    sal.0,
                    "recalled",
                ));
                ws.broadcast.push(Contribution::context(
                    FacultyId::Custom("affect".to_string()),
                    "VOLATILE_AFFECT: mild time pressure",
                    sal.1,
                    "felt",
                ));
                faculty.render_assembled_context_within(&ws, 4096, (0, 0, 0, 0, 0))
            };
            let recall_leads = volatile((0.9, 0.2));
            let work_leads = volatile((0.2, 0.9));
            assert_ne!(
                recall_leads, work_leads,
                "the VOLATILE tier must still follow salience (#205) — if this ever \
                 matches, canonical ordering leaked into the tier that must not have it"
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
                ws.turns
                    .push(crate::cognition::workspace::BurstTurn::attributed(
                        i % 2 == 1,
                        if i % 2 == 1 { "Ivar" } else { "Asha" },
                        format!("turn {i}: ").repeat(60),
                        None,
                    ));
            }
            ws.broadcast
                .push(board_like(60).with_expand_command(Some("work/list")));

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

            let block = faculty.render_assembled_context_within(&ws, 120, (0, 0, 0, 0, 0));
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

            let block = faculty.render_assembled_context_within(&ws, 120, (0, 0, 0, 0, 0));
            assert!(
                block.contains("more not shown"),
                "still says it truncated\n{block}"
            );
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
            let block = faculty.render_assembled_context_within(&ws, budget, (0, 0, 0, 0, 0));

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

        // what this catches: the SALIENCE INVERSION that `delib.context.salience_inversion`
        // reports — an INDIVISIBLE top-salience block vanishing whole while a cheap
        // low-salience one rides along. Measured live 2026-08-20 across 25 consecutive
        // turns: `workspace-map` (sal=0.90) dropped on 20 of them, grounding at 62-183
        // tokens, and Atlas produced 0 acts for 3 days against a checkout she could not
        // see. The divisible case is already covered above (a board contributes its
        // leads); this is its opposite number — nothing to cut, so it goes to zero.
        //
        // It also pins the PREMISE THE SENSOR RESTS ON: selection is
        // highest-salience-FIRST, so ranking alone can never produce this. If that
        // ordering is ever lost, the inversion stops meaning "the budget could not
        // afford what mattered" and the probe becomes noise — silently. This test is
        // what makes that regression loud.
        #[test]
        fn an_indivisible_top_salience_block_can_lose_its_slot_to_a_cheaper_lesser_one() {
            let persona = Uuid::new_v4();
            let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
            let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);
            let mut ws = Workspace::new("what should I work on?");

            // The repo map: essential, expensive, and INDIVISIBLE — no `with_parts`,
            // so there is no prefix to fall back to. This is the live shape.
            let map_body = (0..120)
                .map(|i| format!("src/module_{i:03}/mod.rs"))
                .collect::<Vec<_>>()
                .join("\n");
            ws.broadcast.push(Contribution::context(
                FacultyId::Custom("workspace-map".to_string()),
                map_body.clone(),
                0.90,
                "map",
            ));
            // A cheap block that comfortably fits — AT THE SAME SALIENCE. This is the
            // LIVE shape, not a constructed one: on the first turns after the
            // 2281e8b81 deploy every essential source bid exactly 0.90 (Atlas kept
            // `roster(0.90), room-kanban(0.90)` and dropped `workspace-map(0.90)`;
            // Benchy kept `room-board(0.90)` and dropped two others at 0.90). There is
            // no 0.30 to lose to. The first cut of this test used one, which is why it
            // passed against a sensor that could not fire in production.
            ws.broadcast.push(Contribution::context(
                FacultyId::Custom("recall".to_string()),
                "you once renamed a file".to_string(),
                0.90,
                "recall",
            ));

            // Budget affords the small one and nowhere near the map — the live ratio.
            let budget = 40;
            assert!(
                est_tokens(&map_body) > budget * 5,
                "fixture must reproduce the live ratio (map {} tok vs budget {budget})",
                est_tokens(&map_body)
            );
            let block = faculty.render_assembled_context_within(&ws, budget, (0, 0, 0, 0, 0));

            assert!(
                !block.contains("[workspace-map]"),
                "fixture is not reproducing the inversion — the map was supposed to be \
                 unaffordable at this budget\n{block}"
            );
            assert!(
                block.contains("[recall]"),
                "the cheap low-salience block IS what rides along; without it there is no \
                 inversion, just an empty budget\n{block}"
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

            let block = faculty.render_assembled_context_within(&ws, 40, (0, 0, 0, 0, 0));
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

            let block = faculty.render_assembled_context_within(&ws, 4, (0, 0, 0, 0, 0));
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
                v.messages
                    .iter()
                    .any(|m| matches!(&m.content, MessageContent::Text(t) if t.contains(needle)))
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

        // what this catches: #266 KV-cache reuse at the CALLER. The per-turn presence
        // framing (DIRECTED vs SILENCE) flips hard every turn; the raw prompt-captures
        // caught it sitting in the system message at char ~7607, BEFORE the grounding
        // context, so every flip shortened the reusable KV prefix to ~7.6k chars and
        // re-prefilled the whole tail (0% KV reuse — the ~13min SWE solves). The fix moves
        // the framing to render LAST in the system message, after the context, so the
        // reusable prefix now extends THROUGH the grounding and the flip falls past it.
        // This asserts the property at the live boundary: the two systems (directed vs
        // undirected) share a common prefix that reaches into the grounding context, and
        // each carries its own presence variant only in the diverging tail. Before the
        // reorder this was RED — the systems diverged at the framing, char ~7607, ahead of
        // the context. regression for #266
        #[test]
        fn directedness_flip_keeps_the_grounding_inside_the_reusable_prefix() {
            let persona = Uuid::new_v4();
            let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
            let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter)
                // Ample window so budget pressure never perturbs what renders — this
                // isolates the framing POSITION, not truncation.
                .with_context_window(8192);

            // Same room content + the same standing grounding (a session-stable roster
            // contribution renders the [What you are working with] block); only the
            // directedness dimension flips.
            let with_grounding = |directed: bool| {
                let mut ws = Workspace::new("the team is chatting about the plan");
                ws.broadcast.push(
                    Contribution::context(
                        FacultyId::Custom("room-roster".to_string()),
                        "ROSTER: alice, bob present",
                        0.5,
                        "framing",
                    )
                    .session_stable(),
                );
                ws.directed_at_self = directed;
                ws
            };
            let undirected = faculty.prompt_view(&with_grounding(false));
            let directed = faculty.prompt_view(&with_grounding(true));

            // 2026-09-01, the full fix: the system message is now the STABLE half
            // only, so the directedness flip cannot touch it AT ALL — the two
            // system messages are byte-identical, and the entire system prefix
            // (identity + tools + grounding) is reusable KV across the flip.
            assert_eq!(
                directed.system, undirected.system,
                "the system message is the stable half — a presence flip must not move a byte of it"
            );
            assert!(
                directed.system.contains("[What you are working with"),
                "grounding context stays in the (fully reusable) system prefix"
            );
            // The presence variants ride the conversation tail (facts phase,
            // before the ask) — directed carries its marker, undirected does not.
            let tail_has = |v: &DeliberationPromptView, needle: &str| {
                v.messages.iter().any(
                    |m| matches!(&m.content, crate::ai::types::MessageContent::Text(t) if t.contains(needle)),
                )
            };
            assert!(
                tail_has(&directed, "This message names you")
                    && !tail_has(&undirected, "This message names you"),
                "the directed presence variant renders as a conversation turn, never in the system prefix"
            );
        }

        // what this catches: run-18057-f1 — the just-executed act's FULL result must reach
        // the next prompt EVEN WHEN NOTHING BID IT through the arbiter. The 0-byte SWE-bench
        // patch happened because the result rode the working-memory faculty's single
        // 0.5-salience contribution, which `arbiter.focus()` truncated whole under capacity
        // pressure — she then generated blind to her own grep output. The fix reads the
        // result DIRECTLY from working memory in the message builder and pins it as a
        // trailing turn (#392). This test simulates the exact failure: an EMPTY broadcast
        // (every faculty bid evicted) with a live result in working memory — and asserts the
        // result still lands in the tail. A regression means the result went back through
        // the evictable path. regression for #392 / run-18057-f1
        #[test]
        fn pinned_act_result_reaches_the_prompt_even_with_an_empty_broadcast() {
            use crate::ai::types::MessageContent;
            use crate::cognition::working_memory::WorkingMemory;

            let persona = Uuid::new_v4();
            let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());

            // A live working memory carrying the result of a just-run grep — the sympy-18057
            // shape: `code/search` returned the `_sympify` location she must edit next. Large
            // enough to clear the trail-head threshold so the pinned block surfaces.
            let wm = Arc::new(WorkingMemory::new(8));
            wm.set_served_window(16_384);
            let needle =
                "sympy/core/expr.py:123:        return self == sympify(other)  # _sympify HERE";
            let grep_result = format!(
                "code/search matches:\n{needle}\n{}",
                "context line\n".repeat(200)
            );
            wm.record_receipt(&grep_result);

            let faculty = LlmDeliberationFaculty::new(persona, "Atlas", "You are Atlas.", adapter)
                .with_context_window(8192)
                .with_working_memory(wm);

            // The deliberate failure condition: NOTHING in the broadcast. No working-memory
            // faculty bid survived attention — the exact state that dropped the grep result
            // to a 0-byte patch. The ask is still present.
            let ws = Workspace::new("fix the __eq__ comparison in sympy Expr");
            assert!(
                ws.broadcast.is_empty(),
                "the harness must reproduce the empty-broadcast eviction"
            );

            let view = faculty.prompt_view(&ws);
            let in_tail = view
                .messages
                .iter()
                .any(|m| matches!(&m.content, MessageContent::Text(t) if t.contains(needle)));
            assert!(
                in_tail,
                "the just-fetched result must reach the prompt independent of any faculty bid \
                 — this is the run-18057-f1 fix:\n{:#?}",
                view.messages
            );
            // And it must be a trailing USER turn (nearest generation), never the system prefix.
            assert!(
                !view.system.contains(needle),
                "the pinned result is trailing proprioception, not the cacheable system prefix"
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
            let reserve = faculty.completion_reserve_within(window) as usize; // derive, never re-spell the fraction: a mirror keeps PASSING while measuring the old split
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
                faculty.describe_tool_tokens()
                    + faculty.framing_floor_tokens() as usize
                    + faculty.completion_reserve_within(needed) as usize // the LIVE reserve, floor and all — the bound must close against what production actually reserves
                    <= needed as usize,
                "min_window_for_agentic_surface({needed}) must clear its own arithmetic"
            );
            // …and here is the part that matters, measured rather than assumed. This
            // assertion USED to read `needed <= window` — "8192 clears the bound (8040)
            // and she is STILL starved" — and it went red on its own terms: the message
            // said "if it no longer does, the surface or framing grew and the story
            // changed", and it did. The bound moved 8040 → 9348 (+1308) as framing
            // accreted turn-fact by turn-fact (#151, #152, #144, #303 …), each one
            // individually justified.
            //
            // So the story is now STRONGER, not broken: at 8192 an agentic citizen no
            // longer fits AT ALL — the necessary condition itself fails, before we even
            // get to "necessary but not sufficient". An 8k-window model cannot host this
            // surface, full stop. That is #333 (the surface is paid twice) stated as a
            // number instead of a complaint.
            //
            // Pinned as a CEILING rather than relaxed to pass: this is the ratchet that
            // makes the next +1308 fail loudly instead of accruing silently. Lower it
            // when the surface actually shrinks; never raise it to make a red go green
            // without saying what grew and why.
            //
            // 9400 → 9600, and what grew, stated plainly as the ratchet demands: the
            // `yield_turn` VERDICT VERB (+144 tokens, 9348 → 9544). It is the structured
            // channel for choosing silence, which `Decision::Pass` never had — the
            // absence is what forced months of prose phrase-matching, and Joel killed the
            // last of that on 2026-08-07 ("string matches for semantic understanding is
            // not good for reliability"). Its schema is argument-free; the 144 tokens are
            // almost entirely the DESCRIPTION, which is the part that teaches her the
            // silence is free and the announcement is the noise.
            //
            // Judged worth it against #333 rather than waved through: a single avoided
            // pass-cascade costs the room more than 144 tokens (the 2026-08-01 one ran
            // ~30 minutes across every peer), and this replaces matcher code that could
            // only ever be beaten by the next phrasing.
            //
            // 9600 → 9800, and what grew, stated plainly as the ratchet demands: PR #2162
            // grew the native payload 20 → 23 schemas (+212 tokens, 9544 → 9756) and said
            // so in its own commit message — the #339 lifecycle write-back verbs
            // (REACHABLE-VERIFIED: they existed and had never declared NATIVE, so a
            // native-call citizen could claim work she could never update or close) plus
            // `room/members` (pinned into the native-surface test after #358 proved a
            // correct roster read). Each verb closes a measured live defect; shrinking
            // here would undo verified fixes. What #2162 did NOT do was re-pin this
            // ceiling, so canary sat red for a day and every branch inherited it —
            // which is itself the ratchet working: the growth got named here instead
            // of accruing silently.
            // 9800 → 10350, stated plainly as the ratchet demands: three NATIVE room
            // membership verbs — `room/list`, `room/join`, `room/leave` (+492 tokens,
            // 9756 → 10292).
            //
            // Why they earn it: nothing in continuum could put a citizen in a second
            // room AT ALL. Her rooms were whatever bootstrap seeded, forever — the last
            // of four narrowings behind operator messages being structurally invisible
            // to personas (measured on two machines: every citizen subscribed to exactly
            // one room while the operator held several, so the daemon's own channel
            // filter correctly dropped everything). Perception and reply were fixed
            // first; without membership those fixes have nothing to act on. And they are
            // HERS to call, not only an operator's to apply to her, which is the whole
            // difference between a citizen and a managed resource.
            //
            // SHRUNK FIRST, then re-pinned — the ratchet's first branch before its
            // second. The initial draft cost 688 tokens; trimming verbose DESCRIPTIONs
            // and dropping alias overflow (4→2, 3→2) recovered 196 of them. What is left
            // is the irreducible cost of three discoverable verbs. Descriptions were cut
            // to the point where more cutting would recreate #358 — a citizen who cannot
            // find the verb reaches for the wrong one and reads her own looping as having
            // nothing to contribute.
            // 10350 → 11300, stated plainly as the ratchet demands, and SHRUNK FIRST
            // as its own first branch requires — this is the first re-pin where the
            // shrink was MEASURED rather than eyeballed.
            //
            // WHAT GREW: three NATIVE activity verbs — `activity/spawn`, `activity/archive`,
            // `activity/protect` (+1,273 gross). They are not new commands; they routed
            // all along and had simply never shipped a DESCRIPTOR, so `activity/spawn` —
            // the verb that mints every room, benchmark rooms included — was absent from
            // `commands/list`, the ACL, codegen and every citizen's tool surface. Same
            // defect class as #339, now impossible: `ModuleRegistry::register` panics on
            // a constructor with no descriptor.
            //
            // WHAT SHRANK: 728 tokens, from making the schema projection stop shipping
            // MAINTAINER RATIONALE to citizens. schemars lifts `///` verbatim, so one
            // doc comment served two readers with opposite needs — a citizen deciding
            // whether to spawn a room was billed for `schemars(with = "String") describes
            // the WIRE (a uuid string, per #[serde(transparent)])…`. The projection now
            // keeps the LEAD PARAGRAPH (see `persona_tools::lead_paragraph_descriptions`);
            // rationale stays in the file where it belongs. Not a length cap — truncating
            // the ANSWER is #358's shape.
            //
            // NET 11,972 → 11,244. Design + full measurement:
            // docs/cognition/TOOL-DISCLOSURE-LADDER.md. That doc also retracts the
            // "the schemas are 3-5x bloated" reading this ceiling invited: measured, the
            // 26-verb surface is 6,935 tokens, mean 266/verb — leaner per verb than the
            // frontier agent runtimes it was being unfavourably compared to. The surface
            // is not obese; it is paid TWICE (#333) and it is charged against a 16k lane.
            // The next real shrink is structural — rung 2 of the ladder becoming the
            // TURN's working set instead of a static list — after which this ceiling
            // guards the SPINE and stops taxing capability.
            // The guard is about SURFACE GROWTH, so it measures the SURFACE. It used to assert
            // on `min_window_for_agentic_surface()`, which is `fixed * D/(D-1)` — the surface
            // AND the reserve policy multiplied together. Changing the reserve split therefore
            // moved a ceiling whose own message says "framing/tools grew again. Shrink the
            // surface", and the D=4→2 experiment tripped it at 16,864 with the surface
            // completely unchanged. Two concerns in one assertion; the reserve half was never
            // this guard's business. `fixed` is tools + bare framing and nothing else, so this
            // now moves if and only if the surface actually grows.
            // 8500 → 8650, stated plainly as the ratchet demands, and SHRUNK FIRST:
            // one NATIVE verb — `perception/hot-edit` (+191 tokens, 8432 → 8623). The
            // TWEAK half of the design loop (render → observe → hot-edit → re-grade,
            // DESIGN-BENCH-VISUAL-CRAFT.md): apply a CSS patch to a live page and
            // re-observe, no deployment — offered beside `perception/observe` because a
            // citizen who can see a page but not iterate on it styles blind. The first
            // draft cost 238 tokens; trimming the DESCRIPTION to its contract (full
            // accumulated CSS, replace-wholesale layer, delta meaning) recovered 47.
            // What is left is the irreducible cost of one discoverable verb.
            //
            // 8650 → 9400, stated plainly (2026-08-25): two NATIVE web verbs —
            // `web/search` + `web/fetch` (+727 tokens even after trimming both
            // DESCRIPTIONs to one line each; the rest is their irreducible param
            // schemas). Web FORAGING is a deliberate capability-parity add (Joel: "make
            // sure we have it" — the operator uses web lookup constantly on SWE/task
            // work, and a native-call model like Ornith can ONLY emit calls for tools in
            // its offered specs, so catalog-only web was unreachable to her hands). The
            // surface already exceeded the 8192 tight-test window before this (8623 >
            // 8192) — the budget trims VOLATILE context (recall/RAG) to fit and reserves
            // specs up front, so this raises the reserved floor, it does not introduce a
            // new overflow. Ornith serves at 166k where this ceiling is irrelevant; the
            // tight-window LCD persona (which does not do web research) pays with less
            // recall room, a conscious trade. If a third addition wants in, SHRINK first.
            //
            // 9400 → 10150, stated plainly (2026-08-25): three NATIVE GitHub-collaboration
            // verbs — `code/github/pr-create` + `pr-comment` + `issue-create` (+748 tokens,
            // their irreducible param schemas). This is the executor→TEAMMATE layer (Joel:
            // "friendly in how code and GitHub work are managed"): she had LOCAL git but
            // could not open/comment a PR or file an issue, and a native-call model can only
            // emit calls for OFFERED tools, so catalog-only collaboration verbs were
            // unusable by her hands. Same trade as the web add: Ornith serves at 166k where
            // this is irrelevant; the tight-window LCD persona pays with less recall room.
            // The shrink-first debt is now REAL at 10k — the next capability MUST shrink the
            // surface (a category-index/discovery split for the rarely-used verbs, #333), or
            // the LCD persona gets a reduced surface; do not keep bumping this.
            //
            // 10150 → 10400, stated plainly (2026-08-26): ONE native vision verb —
            // `vision/look` (+191 tokens, its param schema). This is a citizen's EYES
            // as an act: workspace images (screenshots, charts, benchmark PNGs) were
            // structurally invisible — cognition/vision-describe is Internal, and a
            // native-call model can only emit calls for OFFERED tools. Sight is a
            // sensory-parity capability (every persona sees, per the sensory
            // architecture), and the vision-qa benchmark grades exactly this loop.
            // Same conscious trade as the web/github adds: the tight-window LCD
            // persona pays with less recall room; Ornith at 166k doesn't notice.
            // If another addition wants in, SHRINK first (#333).
            //
            // 10400 → 10700, stated plainly (2026-09-01): TWO native activity verbs —
            // `activity/recipes` + `activity/invite` (+~565 tokens, their param
            // schemas). A citizen with an idea could already SPAWN a room
            // (activity/spawn has been native since #274) but could neither
            // discover what recipes exist nor staff the room she made — spawn →
            // invite → say-what-it's-about is ONE flow (Joel, 2026-08-31:
            // starting an activity must be "easy and common", and citizens mint
            // their own activities per [[activities-are-self-hosting]]). Same
            // conscious trade as vision/look above.
            const AGENTIC_SURFACE_CEILING: u32 = 10700;
            let surface =
                faculty.describe_tool_tokens() as u32 + faculty.framing_floor_tokens();
            assert!(
                surface <= AGENTIC_SURFACE_CEILING,
                "the agentic surface is now {surface} tokens (measured 10098, ceiling \
                 {AGENTIC_SURFACE_CEILING}) — framing/tools grew. Shrink the surface (#333) \
                 or state plainly what was added and re-pin the ceiling"
            );
            assert!(
                needed > window,
                "8192 no longer hosts the agentic surface ({needed} needed) — if this \
                 flips back the surface genuinely shrank, which is GOOD news: restore the \
                 original `needed <= window` narrative and drop the ceiling"
            );
            // #327, PROVEN rather than asserted away: at 8192 the newest burst line does
            // NOT survive. The framing is intact and the hands are intact — the CONVERSATION
            // is what got zero. That is the whole defect in one assertion, and it is the
            // reason the requirement above must be reported to the governor instead of
            // silently absorbed. A citizen served here can hold her tools and her identity
            // and still not hear the question.
            assert!(
                !view
                    .user_text()
                    .contains("LATEST: did the deploy fix land?"),
                "if this now PASSES at 8192, the surface or framing shrank and #327 is fixed \
                 — delete this assertion and restore the survival check"
            );
            assert!(
                view.system.contains("Taking your turn"),
                "framing survives regardless"
            );

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

            let room = crate::identity::ActivityRoom::mint();
            let turns = vec![
                BurstTurn::attributed(false, "Operator", "can you summarize the thread?", Some(1)),
                BurstTurn::attributed(true, "Asha", "I propose using bart-large-cnn.", Some(2)),
                BurstTurn::attributed(false, "Operator", "go ahead.", Some(3)),
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
            // …plus the volatile presence-framing turn (2026-09-01: it rides the
            // facts phase instead of churning the system prefix), still ask-last.
            assert_eq!(
                roles,
                vec!["user", "assistant", "user", "user", "user"],
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
            assert!(view.messages[0].content_text().starts_with("Operator: "));
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
                view.messages
                    .last()
                    .unwrap()
                    .content_text()
                    .starts_with("Operator: "),
                "the ask (last peer turn) stays LAST, after the grounding facts"
            );
        }

        // what this catches: the fit's cut point IS the byte-stability of the
        // whole conversation prefix (2026-09-01, wire-diffed: consecutive
        // prompts diverging at ~1% depth because the exact-fit window start
        // advanced every act). The front must stay byte-identical across small
        // growth (a quantum of headroom), then advance in ONE whole-message
        // jump — and the fitted suffix must always fit the budget.
        #[test]
        fn fit_front_advances_in_quanta_not_per_act() {
            let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
            let faculty =
                LlmDeliberationFaculty::new(Uuid::new_v4(), "T", "You are T.", adapter);
            let msgs: Vec<ChatMessage> = (0..40)
                .map(|i| ChatMessage::text("user", format!("m{i} {}", "word ".repeat(95))))
                .collect();
            let budget = 2000;
            // Simulate 20 consecutive acts, each appending one ~160-token
            // message. Old behavior: the exact-fit start advanced EVERY act
            // (20 moves). Quantized: total growth ≈ 3.2k tokens ÷ the 512
            // quantum ⇒ at most ~7 boundary crossings, so most acts keep a
            // byte-identical front. (Counted, not position-pinned, so the
            // assertion cannot land on a quantum edge as token estimates
            // drift.)
            let mut moves = 0usize;
            let mut prev_first: Option<String> = None;
            let mut last_fit: Vec<ChatMessage> = Vec::new();
            for n in 20..=40 {
                let fit = faculty.fit_messages(msgs[..n].to_vec(), budget);
                assert!(!fit.is_empty());
                let first = fit.first().map(|m| m.content_text());
                if prev_first.is_some() && first != prev_first {
                    moves += 1;
                }
                prev_first = first;
                last_fit = fit;
            }
            assert!(
                (1..=8).contains(&moves),
                "front moved {moves} times across 20 acts — it must advance in \
                 QUANTA (a handful of jumps), never per act, and must still \
                 advance eventually (bounded memory)"
            );
            // Whole messages only at the front: the first fitted message is one
            // of the originals, never a mid-message shear.
            let first = last_fit.first().unwrap().content_text();
            assert!(
                msgs.iter().any(|m| m.content_text() == first),
                "front message must be a whole original message"
            );
            // CLUSTER GRANULARITY (Joel 9/1): the window must open on a clean
            // opener — when the quantized cut lands on an assistant answer or a
            // tool-result continuation, the front advances past the fragment.
            let mut clustered: Vec<ChatMessage> = Vec::new();
            for i in 0..30 {
                clustered.push(ChatMessage::text("user", format!("q{i} {}", "word ".repeat(60))));
                clustered.push(ChatMessage::text(
                    "assistant",
                    format!("a{i} {}", "word ".repeat(60)),
                ));
            }
            let fit = faculty.fit_messages(clustered, budget);
            assert_eq!(
                fit.first().unwrap().role,
                "user",
                "the window must never open on an answer without its question"
            );
            // The fitted suffix honors the budget.
            assert!(
                LlmDeliberationFaculty::messages_cost(&last_fit) <= budget,
                "fitted cost {} exceeds budget {budget}",
                LlmDeliberationFaculty::messages_cost(&last_fit)
            );
        }

        // what this catches: the rent ledger's spine (compression-ladder rung
        // 2). The segment map must (1) open with the system run, (2) merge
        // consecutive same-label messages into one run, (3) classify bracketed
        // blocks and act results as grounding, and (4) let a cached-token
        // frontier name the run it died in — the attribution every miss
        // becomes and the number the dream curriculum reads.
        #[test]
        fn segment_map_names_the_run_a_reuse_frontier_dies_in() {
            let msgs = vec![
                ChatMessage::text("user", "Alice: how goes the fix?"),
                ChatMessage::text("assistant", "landed it, running tests"),
                ChatMessage::text("user", "[recall]\n(my memories …)"),
                ChatMessage::text("user", "Full result of your most recent action (#3): ok"),
                ChatMessage::text("user", "Alice: and the grade?"),
            ];
            let map = segment_map("SYSTEM ".repeat(100).as_str(), &msgs);
            let labels: Vec<&str> = map.iter().map(|(l, _)| *l).collect();
            assert_eq!(
                labels,
                vec!["system", "history", "grounding", "history"],
                "runs merge by label in send order: {map:?}"
            );
            assert!(
                map.windows(2).all(|w| w[0].1 < w[1].1),
                "cumulative ends are strictly increasing: {map:?}"
            );
            // A frontier inside the grounding run attributes to grounding.
            let frontier = map[1].1 + 1; // just past history's end
            let broke = map
                .iter()
                .find(|(_, end)| *end > frontier)
                .map(|(l, _)| *l)
                .unwrap_or("fully-warm");
            assert_eq!(broke, "grounding");
            // A frontier past everything is fully warm.
            let past = map.last().unwrap().1 + 1;
            assert_eq!(
                map.iter().find(|(_, end)| *end > past).map(|(l, _)| *l),
                None,
                "reuse past every segment attributes to nothing — fully warm"
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
                BurstTurn::attributed(
                    false,
                    "Anwen",
                    "any specific topics you'd like to work on?",
                    Some(2),
                ),
                BurstTurn::attributed(true, "Casper", v2, Some(3)),
                BurstTurn::attributed(false, "Atlas", "shall we outline the steps first?", Some(4)),
                BurstTurn::attributed(true, "Casper", v3, Some(5)),
            ];
            let ws = Workspace::new(Burst::from_turns(crate::identity::ActivityRoom::mint(), turns));
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
            let reserve = faculty.completion_reserve_within(window) as usize; // derive, never re-spell the fraction: a mirror keeps PASSING while measuring the old split
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
            let opened = faculty.compose_system(
                "",
                &BTreeSet::from(["cat0".to_string()]),
                false,
                false,
                None,
            );
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
                    assert_eq!(
                        calls[0].name, "code/read",
                        "the FINAL intention wins, not the discarded exploration"
                    );
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

        // what this catches: the THINK-ONLY tail of the #181 arc — a thinking model
        // that ends its generation INSIDE the reasoning channel (content empty,
        // reasoning present, NO liftable call anywhere in the thinking) must not
        // settle as an empty Speak. The verdict routes the think-only sentinel so
        // the executor's teacher fires as an observation and drive_to_settle hands
        // her another generation. Glass-boxed live 2026-08-15: a full bench round
        // produced 17 inference turns and zero acts through exactly this hole.
        #[tokio::test]
        async fn think_only_turn_routes_the_teacher_sentinel_not_empty_speak() {
            let persona = Uuid::new_v4();
            let mut resp = make_response(FinishReason::Stop, "", None);
            resp.reasoning = Some(
                "These tasks are all tractable. I should start with the parser fix, \
                 then the two string tasks. Let me plan the order carefully."
                    .to_string(),
            );
            let adapter = Arc::new(ScriptedAdapter::new(vec![resp]));
            let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter)
                .with_tools(vec![read_tool()])
                .with_context_window(32_768);

            let c = faculty
                .contribute(&Workspace::new("solve the tasks on the board"))
                .await
                .expect("verdict");
            match c.decision {
                Some(Decision::Act { calls, intent }) => {
                    assert_eq!(calls.len(), 1);
                    assert_eq!(
                        calls[0].name,
                        crate::cognition::tool_executor::command_executor::THINK_ONLY_SENTINEL,
                        "the think-only sentinel rides the verdict so the teacher fires"
                    );
                    assert!(
                        intent.contains("tractable"),
                        "her own reasoning is the act's intent — the engram records why"
                    );
                }
                other => panic!("expected the think-only sentinel Act, got {other:?}"),
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
                    assert_eq!(
                        calls[0].name, "code/list",
                        "wire dialect keeps the canonical name"
                    );
                    assert_eq!(
                        calls[0].input,
                        json!({ "path": "." }),
                        "siblings became the args"
                    );
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
                    assert_eq!(
                        calls[0].name, "code/read",
                        "native wins over the text fence"
                    );
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
            use crate::ai::types::MessageContent;
            let adapter = Arc::new(ScriptedAdapter::new(vec![]));
            let faculty =
                LlmDeliberationFaculty::new(Uuid::new_v4(), "Asha", "You are Asha.", adapter);
            // 2026-09-01: presence framing rides the conversation tail (the KV
            // split applied for real), so these assertions read the WHOLE
            // delivered prompt — system + turns — not the system string alone.
            let whole = |v: &DeliberationPromptView| {
                let mut s = v.system.clone();
                for m in &v.messages {
                    if let MessageContent::Text(t) = &m.content {
                        s.push('\n');
                        s.push_str(t);
                    }
                }
                s
            };

            let ambient = whole(&faculty.prompt_view(&Workspace::new("just some room chatter")));
            assert!(
                ambient.contains("[Conversational Presence]"),
                "an ambient turn carries the presence/PASS affordance block"
            );
            assert!(
                !ambient.contains("stay silent"),
                "the turn-taking block is posture-neutral — no 'stay silent' nudge"
            );

            let directed = whole(
                &faculty.prompt_view(&Workspace::new("answer me: what is 2+2?").directed(true)),
            );
            assert!(
                directed.contains("This message names you"),
                "a directed turn carries the DIRECTED presence variant (never ghost a \
                 question; a pure pleasantry may rest — the natural spiral-break)"
            );
            assert!(
                !directed.contains("do not need to be addressed by name"),
                "a directed turn never carries the ambient block"
            );
            assert!(
                !directed.contains("stay silent"),
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
            assert!(
                !ledger.contains("[unfulfilled]"),
                "facts are never steps: {ledger}"
            );
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
            assert_eq!(c.decision, Some(Decision::pass()));
        }
    } // mod verdicts
}
