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

use super::deliberation_budget::turn_message_line_addressed;
use continuum_base::token_estimate::{estimate_guard_tokens, GUARD_CHARS_PER_TOKEN};
use super::deliberation_parse::decision_from_response;
use super::deliberation_prompt;
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

mod prompt;
pub use self::prompt::*;
mod request;
mod admission;
pub(crate) mod admission_clock;
use self::request::*;
mod reply_claim;
mod read_receipt;
pub use read_receipt::ReadReceipt;
use self::reply_claim::*;
mod verdict;
mod tool_surface;
use super::tool_surface::{SelectedSurface, ToolSurface};

pub struct ActiveVisualFeedback {
    seq: u64,
    pub parts: Vec<(String, crate::ai::types::ContentPart)>,
}

/// A shared, wait-free model binding — the SAME [`ArcSwap`] the faculty reads on
/// every generation and the owning
/// [`WorkspaceCycle`](super::workspace::WorkspaceCycle) re-homes when the served
/// model changes. One `ArcSwap`, two holders — exactly like [`GenomeHandle`] and
/// [`DecodingHandle`]. A re-home stores a fresh [`ModelBinding`]; the faculty's
/// next generation reads it, and the genome + working memory + admission are
/// carried across untouched (no cycle rebuild). See
/// [[seamless-persona-failover-model-and-genome]].
pub use super::model_binding::ModelBindingHandle;

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
    Arc::new(super::model_binding::ModelBindingState::new(ModelBinding {
        adapter,
        model,
        context_window,
    }))
}

/// Default sampling temperature for deliberation — enough warmth for natural
/// voice, not so much it drifts.
const DEFAULT_TEMPERATURE: f32 = 0.7;

/// The flex claim ids of the deliberation prompt plan. One window, five claimants.
const FLEX_FRAMING: &str = "framing";

const FLEX_REQUIRED: &str = "required_input";

const FLEX_GROUNDING: &str = "grounding";

const FLEX_HISTORY: &str = "history";

const FLEX_REPLY: &str = "reply";

/// Her hands (`prompt/tools_epoch.rs`): floor = the epoch's level or core hands, want = all.
const FLEX_TOOLS: &str = "tools";

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
    pub binding: ModelBindingHandle,
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
    /// Her native tool surfaces (the whole registry, her hands, her core hands), built
    /// once per tool assignment in canonical order; which one a turn sends is
    /// the retained tools in [`Self::prompt_view`], selected from the shared allocation.
    surface: ToolSurface,
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
    /// Where a sent prompt's read receipt goes (`read_receipt.rs`); `None` marks nothing read.
    read_receipt: Option<Arc<dyn read_receipt::ReadReceipt>>,
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
    pub working_set: Option<super::working_set::WorkingSetRegistry>,
    /// Her context view across acts (`prompt/epoch.rs`): one epoch that only appends, turned
    /// over when it outgrows her history's grant.
    context_view: parking_lot::Mutex<
        Option<continuum_context::view::View<crate::cognition::context_streams::ContextMessage>>,
    >,
    /// The tool set her current view epoch chose (`prompt/tools_epoch.rs`).
    tools_epoch: parking_lot::Mutex<Option<prompt::ToolsEpoch>>,
    /// A measurement fork's (`fork_eval_cycle`): its turns warm their own page, never hers.
    forked: bool,
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
            surface: ToolSurface::default(),
            working_memory: None,
            prompt_capture: None,
            read_receipt: None,
            genome: empty_genome(),
            decoding: relaxed_decoding(),
            working_set: None,
            context_view: parking_lot::Mutex::new(None),
            tools_epoch: parking_lot::Mutex::new(None),
            forked: false,
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
        self.share_vocabulary();
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

    /// Is this turn an ACT with her hands (a deliverable, or her own initiative) rather
    /// than a message turn? Asked by the ACT output cap and the lane priority, so it lives
    /// in one place. Never by the tool surface: that is the same on every kind of turn.
    fn is_work_turn(&self, ws: &Workspace) -> bool {
        (ws.workspace_deliverable || ws.initiative) && !self.surface.hands().is_empty()
    }

    /// What kind of turn this is, for its allowance: a work turn is an ACT; anything
    /// else is a PASS.
    pub fn turn_kind(&self, ws: &Workspace) -> TurnKind {
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
        // Re-derive the surface in the (possibly new) model's wire dialect. Which surface
        // a turn sends reads the served window at selection (`select_tool_surface`).
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
    pub fn framing_floor_tokens(&self) -> u32 {
        let bare = self.compose_system(&std::collections::BTreeSet::new(), false, false, None);
        estimate_guard_tokens(&bare) as u32
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
    /// stack hands out. It is only the unmeasured WANT of the reply's claim; measurement
    /// replaces it with her need, and a turn cut inside its answer records that channel at
    /// DOUBLE, so an act that really needs more room earns it back within a turn. What no
    /// claim takes is her view's room, never the reply's (ONE-BUDGET.md slice 6: a runaway
    /// cap claimed above her context brought this defect back on 2026-10-30).
    const COMPLETION_COLD_PRIOR_TOKENS: u32 = crate::inference::planning::MIN_SERVE_CTX;

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
    pub const COMPLETION_FLOOR_TOKENS: u32 = crate::inference::planning::MIN_SERVE_CTX / 8;

    /// Does this workspace carry an [active-work] grounding contribution naming a
    /// card she currently holds IN PROGRESS? Structural claim-state (the source read
    /// it from airc this turn), decoded from our own wire format at the one predicate
    /// colocated with its renderer. Drives the working-presence contract on
    /// undirected turns; derived ONCE per prompt so the framing-token estimate and
    /// the final render agree byte-for-byte (the budget-math invariant).
    pub fn holds_live_work(ws: &Workspace) -> bool {
        ws.broadcast
            .iter()
            .filter(|c| c.decision.is_none() && !c.trailing)
            .filter(|c| c.faculty.as_str() == crate::cognition::faculty_ids::ACTIVE_WORK)
            .any(|c| crate::cognition::faculty_ids::renders_held_in_progress(&c.content))
    }

    /// Does this workspace name a card she HOLDS (Claimed or InProgress)? The lane
    /// gate's key for `LanePriority::Work` — wider than `holds_live_work`, which keys
    /// the working-presence contract on InProgress only.
    /// This mind's next call's bound on the lane: prefill of the prompt she did not have
    /// cached last turn at this box's measured prefill rate, plus her last output at its
    /// measured decode rate. Read from her LAST turn's shape (`LAST_TURN_SHAPE`) — the
    /// prompt is rendered after the gate, so the previous turn is the honest predictor.
    /// `None` when any term is unmeasured: an absence is not a number (the probe says so).
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
            continuum_base::probe!(
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
            .filter(|c| c.faculty.as_str() == crate::cognition::faculty_ids::ACTIVE_WORK)
            .any(|c| crate::cognition::faculty_ids::renders_held_card(&c.content))
    }
}

pub(crate) use super::tool_surface::SurfaceReason;

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
        let mut rendered = Vec::new();
        let contribution = self.contribute_with_receipts(ws, &mut receipts, &mut rendered).await;
        let mut contribution = match contribution {
            Some(contribution) => contribution,
            None if receipts.is_empty() && rendered.is_empty() => return None,
            // A corrective retry may yield before dispatch. Keep its earlier
            // attempt's evidence without inventing a decision or another call.
            None => Contribution::context(FacultyId::Deliberation, "", 0.0, ""),
        };
        contribution.receipts = receipts;
        contribution.render_receipts = rendered;
        Some(contribution)
    }
}

impl LlmDeliberationFaculty {
    /// Materialize only the latest active typed result, once for this turn and
    /// its capacity retry. Handles stay in memory/history; verified pixels live
    /// only in this request projection. No browser action or model call here.
    pub async fn active_visual_feedback(
        &self,
        binding: &ModelBinding,
    ) -> Result<Option<ActiveVisualFeedback>, String> {
        self.active_visual_feedback_at(binding, crate::media::artifact::store_path())
            .await
    }

    pub async fn active_visual_feedback_at(
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

/// Append one request's message list, whole, to `<dir>/<persona>.wire.jsonl` (the wire capture,
/// sink 4 of PRIVACY-OF-THOUGHT.md). A turn in her mind room writes nothing. The boundary is the
/// ADAPTER'S INPUT (`TextGenerationRequest::messages`), not the HTTP body an adapter serializes
/// from it; a write failure is silent here, so a missing row is never evidence no request ran.
fn append_wire_capture(dir: &std::path::Path, persona_id: Uuid, persona_name: &str, room_id: Uuid, request: &TextGenerationRequest) {
    if crate::cognition::mind_room::is_private_room(persona_id, room_id) {
        crate::cognition::mind_room::note_withheld(persona_id, "wire_capture");
        return;
    }
    let row = serde_json::json!({
        "ts_ms": continuum_base::time::EpochMs::now().millis(),
        "persona": persona_name,
        // Whole messages, every part: a text-only projection dropped native tool calls and
        // results, so her newest result read as a bare label in the one input of record
        // (Aiko, the M5, 2026-10-29: a result counted as lost was on the wire).
        "messages": request.messages,
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

#[cfg(test)]
mod tests;

/// The deliberation fixtures this module's tests and core's registry tests share: a scripted
/// adapter that replays canned responses and records every request, its response builder, a
/// read tool, and the workspaces the prompt tests shape.
#[cfg(any(test, feature = "test-fixtures"))]
pub mod test_support {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use serde_json::json;
    use uuid::Uuid;

    use crate::ai::adapter::AIProviderAdapter;
    use crate::ai::types::{
        FinishReason, NativeToolSpec, TextGenerationRequest, TextGenerationResponse, ToolCall,
        ToolInputSchema, UsageMetrics,
    };
    use crate::cognition::workspace::{BurstTurn, Workspace};

    /// The surface internals core's registry tests price against the live registry.
    pub use crate::cognition::tool_surface::hands_surface;
    pub use super::prompt::fit::{GroundingPiece, GroundingPlan};
    pub use super::reply_claim::reply_claim;
    pub use super::request::TurnKind;
    pub use crate::cognition::tool_surface::SelectedSurface;
    pub use crate::cognition::tool_surface::SurfaceReason;

    pub type WindowReadHook = Box<dyn FnOnce() + Send>;
    pub type RequestHook = Box<dyn FnMut(&TextGenerationRequest) -> Option<crate::ai::inference_error::InferenceError> + Send>;

    pub fn make_response(
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

    pub fn read_tool() -> NativeToolSpec {
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
    pub struct ScriptedAdapter {
        pub responses: Mutex<VecDeque<TextGenerationResponse>>,
        pub seen: Mutex<Vec<TextGenerationRequest>>,
        pub rejections: Mutex<VecDeque<crate::ai::inference_error::InferenceError>>,
        pub live_window: Option<u32>,
        pub window_sequence: Mutex<VecDeque<u32>>,
        pub on_window_read: Mutex<Option<WindowReadHook>>,
        pub on_request: Mutex<Option<RequestHook>>,
    }

    impl ScriptedAdapter {
        pub fn new(responses: Vec<TextGenerationResponse>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                seen: Mutex::new(Vec::new()),
                rejections: Mutex::new(VecDeque::new()),
                live_window: None,
                window_sequence: Mutex::new(VecDeque::new()),
                on_window_read: Mutex::new(None),
                on_request: Mutex::new(None),
            }
        }
        pub fn call_count(&self) -> usize {
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
        fn live_served_window(&self) -> Option<u32> {
            // Inject the exact load → observation → publication interleaving
            // without scheduler timing or a second adapter implementation.
            let hook = self
                .on_window_read
                .lock()
                .expect("fixture hook lock")
                .take();
            if let Some(hook) = hook {
                hook();
            }
            let mut sequence = self.window_sequence.lock().expect("fixture window sequence");
            if sequence.len() > 1 { sequence.pop_front() } else {
                sequence.front().copied().or(self.live_window)
            }
        }
        async fn generate_stream_checked(
            &self,
            request: TextGenerationRequest,
            sink: tokio::sync::mpsc::UnboundedSender<crate::ai::adapter::GenerationChunk>,
        ) -> Result<TextGenerationResponse, crate::ai::inference_error::InferenceError>
        {
            let injected = self.on_request.lock().expect("fixture request hook")
                .as_mut().and_then(|hook| hook(&request));
            if let Some(error) = injected {
                self.seen.lock().expect("fixture request log").push(request);
                return Err(error);
            }
            let rejection = self
                .rejections
                .lock()
                .expect("fixture rejection queue")
                .pop_front();
            if let Some(error) = rejection {
                self.seen.lock().expect("fixture request log").push(request);
                return Err(error);
            }
            self.generate_stream(request, sink)
                .await
                .map_err(Into::into)
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

    /// A writing activity: history worth trading, then the ask. Shared by the flex
    /// reply tests below so each compares one workspace shape.
    pub fn writing_workspace(room: crate::identity::ActivityRoom, history: usize, deliverable: bool) -> Workspace {
        use crate::cognition::workspace::Burst;
        let mut turns: Vec<_> = (0..history)
            .map(|i| {
                BurstTurn::attributed(
                    i % 2 == 1,
                    if i % 2 == 1 { "Benchy" } else { "Peer" },
                    format!("distinct_history_{i} ").repeat(60),
                    Some(i as u64),
                )
            })
            .collect();
        turns.push(BurstTurn::attributed(
            false,
            "Peer",
            "Write the migration file now: create the accounts table with its indexes.",
            Some(history as u64 + 1),
        ));
        let mut ws = Workspace::new(Burst::from_turns(room, turns));
        ws.workspace_deliverable = deliverable;
        ws
    }

    pub fn history_with_stimulus(stimulus: &str) -> Workspace {
        use crate::cognition::workspace::Burst;
        let room = crate::identity::ActivityRoom::from_uuid(Uuid::new_v4()).unwrap();
        Workspace::new(Burst::from_turns(
            room,
            vec![
                BurstTurn::attributed(
                    false,
                    "Peer",
                    "old chatter line\n".repeat(2000),
                    Some(1),
                ),
                BurstTurn::attributed(false, "Peer", stimulus, Some(2)),
            ],
        ))
    }
}
