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

/// Identity of one physical expert record: the bundled gate/up/down record
/// of one routed expert in one layer AT ONE PRECISION TIER. This is the ONLY
/// cache key — never a tensor pointer (graph nodes re-mint per decode step),
/// never a per-projection split (3× key-space, and cross-projection index
/// drift gives exactly-0). Tier is part of identity (manifest v2, locked
/// 2026-07-31): a sharp and a cruft copy of the same expert are DIFFERENT
/// bytes from different banks; on promotion the sharp copy admits and the
/// cruft copy ages out naturally — never aliased.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExpertKey {
    pub layer: u16,
    pub expert: u16,
    /// Precision tier (0 = sharpest). Single-tier v1 containers use 0
    /// everywhere, which reproduces the pre-tier key space exactly.
    pub tier: u16,
}

impl ExpertKey {
    /// Tier-0 (sharpest / v1) key — the pre-tier call shape.
    pub fn sharp(layer: u16, expert: u16) -> Self {
        Self {
            layer,
            expert,
            tier: 0,
        }
    }

    /// Packed form (`layer<<32 | expert<<16 | tier`). Widened from WASTE's
    /// u32 when tier joined identity; tier-0 keys keep their low 32 bits
    /// shifted, preserving relative order.
    pub fn packed(self) -> u64 {
        ((self.layer as u64) << 32) | ((self.expert as u64) << 16) | self.tier as u64
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
    /// SHARPEST-tier record size — the cliff and slot derivation use the
    /// worst case so a budget can never be over-committed by tier mix.
    pub record_bytes: u64,
    /// Byte allowance for tier-mixed residency (== the governor grant the
    /// budget was derived from). Admission accounts real per-record bytes
    /// against this, so cruft-tier records pack denser than the slot count
    /// alone would allow — capacity follows the allocator's tier mix
    /// automatically instead of assuming every resident is sharp.
    pub max_bytes: u64,
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
            max_bytes: available_bytes,
        })
    }

    /// Derive from a [`MoeArchProfile`] + container manifest facts — the
    /// #231 adapter-law entry point. The cliff input flows GGUF →
    /// profile → here; no caller ever types an activation count.
    pub fn derive_from_profile(
        available_bytes: u64,
        profile: &crate::capacity::moe_arch_profile::MoeArchProfile,
        record_bytes: u64,
    ) -> Result<Self, BelowCliff> {
        Self::derive(available_bytes, profile.activated_per_token(), record_bytes)
    }
}

#[derive(Debug, Clone)]
struct Slot {
    key: ExpertKey,
    hits: u64,
    last: u64,
    /// Real bytes of this record (per-tier sizes differ, manifest v2).
    bytes: u64,
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
    /// Byte allowance + live account (manifest v2 tier mix): admission
    /// checks BYTES, not just slot count, so cruft records pack denser and
    /// a sharp admit can require multiple cruft evictions.
    max_bytes: u64,
    resident_bytes: u64,
    /// Uniform record size for the tier-less [`Self::touch`] path (the
    /// sharpest tier / v1 case).
    default_record_bytes: u64,
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
            max_bytes: budget.max_bytes.max(budget.record_bytes),
            resident_bytes: 0,
            default_record_bytes: budget.record_bytes,
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

    /// Touch the key at the budget's default (sharpest-tier) record size —
    /// the v1 call shape. See [`Self::touch_sized`].
    pub fn touch(&mut self, key: ExpertKey) -> bool {
        let bytes = self.default_record_bytes;
        self.touch_sized(key, bytes)
    }

    /// Touch the key with its REAL record size (per-tier, manifest v2): hit
    /// bookkeeping if resident; miss + byte-accounted admit if not, evicting
    /// sampled victims until the record fits (a sharp admit may displace
    /// several cruft residents). Returns whether it was a hit. The caller
    /// performs the actual fetch on a miss — bytes live in its pool.
    pub fn touch_sized(&mut self, key: ExpertKey, record_bytes: u64) -> bool {
        self.clock += 1;
        if let Some(&i) = self.index.get(&key) {
            self.hits += 1;
            self.slots[i].hits += 1;
            self.slots[i].last = self.clock;
            return true;
        }
        self.misses += 1;
        // Evict until the incoming record fits BOTH accounts (bytes + slot
        // count). Loops because tier sizes differ; bounded by slot count.
        while !self.slots.is_empty()
            && (self.resident_bytes + record_bytes > self.max_bytes
                || self.slots.len() >= self.max_slots)
        {
            let victim = self.pick_victim();
            self.evictions += 1;
            self.resident_bytes -= self.slots[victim].bytes;
            let last = self.slots.len() - 1;
            self.index.remove(&self.slots[victim].key);
            self.slots.swap(victim, last);
            if victim != last {
                self.index.insert(self.slots[victim].key, victim);
            }
            self.slots.pop();
        }
        self.index.insert(key, self.slots.len());
        self.slots.push(Slot {
            key,
            hits: 1,
            last: self.clock,
            bytes: record_bytes,
        });
        self.resident_bytes += record_bytes;
        false
    }

    /// Best victim among a small random sample (WASTE's approximation).
    fn pick_victim(&mut self) -> usize {
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
        victim
    }

    /// Bytes currently accounted resident — the number the footprint
    /// reporter and the governor lease reconcile against.
    pub fn resident_bytes(&self) -> u64 {
        self.resident_bytes
    }

    pub fn hit_rate(&self) -> f64 {
        let t = self.hits + self.misses;
        if t == 0 {
            0.0
        } else {
            self.hits as f64 / t as f64
        }
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
        // Warm entries are accounted at the default (sharpest-tier) size —
        // conservative: a warm guess never over-fills the byte budget.
        let bytes = self.default_record_bytes;
        for (key, hits) in usage {
            if self.slots.len() >= self.max_slots || self.resident_bytes + bytes > self.max_bytes {
                break;
            }
            if self.index.contains_key(&key) {
                continue;
            }
            self.index.insert(key, self.slots.len());
            self.slots.push(Slot {
                key,
                hits: hits.max(1),
                last: 0,
                bytes,
            });
            self.resident_bytes += bytes;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(layer: u16, expert: u16) -> ExpertKey {
        ExpertKey::sharp(layer, expert)
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
        assert_eq!(key(3, 417).packed(), (3u64 << 32) | (417u64 << 16));
        assert_ne!(key(3, 417).packed(), key(4, 417).packed());
        assert_ne!(key(3, 417).packed(), key(3, 418).packed());
        // Tier is identity: the same (layer, expert) at another tier is a
        // DIFFERENT record — sharp and cruft copies must never alias.
        assert_ne!(
            key(3, 417).packed(),
            ExpertKey {
                layer: 3,
                expert: 417,
                tier: 1
            }
            .packed()
        );

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
    // what this catches: byte-accounted tier-mix admission (manifest v2) —
    // cruft records must pack DENSER than the sharp slot count implies, and
    // a sharp admit into a cruft-full cache must evict as many cruft
    // residents as its bytes require (not exactly one). The account must
    // balance exactly against the byte budget throughout.
    #[test]
    fn tier_mix_packs_by_bytes_and_sharp_admit_evicts_enough_cruft() {
        // Budget: 8 sharp records of 4096B (32768B). Slots derive from the
        // sharp size; bytes are the real constraint.
        let budget = EcacheBudget::derive(8 * 4096, 2, 4096).expect("above cliff");
        let mut c = ExpertEcache::new(budget, EvictionPolicy::Lru);

        // Fill with cruft (tier 1, half-size): byte budget holds 16 of them,
        // but slot count (8) caps first — both accounts are enforced.
        for e in 0..8u16 {
            assert!(!c.touch_sized(
                ExpertKey {
                    layer: 0,
                    expert: e,
                    tier: 1
                },
                2048
            ));
        }
        assert_eq!(c.resident(), 8);
        assert_eq!(c.resident_bytes(), 8 * 2048);

        // One sharp admit (4096B) fits the byte budget with room to spare —
        // slot cap forces exactly one eviction, account stays balanced.
        assert!(!c.touch_sized(ExpertKey::sharp(0, 100), 4096));
        assert_eq!(c.resident(), 8);
        assert_eq!(c.resident_bytes(), 7 * 2048 + 4096);

        // Now BYTE pressure: fill 3 of 4 slots at the default size (12288 of
        // 16384B), then admit an OVERSIZED record (3×4096). Bytes bind before
        // slots — the admit must evict TWO residents (one leaves 20480 > 16384;
        // two leaves 16384 ≤ 16384), never just one.
        let budget = EcacheBudget::derive(4 * 4096, 2, 4096).expect("above cliff");
        let mut c = ExpertEcache::new(budget, EvictionPolicy::Lru);
        for e in 0..3u16 {
            c.touch_sized(ExpertKey::sharp(1, e), 4096);
        }
        assert_eq!(c.resident_bytes(), 3 * 4096);
        assert!(!c.touch_sized(
            ExpertKey {
                layer: 1,
                expert: 200,
                tier: 0
            },
            3 * 4096
        ));
        assert_eq!(c.evictions, 2, "oversized admit must free enough BYTES");
        assert!(
            c.resident_bytes() <= 4 * 4096,
            "account stays within the byte budget: {}",
            c.resident_bytes()
        );
        assert!(
            c.touch_sized(
                ExpertKey {
                    layer: 1,
                    expert: 200,
                    tier: 0
                },
                3 * 4096
            ),
            "and it is resident"
        );
    }

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
            assert!(
                after.touch(k),
                "warm-started key {k:?} must hit on first touch"
            );
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
