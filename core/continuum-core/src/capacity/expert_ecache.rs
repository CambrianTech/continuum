//! expert_ecache — bounded expert-record cache with LFRU eviction and a
//! cliff-aware budget (the M5 half of the #268/#269 division, 2026-07-30).
//!
//! Semantics ported from WASTE's `ecache.{h,c}` (Apache-2.0, sqliteai/waste),
//! the independently-built engine that validated both lanes' architecture —
//! see docs/reference/WASTE-EXTRACT.md for the mined rationale. The three
//! load-bearing choices, each with a measured reason:
//!
//! - **ONE key per logical expert** — `layer<<16 | expert`, with gate/up/down
//!   treated as one bundled record (they are always co-activated; splitting
//!   them 3-ways tripled key-space and was half of the reuse=0 hunt).
//! - **LFRU, not LRU** — frequency first, recency tiebreak, victims from a
//!   small random sample. WASTE's Gate 2: at small cache fractions plain LRU
//!   collapses to 5% hit rate where LFRU holds 29%.
//! - **The budget is a FUNCTION, never a constant** — WASTE's Gate 5 measured
//!   a CLIFF: hit rate is exactly 0 until the budget exceeds one token's
//!   working set, then climbs. Running a cache below the cliff burns RAM for
//!   structurally-zero benefit, so [`EcacheBudget::derive`] REFUSES loudly
//!   below it instead of silently thrashing ([[fallbacks-are-illegal]] — the
//!   reuse=0 mystery was three days of exactly this silence, two nodes wide).

use std::collections::HashMap;

/// Identity of one logical expert: the bundled gate/up/down record of one
/// routed expert in one layer. This is the ONLY cache key — never a tensor
/// pointer (graph nodes re-mint per decode step), never a per-projection
/// split (3× key-space, and cross-projection index drift gives exactly-0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExpertKey {
    pub layer: u16,
    pub expert: u16,
}

impl ExpertKey {
    /// WASTE-compatible packed form (`layer<<16 | expert`).
    pub fn packed(self) -> u32 {
        ((self.layer as u32) << 16) | self.expert as u32
    }
}

/// Why a derived budget was refused. Loud by construction: the caller must
/// either shrink the record (the #268 container: ~600 MB MXFP4 → ~12.4 MB
/// VQ3, a 48× working-set cut) or not run a cache at all — there is no
/// "small cache" middle ground below the cliff.
#[derive(Debug, PartialEq, Eq)]
pub struct BelowCliff {
    pub needed_bytes: u64,
    pub available_bytes: u64,
}

impl std::fmt::Display for BelowCliff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "expert-cache budget below the working-set cliff: need {} bytes \
             (one token's working set), have {} — hit rate would be structurally \
             ZERO (WASTE Gate 5). Shrink records (#268 container) or run cacheless.",
            self.needed_bytes, self.available_bytes
        )
    }
}

/// The derived cache budget. Constructed ONLY through [`Self::derive`] so a
/// below-cliff configuration cannot exist by construction.
#[derive(Debug, Clone, Copy)]
pub struct EcacheBudget {
    pub slots: usize,
    pub record_bytes: u64,
}

impl EcacheBudget {
    /// One token's working set: the bytes every decode step touches. Below
    /// this the cache evicts everything it inserted before the next token
    /// asks — the measured zero-plateau.
    pub fn one_token_working_set(activated_per_token: u32, record_bytes: u64) -> u64 {
        activated_per_token as u64 * record_bytes
    }

    /// Derive a budget from what the machine actually has. `available_bytes`
    /// is the governor-granted allowance (LIVE, a lease — never a config
    /// constant), `activated_per_token` comes from the model's arch profile
    /// (K3: ~8 experts × active MoE layers), `record_bytes` from the
    /// container manifest. Errors below the cliff.
    pub fn derive(
        available_bytes: u64,
        activated_per_token: u32,
        record_bytes: u64,
    ) -> Result<Self, BelowCliff> {
        let cliff = Self::one_token_working_set(activated_per_token, record_bytes);
        if record_bytes == 0 || available_bytes <= cliff {
            return Err(BelowCliff {
                needed_bytes: cliff,
                available_bytes,
            });
        }
        Ok(Self {
            slots: (available_bytes / record_bytes) as usize,
            record_bytes,
        })
    }
}

#[derive(Debug, Clone)]
struct Slot {
    key: ExpertKey,
    hits: u64,
    last: u64,
}

/// Eviction policy. LFRU is the default and the reasoned choice; LRU exists
/// so the invariant test can DEMONSTRATE the gap rather than assert it on
/// faith (the honest-instrument rule).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvictionPolicy {
    Lfru,
    Lru,
}

/// Bounded expert cache: bookkeeping only (keys, LFRU state, counters) — the
/// record BYTES live wherever the caller's pool put them; this decides what
/// stays. Pure and deterministic (seeded sampling), so the invariants are
/// testable without IO.
pub struct ExpertEcache {
    slots: Vec<Slot>,
    index: HashMap<ExpertKey, usize>,
    max_slots: usize,
    clock: u64,
    rng: u64,
    policy: EvictionPolicy,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

/// Victim-sample size — WASTE's approximation: sample a few candidates
/// instead of scanning every slot; the Gate 2 simulator showed the sampled
/// form tracks the exact form.
const VICTIM_SAMPLE: usize = 5;

impl ExpertEcache {
    pub fn new(budget: EcacheBudget, policy: EvictionPolicy) -> Self {
        Self {
            slots: Vec::new(),
            index: HashMap::new(),
            max_slots: budget.slots.max(1),
            clock: 0,
            rng: 0x9E37_79B9_7F4A_7C15,
            policy,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }

    fn next_rand(&mut self) -> u64 {
        // xorshift64* — deterministic, seedless-config, good enough for
        // victim sampling (never used for anything security-shaped).
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Touch the key: hit bookkeeping if resident, miss + admit (evicting a
    /// sampled victim when full) if not. Returns whether it was a hit. The
    /// caller performs the actual fetch on a miss — bytes are its concern.
    pub fn touch(&mut self, key: ExpertKey) -> bool {
        self.clock += 1;
        if let Some(&i) = self.index.get(&key) {
            self.hits += 1;
            self.slots[i].hits += 1;
            self.slots[i].last = self.clock;
            return true;
        }
        self.misses += 1;
        if self.slots.len() < self.max_slots {
            self.index.insert(key, self.slots.len());
            self.slots.push(Slot { key, hits: 1, last: self.clock });
            return false;
        }
        // Evict: best victim among a small random sample.
        let mut victim = (self.next_rand() as usize) % self.slots.len();
        for _ in 1..VICTIM_SAMPLE.min(self.slots.len()) {
            let cand = (self.next_rand() as usize) % self.slots.len();
            let (c, v) = (&self.slots[cand], &self.slots[victim]);
            let cand_worse = match self.policy {
                // Frequency first, recency tiebreak.
                EvictionPolicy::Lfru => (c.hits, c.last) < (v.hits, v.last),
                EvictionPolicy::Lru => c.last < v.last,
            };
            if cand_worse {
                victim = cand;
            }
        }
        self.evictions += 1;
        self.index.remove(&self.slots[victim].key);
        self.index.insert(key, victim);
        self.slots[victim] = Slot { key, hits: 1, last: self.clock };
        false
    }

    pub fn hit_rate(&self) -> f64 {
        let t = self.hits + self.misses;
        if t == 0 { 0.0 } else { self.hits as f64 / t as f64 }
    }

    pub fn resident(&self) -> usize {
        self.slots.len()
    }

    /// Snapshot of the routing hotlist — (key, hits) sorted hottest-first.
    /// WASTE's `usage.waste` analog: the caller persists this across restarts;
    /// the cache itself never does IO.
    pub fn usage(&self) -> Vec<(ExpertKey, u64)> {
        let mut u: Vec<_> = self.slots.iter().map(|s| (s.key, s.hits)).collect();
        u.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        u
    }

    /// Warm-start from a persisted [`Self::usage`] snapshot: pre-admit the
    /// hottest keys so the first tokens after a restart hit instead of
    /// re-ramping from zero (the cold ramp is the ONLY thing this fixes —
    /// steady-state behavior is untouched). Warm entries carry their prior
    /// frequency but `last = 0`, so on recency tiebreaks every live-proven
    /// resident beats a warm guess. Only fills empty slots; never evicts.
    pub fn warm(&mut self, usage: impl IntoIterator<Item = (ExpertKey, u64)>) {
        for (key, hits) in usage {
            if self.slots.len() >= self.max_slots {
                break;
            }
            if self.index.contains_key(&key) {
                continue;
            }
            self.index.insert(key, self.slots.len());
            self.slots.push(Slot { key, hits: hits.max(1), last: 0 });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(layer: u16, expert: u16) -> ExpertKey {
        ExpertKey { layer, expert }
    }

    /// A skewed per-token workload: `hot` experts recur every token (the
    /// power-law head), plus `cold_per_token` one-shot experts per token
    /// (the tail). Deterministic — same shape the Gate 2/5 simulators used.
    fn run_workload(
        cache: &mut ExpertEcache,
        tokens: usize,
        hot: &[ExpertKey],
        cold_per_token: usize,
    ) {
        let mut cold_id: u16 = 1000;
        for _ in 0..tokens {
            for &k in hot {
                cache.touch(k);
            }
            for _ in 0..cold_per_token {
                cache.touch(key(0, cold_id));
                cold_id = cold_id.wrapping_add(1);
            }
        }
    }

    // what this catches: the WASTE Gate-5 CLIFF as a construction invariant —
    // a budget at or below one token's working set cannot exist. This is the
    // fix for the two-node reuse=0 hunt: below-cliff configs refuse loudly
    // instead of running a structurally-zero cache (BigMama's ~8GB pools at
    // ~600MB records sat exactly here for three days).
    #[test]
    fn budget_below_one_token_working_set_is_refused_loudly() {
        let record = 600 * 1024 * 1024u64; // raw MXFP4 expert
        let per_token = 488; // K3: ~8 experts × ~61 layers
        let cliff = EcacheBudget::one_token_working_set(per_token, record);
        let err = EcacheBudget::derive(8 * 1024 * 1024 * 1024, per_token, record)
            .expect_err("8GB at 600MB records is far below the cliff");
        assert_eq!(err.needed_bytes, cliff);

        // The SAME machine clears the cliff once #268 shrinks the record.
        let vq3 = 12_406_784u64; // WASTE's measured K3 record
        EcacheBudget::derive(8 * 1024 * 1024 * 1024, per_token, vq3)
            .expect("8GB at VQ3 records is above the cliff — the container IS the fix");
    }

    // what this catches: above the cliff, a recurring working set actually
    // reuses — hit rate climbs well off zero on a skewed workload. Exactly-0
    // with a working cache and recurring keys is impossible; if this ever
    // fails, the keying broke (the other half of the two-node hunt).
    #[test]
    fn recurring_working_set_reuses_above_the_cliff() {
        let budget = EcacheBudget::derive(200, 8, 10).expect("20 slots > 8-key WS");
        let mut c = ExpertEcache::new(budget, EvictionPolicy::Lfru);
        let hot: Vec<ExpertKey> = (0..8).map(|e| key(1, e)).collect();
        run_workload(&mut c, 50, &hot, 2);
        assert!(
            c.hit_rate() > 0.5,
            "recurring 8-key working set in a 20-slot cache must reuse heavily, got {}",
            c.hit_rate()
        );
    }

    // what this catches: the measured LFRU-over-LRU reason survives the port.
    // At a small cache fraction with a heavy one-shot tail, LRU churns its
    // hot set out (WASTE Gate 2: 5% vs 29%); LFRU must beat it clearly here
    // or the victim logic regressed.
    #[test]
    fn lfru_beats_lru_at_small_cache_fractions() {
        let budget = EcacheBudget::derive(130, 8, 10).expect("12 slots");
        let hot: Vec<ExpertKey> = (0..8).map(|e| key(1, e)).collect();

        let mut lfru = ExpertEcache::new(budget, EvictionPolicy::Lfru);
        run_workload(&mut lfru, 200, &hot, 10);

        let mut lru = ExpertEcache::new(budget, EvictionPolicy::Lru);
        run_workload(&mut lru, 200, &hot, 10);

        assert!(
            lfru.hit_rate() > lru.hit_rate() + 0.1,
            "LFRU must clearly beat LRU under a one-shot tail: lfru={} lru={}",
            lfru.hit_rate(),
            lru.hit_rate()
        );
    }

    // what this catches: key identity is the LOGICAL expert — packed form is
    // stable and collision-free across the full (layer, expert) range, and
    // the same key re-presented is the same entry (no per-projection split,
    // no pointer identity anywhere in the type).
    #[test]
    fn packed_key_is_stable_and_unique() {
        assert_eq!(key(3, 417).packed(), (3u32 << 16) | 417);
        assert_ne!(key(3, 417).packed(), key(4, 417).packed());
        assert_ne!(key(3, 417).packed(), key(3, 418).packed());

        let budget = EcacheBudget::derive(100, 2, 10).expect("10 slots");
        let mut c = ExpertEcache::new(budget, EvictionPolicy::Lfru);
        assert!(!c.touch(key(3, 417)), "first touch is a miss");
        assert!(c.touch(key(3, 417)), "same logical expert is a hit");
        assert_eq!(c.resident(), 1, "one logical expert = one entry");
    }

    // what this catches: the cold ramp after a restart — a cache warmed from
    // the previous run's usage snapshot must serve its FIRST pass over the
    // hot set from residency, not re-miss its way back up (WASTE's
    // usage.waste warm-start; the reboot sibling of the fixed-point re-plan).
    #[test]
    fn warm_start_from_persisted_usage_kills_the_cold_ramp() {
        let budget = EcacheBudget::derive(200, 2, 10).expect("20 slots");
        let hot: Vec<ExpertKey> = (0..8).map(|e| key(0, e)).collect();

        // Run 1: establish the hotlist, then "persist" it.
        let mut before = ExpertEcache::new(budget, EvictionPolicy::Lfru);
        for _ in 0..5 {
            for &k in &hot {
                before.touch(k);
            }
        }
        let snapshot = before.usage();
        assert_eq!(snapshot[0].1, 5, "hotlist carries real frequencies");

        // Run 2 (post-restart): warm, then the first pass is all hits.
        let mut after = ExpertEcache::new(budget, EvictionPolicy::Lfru);
        after.warm(snapshot);
        for &k in &hot {
            assert!(after.touch(k), "warm-started key {k:?} must hit on first touch");
        }
        assert_eq!(after.misses, 0, "zero cold misses after warm-start");

        // Warm never overfills: a hotlist longer than the budget truncates.
        let mut tiny = ExpertEcache::new(
            EcacheBudget::derive(30, 2, 10).expect("3 slots"),
            EvictionPolicy::Lfru,
        );
        tiny.warm((0..100).map(|e| (key(1, e), 2)));
        assert_eq!(tiny.resident(), 3, "warm fills at most the budget");
    }
}
