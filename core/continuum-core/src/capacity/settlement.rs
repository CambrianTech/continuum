//! Settlement — what was PROMISED against what was DELIVERED, and the divergence
//! between them, which is reputation.
//!
//! The adversarial half of the market. `capacity::market::entry_price` values a
//! node on its **self-reported specs** — an IPO on a prospectus nobody has
//! audited. That is the fraud surface: a node overstating its free bytes is not
//! buggy, it is **selling supply it cannot deliver**, and under a specs-only
//! market it gets paid for it.
//!
//! Settlement is what makes the claim honest. The two halves are deliberately
//! adversarial rather than two views of the same optimism: pricing is generous
//! and forward-looking, settlement is strict and backward-looking, and reputation
//! is the running record of where they disagreed.
//!
//! Joel's shape: **IPO on specs -> trade on delivery -> Merkle-ledger reputation
//! -> market-as-selection.** A node that consistently oversells prices itself out
//! without anyone policing it, because buyers can read its verification rate.
//!
//! ## The invariants, and where each comes from
//!
//! 1. **An unverifiable delivery is not a successful one.** No delivery record
//!    means `Unsettled`, never `Honored`. Absence is not evidence — this is the
//!    absence-rendered-as-a-positive-fact bug class restated as an economic rule.
//!    A market that counts silence as success pays for work nobody observed.
//! 2. **Overdelivery does not offset underdelivery.** Promises settle
//!    individually; you cannot average your way out of a broken one. A node that
//!    is magnificent four times and absent once has broken one promise, and the
//!    buyer of the fifth does not care about the other four.
//! 3. **A promise nobody could have kept is fraud at QUOTE time**, not a delivery
//!    failure. Quoting above your own physical ceiling is detectable before any
//!    work happens, and it is a different — worse — fact than failing to deliver
//!    something plausible.
//! 4. **Settlement is symmetric.** The same function judges MY deliveries to
//!    peers. Every node is both consumer and provider (there are no node types),
//!    and a market where you only ever grade others is not a market.

use crate::capacity::grid_budget::BudgetSource;

/// What a node committed to, at quote time.
///
/// `claimed_bytes` is the supply advertised for THIS transaction, and
/// `node_total_bytes` is the physical ceiling it advertised for itself. Keeping
/// both lets settlement distinguish "failed to deliver something plausible" from
/// "quoted something impossible" — invariant 3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Promise {
    /// Who committed. `Local` means this node promised — settlement is symmetric.
    pub seller: BudgetSource,
    /// Supply advertised for this transaction.
    pub claimed_bytes: u64,
    /// The seller's own advertised physical ceiling at quote time.
    pub node_total_bytes: u64,
    /// What the buyer actually required, so a settlement can be read without the
    /// original request in hand.
    pub required_bytes: u64,
}

/// What actually happened. `None` at the call site means no record exists — and
/// that is a distinct outcome from a recorded failure, not a synonym for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    /// Did the work actually land?
    pub landed: bool,
    /// Supply actually made available. Measured, not claimed.
    pub delivered_bytes: u64,
}

/// The verdict on one promise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Delivered what was required. The only outcome that builds reputation.
    Honored,
    /// Reached the buyer, but short of the requirement.
    Shortfall { missing_bytes: u64 },
    /// The work did not land at all.
    Failed,
    /// **Impossible at quote time** — claimed more than its own advertised
    /// ceiling. Detectable before any work happens, and a worse fact than a
    /// delivery failure: this is a lie rather than a disappointment.
    Overquoted { over_by_bytes: u64 },
    /// No delivery record exists. NOT a success and NOT a failure — the market
    /// simply cannot say, and saying otherwise invents evidence.
    Unsettled,
}

impl Verdict {
    /// Only `Honored` counts toward reputation. Everything else — including
    /// `Unsettled` — does not, because a market that rewards unobserved work
    /// rewards claiming rather than doing.
    pub fn builds_reputation(&self) -> bool {
        matches!(self, Verdict::Honored)
    }

    /// Did the seller assert something untrue, as opposed to merely
    /// underperforming? Overquoting is the fraud signal; a shortfall is a bad
    /// day. Conflating them would let genuine overselling hide inside ordinary
    /// variance.
    pub fn is_dishonest(&self) -> bool {
        matches!(self, Verdict::Overquoted { .. })
    }
}

/// One settled transaction, ready to append to the ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settlement {
    pub seller: BudgetSource,
    pub verdict: Verdict,
}

/// Settle one promise against its delivery.
///
/// Pure: no I/O, no clock, no ledger writes — so the rule that decides whether a
/// node gets paid is unit-testable in isolation, and there is exactly one place
/// it is decided.
///
/// Order matters. The quote-time check runs FIRST: a node that promised more than
/// it physically has is `Overquoted` even if it happens to deliver, because it
/// got lucky rather than honest, and the market should price the claim it made.
pub fn settle(promise: &Promise, delivery: Option<&Delivery>) -> Settlement {
    let verdict = if promise.claimed_bytes > promise.node_total_bytes {
        // Invariant 3 — checkable before any work happens. Same shape as
        // grid_budget's per-node clamp, which refuses to believe an offer larger
        // than the offering node.
        Verdict::Overquoted {
            over_by_bytes: promise.claimed_bytes - promise.node_total_bytes,
        }
    } else {
        match delivery {
            // Invariant 1 — no record is not a pass.
            None => Verdict::Unsettled,
            Some(d) if !d.landed => Verdict::Failed,
            Some(d) if d.delivered_bytes < promise.required_bytes => Verdict::Shortfall {
                missing_bytes: promise.required_bytes - d.delivered_bytes,
            },
            Some(_) => Verdict::Honored,
        }
    };

    Settlement {
        seller: promise.seller.clone(),
        verdict,
    }
}

/// A node's standing, derived from settled transactions.
///
/// Reputation is a **verification rate**, not a score someone assigns: honored
/// over settled. Deliberately NOT "honored over all promises" — an unsettled
/// promise is evidence of nothing and must not be allowed to dilute a record in
/// either direction (invariant 1 again, on the aggregate).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Reputation {
    pub honored: u32,
    /// Settled transactions — those the market could actually judge.
    pub settled: u32,
    /// Promises that could not be judged at all. Surfaced rather than folded in,
    /// because a seller with many unsettled promises is itself a signal.
    pub unsettled: u32,
    /// Quote-time lies. Tracked separately from shortfalls: a buyer may forgive a
    /// bad day, but overquoting is a statement about the seller.
    pub dishonest: u32,
}

impl Reputation {
    /// Fold one settlement in.
    pub fn record(&mut self, s: &Settlement) {
        match &s.verdict {
            Verdict::Unsettled => {
                self.unsettled = self.unsettled.saturating_add(1);
                return; // never counts as settled — invariant 1
            }
            Verdict::Overquoted { .. } => self.dishonest = self.dishonest.saturating_add(1),
            _ => {}
        }
        self.settled = self.settled.saturating_add(1);
        if s.verdict.builds_reputation() {
            self.honored = self.honored.saturating_add(1);
        }
    }

    /// Honored / settled. `None` when nothing has been settled — an unproven
    /// seller is UNKNOWN, not perfect and not bad. Returning 1.0 for a node with
    /// no history would let a fresh identity outrank a proven one, which is the
    /// cheapest attack on any reputation system.
    pub fn verification_rate(&self) -> Option<f64> {
        (self.settled > 0).then(|| self.honored as f64 / self.settled as f64)
    }

    /// **The number a market should price on.** A confidence-discounted rate:
    /// evidence earns trust, and absence of evidence earns a little.
    ///
    /// `(honored + PRIOR_GOOD) / (settled + PRIOR_GOOD + PRIOR_BAD)` — a Beta
    /// prior, i.e. pseudo-counts representing skepticism toward the unproven.
    ///
    /// Why not `verification_rate()` directly: a market that prices on the raw
    /// rate has to answer "what about no data?", and BOTH obvious answers are
    /// wrong. Pricing unknown as **perfect** is the sybil hole — measured on the
    /// grid market sim at 100/100 jobs absorbed with the honest node fully
    /// starved, because minting an identity is cheap and each fresh one wins at
    /// the best price. Pricing unknown as **worst** excludes every newcomer, and
    /// a market nobody can enter is not a market either.
    ///
    /// The prior dissolves the question. With the constants below an unproven
    /// seller starts at 0.2 — well beneath a proven performer (→ 1.0), well above
    /// a demonstrated failure (→ 0.0). It wins *occasionally*, when proven nodes
    /// are expensive or busy, which is exactly the capped probe budget bounded
    /// exploration wants: **a newcomer buys standing with delivered work, and
    /// minting N identities buys N small chances rather than N free wins.**
    ///
    /// Monotone in evidence, which is what makes it safe to price on: honoring
    /// raises it, failing lowers it, and no amount of churn resets an identity to
    /// better than 0.2.
    pub fn trust_lower_bound(&self) -> f64 {
        /// Optimism granted to the unproven — the size of the probe budget.
        const PRIOR_GOOD: f64 = 1.0;
        /// Skepticism toward the unproven. The ratio sets the newcomer's start.
        const PRIOR_BAD: f64 = 4.0;

        (self.honored as f64 + PRIOR_GOOD) / (self.settled as f64 + PRIOR_GOOD + PRIOR_BAD)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gb(n: u64) -> u64 {
        n * 1024 * 1024 * 1024
    }

    fn promise(claimed: u64, total: u64, required: u64) -> Promise {
        Promise {
            seller: BudgetSource::Peer("peer-a".into()),
            claimed_bytes: claimed,
            node_total_bytes: total,
            required_bytes: required,
        }
    }

    /// what this catches: silence counted as success. A market that pays for
    /// unobserved work pays for CLAIMING rather than DOING — the
    /// absence-as-positive-fact class with money attached.
    #[test]
    fn a_missing_delivery_record_is_unsettled_never_honored() {
        let s = settle(&promise(gb(16), gb(24), gb(16)), None);
        assert_eq!(s.verdict, Verdict::Unsettled);
        assert!(
            !s.verdict.builds_reputation(),
            "unobserved is not delivered"
        );
    }

    /// what this catches: overquoting hiding inside ordinary variance. Claiming
    /// more than you physically have is detectable at QUOTE time and is a
    /// different, worse fact than underdelivering something plausible.
    #[test]
    fn quoting_above_your_own_ceiling_is_dishonest_not_merely_short() {
        let s = settle(&promise(gb(64), gb(24), gb(16)), None);
        assert_eq!(
            s.verdict,
            Verdict::Overquoted {
                over_by_bytes: gb(40)
            }
        );
        assert!(s.verdict.is_dishonest());
    }

    /// what this catches: getting away with a lie by getting lucky. A node that
    /// promised what it does not have is dishonest even when the delivery
    /// happens to land — the market prices the CLAIM it made.
    #[test]
    fn overquoting_is_dishonest_even_when_the_delivery_lands() {
        let d = Delivery {
            landed: true,
            delivered_bytes: gb(16),
        };
        let s = settle(&promise(gb(64), gb(24), gb(16)), Some(&d));
        assert!(
            s.verdict.is_dishonest(),
            "lucky is not honest: {:?}",
            s.verdict
        );
    }

    #[test]
    fn delivering_the_requirement_is_honored() {
        let d = Delivery {
            landed: true,
            delivered_bytes: gb(16),
        };
        let s = settle(&promise(gb(16), gb(24), gb(16)), Some(&d));
        assert_eq!(s.verdict, Verdict::Honored);
        assert!(s.verdict.builds_reputation());
    }

    /// what this catches: averaging away a broken promise. Promises settle
    /// individually — the buyer of the failed one does not care about the four
    /// that worked.
    #[test]
    fn overdelivery_does_not_offset_a_separate_broken_promise() {
        let mut rep = Reputation::default();
        let generous = Delivery {
            landed: true,
            delivered_bytes: gb(64),
        };
        for _ in 0..4 {
            rep.record(&settle(&promise(gb(16), gb(64), gb(16)), Some(&generous)));
        }
        rep.record(&settle(
            &promise(gb(16), gb(64), gb(16)),
            Some(&Delivery {
                landed: false,
                delivered_bytes: 0,
            }),
        ));
        assert_eq!(rep.settled, 5);
        assert_eq!(rep.honored, 4);
        assert_eq!(
            rep.verification_rate(),
            Some(0.8),
            "four generous deliveries do not erase one failure"
        );
    }

    /// what this catches: unsettled promises diluting a record in EITHER
    /// direction. They are evidence of nothing and must stay out of the rate,
    /// while remaining visible as their own count.
    #[test]
    fn unsettled_promises_stay_out_of_the_rate_but_remain_visible() {
        let mut rep = Reputation::default();
        let ok = Delivery {
            landed: true,
            delivered_bytes: gb(16),
        };
        rep.record(&settle(&promise(gb(16), gb(64), gb(16)), Some(&ok)));
        rep.record(&settle(&promise(gb(16), gb(64), gb(16)), None));
        rep.record(&settle(&promise(gb(16), gb(64), gb(16)), None));
        assert_eq!(rep.settled, 1, "only judgeable transactions are settled");
        assert_eq!(rep.unsettled, 2, "but the unjudgeable ones are not hidden");
        assert_eq!(rep.verification_rate(), Some(1.0));
    }

    /// what this catches: THE cheapest attack on any reputation system — a fresh
    /// identity outranking a proven one. An unproven seller is UNKNOWN, not
    /// perfect. If this ever returns 1.0, sybil nodes win every auction.
    #[test]
    fn an_unproven_seller_is_unknown_not_perfect() {
        assert_eq!(Reputation::default().verification_rate(), None);
    }

    /// what this catches: a one-sided market. Settlement must judge OUR
    /// deliveries too — every node is consumer and provider, and grading only
    /// others is not a market.
    #[test]
    fn settlement_judges_local_deliveries_the_same_way() {
        let mine = Promise {
            seller: BudgetSource::Local,
            claimed_bytes: gb(32),
            node_total_bytes: gb(16),
            required_bytes: gb(8),
        };
        let s = settle(&mine, None);
        assert!(
            s.verdict.is_dishonest(),
            "our own overquote is judged exactly like a peer's"
        );
        assert_eq!(s.seller, BudgetSource::Local);
    }

    /// what this catches: THE SYBIL HOLE, measured. A fresh identity must NOT be
    /// priced at or near a proven performer. On the grid market sim, pricing
    /// unknown as 1.0 let churned shells absorb 100/100 jobs and starve the
    /// honest incumbent completely — minting is cheap, and each new identity won
    /// at the best price.
    #[test]
    fn an_unproven_seller_is_priced_well_below_a_proven_one() {
        let fresh = Reputation::default();
        let mut proven = Reputation::default();
        for _ in 0..20 {
            proven.record(&Settlement {
                seller: BudgetSource::Local,
                verdict: Verdict::Honored,
            });
        }
        assert!(
            fresh.trust_lower_bound() < proven.trust_lower_bound() * 0.5,
            "unproven {} must be far below proven {}",
            fresh.trust_lower_bound(),
            proven.trust_lower_bound()
        );
    }

    /// what this catches: the opposite failure — pricing unknown as WORST, which
    /// bars every newcomer. A market nobody can enter is not a market. The
    /// newcomer needs a real, bounded chance.
    #[test]
    fn but_an_unproven_seller_is_still_admissible_not_excluded() {
        let fresh = Reputation::default();
        let mut failed = Reputation::default();
        for _ in 0..20 {
            failed.record(&Settlement {
                seller: BudgetSource::Local,
                verdict: Verdict::Failed,
            });
        }
        assert!(
            fresh.trust_lower_bound() > 0.0,
            "a newcomer is not excluded"
        );
        assert!(
            fresh.trust_lower_bound() > failed.trust_lower_bound() * 2.0,
            "unproven {} must beat demonstrated failure {}",
            fresh.trust_lower_bound(),
            failed.trust_lower_bound()
        );
    }

    /// what this catches: churn resetting the clock. The whole sybil attack is
    /// "mint a new identity, get a fresh best price". N fresh identities must be
    /// worth N *small* chances, never N free wins — so a fresh identity can never
    /// outrank a node that has actually delivered.
    #[test]
    fn minting_identities_never_beats_having_delivered() {
        let mut modest = Reputation::default();
        // Deliberately imperfect: 3 of 4 honored. Even a MEDIOCRE proven node
        // must outrank an infinitely-churnable fresh one.
        for v in [
            Verdict::Honored,
            Verdict::Honored,
            Verdict::Failed,
            Verdict::Honored,
        ] {
            modest.record(&Settlement {
                seller: BudgetSource::Local,
                verdict: v,
            });
        }
        let fresh = Reputation::default();
        assert!(
            modest.trust_lower_bound() > fresh.trust_lower_bound(),
            "a 3-of-4 record ({}) must beat a fresh shell ({}), or churn wins",
            modest.trust_lower_bound(),
            fresh.trust_lower_bound()
        );
    }

    /// what this catches: a non-monotone trust curve, which would make pricing
    /// exploitable. Honoring must raise standing and failing must lower it, with
    /// no local dips a strategic actor could sit in.
    #[test]
    fn trust_moves_monotonically_with_evidence() {
        let mut r = Reputation::default();
        let mut last = r.trust_lower_bound();
        for _ in 0..10 {
            r.record(&Settlement {
                seller: BudgetSource::Local,
                verdict: Verdict::Honored,
            });
            let now = r.trust_lower_bound();
            assert!(now > last, "honoring must always raise trust");
            last = now;
        }
        for _ in 0..10 {
            r.record(&Settlement {
                seller: BudgetSource::Local,
                verdict: Verdict::Failed,
            });
            let now = r.trust_lower_bound();
            assert!(now < last, "failing must always lower trust");
            last = now;
        }
    }

    /// what this catches: a shortfall silently rounded to success. Landing but
    /// under the requirement is its own verdict, and it names the gap.
    #[test]
    fn landing_under_the_requirement_is_a_named_shortfall() {
        let d = Delivery {
            landed: true,
            delivered_bytes: gb(10),
        };
        let s = settle(&promise(gb(16), gb(64), gb(16)), Some(&d));
        assert_eq!(
            s.verdict,
            Verdict::Shortfall {
                missing_bytes: gb(6)
            }
        );
        assert!(!s.verdict.builds_reputation());
        assert!(!s.verdict.is_dishonest(), "a bad day is not a lie");
    }
}
