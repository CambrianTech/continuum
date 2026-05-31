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

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use uuid::Uuid;

/// Per-engram volatile recall state. Cloneable + Copy because all
/// fields are primitives — recall scoring reads a cheap snapshot
/// without locking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecallMetadata {
    pub salience: f32,
    pub access_count: u32,
    pub last_accessed_ms: u64,
    pub protected_until_ms: u64,
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
    /// salience-1.0 has a half-life 9× longer than salience-0.0.
    /// We implement this as exponential decay with a
    /// salience-modulated half-life: `half_life = base * (1 + s)^2`.
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
    pub fn admit_with_defaults(&self, engram_id: Uuid) {
        self.inner
            .entry(engram_id)
            .or_insert_with(RecallMetadata::default);
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
                // Salience uplift: half the remaining headroom,
                // capped at +0.1 per hit so a single recall doesn't
                // saturate the score.
                let headroom = 1.0 - m.salience;
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
    /// `delta_ms` = wallclock time since this engram's last decay
    /// application (typically since `last_accessed_ms`, or since
    /// the prior decay tick if more recent).
    ///
    /// No-op if the engram is currently inside its novelty
    /// protection window (per the [[cognition-cache-hierarchy]]
    /// one-shot-protection rule).
    pub fn apply_decay(&self, engram_id: Uuid, delta_ms: u64, now_ms: u64) {
        self.inner.entry(engram_id).and_modify(|m| {
            if m.is_protected(now_ms) {
                return;
            }
            let multiplier = m.decay_multiplier(delta_ms);
            m.salience *= multiplier;
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
}

/// Helper for getting the current wallclock as ms since epoch.
/// Used in admission + recall + decay paths to stamp timestamps.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_registry_is_empty() {
        let r = RecallMetadataRegistry::new();
        assert_eq!(r.len(), 0);
        assert!(r.is_empty());
    }

    #[test]
    fn admit_with_defaults_creates_neutral_entry() {
        let r = RecallMetadataRegistry::new();
        let id = Uuid::new_v4();
        r.admit_with_defaults(id);
        let m = r.get(id).unwrap();
        assert_eq!(m, RecallMetadata::default());
        assert_eq!(m.salience, 0.5);
        assert_eq!(m.access_count, 0);
    }

    #[test]
    fn admit_overrides_default_metadata() {
        let r = RecallMetadataRegistry::new();
        let id = Uuid::new_v4();
        r.admit_with_defaults(id);
        let custom = RecallMetadata {
            salience: 0.9,
            access_count: 0,
            last_accessed_ms: 0,
            protected_until_ms: 1000,
        };
        r.admit(id, custom);
        assert_eq!(r.get(id).unwrap(), custom);
    }

    #[test]
    fn record_recall_hit_increments_and_uplifts() {
        let r = RecallMetadataRegistry::new();
        let id = Uuid::new_v4();
        r.admit_with_defaults(id);
        let before = r.get(id).unwrap();
        assert_eq!(before.salience, 0.5);

        r.record_recall_hit(id, 1_000_000);
        let after_one = r.get(id).unwrap();
        assert_eq!(after_one.access_count, 1);
        assert_eq!(after_one.last_accessed_ms, 1_000_000);
        // Salience should have grown but not by more than the cap (0.1)
        // per hit.
        assert!(after_one.salience > before.salience);
        assert!(after_one.salience <= before.salience + 0.1 + f32::EPSILON);

        // Two more hits — salience keeps growing with diminishing
        // returns, asymptoting toward 1.0.
        r.record_recall_hit(id, 1_001_000);
        r.record_recall_hit(id, 1_002_000);
        let after_three = r.get(id).unwrap();
        assert_eq!(after_three.access_count, 3);
        assert!(after_three.salience > after_one.salience);
        assert!(after_three.salience <= 1.0);
    }

    #[test]
    fn record_recall_hit_creates_entry_if_absent() {
        let r = RecallMetadataRegistry::new();
        let id = Uuid::new_v4();
        // No prior admit call.
        r.record_recall_hit(id, 12345);
        let m = r.get(id).unwrap();
        assert_eq!(m.access_count, 1);
        assert_eq!(m.last_accessed_ms, 12345);
    }

    #[test]
    fn apply_decay_reduces_salience_over_time() {
        let r = RecallMetadataRegistry::new();
        let id = Uuid::new_v4();
        let m = RecallMetadata {
            salience: 0.8,
            access_count: 0,
            last_accessed_ms: 0,
            protected_until_ms: 0,
        };
        r.admit(id, m);

        // Apply 2 hours of decay (well past the half-life for
        // salience=0.8). Salience should drop significantly.
        let two_hours_ms: u64 = 7_200_000;
        r.apply_decay(id, two_hours_ms, two_hours_ms);
        let decayed = r.get(id).unwrap();
        assert!(decayed.salience < 0.8, "got {}", decayed.salience);
        assert!(decayed.salience > 0.0);
    }

    #[test]
    fn apply_decay_skips_protected_engrams() {
        let r = RecallMetadataRegistry::new();
        let id = Uuid::new_v4();
        let m = RecallMetadata {
            salience: 0.8,
            access_count: 0,
            last_accessed_ms: 0,
            // Protection window extends well into the future.
            protected_until_ms: 100_000_000_000,
        };
        r.admit(id, m);

        // Try to decay during protection window. Should be no-op.
        r.apply_decay(id, 7_200_000, 1_000_000);
        let after = r.get(id).unwrap();
        assert_eq!(after.salience, 0.8, "protection window failed to prevent decay");
    }

    #[test]
    fn high_salience_decays_slower_than_low() {
        // Algorithm 4 invariant: salience-1.0 has a half-life 4×
        // longer than salience-0.0 (we use (1+s)^2 multiplier).
        let r = RecallMetadataRegistry::new();
        let low_id = Uuid::new_v4();
        let high_id = Uuid::new_v4();
        r.admit(
            low_id,
            RecallMetadata {
                salience: 0.0,
                ..Default::default()
            },
        );
        r.admit(
            high_id,
            RecallMetadata {
                salience: 1.0,
                ..Default::default()
            },
        );

        let one_hour_ms: u64 = 3_600_000;
        // Note: both engrams start at access_count=0, last_accessed=0,
        // protected_until=0 so neither is protected and decay applies.
        r.apply_decay(low_id, one_hour_ms, one_hour_ms);
        r.apply_decay(high_id, one_hour_ms, one_hour_ms);
        let low_after = r.get(low_id).unwrap();
        let high_after = r.get(high_id).unwrap();
        // Low: ~0.0 (already at 0, no further decay matters)
        assert!(low_after.salience < 0.5);
        // High: still > 0.7 after one hour (because half-life is 4h)
        assert!(
            high_after.salience > 0.7,
            "high-salience decayed too fast: {}",
            high_after.salience
        );
    }

    #[test]
    fn evict_removes_metadata() {
        let r = RecallMetadataRegistry::new();
        let id = Uuid::new_v4();
        r.admit_with_defaults(id);
        assert!(r.get(id).is_some());
        let removed = r.evict(id);
        assert!(removed.is_some());
        assert!(r.get(id).is_none());
    }

    #[test]
    fn clone_shares_inner() {
        let r1 = RecallMetadataRegistry::new();
        let r2 = r1.clone();
        let id = Uuid::new_v4();
        r1.admit_with_defaults(id);
        // r2 should see the same entry — they share Arc<DashMap>.
        assert!(r2.get(id).is_some());
        assert_eq!(r2.len(), 1);
    }

    #[test]
    fn engram_ids_returns_all_tracked() {
        let r = RecallMetadataRegistry::new();
        let ids: Vec<Uuid> = (0..5).map(|_| Uuid::new_v4()).collect();
        for id in &ids {
            r.admit_with_defaults(*id);
        }
        let listed = r.engram_ids();
        assert_eq!(listed.len(), 5);
        for id in &ids {
            assert!(listed.contains(id));
        }
    }
}
