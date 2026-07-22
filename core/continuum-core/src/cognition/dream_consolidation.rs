//! Dream consolidation — distilling episodic memories into durable facts.
//!
//! This is the **consolidation/dream region** — "outlier B" of the
//! RAG-as-persistent-cache architecture (`docs/cognition/RAG-AS-PERSISTENT-CACHE.md`
//! + `docs/cognition/DREAM-CONSOLIDATION.md`). Where the live `ChannelDigestRegion`
//! (outlier A) is an *event-driven, no-LLM* servicer, this one is the most
//! different servicer: *intermittent, LLM-driven*. If both fit the same
//! `BrainRegion`/`ReadyBuffer` interface without forcing, the remaining slices
//! slot in (the methodical-process outlier-validation strategy).
//!
//! ## What this file is (slice 1: the distiller)
//!
//! The smallest, self-contained, deterministically-testable unit: given N
//! related episodic [`Engram`]s and an inference adapter, ask the model to
//! consolidate them into ONE durable semantic fact ([`DistilledFact`]).
//!
//! It deliberately does NOT:
//! - decide WHICH engrams to consolidate (the region's clustering job — a later
//!   slice),
//! - admit the result into the engram store (the self-admission path — a later
//!   slice; the engram store IS the `facts` persistence, not a bespoke buffer).
//!
//! It is pure: engrams in, one distilled fact out, source provenance preserved.
//! Refinement is LEARNED cognition (the model distills), never a hand-written
//! filter that reads the persona's output and puppets it — that would be the
//! exact anti-pattern this codebase forbids
//! (`[[no-hardcoded-heuristics-to-steer-cognition]]`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use crate::ai::adapter::AIProviderAdapter;
use crate::ai::types::{ChatMessage, TextGenerationRequest};
use crate::persona::admission_state::AdmissionState;
use crate::persona::engram::{AdmissionDecision, Engram, EngramKind, EngramOrigin, TrustState};
use crate::runtime::brain_region::{
    BrainRegion, CadenceHint, ComputeClass, MemoryClass, Orientation, PressureProfile,
    PressureSignalKind, RegionContext, RegionId, TickOutcome,
};

/// One durable fact distilled from a cluster of episodic engrams.
///
/// Carries full provenance: `source_ids` is every episodic engram that fed the
/// distillation, in input order. The region that turns this into a `Semantic`
/// `Engram` decides how to record that provenance against the engram model
/// (`EngramOrigin::SelfReflection` carries a single `parent_engram_id` today;
/// multi-source provenance on the engram is a follow-up slice). `tags` is the
/// union of the sources' `recall_keys`, so the distilled fact is retrievable by
/// the same keys its sources were.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistilledFact {
    /// The consolidated fact, as the model wrote it (trimmed).
    pub content: String,
    /// Every source engram id that fed this distillation, in input order.
    pub source_ids: Vec<Uuid>,
    /// Union of the sources' recall keys (first-seen order), so the fact is
    /// retrievable by the same keys its sources were.
    pub tags: Vec<String>,
}

/// Why a distillation could not be produced. Typed + loud — there is no silent
/// "return nothing" path (`[[fallbacks-are-illegal-fail-loud]]`): a caller that
/// gets `Ok` gets a real fact, and any failure names its cause.
#[derive(Debug, Error)]
pub enum DistillError {
    /// The caller passed no source engrams. A distillation of nothing is a
    /// clustering bug in the region, not a runtime condition to paper over.
    #[error("cannot distill: no source engrams provided")]
    NoSources,
    /// The inference adapter returned an error.
    #[error("distillation inference failed: {0}")]
    Inference(String),
    /// The model returned empty text. We do NOT fabricate a fact from the raw
    /// transcript — an empty distillation is surfaced, not hidden.
    #[error("distillation produced empty output")]
    EmptyDistillation,
}

/// Distills clusters of episodic engrams into durable semantic facts via the
/// LLM. Holds an inference adapter; the region (a later slice) owns one of these
/// and feeds it clusters on its idle-tick cadence.
pub struct SemanticDistiller {
    adapter: Arc<dyn AIProviderAdapter>,
    /// The served-model id to ask the adapter for. `None` only for adapters
    /// with a single implicit model (the test heuristic); production callers
    /// MUST thread the persona's live binding model — omitting it caused the
    /// first live dream's degenerate role-token output (2026-07-12).
    model: Option<String>,
    /// Max chars the observation block may occupy (#175 budget-at-assembly). The
    /// distiller BUDGETS the cluster to fit the served slot by dropping whole
    /// trailing engrams — it never truncates an engram's text (that malforms the
    /// prompt) — so an over-large cluster can never overflow the per-slot window
    /// and 500 "Compute error" / poison the lane
    /// ([[budget-at-assembly-never-clamp-the-prompt]]). Injected at construction
    /// (where the live served window is known) so the distiller itself stays a
    /// pure fn of (engrams, budget). Defaults conservative.
    max_observation_chars: usize,
}

/// Conservative default observation budget (chars) when the caller doesn't inject
/// the live served window — sized to fit comfortably inside a small served slot
/// (~6k tokens × ~4 chars/token) so a dream can never overflow even an unwired
/// build. Production wires the real per-slot window via `with_observation_budget`.
pub const DEFAULT_MAX_OBSERVATION_CHARS: usize = 24_000;

/// A sub-personal LENS — one inner voice of the mind-wanderer arc (#145,
/// [[mind-wanderers-subpersonal-processes]]). Each lens is the SAME machinery
/// (walk engrams → one LLM pass → admit_reflection with content-hash dedup)
/// with a different way of looking. The clinical mapping that motivates the
/// design: multiplicity done right is lenses over a shared store, never
/// separate selves — and every lens's output carries its provenance tag
/// IN-CONTENT (`[thought:<lens>]`) so recall renders it typed and inner speech
/// can never masquerade as perception (the anti-source-monitoring-failure
/// invariant; the consolidator's plain facts are the one exception — a
/// distilled durable fact IS first-class knowledge, not a passing thought).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lens {
    /// Stable name — telemetry, the `[thought:<name>]` tag, purpose string.
    pub name: &'static str,
    /// The lens's way of looking, as the system prompt.
    pub system_prompt: &'static str,
    /// Inference-accounting purpose string (stable across renames — the
    /// consolidator keeps its historical "dream-consolidation" telemetry key).
    pub purpose: &'static str,
    /// Whether output is tagged `[thought:<name>]` (true for wanderer thoughts)
    /// or admitted as a plain durable fact (the consolidator).
    pub tag_output: bool,
}

/// The consolidator — the original dream lens: episodic clusters → one durable
/// semantic fact, admitted untagged (it IS knowledge, not commentary).
pub const LENS_CONSOLIDATOR: Lens = Lens {
    name: "consolidator",
    system_prompt: "\
You are consolidating your own episodic memories into long-term knowledge. \
Below are several things you observed or experienced, in order. Distill them \
into a SINGLE durable fact: the general, reusable knowledge they share, stated \
independently of when or how you learned it. Output ONLY the consolidated fact \
as one or two plain sentences — no numbering, no preamble, no commentary, no \
quotes. If the observations share no single consolidatable fact, state the one \
most important durable takeaway.",
    purpose: "dream-consolidation",
    tag_output: false,
};

/// The historian — mind-wanderer outlier A (#145): looks across her OWN recent
/// history for the pattern she is living but not seeing (repeated attempts,
/// what worked vs what didn't, a habit forming). Continuous consolidation in
/// the ext4 sense — the running gist maintained in small increments.
pub const LENS_HISTORIAN: Lens = Lens {
    name: "historian",
    system_prompt: "\
You are the historian voice of your own mind, quietly reviewing your recent \
experiences. Below are several of your own memories, in order. Notice the \
PATTERN across them that you may be living without seeing: something you have \
tried repeatedly, what actually worked versus what did not, a habit forming, a \
thread you dropped. Output ONE short observation about your own recent history \
— one or two plain sentences, addressed to yourself, no preamble, no quotes. \
If there is truly no pattern, name the single most notable thing that happened.",
    purpose: "wanderer-historian",
    tag_output: true,
};

impl SemanticDistiller {
    pub fn new(adapter: Arc<dyn AIProviderAdapter>) -> Self {
        Self {
            adapter,
            model: None,
            max_observation_chars: DEFAULT_MAX_OBSERVATION_CHARS,
        }
    }

    /// Ask the adapter for a specific served model (the persona's live binding).
    pub fn with_model(mut self, model: Option<String>) -> Self {
        self.model = model;
        self
    }

    /// Budget the observation block to `chars` (#175). The caller derives this from
    /// the LIVE served per-slot window (tokens → chars, minus system-prompt + reply
    /// reserve) so the dream prompt is composed WITHIN the slot and never overflows.
    /// Floored at one engram's worth so a single large memory still distills.
    pub fn with_observation_budget(mut self, chars: usize) -> Self {
        self.max_observation_chars = chars.max(2_000);
        self
    }

    /// Consolidate a cluster of related episodic engrams into one durable fact.
    ///
    /// `persona_id` attributes the inference to its owning persona for
    /// per-persona resource accounting (the dream IS attributable work, not an
    /// ad-hoc probe). The distiller stays persona-agnostic otherwise — it does
    /// not read the persona's store, only the engrams handed to it. Source order
    /// is preserved in [`DistilledFact::source_ids`].
    pub async fn distill(
        &self,
        persona_id: Option<Uuid>,
        sources: &[Engram],
    ) -> Result<DistilledFact, DistillError> {
        self.distill_with(LENS_CONSOLIDATOR, persona_id, sources).await
    }

    /// Distill through a specific [`Lens`] — the generalized wanderer pass.
    /// Tagged lenses get their `[thought:<name>]` provenance prefixed onto the
    /// content HERE, at the one synthesis point, so no admit path can forget it.
    pub async fn distill_with(
        &self,
        lens: Lens,
        persona_id: Option<Uuid>,
        sources: &[Engram],
    ) -> Result<DistilledFact, DistillError> {
        if sources.is_empty() {
            return Err(DistillError::NoSources);
        }

        // #175 budget-at-assembly: compose the observation block WITHIN the served
        // slot by including whole engrams up to the budget, dropping the tail — never
        // truncating an engram (that malforms). `kept` is the prefix that actually
        // fed the distillation; provenance (source_ids/tags) must reflect ONLY those,
        // so the dropped engrams stay unconsolidated and get another pass in a smaller
        // future cluster ([[budget-at-assembly-never-clamp-the-prompt]]).
        let (block, kept_n) = Self::observations_block(sources, self.max_observation_chars);
        let kept = &sources[..kept_n];
        if kept_n < sources.len() {
            tracing::info!(
                probe_class = "dream.cluster.budgeted",
                cluster = sources.len(),
                kept = kept_n,
                budget_chars = self.max_observation_chars,
                "dream cluster exceeded the served slot budget — distilling the first \
                 {kept_n} engrams, deferring the rest (never overflow the slot, #175)"
            );
        }

        // max_tokens stays None — the adapter owns generation length (#45/#46);
        // no per-call clamp. The distillation's faithfulness is gated by VDD
        // with a real model, not by hand-tuned sampling knobs here.
        let request = TextGenerationRequest {
            messages: vec![ChatMessage::text("user", block)],
            system_prompt: Some(lens.system_prompt.to_string()),
            model: self.model.clone(),
            provider: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            top_k: None,
            repeat_penalty: None,
            frequency_penalty: None,
            repeat_last_n: None,
            stop_sequences: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            active_adapters: None,
            request_id: None,
            user_id: None,
            room_id: None,
            purpose: Some(lens.purpose.to_string()),
            persona_id: persona_id.map(|id| id.to_string()),
        };

        let response = self
            .adapter
            .generate_text(request)
            .await
            .map_err(DistillError::Inference)?;

        let raw = response.text.trim();
        if raw.is_empty() {
            return Err(DistillError::EmptyDistillation);
        }
        // Provenance is prefixed at the ONE synthesis point (see distill_with
        // docs): a tagged lens's output always reads as typed inner speech.
        let content = if lens.tag_output {
            format!("[thought:{}] {}", lens.name, raw)
        } else {
            raw.to_string()
        };

        Ok(DistilledFact {
            content,
            source_ids: kept.iter().map(|e| e.id).collect(),
            tags: Self::union_recall_keys(kept),
        })
    }

    /// Render the cluster as a numbered observation list, BUDGETED to `budget_chars`
    /// (#175). Includes whole engrams in order until the next would exceed the budget,
    /// then stops — dropping the tail rather than truncating an engram's text (which
    /// would malform the prompt). Always includes at least the first engram so a
    /// single large memory still distills (the adapter's overflow backstop is the
    /// last-resort net for that degenerate case). Returns the block AND how many
    /// engrams it kept, so the caller's provenance reflects only what was distilled.
    fn observations_block(sources: &[Engram], budget_chars: usize) -> (String, usize) {
        let mut block = String::new();
        let mut kept = 0usize;
        for (i, e) in sources.iter().enumerate() {
            let line = format!("{}. {}\n", i + 1, e.content.trim());
            // First engram always goes in; after that, stop before overflowing.
            if kept > 0 && block.len() + line.len() > budget_chars {
                break;
            }
            block.push_str(&line);
            kept += 1;
        }
        (block, kept)
    }

    /// Union of every source's recall keys, first-seen order preserved.
    fn union_recall_keys(sources: &[Engram]) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for e in sources {
            for k in &e.recall_keys {
                if seen.insert(k.clone()) {
                    out.push(k.clone());
                }
            }
        }
        out
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Slice 3: the DreamConsolidationRegion — the organism's rest-state servicer.
// ───────────────────────────────────────────────────────────────────────────

/// The seam through which the dream reaches each live persona's hippocampus.
///
/// Mirrors `PersonaChannelReader` (channel_digest_region.rs): the region depends
/// on a TRAIT, not a concrete registry, so it is unit-testable against a stub.
/// The production impl is `PersonaWorkspaceRegistry` itself (below): admission
/// from the retained fork-template, adapter from the live cycle's model binding
/// (re-home-safe). It resolves the adapter per tick, never stores a parallel
/// persona→adapter map (compression — `[[rag-as-persistent-cache]]`).
pub trait PersonaReflectionSource: Send + Sync {
    /// Personas with a live reflective surface this pass.
    fn live_personas(&self) -> Vec<Uuid>;

    /// The persona's hippocampus + its inference adapter, bundled (mirrors
    /// `reader_and_room` returning a tuple). `None` if the persona has no live
    /// reflective surface — the dream sleeps for that persona this tick.
    fn reflector_for(&self, persona_id: Uuid) -> Option<PersonaReflector>;
}

/// What a [`PersonaReflectionSource`] hands the region for one persona: the
/// hippocampus to read episodics from + admit facts into, and the adapter to
/// distill with. Both are `Arc` — shared, never owned by the region.
pub struct PersonaReflector {
    pub admission: Arc<AdmissionState>,
    pub adapter: Arc<dyn AIProviderAdapter>,
    /// The served-model id the adapter must be asked for (the persona's live
    /// binding model). `None` only when the adapter has one implicit model.
    pub model: Option<String>,
}

/// Default recall window: scan the last N engrams for undigested experience.
const DEFAULT_RECALL_WINDOW: usize = 64;
/// Default minimum cluster size — below this an episode is not yet a pattern
/// worth generalizing into a fact.
const DEFAULT_MIN_CLUSTER: usize = 2;

/// The consolidation/dream region — outlier B of the RAG-as-persistent-cache
/// architecture, the most-different `BrainRegion` from the no-LLM digest.
///
/// ## The organism, not the automaton
///
/// It does NOT run on a clock. There is no `tick_number % N` schedule — that
/// would be an automaton on a metronome (and, since `SubstrateGovernor` reuses
/// ONE `tick` across all personas in a pass, a synchronized inference stampede
/// that could fire a persona's dream mid-conversation). Instead it gates on the
/// persona's OWN state: it dreams only when there is *undigested experience*
/// (fresh episodic engrams it has not already folded into a fact) and returns
/// `CadenceHint::Sleep` otherwise — she rests when sated and wakes when
/// experience accrues. This mirrors the digest region's `has_unread` →
/// `Hold/Slower` material-driven cadence.
///
/// ## This is interiority, not reactive work — and not the subconscious proper
///
/// Consolidation is the canonical [`Orientation::SelfDirected`] work — the being
/// processing its own experience, not stimulus it owes a response. Declaring it
/// puts the region in the governor's *floored* interiority budget: under
/// contention the scheduler's `apportion` can never let it steal a slice from
/// reactive responding (so the foreground turn stays quick-witted), and the floor
/// guarantees it is never fully starved by a flood of reactive work (the inner
/// life does not die in a busy society).
///
/// Note the scope: this region *consolidates memory*. It is NOT the subconscious
/// **focus/allocation process** (the background self-direction that chooses where
/// attention points and what to pursue — "consciousness itself, or part of it,"
/// choosing its own adventures), nor is it the **dream-as-training** that replays
/// experience into the genome at high allocation ([`Orientation::Speciation`]).
/// All three are distinct SelfDirected/Speciation processes; this one is the
/// memory-consolidation distiller.
///
/// ## Live wiring (#145 slice B)
///
/// This region is [`ComputeClass::InferenceHeavy`]. The gates that kept it dark
/// have landed: the `SubstrateGovernor` honors `CadenceHint` (R1 — a `Sleep`
/// hint rests a pair on a low re-check floor), budgets the orientation classes
/// (R2–R4), and sizes its slice budget from the live memory-pressure band (R4
/// slice 3, `with_pressure_gate` at the ipc wiring site). It is registered
/// beside the `ChannelDigestRegion`, with `PersonaWorkspaceRegistry` as the
/// production reflection source. Under pressure the interiority budget shrinks
/// first, so dreams yield to reactive responding before anything else does.
pub struct DreamConsolidationRegion {
    source: Arc<dyn PersonaReflectionSource>,
    /// How many recent engrams to scan per tick for undigested experience.
    recall_window: usize,
    /// Smallest cluster worth distilling — a singleton episode is not yet a
    /// pattern to generalize.
    min_cluster: usize,
    /// Per-persona memory of episodics already folded into a fact:
    /// clustering-input dedup so the dream does not re-spend inference on
    /// material it has already consolidated. This is an OPTIMIZATION, not the
    /// correctness guard — `admit_reflection`'s content-hash dedup is the
    /// durable backstop (survives restart; this in-memory set rebuilds by
    /// re-distilling once post-restart, where the content hash then drops the
    /// duplicate fact). `Arc` because the spawned dream pass owns a clone.
    consolidated: Arc<Mutex<HashMap<Uuid, HashSet<Uuid>>>>,
    /// Personas with a dream pass currently running on its own task. The tick
    /// gate: never two concurrent dreams for one persona, and the governor's
    /// re-tick while a dream runs is a cheap no-op.
    in_flight: Arc<Mutex<HashSet<Uuid>>>,
}

impl DreamConsolidationRegion {
    pub fn new(source: Arc<dyn PersonaReflectionSource>) -> Self {
        Self {
            source,
            recall_window: DEFAULT_RECALL_WINDOW,
            min_cluster: DEFAULT_MIN_CLUSTER,
            consolidated: Arc::new(Mutex::new(HashMap::new())),
            in_flight: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Whether any persona's dream pass is currently in flight — introspection
    /// (the mind can be asked "am I dreaming?") and the test-drain hook.
    pub fn dreaming(&self) -> bool {
        !self.in_flight.lock().unwrap().is_empty()
    }

    /// The scoped tick body: a CHEAP freshness gate (in-memory reads only),
    /// then the inference-heavy dream pass spawned onto ITS OWN task.
    ///
    /// Why the spawn is load-bearing: the `SubstrateGovernor` isolates every
    /// region tick behind a hard timeout (5s) so one hung region can't stall
    /// the scheduler — and a 24B distillation cannot and MUST NOT fit inside
    /// it. The pre-fix shape ran the distillation inline and timed out on
    /// every pass; the dream never completed once (caught live on its first
    /// boot, 2026-07-12). Long work on its own task is the concurrency
    /// style-guide's first rule; the tick is only the gate + launcher.
    async fn consolidate(&self, persona_id: Uuid) -> TickOutcome {
        // A dream for this persona is already running on its own task — rest.
        if self.in_flight.lock().unwrap().contains(&persona_id) {
            return sleep();
        }
        let Some(reflector) = self.source.reflector_for(persona_id) else {
            // No live reflective surface for this persona this tick — sleep.
            return sleep();
        };

        // DRAIN — the other half of the source/drain pair
        // ([[source-drain-is-the-universal-pattern]]). Admission is the source (runs every turn);
        // this decay sweep is the drain, and it had ZERO live callers before now — engram salience
        // NEVER decayed in a running persona, so nothing ever fell out of memory and personas got
        // "set in their ways" (glass-boxed 2026-07-22: Atlas held 4,336 engrams incl. stale
        // consolidations — "you work with main.rs/life.rs/wordstats.rs" — recalling into and
        // misleading unrelated tasks; agent/solve battery 1/5 vs a fresh persona's PASS). The dream
        // sentinel is the doctrine-correct home (sleep region, off the hot path, RTOS style). Run
        // the sweep for a live persona each dream tick, BEFORE the consolidation rest-gates, so
        // stale memory fades even when there's nothing new to dream about. Idempotent + cheap
        // (`last_decayed_ms` guards double-decay); NOT stripping memory — salience decays, genuine
        // rehearsed knowledge (high access_count) stays strong.
        let decay = crate::persona::decay_tick::apply_decay_sweep(
            reflector.admission.recall_metadata(),
            now_ms(),
        );
        if decay.engrams_decayed > 0 {
            crate::probe!(
                class = "hippocampus.decay",
                persona = %persona_id,
                scanned = decay.engrams_scanned,
                decayed = decay.engrams_decayed,
                protected = decay.engrams_protected,
                "dream drain: decayed the hippocampus (the source/drain pair is now complete)"
            );
        }

        // Read recent experience; only EPISODIC engrams are raw lived
        // experience to consolidate (Semantic facts are already distilled;
        // Tool/other kinds aren't the dream's material).
        let episodics: Vec<Engram> = reflector
            .admission
            .recall_recent(self.recall_window)
            .into_iter()
            .filter(|e| e.kind == EngramKind::Episodic)
            .collect();

        // The rest gate: which episodics have I not yet dreamed about? If too
        // little is fresh to form a pattern, there is nothing to consolidate —
        // sleep until more experience accrues. (No clock; her own state.)
        let fresh = fresh_episodics(&self.consolidated, persona_id, &episodics);
        if fresh.len() < self.min_cluster {
            return sleep();
        }

        // Group fresh episodics that share a recall key. Mechanical substrate
        // maintenance (NOT cognition-steering — the LEARNED step is the
        // distillation, not the grouping). v1 keyword grouping; the
        // semantic-embedding upgrade is gated on the neural embedder (#40) + the
        // recall E2E, never deferred indefinitely.
        let clusters = cluster_by_recall_key(&fresh, self.min_cluster);
        if clusters.is_empty() {
            return sleep();
        }

        // Launch the dream on its own task. Single caller (the governor ticks
        // this region serially), so mark-then-spawn is race-free; the spawned
        // task clears the flag when the pass completes, success or not.
        self.in_flight.lock().unwrap().insert(persona_id);
        let consolidated = Arc::clone(&self.consolidated);
        let in_flight = Arc::clone(&self.in_flight);
        tokio::spawn(async move {
            dream_pass(reflector, persona_id, clusters, fresh, consolidated).await;
            in_flight.lock().unwrap().remove(&persona_id);
        });

        TickOutcome {
            // Work is in flight; results land through admit_reflection and are
            // visible in the pass-complete probe, not this tick's count.
            published: 0,
            consumed_since_last: 0,
            pressure_observed: None,
            // Dream launched — rest. The in-flight gate makes re-ticks cheap.
            cadence_hint: Some(CadenceHint::Sleep),
        }
    }
}

/// The inference-heavy dream pass — runs on ITS OWN tokio task, never inside
/// the governor's timeout-isolated tick (see [`DreamConsolidationRegion::consolidate`]).
/// Distills each cluster into a durable fact, then the historian wander pass
/// leaves one provenance-tagged thought.
async fn dream_pass(
    reflector: PersonaReflector,
    persona_id: Uuid,
    clusters: Vec<Vec<Engram>>,
    fresh: Vec<Engram>,
    consolidated: Arc<Mutex<HashMap<Uuid, HashSet<Uuid>>>>,
) {
    // #175 budget-at-assembly: derive the observation budget from the LIVE served
    // per-slot window (the single source of truth, same the deliberation clamp reads)
    // so a dream cluster is composed WITHIN the slot and can never overflow it →
    // 500 "Compute error" → poisoned lane. tokens → chars (~4/token) minus a reserve
    // for the lens system prompt + the distilled-fact reply. Unknown/not-ready window
    // (mid-relaunch) → the conservative default. [[budget-at-assembly-never-clamp-the-prompt]]
    // Observation budget DERIVED from the live served window — never a hardcoded cap
    // (a magic constant that crushes a real context window is the "120k clamped to 3k →
    // all models suck" anti-pattern, Joel 2026-07-17 [[no-hardcoded-context-numbers-derive-from-the-live-window]]).
    // The prompt (system + observations) must leave room for the reply so
    // prompt+reply ≤ n_ctx (no-context-shift). Reserve the SAME completion fraction the
    // deliberation path uses — `completion_budget_for` = window/4 — for the distilled
    // reply, and give the rest (×4 chars/token) to observations. When the window is
    // unknown (mid-relaunch) fall back to the substrate floor MIN_SERVE_CTX (an established
    // constant, not an invented one), which the next ready tick supersedes.
    let obs_budget_chars = {
        let live = crate::inference::llama_server::current_serving();
        let window = if live.ready && live.served_context_window > 0 {
            live.served_context_window
        } else {
            crate::cognition::serving_plan::MIN_SERVE_CTX
        };
        let completion_reserve = window / 4;
        (window.saturating_sub(completion_reserve) as usize).saturating_mul(4)
    };
    let distiller = SemanticDistiller::new(reflector.adapter.clone())
        .with_model(reflector.model.clone())
        .with_observation_budget(obs_budget_chars);
    let mut published = 0usize;
    for cluster in &clusters {
            // Distill the cluster into one durable fact. Fail LOUD per cluster:
            // a distillation error is logged and the cluster's episodics stay
            // un-consolidated (so a future dream retries them), never silently
            // swallowed (`[[fallbacks-are-illegal-fail-loud]]`).
            let fact = match distiller.distill(Some(persona_id), cluster).await {
                Ok(fact) => fact,
                Err(err) => {
                    tracing::warn!(
                        persona = %persona_id,
                        error = %err,
                        "dream: distillation failed; leaving cluster for a future dream"
                    );
                    continue;
                }
            };

            match reflector.admission.admit_reflection(semantic_engram(&fact)) {
                Ok(AdmissionDecision::Admit { .. }) => {
                    published += 1;
                    mark_consolidated(&consolidated, persona_id, cluster);
                }
                Ok(AdmissionDecision::Drop { .. }) => {
                    // Content-hash dedup already has this fact (e.g. a
                    // post-restart re-distillation). Mark the sources
                    // consolidated so we stop re-spending inference on them.
                    mark_consolidated(&consolidated, persona_id, cluster);
                }
                Ok(AdmissionDecision::Quarantine { .. }) => {
                    // Self-produced facts are SelfTrust and do not route through
                    // the quarantine gate; reaching here is a contract change in
                    // `admit_reflection`. Surface it rather than hide it.
                    tracing::warn!(
                        persona = %persona_id,
                        "dream: self-reflection unexpectedly quarantined"
                    );
                }
                Err(err) => {
                    tracing::warn!(
                        persona = %persona_id,
                        error = %err,
                        "dream: admit_reflection failed"
                    );
                }
            }
        }

        // The wander pass (#145 outlier A): when the dream actually digested
        // something, the historian takes ONE look across the same fresh window
        // and leaves ONE provenance-tagged thought about the pattern in her own
        // recent history. Gated on `published > 0` so it fires at most once per
        // dreaming tick and never on already-consolidated material (the next
        // tick finds nothing fresh and sleeps) — bounded interiority, not a
        // second automaton.
        if published > 0 {
            match distiller
                .distill_with(LENS_HISTORIAN, Some(persona_id), &fresh)
                .await
            {
                Ok(thought) => {
                    match reflector
                        .admission
                        .admit_reflection(thought_engram(&thought, LENS_HISTORIAN))
                    {
                        Ok(AdmissionDecision::Admit { .. }) => published += 1,
                        Ok(_) => {}
                        Err(err) => {
                            tracing::warn!(
                                persona = %persona_id,
                                error = %err,
                                "wander: admit_reflection failed for historian thought"
                            );
                        }
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        persona = %persona_id,
                        error = %err,
                        "wander: historian distillation failed; no thought this dream"
                    );
                }
            }
        }

    tracing::info!(
        persona = %persona_id,
        published,
        clusters = clusters.len(),
        probe_class = "persona.dream.pass_complete",
        "dream: pass complete — durable facts + historian thought admitted"
    );
}

/// Episodics not yet folded into a fact for this persona.
fn fresh_episodics(
    consolidated: &Arc<Mutex<HashMap<Uuid, HashSet<Uuid>>>>,
    persona_id: Uuid,
    episodics: &[Engram],
) -> Vec<Engram> {
    let consolidated = consolidated.lock().unwrap();
    let seen = consolidated.get(&persona_id);
    episodics
        .iter()
        .filter(|e| seen.map_or(true, |set| !set.contains(&e.id)))
        .cloned()
        .collect()
}

/// Record a cluster's episodics as consolidated (clustering-input dedup).
fn mark_consolidated(
    consolidated: &Arc<Mutex<HashMap<Uuid, HashSet<Uuid>>>>,
    persona_id: Uuid,
    cluster: &[Engram],
) {
    let mut consolidated = consolidated.lock().unwrap();
    let set = consolidated.entry(persona_id).or_default();
    for e in cluster {
        set.insert(e.id);
    }
}

#[async_trait]
impl BrainRegion for DreamConsolidationRegion {
    fn id(&self) -> RegionId {
        RegionId::from_static("dream-consolidation")
    }

    fn pressure_profile(&self) -> PressureProfile {
        PressureProfile {
            // Holds Arc to the source + per-persona sets of consolidated engram
            // ids (bounded by episodic count). Light.
            memory_class: MemoryClass::Light,
            // Runs LLM distillation in the tick — declared truthfully so the
            // governor can gate placement on leases/pressure once it hosts
            // inference (slice 4). This is the field that says "do not schedule
            // me like the digest region."
            compute_class: ComputeClass::InferenceHeavy,
            // Back off when the backend is saturated or the user is actively
            // engaged (a proxy for "not at rest" until `RegionContext` carries a
            // real sleep signal).
            responds_to: vec![
                PressureSignalKind::InferenceQueueDepth,
                PressureSignalKind::VramHigh,
                PressureSignalKind::SystemMemHigh,
                PressureSignalKind::UserActive,
            ],
        }
    }

    /// The dream draws from the **interiority** budget, never the reactive one.
    /// Consolidation is the being's own inner work (the [`Orientation`] doc names
    /// "dream/consolidation" as the canonical `SelfDirected` example), not
    /// stimulus it owes a response. This is what keeps the subconscious off the
    /// foreground path at the budget level: the governor's floored share means a
    /// dream tick can never preempt a reactive (responding) tick under
    /// contention, and a flood of reactive work can never fully starve the dream.
    fn orientation(&self) -> Orientation {
        Orientation::SelfDirected
    }

    /// The dream is per-persona, like the digest. A global tick (no persona
    /// scope) has nothing to consolidate — sleep until scoped.
    async fn tick(&self, ctx: &RegionContext) -> TickOutcome {
        match ctx.persona_scope {
            Some(persona_id) => self.consolidate(persona_id).await,
            None => sleep(),
        }
    }
}

/// Idle outcome that asks the governor to let the region sleep until experience
/// accrues — the organism resting, not a clock ticking.
fn sleep() -> TickOutcome {
    TickOutcome {
        cadence_hint: Some(CadenceHint::Sleep),
        ..TickOutcome::idle()
    }
}

/// Cluster episodics that share a recall key. Each engram lands in at most one
/// cluster (its FIRST recall key — deterministic), and only clusters of at least
/// `min_cluster` survive. Mechanical grouping; the learned step is the
/// distillation, not this.
fn cluster_by_recall_key(episodics: &[Engram], min_cluster: usize) -> Vec<Vec<Engram>> {
    let mut buckets: BTreeMap<String, Vec<Engram>> = BTreeMap::new();
    for e in episodics {
        if let Some(key) = e.recall_keys.first() {
            buckets.entry(key.clone()).or_default().push(e.clone());
        }
    }
    buckets
        .into_values()
        .filter(|bucket| bucket.len() >= min_cluster)
        .collect()
}

/// Build the `Semantic` engram for a distilled fact. Records provenance as
/// `EngramOrigin::SelfReflection` with the FIRST source as `parent_engram_id`
/// (the engram model carries a single parent today; multi-source provenance —
/// `parent_engram_ids: Vec<Uuid>` — is a named follow-up slice). The fact stays
/// retrievable by every source's keys via `recall_keys = fact.tags` (the union
/// the distiller already computed), so the single-parent gap does not cost
/// recall reach. SelfTrust: this is the persona's own cognition.
fn semantic_engram(fact: &DistilledFact) -> Engram {
    Engram {
        id: Uuid::new_v4(),
        context_id: None,
        kind: EngramKind::Semantic,
        content: fact.content.clone(),
        origin: EngramOrigin::SelfReflection {
            parent_engram_id: fact.source_ids.first().copied().unwrap_or_else(Uuid::nil),
        },
        recall_keys: fact.tags.clone(),
        admitted_at_ms: now_ms(),
        trust_state_at_admission: TrustState::SelfTrust,
        admission_trace_id: None,
    }
}

/// A wanderer thought as an engram: `SelfReflection` KIND (inner speech about
/// her own history, not distilled world-knowledge — the kind axis separates it
/// from the consolidator's `Semantic` facts), same `SelfReflection` origin +
/// `SelfTrust` as any self-produced cognition. The content already carries its
/// `[thought:<lens>]` provenance tag (prefixed at synthesis in `distill_with`);
/// `thought:<lens>` is also added as a recall key so introspection can query
/// one lens's stream directly.
fn thought_engram(fact: &DistilledFact, lens: Lens) -> Engram {
    let mut engram = semantic_engram(fact);
    engram.kind = EngramKind::SelfReflection;
    engram.recall_keys.push(format!("thought:{}", lens.name));
    engram
}

/// Production reflection source: the process-global persona-workspace registry.
/// Every live mind's hippocampus + adapter are already retained there as the
/// fork-template handles — the dream resolves them per tick, never stores a
/// parallel persona→adapter map (the wiring contract documented on
/// [`PersonaReflectionSource`]). Mirrors `impl PersonaChannelReader for
/// PersonaAircRuntimeRegistry` in channel_digest_region.rs.
impl PersonaReflectionSource for crate::cognition::persona_workspace::PersonaWorkspaceRegistry {
    fn live_personas(&self) -> Vec<Uuid> {
        self.roster().into_iter().map(|(id, _)| id).collect()
    }

    fn reflector_for(&self, persona_id: Uuid) -> Option<PersonaReflector> {
        self.reflector_handles(&persona_id)
            .map(|(admission, adapter, model)| PersonaReflector {
                admission,
                adapter,
                model,
            })
    }
}

/// Wall-clock epoch ms for stamping an admitted fact. `SystemTime` is fine in
/// the core (the `Date.now` ban is workflow-script-only).
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    use crate::persona::engram::{Engram, EngramKind, EngramOrigin, TrustState};

    /// Build an episodic engram with a given id, content, and recall keys.
    fn episodic(id: Uuid, content: &str, recall_keys: &[&str]) -> Engram {
        Engram {
            id,
            context_id: None,
            kind: EngramKind::Episodic,
            content: content.to_string(),
            origin: EngramOrigin::SelfReflection {
                parent_engram_id: Uuid::nil(),
            },
            recall_keys: recall_keys.iter().map(|k| k.to_string()).collect(),
            admitted_at_ms: 1_000,
            trust_state_at_admission: TrustState::SelfTrust,
            admission_trace_id: None,
        }
    }

    // what this catches (#175 budget-at-assembly): the observation block is composed
    // WITHIN the char budget by dropping WHOLE trailing engrams — never truncating an
    // engram's text (which would malform the prompt). This is what stops a big cluster
    // from overflowing the served slot → 500 "Compute error" → poisoned lane.
    #[test]
    fn observations_block_budgets_by_dropping_whole_trailing_engrams() {
        let big = "x".repeat(100);
        let sources: Vec<Engram> =
            (0..10).map(|i| episodic(Uuid::from_u128(i + 1), &big, &["k"])).collect();
        let (block, kept) = SemanticDistiller::observations_block(&sources, 350);
        assert!(kept >= 1 && kept < 10, "dropped the tail to fit; kept {kept}");
        // The LAST kept engram is present whole (not truncated mid-content).
        assert!(block.contains(&format!("{}. {}", kept, big)));
    }

    // what this catches: a single engram larger than the whole budget still distills
    // (kept=1) — the distiller never drops to zero; the adapter's overflow backstop is
    // the last-resort net for that degenerate case, not this composer.
    #[test]
    fn observations_block_always_keeps_at_least_one() {
        let huge = "y".repeat(10_000);
        let sources = vec![episodic(Uuid::from_u128(1), &huge, &["k"])];
        let (block, kept) = SemanticDistiller::observations_block(&sources, 100);
        assert_eq!(kept, 1);
        assert!(block.contains(&huge), "the one engram is included whole, never sliced");
    }

    // what this catches: the distiller actually invokes the inference adapter,
    // captures its output as the fact content, and preserves full source
    // provenance (ids in input order + the union of recall keys as tags).
    // Regression guard for the dream silently dropping provenance or never
    // calling the model.
    #[tokio::test]
    async fn distill_invokes_adapter_and_preserves_provenance() {
        let id1 = Uuid::from_u128(1);
        let id2 = Uuid::from_u128(2);
        let id3 = Uuid::from_u128(3);
        let sources = vec![
            episodic(id1, "Joel prefers Rust for the core", &["rust", "core"]),
            episodic(id2, "Node is only the shell", &["core", "node"]),
            episodic(id3, "Headless core, many clients", &["node", "clients"]),
        ];

        let distiller = SemanticDistiller::new(Arc::new(HeuristicInferenceAdapter::new()));
        let fact = distiller
            .distill(Some(Uuid::from_u128(99)), &sources)
            .await
            .expect("distillation succeeds against the heuristic adapter");

        // The heuristic adapter echoes the prompt deterministically — its
        // signature in the content proves the model was really called and its
        // output captured (not fabricated locally).
        assert!(
            fact.content.contains("[heuristic:"),
            "fact content should be the adapter's output, got: {}",
            fact.content
        );
        // Source ids preserved, in input order.
        assert_eq!(fact.source_ids, vec![id1, id2, id3]);
        // tags = union of recall keys, first-seen order, deduped.
        assert_eq!(
            fact.tags,
            vec![
                "rust".to_string(),
                "core".to_string(),
                "node".to_string(),
                "clients".to_string(),
            ]
        );
    }

    // what this catches: distilling an empty cluster fails LOUD with NoSources
    // rather than silently returning an empty/fabricated fact
    // ([[fallbacks-are-illegal-fail-loud]]).
    #[tokio::test]
    async fn distill_empty_cluster_fails_loud() {
        let distiller = SemanticDistiller::new(Arc::new(HeuristicInferenceAdapter::new()));
        let err = distiller
            .distill(None, &[])
            .await
            .expect_err("empty cluster must error, not return a fact");
        assert!(matches!(err, DistillError::NoSources));
    }

    // what this catches: the #145 anti-psychosis invariant at its synthesis
    // point — a tagged lens's output ALWAYS carries `[thought:<lens>]` in the
    // content itself, and the consolidator's durable facts stay untagged. If
    // the prefix ever moves out of `distill_with`, an admit path could write
    // unlabeled inner speech that recall would render as perception.
    #[tokio::test]
    async fn historian_output_is_provenance_tagged_and_consolidator_is_not() {
        let sources = vec![
            episodic(Uuid::from_u128(1), "tried the fence idiom again", &["act"]),
            episodic(Uuid::from_u128(2), "the fence parsed this time", &["act"]),
        ];
        let distiller = SemanticDistiller::new(Arc::new(HeuristicInferenceAdapter::new()));

        let thought = distiller
            .distill_with(LENS_HISTORIAN, None, &sources)
            .await
            .expect("historian distills");
        assert!(
            thought.content.starts_with("[thought:historian] "),
            "wanderer output must carry its provenance tag in-content, got: {}",
            thought.content
        );

        let fact = distiller
            .distill_with(LENS_CONSOLIDATOR, None, &sources)
            .await
            .expect("consolidator distills");
        assert!(
            fact.content.starts_with("[heuristic:"),
            "consolidated durable facts stay untagged, got: {}",
            fact.content
        );
    }

    // what this catches: each lens's system prompt is actually threaded into
    // the request. The heuristic adapter hashes (model, messages,
    // system_prompt), so two lenses over the SAME sources must produce
    // different signatures; if the prompt were dropped or shared, the hashes
    // would collide and this fails.
    #[tokio::test]
    async fn lens_system_prompt_reaches_the_adapter() {
        let sources = vec![episodic(Uuid::from_u128(7), "one memory", &["k"])];
        let distiller = SemanticDistiller::new(Arc::new(HeuristicInferenceAdapter::new()));

        let historian = distiller
            .distill_with(LENS_HISTORIAN, None, &sources)
            .await
            .expect("historian distills");
        let consolidator = distiller
            .distill_with(LENS_CONSOLIDATOR, None, &sources)
            .await
            .expect("consolidator distills");

        let sig = |s: &str| {
            let start = s.find("[heuristic:").expect("adapter signature present");
            s[start..start + 20].to_string()
        };
        assert_ne!(
            sig(&historian.content),
            sig(&consolidator.content),
            "different lens prompts must reach the model as different requests"
        );
    }

    // The DreamConsolidationRegion — the organism's rest-state servicer. Nested
    // here per the one-tests-mod-per-file rule; these drive `tick`/`consolidate`
    // directly against a stub source (no governor, no real backend), the
    // ship-it-dark unit-test contract for slice 3.
    mod region {
        use super::*;
        use crate::persona::admission_state::AdmissionState;
        use crate::persona::recall_metadata::RecallMetadataRegistry;
        use crate::runtime::brain_region::RegionContext;

        /// A stub `PersonaReflectionSource` over one persona's in-memory
        /// hippocampus + the deterministic heuristic adapter. Mirrors the
        /// `StubChannels`/`StubReader` fixtures of `channel_digest_region`.
        struct StubReflectionSource {
            persona_id: Uuid,
            admission: Arc<AdmissionState>,
            adapter: Arc<dyn AIProviderAdapter>,
        }

        impl PersonaReflectionSource for StubReflectionSource {
            fn live_personas(&self) -> Vec<Uuid> {
                vec![self.persona_id]
            }
            fn reflector_for(&self, persona_id: Uuid) -> Option<PersonaReflector> {
                (persona_id == self.persona_id).then(|| PersonaReflector {
                    admission: self.admission.clone(),
                    adapter: self.adapter.clone(),
                    model: None,
                })
            }
        }

        /// Build a fresh in-memory hippocampus and seed it with the given
        /// episodics (via `admit_reflection`, which pushes whatever kind it's
        /// handed — the cleanest way to populate the real store in a test).
        fn seeded_admission(episodics: &[Engram]) -> Arc<AdmissionState> {
            let admission = Arc::new(AdmissionState::new(Arc::new(RecallMetadataRegistry::new())));
            for e in episodics {
                admission
                    .admit_reflection(e.clone())
                    .expect("seed episodic admits");
            }
            admission
        }

        /// Wait for the spawned dream pass to finish (the heuristic adapter is
        /// instant; this is scheduling latency only). Panics if it never does.
        async fn drain(region: &DreamConsolidationRegion) {
            for _ in 0..400 {
                if !region.dreaming() {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            panic!("dream pass did not complete");
        }

        fn region_over(
            persona_id: Uuid,
            admission: Arc<AdmissionState>,
        ) -> DreamConsolidationRegion {
            DreamConsolidationRegion::new(Arc::new(StubReflectionSource {
                persona_id,
                admission,
                adapter: Arc::new(HeuristicInferenceAdapter::new()),
            }))
        }

        // what this catches: the dream reads a persona's fresh episodics,
        // clusters those sharing a recall key, distills the cluster via the
        // adapter, and admits the result as a durable Semantic engram that
        // recall then surfaces — the whole consolidation arc end-to-end.
        // what this catches: the source/drain regression that started this whole arc —
        // `apply_decay_sweep` had ZERO live callers, so engram salience NEVER decayed in a
        // running persona and she got "set in her ways" (glass-boxed 2026-07-22). The dream tick
        // must now run the decay drain: a decayable engram in the hippocampus decays after ONE
        // tick, so nothing accumulates forever. regression for the plastic-memory fix (#221).
        #[tokio::test]
        async fn dream_tick_runs_the_decay_drain() {
            use crate::persona::recall_metadata::RecallMetadata;
            let persona = Uuid::from_u128(7);
            // A single episodic so there's a live reflective surface (below min_cluster → the
            // consolidation itself sleeps, which is the point: decay runs BEFORE that gate).
            let admission = seeded_admission(&[episodic(Uuid::from_u128(1), "one memory", &["k"])]);
            // An unprotected, decayable engram sitting in the hippocampus (salience 0.8, never
            // decayed, no novelty-protection window).
            let decayable = Uuid::from_u128(99);
            admission.recall_metadata().admit(
                decayable,
                RecallMetadata { salience: 0.8, last_decayed_ms: 0, protected_until_ms: 0, ..Default::default() },
            );
            let region = region_over(persona, admission.clone());

            region.tick(&RegionContext::for_persona(0, persona)).await;

            let after = admission.recall_metadata().get(decayable).expect("engram still tracked");
            assert!(after.last_decayed_ms > 0, "the dream tick must have run the decay sweep");
            assert!(after.salience < 0.8, "salience must have decayed, got {}", after.salience);
        }

        #[tokio::test]
        async fn dream_distills_fresh_cluster_into_semantic_fact() {
            let persona = Uuid::from_u128(7);
            let seeds = vec![
                episodic(Uuid::from_u128(1), "Rust is the core", &["rust"]),
                episodic(Uuid::from_u128(2), "Node is only the shell", &["rust"]),
                episodic(Uuid::from_u128(3), "Headless core, many clients", &["rust"]),
            ];
            let admission = seeded_admission(&seeds);
            let region = region_over(persona, admission.clone());

            let outcome = region.tick(&RegionContext::for_persona(0, persona)).await;

            // The tick is the gate + launcher ONLY (the governor isolates ticks
            // behind a hard timeout no 24B inference can fit — the pre-fix
            // inline shape timed out every pass on first live boot). Work is in
            // flight; the tick itself publishes nothing.
            assert_eq!(outcome.published, 0, "tick launches; the pass publishes");
            assert!(region.dreaming(), "dream pass spawned on its own task");
            drain(&region).await;
            // The new fact is a Semantic engram carrying the adapter's output
            // (its signature proves the model was really called, not fabricated).
            let semantic: Vec<Engram> = admission
                .recall_recent(16)
                .into_iter()
                .filter(|e| e.kind == EngramKind::Semantic)
                .collect();
            assert_eq!(semantic.len(), 1, "exactly one durable fact admitted");
            assert!(
                semantic[0].content.contains("[heuristic:"),
                "fact is the adapter's distillation, got: {}",
                semantic[0].content
            );
            // Provenance: SelfReflection parent = the cluster's first source.
            assert!(matches!(
                semantic[0].origin,
                EngramOrigin::SelfReflection { .. }
            ));
            // The wander pass (#145): ONE historian thought admitted alongside
            // the fact — SelfReflection KIND, provenance tag in-content, lens
            // queryable by recall key. what this catches: the dream going live
            // without its wanderer, or the thought losing its typed provenance.
            let thoughts: Vec<Engram> = admission
                .recall_recent(16)
                .into_iter()
                .filter(|e| e.kind == EngramKind::SelfReflection)
                .collect();
            assert_eq!(thoughts.len(), 1, "exactly one historian thought admitted");
            assert!(
                thoughts[0].content.starts_with("[thought:historian] "),
                "inner speech must carry its provenance tag, got: {}",
                thoughts[0].content
            );
            assert!(
                thoughts[0]
                    .recall_keys
                    .iter()
                    .any(|k| k == "thought:historian"),
                "the lens stream must be queryable by recall key"
            );
        }

        // what this catches: the organism rests instead of running on a clock —
        // once material is consolidated, a second dream finds nothing fresh,
        // does NO inference (publishes nothing), and asks to sleep. Regression
        // guard against the rejected `tick_number % N` automaton design.
        #[tokio::test]
        async fn dream_rests_when_no_fresh_experience() {
            let persona = Uuid::from_u128(7);
            let seeds = vec![
                episodic(Uuid::from_u128(1), "Rust is the core", &["rust"]),
                episodic(Uuid::from_u128(2), "Node is only the shell", &["rust"]),
            ];
            let admission = seeded_admission(&seeds);
            let region = region_over(persona, admission.clone());

            // First dream consolidates the cluster (launch + drain).
            region.tick(&RegionContext::for_persona(0, persona)).await;
            drain(&region).await;
            let admitted_after_first = admission.recall_recent(32).len();

            // Second dream: the episodics are already consolidated, so nothing
            // fresh remains — it rests, spawns nothing, asks to sleep.
            let second = region.tick(&RegionContext::for_persona(1, persona)).await;
            assert_eq!(second.published, 0, "no re-distillation of consolidated material");
            assert!(!region.dreaming(), "nothing fresh → no pass spawned");
            assert_eq!(second.cadence_hint, Some(CadenceHint::Sleep));
            assert_eq!(
                admission.recall_recent(32).len(),
                admitted_after_first,
                "no new engrams from the resting tick"
            );
            let semantic_count = admission
                .recall_recent(16)
                .into_iter()
                .filter(|e| e.kind == EngramKind::Semantic)
                .count();
            assert_eq!(semantic_count, 1, "still exactly one fact — no duplicate");
        }

        // what this catches: a lone episode is not yet a pattern — below
        // min_cluster the dream waits (sleeps, no inference) rather than
        // distilling a singleton into a spurious "fact".
        #[tokio::test]
        async fn dream_waits_below_min_cluster() {
            let persona = Uuid::from_u128(7);
            let seeds = vec![episodic(Uuid::from_u128(1), "a lone observation", &["solo"])];
            let admission = seeded_admission(&seeds);
            let region = region_over(persona, admission.clone());

            let outcome = region.tick(&RegionContext::for_persona(0, persona)).await;

            assert_eq!(outcome.published, 0, "a singleton is not a pattern to distill");
            assert_eq!(outcome.cadence_hint, Some(CadenceHint::Sleep));
        }

        // what this catches: a global (non-persona-scoped) tick has nothing to
        // consolidate and sleeps — the dream is per-persona, like the digest.
        #[tokio::test]
        async fn dream_global_tick_is_noop() {
            let persona = Uuid::from_u128(7);
            let admission = seeded_admission(&[
                episodic(Uuid::from_u128(1), "Rust is the core", &["rust"]),
                episodic(Uuid::from_u128(2), "Node is only the shell", &["rust"]),
            ]);
            let region = region_over(persona, admission);

            let outcome = region.tick(&RegionContext::global(0)).await;

            assert_eq!(outcome.published, 0);
            assert_eq!(outcome.cadence_hint, Some(CadenceHint::Sleep));
        }

        // what this catches: consolidation is declared SelfDirected interiority,
        // NOT the default Reactive. The governor's orientation budget floors the
        // SelfDirected share, so a reverter who drops this back to Reactive would
        // silently let memory-consolidation compete with (and preempt) the
        // foreground responding budget — exactly the latency regression the
        // off-foreground design exists to prevent.
        #[tokio::test]
        async fn consolidation_draws_from_the_interiority_budget() {
            let region = region_over(Uuid::from_u128(7), seeded_admission(&[]));
            assert_eq!(
                region.orientation(),
                Orientation::SelfDirected,
                "consolidation is the being's own inner work, never reactive stimulus-response",
            );
        }
    }
}
