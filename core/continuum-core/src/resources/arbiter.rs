//! The arbiter — the swappable *policy* that scores value and urgency.
//!
//! The [ledger](super::ledger) is **mechanism**: it enforces the over-commit
//! guard, reservation floors, and min-dwell, and it never decides *who* wins —
//! it only refuses to *violate* a safety bound. The arbiter decides. It answers
//! the two questions every allocator under scarcity must answer, as continuous
//! scalars — quantifiable the way a RANSAC inlier score or an ML loss is:
//!
//! - **Reclaim-worthiness** of a held lease — higher means take it back sooner.
//! - **Urgency** of an unmet demand — higher means grant it sooner, and it
//!   *rises over time* (the cost of making a request wait keeps climbing, like
//!   a traffic light where the side road eventually has to turn green).
//!
//! Mechanism / policy split: the ledger gates *eligibility* (floors, dwell,
//! over-commit); the arbiter only *orders* within what is already safe to take.
//! A bad policy can pick a worse victim, never an unsafe one.
//!
//! Start dumb and fast. [`TieredArbiter`] is a closed-form heuristic that
//! reproduces the safest-first + LRU ordering the ledger shipped with, expressed
//! as a score so a richer scorer — task-complexity estimation, a learned model,
//! a persona-analysis panel — drops in behind this same trait without touching
//! the selector. This is the control-loop shape: a swappable POLICY over a fixed
//! deterministic safety bound, ramped up with the complexity of the decision.

use super::lease::{LeaseRequest, ReclaimPolicy, ResourceLease};

/// Pure inputs an arbiter scores against. The ledger stays clock-free; the
/// daemon supplies `now_ms` and the current `pressure` (the traffic density)
/// read from the existing pressure monitors — never recomputed here.
#[derive(Debug, Clone, Copy)]
pub struct ArbiterContext {
    pub now_ms: u64,
    /// Normalized contention on this kind, `0.0` (idle) .. `1.0` (critical).
    /// Higher pressure steepens demand urgency; it does not change reclaim
    /// *order* (it scales all candidates uniformly).
    pub pressure: f64,
}

/// The policy seam. Both methods return a continuous score; the ledger and
/// daemon read them, never the reverse. Implementations range from the
/// dumb-fast [`TieredArbiter`] to future learned scorers — all behind this one
/// trait so the allocation policy is swappable without rewiring the substrate.
pub trait LeaseArbiter: Send + Sync {
    fn name(&self) -> &str;

    /// Reclaim-worthiness of a held lease — the victim selector takes the
    /// HIGHEST-scoring *eligible* leases first. Eligibility (floors, dwell,
    /// active-Pinned exclusion) is the ledger's job; this only orders the
    /// already-safe set. An active `Pinned` lease should never be scored for
    /// reclaim — return [`f64::MIN`] as a belt-and-suspenders guard (the ledger
    /// already filters it out via `reclaim_rank`).
    fn reclaim_score(&self, lease: &ResourceLease, ctx: &ArbiterContext) -> f64;

    /// Urgency of an unmet demand, GROWING with `waited_ms` — the "cost goes up
    /// over time" the directive calls for. Anti-starvation: a low-value request
    /// that has waited long enough eventually out-scores fresh high-value
    /// competition, so no demand waits forever. This orders the GRANT queue; it
    /// never preempts an active lease (that stays the ledger's protected path).
    fn demand_urgency(&self, request: &LeaseRequest, waited_ms: u64, ctx: &ArbiterContext) -> f64;
}

/// Monotonic, saturating elapsed-time term in `[0, 1)`: `e / (e + half_life)`.
/// Half its ceiling is reached at `half_life`. Bounded, so a weighted age/wait
/// term can reorder within a band without ever crossing a larger band gap.
fn saturating_elapsed(elapsed_ms: u64, half_life_ms: f64) -> f64 {
    let e = elapsed_ms as f64;
    e / (e + half_life_ms.max(1.0))
}

/// The dumb-but-real first policy: closed-form, fast, deterministic. Tiers are
/// spaced so the bounded age/wait term reorders *within* a tier (LRU) but can
/// never lift a lease across a tier boundary. Constructed with explicit values
/// (defaults via `Default`) per the substrate config pattern — thresholds are
/// code, not env vars.
#[derive(Debug, Clone)]
pub struct TieredArbiter {
    /// Reclaim tier bases. Expired bytes are overdue (take first), then `Hard`
    /// (tolerates a yank), then `Graceful` (ask first). Gaps must exceed
    /// `age_weight` so age never crosses a tier.
    expired_base: f64,
    hard_base: f64,
    graceful_base: f64,
    /// Within-tier LRU pull: older leases score higher, bounded below the gap.
    age_weight: f64,
    age_half_life_ms: f64,
    /// Demand tier bases by the request's reclaim policy — a realtime `Pinned`
    /// demand (a starting call) outranks elastic inference at rest.
    pinned_demand_base: f64,
    graceful_demand_base: f64,
    hard_demand_base: f64,
    /// Anti-starvation: how hard waiting lifts a demand, and how fast. Large
    /// enough that a long-waited low-value demand eventually crosses a fresh
    /// high-value one.
    wait_weight: f64,
    wait_half_life_ms: f64,
}

impl Default for TieredArbiter {
    fn default() -> Self {
        Self {
            expired_base: 3_000.0,
            hard_base: 2_000.0,
            graceful_base: 1_000.0,
            age_weight: 900.0, // < 1_000 tier gap → age reorders within, never across
            age_half_life_ms: 60_000.0, // a minute-old lease is ~half-aged
            pinned_demand_base: 3_000.0,
            graceful_demand_base: 1_000.0,
            hard_demand_base: 500.0,
            wait_weight: 4_000.0, // a long wait can lift a graceful demand past a fresh pinned one
            wait_half_life_ms: 30_000.0,
        }
    }
}

impl LeaseArbiter for TieredArbiter {
    fn name(&self) -> &str {
        "tiered"
    }

    fn reclaim_score(&self, lease: &ResourceLease, ctx: &ArbiterContext) -> f64 {
        let base = if lease.is_expired(ctx.now_ms) {
            self.expired_base
        } else {
            match lease.reclaim_policy {
                ReclaimPolicy::Hard => self.hard_base,
                ReclaimPolicy::Graceful => self.graceful_base,
                // Active pinned is never reclaimable — guard even though the
                // ledger filters it before scoring.
                ReclaimPolicy::Pinned => return f64::MIN,
            }
        };
        let age = ctx.now_ms.saturating_sub(lease.acquired_at_ms);
        base + self.age_weight * saturating_elapsed(age, self.age_half_life_ms)
    }

    fn demand_urgency(&self, request: &LeaseRequest, waited_ms: u64, ctx: &ArbiterContext) -> f64 {
        let base = match request.reclaim_policy {
            ReclaimPolicy::Pinned => self.pinned_demand_base,
            ReclaimPolicy::Graceful => self.graceful_demand_base,
            ReclaimPolicy::Hard => self.hard_demand_base,
        };
        let wait = self.wait_weight * saturating_elapsed(waited_ms, self.wait_half_life_ms);
        // Contention steepens urgency: under pressure, waiting hurts more.
        (base + wait) * (1.0 + ctx.pressure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::lease::ResourceKind;

    fn lease(id: &str, policy: ReclaimPolicy, acquired: u64, expires: u64) -> ResourceLease {
        ResourceLease {
            lease_id: id.into(),
            consumer_id: "serving".into(),
            kind: ResourceKind::Vram,
            bytes: 1_000,
            acquired_at_ms: acquired,
            expires_at_ms: expires,
            reclaim_policy: policy,
        }
    }

    fn req(policy: ReclaimPolicy) -> LeaseRequest {
        LeaseRequest {
            consumer_id: "serving".into(),
            kind: ResourceKind::Vram,
            bytes: 1_000,
            ttl_ms: 1_000,
            reclaim_policy: policy,
        }
    }

    // what this catches: the dumb policy must keep tiers separated — expired >
    // Hard > Graceful — with the age term reordering only WITHIN a tier and
    // never lifting a younger-tier lease across a boundary. If age could cross a
    // tier, a long-lived Graceful lease would be reclaimed before a fresh Hard
    // one, inverting the safety ordering the ledger relies on.
    #[test]
    fn reclaim_score_separates_tiers_and_breaks_ties_by_age() {
        let a = TieredArbiter::default();
        let ctx = ArbiterContext {
            now_ms: 100_000,
            pressure: 0.0,
        };

        let expired = a.reclaim_score(&lease("e", ReclaimPolicy::Graceful, 0, 50_000), &ctx);
        let hard = a.reclaim_score(&lease("h", ReclaimPolicy::Hard, 0, u64::MAX), &ctx);
        let graceful_old =
            a.reclaim_score(&lease("g_old", ReclaimPolicy::Graceful, 0, u64::MAX), &ctx);
        let graceful_new = a.reclaim_score(
            &lease("g_new", ReclaimPolicy::Graceful, 90_000, u64::MAX),
            &ctx,
        );

        // tiers never cross, even though graceful_old is maximally aged
        assert!(expired > hard, "expired outranks hard");
        assert!(
            hard > graceful_old,
            "hard outranks even the oldest graceful"
        );
        // within the graceful tier, older (LRU) scores higher
        assert!(
            graceful_old > graceful_new,
            "older graceful reclaimed first"
        );

        // active pinned is the never-reclaim guard
        assert_eq!(
            a.reclaim_score(&lease("p", ReclaimPolicy::Pinned, 0, u64::MAX), &ctx),
            f64::MIN
        );
    }

    // what this catches: the anti-starvation crossover the directive demands —
    // "cost goes up over time." A graceful demand that has waited long enough
    // must eventually out-score a freshly-arrived pinned demand, or a low-value
    // request could be starved forever behind a stream of high-value ones.
    #[test]
    fn demand_urgency_rises_with_wait_until_it_crosses_a_fresh_higher_tier() {
        let a = TieredArbiter::default();
        let ctx = ArbiterContext {
            now_ms: 0,
            pressure: 0.0,
        };

        let fresh_pinned = a.demand_urgency(&req(ReclaimPolicy::Pinned), 0, &ctx);
        let fresh_graceful = a.demand_urgency(&req(ReclaimPolicy::Graceful), 0, &ctx);
        let waited_graceful = a.demand_urgency(&req(ReclaimPolicy::Graceful), 120_000, &ctx);

        assert!(
            fresh_pinned > fresh_graceful,
            "at equal wait, pinned outranks graceful"
        );
        assert!(
            waited_graceful > fresh_pinned,
            "a long-waited graceful eventually crosses a fresh pinned — nothing waits forever"
        );
    }

    // what this catches: pressure steepens urgency without reordering reclaim.
    // Demand urgency must scale up under contention (the same wait hurts more
    // when the resource is scarce), so the daemon grants more aggressively under
    // pressure — but reclaim ORDER is pressure-invariant (it scales uniformly).
    #[test]
    fn pressure_scales_demand_urgency_but_not_reclaim_order() {
        let a = TieredArbiter::default();
        let calm = ArbiterContext {
            now_ms: 10_000,
            pressure: 0.0,
        };
        let busy = ArbiterContext {
            now_ms: 10_000,
            pressure: 1.0,
        };

        let u_calm = a.demand_urgency(&req(ReclaimPolicy::Graceful), 5_000, &calm);
        let u_busy = a.demand_urgency(&req(ReclaimPolicy::Graceful), 5_000, &busy);
        assert!(u_busy > u_calm, "contention raises demand urgency");

        // reclaim order: under either pressure, hard still outranks graceful
        let hard = lease("h", ReclaimPolicy::Hard, 0, u64::MAX);
        let graceful = lease("g", ReclaimPolicy::Graceful, 0, u64::MAX);
        assert!(a.reclaim_score(&hard, &calm) > a.reclaim_score(&graceful, &calm));
        assert!(a.reclaim_score(&hard, &busy) > a.reclaim_score(&graceful, &busy));
    }
}
