//! Eligibility — can this node actually host this job? A **hard gate, evaluated
//! before price**.
//!
//! ## Why this is a correctness rule, not an anti-fraud measure
//!
//! A node that cannot host a job should not be offered it, whether or not anyone
//! is attacking. This would be right with no economy at all — which makes it a
//! sturdier foundation than a cost that has to be tuned.
//!
//! It happens to also close the one hole [`super::settlement::Reputation::trust_lower_bound`]
//! leaves open, and that is worth understanding because it explains the ordering.
//!
//! ## How it collapses cross-class churn into same-class
//!
//! `trust_lower_bound` prices an unproven seller at ~0.2, which takes sybil churn
//! from 100/100 absorbed jobs to 0/100 — **within a class**. Cross-class churn
//! survives it: a shell advertises arbitrarily cheap specs, lands in a class where
//! no proven competitor exists, and wins by default rather than by trust.
//!
//! The attack needs an **uncontested cheap class to hide in**. Eligibility removes
//! that terrain: if a node is only offered jobs its advertised capability actually
//! meets, then to be eligible for real work a shell must claim capability
//! comparable to real competitors — and the moment it does, it is in a contested
//! class where `trust_lower_bound` already bites completely.
//!
//! **Advertise cheap, or win real jobs. Not both.** And a shell that inflates its
//! claim to qualify is [`Eligibility::VoidClaim`] here, or `Overquoted` at
//! settlement — dishonest rather than merely short, which is the worse verdict.
//!
//! So the two levers are not redundant barriers. This one removes the ground the
//! attack stands on; the mint stake (M5's lane) prices the cost of *trying*, which
//! this does not.
//!
//! ## Ordering is load-bearing
//!
//! Eligibility is evaluated **before** price and **independently of** reputation.
//! No amount of cheapness makes an ineligible node eligible, and a spotless record
//! does not let a node take work it cannot hold. Fold this into a scoring function
//! and the gate becomes a weight — which is exactly how "cheap enough" starts
//! winning jobs it cannot serve.

/// What a job needs to run at all. Deliberately a floor, not a preference: a
/// preference belongs in ranking, and mixing the two is how a hard gate quietly
/// becomes a soft one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Requirement {
    /// Serving bytes the job cannot run below.
    pub min_bytes: u64,
}

/// What a node says it can provide for this job.
///
/// Both fields are the node's own claims. `node_total_bytes` is carried so an
/// impossible claim is detectable here — before any work is dispatched — rather
/// than only after a delivery fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offer {
    pub advertised_bytes: u64,
    /// The node's own advertised physical ceiling.
    pub node_total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Eligibility {
    /// May be offered the job. Ranking (price, reputation, latency) happens after
    /// this, never instead of it.
    Eligible,
    /// Honest but too small. Not a fraud signal — most nodes are ineligible for
    /// most jobs, and that is ordinary.
    Insufficient { short_by: u64 },
    /// Claimed more than its own advertised ceiling. The claim is **void**: it is
    /// not a cheap bid to be outranked, it is not an offer at all. Refused here so
    /// an impossible promise never reaches dispatch.
    VoidClaim { over_by: u64 },
}

impl Eligibility {
    pub fn is_eligible(&self) -> bool {
        matches!(self, Eligibility::Eligible)
    }

    /// Did the node assert something impossible, as opposed to being honestly
    /// small? Kept distinct so genuine overclaiming cannot hide among the ordinary
    /// mass of too-small nodes.
    pub fn is_void(&self) -> bool {
        matches!(self, Eligibility::VoidClaim { .. })
    }
}

/// Decide whether `offer` may be considered for `requirement`.
///
/// Pure, and takes **no price and no reputation** — not an oversight. Passing
/// them in would invite weighing them, and the entire value of this gate is that
/// it cannot be outbid.
///
/// The void check runs first: a node claiming beyond its own ceiling is refused
/// even when the inflated claim would have satisfied the requirement. Otherwise a
/// shell could qualify for anything by simply claiming more.
pub fn eligible(requirement: &Requirement, offer: &Offer) -> Eligibility {
    if offer.advertised_bytes > offer.node_total_bytes {
        return Eligibility::VoidClaim {
            over_by: offer.advertised_bytes - offer.node_total_bytes,
        };
    }
    if offer.advertised_bytes < requirement.min_bytes {
        return Eligibility::Insufficient {
            short_by: requirement.min_bytes - offer.advertised_bytes,
        };
    }
    Eligibility::Eligible
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gb(n: u64) -> u64 {
        n * 1024 * 1024 * 1024
    }

    fn need(n: u64) -> Requirement {
        Requirement { min_bytes: gb(n) }
    }

    fn offer(advertised: u64, total: u64) -> Offer {
        Offer {
            advertised_bytes: gb(advertised),
            node_total_bytes: gb(total),
        }
    }

    /// what this catches: THE CROSS-CLASS HOLE. A shell advertising cheap specs
    /// must not be offered real work AT ANY PRICE. This is the property that
    /// collapses cross-class churn into same-class, where trust_lower_bound
    /// already takes sybil absorption to 0/100.
    #[test]
    fn a_cheap_shell_is_not_offered_work_it_cannot_hold() {
        let e = eligible(&need(24), &offer(8, 8));
        assert!(!e.is_eligible());
        assert_eq!(e, Eligibility::Insufficient { short_by: gb(16) });
        assert!(
            !e.is_void(),
            "honestly small is not fraud — most nodes are ineligible for most jobs"
        );
    }

    /// what this catches: qualifying by simply claiming more. If inflating the
    /// claim let a shell pass the gate, the filter would be decorative — the
    /// attacker's specs are self-reported, so the gate must reject impossible
    /// claims rather than believe them.
    #[test]
    fn inflating_the_claim_to_qualify_is_void_not_eligible() {
        // Claims 64GB (would satisfy a 24GB job) on a 16GB card.
        let e = eligible(&need(24), &offer(64, 16));
        assert!(!e.is_eligible(), "an impossible claim never qualifies");
        assert!(e.is_void());
        assert_eq!(e, Eligibility::VoidClaim { over_by: gb(48) });
    }

    /// what this catches: the void check running AFTER the sufficiency check. A
    /// node overclaiming *below* the requirement is still lying, and the verdict
    /// must say so rather than reporting the more forgiving "too small".
    #[test]
    fn an_impossible_claim_is_void_even_when_it_is_also_too_small() {
        // Claims 12GB on an 8GB card, for a 24GB job — both lying AND short.
        let e = eligible(&need(24), &offer(12, 8));
        assert!(e.is_void(), "the lie is the more important fact: {e:?}");
    }

    /// what this catches: an off-by-one at the boundary that would exclude a node
    /// that exactly fits. The requirement is a floor the job cannot run BELOW —
    /// meeting it exactly is meeting it.
    #[test]
    fn exactly_meeting_the_requirement_is_eligible() {
        assert!(eligible(&need(24), &offer(24, 24)).is_eligible());
        assert!(!eligible(&need(24), &offer(23, 24)).is_eligible());
    }

    /// what this catches: THE ORDERING, which is the whole point. `eligible` takes
    /// no price and no reputation, so this test is really a signature check —
    /// if a future refactor adds them as parameters, the gate has become a weight
    /// and can be outbid. A spotless record must not let a node take work it
    /// cannot hold, and a rock-bottom price must not either.
    #[test]
    fn eligibility_cannot_be_bought_or_earned() {
        let too_small = offer(8, 8);
        let job = need(24);
        // There is deliberately no argument to vary here. The gate is a function
        // of capability alone; the only way to pass it is to be big enough.
        for _ in 0..100 {
            assert!(
                !eligible(&job, &too_small).is_eligible(),
                "no repetition, price, or standing changes a capability verdict"
            );
        }
    }

    /// what this catches: a node being judged differently because it is us. Same
    /// gate for local and remote — there are no node types, and an offer we make
    /// to ourselves is judged exactly like one a peer makes to us.
    #[test]
    fn the_local_node_is_gated_identically() {
        assert!(!eligible(&need(48), &offer(32, 32)).is_eligible());
        assert!(eligible(&need(16), &offer(32, 32)).is_eligible());
    }
}
