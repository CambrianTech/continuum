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

use arc_swap::{ArcSwap, ArcSwapOption};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use uuid::Uuid;

use super::deliberation_budget::{est_tokens, turn_message_line_addressed, GUARD_CHARS_PER_TOKEN};
use super::deliberation_parse::decision_from_response;
use super::deliberation_prompt;
use super::context_render::{self, ContextComposition};
use super::epoch_view;
use super::grounding_plan::{Fitted, GroundingPiece, GroundingPlan};
use continuum_context::epoch::SourceId;
use super::persona_tools;
use super::workspace::{Contribution, Decision, Faculty, FacultyId, TurnAttention, TurnVoice, Workspace};
use crate::ai::adapter::AIProviderAdapter;
use crate::ai::types::{ActiveAdapterRequest, ChatMessage, FinishReason, NativeToolSpec, TextGenerationRequest, TextGenerationResponse};

/// Ambient deliberation yields to measured foreground work. Human opportunity
/// keeps its existing turn class even when the message did not name the persona.
fn deliberation_slot_class(
    purpose: Option<&str>,
    attention: TurnAttention,
) -> crate::inference::slots::SlotClass {
    use crate::inference::slots::{class_for, SlotClass};
    match class_for(purpose) {
        SlotClass::Turn if !attention.requires_priority() => SlotClass::Background,
        class => class,
    }
}

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

pub use super::model_binding::ModelBinding;

mod verdict;

mod request;
use self::request::{TurnKind, TurnShape};
use crate::ipc::vitals_emitter;
use crate::modules::citizen_health;

mod tool_surface;

struct ActiveVisualFeedback {
    seq: u64,
    parts: Vec<(String, crate::ai::types::ContentPart)>,
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
    // A bound faculty always names the model it is bound to. `None` meant "the adapter's
    // own model" and was left as `None`, so every rate lookup keyed by the model took the
    // no-model arm: on the M5 (2026-10-18, 21 of 21 turns since 13:45Z) the latency cap
    // read `rate_source none` beside a persisted 105.7 tokens/s for the served model and
    // fell to the unmeasured floor, five times the latency target. Resolve it HERE, the
    // one owner of the binding, never at each reader.
    let model = Some(model.unwrap_or_else(|| adapter.default_model().to_owned())); // unwrap_or_else: `None` = the adapter's own model, resolved at bind time
    Arc::new(ArcSwap::from_pointee(ModelBinding {
        adapter,
        model,
        context_window,
    }))
}

/// Calibration from one authoritative provider observation. This scales the
/// CURRENT prompt estimate; a large request's absolute excess is never charged
/// to every later small request. It remains an estimate, not a tokenizer proof.
#[derive(Clone, Copy)]
struct PromptCalibration {
    estimated: std::num::NonZeroUsize,
    measured: std::num::NonZeroU32,
    input_identity: Option<[u8; 32]>,
}

impl Default for PromptCalibration {
    fn default() -> Self {
        Self {
            estimated: std::num::NonZeroUsize::MIN,
            measured: std::num::NonZeroU32::MIN,
            input_identity: None,
        }
    }
}

impl PromptCalibration {
    fn observed(estimated: usize, measured: u32, input_identity: Option<[u8; 32]>) -> Option<Self> {
        Some(Self {
            estimated: std::num::NonZeroUsize::new(estimated)?,
            measured: std::num::NonZeroU32::new(measured)?,
            input_identity,
        })
    }

    fn charge(self, estimate: usize) -> usize {
        let tokens =
            (estimate as u128 * self.measured.get() as u128).div_ceil(self.estimated.get() as u128);
        // Never underprice the existing estimate. u128 keeps this portable even
        // for a full usize-sized prompt estimate multiplied by a u32 count.
        estimate.max(tokens.min(usize::MAX as u128) as usize)
    }

    fn prompt_budget(self, measured_budget: usize) -> usize {
        let tokens =
            measured_budget as u128 * self.estimated.get() as u128 / self.measured.get() as u128;
        measured_budget.min(tokens.min(usize::MAX as u128) as usize)
    }
}

/// One observation in the existing persona-owned faculty. Route/window changes
/// invalidate it; no global cache, room map, or retained request payload.
struct PromptFeedback {
    binding: Arc<ModelBinding>,
    calibration: PromptCalibration,
    available: u32,
}

impl PromptFeedback {
    fn applies_to(&self, binding: &ModelBinding) -> bool {
        Arc::ptr_eq(&self.binding.adapter, &binding.adapter)
            && self.binding.model == binding.model
            && self.binding.context_window == binding.context_window
    }
}

/// Default sampling temperature for deliberation — enough warmth for natural
/// voice, not so much it drifts.
const DEFAULT_TEMPERATURE: f32 = 0.7;

/// A pass is bounded at this multiple of her measured p90 turn (never below the act's
/// cap). A runaway bound, not a budget: it binds only on a loop (Cormac on #4815: a loop
/// must not hold the lane for minutes; twice her p90 sits far above the one-in-ten tail
/// a p90 lets through).
const PASS_RUNAWAY_MULTIPLE: u32 = 2;

/// The flex claim ids of the deliberation prompt plan. One window, five claimants.
const FLEX_FRAMING: &str = "framing";
const FLEX_REQUIRED: &str = "required_input";
const FLEX_GROUNDING: &str = "grounding";
const FLEX_HISTORY: &str = "history";
const FLEX_REPLY: &str = "reply";

/// THE REPLY'S CLAIM on the served window, and its runaway bound. Pure.
///
/// The reply is one claimant in the same flexbox allocation as framing, required input,
/// grounding and history ([`crate::persona::rag_budget::flex_allocate`]) — never a share
/// carved off the top by its own rule. The carve-off is what cut Benchy's first write
/// mid-call (2026-10-18 02:10Z): her reserve was twice her read-only peak (~5.4k), her
/// think was capped near 2k (`delib.emission.think_budget_hit` at 1,592–1,746), and the
/// write needed more than either, on a window with room to spare.
///
/// - floor = min: the smallest usable reply
///   ([`LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS`]); an ACT floors at her measured
///   answer channel (the tool call itself). The caller caps it so required input always
///   fits — the reply never takes from required content.
/// - max: what she is expected to write — her measured need, or the reply-sized cold
///   prior when unmeasured; at least twice a pending cut's peak (the cut write gets its
///   room back on the retry). Never past the runaway bound.
/// - priority: an ambient pass 4, a directed pass 6, an act 8, an act on live held work
///   or after a pending cut 10 — the weight the reply grows by against history (5) and
///   grounding (6).
///
/// Why the floor is the min and not the whole want: the allocator's min pass runs for
/// EVERY claim before any claim grows, so a min of the whole want would pre-empt history
/// and grounding regardless of priority — on a 2,048 window an ambient pass's 2,048-token
/// prior took the entire window and the presence framing dropped out of the prompt
/// (`directed_turn_withholds_the_silence_escape`). Above the floor the reply GROWS by its
/// priority, so priority is what decides.
///
/// The runaway bound: [`LlmDeliberationFaculty::ACT_OUTPUT_CAP`] for an act; for a pass,
/// twice her measured turn, never below the act's cap.
fn reply_claim(
    kind: TurnKind,
    need: Option<super::working_set::OutputNeed>,
    pending_peak: u32,
    live_work: bool,
    directed: bool,
) -> (crate::persona::rag_budget::RagSourceBudget, u32) {
    let cap = LlmDeliberationFaculty::ACT_OUTPUT_CAP;
    let runaway = match kind {
        TurnKind::Act => cap,
        TurnKind::Pass => cap.max(need.map_or(0, |n| n.total().saturating_mul(PASS_RUNAWAY_MULTIPLE))),
    };
    let floor = match (kind, need) {
        (TurnKind::Act, Some(n)) => n.answer.max(LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS),
        _ => LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS,
    }
    .min(runaway);
    let wanted = need
        .map_or(LlmDeliberationFaculty::COMPLETION_COLD_PRIOR_TOKENS, |n| n.total()) // map_or: unmeasured = a REPLY-sized prior, never a window fraction
        .max(pending_peak.saturating_mul(2))
        .max(floor)
        .min(runaway);
    let priority = match kind {
        TurnKind::Act if live_work || pending_peak > 0 => 10,
        TurnKind::Act => 8,
        TurnKind::Pass if directed => 6,
        TurnKind::Pass => 4,
    };
    (
        crate::persona::rag_budget::RagSourceBudget {
            source_id: FLEX_REPLY.to_owned(),
            priority,
            floor_tokens: floor,
            min_tokens: floor,
            max_tokens: wanted,
            required: true,
        },
        runaway,
    )
}

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
    prompt_feedback: ArcSwapOption<PromptFeedback>,
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
    /// Her HANDS for a work turn: the subset of `native_specs` whose COMMAND
    /// name (before the wire dialect renames it) is a file/work/git/cargo/tool
    /// verb, plus the discovery pair. Chosen at rebuild on the raw names —
    /// filtering the wire names by `code/` prefixes matched nothing and every
    /// work turn went out with ZERO tools (2026-09-04, 13 turns, all passed).
    hands_specs: Vec<NativeToolSpec>,
    /// `hands_specs` plus the reviewer verbs (`tool_dialect::reviewer_hand`): the hands
    /// of a work turn on a REVIEW card. Built beside `hands_specs` at rebuild.
    review_hands_specs: Vec<NativeToolSpec>,
    /// The RAW command name of each entry in `native_specs`, same order. The specs
    /// are cached in the model's wire dialect (`edit_file`), so any selection keyed
    /// on canonical names (`code/edit`) — the room's affordances — must key here.
    /// The 2026-09-04 muting was exactly a name-space mismatch at this seam.
    native_command_names: Vec<String>,
    /// Token cost of serializing `native_specs` into the request — memoized at
    /// [`Self::rebuild_tool_surface`] time because the specs are static between
    /// rebuilds while [`Self::describe_tool_tokens`] is consulted on EVERY
    /// prompt assembly (per-turn serde of ~a dozen schemas, pure waste).
    tool_surface_tokens: usize,
    /// Token cost of serializing `hands_specs` — memoized for the same reason as
    /// `tool_surface_tokens`, and needed for the same accounting.
    ///
    /// Without it the budget prices the FULL registry on a turn that sends only
    /// hands, which is not a cosmetic error: `prompt_view_within` subtracts this
    /// from the message budget, so over-pricing trims conversation to fit room
    /// that was never occupied. Card dec1a7ff.
    hands_surface_tokens: usize,
    /// Token cost of `review_hands_specs`, memoized like `hands_surface_tokens`.
    review_hands_surface_tokens: usize,
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
    /// Her user-side view, held across acts: it only appends until it turns over.
    epoch: parking_lot::Mutex<epoch_view::EpochView>,
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
            binding: model_binding(adapter, None, crate::inference::planning::MIN_SERVE_CTX),
            prompt_feedback: ArcSwapOption::empty(),
            temperature: DEFAULT_TEMPERATURE,
            tools: Vec::new(),
            native_specs: Vec::new(),
            hands_specs: Vec::new(),
            review_hands_specs: Vec::new(),
            native_command_names: Vec::new(),
            tool_surface_tokens: 0,
            hands_surface_tokens: 0,
            review_hands_surface_tokens: 0,
            working_memory: None,
            prompt_capture: None,
            genome: empty_genome(),
            decoding: relaxed_decoding(),
            working_set: None,
            epoch: parking_lot::Mutex::new(epoch_view::EpochView::default()),
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

    /// Is this turn an ACT on a deliverable, rather than a message turn?
    ///
    /// Asked by two independent concerns — the surface selection (a work turn takes
    /// her hands, for focus) and the ACT output cap — so it lives in one place
    /// rather than being spelled twice and drifting.
    fn is_work_turn(&self, ws: &Workspace) -> bool {
        ws.workspace_deliverable && !self.hands_specs.is_empty()
    }

    /// What kind of turn this is, for its allowance: a work turn is an ACT; anything
    /// else is a PASS.
    fn turn_kind(&self, ws: &Workspace) -> TurnKind {
        if self.is_work_turn(ws) {
            TurnKind::Act
        } else {
            TurnKind::Pass
        }
    }

    /// Set the effective served context window (tokens) this faculty must keep its
    /// prompt within. The live spawn path passes `profile.context_length`
    /// (task #50 — for a Local persona that is the planner's
    /// `ServingPlan.served_context_window`). Default: the runnable floor
    /// [`MIN_SERVE_CTX`](crate::inference::planning::MIN_SERVE_CTX) for a
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
    /// the smallest usable reply provably cannot coexist. Necessary, NOT sufficient —
    /// clearing it does not mean she can hear you. Measured 2026-08-06: bare framing 1419 +
    /// tools 4609, and the newest message in the burst STILL did not survive at 8192,
    /// because the framing a real turn renders (expanded tool categories, perception facts)
    /// exceeds the bare floor and context is what yields. Treat this as "you are definitely
    /// broken below here", never "you are fine above it".
    ///
    /// `fixed prompt cost + reply floor`, derived, never a magic number: the surface is
    /// measured off the specs actually offered, and the reply floor is the same
    /// [`Self::COMPLETION_FLOOR_TOKENS`] the reply's flex claim floors at ([`reply_claim`]).
    /// It used to be `fixed × D/(D-1)` for a reply SHARE of `1/D`; the reply is now a
    /// claimant in the window's one allocation, so its smallest grant is what the bound
    /// must close against, not a fraction.
    pub fn min_window_for_agentic_surface(&self) -> u32 {
        (self.describe_tool_tokens() as u32)
            .saturating_add(self.framing_floor_tokens())
            .saturating_add(Self::COMPLETION_FLOOR_TOKENS)
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

    /// What an UNMEASURED mind's reply wants ([`reply_claim`]'s max): a reply-sized
    /// constant, never a fraction of the window.
    ///
    /// The prior used to be a `window / 2` share, on the reasoning
    /// that the most generous defensible ask is the safe default. Measured on one citizen,
    /// same question, 28 minutes apart (2026-09-18, Cormac, `delib.context.render`):
    ///
    /// | seat | window | reserve | context rendered | answer |
    /// |------|--------|---------|------------------|--------|
    /// | remote 27B | 16,384 | 2,817 (measured) | ~11k | "Tracing the dream/memory-consolidation path through `dream_consolidation.rs`…" |
    /// | home | 32,768 | **16,384** (the share) | **297** | "As an AI assistant, I am currently not actively engaged in any specific project…" |
    ///
    /// A BIGGER window gave her LESS context. Half of it was held back for a reply she
    /// would never write, framing took 2,727 of the remainder, and the 12k of conversation
    /// did not fit at all — she was asked who she is with 297 tokens of world in front of
    /// her, and the stub is the CORRECT answer to that prompt. She was not erased by a weak
    /// model; she was handed nothing.
    ///
    /// The same fraction on the other end of the range is the 5090's 101,120-token window
    /// licensing a 100k think-only turn that wrote nothing. One defect, two faces: the
    /// reserve was sized to the WINDOW instead of to the REPLY. On a big window that is a
    /// licence to spin; on a small one the person does not fit.
    ///
    /// `MIN_SERVE_CTX` and not a new literal, deliberately: the smallest window the serving
    /// stack hands out. It is only the unmeasured WANT of the reply's claim: what no other
    /// claim takes still flows to the reply under its runaway bound, measurement replaces
    /// it with her need, and a turn cut inside its answer records that channel at DOUBLE,
    /// so an act that really needs more room earns it back within a turn.
    const COMPLETION_COLD_PRIOR_TOKENS: u32 = crate::inference::planning::MIN_SERVE_CTX;

    /// The share of the served window the REQUIRED room payload may occupy.
    ///
    /// Room updates are required input — a citizen must see what was said to her —
    /// so `fit_messages` may not drop them the way it drops old history. Unbounded,
    /// that makes a BUSY ROOM into a mute citizen: measured 2026-09-09 on BigMama,
    /// `delib.turn.demand` reported `conversation_tokens=53522` against a 23,040
    /// window (`over_window=3.14`). The required side then exceeded the whole
    /// budget, `grounding_budget` saturated to 0, and `GroundingPlan::minimum`
    /// refused 192 of 229 cognition cycles — 84% — with
    /// "requires 810 tokens, but only 0 are available". Both live citizens were
    /// mute for hours while `serving/status` read `ready: true`.
    ///
    /// The window is not the fault and raising it does not fix this: the room grows
    /// without bound and any fixed window is eventually 3x under. What is bounded
    /// here is what the turn REQUIRES; the rest of the room is still reachable
    /// through history and recall.
    const ROOM_UPDATE_SHARE_DENOM: u32 = 4;
    /// The action result's share of the window before its tail is DEMOTED to ordinary
    /// history. Same denominator as the room payload for the same reason: no single
    /// required item may claim more than a quarter of the window, because the moment one
    /// does, every other required item is competing for what is left and the turn refuses.
    const RESULT_SHARE_DENOM: u32 = 4;

    /// The most an ACT turn may generate — a RUNAWAY bound, not a budget.
    /// Measured acts that COMMITTED a tool call ran 4,188–9,818 tokens on the
    /// 27B (the model thinks before it calls; 2026-09-07 acceptance rows), so
    /// any cap below that faults the act and re-samples it — more decode for
    /// zero committed work (IntelMac's review of #3824). This binds only past
    /// the largest committed act we have seen; the real lever is bounding the
    /// THINKING on act turns, which is card 61b6e54d's next slice.
    pub const ACT_OUTPUT_CAP: u32 = 12_288;

    /// The TIME term's budgets, in seconds, per turn kind — `budget × her measured
    /// decode rate` is a FLOOR the allowance may rise to on a fast lane (the 5090 at
    /// 40 tok/s: an act may run to 6,000 tokens), never the bound. The bound is her
    /// measured reasoning + answer need, under the reserve ([`output_allowance`]). Sized
    /// small on purpose: a floor that is too low costs nothing when the need is
    /// measured and is not consulted when it is not; a bound that is too low ended
    /// turns inside the reasoning channel (the M5, 2026-09-20).
    pub const ACT_LATENCY_BUDGET_SECS: f64 = 150.0;
    pub const PASS_LATENCY_BUDGET_SECS: f64 = 60.0;

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
    const COMPLETION_FLOOR_TOKENS: u32 = crate::inference::planning::MIN_SERVE_CTX / 8;

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
            .map(|c| Self::context_piece_tokens(c.faculty.as_str(), c.content.len()))
            .sum()
    }

    /// Inputs measured at the request boundary, including the selected native
    /// surface, let the render receipt reconcile with the whole-window budget.
    #[cfg(test)]
    fn render_assembled_context_within(
        &self,
        ws: &Workspace,
        budget_tokens: usize,
        plan: &GroundingPlan<'_>,
        composition: ContextComposition,
    ) -> String {
        context_render::render_context_within(&self.persona_name, self.describe_tool_tokens(), ws, budget_tokens, plan, composition).0
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

    /// Does this workspace name a card she HOLDS (Claimed or InProgress)? The lane
    /// gate's key for `LanePriority::Work` — wider than `holds_live_work`, which keys
    /// the working-presence contract on InProgress only.
    /// This mind's next call's bound on the lane: prefill of the prompt she did not have
    /// cached last turn at this box's measured prefill rate, plus her last output at its
    /// measured decode rate. Read from her LAST turn's shape (`LAST_TURN_SHAPE`) — the
    /// prompt is rendered after the gate, so the previous turn is the honest predictor.
    /// `None` when any term is unmeasured: an absence is not a number, and the reserve is
    /// then not lent (`resource_admission::reserve_lendable`).
    fn expected_occupancy(&self) -> Option<std::time::Duration> {
        expected_occupancy_for(self.persona_id)
    }

    /// The bound this turn carries on the wire (`TextGenerationRequest::turn_bound`):
    /// her expected occupancy with headroom (`inference::turn_bound`). Every waiting
    /// seam — the header wait, the stream's queue budget, the remote deadline — takes
    /// `max(its floor, this)`, so a 30k prompt on a 25 tok/s box (~1200 s to the first
    /// byte) is waited out, never read as dead (card ba82d0a0). `None` until her first
    /// measured turn on a measured box: the floors govern alone, an absence is not a
    /// number.
    fn turn_bound(&self) -> Option<std::time::Duration> {
        let expected = self.expected_occupancy();
        let bound = crate::inference::turn_bound::from_expectation(expected);
        if let (Some(expected), Some(bound)) = (expected, bound) {
            crate::probe!(
                class = "delib.turn.bound",
                persona = %self.persona_name,
                expected_secs = expected.as_secs(),
                turn_bound_secs = bound.as_secs(),
                "the turn's wire bound — her measured expectation with headroom"
            );
        }
        bound
    }

    fn holds_work_card(ws: &Workspace) -> bool {
        ws.broadcast
            .iter()
            .filter(|c| c.decision.is_none() && !c.trailing)
            .filter(|c| c.faculty.as_str() == crate::persona::active_work_source::SOURCE_ID)
            .any(|c| crate::persona::active_work_source::renders_held_card(&c.content))
    }

    /// The EXACT prompt this faculty sends the model this tick — the system
    /// prompt (identity + assembled RAG context + how-to-participate + silence
    /// affordance) and the user burst. Exposed so what the LLM sees is trivially
    /// introspectable at every turn (tests, replay, operator tooling). The RAG is
    /// the load-bearing input; it must never be opaque.
    ///
    /// Fits the estimated request to the served window, including selected tool
    /// schemas and the planned completion reserve. Identity framing, complete
    /// current inputs/results, and source-declared minimum standing context are
    /// retained first; insufficient capacity produces an explicit fault. Optional
    /// history then fits by whole, quantized front eviction. Additional source
    /// units and recall use the remaining room. Requiredness is independent of
    /// system/trailing cache placement; this estimate is not a backend token count.
    pub fn prompt_view(&self, ws: &Workspace) -> DeliberationPromptView {
        // Introspection / test entry: size against the CURRENT served window. The
        // production path (`contribute`) instead loads the binding ONCE and calls
        // `prompt_view_within` with that window, so prompt sizing and the request's
        // model come from the same atomic snapshot (no torn read across a re-home).
        let binding = self.binding.load();
        let (window, calibration) = self.prompt_fit(&binding);
        self.prompt_view_with_feedback(ws, window, calibration, binding.requested_model())
    }

    fn prompt_fit(&self, binding: &ModelBinding) -> (u32, PromptCalibration) {
        let feedback = self.prompt_feedback.load();
        feedback.as_ref().filter(|f| f.applies_to(binding)).map_or(
            (binding.context_window, PromptCalibration::default()),
            |f| (binding.context_window.min(f.available), f.calibration),
        )
    }

    /// Fixture view against an explicit window, without a prior provider observation.
    #[cfg(test)]
    fn prompt_view_within(&self, ws: &Workspace, context_window: u32) -> DeliberationPromptView {
        self.prompt_view_with_feedback(ws, context_window, PromptCalibration::default(), self.binding.load().requested_model())
    }

    /// Every reply carries the engine's own count of the prompt it was sent: retain that
    /// count over this turn's estimate as the density the NEXT prompt is fitted with, on
    /// this binding, for any input. Refusal-only calibration never learned the case that
    /// cost Kimi one turn in four on 2026-10-06: the bytes/3 estimate undercounted her
    /// prompt by 18-23% (engine 65,607 against 55,603 estimated), the prompt still FIT
    /// the slot, so nothing was refused, and the reply's reserve was eaten by the prompt
    /// itself: generation hit the slot after ~950 tokens and the turn ended mid-thought.
    /// A ratio below one changes nothing (`charge` never underprices), and each reply
    /// replaces the last, so one unusually dense turn shapes only the turn after it.
    fn learn_prompt_density(
        &self,
        binding: &Arc<ModelBinding>,
        view: &DeliberationPromptView,
        engine_prompt_tokens: u32,
        available: u32,
    ) {
        let Some(calibration) =
            PromptCalibration::observed(view.estimated_prompt_tokens, engine_prompt_tokens, None)
        else {
            return; // the lane reported no count: nothing measured, nothing retained
        };
        self.prompt_feedback.store(Some(Arc::new(PromptFeedback {
            binding: Arc::clone(binding),
            calibration,
            available,
        })));
        crate::probe!(
            class = "delib.prompt.density",
            persona = %self.persona_name,
            estimated_prompt_tokens = view.estimated_prompt_tokens,
            engine_prompt_tokens,
            "the engine's count of this prompt, retained as the next prompt's density"
        );
    }

    /// Apply observed token density to the same standing-grounding plan.
    fn prompt_view_with_feedback(
        &self,
        ws: &Workspace,
        context_window: u32,
        calibration: PromptCalibration,
        model: &str,
    ) -> DeliberationPromptView {
        self.prompt_view_with_visual_feedback(ws, context_window, calibration, None, model)
    }

    fn prompt_view_with_visual_feedback(
        &self,
        ws: &Workspace,
        context_window: u32,
        calibration: PromptCalibration,
        visual: Option<&ActiveVisualFeedback>,
        model: &str,
    ) -> DeliberationPromptView {
        // THE REPLY'S CLAIM ([`reply_claim`]): what she is expected to write, at the
        // priority this turn gives it. It competes in the window's one allocation below;
        // its grant travels with the view into `build_request_within` as `max_tokens`.
        let pending_peak = self.working_memory.as_ref()
            .map_or(0, |wm| wm.pending_emission_peak(ws.activity_room_id));
        let (reply, reply_runaway) = reply_claim(
            self.turn_kind(ws),
            self.working_set.as_ref().and_then(|reg| reg.need_of(self.persona_id)),
            pending_peak,
            Self::holds_live_work(ws),
            ws.directed_at_self(),
        );
        let desired_reply = reply.max_tokens as usize;

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
        // The SELECTED surface, not the whole registry: `contribute` narrows to her
        // hands on a work turn, and subtracting the full registry here trimmed the
        // conversation to fit room that was never occupied (card dec1a7ff). One
        // decision, made once, consulted by the budget and by the request alike.
        let selected = self.select_tool_surface(ws, context_window);

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
        let framing_tokens = Self::framing_cost(
            &self
                .compose_system_split(
                    "",
                    &expanded,
                    ws.directed_at_self(),
                    ws.self_initiated,
                    ws.now_ms,
                    holds_live_work,
                )
                .stable,
        );

        // The volatile framing never rides the system message it invalidated on every act
        // (see the split note below). Its clock is per act (the FACTS phase, before the ask);
        // its own-time and presence half changes only with her posture, so her epoch holds it.
        let clock = deliberation_prompt::trailing_clock(ws.now_ms);
        let mut all_messages = self.messages_with_visual_feedback(ws, clock.as_deref(), visual);
        // Her body as an epoch: the last act's body, then this act's news. Everything below
        // (the window's one allocation, the fit) prices what will actually be sent. Rendered
        // on a copy: only a request the model answered becomes her epoch (`contribute`).
        let mut epoch = self.epoch.lock().clone();
        all_messages.grounding_at = epoch.materialize(
            &mut all_messages.history,
            &all_messages.body,
            ws.activity_room_id,
            Self::message_cost,
            &self.persona_name,
        );
        // STANDING PIECES ARE HER EPOCH'S TOO (the M5, c9bc93951: ~11k tokens of own-time,
        // work-turn, steps, open views and notices re-prefilled after the body every act).
        // Each is a source appended only when it changes, after the fit like the standing
        // grounding; what stays after the body is per act: facts, the clock, an external ask,
        // this turn's room messages and the newest tool exchange.
        let standing = epoch_view::standing_blocks(epoch_view::Standing {
            framing: deliberation_prompt::trailing_standing(ws.self_initiated, ws.directed_at_self(), holds_live_work),
            own_ask: ws.self_initiated.then(|| all_messages.stimulus.take()).flatten().map(|m| m.content_text()),
            ledger: all_messages.ledger.take().map(|m| m.content_text()),
            notices: std::mem::take(&mut all_messages.notices).into_iter().map(|(id, m)| (id, m.content_text())).collect(),
            open_views: self.working_memory.as_ref().and_then(|wm| wm.open_views_items()),
        });
        let standing_unsaid = epoch.unsaid_tokens(&standing, Self::message_cost);
        // The room a turnover leaves: the most her body grew by in one act, and at least the
        // pinned result, which joins the body next act (a cold start's only evidence).
        let headroom = epoch.growth().max(Self::messages_cost(&all_messages.latest_result));
        // A pointer only substitutes when the view turns over (`epoch.turn_over` supplies them).
        all_messages.result_pointers.clear();

        // Price the untruncated trailing sources separately: messages_unfitted
        // leaves them borrowed until the same plan has reserved their exact units.
        let input_identity = all_messages.input_identity;
        let calibration = if calibration
            .input_identity
            .is_some_and(|key| key != input_identity)
        {
            PromptCalibration::default()
        } else {
            calibration
        };
        let unfitted_message_tokens = all_messages.tokens()
            + ws.broadcast
                .iter()
                .filter(|c| c.decision.is_none() && c.trailing && c.standing_grounding)
                .map(|c| GroundingPiece::whole(c).tokens)
                .sum::<usize>();
        // KEEP THE OVERFLOW. This used to be one chained `saturating_sub` ending in
        // `- all_messages.required_tokens()`, and the saturation is where the diagnosis
        // died: "over by 31,000" and "exactly at the line" both arrive as `0`, so the one
        // number that says HOW BADLY the turn is over its window was destroyed at the
        // moment it was computed. Subtract in two steps and probe the remainder.
        let required_payload = all_messages.required_tokens();
        let after_reserves = calibration
            .prompt_budget(
                (context_window as usize).saturating_sub(Self::COMPLETION_FLOOR_TOKENS as usize),
            )
            .saturating_sub(framing_tokens)
            .saturating_sub(selected.tokens);
        let grounding_budget = after_reserves.saturating_sub(required_payload);
        let payload_overflow = required_payload.saturating_sub(after_reserves);
        if payload_overflow > 0 {
            crate::probe!(
                class = "delib.payload.over_window",
                persona = %self.persona_name,
                required_payload,
                after_reserves,
                payload_overflow,
                context_window,
                framing_tokens,
                tool_tokens = selected.tokens,
                "the turn's IRREDUCIBLE payload does not fit the served window — grounding                  gets nothing, and this is the amount by which the window is short"
            );
        }
        // GROUNDING MAY NEVER MUTE A CITIZEN.
        //
        // `GroundingPlan::minimum` used to return `Err` here, and that error was promoted
        // ahead of every other capacity check (`grounding_error.or(capacity_error)`) into a
        // hard `deliberation_fault`. So a citizen whose ENRICHMENT would not fit refused the
        // whole turn — no model call, no act, zero `model_ms` — and, because she keeps her
        // held card while refusing, she can hold a benchmark card forever and never work it.
        //
        // Measured live 2026-09-09 on Kimi, holding `hard-rs/expr_eval`: served window
        // 28,160, conversation 39,532, and every 15 s a `work_turn` that produced
        // `act=0 model_ms=0` with `required 613 > budget 0`. Six hundred and thirteen tokens
        // of roster + workspace-map + working-memory, against a 28k window, decided she
        // could not think at all.
        //
        // Grounding is enrichment. When it does not fit, the honest move is to go without it
        // and think anyway, not to fall silent — a partial world beats a dead mind. Nothing
        // is deleted that the real guards would have kept: the conversation fitter still
        // refuses when the IRREDUCIBLE payload cannot fit (`fit_messages`), and the wire
        // check still refuses when the assembled request exceeds the window. Only the
        // enrichment's own shortfall stops being fatal.
        let mut grounding = match GroundingPlan::within_budget(ws, grounding_budget) {
            Fitted::Whole(plan) => plan,
            Fitted::Reduced { plan, wanted } => {
                crate::probe!(
                    class = "delib.grounding.shed",
                    persona = %self.persona_name,
                    wanted_tokens = wanted,
                    budget_tokens = grounding_budget,
                    kept_tokens = plan.system_reserve() + plan.trailing_tokens(),
                    "standing grounding did not fit its budget — thinking with less                      enrichment rather than refusing the turn"
                );
                plan
            }
        };
        // Everything appended after the fit is required input: the standing grounding and the
        // standing pieces her epoch does not hold yet.
        all_messages.grounding_tokens = grounding.trailing_tokens() + standing_unsaid;
        let ctx_floor = if grounding.pieces.is_empty() {
            // Preserve the existing optional-enrichment allowance when no standing
            // source bid. This soft floor confers no requiredness on recall or on
            // cache placement; standing sources above are never capped by a ratio.
            let offered = || {
                ws.broadcast
                    .iter()
                    .filter(|c| c.decision.is_none() && !c.trailing)
            };
            let cost =
                |c: &Contribution| Self::context_piece_tokens(c.faculty.as_str(), c.content.len());
            // Against the reply's FLOOR: its desire is negotiated below, in the same
            // allocation as this floor, never pre-charged here.
            let after_framing = calibration
                .prompt_budget((context_window as usize).saturating_sub(reply.floor_tokens as usize))
                .saturating_sub(selected.tokens)
                .saturating_sub(framing_tokens);
            offered()
                .find(|c| c.faculty.as_str() == crate::persona::active_work_source::SOURCE_ID)
                .map(cost)
                .or_else(|| offered().map(cost).min())
                .map(|tokens| tokens + Self::working_context_header_cost())
                .unwrap_or(0) // No optional source bid means no enrichment reservation.
                .min(after_framing / 2)
        } else {
            grounding.system_reserve()
        };

        // ONE ALLOCATION FOR THE WHOLE WINDOW. Owner's rule: "Flexbox is the right way.
        // It's like the governor: if you don't use it for literally everything it doesn't
        // work." Every claimant on the served window bids in engine-calibrated tokens —
        // framing + the selected tools and the required input (required, never traded),
        // standing grounding, optional history, and the REPLY — and the one flexbox
        // algorithm decides ([`crate::persona::rag_budget::flex_allocate`]). The reply was
        // carved off the top by its own rule and then yielded in two steps; on a writing
        // turn that cut Benchy's first write mid-call (2026-10-18 02:10Z). Now it trades
        // against history and grounding by priority and never takes from required input.
        //
        // HER LAST MOVE IS HISTORY'S FLOOR: re-issuing an act she already ran costs a whole
        // extra turn, so what her own last move and its outcome cost comes ahead of every
        // optional claim, the reply's desire included; `fit_messages` refuses to evict it.
        // History's ceiling is the window: what a turn may cost in prefill TIME is a bound on
        // what this act adds to her epoch (below), never on the epoch she already holds.
        let evidence = all_messages.recent_move_evidence();
        let required_tokens = all_messages.required_tokens();
        let optional_history = all_messages.tokens().saturating_sub(required_tokens);
        // A quiet self-tick (no required input) still sends her newest whole message.
        let quiet_minimum = if required_tokens == 0 {
            all_messages.history.last().map_or(0, Self::message_cost) // map_or: no history = nothing to keep
        } else {
            0
        };
        let history_floor = evidence.tokens.max(quiet_minimum).min(optional_history);
        let claim = |tokens: usize| calibration.charge(tokens).min(u32::MAX as usize) as u32;
        let fixed_cost = claim(framing_tokens.saturating_add(selected.tokens));
        let required_cost = claim(required_tokens);
        let mut reply = reply;
        // The reply never takes from required content: its floor is what the window leaves
        // after framing, tools and the required input. Its UNCLIPPED floor is her need.
        let reply_need = reply.floor_tokens;
        reply.floor_tokens = reply_need.min(context_window.saturating_sub(fixed_cost.saturating_add(required_cost)));
        reply.min_tokens = reply.floor_tokens;
        let fixed_claim = |id: &str, tokens: u32| crate::persona::rag_budget::RagSourceBudget {
            source_id: id.to_owned(),
            priority: 10,
            floor_tokens: tokens,
            min_tokens: tokens,
            max_tokens: tokens,
            required: true,
        };
        let grounding_floor = claim(ctx_floor);
        let history_floor = claim(history_floor);
        let claims = [
            fixed_claim(FLEX_FRAMING, fixed_cost),
            fixed_claim(FLEX_REQUIRED, required_cost),
            match epoch.head_grant() {
                // A bound head claims exactly the grant it was admitted with: what it wants
                // beyond that is the next epoch's to decide, never the body's room to take.
                Some(grant) => fixed_claim(FLEX_GROUNDING, claim(grant)),
                None => crate::persona::rag_budget::RagSourceBudget {
                    source_id: FLEX_GROUNDING.to_owned(),
                    priority: 6,
                    floor_tokens: grounding_floor,
                    min_tokens: grounding_floor,
                    // What grounding wants: every offered context piece and, when there is
                    // one, its header. Nothing offered wants nothing.
                    max_tokens: claim(
                        match Self::assembled_context_cost(ws) {
                            0 => 0,
                            offered => offered.saturating_add(Self::working_context_header_cost()),
                        }
                        .max(ctx_floor),
                    ),
                    required: false,
                },
            },
            crate::persona::rag_budget::RagSourceBudget {
                source_id: FLEX_HISTORY.to_owned(),
                priority: 5,
                floor_tokens: history_floor,
                min_tokens: history_floor,
                max_tokens: claim(optional_history).max(history_floor),
                required: false,
            },
            reply,
        ];
        let plan = crate::persona::rag_budget::flex_allocate(context_window, &claims);
        let budget_floor = super::window_allocator::turn_need(&claims).saturating_sub(claims[4].floor_tokens).saturating_add(reply_need);
        // Whatever no claim took is hers to write with, up to the runaway bound.
        let reply_grant = plan.granted(FLEX_REPLY);
        let completion_reserve = reply_grant
            .saturating_add(plan.unallocated)
            .min(reply_runaway)
            .max(1) as usize;
        crate::probe!(
            class = "delib.window.flex",
            persona = %self.persona_name,
            context_window,
            framing_tokens = fixed_cost,
            required_tokens = required_cost,
            grounding_granted = plan.granted(FLEX_GROUNDING),
            history_granted = plan.granted(FLEX_HISTORY),
            reply_priority = claims[4].priority,
            reply_floor = claims[4].floor_tokens,
            reply_wanted = claims[4].max_tokens,
            reply_granted = reply_grant,
            unallocated = plan.unallocated,
            reply_runaway,
            allowance = completion_reserve,
            escalation = plan.escalation_needed,
            "the served window's one allocation — framing, required input, grounding, \
             history and the reply by priority; the allowance is the reply's grant plus \
             what no claim took, under its runaway bound"
        );
        // Grounding is optional: when its floor did not fit, it holds no room the
        // required input then has to do without.
        let ctx_floor = if plan.granted(FLEX_GROUNDING) < grounding_floor { 0 } else { ctx_floor };
        let after_framing = calibration
            .prompt_budget((context_window as usize).saturating_sub(completion_reserve))
            .saturating_sub(selected.tokens)
            .saturating_sub(framing_tokens);
        let window_msg_budget = after_framing.saturating_sub(ctx_floor);
        // History gets its grant; the required input is never part of the trade.
        let msg_budget = window_msg_budget.min(
            required_tokens
                .saturating_add(calibration.prompt_budget(plan.granted(FLEX_HISTORY) as usize)),
        );
        // A bound head keeps the grant it was admitted with; the body has what is left of it.
        let msg_budget = match epoch.head_grant() {
            Some(grant) => msg_budget.min(after_framing.saturating_sub(grant)),
            None => msg_budget,
        };

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
        let demand_tokens = calibration
            .charge(
                framing_tokens
                    .saturating_add(unfitted_message_tokens)
                    .saturating_add(Self::assembled_context_cost(ws))
                    .saturating_add(Self::working_context_header_cost())
                    // Demand includes the full registry only when capacity withheld it.
                    .saturating_add(selected.demand_tokens(self.describe_tool_tokens())),
            )
            // Output is already measured in provider tokens; calibrate only input.
            .saturating_add(desired_reply);
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
            conversation_tokens = unfitted_message_tokens,
            grounding_tokens = Self::assembled_context_cost(ws),
            completion_reserve,
            // The honest headline: >1.0 means this turn wanted more window than it had,
            // and by how much. This is the number that decides whether the served
            // window is provisioned for the work or for a constant.
            over_window = demand_tokens as f32 / (context_window.max(1) as f32),
            "turn demand vs served window"
        );

        let protocol_error = all_messages.active_protocol_error();
        let start_room = super::context_budget::ContextBudget::fresh_prefill_tokens(crate::inference::prefill_rate::measured_rate_for(model), context_window, crate::cognition::audience::current()).saturating_sub(selected.tokens + framing_tokens);
        let (room, fit) = match epoch.turnover_for(all_messages.tokens(), msg_budget, start_room) {
            None => (after_framing, self.fit_messages(all_messages, msg_budget)),
            Some(cause) => {
                let budget = all_messages.restart_epoch(&mut epoch, cause, &standing, standing_unsaid, headroom, start_room.min(msg_budget), &self.persona_name);
                let fit = self.fit_messages(all_messages, budget);
                if let Ok(f) = &fit { epoch.keep(f.receipt.dropped_messages, f.messages.iter().take(f.grounding_at)); }
                (after_framing.min(start_room), fit)
            }
        };
        let (mut fitted, capacity_error) = match fit {
            Ok(fitted) => (fitted, None),
            Err(error) => (FittedMessages::default(), Some(error)),
        };
        // THE FIT'S RECEIPT (card d33e928a, Kimi's finding): whenever the fit dropped
        // history, say what it dropped and from what budget, and say it loudly when
        // nothing optional survived — that turn composes with no room at all, operator
        // direction included, and before this there was no line to read it from.
        if capacity_error.is_none() && fitted.receipt.dropped_messages > 0 {
            let r = fitted.receipt;
            crate::probe!(
                class = if r.emptied() { "delib.fill.history_emptied" } else { "delib.fill.history_dropped" },
                persona = %self.persona_name,
                context_window,
                completion_reserve,
                msg_budget,
                required_tokens = r.required_tokens,
                history_budget = r.history_budget,
                history_messages = r.history_messages,
                history_tokens = r.history_tokens,
                kept_messages = r.kept_messages,
                dropped_messages = r.dropped_messages,
                dropped_tokens = r.dropped_tokens,
                headroom = r.headroom,
                "the fit dropped optional history from the front; history_emptied means nothing \
                 optional survived and the turn composes with no room messages at all"
            );
        }
        // WHAT THIS TURN ACTUALLY SENDS — the post-fit size the served window must
        // hold. Recorded beside the untrimmed demand above: that one is the growth
        // signal (and the upper bound), this one is what a slot must FIT. The planner
        // sizes the window from this with headroom (`ServingDemand::window_target`),
        // so a 25k working set no longer provisions a 137k slot (2026-09-13).
        if let Some(reg) = &self.working_set {
            let sent = framing_tokens
                .saturating_add(Self::messages_cost(&fitted.messages))
                .saturating_add(desired_reply);
            // At a window that TRIMMED this turn the post-fit size is the window's
            // echo, not the turn's — the untrimmed demand is what the plan must hear
            // (`sent_sample_for_plan`; the 5090's self-sealed 2048, 2026-09-20).
            let sample = crate::cognition::working_set::WorkingSetRegistry::sent_sample_for_plan(
                demand_tokens.min(u32::MAX as usize) as u32,
                sent.min(u32::MAX as usize) as u32,
                context_window,
            );
            reg.record_sent(
                self.persona_id,
                sample,
                ws.now_ms.unwrap_or(0), // JUSTIFIED unwrap_or: unstamped cycle still measures honestly
            );
            // HER NEED, the allocator's gate (card 70706a9e): the BUDGETER'S FLOOR on a held
            // turn, never its post-fit size, which the flexbox grows to any window (the M5,
            // 10-18: ~48.6k on a 102k lane refused a second lane; 4 residents on 1 slot).
            let sent_tokens = sent.min(u32::MAX as usize) as u32;
            let framing = framing_tokens.min(u32::MAX as usize) as u32;
            let fit_ok = capacity_error.is_none();
            match crate::cognition::working_set::need_sample(sent_tokens, framing, fit_ok).map(|_| budget_floor) {
                Some(need) => reg.record_need(
                    self.persona_id,
                    need,
                    ws.now_ms.unwrap_or(0), // JUSTIFIED unwrap_or: unstamped cycle still measures honestly
                ),
                // THE UNHELD TURN IS SAID, not skipped (card 39822816): a mind the
                // allocator labels `wish:` after a deploy carrying #4325 must be
                // distinguishable, from the ledger alone, between "every turn was
                // starved", "every turn emptied to the framing", and "this seam was
                // never reached". One row per unheld turn, with the reason.
                None => crate::probe!(
                    class = "persona.need.unheld",
                    persona = %self.persona_name,
                    sent_tokens,
                    framing_tokens = framing,
                    fit_ok,
                    reason = if fit_ok { "framing_only" } else { "starved" },
                    "this turn measured no need — not held, so the wish stays her gate"
                ),
            }
        }
        // Only the source's truthful minimum outranks optional conversation.
        // Additional declared list units use actual leftover room after fitting;
        // a large list cannot turn every detail into mandatory input.
        // A standing source keeps the size her epoch admitted it at while it says the same thing
        // (never re-rendered because the room left over changed); a new or changed one is
        // admitted from the room left after the measured growth.
        if capacity_error.is_none() {
            let held_room = room
                .saturating_sub(Self::messages_cost(&fitted.messages))
                .saturating_sub(epoch.unsaid_tokens(&standing, Self::message_cost));
            grounding.expand_within(held_room.saturating_sub(headroom), held_room, |c| epoch.admitted(c.faculty.as_str(), epoch_view::content_fingerprint(&c.content, &c.parts)));
            for piece in &grounding.pieces {
                let c = piece.contribution;
                epoch.admit(c.faculty.as_str(), epoch_view::content_fingerprint(&c.content, &c.parts), piece.units);
            }
        }
        // Standing pieces are priced after the fit; their news joins the epoch's end. A
        // refused view is never sent, so it leaves her epoch untouched.
        let mut trailing_grounding = epoch.append(&standing, Self::message_cost);
        trailing_grounding.extend(epoch.append(&grounding.trailing_blocks(), Self::message_cost));
        fitted.refit.grounding_len = trailing_grounding.len();
        fitted.messages.splice(
            fitted.grounding_at..fitted.grounding_at,
            trailing_grounding,
        );
        let messages = fitted.messages;

        // Whatever remains after framing + conversation goes to enrichment
        // context. The framing estimate above was taken with an EMPTY context,
        // where `working_context_block`'s wrapper header is absent — so the
        // moment any context renders, the system prompt grows by that header
        // too. Charge the context budget for it up front, or the final prompt
        // systematically exceeds the estimate by ~50 tokens (masked by
        // rounding slop until the tool-menu example grew the prompt to the
        // budget edge — glass-boxed 2026-07-13, llama-server 400 territory).
        let used_msg_tokens = Self::messages_cost(&messages);
        // Grounding gets everything the conversation did not actually use — never
        // less than the floor reserved above.
        // Bound to her epoch, the head renders inside the grant it was admitted with, so the
        // body's growth never re-renders it; a fresh head takes what the body and its measured
        // growth leave.
        let ctx_budget = match epoch.head_grant() {
            Some(grant) => grant.saturating_sub(Self::working_context_header_cost()),
            None => room
                .saturating_sub(used_msg_tokens)
                .saturating_sub(headroom)
                .saturating_sub(Self::working_context_header_cost())
                .max(ctx_floor.saturating_sub(Self::working_context_header_cost())),
        };
        let (context, admitted_tokens) = context_render::render_context_within(
            &self.persona_name,
            self.describe_tool_tokens(),
            ws,
            ctx_budget,
            &grounding,
            ContextComposition {
                context_window,
                framing_tokens,
                conversation_tokens: used_msg_tokens,
                ctx_floor,
                after_framing,
                tool_tokens: selected.tokens,
                surface_reason: selected.reason,
            },
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
                ws.directed_at_self(),
                ws.self_initiated,
                ws.now_ms,
                holds_live_work,
            )
            .stable;
        epoch.bind_head(
            epoch_view::Head {
                system: system.as_str().into(),
                tools: epoch_view::tools_fingerprint(
                    selected.specs.iter().flat_map(|specs| specs.iter().map(|spec| spec.name.as_str())),
                    selected.tokens,
                ),
                model: model.into(),
                grant: if context.is_empty() { 0 } else { admitted_tokens + Self::working_context_header_cost() },
            },
            &self.persona_name,
        );
        // This is the native request estimate, not a backend tokenizer count.
        // Adapters may add model-specific framing; live acceptance still checks
        // their captured request/usage rather than assuming this estimate exact.
        let estimated_prompt_tokens = Self::framing_cost(&system)
            .saturating_add(Self::messages_cost(&messages))
            .saturating_add(selected.tokens);
        let wire_tokens = calibration
            .charge(estimated_prompt_tokens)
            .saturating_add(completion_reserve);
        let capacity_error = capacity_error.or_else(|| {
            (wire_tokens > context_window as usize).then_some(PromptCapacityError {
                required_tokens: wire_tokens,
                budget_tokens: context_window as usize,
            })
        });
        let segments = segment_map(&system, &messages);
        DeliberationPromptView {
            refit: fitted.refit,
            system,
            input_identity,
            estimated_prompt_tokens,
            completion_reserve: completion_reserve as u32,
            capacity_error,
            protocol_error,
            messages,
            segments,
            epoch: Some(epoch),
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

    /// The canonical render before her epoch and the fit: what every source offers this act,
    /// for tests that inspect the sources rather than what is sent.
    #[cfg(test)]
    fn messages_unfitted(&self, ws: &Workspace, trailing_framing: Option<&str>) -> PromptMessages {
        self.messages_with_visual_feedback(ws, trailing_framing, None)
    }

    fn messages_with_visual_feedback(
        &self,
        ws: &Workspace,
        trailing_framing: Option<&str>,
        visual: Option<&ActiveVisualFeedback>,
    ) -> PromptMessages {
        // Required-input cache identity, not provider wire provenance. Hash
        // borrowed payload once, never JSON, optional history, or generated
        // clock/cycle/diagnostic framing. The existing source-defined minimum
        // keeps large optional lists out of this path; do not hash content and
        // its duplicate parts. Model/window and schema rebuilds key the owner.
        let mut input = Sha256::new();
        let field = |hash: &mut Sha256, bytes: &[u8]| {
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        };
        field(&mut input, self.system_prompt.as_bytes());
        field(&mut input, ws.room_id.as_bytes());
        let standing = || {
            ws.broadcast
                .iter()
                .filter(|c| c.decision.is_none() && c.fault.is_none() && c.standing_grounding)
        };
        input.update(b"standing");
        input.update((standing().count() as u64).to_le_bytes());
        for c in standing() {
            field(&mut input, c.faculty.as_str().as_bytes());
            let piece = GroundingPiece::minimum(c);
            match piece.units {
                None => field(&mut input, c.content.as_bytes()),
                Some(units) => {
                    for part in c.parts.iter().take(units) {
                        field(&mut input, part.as_bytes());
                    }
                }
            }
        }
        input.update(b"room_updates");
        input.update((ws.room_updates.len() as u64).to_le_bytes());
        for update in ws.room_updates.iter() {
            field(&mut input, update.peer_id.as_bytes());
            field(&mut input, update.room_id.as_bytes());
            field(&mut input, update.text.as_bytes());
        }
        // Coalesce consecutive same-role turns in chronological order. Content
        // similarity cannot establish event identity: "tests passed" and "tests
        // failed" can be near-identical, and a later identical statement can
        // report another change of state. Preserve that history for the citizen;
        // the existing fitter bounds it and perception facts describe repetition.
        let mut groups: Vec<(&'static str, Vec<(SourceId, String)>)> = Vec::new();
        // Preserve the newest external SPEECH as activity context, even when an
        // own receipt or perception fact follows it. This says what was heard,
        // never what the citizen must obey. Keep the typed boundary before
        // same-role coalescing can bury it inside room history (f09424d4).
        // Source-item/ambient causal provenance remains card 27fdf307; do not
        // infer an instruction or a work assignment from these bytes.
        let stimulus_index = ws
            .turns
            .iter()
            .rposition(|turn| !turn.is_self && turn.voice == TurnVoice::Speech);
        let mut stimulus = None;
        let mut after_stimulus = false;
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
        input.update(b"stimulus");
        input.update([stimulus_index.is_some() as u8]);
        // The system's own words to her (`TurnVoice::Perception`: a [pattern] notice, a
        // board anchor) are perception facts, not conversation. In the conversation they
        // sat AHEAD of the append-only results ring, so each new notice displaced the ring
        // and everything behind it: Benchy's engine reuse fell to the system prompt alone
        // (7,441 tokens) the turn a `[pattern]` notice landed (the M5, 2026-10-18 16:16Z).
        // They render with the facts, after every append-only block, before the ask.
        let mut perceived: Vec<String> = Vec::new();
        let turn_sources = epoch_view::turn_sources(&ws.turns);
        for ((index, turn), source) in ws.turns.iter().enumerate().zip(turn_sources) {
            let role = if turn.is_self { "assistant" } else { "user" };
            let line = turn_message_line_addressed(turn, &participants, &self.persona_name);
            if turn.voice == TurnVoice::Perception && Some(index) != stimulus_index {
                // A fact, like every other fact: not part of the request's input identity, so
                // diagnostic churn never reads as new input to a retained refusal.
                perceived.push(line);
                continue;
            }
            if Some(index) == stimulus_index {
                // The event time and addressed author are required provenance too.
                field(&mut input, line.as_bytes());
                stimulus = Some(ChatMessage::text(role, line));
                after_stimulus = true;
                continue;
            }
            match groups.last_mut() {
                Some((r, lines)) if *r == role && !after_stimulus => lines.push((source, line)),
                _ => groups.push((role, vec![(source, line)])),
            }
            after_stimulus = false;
        }
        // The body: every source that only grows or changes rarely, keyed so her epoch
        // (`epoch_view`) appends only what is new. Conversation, then the results ring, then
        // the trailing grounding; facts, the clock and the ask follow it every act.
        let mut body: Vec<epoch_view::Block> =
            groups.into_iter().map(|(role, items)| epoch_view::Block::turns(role, items)).collect();

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
            room_id: Some(ws.activity_room_id),
            persona_id: Some(self.persona_id),
        };
        let mut ledger = Vec::new();
        let mut facts = perceived;
        let mut notices = Vec::new();
        for fact in super::perception_facts::render_facts(
            &fact_cx,
            &super::perception_facts::FactPolicy::default(),
        ) {
            use super::perception_facts::FactPlacement;
            match fact.placement {
                FactPlacement::History => facts.push(fact.text),
                FactPlacement::Ledger => ledger.push(fact.text),
                FactPlacement::Notice => notices.push((fact.id, ChatMessage::text("user", fact.text))),
            }
        }
        // The required tail (ask, pinned newest result) is assembled by the fit. Facts stay
        // BEFORE the ask (the 2026-07-20 parrot-loop bisect); the pinned result stays nearest
        // generation (#392); trailing grounding stays out of the system prefix (#205/#2415).
        //
        // THE BODY IS AN EPOCH (docs/architecture/MIND-CONTEXT-AND-ENGINE-CONFIG.md). The
        // order inside it was flipped four times (2026-08-23, 08-24 twice, 10-18) chasing
        // byte-stability between re-renders; each order moved the prefix somewhere else. The
        // epoch appends this act's news after the last act's body instead, so the order here
        // is only the order of a fresh epoch. Saved results carry their compact pointer for
        // the fit; the active result is pinned below, not here.
        if let Some(wm) = &self.working_memory {
            body.extend(wm.recent_results_with_pointers().into_iter().map(epoch_view::Block::from));
        }
        // Trailing proprioception (#205) and trailing grounding: the working-memory trail
        // last, as the churniest standing block.
        let trailing = |wm_trail: bool| {
            ws.broadcast
                .iter()
                .filter(|c| c.decision.is_none() && c.trailing && !c.standing_grounding)
                .filter(move |c| {
                    (c.faculty.as_str() == crate::cognition::working_memory::WM_FACULTY_ID) == wm_trail
                })
                .filter(|c| !c.content.trim().is_empty())
                .map(|c| (c.faculty.as_str(), c.content.as_str()))
        };
        body.extend(epoch_view::contribution_blocks(trailing(false)));
        body.extend(epoch_view::contribution_blocks(trailing(true)));
        let result_pointers = body
            .iter()
            .enumerate()
            .filter_map(|(index, block)| block.alternative().map(|p| (index, ChatMessage::text("user", p))))
            .collect();
        let grounding_at = body.len();
        let mut messages: Vec<ChatMessage> = body.iter().map(epoch_view::Block::message).collect();
        // Facts flicker per act: after the body, immediately before the ask (parrot-fix
        // order). The volatile framing (the [now] clock, the DIRECTED/SILENCE flip) is the
        // same class: in the system message it invalidated the KV prefix on nearly every act
        // (hit_rate 0.0, 2026-09-01).
        for fact in facts {
            messages.push(ChatMessage::text("user", fact));
        }
        if let Some(framing) = trailing_framing {
            messages.push(ChatMessage::text("user", framing.to_string()));
        }
        // PINNED just-executed result (#392): read straight from working memory, never a
        // faculty bid that attention could evict (the 0-byte SWE-bench patch, run-18057-f1).
        // Use the actual received action payload. The old pinned block clipped
        // its head before this fitter clipped its tail: the two cuts could leave
        // only a truncation notice. Oversized output paging belongs to the
        // existing spill/tool-output producer, never to a generic byte reducer.
        input.update(b"active_result");
        let (latest_result, demoted_result) = match self.working_memory.as_ref() {
            None => (Vec::new(), None),
            Some(wm) => match wm.active_action_full() {
                None => (Vec::new(), None),
                Some((seq, full)) => {
                    let message = wm.full_result_message(seq, &full);
                    // Provenance remains part of request identity. Typed calls and
                    // producer payloads below additionally identify the native exchange.
                    field(&mut input, message.as_bytes());
                    let observations = wm.active_action_observations(seq);
                    let calls = wm.active_action_native_calls(seq);
                    let (mut group, tail) = if calls.is_empty() {
                        // Legacy receipts have no native IDs to echo.
                        let (head, tail) = self.split_action_result(message);
                        (vec![head], tail)
                    } else {
                        let results: Vec<_> = observations.iter().map(|o| o.output.result.clone()).collect();
                        for observation in &observations {
                            field(&mut input, observation.output.result.tool_use_id.as_bytes());
                            field(&mut input, observation.output.result.content.as_bytes());
                            input.update([match observation.output.result.is_error { None => 0, Some(false) => 1, Some(true) => 2 }]);
                        }
                        for call in &calls {
                            field(&mut input, call.id.as_bytes());
                            field(&mut input, call.name.as_bytes());
                            field(&mut input, call.input.to_string().as_bytes());
                        }
                        let calls = ChatMessage::assistant_tool_use(&calls);
                        let mut results = ChatMessage::tool_results(&results);
                        // Preserve producer-owned whole-or-pointer payloads, never expand
                        // spill handles or reconstruct native protocol from receipt prose.
                        if let crate::ai::types::MessageContent::Parts(parts) = &mut results.content {
                            parts.push(crate::ai::types::ContentPart::Text { text: wm.full_result_message(seq, "") });

                        }
                        // This entire required group survives fitting or faults together.
                        (vec![calls, results], None)
                    };
                    if let Some(visual) = visual.filter(|v| v.seq == seq && !observations.is_empty()) {
                        // Legacy checkpoints can still own pixels without original
                        // native call provenance. Preserve either representation.
                        if let Some(result) = group.last_mut() {
                            let mut parts = match std::mem::replace(&mut result.content, crate::ai::types::MessageContent::Parts(Vec::new())) {
                                crate::ai::types::MessageContent::Text(text) => vec![crate::ai::types::ContentPart::Text { text }],
                                crate::ai::types::MessageContent::Parts(parts) => parts,
                            };
                            for (hash, part) in &visual.parts {
                                field(&mut input, hash.as_bytes());
                                match part {
                                    crate::ai::types::ContentPart::Text { text } => field(&mut input, text.as_bytes()),
                                    _ => field(&mut input, b"native-image"),
                                }
                                parts.push(part.clone());
                            }
                            result.content = crate::ai::types::MessageContent::Parts(parts);
                        }
                    }
                    (group, tail)
                }
            },
        };

        // An empty conversation is a legitimate state (a quiet room on a
        // self-initiated tick): the situation lives in the system prompt's assembled
        // context, not in a conversation turn. Adapters still require ≥1 message, so
        // emit the rendered world-state projection as one user message — the exact
        // single-message shape the faculty sent before this refactor. NOT a fallback
        // hiding a defect ([[fallbacks-are-illegal-fail-loud]]): zero turns is a
        // real, valid input, and this is its faithful representation.
        if messages.is_empty() && stimulus.is_none() && latest_result.is_empty() {
            field(&mut input, ws.world_state.as_bytes());
            stimulus = Some(ChatMessage::text("user", ws.world_state.clone()));
        }
        input.update([(!latest_result.is_empty()) as u8]);
        // Demoted room updates ride in ordinary history: still shown when the window
        // has room, trimmed by the existing front-drop only under pressure. They are
        // chronologically newer than prior history, so they append to its end.
        let (room_updates, demoted_updates) = self.split_room_updates(ws);
        let mut messages = messages;
        // The action result's demoted TAIL rides the same way, and goes in FIRST: it is
        // the outcome of the act that preceded these room updates, so appending it before
        // them keeps history in the order it happened. Without this the tail would be
        // silently dropped, which is the discarding this whole split exists to avoid.
        messages.extend(demoted_result);
        messages.extend(demoted_updates);
        let action_statement = self
            .working_memory
            .as_ref()
            .and_then(|memory| memory.action_statement(ws.activity_room_id))
            .map(|text| ChatMessage::text("assistant", text));
        if let Some(statement) = &action_statement {
            field(&mut input, statement.content_text().as_bytes());
        }
        PromptMessages {
            input_identity: input.finalize().into(),
            history: messages,
            body,
            headroom: 0,
            result_pointers,
            grounding_tokens: 0,
            grounding_at,
            stimulus,
            latest_result,
            action_statement,
            room_updates,
            notices,
            ledger: ledger.into_iter().next().map(|text| ChatMessage::text("user", text)),
        }
    }

    /// Split this turn's room updates into the part that is REQUIRED and the part
    /// that merely wants to be there. Nothing is discarded.
    ///
    /// Room updates are required input — `required_tokens` counts them and
    /// `fit_messages` never drops them — so an unbounded room is a citizen who can
    /// no longer think. But simply CUTTING the excess would be worse than the bug:
    /// a task, a correction, or an instruction that arrived earlier in the batch
    /// would be deleted before she ever read it, and she would have no way to know
    /// something was taken.
    ///
    /// So the overflow is DEMOTED, not dropped. The newest share stays required; the
    /// older remainder is returned to be appended to ordinary history, where it is
    /// still shown whenever the window has room and is trimmed by the existing
    /// front-drop only under real pressure — the same treatment every other past
    /// message gets. The information survives; only its claim on the required budget
    /// does not.
    ///
    /// Returns `(required_newest, demoted_older)`, both chronological.
    /// Split the action result into the REQUIRED head and a DEMOTED tail.
    ///
    /// The result of the citizen's last act is required input — she cannot reason about
    /// what her hands just did if it is dropped — and it was also UNBOUNDED. A single
    /// `code/read` or `code/shell` output therefore claimed the whole window as
    /// irreducible payload, and `fit_messages` refused the turn, and she went mute while
    /// holding her card.
    ///
    /// Measured 2026-09-10 on a 23,040-token lane: `required_payload = 18,629` against
    /// `after_reserves = 15,688`, over by 2,941, with `grounding_tokens = 113` proving
    /// grounding innocent and `received = 2` proving it was ONE enormous message. 836
    /// consecutive refusals, zero acts, zero inference, for the whole of a benchmark
    /// round. The build had already bounded room updates (#3947) and stopped grounding
    /// vetoing the turn (#3954) — this was the same class, one field over.
    ///
    /// The code here used to say oversized output "belongs to the existing
    /// spill/tool-output producer, never to a generic byte reducer". That is a good rule
    /// about WHO SHOULD SHRINK IT, and it was enforced nowhere: nothing checked that the
    /// producer had, so an unspilled result walked straight into required input.
    ///
    /// DEMOTED, never discarded (Joel, 2026-09-09: "maybe trim clipped or killed
    /// important information the persona needed to complete things"). The head stays
    /// required so the act's outcome — status, error, the first lines that say what
    /// happened — is always in front of her; the tail becomes ordinary history, still
    /// rendered whenever the window allows, and still re-readable through its spill
    /// handle. A cut on a CHAR boundary would split a UTF-8 sequence, so the split walks
    /// to a line boundary and says how much moved.
    fn split_action_result(&self, message: String) -> (ChatMessage, Option<ChatMessage>) {
        let window = self.binding.load().context_window as usize;
        let cap = window / Self::RESULT_SHARE_DENOM as usize;
        let whole = ChatMessage::text("user", message.clone());
        if Self::messages_cost(std::slice::from_ref(&whole)) <= cap {
            return (whole, None);
        }
        // WHICH HALF IS REQUIRED IS DECIDED BY WIRE ORDER, not by preference.
        //
        // History renders BEFORE the required slot (`prompt.history.extend(latest_result)`
        // appends the required part last). So keeping the HEAD required would place the
        // result's beginning AFTER its middle on the wire — she would read it backwards.
        // Keeping the TAIL required puts the demoted beginning in history and the ending
        // in the required slot, which is the original order, and it keeps the payload
        // ending byte-for-byte where the existing wire test expects it.
        //
        // Walk lines from the END while the tail still fits its share.
        let lines: Vec<&str> = message.split_inclusive('\n').collect();
        let mut take = 0usize;
        let mut tail = String::new();
        for line in lines.iter().rev() {
            let candidate = format!("{line}{tail}");
            let cost = Self::messages_cost(std::slice::from_ref(&ChatMessage::text(
                "user",
                candidate.clone(),
            )));
            // The LAST line is always required even if it alone exceeds the share: a
            // required slot holding none of her result tells her nothing about what her
            // hands just did, and a genuinely impossible payload is the existing capacity
            // fault's job to report honestly.
            if take == 0 || cost <= cap {
                tail = candidate;
                take += 1;
            } else {
                break;
            }
        }
        if take >= lines.len() {
            return (whole, None);
        }
        let head: String = lines[..lines.len() - take].concat();
        let head_msg = ChatMessage::text("user", head);
        let tail_msg = ChatMessage::text("user", tail);
        crate::probe!(
            class = "delib.action_result.demoted",
            persona = %self.persona_name,
            window = window,
            cap = cap,
            required_tokens = Self::messages_cost(std::slice::from_ref(&tail_msg)),
            demoted_tokens = Self::messages_cost(std::slice::from_ref(&head_msg)),
            "the action result exceeded its share of the window — its ending stays              REQUIRED and the earlier part was demoted to ordinary history, still rendered              when the window allows and still re-readable via its spill handle. Nothing              was discarded: an unbounded required result is what makes one big tool output              a mute citizen."
        );
        (tail_msg, Some(head_msg))
    }

    fn split_room_updates(&self, ws: &Workspace) -> (Vec<ChatMessage>, Vec<ChatMessage>) {
        let window = self.binding.load().context_window as usize;
        let cap = window / Self::ROOM_UPDATE_SHARE_DENOM as usize;
        let mut required: Vec<ChatMessage> = Vec::new();
        let mut demoted: Vec<ChatMessage> = Vec::new();
        let mut used = 0usize;
        // Newest first so the REQUIRED half is the freshest room state.
        for message in ws.room_updates.iter().rev() {
            let rendered = ChatMessage::text("user", message.render_room_update());
            let cost = Self::messages_cost(std::slice::from_ref(&rendered));
            // The newest update is always required, even if it alone exceeds the
            // share: one oversized message must not silently demote the very
            // stimulus the turn exists to answer. If it genuinely cannot fit, the
            // existing capacity fault says so honestly.
            if required.is_empty() || used.saturating_add(cost) <= cap {
                used += cost;
                required.push(rendered);
            } else {
                demoted.push(rendered);
            }
        }
        required.reverse();
        demoted.reverse();
        if !demoted.is_empty() {
            let demoted_tokens = Self::messages_cost(&demoted);
            crate::probe!(
                class = "delib.room_updates.demoted",
                persona = %self.persona_name,
                window = window,
                cap = cap,
                required = required.len(),
                required_tokens = used,
                demoted = demoted.len(),
                demoted_tokens = demoted_tokens,
                "required room payload exceeded its share of the window — the newest                  stay REQUIRED and the older were demoted to ordinary history, which                  is still rendered when the window allows. Nothing was discarded: an                  unbounded required payload is what makes a busy room a mute citizen."
            );
        }
        (required, demoted)
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
    pub(super) const PER_MESSAGE_TEMPLATE_TOKENS: usize = 5;

    pub(super) fn context_piece_tokens(faculty: &str, body_bytes: usize) -> usize {
        // The actual rendered shape is "\n[faculty]\n<body>\n".
        (faculty.len() + body_bytes + "\n[]\n\n".len()).div_ceil(GUARD_CHARS_PER_TOKEN)
    }

    pub(super) fn working_context_header_cost() -> usize {
        deliberation_prompt::WORKING_CONTEXT_HEADER
            .len()
            .div_ceil(GUARD_CHARS_PER_TOKEN)
    }

    fn framing_cost(system: &str) -> usize {
        // System is a message too; the template also opens the assistant's
        // generation turn. Charge both wrappers using the same format allowance.
        est_tokens(system) + 2 * Self::PER_MESSAGE_TEMPLATE_TOKENS
    }

    /// How many MESSAGES a cluster walk may cross — how far the front may advance past a
    /// broken cluster, and how deep the fill cap looks back for her own last move. A
    /// count of messages, not of tokens or seconds: there is nothing here to measure.
    const CLUSTER_SCAN_MESSAGES: usize = 6;

    /// HER OWN move: an assistant turn.
    fn is_own_move(message: &ChatMessage) -> bool {
        message.role == "assistant"
    }

    /// The OUTCOME of one of her moves: a tool-result / receipt block.
    fn is_move_result(message: &ChatMessage) -> bool {
        let body = message.content_text();
        body.starts_with("Full result of") || body.starts_with('⚙')
    }

    /// Does this message CONTINUE a cluster rather than open one? One owner for the
    /// predicate: the evictor uses it so a window never opens on an answer without its
    /// question, and [`PromptMessages::recent_move_evidence`] uses it to find the run at
    /// the END that must not be evicted at all.
    fn continues_cluster(message: &ChatMessage) -> bool {
        Self::is_own_move(message) || Self::is_move_result(message)
    }

    /// What this conversation costs whole, with no budget applied — the conversational
    /// half of a turn's demand ([`super::working_set`]).
    fn messages_cost(messages: &[ChatMessage]) -> usize {
        messages.iter().map(Self::message_cost).sum()
    }

    fn message_cost(message: &ChatMessage) -> usize {
        // Text-only lower bound, calibrated by the provider's measured prompt
        // count. Never count base64 characters as image tokens. Native images
        // require the finalized-body image quote before local generation.
        let tokens = super::token_budget::estimate_message_content_tokens(&message.content, est_tokens);
        tokens + Self::PER_MESSAGE_TEMPLATE_TOKENS
    }

    /// Fit an already-built conversation to `budget_tokens` with a whole-message front drop.
    /// Within an epoch nothing is dropped (`epoch_view`); this drop runs when the epoch turns
    /// over, and it frees `prompt.headroom` past the minimum: her measured per-act growth, the
    /// room the new epoch grows into before it turns over again (an exact fit turned over on
    /// every act). Whole turns only, never a mid-message shear
    /// ([[collapse-dont-clip-condense-the-past-never-erode-it]]).
    fn fit_messages(
        &self,
        mut prompt: PromptMessages,
        budget_tokens: usize,
    ) -> Result<FittedMessages, PromptCapacityError> {
        let required_tokens = prompt.required_tokens();
        // A quiet self-tick still needs one complete situation message. It has
        // no external stimulus to protect, but returning zero messages is not a
        // valid provider request. Preserve its newest whole context or fault.
        let ambient_tokens = if required_tokens == 0 {
            match prompt.history.last() {
                Some(message) => Self::message_cost(message),
                None => 0, // no history and no required payload: assembly supplies the quiet world-state
            }
        } else {
            0
        };
        let minimum_tokens = required_tokens + ambient_tokens;
        if minimum_tokens > budget_tokens {
            return Err(PromptCapacityError {
                required_tokens: minimum_tokens,
                budget_tokens,
            });
        }
        let history_budget = budget_tokens - required_tokens;
        // 82416bb1: during an ongoing concern WM's ring only appends. Before
        // dropping a result AND its expansion handle, price the source's compact
        // alternative. No second store or mutation of WM: custody stays with the
        // existing saved result, and only the fitted view changes. Unsaved results
        // never supply an alternative. Ordinary eviction still bounds this view.
        // Collapsing and dropping both leave the measured growth free (`prompt.headroom`).
        let headroom = prompt.headroom;
        let mut history_tokens = Self::messages_cost(&prompt.history);
        // A turnover's view must leave the growth free even when compaction alone fit it.
        let over = history_tokens.saturating_add(headroom) > history_budget;
        for (index, pointer) in std::mem::take(&mut prompt.result_pointers) {
            let Some(message) = prompt.history.get_mut(index) else { continue };
            let old_cost = Self::message_cost(message);
            let pointer_cost = Self::message_cost(&pointer);
            if over && history_tokens > history_budget.saturating_sub(headroom) && pointer_cost < old_cost {
                history_tokens = history_tokens.saturating_sub(old_cost - pointer_cost);
                *message = pointer;
            } else {
                prompt.result_pointers.push((index, pointer));
            }
        }
        // Read BEFORE the history is borrowed for costing: what her own last move and its
        // outcome cost, so the drop below can refuse to reach them.
        let evidence = prompt.recent_move_evidence();
        let messages = &prompt.history;
        // Per-message template overhead: `Self::PER_MESSAGE_TEMPLATE_TOKENS`, shared
        // with `messages_cost` so measurement and fitting charge identically.
        let costs: Vec<usize> = messages.iter().map(Self::message_cost).collect();
        let total: usize = costs.iter().sum();
        let mut start = 0usize;
        if total.saturating_add(headroom) > history_budget {
            let drop_q = total.saturating_add(headroom) - history_budget;
            let mut dropped = 0usize;
            // Only optional history yields. Stimulus and action payload are typed
            // slots outside this eviction range, even when both render as user.
            let drop_end = if required_tokens == 0 {
                messages.len().saturating_sub(1)
            } else {
                messages.len()
            };
            // HER OWN LAST MOVE IS THE LAST THING TO GO, never the first.
            //
            // Front-dropping is already recency-ordered, so the oldest turns yield first
            // — but HOW FAR the drop reaches is decided by cost alone, and a tight fill
            // budget walks it right up to the run at the end: her act, its result, and
            // the turn that asked for it. That run is the evidence that she ALREADY DID
            // THE THING. Evict it and she re-issues the same act — which the M5 records
            // as `persona.act.repeat_short_circuited` ("identical act already satisfied
            // this turn"), then `persona.act.no_deliverable_yet`, then an hour line with
            // zero writes that reads as a lazy persona.
            //
            // Protected whenever the budget can hold it at all. When it CANNOT, nothing
            // here can save it — stopping short would put the assembled prompt over the
            // window, which mutes her for the whole tick — but it is never silent: a
            // `CannotFit` is an outcome this code declares and names, with what was lost.
            //
            // `min`, not `saturating_sub`: the quiet-tick branch above already holds the
            // LAST message back, and that message is inside the evidence suffix —
            // subtracting would reserve one extra old turn and leave the fit over budget.
            let evidence_fit = evidence.fit_within(history_budget);
            if evidence_fit == EvidenceFit::CannotFit {
                // A REFUSAL, NOT A QUIET DROP (Astra's review of #4290, and her test
                // `continuity_insufficient_capacity_refuses_instead_of_dropping_evidence`).
                //
                // This branch is only reached when every other optional turn has already
                // yielded and her act and its result STILL do not fit. Continuing here
                // would forget a completed act and let her re-issue it, with a probe as
                // the only trace — "required action/result continuity must not become
                // optional when it exceeds capacity". So it is a capacity error, the same
                // outcome the irreducible stimulus and `latest_result` already produce,
                // on the same path the caller already handles.
                //
                // It is loud in both directions: the probe names what did not fit, and the
                // error carries the real requirement, which is the number the untruncated
                // demand receipt feeds to `serving_plan` — so a window too small for one
                // act gets PROVISIONED rather than silently trimmed forever.
                crate::probe!(
                    class = "delib.fill.evidence_cannot_fit",
                    persona = %self.persona_name,
                    verdict = evidence_fit.as_str(),
                    evidence_messages = evidence.messages,
                    evidence_tokens = evidence.tokens,
                    largest_message_tokens = evidence.largest_message,
                    history_budget,
                    truncated_chain = evidence.truncated,
                    "her most recent act and its result do NOT fit the conversation \
                     budget — refusing the turn rather than forgetting a completed act. \
                     The window is too small for a turn of this shape"
                );
                return Err(PromptCapacityError {
                    required_tokens: required_tokens.saturating_add(evidence.tokens),
                    budget_tokens,
                });
            }
            if evidence_fit == EvidenceFit::TooLargeToCarry {
                // Loud, but NOT a refusal — see [`EvidenceFit::TooLargeToCarry`]. The
                // oversized member yields like any other optional turn and the rest of
                // the drop proceeds; refusing here is the 836-refusals outage.
                crate::probe!(
                    class = "delib.fill.evidence_cannot_fit",
                    persona = %self.persona_name,
                    verdict = evidence_fit.as_str(),
                    evidence_messages = evidence.messages,
                    evidence_tokens = evidence.tokens,
                    largest_message_tokens = evidence.largest_message,
                    history_budget,
                    truncated_chain = evidence.truncated,
                    "one message of her last act is on its own larger than the whole \
                     conversation budget — it cannot be carried whole by any policy, so \
                     it yields rather than muting the turn. The answer is condensing it, \
                     never a capacity fault"
                );
            }
            let drop_end = if evidence_fit == EvidenceFit::Protected {
                drop_end.min(messages.len().saturating_sub(evidence.messages))
            } else {
                drop_end
            };
            while start < drop_end && dropped < drop_q {
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
            let mut opener_advance = 0usize;
            while start + 1 < messages.len()
                && start < drop_end
                && opener_advance < Self::CLUSTER_SCAN_MESSAGES
                && Self::continues_cluster(&messages[start])
            {
                start += 1;
                opener_advance += 1;
            }
        }
        let receipt = FitReceipt {
            history_messages: costs.len(),
            history_tokens: total,
            kept_messages: costs.len() - start,
            dropped_messages: start,
            dropped_tokens: costs[..start].iter().sum(),
            required_tokens,
            history_budget,
            headroom,
        };
        // Move the surviving messages; fitting never clones the entire prompt.
        prompt.history.drain(..start);
        let grounding_at = prompt.grounding_at.saturating_sub(start);
        let refit = PromptRefit {
            history_len: prompt.history.len(),
            grounding_at,
            grounding_len: 0,
            result_pointers: prompt.result_pointers.into_iter()
                .filter_map(|(index, pointer)| index.checked_sub(start).map(|i| (i, pointer)))
                .collect(),
        };
        // The steps ledger changes on every act (a new action at its tail, an old one aged
        // out at its head). In a multi-act turn it rides after everything append-only
        // across the acts and just before the pinned result, which stays last (card
        // 5b09111e: on the M5 it sat ahead of the framing, the ask and 18 room updates that
        // were byte-identical between acts, and forfeited all of them). With no pinned
        // result it keeps its place ahead of the ask: bracketed meta after the ask gets
        // answered and parroted in its place (the 2026-07-20 humaneval fix).
        if !prompt.latest_result.is_empty() {
            prompt.history.extend(prompt.stimulus);
            prompt.history.extend(prompt.room_updates);
            prompt.history.extend(prompt.ledger);
            prompt.history.extend(prompt.notices.into_iter().map(|(_, notice)| notice));
            prompt.history.extend(prompt.action_statement);
            prompt.history.extend(prompt.latest_result);
        } else {
            prompt.history.extend(prompt.action_statement);
            prompt.history.extend(prompt.ledger);
            prompt.history.extend(prompt.notices.into_iter().map(|(_, notice)| notice));
            prompt.history.extend(prompt.stimulus);
            prompt.history.extend(prompt.room_updates);
        }
        Ok(FittedMessages {
            messages: prompt.history,
            grounding_at,
            receipt,
            refit,
        })
    }

    /// Reduce only the already admitted optional snapshot after an exact quote.
    /// The conversion proposes a reduction, not a tokenizer guarantee; the adapter
    /// must quote the resulting request again. Each successful correction consumes
    /// a whole optional message or its one saved-result representation.
    fn refit_rejected_prompt(
        &self,
        mut view: DeliberationPromptView,
        measured: u32,
        available: u32,
        reply: u32,
    ) -> Result<DeliberationPromptView, PromptCapacityError> {
        let old_estimate = view.estimated_prompt_tokens;
        let old_reductions = view.refit.remaining_reductions();
        let refusal = || PromptCapacityError {
            required_tokens: (measured as usize).saturating_add(reply as usize),
            budget_tokens: available as usize,
        };
        if measured == 0 || old_estimate == 0 || old_reductions == 0 {
            return Err(refusal());
        }
        // Local admission requires prompt + reply < window (serving_guard).
        // The additional token represents that strict boundary, not a safety fudge.
        let deficit = (measured as usize).saturating_add(reply as usize)
            .saturating_sub((available as usize).saturating_sub(1));
        let reduction = ((deficit as u128 * old_estimate as u128)
            .div_ceil(measured as u128)).min(usize::MAX as u128) as usize;
        if reduction == 0 { return Err(refusal()); }
        let input_budget = old_estimate.saturating_sub(reduction);
        let fixed_tokens = old_estimate.saturating_sub(Self::messages_cost(&view.messages));
        let grounding: Vec<_> = view.messages.drain(
            view.refit.grounding_at..view.refit.grounding_at + view.refit.grounding_len
        ).collect();
        let required = view.messages.split_off(view.refit.history_len);
        let frozen_grounding_tokens = Self::messages_cost(&grounding);
        let prompt = PromptMessages {
            body: Vec::new(),
            headroom: 0,
            input_identity: view.input_identity,
            history: view.messages,
            result_pointers: view.refit.result_pointers,
            grounding_tokens: 0,
            grounding_at: view.refit.grounding_at,
            stimulus: None,
            latest_result: required,
            action_statement: None,
            room_updates: Vec::new(),
            ledger: None,
            notices: Vec::new(),
        };
        let mut fitted = self.fit_messages(prompt, input_budget
            .saturating_sub(fixed_tokens).saturating_sub(frozen_grounding_tokens))?;
        if fitted.refit.remaining_reductions() >= old_reductions {
            return Err(refusal());
        }
        fitted.refit.grounding_len = grounding.len();
        fitted.messages.splice(fitted.grounding_at..fitted.grounding_at, grounding);
        let estimate = fixed_tokens.saturating_add(Self::messages_cost(&fitted.messages));
        if estimate >= old_estimate { return Err(refusal()); }
        view.messages = fitted.messages;
        view.refit = fitted.refit;
        view.estimated_prompt_tokens = estimate;
        view.segments = segment_map(&view.system, &view.messages);
        view.capacity_error = None;
        view.epoch = None;
        Ok(view)
    }
}

pub(crate) use super::tool_surface::SurfaceReason;

/// Prompt roles are provider-facing; these slots retain the substrate's meaning
/// until fitting is complete. They confer no authority on a message's author.
struct PromptMessages {
    input_identity: [u8; 32],
    history: Vec<ChatMessage>,
    /// The keyed sources `history[..grounding_at]` renders; her epoch appends their news.
    body: Vec<epoch_view::Block>,
    /// Tokens a front drop frees past the minimum: the room her next epoch grows into.
    headroom: usize,
    /// Source-owned compact alternatives, indexed into history; never inferred from prose.
    result_pointers: Vec<(usize, ChatMessage)>,
    /// Kept before the append-only result ring even when older history yields.
    grounding_tokens: usize,
    grounding_at: usize,
    stimulus: Option<ChatMessage>,
    /// Atomic required native call/result group (one prose message for legacy receipts).
    latest_result: Vec<ChatMessage>,
    /// Public action text retains assistant provenance and required-input pricing.
    action_statement: Option<ChatMessage>,
    room_updates: Vec<ChatMessage>,
    /// The steps ledger, which changes on every act: it rides after the room updates and
    /// before the pinned result, so nothing stable sits behind it (card 5b09111e).
    ledger: Option<ChatMessage>,
    /// Substrate notices cannot disappear with the optional thought bid. Their
    /// existing store owns retention; oversized payloads use the explicit fit refusal.
    notices: Vec<(&'static str, ChatMessage)>,
}

/// Her OWN most recent move and its outcome, priced — the SUFFIX of the optional history
/// that a binding fill budget must not reach into.
///
/// A suffix, not a run in the middle: you cannot drop from the middle of a conversation,
/// so protecting her act means keeping everything from its opener to the end — including
/// the follow-up question that arrived after it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct RecentMove {
    messages: usize,
    tokens: usize,
    /// The cluster extends FURTHER BACK than the scan bound: this is a suffix of her
    /// causal chain, not the chain. Rides the receipt so a partial protection never
    /// reads as a whole one.
    truncated: bool,
    /// The largest single message in the unit. Separates "this unit does not fit, but
    /// every piece of it COULD have been kept" from "one piece of it cannot be carried
    /// whole under any policy" — see [`EvidenceFit`].
    largest_message: usize,
}

/// Whether her most recent causal unit can be held out of a binding drop.
///
/// The seam other machinery composes with — a `cannot-fit` here is an OUTCOME the
/// substrate declares and names on `delib.fill.evidence_cannot_fit`, never a quiet
/// fallback to ordinary dropping. Something too large to fit must never disappear with
/// nobody told; that is the disease this whole change exists to end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EvidenceFit {
    /// No move of hers within the scan bound — there is nothing to protect.
    NoMove,
    /// The unit fits the budget and is held out of the drop.
    Protected,
    /// The unit is larger than the budget, but every message in it would individually
    /// fit. Dropping it forgets a completed act that COULD have been kept, so the turn
    /// refuses instead (Astra's review of #4290): required action/result continuity does
    /// not become optional when it exceeds capacity.
    CannotFit,
    /// One message in the unit is on its own larger than the whole conversation budget.
    ///
    /// No policy can carry that whole, and refusing here does not preserve it — it just
    /// mutes the citizen. That exact refusal is a MEASURED outage: an unbounded required
    /// result produced 836 refusals and zero acts on 2026-09-10, and the substrate's
    /// answer was to cap the result at its share of the window and demote the remainder
    /// to history (the demoted remainder is what lands back here). So this is loud but
    /// not fatal: the oversized member yields like any other optional turn, and the real
    /// fix is condensing it, never a capacity fault
    /// ([[collapse-dont-clip-condense-the-past-never-erode-it]]).
    TooLargeToCarry,
}

impl EvidenceFit {
    fn as_str(self) -> &'static str {
        match self {
            Self::NoMove => "no_move",
            Self::Protected => "protected",
            Self::CannotFit => "cannot_fit",
            Self::TooLargeToCarry => "too_large_to_carry",
        }
    }
}

impl RecentMove {
    /// Can this unit be held out of a drop against `history_budget`? See [`EvidenceFit`]
    /// — a `CannotFit` is declared and named, never a silent fallback.
    fn fit_within(self, history_budget: usize) -> EvidenceFit {
        if self.messages == 0 || self.tokens == 0 {
            EvidenceFit::NoMove
        } else if self.tokens <= history_budget {
            EvidenceFit::Protected
        } else if self.largest_message > history_budget {
            EvidenceFit::TooLargeToCarry
        } else {
            EvidenceFit::CannotFit
        }
    }
}

impl PromptMessages {
    /// The suffix that carries her OWN most recent move and its outcome: from the turn
    /// that OPENED the cluster through the end of the history.
    ///
    /// Her move need not be the LAST message — a follow-up question, a room turn, a peer
    /// asking about the thing she just did all arrive after it, and those are exactly the
    /// turns whose answer depends on the act being still visible. So the scan walks back
    /// past trailing ordinary turns to find her move, then back through its cluster, then
    /// one more for the opener (an act without the ask that produced it is the
    /// broken-cluster shape the evictor already refuses to leave at the front).
    ///
    /// # What this does NOT do, stated plainly
    ///
    /// This is an ASSEMBLER HEURISTIC over message roles and prose prefixes, and it is
    /// bounded by [`LlmDeliberationFaculty::CLUSTER_SCAN_MESSAGES`] in both directions:
    ///
    /// - a move further back than the bound (more than N ordinary turns have happened
    ///   since) is NOT protected — `RecentMove::default()`, and the receipt says
    ///   `kept_recent_move_messages=0`;
    /// - a tool chain LONGER than the bound is protected only as a SUFFIX, with
    ///   [`RecentMove::truncated`] set, and no opener is added because the message before
    ///   the cut is another link, not the real opener;
    /// - it does not protect an act referenced by an arbitrarily later message, and it
    ///   does not close recent-action loss.
    ///
    /// The real answer is SOURCE-OWNED CAUSAL GROUPING — the source that emits an act and
    /// its result declares them one unit, and the assembler protects declared units
    /// instead of sniffing prefixes. That is out of reach in this change (Astra's review
    /// of #4290); this is the smaller honest move, with its limits on the receipt.
    fn recent_move_evidence(&self) -> RecentMove {
        let history = &self.history;
        let scan = LlmDeliberationFaculty::CLUSTER_SCAN_MESSAGES;
        // Walk back over turns that arrived AFTER her move.
        let mut end = history.len();
        let mut skipped = 0usize;
        while end > 0
            && skipped < scan
            && !LlmDeliberationFaculty::continues_cluster(&history[end - 1])
        {
            end -= 1;
            skipped += 1;
        }
        if end == 0 || !LlmDeliberationFaculty::continues_cluster(&history[end - 1]) {
            return RecentMove::default();
        }
        // …then back through the cluster itself.
        let mut idx = end;
        let mut span = 0usize;
        while idx > 0 && span < scan && LlmDeliberationFaculty::continues_cluster(&history[idx - 1])
        {
            idx -= 1;
            span += 1;
        }
        let truncated = idx > 0 && LlmDeliberationFaculty::continues_cluster(&history[idx - 1]);
        if !truncated && idx > 0 {
            idx -= 1;
        }
        RecentMove {
            messages: history.len() - idx,
            tokens: history[idx..]
                .iter()
                .map(LlmDeliberationFaculty::message_cost)
                .sum(),
            truncated,
            largest_message: history[idx..]
                .iter()
                .map(LlmDeliberationFaculty::message_cost)
                .max()
                .unwrap_or(0), // unwrap_or: the span is non-empty by construction here; 0 is unreachable
        }
    }

    /// Native active batches must preserve exact one-to-one correlation. Legacy
    /// prose has no native calls; it is not reconstructed or silently substituted.
    fn active_protocol_error(&self) -> Option<String> {
        use crate::ai::types::{ContentPart, MessageContent};
        let mut calls = Vec::new();
        let mut results = Vec::new();
        for message in &self.latest_result {
            if let MessageContent::Parts(parts) = &message.content {
                for part in parts {
                    match part {
                        ContentPart::ToolUse { id, .. } => calls.push(id.as_str()),
                        ContentPart::ToolResult { tool_use_id, .. } => results.push(tool_use_id.as_str()),
                        _ => {}
                    }
                }
            }
        }
        let unique: std::collections::HashSet<_> = calls.iter().copied().collect();
        (calls != results || unique.len() != calls.len() || calls.iter().any(|id| id.is_empty()))
            .then(|| "active tool receipt has missing, duplicate, or mismatched native call/result IDs; refusing an invalid continuation".to_string())
    }

    /// Her epoch turned over: the history's epoch part becomes the compacted view (with the result
    /// pointers the fit may substitute), the growth room is freed, the standing pieces count again.
    /// Returns its budget: all fresh, so `start` (net of framing and tools), never below her required input.
    fn restart_epoch(&mut self, epoch: &mut epoch_view::EpochView, cause: epoch_view::TurnoverCause, standing: &[epoch_view::Block], standing_unsaid: usize, headroom: usize, start: usize, persona: &str) -> usize {
        let (len, pointers) = epoch.turn_over(&mut self.history, self.grounding_at, &self.body, cause, persona);
        self.grounding_at = len;
        self.result_pointers = pointers;
        self.headroom = headroom;
        self.grounding_tokens = self.grounding_tokens - standing_unsaid
            + epoch.unsaid_tokens(standing, LlmDeliberationFaculty::message_cost);
        start.max(self.required_tokens())
    }

    fn required_tokens(&self) -> usize {
        self.stimulus
            .iter()
            .chain(self.room_updates.iter())
            .chain(self.ledger.iter())
            .chain(self.notices.iter().map(|(_, notice)| notice))
            .chain(self.action_statement.iter())
            .chain(self.latest_result.iter())
            .map(|message| LlmDeliberationFaculty::messages_cost(std::slice::from_ref(message)))
            .sum::<usize>()
            + self.grounding_tokens
    }

    fn tokens(&self) -> usize {
        LlmDeliberationFaculty::messages_cost(&self.history) + self.required_tokens()
    }
}

/// Finite optional representations remaining in the admitted prompt. Grounding
/// stays frozen at its original position; required messages remain one intact tail.
#[derive(Debug, Clone, Default)]
struct PromptRefit {
    history_len: usize,
    grounding_at: usize,
    grounding_len: usize,
    result_pointers: Vec<(usize, ChatMessage)>,
}

impl PromptRefit {
    fn remaining_reductions(&self) -> usize {
        self.history_len.saturating_add(self.result_pointers.len())
    }
}

#[derive(Default)]
struct FittedMessages {
    refit: PromptRefit,
    messages: Vec<ChatMessage>,
    grounding_at: usize,
    /// What the fit did to her optional history (card d33e928a, Kimi's finding): the
    /// front-drop was silent, so a turn whose reduction emptied her history — every room
    /// message, operator direction included, gone before compose — left no receipt.
    receipt: FitReceipt,
}

/// The fit's own account of the optional history it was handed and what survived.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
struct FitReceipt {
    history_messages: usize,
    history_tokens: usize,
    kept_messages: usize,
    dropped_messages: usize,
    dropped_tokens: usize,
    required_tokens: usize,
    history_budget: usize,
    headroom: usize,
}

impl FitReceipt {
    /// Optional history existed and none of it survived the fit.
    fn emptied(&self) -> bool {
        self.history_messages > 0 && self.kept_messages == 0
    }
}


/// The required prompt input cannot be represented in this window.
/// No partial payload is submitted: the existing fault and demand paths expose
/// the capacity problem without recording it as a citizen's decision to pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptCapacityError {
    pub required_tokens: usize,
    pub budget_tokens: usize,
}

impl std::fmt::Display for PromptCapacityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "complete prompt input requires {} tokens, but only {} are available",
            self.required_tokens, self.budget_tokens
        )
    }
}

/// A snapshot of exactly what the deliberation faculty sends the model — the
/// glass box over the RAG/prompt. Print it, capture it, diff it across turns.
#[derive(Debug, Clone)]
pub struct DeliberationPromptView {
    // Source-owned boundaries over this exact assembled snapshot, never re-inferred
    // from prose or refreshed from working memory during corrective admission.
    refit: PromptRefit,
    pub system: String,
    input_identity: [u8; 32],
    /// Cached pre-calibration cost, including selected schemas and framing.
    pub estimated_prompt_tokens: usize,
    /// The reply's allowance this plan granted (its flex grant plus what no other claim
    /// took, under its runaway bound) — charged against the window here, and carried
    /// verbatim as the wire request's `max_tokens`.
    pub completion_reserve: u32,
    pub capacity_error: Option<PromptCapacityError>,
    pub protocol_error: Option<String>,
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
    /// Her epoch with this view's body, committed only if the model answers this view.
    epoch: Option<epoch_view::EpochView>,
}

impl DeliberationPromptView {
    /// Index of the first message after her epoch body: the body, then the standing pieces
    /// and grounding appended after the fit, end here; the per-act tail starts here.
    pub(crate) fn stable_prefix_messages(&self) -> usize {
        self.refit.grounding_at + self.refit.grounding_len
    }

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
        let mut receipts = Vec::new();
        let contribution = self.contribute_with_receipts(ws, &mut receipts).await;
        let mut contribution = match contribution {
            Some(contribution) => contribution,
            None if receipts.is_empty() => return None,
            // A corrective retry may yield before dispatch. Keep its earlier
            // attempt's evidence without inventing a decision or another call.
            None => Contribution::context(FacultyId::Deliberation, "", 0.0, ""),
        };
        contribution.receipts = receipts;
        Some(contribution)
    }
}

impl LlmDeliberationFaculty {
    /// Materialize only the latest active typed result, once for this turn and
    /// its capacity retry. Handles stay in memory/history; verified pixels live
    /// only in this request projection. No browser action or model call here.
    async fn active_visual_feedback(
        &self,
        binding: &ModelBinding,
    ) -> Result<Option<ActiveVisualFeedback>, String> {
        self.active_visual_feedback_at(binding, crate::media::artifact::store_path())
            .await
    }

    async fn active_visual_feedback_at(
        &self,
        binding: &ModelBinding,
        store_path: std::path::PathBuf,
    ) -> Result<Option<ActiveVisualFeedback>, String> {
        use crate::ai::types::{ContentPart, ImageInput};
        use base64::Engine;

        let Some(wm) = &self.working_memory else {
            return Ok(None);
        };
        let Some((seq, _)) = wm.active_action_full() else {
            return Ok(None);
        };
        let images: Vec<_> = wm
            .active_action_observations(seq)
            .into_iter()
            .flat_map(|act| act.output.image.into_iter()
                .chain(act.output.additional_images.into_iter().flatten()))
            .collect();
        if images.is_empty() {
            return Ok(None);
        }
        let model = binding.requested_model();
        let native = binding
            .adapter
            .model_metadata(model)
            .is_some_and(|model| model.has(crate::model_registry::Capability::Vision));
        if !native {
            let parts = images.into_iter().map(|image| {
                let text = format!(
                    "[image {}: native pixels were not attached; model {model} has no declared Vision capability in its adapter metadata. The artifact receipt is not visual perception.]",
                    image.hash,
                );
                (image.hash, ContentPart::Text { text })
            }).collect();
            return Ok(Some(ActiveVisualFeedback { seq, parts }));
        }
        // Verified CAS reads and encoding are blocking work; never perform them
        // on the runtime executor or while holding working-memory locks.
        let parts = tokio::task::spawn_blocking(move || {
            let store = airc_blobs::FsStore::new(store_path).map_err(|error| error.to_string());
            images
                .into_iter()
                .map(|image| {
                    let part = match store.as_ref().map_err(Clone::clone)
                        .and_then(|store| image.read(store)) {
                        Ok(bytes) => ContentPart::Image {
                            image: ImageInput {
                                url: None,
                                base64: Some(
                                    base64::engine::general_purpose::STANDARD.encode(bytes),
                                ),
                                mime_type: Some(image.mime),
                            },
                        },
                        Err(error) => ContentPart::Text { text: format!(
                            "[image {} could not be loaded: {error}. No pixels attached; capture or inspect the image again.]", image.hash,
                        ) },
                    };
                    (image.hash, part)
                })
                .collect::<Vec<_>>()
        })
        .await
        .map_err(|error| format!("active screenshot projection failed: {error}"))?;
        Ok(Some(ActiveVisualFeedback { seq, parts }))
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
        // Occurrence-time framing belongs to speech history, not grounding.
        let dated_history = body.starts_with("[occurred ")
            || body.starts_with("[occurrence time unknown] ");
        let label = if !dated_history
            && (body.starts_with('[')
                || body.starts_with("Full result of")
                || body.starts_with('⚙'))
        {
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
static LAST_TURN_SHAPE: std::sync::LazyLock<dashmap::DashMap<uuid::Uuid, TurnShape>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);

/// A MIND'S MEASURED TURN, READABLE FROM OUTSIDE HER FACULTY (card ebce2ba0). The same
/// number [`LlmDeliberationFaculty::expected_occupancy`] serves — her last turn's shape at
/// this box's measured prefill and decode rates — exposed so the seams ABOVE the faculty
/// can size their own bounds from it instead of from a constant. The act deadline
/// (`act_observe::settle`) is the first such caller: it was reaping generations that were
/// still inside the bound the request itself carried.
///
/// `None` when any term is unmeasured (her first turn, an unmeasured box): an absence is
/// not a number, and the named constant then governs alone.
///
/// The rates come through the SAME ladder the fill cap reads (fresh → stale → the box's
/// most conservative → none), not the fresh-only read: a stale 63 t/s point was good
/// enough to ADMIT a 1907-token prompt on the IntelMac (card c30a4757) while this
/// function, asking for fresh only, returned `None` and left the 300 s floor to govern
/// the wait for the prompt that rate had sized. One rate, one meaning: what sizes the
/// prompt sizes its wait.
pub(crate) fn expected_occupancy_for(persona_id: uuid::Uuid) -> Option<std::time::Duration> {
    let shape = LAST_TURN_SHAPE.get(&persona_id).map(|s| *s)?;
    let model = crate::inference::llama_server::current_serving().active_model?;
    let prefill_tps = crate::inference::prefill_rate::measured_rate_for(&model).tps?;
    let decode_tps = crate::inference::decode_knee::rate_for(&model).tps?;
    occupancy_of(shape, prefill_tps, decode_tps)
}

/// PURE: the wall time one turn of `shape` is expected to occupy the lane at this box's
/// measured rates — the UNCACHED prompt at `prefill_tps` plus the output at `decode_tps`.
/// `None` unless both rates are positive numbers: an absence (or a NaN) is not a number.
fn occupancy_of(shape: TurnShape, prefill_tps: f64, decode_tps: f64) -> Option<std::time::Duration> {
    if !(prefill_tps > 0.0 && decode_tps > 0.0) {
        return None;
    }
    let uncached = shape.input.saturating_sub(shape.cached) as f64;
    let secs = uncached / prefill_tps + shape.output as f64 / decode_tps;
    Some(std::time::Duration::from_secs_f64(secs))
}

/// Append one request's exact message list to `<dir>/<persona>.wire.jsonl` (the wire capture,
/// sink 4 of PRIVACY-OF-THOUGHT.md). A turn in her mind room writes nothing.
fn append_wire_capture(dir: &std::path::Path, persona_id: Uuid, persona_name: &str, room_id: Uuid, request: &TextGenerationRequest) {
    if crate::persona::mind_room::is_private_room(persona_id, room_id) {
        crate::persona::mind_room::note_withheld(persona_id, "wire_capture");
        return;
    }
    let row = serde_json::json!({
        "ts_ms": crate::time::EpochMs::now().millis(),
        "persona": persona_name,
        "messages": request
            .messages
            .iter()
            .map(|m| serde_json::json!({"role": m.role, "text": m.content_text()}))
            .collect::<Vec<_>>(),
    });
    let _ = std::fs::create_dir_all(dir);
    use std::io::Write as _;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(format!("{persona_name}.wire.jsonl")))
    {
        let _ = writeln!(f, "{row}");
    }
}

/// Her HANDS: the file / work / git / cargo / tool verbs plus the discovery pair,
/// selected on the COMMAND names (`code/read`, `work/state`, …) before the wire
/// dialect renames them (`edit_file`, `list_recipes`, …).
/// The verbs a capable citizen's hands add over the core working set: the web, and the
/// push and PR verbs that make a change land without an operator (#4532). Offered only when
/// the served window's tool share holds them (see `rebuild_tool_surface`).
fn is_extended_hand(name: &str) -> bool {
    // `code/shell-poll` is NOT extended: it is the second half of `code/shell`, whose own
    // description says to poll a running execution with it. Idris (2026-09-29) started a
    // long command on a core-hands window and had no verb to see it finish.
    // Rooms and chat are hers (Joel: "join in anywhere like any of you"), and extended:
    // a window too small for the full set keeps the focused working set and its
    // conversation; a window that holds them (Kimi's does) carries her rooms and voice.
    name.starts_with("web/")
        // Her self-determination verbs (focus/continue, nudge, mute) ride every window
        // that holds them; on the 8192 survival window the newest line outranks them
        // (the guard below this file's tool_surface test), as it does the web.
        || name.starts_with("focus/")
        || name.starts_with("room/")
        || name.starts_with("chat/")
        || matches!(name, "code/git/add" | "code/git/push" | "code/github/pr-create" | "code/github/pr-comment")
}

/// Her hands without the extended verbs: what a window too small for the full set carries.
fn core_hands(raw: &[NativeToolSpec]) -> Vec<NativeToolSpec> {
    hands_surface(raw).into_iter().filter(|s| !is_extended_hand(&s.name)).collect()
}

fn hands_surface(raw: &[NativeToolSpec]) -> Vec<NativeToolSpec> {
    raw.iter()
        .filter(|s| {
            let n = s.name.as_str();
            // `code/git/apply` takes a PEER's unified diff; offered among her
            // hands it reads as "apply my fix" and four citizens called it
            // with an empty patch inside twenty minutes (2026-09-04) — the
            // offered-tool-is-must-use reflex. It stays one `commands/list`
            // away for a reviewer who actually holds a peer's diff.
            if crate::cognition::tool_dialect::withheld_from_hands(n).is_some() {
                return false;
            }
            // The same reflex, measured 2026-09-19 (Joel: "she could do all this
            // before"): `work/submission` ("Inspect a submission…", params card_id +
            // room) reads to a model holding a card as "look at my card", and
            // `work/review` as "review the spec". Kimi called work/submission 6 of 6
            // times failed and work/review 3 of 3; Delia on IntelMac 48 and 72 in one
            // day; Sahar the same shape — every one an act spent on a refusal, so the
            // turn that should have edited read instead. Both joined the surface on
            // 9/16 (#4102), the night the landings stopped. A reviewer who holds a
            // review card, or a citizen reading receipts, reaches them through
            // `commands/list`; the holder's own hands are `work/get` and `work/submit`.
            // HER HANDS ARE OURS (Joel, 2026-09-28: the citizens were hand-crippled; the
            // point is a team that replaces Claude or Codex; 2026-10-04: let them, then
            // restrict later). Every resident gets the web (search and fetch), like Claude,
            // whatever her model, matching `resident_trust` at the gate so what she is
            // offered is what she may do; a window too small for the full set carries
            // `core_hands`. Not every verb: a full dump blows the prompt budget and has
            // muted personas before (persona_tools bound).
            // Her own mind's verbs are hands (focus/continue, focus/nudge, focus/mute): the
            // self-determination seam is decorative if she can read its manual but has no
            // slot to call it. Kimi, 2026-10-05: "present in the registry, absent from my
            // surface ... I've read it six times ... there is nothing to invoke against."
            n.starts_with("focus/")
                // Her rooms and her voice in them are hers (room/join, room/leave,
                // room/list, room/members, chat/send): with no automatic seating
                // (HER-LOOP row C), a citizen who cannot join a room can be unseated but
                // never choose one. Joel: "she ought to be able to just join in anywhere
                // like any of you."
                || n.starts_with("room/")
                || n.starts_with("chat/")
                || n.starts_with("code/")
                || n.starts_with("work/")
                || n.starts_with("git/")
                || n.starts_with("cargo/")
                || n.starts_with("tool/")
                || n.starts_with("commands/")
                || n.starts_with("web/")
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests;
