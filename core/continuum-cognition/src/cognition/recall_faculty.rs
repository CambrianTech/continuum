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

use continuum_base::time::EpochMs;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::future::{join, join_all};
use uuid::Uuid;

use crate::ai::embedding::EmbeddingProvider;
use super::working_memory::WorkingMemory;
use super::workspace::{Contribution, Faculty, FacultyId, Workspace};
use crate::cognition::admission_state::AdmissionState;
use crate::cognition::engram::{Engram, EngramOrigin};

/// Hard safety ceiling on memories surfaced per tick, regardless of model size —
/// [`recall_count_for_window`] picks the ACTUAL count (smaller) within this.
/// `with_limit` can lower it for tests/bench.
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
pub fn focused_query(world_state: &str) -> &str {
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
/// the unix epoch, matching the `EpochMs::now().millis()` convention used across cognition.
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// The default wall clock (ms since unix epoch).
fn wall_clock() -> Clock {
    Arc::new(|| {
        EpochMs::now().millis()
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
    /// ([`crate::cognition::focus::registry`]) — one home for focus state.
    focus_policy: Arc<dyn crate::cognition::focus_policy::FocusPolicy>,
    /// STICKY RENDER CACHE (rung 3): per room, the last surfaced set (ids, in
    /// order), its rendered block, and the refresh countdown. The block is
    /// reused byte-identical under the hysteresis rules at the reuse site —
    /// killing the mutation sources (re-scored ordering, ticking age labels)
    /// the rent ledger attributed warm KV breaks to, with staleness bounded at
    /// [`RECALL_STICKY_TURNS`] turns and a new TOP memory always fresh. Tiny:
    /// N-rooms entries of a few KB each, per persona.
    sticky: parking_lot::Mutex<std::collections::HashMap<Uuid, (Vec<Uuid>, String, std::sync::Arc<[crate::cognition::rag_budget::RagItem]>, u8)>>,
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
// context-budget-exempt: a MATCH-WINDOW length for dedup containment (how many chars must appear verbatim to count as the same text), not a prompt/context size — it neither budgets nor clamps any window
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
        // How many memories, by the served model's capability (its cognitive load, measured);
        // never how many tokens: her view's one allocation prices the block and picks its
        // level (FLOW-IMPLEMENTATION-PLAN §2d), where a window share cut it before.
        let surface_count = recall_count_for_window(self.context_window).min(self.limit);

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
                        let scalar = crate::cognition::focus::registry()
                            .get(&self.persona_id)
                            .map(|h| {
                                h.lock()
                                    .expect("focus state mutex poisoned by a prior panic")
                                    .focus()
                            })
                            .unwrap_or(crate::cognition::focus::RESTING_FOCUS);
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
        crate::cognition::vitals_pulse::record_recall(self.persona_id, surfaced_ids.len() as u64);

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
            if let Some((prev_ids, prev_content, prev_units, remaining)) = sticky.get_mut(&ws.room_id) {
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
                        .with_units(prev_units.to_vec())
                        .trailing(),
                    );
                }
            }
        }
        let now_ms = (self.clock)();
        let rendered: Vec<(&Engram, String)> = scored
            .iter()
            .map(|(_, engram, _, _)| (engram, render_memory_line(engram, self.persona_id, now_ms)))
            .collect();
        let lines = rendered.iter().map(|(_, line)| line.as_str()).collect::<Vec<_>>().join("\n");
        // Each memory a unit at its levels, so her view can show the block at gists or
        // handles when it cannot hold every memory whole; the frame stays whole.
        let units: Vec<crate::cognition::rag_budget::RagItem> = std::iter::once(
            crate::cognition::rag_budget::RagItem::whole("frame", RECALL_MEMORY_FRAME, serde_json::Value::Null),
        )
        .chain(rendered.iter().map(|(engram, line)| memory_unit(engram, line)))
        .collect();
        // Label the section AS memory, inline, so a transcript-trained model reads the
        // lines below as RECOLLECTION — "I remember/recall that …" — not as statements
        // about the present. Glass-boxed 2026-07-14 (Joel, "in prose/rag form engrams
        // are more like 'I remember/recall that:'"): Atlas recalled a stale "you keep
        // failing to claim" and a 42m-old "already claimed by another peer" AS current
        // truth, contradicting his fresh successful claim, and looped. The provenance
        // prefix per line already marks WHO/WHEN; this frames the whole block so the
        // pastness is unmissable. Not a directive about what to do — just what this IS.
        let content = format!("{RECALL_MEMORY_FRAME}\n{lines}");
        let units: std::sync::Arc<[crate::cognition::rag_budget::RagItem]> = units.into();
        self.sticky.lock().insert(
            ws.room_id,
            (surfaced_ids, content.clone(), std::sync::Arc::clone(&units), RECALL_STICKY_TURNS),
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
                .with_units(units.to_vec())
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
    let age = EpochMs::from_millis(now_ms).since(EpochMs::from_millis(engram.admitted_at_ms)).ago();
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
/// One memory as a unit: its rendered line whole, its first sentence when shorter, and a
/// handle she reopens through `cognition/recall`.
fn memory_unit(engram: &Engram, line: &str) -> crate::cognition::rag_budget::RagItem {
    let id = engram.id.to_string();
    let unit = crate::cognition::rag_budget::RagItem::whole(id.clone(), line, serde_json::json!({ "engram_id": id }))
        .with_pointer(format!("- [memory {} — cognition/recall]", airc_core::shown_id::shown_form(engram.id)));
    match line.find(". ").map(|end| &line[..=end]) {
        Some(gist) if gist.len() < line.len() => unit.with_summary(gist),
        _ => unit,
    }
}

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

#[cfg(test)]
mod tests;
