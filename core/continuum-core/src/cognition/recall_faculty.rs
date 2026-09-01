//! RecallFaculty — the hippocampus as a Workspace faculty.
//!
//! This is the perception-tier faculty that pulls relevant memory into the
//! Global Workspace (§2 of PERSONA-BRAIN-ARCHITECTURE.md) each cognition tick.
//! It bids context that the deliberation faculty then reasons over in phase 2
//! (staged assembly — "pull relevant memory, THEN decide").
//!
//! ## Why this faculty (killing the recall split)
//!
//! Recall lived in two disconnected places: the RAG `EngramSource`
//! (`persona/engram_source.rs`), which ranks by `salience × recency` but does
//! **not** record recall hits, and `AdmissionState::recall_scored`, which ranks
//! AND closes the bidirectional loop (records the hit → uplifts salience →
//! observes persistence). The RAG path was the *one-way* one. `RecallFaculty`
//! routes recall through `recall_scored` — the loop-closing path — so a memory
//! that gets recalled into the workspace this tick is **strengthened for next
//! tick** (Hebbian rehearsal, use-it-keeps-it). That is the "goes both ways"
//! property: retrieval feeds back into encoding.
//!
//! ## ML-derived salience, not a hand-weight
//!
//! The faculty's bid carries the **top recalled memory's post-decay salience**
//! as its workspace salience — how relevant the hippocampus thinks its best hit
//! is. The arbiter integrates that score; it never invents one. There is no
//! caste, no mention test, no `if` — just the recall score competing for
//! attention.
//!
//! ## Future slice: query-conditioned relevance
//!
//! v1 surfaces the most salient + recent memories (the existing Algorithm-4
//! ranking). Conditioning recall on the *current burst* (the workspace
//! `world_state` as a query — topic similarity over engram embeddings) is the
//! next slice, when embeddings flow through. The faculty seam does not change
//! when that lands: the backend behind `contribute` gets smarter, the brain is
//! unchanged.

use std::sync::Arc;

use async_trait::async_trait;
use futures_util::future::{join, join_all};
use uuid::Uuid;

use super::embedding::{cosine_similarity, EmbeddingProvider};
use super::working_memory::WorkingMemory;
use super::workspace::{Contribution, Faculty, FacultyId, Workspace};
use crate::persona::admission_state::AdmissionState;
use crate::persona::engram::{Engram, EngramOrigin};

/// Hard safety ceiling on memories surfaced per tick, regardless of model size —
/// the [`RecallBudget`] picks the ACTUAL count (smaller) within this. `with_limit`
/// can lower it for tests/bench. Generous: the budget, not this, is the real
/// limiter on a live persona.
const DEFAULT_RECALL_LIMIT: usize = 16;

/// How many consecutive turns the sticky recall rendering may hold its bytes
/// against LOW-RANK set churn before a refresh re-renders with accumulated
/// novelty (a NEW TOP memory always refreshes immediately, countdown or not).
/// The exploitation half of the ladder's promotion mix, applied at the recall
/// tier; deliberately small so staleness is bounded at a few acts. Tunable as
/// the rent ledger accumulates ("tune it as we go", 2026-09-01).
const RECALL_STICKY_TURNS: u8 = 3;

/// Fallback absolute cosine floor handed to the default [`SignificanceRanker`]
/// for an UNCALIBRATED embedding space only. An absolute floor can never be the
/// primary gate: it is calibrated against one embedder's similarity distribution
/// and silently breaks under another's — glass-boxed live 2026-07-10: under the
/// neural Qwen3-Embedding (measured unrelated-null μ=0.304), this 0.15 floor sat
/// 1.8σ BELOW what unrelated texts score, filtered NOTHING, and
/// saturated-salience room chatter surfaced identically on 11/11 different
/// coding-exam tasks. The primary gate lives in the [`RecallRanker`] adapter
/// (significance vs the MEASURED null).
const RECALL_RELEVANCE_FLOOR: f32 = 0.15;

/// Recall count budgeted by the served model's capability, proxied by its context
/// window (the metric the registry reliably carries today; param-size feeds in via
/// #74 when the Model row hydrates it). A small model juggles fewer working items,
/// so it gets fewer memories — burying a 4B under a dozen recalls is what collapsed
/// its coding (clean 22/30 → wrapped 4/30, 2026-06-27). `0` = window unknown
/// (harness) → the historical default of 5, unchanged.
fn recall_count_for_window(context_window: u32) -> usize {
    match context_window {
        0 => 5,
        1..=8_191 => 3,        // ~4B served tight (e.g. 4096)
        8_192..=32_767 => 5,   // mid (8–32k)
        32_768..=131_071 => 8, // large local (e.g. 14B @ 32k+)
        _ => 12,               // cloud-class windows
    }
}

/// The per-tick recall budget: how many memories may surface, the closest-match
/// floor below which a candidate is dropped as topically-irrelevant noise, and the
/// token ceiling recall may not exceed in the served window. Derived from the
/// model's served capability — a small model is not buried under memory it cannot
/// hold, and recall never eats the window. See [`recall_count_for_window`],
/// [`RECALL_RELEVANCE_FLOOR`], [`ContextBudget::recall_tokens`].
struct RecallBudget {
    max_count: usize,
    token_ceiling: usize,
}

impl RecallBudget {
    fn for_window(context_window: u32) -> Self {
        let token_ceiling = if context_window == 0 {
            usize::MAX // window unknown → don't token-gate (count + floor still apply)
        } else {
            crate::cognition::context_budget::ContextBudget::from_window(context_window)
                .recall_tokens()
                .max(MIN_RECALL_TOKENS)
        };
        Self {
            max_count: recall_count_for_window(context_window),
            token_ceiling,
        }
    }
}

/// Floor on the recall token ceiling so even a tiny served window fits at least a
/// memory or two — below this, recall would be silently empty.
// context-budget-exempt: a FLOOR under the recall allocation — it only ever raises a budget, never caps one, so a big window is never clamped by it
const MIN_RECALL_TOKENS: usize = 256;

/// Cheap token estimate for one surfaced memory: the body plus the `- …\n` list
/// framing (padded generously — the score annotation it once covered is no longer
/// model-visible), at the ~4-chars-per-token rule of thumb. Used only to keep
/// recall within its window budget — an estimate, not a tokenizer.
fn estimate_recall_tokens(content: &str) -> usize {
    (content.chars().count() + 24) / 4 + 1
}

/// When relevance re-ranking is active, over-fetch this many × the surface limit
/// as candidates, then narrow by relevance. Over-fetch so a topically-relevant
/// but lower-salience memory can still enter the running and win the re-rank.
const RERANK_CANDIDATE_MULTIPLIER: usize = 4;

/// Default blend weight for relevance (cosine vs the burst) against the memory's
/// salience-decay score: `weight·rel + (1-weight)·salience`. 0.5 = equal voice.
/// Configurable per RecallFaculty via [`RecallFaculty::with_relevance_weight`] so
/// the replay A/B bench can sweep 0.0 (pure salience = old behaviour) → 1.0 (pure
/// relevance) and DIFF the resulting traces — tuning by evidence, not guessing.
pub const DEFAULT_RELEVANCE_WEIGHT: f32 = 0.5;

/// The query recall conditions on — the CURRENT stimulus, not the whole burst.
/// Live, `world_state` is the full room transcript; embedding ALL of it dilutes
/// relevance with unrelated chatter, so a salient memory matching the NOISE beats
/// the memory matching the actual message (observed live 2026-06-22; pinned by
/// `relevance_survives_a_noisy_burst_query`). Focus on the most-recent message: the
/// last non-empty, non-room-header line, with a `[t=...]` timestamp prefix stripped.
/// A single-line `world_state` (a tidy query) returns unchanged — backward compatible.
/// Structural extraction (most-recent line), not content interpretation.
///
/// Annotation lines are never the stimulus: the burst carries bracket-tagged
/// STRUCTURAL annotations (`[room …]` headers, the `[pattern]` repetition
/// brick) alongside `[t=…]`-stamped messages. Glass-boxed 2026-07-10: a
/// directed gateway question arrived with the `[pattern]` brick appended
/// after it, the brick became the recall query, and recall surfaced
/// spiral/work memories instead of the asked-about fact (the live-room
/// query-dilution failure; the controlled eval passed because its burst was
/// only the question). The rule is bracket-tag GRAMMAR — a line starting
/// with `[` is an annotation unless it is a `[t=…]` message stamp — never
/// content inspection.
fn focused_query(world_state: &str) -> &str {
    let line = world_state
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && (!l.starts_with('[') || l.starts_with("[t=")))
        .last()
        .unwrap_or(world_state.trim());
    if let Some(rest) = line.strip_prefix("[t=") {
        if let Some(close) = rest.find("] ") {
            return rest[close + 2..].trim();
        }
    }
    line
}

/// Wall-clock seam — injectable so tests are deterministic. Returns ms since
/// the unix epoch, matching the `now_ms()` convention used across cognition.
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// The default wall clock (ms since unix epoch).
fn wall_clock() -> Clock {
    Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    })
}

/// The hippocampus, exposed as a `Faculty`. Persona-scoped: it owns an
/// `Arc<AdmissionState>` (shared with the admission pipeline so encoding and
/// recall see the same store). Perception tier — bids in phase 1.
pub struct RecallFaculty {
    persona_id: Uuid,
    admission_state: Arc<AdmissionState>,
    limit: usize,
    clock: Clock,
    /// Optional relevance re-ranker. When set, recall surfaces the memory most
    /// RELEVANT to the current burst (cosine, blended with salience), not just
    /// the most salient/recent — the "memory works as designed at scale" path.
    /// `None` → pure salience×recency (the backwards-compatible default). The
    /// backend is swappable (lexical bootstrap now; neural local embedder later).
    embedder: Option<Arc<dyn EmbeddingProvider>>,
    /// Relevance blend weight (0.0 = pure salience×recency, 1.0 = pure cosine
    /// relevance). Only used when `embedder` is set. Configurable so the replay
    /// A/B bench can sweep it; defaults to [`DEFAULT_RELEVANCE_WEIGHT`].
    relevance_weight: f32,
    /// The served model's context window in tokens — the capability metric the
    /// [`RecallBudget`] scales the recall count + token ceiling by (a 4B served
    /// tight gets fewer memories than a cloud-class window). `0` → window unknown
    /// (harness) → the historical default count, no token gate. Threaded from
    /// [`PersonaBrainConfig::context_window`] via `with_context_window`.
    context_window: u32,
    /// The ranking ADAPTER (see [`crate::cognition::recall_ranker`]): which
    /// candidates surface and in what order. `None` → a default
    /// [`SignificanceRanker`] built per tick from `relevance_weight` (impl A,
    /// statistical). A trained ranker (impl B) is injected via
    /// [`with_ranker`](Self::with_ranker) and A/B'd on the replay bench.
    ranker: Option<Arc<dyn crate::cognition::recall_ranker::RecallRanker>>,
    /// The recency channel (working memory), when shared in. Recall is the SEMANTIC
    /// channel for the broader past; the recency channel owns what the persona's
    /// hands JUST did ([[act-results-need-a-recency-channel-not-semantic-recall]]).
    /// When set, recall drops any engram whose content the working-memory head
    /// already carries — so a just-happened act is not shown twice in one prompt
    /// (its head in `[working-memory]`, its full body in `[recall]`), which wasted
    /// prefill on a byte-for-byte-overlapping block. Once the act ages out of the
    /// small working-memory window, recall surfaces the full engram again: a clean
    /// recency→semantic handoff, never both at once. `None` → no dedup (harness /
    /// backward-compatible).
    working_memory: Option<Arc<WorkingMemory>>,
    /// The focus→thresholds junction (docs/cognition/FOCUS-AS-ATTENTION-
    /// TEMPERATURE.md). Default = [`CalibratedConstants`] — a behavioral no-op
    /// until something moves the dial (recipe defaults, `focus/nudge`, or a
    /// learned policy — same seam for all three). The SCALAR it consumes is
    /// read per tick from the persona focus kernel
    /// ([`crate::persona::focus::registry`]) — one home for focus state.
    focus_policy: Arc<dyn crate::cognition::focus_policy::FocusPolicy>,
    /// STICKY RENDER CACHE (rung 3): per room, the last surfaced set (ids, in
    /// order), its rendered block, and the refresh countdown. The block is
    /// reused byte-identical under the hysteresis rules at the reuse site —
    /// killing the mutation sources (re-scored ordering, ticking age labels)
    /// the rent ledger attributed warm KV breaks to, with staleness bounded at
    /// [`RECALL_STICKY_TURNS`] turns and a new TOP memory always fresh. Tiny:
    /// N-rooms entries of a few KB each, per persona.
    sticky: parking_lot::Mutex<std::collections::HashMap<Uuid, (Vec<Uuid>, String, u8)>>,
}

impl RecallFaculty {
    /// Construct with the default recall limit and wall clock, no re-ranker.
    pub fn new(persona_id: Uuid, admission_state: Arc<AdmissionState>) -> Self {
        Self {
            persona_id,
            admission_state,
            limit: DEFAULT_RECALL_LIMIT,
            clock: wall_clock(),
            embedder: None,
            relevance_weight: DEFAULT_RELEVANCE_WEIGHT,
            context_window: 0,
            ranker: None,
            working_memory: None,
            focus_policy: Arc::new(crate::cognition::focus_policy::CalibratedConstants),
            sticky: parking_lot::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Inject a focus policy (formula or learned). The junction where "focus
    /// cleans up the RAG": at a high kernel scalar the recall significance bar
    /// rises so only exceptional memories intrude.
    pub fn with_focus_policy(
        mut self,
        policy: Arc<dyn crate::cognition::focus_policy::FocusPolicy>,
    ) -> Self {
        self.focus_policy = policy;
        self
    }

    /// Inject a ranking adapter (a trained reranker, or a tuned statistical one).
    /// Default (`None`) = [`SignificanceRanker`] at this faculty's
    /// `relevance_weight` — implementation A, the permanent null model any
    /// learned ranker must beat on the replay bench before shipping.
    pub fn with_ranker(
        mut self,
        ranker: Arc<dyn crate::cognition::recall_ranker::RecallRanker>,
    ) -> Self {
        self.ranker = Some(ranker);
        self
    }

    /// Override the hard safety ceiling on memories surfaced per tick. The
    /// [`RecallBudget`] still picks the actual count within this — `with_limit`
    /// only lowers the cap (tests / bench).
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit.max(1);
        self
    }

    /// Thread the served model's context window (tokens) so the [`RecallBudget`]
    /// scales the recall count + token ceiling by the model's capability. The
    /// live/eval spawn path sets this from [`PersonaBrainConfig::context_window`]
    /// (single-sourced, task #50). Unset → `0` → historical default count.
    pub fn with_context_window(mut self, context_window: u32) -> Self {
        self.context_window = context_window;
        self
    }

    /// Inject a deterministic clock (tests / replay).
    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    /// Install a relevance re-ranker (embedding similarity vs the burst). Recall
    /// then surfaces the most relevant memory, not just the most recent.
    pub fn with_embedder(mut self, embedder: Arc<dyn EmbeddingProvider>) -> Self {
        self.embedder = Some(embedder);
        self
    }

    /// Set the relevance blend weight (0.0 = pure salience×recency, 1.0 = pure
    /// cosine relevance to the burst), clamped to `[0,1]`. The replay A/B bench
    /// sweeps this to find where the salience↔relevance trade-off lands.
    pub fn with_relevance_weight(mut self, weight: f32) -> Self {
        self.relevance_weight = weight.clamp(0.0, 1.0);
        self
    }

    /// The current relevance blend weight (for the bench / introspection).
    pub fn relevance_weight(&self) -> f32 {
        self.relevance_weight
    }

    /// Share in the recency channel (working memory) so recall can drop an engram the
    /// working-memory head already carries — the just-happened act appears once (its
    /// head, in `[working-memory]`), not twice (also its full body in `[recall]`). See
    /// [`working_memory`](Self::working_memory) for the recency→semantic handoff.
    pub fn with_working_memory(mut self, working_memory: Arc<WorkingMemory>) -> Self {
        self.working_memory = Some(working_memory);
        self
    }

    /// The persona this faculty recalls for.
    pub fn persona_id(&self) -> Uuid {
        self.persona_id
    }
}

/// Strip the monotonic `[action #N] ` recency stamp that `WorkingMemory::record_action`
/// prepends, returning the raw observation head the entry carries. The head is the
/// first `WM_ACTION_HEAD_CHARS` of the SAME observation string whose full form is the
/// admitted engram content (see `act_observe::apply_act`), so the stripped body is an
/// exact PREFIX of that engram's content — which makes the dedup below a `starts_with`
/// test, not a fuzzy match. A non-action reasoning trace (no stamp) is returned
/// unchanged; it won't prefix an act engram, so it never spuriously dedups.
fn strip_action_stamp(entry: &str) -> &str {
    entry
        .strip_prefix("[action #")
        .and_then(|rest| rest.find("] ").map(|i| &rest[i + 2..]))
        .unwrap_or(entry)
        .trim()
}

/// #166 near-duplicate collapse: are two surfaced memories "the same thing restated"?
/// After collapsing whitespace + lowercasing, they are if one is a prefix of the other
/// OR they share an identical head of [`NEAR_DUP_HEAD_CHARS`]. This is what keeps three
/// near-identical own turns ("I ran `code/tree` to explore the workspace…" with
/// different tails) from taking every recall slot with one restated thought. Deliberately
/// conservative — genuinely distinct memories almost never share a 48-char identical head
/// — so a distinct memory is never collapsed. Char-boundary safe (operates on chars,
/// not bytes).
// context-budget-exempt: length of a dedup KEY prefix (near-duplicate detection), never text shown to the model
const NEAR_DUP_HEAD_CHARS: usize = 48;

fn recall_near_duplicate(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> String {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }
    let (na, nb) = (norm(a), norm(b));
    if na.is_empty() || nb.is_empty() {
        return false;
    }
    if na.starts_with(&nb) || nb.starts_with(&na) {
        return true;
    }
    let head = |s: &str| s.chars().take(NEAR_DUP_HEAD_CHARS).collect::<String>();
    let (ha, hb) = (head(&na), head(&nb));
    ha.chars().count() >= NEAR_DUP_HEAD_CHARS
        && hb.chars().count() >= NEAR_DUP_HEAD_CHARS
        && ha == hb
}

/// Minimum stripped-body length for a working-memory entry to gate recall dedup. A
/// trivially-short trace (e.g. an empty or one-token action) is too generic to safely
/// prefix-match a distinct engram, so it never suppresses recall — only a substantive
/// act head does.
// context-budget-exempt: minimum body length for a dedup KEY to be meaningful, never text shown to the model
const WM_DEDUP_MIN_BODY_CHARS: usize = 24;

/// How many leading chars of an engram's body must appear verbatim in the
/// current burst for the memory to count as "already in the window" (burst
/// dedup). Long enough that containment means the SAME text, short enough to
/// survive the window's own trimming of a long message's tail.
const BURST_DEDUP_HEAD_CHARS: usize = 64;

#[async_trait]
impl Faculty for RecallFaculty {
    fn id(&self) -> FacultyId {
        FacultyId::Recall
    }

    // Perception tier (reacts_to_broadcast() == false, the default): recall
    // reacts to the raw world-state, bidding its memories into phase 1 so the
    // deliberation faculty can condition on them in phase 2.
    async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
        let now = (self.clock)();

        // The per-tick recall budget, scaled by the served model's capability
        // (context window). Computed BEFORE the fetch so the over-fetch covers the
        // budget's count, not a flat constant — a cloud-class window can surface
        // more than the historical 5, a tight 4B fewer.
        let budget = RecallBudget::for_window(self.context_window);
        let surface_count = budget.max_count.min(self.limit);

        // Fetch candidates WITHOUT recording hits — we record the hit on what we
        // actually SURFACE (below), not on candidates that lose the re-rank.
        // Over-fetch when re-ranking so a relevant-but-lower-salience memory can
        // still win.
        let fetch_n = if self.embedder.is_some() {
            surface_count
                .saturating_mul(RERANK_CANDIDATE_MULTIPLIER)
                .max(surface_count)
        } else {
            surface_count
        };
        let candidates = self.admission_state.recall_candidates(now, fetch_n);
        if candidates.is_empty() {
            return None;
        }

        // Score: (blended_score, engram, salience, relevance). With an embedder,
        // blended_score mixes cosine-relevance-to-the-burst with salience — so
        // recall ORDERS by the RELEVANT memory, not just the salient/recent one —
        // and `relevance` is retained SEPARATELY so the closest-match floor can
        // gate on it (a high-salience but topically-unrelated nag blends well above
        // any sane blended floor, yet its cosine is ~0 and must be dropped).
        // Without an embedder there is no relevance signal: blended_score IS
        // salience (candidates already in that order) and relevance is 1.0 so the
        // floor never fires (pure salience×recency, backward-compatible).
        // The 4th element is `passes_gate`: did this candidate clear the relevance
        // gate? The DECISION lives behind the [`RecallRanker`] adapter seam —
        // implementation A ([`SignificanceRanker`]) gates by significance vs the
        // space's MEASURED unrelated-null and blends for ordering; a trained
        // ranker (learned head over the same features, rustc-graded labels) slots
        // in via [`with_ranker`](Self::with_ranker) and is A/B'd on the replay
        // bench against A before shipping. No embedder → no relevance signal →
        // everything passes (pure salience×recency, unchanged).
        let null = self.embedder.as_ref().and_then(|e| e.unrelated_null());
        let scored: Vec<(f32, Engram, f32, bool, f32)> = match &self.embedder {
            Some(embedder) => {
                // Embed the query AND every candidate CONCURRENTLY. Each embed is an
                // independent IO future on the neural/grid backend; awaiting them in
                // a serial loop stalled the whole tick on N sequential round-trips —
                // a starved-FIFO blocker that grew with the over-fetch multiplier.
                // `join` races the query against the candidate batch as one organic
                // unit; the cache still collapses repeats to a sync hit.
                let query_fut = embedder.embed(focused_query(&ws.world_state));
                let cand_futs =
                    join_all(candidates.iter().map(|(e, _)| embedder.embed(&e.content)));
                let (query, cand_embeds) = join(query_fut, cand_futs).await;
                // Rank + gate through the adapter (content never crosses the seam —
                // embeddings and usage signals only, so no ranker CAN regress to
                // string matching).
                let ranker_cands: Vec<crate::cognition::recall_ranker::RecallCandidate<'_>> =
                    cand_embeds
                        .iter()
                        .zip(candidates.iter())
                        .map(|(emb, (_, salience))| {
                            crate::cognition::recall_ranker::RecallCandidate {
                                embedding: emb.as_slice(),
                                salience: *salience,
                            }
                        })
                        .collect();
                let ranker: Arc<dyn crate::cognition::recall_ranker::RecallRanker> =
                    self.ranker.clone().unwrap_or_else(|| {
                        // The significance bar comes from the focus junction:
                        // σ(focus), anchored so NEUTRAL == the conventional 3σ.
                        let mut r = crate::cognition::recall_ranker::SignificanceRanker::new(
                            self.relevance_weight,
                            RECALL_RELEVANCE_FLOOR,
                        );
                        // The scalar comes from HER focus kernel, per tick —
                        // peek only (a persona who never touched focus reads
                        // the resting setpoint; no state is created for her).
                        let scalar = crate::persona::focus::registry()
                            .get(&self.persona_id)
                            .map(|h| {
                                h.lock()
                                    .expect("focus state mutex poisoned by a prior panic")
                                    .focus()
                            })
                            .unwrap_or(crate::persona::focus::RESTING_FOCUS);
                        r.sigma = self.focus_policy.recall_sigma(scalar);
                        Arc::new(r)
                    });
                let verdicts = ranker
                    .rank(
                        &query,
                        &ranker_cands,
                        crate::cognition::recall_ranker::SpaceCalibration {
                            unrelated_null: null,
                        },
                    )
                    .await;
                let mut s: Vec<(f32, Engram, f32, bool, f32)> = candidates
                    .into_iter()
                    .zip(verdicts)
                    .map(|((engram, salience), v)| {
                        (v.score, engram, salience, v.passes, v.attention_bid)
                    })
                    .collect();
                s.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
                s
            }
            None => candidates
                .into_iter()
                .map(|(engram, salience)| (salience, engram, salience, true, salience))
                .collect(),
        };

        // Apply the budget: the relevance gate (standout / small-pool floor,
        // computed above), capability-scaled count, and a context-window token
        // ceiling. Ordered by blended score, so we take the most relevant+salient
        // first; the gate uses `continue` (not `break`) because a low-relevance
        // high-salience nag can rank above a genuinely relevant memory under the
        // blend, and we want to skip the nag and keep scanning for the relevant one.
        let mut surfaced: Vec<(f32, Engram, f32, f32)> = Vec::with_capacity(surface_count);
        let mut used_tokens = 0usize;
        for (blended, engram, salience, passes_gate, attention_bid) in scored.into_iter() {
            if surfaced.len() >= surface_count {
                break;
            }
            if !passes_gate {
                continue; // does not stand out from the pool — junk in the prompt
            }
            // #166: skip a memory that merely RESTATES one already surfaced this tick.
            // The persona's own near-identical recent turns (e.g. three "I ran
            // `code/tree` to explore the workspace…" with different tails) otherwise
            // take every recall slot with one restated thought, drowning distinct
            // knowledge — and recall-hit uplift then entrenches the restatement. Keep
            // the highest-blend copy (we scan best-first) and `continue` scanning for a
            // genuinely different memory, mirroring the `passes_gate` skip above.
            if surfaced
                .iter()
                .any(|(_, e, _, _)| recall_near_duplicate(&e.content, &engram.content))
            {
                continue;
            }
            let est = estimate_recall_tokens(&engram.content);
            // Always admit the first qualifying memory (don't starve recall on a
            // tiny window); after that, stop before the token ceiling is breached.
            if !surfaced.is_empty() && used_tokens + est > budget.token_ceiling {
                break;
            }
            used_tokens += est;
            surfaced.push((blended, engram, salience, attention_bid));
        }

        // Recency→semantic handoff: drop any surfaced engram the recency channel
        // (working memory) ALREADY carries this tick. The persona's own just-happened
        // act lands in BOTH tiers by design — an 800-char head in working memory, the
        // full result-as-memory as an engram (`act_observe::apply_act`). Surfacing
        // both in one prompt shows the act twice (head in `[working-memory]`, full
        // body in `[recall]`) and burns prefill on a byte-overlapping block. The
        // recency channel owns what just happened; recall is for the broader past
        // ([[act-results-need-a-recency-channel-not-semantic-recall]]). The WM head is
        // an exact prefix of the engram content, so `starts_with` is precise — and
        // once the act ages out of the small WM window, recall surfaces the full
        // engram again. Done AFTER budgeting so we never record a recall hit (below)
        // on a memory we drop here.
        if let Some(wm) = &self.working_memory {
            let recent = wm.recent();
            let wm_bodies: Vec<&str> = recent
                .iter()
                .map(|e| strip_action_stamp(e))
                .filter(|b| b.len() >= WM_DEDUP_MIN_BODY_CHARS)
                .collect();
            if !wm_bodies.is_empty() {
                surfaced.retain(|(_, engram, _, _)| {
                    !wm_bodies.iter().any(|b| engram.content.starts_with(b))
                });
            }
        }
        // BURST DEDUP (rung 3c, the ledger's third same-evening verdict,
        // 2026-09-01 night): relevance-ranking against the current burst
        // surfaces the engrams OF the messages already sitting in the window —
        // recall re-serving the conversation to itself, 2k tokens below the
        // original. It cost three ways at once: duplicated content pays rent
        // twice; the recall top tracked the window's bleeding edge, so it
        // changed EVERY turn and the sticky cache never engaged (0 fires in
        // two live windows); and the block stopped being what recall is FOR —
        // the broader past. Same shape as the WM dedup above, one level up:
        // an engram whose (stamp-stripped) head the burst already contains is
        // already perceived — drop it from recall. Head-window containment,
        // char-boundary safe, zero allocation per check.
        if !ws.world_state.is_empty() {
            surfaced.retain(|(_, engram, _, _)| {
                let body = strip_action_stamp(&engram.content);
                if body.len() < WM_DEDUP_MIN_BODY_CHARS {
                    return true; // short memories: containment is coincidence-prone
                }
                let head_end = body
                    .char_indices()
                    .nth(BURST_DEDUP_HEAD_CHARS)
                    .map(|(i, _)| i)
                    .unwrap_or(body.len());
                !ws.world_state.contains(&body[..head_end])
            });
        }
        let scored = surfaced;

        // Nothing cleared the closest-match floor — surface nothing rather than
        // pad the prompt with irrelevant memory (the bug this fixes: 5 salience-1.0
        // but cosine-~0 nags polluting every turn).
        if scored.is_empty() {
            return None;
        }

        // Close the loop on what we ACTUALLY surface — Hebbian rehearsal on the
        // memories the persona truly used this tick (uplift + persistence).
        let surfaced_ids: Vec<Uuid> = scored.iter().map(|(_, e, _, _)| e.id).collect();
        self.admission_state.record_recall_hits(&surfaced_ids, now);

        // RTOS probe at the hippocampus seam: WHAT query conditioned recall and
        // WHAT won, with the scores the model never sees. This is how a
        // wrong-memory-surfaced bug is diagnosed from the log (glass-boxed live
        // 2026-07-10: 11 different coding exam tasks each recalled the identical
        // stale room imperative — without this probe the ranking was a black box).
        tracing::info!(
            probe_class = "recall.surface",
            persona_id = %self.persona_id,
            query = %focused_query(&ws.world_state).chars().take(90).collect::<String>(),
            embedder = self.embedder.is_some(),
            relevance_weight = self.relevance_weight,
            null_mean = null.map(|(m, _)| m).unwrap_or(f32::NAN),
            null_std = null.map(|(_, s)| s).unwrap_or(f32::NAN),
            surfaced = %scored
                .iter()
                .map(|(blended, e, sal, _)| format!(
                    "[blend={blended:.3} sal={sal:.3} | {}]",
                    e.content.chars().take(60).collect::<String>().replace('\n', " ")
                ))
                .collect::<Vec<_>>()
                .join(" "),
            "recall surfaced {} memor{}",
            scored.len(),
            if scored.len() == 1 { "y" } else { "ies" }
        );

        // The faculty's bid salience: max(blended score, intrinsic_salience). The
        // `.max` removes the regression where a lexically-thin burst (cosine ≈ 0)
        // would halve recall's bid to 0.5·salience and silently under-weight it
        // in the arbiter — recall now never bids BELOW the surfaced memory's own
        // salience, and a highly-relevant hit can bid ABOVE it. (`f32::max` also
        // returns the finite operand if the other is NaN — defensive.)
        // The faculty's workspace bid: attention honors EVIDENCE. The ranker maps
        // each surfaced memory's significance to a bid (Φ(z) for the statistical
        // ranker — the probability the similarity is not noise); the block bids at
        // the STRONGEST surfaced evidence, so a 5.5σ memory holds its seat against
        // the 0.9 standing-framing floor instead of being evicted from the prompt
        // (the utilization failure this fixes: she answered "I don't know" about a
        // fact her hippocampus HAD surfaced — glass-boxed live 2026-07-10, #130).
        // Floored by the historical max(blend, salience) so the no-embedder and
        // uncalibrated paths keep their exact prior bids.
        let top_salience = scored
            .iter()
            .map(|(_, _, _, bid)| *bid)
            .fold(scored[0].0.max(scored[0].2), f32::max)
            .clamp(0.0, 1.0);
        // The memory line the MODEL sees carries no internal score — glass-boxed
        // live 2026-07-09: Anwen broadcast "...productive sessions together!
        // (salience 0.99)", parroting the annotation straight from her prompt.
        // Scores stay in probes/captures/introspection (where the debugger reads
        // them), never in the model-visible rendering ([[px-persona-experience-tools-as-good-ux]]).
        //
        // But PROVENANCE is not a score — it is what makes a memory READ as a
        // memory. Glass-boxed live 2026-07-10 (silver-harbor): the block rendered
        // as bare unattributed quotes, three of five being the QUESTION itself,
        // so the natural completion of the pattern "this question keeps being
        // asked with no answer visible" was "I don't know" — about a fact sitting
        // two lines below, in the same block. The engram KNOWS who spoke and
        // when ([`EngramOrigin`]); dropping that at render time was the positron
        // failure (right data, hostile projection — Joel: "positron in coding
        // view ought to be more accommodating and ergonomic or we have failed").
        // Structural attribution only: origin variant + speaker identity + age —
        // never content inspection ([[no-hardcoded-heuristics-to-steer-cognition]]).
        // STICKY RENDERING (mind-major spine — compression-ladder rung 3, driven
        // by the rent ledger's FIRST verdict, 2026-09-01: warm turns break in
        // `grounding`, and recall is that run's churner). Two byte-mutation
        // sources hide in a re-render of the SAME memories: re-scored ordering,
        // and the humanized ages ("16h ago" → "17h ago") that tick on every
        // render. So the rendered block is cached per room and reused BYTE-
        // IDENTICAL for as long as the surfaced set (ids, in order) is
        // unchanged — the ages freeze at first render, which the frame already
        // licenses ("They describe the PAST"). A set change (new memory
        // surfaced, one dropped, order moved) re-renders fresh with fresh ages.
        // Scoring, uplift, and the bid salience stay live every turn — only the
        // BYTES the model re-reads go still.
        // HYSTERESIS, not exact-match (v2, hours after v1): live rooms admit
        // new engrams every act, so "identical set" NEVER held (0 sticky fires
        // in the first live window) — the cache must tolerate LOW-RANK churn.
        // Reuse rules: (a) identical set → reuse and re-arm the countdown;
        // (b) the fresh TOP memory is already in the cached set and the
        // countdown is live → reuse (the newcomer is lower-ranked shuffle;
        // staleness bounded at RECALL_STICKY_TURNS turns); (c) a NEW TOP
        // memory always renders immediately — answering-relevance never waits
        // on cache stability. The exploration/exploitation mix of the
        // promotion doctrine, applied at the recall tier; the countdown is the
        // tunable ("tune it as we go").
        {
            let mut sticky = self.sticky.lock();
            if let Some((prev_ids, prev_content, remaining)) = sticky.get_mut(&ws.room_id) {
                let set_equal = *prev_ids == surfaced_ids;
                let top_already_cached = surfaced_ids
                    .first()
                    .is_some_and(|top| prev_ids.contains(top));
                if set_equal || (*remaining > 0 && top_already_cached) {
                    if set_equal {
                        *remaining = RECALL_STICKY_TURNS;
                    } else {
                        *remaining -= 1;
                    }
                    let reasoning = format!(
                        "recalled {} memor{} (sticky re-render — bytes held for KV reuse; refresh in {} turn(s))",
                        scored.len(),
                        if scored.len() == 1 { "y" } else { "ies" },
                        remaining,
                    );
                    return Some(
                        Contribution::context(
                            FacultyId::Recall,
                            prev_content.clone(),
                            top_salience,
                            reasoning,
                        )
                        .trailing(),
                    );
                }
            }
        }
        let now_ms = (self.clock)();
        let lines = scored
            .iter()
            .map(|(_, engram, _, _)| render_memory_line(engram, self.persona_id, now_ms))
            .collect::<Vec<_>>()
            .join("\n");
        // Label the section AS memory, inline, so a transcript-trained model reads the
        // lines below as RECOLLECTION — "I remember/recall that …" — not as statements
        // about the present. Glass-boxed 2026-07-14 (Joel, "in prose/rag form engrams
        // are more like 'I remember/recall that:'"): Atlas recalled a stale "you keep
        // failing to claim" and a 42m-old "already claimed by another peer" AS current
        // truth, contradicting his fresh successful claim, and looped. The provenance
        // prefix per line already marks WHO/WHEN; this frames the whole block so the
        // pastness is unmissable. Not a directive about what to do — just what this IS.
        let content = format!("{RECALL_MEMORY_FRAME}\n{lines}");
        self.sticky.lock().insert(
            ws.room_id,
            (surfaced_ids, content.clone(), RECALL_STICKY_TURNS),
        );
        let reasoning = format!(
            "recalled {} memor{} ({}) — salience-uplifted, loop closed",
            scored.len(),
            if scored.len() == 1 { "y" } else { "ies" },
            if self.embedder.is_some() {
                "relevance-ranked vs the burst"
            } else {
                "salience×recency"
            }
        );

        Some(
            Contribution::context(FacultyId::Recall, content, top_salience, reasoning)
                // TRAILING, not stable: recall is re-scored EVERY turn (salience ×
                // recency, new engrams admitted between turns), so its block churns
                // mid-list — measured 2026-09-01 as the system prompt mutating at
                // char ~8.5k between consecutive turns (an insert + a re-fit that
                // shrank the head 12.3k→10.7k), which invalidated the entire KV
                // prefix behind it: hit_rate 0.0 with every persona on her OWN
                // slot and the server's reuse proven perfect the same hour (test
                // prompt ×2 → cache_n 397/401). The stable_blocks doc already
                // promises "recall … is separated out as its own .trailing()
                // turns (#205)"; this makes that promise true for FacultyId::
                // Recall ([[a-mutating-system-prompt-destroys-kv-reuse-for-everything-after-it]]).
                .trailing(),
        )
    }
}

/// Inline label for the recall section: frames the lines below AS memory, so a
/// transcript-trained model reads them as "I remember/recall that …" rather than as
/// statements about the present moment (Joel, 2026-07-14). Non-directive — it says
/// what the block IS, not what to do; each line still carries its own who/when prefix.
const RECALL_MEMORY_FRAME: &str =
    "(These are my own MEMORIES — things I recall from earlier, tagged with who they \
     came from and how long ago. They describe the PAST, not necessarily the present.)";

/// Structural provenance for a rendered memory line: WHO the memory came from
/// (relative to `persona_id`) and HOW LONG AGO it was admitted — read from
/// [`EngramOrigin`] + `admitted_at_ms`, never from the content. This is the
/// positron move at the recall seam: the store already knows the provenance;
/// the projection must carry it or a fact and a bystander's question render
/// identically (the silver-harbor IDK — see the call site).
fn provenance_prefix(engram: &Engram, persona_id: Uuid, now_ms: u64) -> String {
    let age = humanize_age(now_ms.saturating_sub(engram.admitted_at_ms));
    let who = match &engram.origin {
        EngramOrigin::Chat(r) => {
            if r.sender_id == persona_id {
                "you said"
            } else {
                "heard"
            }
        }
        EngramOrigin::Airc(r) => {
            if r.sender_id == persona_id.to_string() {
                "you said"
            } else {
                "heard"
            }
        }
        EngramOrigin::Tool(_) => "you did",
        EngramOrigin::SelfReflection { .. } => "you reflected",
        // An agent-authored lesson (agent-memory bridge): "you learned" for this
        // agent's own memory, "learned" for a lesson another agent shared into the
        // corpus. Not speech, so render_memory_line never quote-wraps it.
        EngramOrigin::Agent(r) => {
            if r.agent_peer_id == persona_id {
                "you learned"
            } else {
                "learned"
            }
        }
    };
    format!("({who}, {age}) ")
}

/// One rendered memory line: provenance prefix + content, with PEER-HEARD speech
/// additionally wrapped in quotation marks. The quotes are typography, not
/// content editing — recalled speech IS a quotation, and rendering it bare
/// invites replaying it as one's own words. Verified live 2026-07-11 (#134
/// specimen 2): a persona's recall showed `- (heard, 2h ago) I see that I've
/// been repeating myself…` with honest provenance, and she still broadcast the
/// peer's first-person message verbatim as her own. Quote marks are the
/// strongest structural signal a transcript-trained model has that words belong
/// to someone else.
fn render_memory_line(engram: &Engram, persona_id: Uuid, now_ms: u64) -> String {
    let prefix = provenance_prefix(engram, persona_id, now_ms);
    let heard = matches!(
        &engram.origin,
        EngramOrigin::Chat(r) if r.sender_id != persona_id
    ) || matches!(
        &engram.origin,
        EngramOrigin::Airc(r) if r.sender_id != persona_id.to_string()
    );
    if heard {
        format!("- {prefix}\u{201c}{}\u{201d}", engram.content)
    } else {
        format!("- {prefix}{}", engram.content)
    }
}

/// Coarse human age buckets — a memory's rough distance in time, not a
/// timestamp. Coarseness is deliberate: "2h ago" orients; "7,243,118ms"
/// is noise the model would parrot.
fn humanize_age(delta_ms: u64) -> String {
    const MIN: u64 = 60_000;
    const HOUR: u64 = 60 * MIN;
    const DAY: u64 = 24 * HOUR;
    match delta_ms {
        d if d < 2 * MIN => "moments ago".to_string(),
        d if d < 2 * HOUR => format!("{}m ago", d / MIN),
        d if d < 2 * DAY => format!("{}h ago", d / HOUR),
        d => format!("{}d ago", d / DAY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (#166): the persona's own near-identical recent turns must
    // collapse to ONE recall slot, not take three — three "I ran `code/tree` to
    // explore the workspace…" with different tails otherwise drown distinct knowledge,
    // and recall-hit uplift then entrenches the restatement. Distinct memories, and
    // ones that merely share a short lead, are NEVER collapsed.
    #[test]
    fn near_identical_own_turns_collapse_but_distinct_memories_survive() {
        let a = "I ran `code/tree` to explore the workspace structure, and I see a Rust project";
        let b = "I ran `code/tree`  to explore the workspace structure, and I found several dirs"; // same 48-char head, different tail + whitespace
        let c = "The deploy pipeline went green after the 4pm fix to the auth handler";
        // Same restated thought → near-duplicate.
        assert!(recall_near_duplicate(a, b));
        // Prefix relationship → near-duplicate.
        assert!(recall_near_duplicate(
            "I ran code/tree to explore",
            "I ran code/tree to explore the whole tree"
        ));
        // Genuinely different memory → NOT collapsed.
        assert!(!recall_near_duplicate(a, c));
        // Short shared lead only (< head window, no prefix) → NOT collapsed.
        assert!(!recall_near_duplicate(
            "the cat sat on the mat",
            "the cat ran up the wall today"
        ));
        // Empty never collapses.
        assert!(!recall_near_duplicate("", a));
    }

    // what this catches: bracket-tagged perception annotations ([pattern], [room …])
    // are never the recall query — the query is the newest MESSAGE. Regression for
    // the 2026-07-10 live query-dilution: the [pattern] brick, appended after a
    // directed question, became the query and recall surfaced spiral memories
    // instead of the asked-about fact. [t=…]-stamped messages still qualify.
    #[test]
    fn focused_query_skips_annotation_lines() {
        let burst = "[room general]\n\
                     [t=100] Claude: what port does silver-harbor point to?\n\
                     [pattern] Anwen's last 9 messages repeat the same sentiment.";
        assert_eq!(
            focused_query(burst),
            "Claude: what port does silver-harbor point to?"
        );
        // A trailing plain message still wins over everything.
        let burst2 = "[room general]\nAsha: hello\nClaude: the real question";
        assert_eq!(focused_query(burst2), "Claude: the real question");
        // All-annotation burst degrades to the trimmed whole (never panics).
        assert_eq!(focused_query("[room general]"), "[room general]");
    }

    // what this catches: the rendered [recall] line carries STRUCTURAL provenance
    // (who + age from EngramOrigin, never content inspection). Regression for the
    // silver-harbor IDK (2026-07-10): bare unattributed quotes made a taught fact
    // and a bystander's question render identically, and the model completed the
    // "question repeated, no answer visible" pattern with "I don't know" — about
    // a fact two lines below in the same block.
    #[test]
    fn provenance_prefix_attributes_who_and_age_structurally() {
        let me = Uuid::new_v4();
        let peer = Uuid::new_v4();
        let now: u64 = 10 * 24 * 60 * 60 * 1000;
        let engram_from = |sender: Uuid, age_ms: u64| Engram {
            context_id: None,
            id: Uuid::new_v4(),
            kind: EngramKind::Episodic,
            content: "staging gateway is on port 58057".to_string(),
            origin: EngramOrigin::Chat(ChatMessageRef {
                message_id: Uuid::new_v4(),
                room_id: Uuid::new_v4(),
                sender_id: sender,
                posted_at_ms: now - age_ms,
                content_hash: "h".to_string(),
            }),
            recall_keys: Vec::new(),
            admitted_at_ms: now - age_ms,
            trust_state_at_admission: TrustState::ApprovedPeer,
            admission_trace_id: None,
        };
        // A peer's words render as heard; her own as said — and the age buckets
        // stay coarse and human, never raw milliseconds for the model to parrot.
        assert_eq!(
            provenance_prefix(&engram_from(peer, 3 * 60 * 60 * 1000), me, now),
            "(heard, 3h ago) "
        );
        assert_eq!(
            provenance_prefix(&engram_from(me, 5 * 60 * 1000), me, now),
            "(you said, 5m ago) "
        );
        assert_eq!(
            provenance_prefix(&engram_from(peer, 30_000), me, now),
            "(heard, moments ago) "
        );
        assert_eq!(
            provenance_prefix(&engram_from(peer, 3 * 24 * 60 * 60 * 1000), me, now),
            "(heard, 3d ago) "
        );

        // PEER-HEARD speech renders QUOTED — recalled speech is a quotation
        // (#134 specimen 2: honest provenance alone didn't stop a persona
        // broadcasting a peer's first-person recalled message as her own; quote
        // marks are the strongest structural not-your-words signal). Her OWN
        // words render bare — quoting yourself invites parroting yourself.
        assert_eq!(
            render_memory_line(&engram_from(peer, 3 * 60 * 60 * 1000), me, now),
            "- (heard, 3h ago) \u{201c}staging gateway is on port 58057\u{201d}"
        );
        assert_eq!(
            render_memory_line(&engram_from(me, 5 * 60 * 1000), me, now),
            "- (you said, 5m ago) staging gateway is on port 58057"
        );
    }
    use crate::persona::engram::{ChatMessageRef, Engram, EngramKind, EngramOrigin, TrustState};
    use crate::persona::recall_metadata::{RecallMetadata, RecallMetadataRegistry};

    /// Build a persona-scoped AdmissionState with `count` engrams, each tracked
    /// in the recall registry at a chosen salience. Mirrors the proven fixture
    /// in engram_source.rs. `last_decayed_ms = now` so a same-instant recall
    /// applies a no-op decay (multiplier ≈ 1) and the uplift is observable.
    fn fixture(count: usize, now_ms: u64) -> (Uuid, Arc<AdmissionState>, Vec<Uuid>) {
        let persona = Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap();
        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta.clone()));
        let mut ids = Vec::new();
        for i in 0..count {
            let id = Uuid::new_v4();
            ids.push(id);
            let engram = Engram {
                context_id: None,
                id,
                kind: EngramKind::Episodic,
                content: format!("memory body number {i}"),
                origin: EngramOrigin::Chat(ChatMessageRef {
                    message_id: Uuid::new_v4(),
                    room_id: Uuid::new_v4(),
                    sender_id: Uuid::new_v4(),
                    posted_at_ms: now_ms.saturating_sub((i as u64) * 60_000),
                    content_hash: format!("hash-{i}"),
                }),
                recall_keys: Vec::new(),
                admitted_at_ms: now_ms.saturating_sub((i as u64) * 60_000),
                trust_state_at_admission: TrustState::ApprovedPeer,
                admission_trace_id: None,
            };
            state.push_for_test(engram);
            recall_meta.admit(
                id,
                RecallMetadata {
                    // Increasing salience so engram 0 is NOT the most salient —
                    // proves ranking, not insertion order.
                    salience: 0.4 + (i as f32 * 0.1).min(0.5),
                    access_count: 0,
                    last_accessed_ms: 0,
                    protected_until_ms: 0,
                    last_decayed_ms: now_ms,
                },
            );
        }
        (persona, state, ids)
    }

    // what this catches: the faculty surfaces recalled memory as a context
    // Contribution under FacultyId::Recall, with salience = the top hit's score.
    #[tokio::test]
    async fn surfaces_top_salience_memory_as_context() {
        let now = 1_000_000_000;
        let (persona, state, _ids) = fixture(5, now);
        let faculty = RecallFaculty::new(persona, state).with_clock(Arc::new(move || now));
        let c = faculty
            .contribute(&Workspace::new("what's the status?"))
            .await
            .expect("recall should bid when the store is non-empty");
        assert_eq!(c.faculty, FacultyId::Recall);
        assert!(c.decision.is_none(), "recall is context, never a verdict");
        // Regression for the 2026-09-01 KV-prefix churn: recall is re-scored
        // every turn, so it must render TRAILING (nearest generation), never in
        // the cacheable system prefix it would invalidate on every mutation.
        assert!(c.trailing, "recall must ride the volatile tail, not the stable head");
        // The most salient engram (highest index in the fixture) leads.
        assert!(
            c.content.contains("memory body number 4"),
            "top-salience memory should be surfaced, got: {}",
            c.content
        );
        assert!(c.salience > 0.0);
    }

    // what this catches: burst dedup (rung 3c). A memory whose content the
    // current window ALREADY shows is re-served duplication — it must drop
    // from recall (it paid rent twice, and its per-turn novelty as the recall
    // top is why the sticky cache never engaged: 0 fires across two live
    // windows). Memories NOT visible in the burst survive; short bodies are
    // exempt (containment is coincidence-prone).
    #[tokio::test]
    async fn burst_dedup_drops_window_visible_memories_but_keeps_the_past() {
        let now = 1_000_000_000u64;
        // Own fixture: bodies long enough to clear the dedup floor (the shared
        // fixture's ~20-char bodies are deliberately exempt as too short).
        let persona = Uuid::parse_str("00000000-0000-0000-0000-000000000bbb").unwrap();
        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta.clone()));
        let body = |i: usize| {
            format!("long memory number {i} — it stretches well past the dedup floor with detail")
        };
        for i in 0..2usize {
            let id = Uuid::new_v4();
            state.push_for_test(Engram {
                context_id: None,
                id,
                kind: EngramKind::Episodic,
                content: body(i),
                origin: EngramOrigin::Chat(ChatMessageRef {
                    message_id: Uuid::new_v4(),
                    room_id: Uuid::new_v4(),
                    sender_id: Uuid::new_v4(),
                    posted_at_ms: now,
                    content_hash: format!("hash-{i}"),
                }),
                recall_keys: Vec::new(),
                admitted_at_ms: now,
                trust_state_at_admission: TrustState::ApprovedPeer,
                admission_trace_id: None,
            });
            recall_meta.admit(
                id,
                RecallMetadata {
                    salience: 0.9,
                    access_count: 0,
                    last_accessed_ms: 0,
                    protected_until_ms: 0,
                    last_decayed_ms: now,
                },
            );
        }
        let faculty = RecallFaculty::new(persona, state).with_clock(Arc::new(move || now));
        // Memory 1 appears VERBATIM in the burst: already perceived — recall
        // must not re-serve it. Memory 0 stays recallable.
        let burst = format!("Alice: earlier someone said\n{}\nAlice: next?", body(1));
        let c = faculty
            .contribute(&Workspace::new(burst.as_str()))
            .await
            .expect("the non-visible memory still bids");
        assert!(
            !c.content.contains("long memory number 1"),
            "a memory the window already shows must not be re-served: {}",
            c.content
        );
        assert!(
            c.content.contains("long memory number 0"),
            "the broader past survives dedup: {}",
            c.content
        );
    }

    // what this catches: sticky rendering (rung 3, from the rent ledger's first
    // verdict — warm KV breaks die in the recall run). Re-contributing with the
    // SAME surfaced set must return the byte-identical block even as the clock
    // ticks (age labels frozen — "16h ago" must not become "17h ago" and
    // invalidate the KV tail behind it). Admitting a NEW memory changes the set
    // and re-renders fresh.
    #[tokio::test]
    async fn sticky_recall_freezes_bytes_until_the_surfaced_set_changes() {
        use std::sync::atomic::{AtomicU64, Ordering};
        let now = 1_000_000_000u64;
        let (persona, state, _ids) = fixture(3, now);
        let tick = Arc::new(AtomicU64::new(now));
        let t2 = tick.clone();
        let faculty = RecallFaculty::new(persona, state.clone())
            .with_clock(Arc::new(move || t2.load(Ordering::Relaxed)));
        let ws = Workspace::new("what's the status?");
        let first = faculty.contribute(&ws).await.expect("recall bids");
        // The clock advances 2 hours — a naive re-render would tick every age
        // label and mutate the block's bytes.
        tick.store(now + 2 * 3600 * 1000, Ordering::Relaxed);
        let second = faculty.contribute(&ws).await.expect("recall bids again");
        assert_eq!(
            first.content, second.content,
            "unchanged set must re-render BYTE-IDENTICAL (ages frozen) — this is \
             the KV stability the rent ledger attributed warm breaks to"
        );
        assert!(second.trailing, "sticky path preserves trailing placement");
    }

    // what this catches: empty store → abstain (None), not an empty bid.
    #[tokio::test]
    async fn abstains_on_empty_store() {
        let now = 1_000_000_000;
        let (persona, state, _ids) = fixture(0, now);
        let faculty = RecallFaculty::new(persona, state).with_clock(Arc::new(move || now));
        assert!(faculty.contribute(&Workspace::new("hi")).await.is_none());
    }

    // what this catches: THE BIDIRECTIONAL LOOP — recalling a memory into the
    // workspace records a recall hit (access_count++, salience uplift). Retrieval
    // feeds back into encoding; the recalled memory is strengthened for next tick.
    // This is the half EngramSource never closed.
    #[tokio::test]
    async fn recall_closes_the_loop_uplifting_what_it_surfaces() {
        let now = 1_000_000_000;
        let (persona, state, ids) = fixture(3, now);
        let recall_meta = state.recall_metadata().clone();

        let before: Vec<(f32, u32)> = ids
            .iter()
            .map(|id| {
                let m = recall_meta.get(*id).unwrap();
                (m.salience, m.access_count)
            })
            .collect();

        let faculty = RecallFaculty::new(persona, state).with_clock(Arc::new(move || now));
        let _ = faculty.contribute(&Workspace::new("status?")).await;

        // Every surfaced engram had its access_count bumped (the hit was
        // recorded) and salience did not fall below where it started (uplift,
        // no net decay at the same instant).
        for (i, id) in ids.iter().enumerate() {
            let m = recall_meta.get(*id).unwrap();
            assert!(
                m.access_count > before[i].1,
                "access_count must rise — the recall hit closes the loop"
            );
            assert!(
                m.salience >= before[i].0,
                "salience must not fall — recall uplifts what it surfaces"
            );
        }
    }

    // what this catches: recall is a PERCEPTION-tier faculty — it bids in phase 1
    // over the raw world-state, so its memories are in the broadcast before the
    // deliberation faculty (phase 2) reasons over them.
    #[test]
    fn recall_is_perception_tier() {
        let now = 1_000_000_000;
        let (persona, state, _ids) = fixture(1, now);
        let faculty = RecallFaculty::new(persona, state);
        assert!(
            !faculty.reacts_to_broadcast(),
            "recall must bid in phase 1, not after the broadcast is assembled"
        );
    }

    // what this catches: MEMORY WORKS AS DESIGNED → coherence across turns. A
    // substantive statement ADMITTED in turn 1 (the real store path, admit()) is
    // RECALLED in a later turn — so the persona carries context forward instead of
    // amnesia each turn. Clock-controlled (recall runs moments after admit) so
    // decay doesn't floor it — the reproducible-clock pattern that the wall-clock
    // live run exposed as needed. This is the store→recall loop the conversation
    // coherence depends on.
    #[tokio::test]
    async fn memory_carries_context_across_turns() {
        use crate::persona::engram::AdmissionDecision;
        use crate::persona::types::{InboxMessage, SenderType};

        let now1 = 1_000_000_000u64;
        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta));

        // TURN 1: a decision worth remembering — stored through the real admission
        // pipeline (not a test back-door push).
        let msg = InboxMessage {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            sender_name: "Operator".to_string(),
            sender_type: SenderType::Human,
            content:
                "We decided to ship the new auth flow behind a feature flag and ramp to 10% first."
                    .to_string(),
            timestamp: now1,
            priority: 0.8,
            source_modality: None,
            voice_session_id: None,
        };
        let decision = state.admit(&msg, None).expect("admit should not error");
        assert!(
            matches!(decision, AdmissionDecision::Admit { .. }),
            "a substantive decision must be admitted to memory, got: {decision:?}"
        );

        // TURN 2 (moments later): recall must surface that decision so the persona
        // stays coherent with what was decided.
        let persona = Uuid::new_v4();
        let now2 = now1 + 5_000;
        let recall = RecallFaculty::new(persona, state).with_clock(Arc::new(move || now2));
        let c = recall
            .contribute(&Workspace::new(
                "what was our rollout plan for the auth flow again?",
            ))
            .await
            .expect("recall should surface the stored decision in a later turn");
        assert!(
            c.content.contains("feature flag"),
            "turn-2 recall must carry the turn-1 memory forward (coherence across turns); got: {}",
            c.content
        );
    }

    // what this catches: RELEVANCE BEATS RECENCY — recall with an embedder
    // surfaces the topically-relevant memory even when a MORE-salient, MORE-recent
    // but irrelevant memory exists. Without the embedder, salience wins and the
    // irrelevant memory surfaces. This is "memory works as designed at scale": as
    // memory grows, you need the RIGHT memory, not the latest. The lexical
    // embedder is the bootstrap; a neural one slots in behind the same trait.
    #[tokio::test]
    async fn relevance_beats_recency_with_embedder() {
        use crate::cognition::embedding::LexicalEmbedder;

        let now = 1_000_000_000u64;
        let query = "what was our rollout plan for the auth flow again?";

        let seed = || {
            let recall_meta = Arc::new(RecallMetadataRegistry::new());
            let state = Arc::new(AdmissionState::new(recall_meta.clone()));
            let mut mk = |content: &str, salience: f32, age_ms: u64| {
                let id = Uuid::new_v4();
                state.push_for_test(Engram {
                    context_id: None,
                    id,
                    kind: EngramKind::Episodic,
                    content: content.to_string(),
                    origin: EngramOrigin::Chat(ChatMessageRef {
                        message_id: Uuid::new_v4(),
                        room_id: Uuid::new_v4(),
                        sender_id: Uuid::new_v4(),
                        posted_at_ms: now - age_ms,
                        content_hash: "h".to_string(),
                    }),
                    recall_keys: Vec::new(),
                    admitted_at_ms: now - age_ms,
                    trust_state_at_admission: TrustState::ApprovedPeer,
                    admission_trace_id: None,
                });
                recall_meta.admit(
                    id,
                    RecallMetadata {
                        salience,
                        access_count: 0,
                        last_accessed_ms: 0,
                        protected_until_ms: 0,
                        last_decayed_ms: now,
                    },
                );
            };
            // RELEVANT to the query, but LOWER salience and OLDER:
            mk(
                "we will ship the auth flow behind a feature flag and ramp the rollout to 10%",
                0.4,
                60_000,
            );
            // IRRELEVANT, but HIGHER salience and NEWER:
            mk("lunch is at noon, someone booked the corner table", 0.6, 0);
            state
        };

        let persona = Uuid::new_v4();

        // Without a re-ranker: salience wins → the irrelevant (more salient) memory.
        let plain = RecallFaculty::new(persona, seed())
            .with_limit(1)
            .with_clock(Arc::new(move || now));
        let pc = plain.contribute(&Workspace::new(query)).await.unwrap();
        assert!(
            pc.content.contains("lunch"),
            "salience-only recall surfaces the more-salient-but-irrelevant memory; got: {}",
            pc.content
        );

        // With the relevance re-ranker: the auth-flow memory wins DESPITE lower
        // salience — recall now surfaces what's relevant to the burst.
        let smart = RecallFaculty::new(persona, seed())
            .with_limit(1)
            .with_clock(Arc::new(move || now))
            .with_embedder(Arc::new(LexicalEmbedder::new()));
        let sc = smart.contribute(&Workspace::new(query)).await.unwrap();
        assert!(
            sc.content.contains("feature flag"),
            "relevance recall must surface the auth-flow memory despite lower salience; got: {}",
            sc.content
        );
    }

    // what this catches: the LIVE recall failure mode the clean-query tests miss.
    // `contribute` embeds the WHOLE `world_state` as the query — and live, that is
    // the full room burst, not a tidy question. So a relevant fact must still
    // surface when the real question is BURIED in unrelated chatter, against
    // high-salience distractors that lexically match the NOISE. Observed live
    // (2026-06-22): taught a deploy codename, then "what's the codename?" surfaced
    // salient-but-irrelevant old prompts instead — the burst noise diluted the
    // query. RED here = recall is not robust to a noisy burst (the fix is to embed
    // a FOCUSED query, the triggering message, not the whole transcript).
    #[tokio::test]
    async fn relevance_survives_a_noisy_burst_query() {
        use crate::cognition::embedding::LexicalEmbedder;

        let now = 1_000_000_000u64;
        // The LIVE shape: the actual question buried in unrelated room chatter
        // whose words (lunch, game, coffee) match the high-salience distractors.
        // The persona reacts to the NEWEST message, so the triggering question is
        // the last line — preceded by unrelated, more-salient chatter.
        let noisy_burst = "[room 7f3a]\n\
             [t=1] alice: morning all, fresh coffee in the kitchen\n\
             [t=2] bob: did anyone catch the game last night, what a finish\n\
             [t=3] alice: reminder lunch is at noon, i booked the corner table\n\
             [t=4] joel: what's the deploy codename for our next release?";

        let seed = || {
            let recall_meta = Arc::new(RecallMetadataRegistry::new());
            let state = Arc::new(AdmissionState::new(recall_meta.clone()));
            let mut mk = |content: &str, salience: f32, age_ms: u64| {
                let id = Uuid::new_v4();
                state.push_for_test(Engram {
                    context_id: None,
                    id,
                    kind: EngramKind::Episodic,
                    content: content.to_string(),
                    origin: EngramOrigin::Chat(ChatMessageRef {
                        message_id: Uuid::new_v4(),
                        room_id: Uuid::new_v4(),
                        sender_id: Uuid::new_v4(),
                        posted_at_ms: now - age_ms,
                        content_hash: "h".to_string(),
                    }),
                    recall_keys: Vec::new(),
                    admitted_at_ms: now - age_ms,
                    trust_state_at_admission: TrustState::ApprovedPeer,
                    admission_trace_id: None,
                });
                recall_meta.admit(
                    id,
                    RecallMetadata {
                        salience,
                        access_count: 0,
                        last_accessed_ms: 0,
                        protected_until_ms: 0,
                        last_decayed_ms: now,
                    },
                );
            };
            // The RELEVANT fact — lower salience (it's not the loudest memory):
            mk(
                "the deploy codename for our next release is BLUEHERON-7",
                0.4,
                60_000,
            );
            // HIGH-salience distractors that match the burst's NOISE, not the question:
            mk("lunch is at noon, the corner table is booked", 0.9, 0);
            mk("the game last night was a great finish", 0.9, 0);
            mk("there is fresh coffee in the kitchen", 0.9, 0);
            state
        };

        let persona = Uuid::new_v4();
        let smart = RecallFaculty::new(persona, seed())
            .with_limit(1)
            .with_clock(Arc::new(move || now))
            .with_embedder(Arc::new(LexicalEmbedder::new()));
        let c = smart
            .contribute(&Workspace::new(noisy_burst))
            .await
            .expect("recall should surface a memory");
        assert!(
            c.content.contains("BLUEHERON"),
            "recall must surface the codename memory relevant to the BURIED question, \
             not a salient memory matching the burst's noise; got: {}",
            c.content
        );
    }

    // what this catches: the RANKING clutter seen live (2026-06-22) — even after
    // the focused-query fix, a relevant but lower-salience fact got INTO recall but
    // ranked BELOW a max-salience irrelevant memory (an old prompt rehearsed to
    // salience 1.0 by the recall-hit loop). With `limit 1` that means the wrong
    // memory surfaces. This pins that the relevance blend weight is high enough for
    // relevance to overcome a maxed-out salience when the query clearly matches.
    #[tokio::test]
    async fn relevance_outranks_max_salience_noise() {
        use crate::cognition::embedding::LexicalEmbedder;

        let now = 1_000_000_000u64;
        let query = "what is the staging server's secret port?";

        let seed = || {
            let recall_meta = Arc::new(RecallMetadataRegistry::new());
            let state = Arc::new(AdmissionState::new(recall_meta.clone()));
            let mut mk = |content: &str, salience: f32| {
                let id = Uuid::new_v4();
                state.push_for_test(Engram {
                    context_id: None,
                    id,
                    kind: EngramKind::Episodic,
                    content: content.to_string(),
                    origin: EngramOrigin::Chat(ChatMessageRef {
                        message_id: Uuid::new_v4(),
                        room_id: Uuid::new_v4(),
                        sender_id: Uuid::new_v4(),
                        posted_at_ms: now,
                        content_hash: "h".to_string(),
                    }),
                    recall_keys: Vec::new(),
                    admitted_at_ms: now,
                    trust_state_at_admission: TrustState::ApprovedPeer,
                    admission_trace_id: None,
                });
                recall_meta.admit(
                    id,
                    RecallMetadata {
                        salience,
                        access_count: 0,
                        last_accessed_ms: 0,
                        protected_until_ms: 0,
                        last_decayed_ms: now,
                    },
                );
            };
            // RELEVANT to the query, but LOW salience (a fresh fact, not yet rehearsed):
            mk("the staging server's secret port is 47823", 0.3);
            // IRRELEVANT, but MAXED salience (an old prompt the recall loop over-rehearsed):
            mk("run the ping tool and report the round-trip time", 1.0);
            state
        };

        let persona = Uuid::new_v4();
        let recall = RecallFaculty::new(persona, seed())
            .with_limit(1)
            .with_clock(Arc::new(move || now))
            .with_embedder(Arc::new(LexicalEmbedder::new()));
        let c = recall.contribute(&Workspace::new(query)).await.unwrap();
        assert!(
            c.content.contains("47823"),
            "a clearly-relevant fact must out-rank a maxed-salience irrelevant memory; got: {}",
            c.content
        );
    }

    // ---- The mind in action: real hippocampus → workspace → informed decision ----

    use super::super::workspace::{
        Decision, NoopWorkspaceCaptureSink, WorkspaceCaptureSink, WorkspaceCycle, WorkspaceTrace,
    };

    /// A deliberation faculty that conditions its reply on what recall surfaced.
    struct DeliberateOnRecall;
    #[async_trait]
    impl Faculty for DeliberateOnRecall {
        fn id(&self) -> FacultyId {
            FacultyId::Deliberation
        }
        fn reacts_to_broadcast(&self) -> bool {
            true
        }
        async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
            match ws.broadcast.iter().find(|c| c.faculty == FacultyId::Recall) {
                Some(mem) => {
                    // Reference the most relevant recalled MEMORY line (each starts with
                    // "- "), skipping the section's memory-frame header.
                    let first_line = mem
                        .content
                        .lines()
                        .find(|l| l.trim_start().starts_with("- "))
                        .unwrap_or("")
                        .to_string();
                    Some(Contribution::verdict(
                        Decision::Speak {
                            text: format!("Picking up the thread — I recall: {first_line}"),
                        },
                        0.92,
                        "decision conditioned on recalled memory (phase-2 over phase-1 context)",
                    ))
                }
                None => Some(Contribution::verdict(
                    Decision::Pass,
                    0.4,
                    "no memory surfaced — nothing to ground a reply on",
                )),
            }
        }
    }

    /// A capture sink that pretty-prints the full tick so we can WATCH the mind.
    struct PrintingSink;
    impl WorkspaceCaptureSink for PrintingSink {
        fn record(&self, t: &WorkspaceTrace) {
            println!("\n========== WORKSPACE TICK ==========");
            println!("world_state (the consolidated burst):\n  {}", t.world_state);
            println!("\n-- phase 1: all faculty bids (the full competition) --");
            for b in &t.bids {
                println!(
                    "  [{:<12}] salience {:.2}  {}  (why: {})",
                    b.faculty.as_str(),
                    b.salience,
                    b.content.replace('\n', " / "),
                    b.reasoning
                );
            }
            println!("\n-- assembled context the decider SAW (context_broadcast) --");
            for c in &t.context_broadcast {
                println!(
                    "  [{:<12}] {}",
                    c.faculty.as_str(),
                    c.content.replace('\n', " / ")
                );
            }
            println!("\n-- decision (output of deliberation over that context) --");
            println!("  {:?}", t.decision);
            println!("====================================\n");
        }
    }

    // what this catches: END-TO-END — a real AdmissionState hippocampus bids
    // recalled memory into phase 1; the arbiter routes it into the broadcast;
    // the deliberation faculty in phase 2 reads it and produces a Decision that
    // REFERENCES the recalled memory. Prints the full trace (run with
    // `--nocapture` to watch). This is the coherence claim, demonstrated.
    #[tokio::test]
    async fn mind_in_action_recall_informs_the_decision() {
        let now = 1_000_000_000;
        let (persona, state, _ids) = fixture(3, now);

        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(RecallFaculty::new(persona, state).with_clock(Arc::new(move || now))),
            Arc::new(DeliberateOnRecall),
        ];
        let ws = WorkspaceCycle::new(
            faculties,
            Arc::new(super::super::workspace::SalienceArbiter),
            5,
        )
        .with_capture(Arc::new(PrintingSink))
        .run("teammate asks: where did we land on the deploy?")
        .await;

        match ws.decision() {
            Some(Decision::Speak { text }) => assert!(
                text.contains("memory body"),
                "the spoken decision must be grounded in recalled memory, got: {text}"
            ),
            other => panic!("expected a recall-grounded Speak, got {other:?}"),
        }

        // Silence the unused-import lint when this module's other helpers vary.
        let _ = NoopWorkspaceCaptureSink;
    }

    // what this catches: with_relevance_weight TUNES the salience↔relevance blend
    // — weight 0.0 = pure salience (the more-salient irrelevant memory wins),
    // weight 1.0 = pure relevance (the topically-relevant memory wins). This is
    // the knob the replay A/B bench sweeps to prove recall propagates to behavior.
    #[tokio::test]
    async fn relevance_weight_tunes_the_blend() {
        use crate::cognition::embedding::LexicalEmbedder;
        let now = 1_000_000_000u64;
        let query = "what was our rollout plan for the auth flow again?";
        let seed = || {
            let recall_meta = Arc::new(RecallMetadataRegistry::new());
            let state = Arc::new(AdmissionState::new(recall_meta.clone()));
            let mut mk = |content: &str, salience: f32, age: u64| {
                let id = Uuid::new_v4();
                state.push_for_test(Engram {
                    context_id: None,
                    id,
                    kind: EngramKind::Episodic,
                    content: content.to_string(),
                    origin: EngramOrigin::Chat(ChatMessageRef {
                        message_id: Uuid::new_v4(),
                        room_id: Uuid::new_v4(),
                        sender_id: Uuid::new_v4(),
                        posted_at_ms: now - age,
                        content_hash: "h".to_string(),
                    }),
                    recall_keys: Vec::new(),
                    admitted_at_ms: now - age,
                    trust_state_at_admission: TrustState::ApprovedPeer,
                    admission_trace_id: None,
                });
                recall_meta.admit(
                    id,
                    RecallMetadata {
                        salience,
                        access_count: 0,
                        last_accessed_ms: 0,
                        protected_until_ms: 0,
                        last_decayed_ms: now,
                    },
                );
            };
            mk(
                "ship the auth flow behind a feature flag and ramp the rollout to 10%",
                0.4,
                60_000,
            );
            mk("lunch is at noon, someone booked the corner table", 0.6, 0);
            state
        };
        let persona = Uuid::new_v4();

        // weight 0.0 → pure salience → the more-salient IRRELEVANT memory.
        let salience_only = RecallFaculty::new(persona, seed())
            .with_limit(1)
            .with_clock(Arc::new(move || now))
            .with_embedder(Arc::new(LexicalEmbedder::new()))
            .with_relevance_weight(0.0);
        assert!(
            salience_only
                .contribute(&Workspace::new(query))
                .await
                .unwrap()
                .content
                .contains("lunch"),
            "weight 0.0 = pure salience → irrelevant-but-salient memory"
        );

        // weight 1.0 → pure relevance → the topically-relevant memory.
        let relevance_only = RecallFaculty::new(persona, seed())
            .with_limit(1)
            .with_clock(Arc::new(move || now))
            .with_embedder(Arc::new(LexicalEmbedder::new()))
            .with_relevance_weight(1.0);
        assert!(
            relevance_only
                .contribute(&Workspace::new(query))
                .await
                .unwrap()
                .content
                .contains("feature flag"),
            "weight 1.0 = pure relevance → topically-relevant memory"
        );
    }

    // what this catches: THE CAESAR-PROMPT CONTAMINATION BUG (2026-06-27). Live,
    // 5 memories rehearsed to salience 1.0 by the recall-hit loop but topically
    // UNRELATED to the task ("caesar cipher") polluted every prompt — blending to
    // ~0.5 (0.5·0 rel + 0.5·1.0 sal), well above any blended floor, so they slipped
    // through. The closest-match floor gates on the RELEVANCE component, not the
    // blend, so a cosine-~0 nag is dropped no matter how salient. RED before the
    // floor: the nags surface and crowd out / dilute the one relevant memory.
    #[tokio::test]
    async fn recall_drops_topically_irrelevant_high_salience_nags() {
        use crate::cognition::embedding::LexicalEmbedder;
        let now = 1_000_000_000u64;
        let query = "write a caesar cipher function";

        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta.clone()));
        let mut mk = |content: &str, salience: f32| {
            let id = Uuid::new_v4();
            state.push_for_test(Engram {
                context_id: None,
                id,
                kind: EngramKind::Episodic,
                content: content.to_string(),
                origin: EngramOrigin::Chat(ChatMessageRef {
                    message_id: Uuid::new_v4(),
                    room_id: Uuid::new_v4(),
                    sender_id: Uuid::new_v4(),
                    posted_at_ms: now,
                    content_hash: "h".to_string(),
                }),
                recall_keys: Vec::new(),
                admitted_at_ms: now,
                trust_state_at_admission: TrustState::ApprovedPeer,
                admission_trace_id: None,
            });
            recall_meta.admit(
                id,
                RecallMetadata {
                    salience,
                    access_count: 0,
                    last_accessed_ms: 0,
                    protected_until_ms: 0,
                    last_decayed_ms: now,
                },
            );
        };
        // The one RELEVANT memory, modest salience.
        mk("caesar cipher shifts each letter by fixed amount", 0.4);
        // Five high-salience nags, each topically orthogonal (no shared tokens with
        // the task) — mirrors the live contamination: code prompts ("nth prime",
        // "fibonacci") rehearsed to salience 1.0 but unrelated to the caesar task.
        mk("lunchtime sandwiches arrived early", 1.0);
        mk("soccer scores updated overnight", 1.0);
        mk("espresso machine needs descaling", 1.0);
        mk("office ferns require watering", 1.0);
        mk("garage shutters lock midnight", 1.0);

        let recall = RecallFaculty::new(Uuid::new_v4(), state)
            .with_clock(Arc::new(move || now))
            .with_embedder(Arc::new(LexicalEmbedder::new()))
            .with_context_window(8192);
        let c = recall
            .contribute(&Workspace::new(query))
            .await
            .expect("the relevant memory should surface");
        assert!(
            c.content.contains("caesar cipher shifts"),
            "the topically-relevant memory must surface; got: {}",
            c.content
        );
        for nag in ["lunch", "game", "coffee", "plants", "parking"] {
            assert!(
                !c.content.contains(nag),
                "the irrelevant high-salience nag '{nag}' must be dropped by the closest-match floor; got: {}",
                c.content
            );
        }
    }

    // what this catches: the recall COUNT scales with the served model's
    // capability (proxied by context window) — a tight 4B window surfaces fewer
    // memories than a cloud-class one, so a small model isn't buried under memory
    // it can't juggle. `0` (window unknown) keeps the historical default of 5.
    #[test]
    fn recall_count_scales_with_context_window() {
        assert_eq!(
            recall_count_for_window(0),
            5,
            "unknown window → historical default"
        );
        assert_eq!(
            recall_count_for_window(4096),
            3,
            "tight 4B window → fewer memories"
        );
        assert_eq!(recall_count_for_window(16384), 5);
        assert_eq!(recall_count_for_window(65536), 8);
        assert_eq!(
            recall_count_for_window(262144),
            12,
            "cloud-class window → more memories"
        );
        // Monotonic non-decreasing across KNOWN windows (0 is the unknown sentinel,
        // excluded — it deliberately returns the historical default, not the floor).
        let windows = [4096u32, 8192, 32768, 131072, 262144];
        for pair in windows.windows(2) {
            assert!(
                recall_count_for_window(pair[1]) >= recall_count_for_window(pair[0]),
                "count must not shrink as the window grows: {pair:?}"
            );
        }
    }

    // what this catches: the capability count BOUNDS how many memories surface even
    // when many are relevant — a tight 4B window (4096) caps recall at 3 though 10
    // equally-relevant memories qualify. This is the "limited to a reasonable
    // number by the model metric" half of the budget.
    #[tokio::test]
    async fn recall_count_is_bounded_by_a_tight_window() {
        use crate::cognition::embedding::LexicalEmbedder;
        let now = 1_000_000_000u64;
        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta.clone()));
        for i in 0..10 {
            let id = Uuid::new_v4();
            state.push_for_test(Engram {
                context_id: None,
                id,
                kind: EngramKind::Episodic,
                content: format!("auth flow rollout note {i}"),
                origin: EngramOrigin::Chat(ChatMessageRef {
                    message_id: Uuid::new_v4(),
                    room_id: Uuid::new_v4(),
                    sender_id: Uuid::new_v4(),
                    posted_at_ms: now,
                    content_hash: format!("h{i}"),
                }),
                recall_keys: Vec::new(),
                admitted_at_ms: now,
                trust_state_at_admission: TrustState::ApprovedPeer,
                admission_trace_id: None,
            });
            recall_meta.admit(
                id,
                RecallMetadata {
                    salience: 0.5,
                    access_count: 0,
                    last_accessed_ms: 0,
                    protected_until_ms: 0,
                    last_decayed_ms: now,
                },
            );
        }
        let recall = RecallFaculty::new(Uuid::new_v4(), state)
            .with_clock(Arc::new(move || now))
            .with_embedder(Arc::new(LexicalEmbedder::new()))
            .with_context_window(4096);
        let c = recall
            .contribute(&Workspace::new("auth flow rollout"))
            .await
            .expect("relevant memories should surface");
        assert_eq!(
            // Count MEMORY lines (each starts with "- "), not the section frame header.
            c.content
                .lines()
                .filter(|l| l.trim_start().starts_with("- "))
                .count(),
            3,
            "a 4096-token window caps recall at 3 memories; got:\n{}",
            c.content
        );
    }

    // what this catches: the recency→semantic handoff. The persona's own just-happened
    // act lands in BOTH tiers by design (an 800-char head in working memory, the full
    // result as an engram — act_observe::apply_act). Without the dedup, one prompt shows
    // the act twice: its head in [working-memory] AND its full body in [recall], burning
    // prefill on a byte-overlapping block. With the working-memory Arc shared in, recall
    // drops the engram whose content the working-memory head already carries (exact
    // prefix match), while an UNRELATED memory still surfaces. Regression for
    // [[act-results-need-a-recency-channel-not-semantic-recall]].
    #[tokio::test]
    async fn recall_drops_an_act_the_recency_channel_already_carries() {
        let now = 1_000_000_000u64;
        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta.clone()));

        // The persona's own action observation (the exact string apply_act admits) and
        // an unrelated long-term memory. Same salience so ordering can't hide the drop.
        let own_act =
            "I ran code/run(source=fn is_prime) because I must verify. Result: 2 prime, 4 not";
        let other = "The team agreed to ship the auth flow on Friday.";
        for content in [own_act, other] {
            let id = Uuid::new_v4();
            state.push_for_test(Engram {
                context_id: None,
                id,
                kind: EngramKind::Episodic,
                content: content.to_string(),
                origin: EngramOrigin::Chat(ChatMessageRef {
                    message_id: Uuid::new_v4(),
                    room_id: Uuid::new_v4(),
                    sender_id: Uuid::new_v4(),
                    posted_at_ms: now,
                    content_hash: format!("h-{}", content.len()),
                }),
                recall_keys: Vec::new(),
                admitted_at_ms: now,
                trust_state_at_admission: TrustState::ApprovedPeer,
                admission_trace_id: None,
            });
            recall_meta.admit(
                id,
                RecallMetadata {
                    salience: 0.7,
                    access_count: 0,
                    last_accessed_ms: 0,
                    protected_until_ms: 0,
                    last_decayed_ms: now,
                },
            );
        }

        // The recency channel carries the SAME act (its head, stamped), exactly as
        // apply_act records it after executing the Decision::Act.
        let wm = Arc::new(WorkingMemory::new(3));
        wm.record_receipt(own_act);

        // No embedder → pure salience×recency, relevance floor disabled → both would
        // surface if not for the working-memory dedup. Large window so count is not the
        // limiter.
        let recall = RecallFaculty::new(Uuid::new_v4(), state)
            .with_clock(Arc::new(move || now))
            .with_context_window(131_072)
            .with_working_memory(Arc::clone(&wm));
        let c = recall
            .contribute(&Workspace::new("is_prime check"))
            .await
            .expect("the unrelated memory still surfaces");
        assert!(
            !c.content.contains("code/run(source=fn is_prime)"),
            "recall must NOT re-surface an act the recency channel already carries; got:\n{}",
            c.content
        );
        assert!(
            c.content.contains("ship the auth flow on Friday"),
            "an unrelated long-term memory must still surface; got:\n{}",
            c.content
        );
    }

    // what this catches: with NO working memory shared in (harness / backward-compatible
    // path), recall is unchanged — the act engram still surfaces. Guards against the
    // dedup firing when it has no recency channel to defer to.
    #[tokio::test]
    async fn recall_keeps_the_act_when_no_recency_channel_is_shared() {
        let now = 1_000_000_000u64;
        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta.clone()));
        let own_act =
            "I ran code/run(source=fn is_prime) because I must verify. Result: 2 prime, 4 not";
        let id = Uuid::new_v4();
        state.push_for_test(Engram {
            context_id: None,
            id,
            kind: EngramKind::Episodic,
            content: own_act.to_string(),
            origin: EngramOrigin::Chat(ChatMessageRef {
                message_id: Uuid::new_v4(),
                room_id: Uuid::new_v4(),
                sender_id: Uuid::new_v4(),
                posted_at_ms: now,
                content_hash: "h".to_string(),
            }),
            recall_keys: Vec::new(),
            admitted_at_ms: now,
            trust_state_at_admission: TrustState::ApprovedPeer,
            admission_trace_id: None,
        });
        recall_meta.admit(
            id,
            RecallMetadata {
                salience: 0.7,
                access_count: 0,
                last_accessed_ms: 0,
                protected_until_ms: 0,
                last_decayed_ms: now,
            },
        );
        // No .with_working_memory(...) — the dedup has nothing to defer to.
        let recall = RecallFaculty::new(Uuid::new_v4(), state)
            .with_clock(Arc::new(move || now))
            .with_context_window(131_072);
        let c = recall
            .contribute(&Workspace::new("is_prime check"))
            .await
            .expect("the act memory surfaces with no recency channel");
        assert!(
            c.content.contains("code/run(source=fn is_prime)"),
            "without a working-memory channel, recall must keep the act; got:\n{}",
            c.content
        );
    }

    /// Test-only anisotropic embedding space: unrelated pairs baseline at a
    /// NONZERO cosine (like the neural Qwen3 space, measured ≈0.25–0.3), with a
    /// calibrated null. `embed` keys canned contents to fixed 2-d unit vectors —
    /// a geometry fixture, not matching logic.
    struct AnisotropicStub;

    #[async_trait]
    impl crate::cognition::embedding::EmbeddingProvider for AnisotropicStub {
        fn id(&self) -> &str {
            "test-anisotropic"
        }
        fn dim(&self) -> usize {
            2
        }
        async fn embed(&self, text: &str) -> Vec<f32> {
            if text.contains("BLUEHERON") {
                vec![0.8, 0.6] // cos vs query = 0.8 → z = (0.8−0.27)/0.04 ≈ 13 — significant
            } else if text.contains("deploy codename") {
                vec![1.0, 0.0] // the query
            } else {
                vec![0.28, 0.96] // cos vs query = 0.28 ≈ the null — NOT significant
            }
        }
        fn unrelated_null(&self) -> Option<(f32, f32)> {
            Some((0.27, 0.04)) // the measured anisotropy of this space
        }
    }

    // what this catches: the live 2026-07-10 hippocampus bug — under an
    // ANISOTROPIC space (unrelated baseline ≈0.27, NOT 0) the old absolute floor
    // (0.15 < baseline) filtered nothing, so a salience-saturated room nag
    // surfaced identically on 11/11 unrelated coding tasks. The calibrated
    // SIGNIFICANCE gate must (a) DROP null-scoring junk even at salience 0.99,
    // (b) SURFACE a true match (several σ above the null) despite lower salience,
    // and (c) surface NOTHING when no memory rejects the null — never pad the
    // prompt with the least-irrelevant memory.
    #[tokio::test]
    async fn significance_gate_beats_saturated_salience_under_anisotropic_space() {
        let now = 1_000_000_000u64;
        let query = "what was the deploy codename again?";
        let seed = |with_match: bool| {
            let recall_meta = Arc::new(RecallMetadataRegistry::new());
            let state = Arc::new(AdmissionState::new(recall_meta.clone()));
            let mut mk = |content: &str, salience: f32| {
                let id = Uuid::new_v4();
                state.push_for_test(Engram {
                    context_id: None,
                    id,
                    kind: EngramKind::Episodic,
                    content: content.to_string(),
                    origin: EngramOrigin::Chat(ChatMessageRef {
                        message_id: Uuid::new_v4(),
                        room_id: Uuid::new_v4(),
                        sender_id: Uuid::new_v4(),
                        posted_at_ms: now,
                        content_hash: "h".to_string(),
                    }),
                    recall_keys: Vec::new(),
                    admitted_at_ms: now,
                    trust_state_at_admission: TrustState::ApprovedPeer,
                    admission_trace_id: None,
                });
                recall_meta.admit(
                    id,
                    RecallMetadata {
                        salience,
                        access_count: 0,
                        last_accessed_ms: 0,
                        protected_until_ms: 0,
                        last_decayed_ms: now,
                    },
                );
            };
            // Rehearsal-saturated junk (the "enough goodbyes" class): max salience,
            // cosine AT the null.
            mk("enough goodbyes, the board has real work", 0.99);
            mk("hello, how can I assist you today", 0.99);
            mk("let me check the current task at hand", 0.99);
            if with_match {
                // The genuinely relevant memory — LOWER salience.
                mk("the codename is BLUEHERON-7", 0.4);
            }
            state
        };

        // (a)+(b): the true match surfaces; every saturated-junk memory is dropped.
        let recall = RecallFaculty::new(Uuid::new_v4(), seed(true))
            .with_clock(Arc::new(move || now))
            .with_embedder(Arc::new(AnisotropicStub));
        let c = recall
            .contribute(&Workspace::new(query))
            .await
            .expect("the significant memory must surface");
        assert!(
            c.content.contains("BLUEHERON"),
            "the true match must surface despite lower salience; got:\n{}",
            c.content
        );
        assert!(
            !c.content.contains("goodbyes") && !c.content.contains("assist you"),
            "null-scoring junk must be dropped even at salience 0.99; got:\n{}",
            c.content
        );

        // (c): with NO memory rejecting the null, recall surfaces NOTHING.
        let recall = RecallFaculty::new(Uuid::new_v4(), seed(false))
            .with_clock(Arc::new(move || now))
            .with_embedder(Arc::new(AnisotropicStub));
        assert!(
            recall.contribute(&Workspace::new(query)).await.is_none(),
            "an all-junk pool must surface nothing, never the least-irrelevant memory"
        );
    }
}
