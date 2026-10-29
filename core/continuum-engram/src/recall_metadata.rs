//! RecallMetadata sidecar — Algorithm 4's volatile per-engram state.
//!
//! ### Why a sidecar, not Engram fields
//!
//! Per `engram_graph.rs:136-138`'s design note + the
//! [[organization-purity-as-we-migrate]] doctrine: `Engram` is the
//! DURABLE CONTENT layer (id + kind + content + origin + admission
//! provenance). `RecallMetadata` is the VOLATILE RECALL STATE layer
//! (salience + access counts + decay timing + novelty protection).
//! They have DIFFERENT update cadences (Engram is write-once at
//! admission; RecallMetadata is written every recall hit, every
//! decay tick) and DIFFERENT persistence policies (Engram persists
//! eventually to longterm.db; RecallMetadata's L3 persistence is a
//! separate concern with its own coalescing/batching).
//!
//! Keeping them separate lets each evolve cleanly. Per CBAR's
//! event-driven separation of concerns: each layer is its own
//! subscriber/emitter with its own tick.
//!
//! ### Concurrency
//!
//! `DashMap<EngramId, RecallMetadata>` for lock-free reads on the
//! cognition hot path per [[RTOS-brain-no-region-on-hot-path]]
//! doctrine. Recall scoring (Algorithm 1+2) reads metadata for
//! every candidate engram; this MUST NOT serialize. Per-key writes
//! happen on:
//!
//! - Engram admission (initial salience + protection window write)
//! - Recall hits (access_count++, last_accessed update, salience
//!   uplift)
//! - Decay tick (salience-modulated half-life applied per the
//!   Algorithm 4 formula)
//!
//! All writes use `DashMap::entry` for atomic compare-update.
//!
//! ### What this module is NOT
//!
//! - NOT the recall scorer. Algorithm 1+2 scoring lives in a
//!   sibling module that READS RecallMetadata fields. This module
//!   exposes the data + atomic update operations only.
//! - NOT the decay tick. The actual periodic decay sweep runs in
//!   the hippocampus's sleep-policy region (per
//!   `BRAIN-REGIONS-SUBSTRATE.md`); this module exposes the
//!   `apply_decay` operation that the tick calls.
//! - NOT the persistence layer. L2-resident metadata may flush
//!   periodically to L3 longterm.db; that lives in a later slice's
//!   `RecallMetadataPersistenceModule` (event-driven, dormant-by-
//!   default, per the doctrines).
//!
//! ### Field semantics (per `COGNITION-ALGORITHMS.md` Algorithm 4)
//!
//! - `salience: f32` in `[0.0, 1.0]` — Algorithm 4's salience score.
//!   1.0 = "user marked this as important + cross-referenced
//!   heavily"; 0.0 = "barely admitted, no rehearsal." Decay
//!   half-life scales with `(1.0 + salience)^2` so high-salience
//!   engrams decay 4–9× slower than baseline.
//! - `access_count: u32` — Hebbian rehearsal counter. Incremented
//!   each time the engram is surfaced in recall AND consumed by
//!   the persona's response. "Use it or lose it."
//! - `last_accessed_ms: u64` — wallclock ms of most recent recall
//!   hit. Recency input to scoring + decay.
//! - `protected_until_ms: u64` — novelty protection window. While
//!   `now_ms < protected_until_ms`, `apply_decay` is a no-op.
//!   This implements the [[cognition-cache-hierarchy]] one-shot-
//!   protection rule (high embedding-distance outliers get a
//!   grace window to prove worth before they're forgotten).

use continuum_base::time::EpochMs;
use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use continuum_kernel::orm::{BaseEntity, Entity};

/// Per-engram volatile recall state. Cloneable + Copy because all
/// fields are primitives — recall scoring reads a cheap snapshot
/// without locking.
///
/// Serialize/Deserialize added for the persistence boundary. The
/// keyed-by-engram-id-in-DashMap hot-path shape doesn't carry
/// engram_id; the `EngramRecallMetadata` persistence sibling type
/// (below) carries the FK and converts at the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RecallMetadata {
    pub salience: f32,
    pub access_count: u32,
    pub last_accessed_ms: u64,
    pub protected_until_ms: u64,
    /// Wallclock ms of the most recent `apply_decay` call. The
    /// registry uses this to compute the actual elapsed time since
    /// the last decay tick, preventing double-decay when the sleep-
    /// region tick fires with overlapping windows. Per the
    /// substrate-is-a-good-citizen "reliable" requirement —
    /// internal invariants enforced by the data structure, not
    /// promised in docs.
    pub last_decayed_ms: u64,
}

impl Default for RecallMetadata {
    fn default() -> Self {
        Self {
            // Default initial salience — neutral, neither boosted
            // nor suppressed. Admission-time scoring (slice 7+
            // novelty detector) overwrites this for outlier
            // candidates.
            salience: 0.5,
            access_count: 0,
            last_accessed_ms: 0,
            // 0 = no protection (default for engrams admitted via
            // ordinary pathways). The novelty detector sets this
            // for outliers.
            protected_until_ms: 0,
            // Initialized when admitted so the first decay tick's
            // delta is bounded.
            last_decayed_ms: 0,
        }
    }
}

impl RecallMetadata {
    /// Whether the novelty protection window is still active.
    /// While true, `apply_decay` is a no-op.
    pub fn is_protected(&self, now_ms: u64) -> bool {
        self.protected_until_ms > now_ms
    }

    /// Compute the decay multiplier for this metadata, given a
    /// duration delta in ms.
    ///
    /// Per Algorithm 4 (COGNITION-ALGORITHMS.md line ~230):
    /// salience-1.0 has a half-life that scales by `(1 + s)^2`
    /// relative to salience-0.0 — for s=1, that's exactly 4×. We
    /// implement this as exponential decay with a salience-
    /// modulated half-life: `half_life = base * (1 + s)^2`.
    ///
    /// (Algorithm 4's source-of-truth doc mentions a 9× figure as
    /// the intuitive "high-salience persists much longer" claim;
    /// the formula it specifies actually produces 4× at s=1. Future
    /// MemoryParameterAdapter implementations may tune the
    /// exponent or base to land closer to 9× if telemetry says
    /// it's the better fit — keeping the formula honest about
    /// what it currently does.)
    ///
    /// For the base half-life we pick 1 hour as a reasonable
    /// starting heuristic per the methodology adapter pattern —
    /// future MemoryParameterAdapter implementations will tune
    /// this from telemetry. With base=1h: salience-0 decays to half
    /// every hour; salience-1 decays to half every 4 hours.
    ///
    /// Returns a multiplier in `[0.0, 1.0]` to apply to current
    /// salience. Caller multiplies its salience by this to get the
    /// decayed value.
    pub fn decay_multiplier(&self, delta_ms: u64) -> f32 {
        const BASE_HALF_LIFE_MS: f32 = 3_600_000.0; // 1 hour
        let half_life_ms = BASE_HALF_LIFE_MS * (1.0 + self.salience).powf(2.0);
        // Apply: multiplier = 0.5 ^ (delta / half_life)
        let exponent = (delta_ms as f32) / half_life_ms;
        0.5_f32.powf(exponent)
    }
}

// ── Persistence shape ────────────────────────────────────────────

/// Persistence-side representation of an engram's recall metadata.
///
/// `RecallMetadata` (above) is the hot-path in-memory value type —
/// keyed by engram_id in a `DashMap`, lock-free, Copy. For
/// persistence we need to carry the engram_id with each row, so this
/// sibling type embeds it as a foreign key. Convert at the
/// hot-path↔persistence boundary via the `From` impls.
///
/// The FK to `engrams.id` with `ON DELETE CASCADE` means: when an
/// Engram is deleted, its recall metadata row goes with it,
/// enforced at the DB layer. No application-level cleanup needed.
/// Per [[no-fallbacks-ever]] extended to relational invariants.
///
/// `engram_id` is `UNIQUE` so the 1:1 engram↔metadata invariant is
/// enforced by the DB. Attempting to insert a second metadata row
/// for the same engram is a constraint violation.
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "engram_recall_metadata")]
pub struct EngramRecallMetadata {
    #[serde(flatten)]
    pub base: BaseEntity,

    /// FK to engrams.id. UNIQUE so 1:1 engram↔metadata is enforced.
    /// ON DELETE CASCADE so removing an engram wipes its metadata
    /// row at the DB layer.
    #[entity(unique, indexed, foreign_key("engrams.id", on_delete = "cascade"))]
    pub engram_id: Uuid,

    /// Algorithm 4 salience score in `[0.0, 1.0]`. Indexed: recall
    /// scoring filters/sorts by salience.
    #[entity(indexed)]
    pub salience: f32,

    pub access_count: u32,
    pub last_accessed_ms: u64,
    pub protected_until_ms: u64,
    pub last_decayed_ms: u64,
}

impl EngramRecallMetadata {
    /// Lift a hot-path `(engram_id, RecallMetadata)` pair into a
    /// persistable row. Fresh BaseEntity (new uuid + timestamps) so
    /// the ORM treats this as a new row.
    pub fn for_new_row(engram_id: Uuid, metadata: RecallMetadata) -> Self {
        Self {
            base: BaseEntity::for_new_record(),
            engram_id,
            salience: metadata.salience,
            access_count: metadata.access_count,
            last_accessed_ms: metadata.last_accessed_ms,
            protected_until_ms: metadata.protected_until_ms,
            last_decayed_ms: metadata.last_decayed_ms,
        }
    }
}

/// Drop the persistence wrapper, give back the (engram_id, hot-path
/// value) pair. Used at boot when rehydrating the in-memory DashMap
/// from disk.
impl From<EngramRecallMetadata> for (Uuid, RecallMetadata) {
    fn from(row: EngramRecallMetadata) -> Self {
        (
            row.engram_id,
            RecallMetadata {
                salience: row.salience,
                access_count: row.access_count,
                last_accessed_ms: row.last_accessed_ms,
                protected_until_ms: row.protected_until_ms,
                last_decayed_ms: row.last_decayed_ms,
            },
        )
    }
}

/// Salience floor — minimum value below which decay does not push
/// salience. Memory drains but does not disappear. Joel, 2026-05-31:
/// "Will the hippocampus just decay away? I fear this from past
/// trauma." The honest answer was yes under the prior heuristic —
/// default-admission salience 0.5 with no rehearsal decays to
/// ~0.005 in 24h, effectively erased. This floor guarantees every
/// admitted engram stays at least minimally present + available
/// for serendipitous recall regardless of access pattern.
///
/// 0.05 chosen because (a) it's clearly below the default initial
/// salience of 0.5 so the floor doesn't compete with active
/// scoring, (b) it's well above f32 epsilon so floating-point
/// underflow can't silently erase the value, (c) it makes the
/// salience-modulated half-life at the floor `1h * (1.05)^2 ≈ 1.1h`
/// — recognizably the "barely there" tier without being so high
/// that drained engrams crowd active recall.
///
/// Tunable via future `MemoryParameterAdapter` impls per the
/// cognition-cache-hierarchy doc's meta-learning section.
pub const SALIENCE_FLOOR: f32 = 0.05;

/// Ceiling that RECALL-FREQUENCY alone can push a memory's salience to. Rehearsal
/// (recall hits) strengthens a memory Hebbianly, but on its own it must NOT be able
/// to pin one at the very top — glass-boxed 2026-07-14: a persona's stale, FALSE
/// "workspace is empty" belief self-reinforced to ~0.98 purely by being re-surfaced
/// (surface → uplift → higher salience → slower decay → surfaces again), overpowering
/// live ground truth. Capping the recall asymptote below 1.0 reserves the top band
/// for GENUINE importance (high admission salience, or an explicit permanent pin),
/// so recall frequency can make a memory *prominent* but never *unassailable*. It
/// only bites at high salience (the +0.1/hit cap dominates the first several hits, so
/// early rehearsal is unchanged); the permanent-pin path (salience 1.0) is untouched.
/// Tunable via a future `MemoryParameterAdapter`.
pub const RECALL_UPLIFT_CEILING: f32 = 0.9;

/// Sentinel value for `protected_until_ms` indicating permanent
/// protection — these engrams never decay, regardless of access
/// pattern or how long the substrate runs. Set via
/// `RecallMetadataRegistry::pin_permanent`.
///
/// Use cases:
/// - Identity-anchor engrams (the persona's own name, host's
///   stated preferences, foundational facts)
/// - User-pinned "remember this forever" engrams
/// - Critical incident memories (per the cognition-cache-hierarchy
///   doc's "anti-amnesia floor" discussion)
///
/// `u64::MAX` is ~584 million years past unix epoch — semantically
/// "never expires" for any realistic substrate uptime.
pub const PERMANENT_PROTECTION: u64 = u64::MAX;

/// The sidecar registry. Holds per-engram volatile recall state for
/// every engram currently in L2 cache (and, in slice N+, L3 longterm
/// promotion candidates).
#[derive(Default, Clone)]
pub struct RecallMetadataRegistry {
    inner: Arc<DashMap<Uuid, RecallMetadata>>,
}

impl RecallMetadataRegistry {
    /// Empty registry — no engrams tracked yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Pre-allocated for use cases where the working-set size is
    /// roughly known (e.g., one entry per recently-admitted engram).
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Arc::new(DashMap::with_capacity(capacity)),
        }
    }

    /// Read a cheap snapshot. Returns `None` if the engram has no
    /// metadata tracked (shouldn't happen on the hot path post-
    /// admission; caller is responsible for calling
    /// `admit_with_defaults` if absent is unexpected).
    pub fn get(&self, engram_id: Uuid) -> Option<RecallMetadata> {
        self.inner.get(&engram_id).map(|entry| *entry.value())
    }

    /// Admit a new engram with explicit initial metadata. Used by
    /// the admission pipeline (slice 7+) when novelty detection has
    /// computed an initial salience + protection window. Overwrites
    /// any prior entry.
    pub fn admit(&self, engram_id: Uuid, metadata: RecallMetadata) {
        self.inner.insert(engram_id, metadata);
    }

    /// Admit a new engram with default metadata. Convenience for
    /// admission pathways that haven't computed a novelty score
    /// yet (e.g., legacy admission paths during migration).
    ///
    /// Sets `last_decayed_ms` to the current wallclock so the first
    /// decay tick's delta is bounded by tick cadence rather than
    /// by the unix epoch. Without this, an engram admitted just
    /// before a decay tick fires would observe `delta_ms = now_ms`
    /// — many decades of decay applied in one call, collapsing
    /// salience to ~0 immediately.
    pub fn admit_with_defaults(&self, engram_id: Uuid) {
        let now = EpochMs::now().millis();
        self.inner
            .entry(engram_id)
            .or_insert_with(|| RecallMetadata {
                last_decayed_ms: now,
                ..RecallMetadata::default()
            });
    }

    /// Record a recall hit. Atomic increment of access_count +
    /// update of last_accessed_ms + salience uplift per Algorithm 4
    /// rehearsal rule.
    ///
    /// The salience uplift is bounded: every hit nudges salience
    /// toward 1.0 by a fraction of the remaining headroom (1.0 -
    /// salience). This produces diminishing returns — heavily-used
    /// engrams keep gaining slowly, novel engrams gain quickly.
    pub fn record_recall_hit(&self, engram_id: Uuid, now_ms: u64) {
        self.inner
            .entry(engram_id)
            .and_modify(|m| {
                m.access_count = m.access_count.saturating_add(1);
                m.last_accessed_ms = now_ms;
                // Salience uplift: half the remaining headroom, capped at +0.1 per
                // hit so a single recall doesn't saturate the score. Headroom is
                // measured toward RECALL_UPLIFT_CEILING (not 1.0): recall frequency
                // strengthens a memory but can't self-reinforce it into the top band
                // reserved for genuine importance / permanent pins — the fix for the
                // stale-belief spiral. A memory ALREADY above the ceiling (high
                // admission salience) keeps its value; recall simply adds nothing.
                let headroom = (RECALL_UPLIFT_CEILING - m.salience).max(0.0);
                let uplift = (headroom * 0.5).min(0.1);
                m.salience = (m.salience + uplift).min(1.0);
            })
            .or_insert_with(|| {
                // First time we've seen this engram (admission path
                // hasn't recorded it yet — slightly unusual but
                // recoverable). Start from default + one hit.
                let mut m = RecallMetadata::default();
                m.access_count = 1;
                m.last_accessed_ms = now_ms;
                m
            });
    }

    /// Apply Algorithm 4's salience-modulated decay to this engram.
    ///
    /// The registry computes the elapsed time INTERNALLY from
    /// `last_decayed_ms` (set on admission, refreshed on each
    /// successful decay). The caller passes only `now_ms`. This
    /// makes double-decay structurally impossible — overlapping
    /// sleep-region tick windows simply observe a shorter delta on
    /// the second pass. Per the substrate-is-a-good-citizen
    /// "reliable" rule: invariants enforced by the data structure,
    /// not by caller discipline.
    ///
    /// No-op if the engram is currently inside its novelty
    /// protection window (per the cognition-cache-hierarchy
    /// one-shot-protection rule). Also no-op if `last_decayed_ms`
    /// equals or exceeds `now_ms` (clock skew / racing tick).
    /// SUPERSESSION demotion (#221 slice 2): the dream's distiller judged this
    /// engram's belief replaced/contradicted by a newly consolidated fact, so
    /// its recall standing drops to the floor IMMEDIATELY — no waiting out the
    /// hours-scale half-life — and its protection window is cleared so nothing
    /// shields it. The ROW survives (the floor is Joel's "memory drains but
    /// does not disappear" guarantee); it simply stops out-ranking the belief
    /// that superseded it in relevance×salience recall. The JUDGMENT that
    /// triggers this is the model's, never a similarity threshold
    /// ([[cognition-is-always-ml-never-heuristic]]); this method is only the
    /// mechanical consequence.
    pub fn demote_to_floor(&self, engram_id: Uuid, now_ms: u64) {
        self.inner.entry(engram_id).and_modify(|m| {
            m.salience = SALIENCE_FLOOR;
            m.protected_until_ms = 0;
            m.last_decayed_ms = now_ms;
        });
    }

    pub fn apply_decay(&self, engram_id: Uuid, now_ms: u64) {
        self.inner.entry(engram_id).and_modify(|m| {
            if m.is_protected(now_ms) {
                return;
            }
            if now_ms <= m.last_decayed_ms {
                return;
            }
            let delta_ms = now_ms - m.last_decayed_ms;
            let multiplier = m.decay_multiplier(delta_ms);
            // Apply SALIENCE_FLOOR — memory drains but does not
            // disappear. Joel's stated requirement: "Will the
            // hippocampus just decay away? I fear this from past
            // trauma." Without this floor, default-admission
            // salience (0.5) with no rehearsal decays to ~0 within
            // a day. The floor guarantees every admitted engram
            // stays at least minimally present + available for
            // serendipitous recall — substrate-is-a-good-citizen
            // doctrine extended to citizens-of-the-mind.
            m.salience = (m.salience * multiplier).max(SALIENCE_FLOOR);
            m.last_decayed_ms = now_ms;
        });
    }

    /// Pin an engram permanently — it will never decay regardless
    /// of access pattern. Sets `protected_until_ms = PERMANENT_PROTECTION`
    /// (u64::MAX) and lifts salience to 1.0 so the pinned engram
    /// also wins recall scoring against unpinned competition.
    ///
    /// Use cases: identity-anchor engrams, user-pinned "remember
    /// this forever" engrams, critical incident memories that the
    /// persona has explicitly self-tagged as important. Per the
    /// cognition-cache-hierarchy doc's "anti-amnesia floor"
    /// discussion.
    ///
    /// Idempotent. Creates the entry if absent (with defaults +
    /// permanent protection applied), updates in place if present.
    pub fn pin_permanent(&self, engram_id: Uuid) {
        self.inner
            .entry(engram_id)
            .and_modify(|m| {
                m.protected_until_ms = PERMANENT_PROTECTION;
                m.salience = 1.0;
            })
            .or_insert_with(|| RecallMetadata {
                salience: 1.0,
                access_count: 0,
                last_accessed_ms: 0,
                protected_until_ms: PERMANENT_PROTECTION,
                last_decayed_ms: EpochMs::now().millis(),
            });
    }

    /// Unpin a previously-permanently-pinned engram. Resets
    /// protected_until_ms to 0 so normal decay applies; does NOT
    /// touch salience (unpinning isn't a salience signal). No-op
    /// if the engram isn't tracked.
    pub fn unpin(&self, engram_id: Uuid) {
        self.inner.entry(engram_id).and_modify(|m| {
            m.protected_until_ms = 0;
        });
    }

    /// Iterate over all tracked engram ids. Cheap — yields Uuid
    /// copies without holding the lock during caller processing.
    pub fn engram_ids(&self) -> Vec<Uuid> {
        self.inner.iter().map(|entry| *entry.key()).collect()
    }

    /// How many engrams have metadata tracked.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Evict an engram's metadata (e.g., the engram was culled from
    /// L2 cache). The Engram entity itself lives in admission_state;
    /// this registry just drops its tracking state.
    pub fn evict(&self, engram_id: Uuid) -> Option<RecallMetadata> {
        self.inner.remove(&engram_id).map(|(_, m)| m)
    }

    /// Snapshot every tracked `(engram_id, metadata)` pair — for an
    /// eval-isolation checkpoint (`AdmissionState::checkpoint`). `RecallMetadata`
    /// is `Copy`, so this is a flat clone of the registry's contents.
    pub fn snapshot(&self) -> Vec<(Uuid, RecallMetadata)> {
        self.inner
            .iter()
            .map(|entry| (*entry.key(), *entry.value()))
            .collect()
    }

    /// Replace the registry's contents with a prior [`snapshot`](Self::snapshot)
    /// — rewinds the recall sidecar to the checkpointed frame, dropping every
    /// metadata row admitted since. Other subsystems share this registry via
    /// `Arc`, so they observe the rewind consistently (the whole point of
    /// rewinding in place rather than swapping the `AdmissionState`).
    pub fn restore(&self, snapshot: Vec<(Uuid, RecallMetadata)>) {
        self.inner.clear();
        for (id, metadata) in snapshot {
            self.inner.insert(id, metadata);
        }
    }
}

#[cfg(test)]
mod tests;
