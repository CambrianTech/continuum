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

/// A class of work, not a size of work.
///
/// Why this exists: the gate began as a single scalar (serving bytes), which
/// answers only "can this node HOLD the job". That is the right question for one
/// job kind and the wrong question for a grid. A node with 6 GB and no CUDA is
/// correctly refused a 35B LLM and is perfectly capable of a YOLO-class CNN, an
/// embedding batch, or acting as a client — and a scalar cannot say so. Under the
/// low-end tier contract (a weak machine is CLIENT-FIRST, excluded from GPU
/// personas, but "we may find some things it can help with") that expressiveness
/// IS the contract: exclusion from one class must not read as worthlessness.
///
/// Open-ended on purpose — new classes arrive with new hardware, and an
/// exhaustive enum would make the substrate the bottleneck on what a node may
/// offer. Compared by value, so an unknown class simply matches nobody rather
/// than matching everybody.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Capability(pub String);

impl Capability {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }
    /// Text generation on a language model — the class the byte floor was
    /// originally written for.
    pub fn llm_inference() -> Self {
        Self::new("llm-inference")
    }
    /// Convolutional vision work (detection, classification). Named because it is
    /// the concrete thing a below-floor node was called out as able to contribute.
    pub fn vision_cnn() -> Self {
        Self::new("vision-cnn")
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The classes a node claims. A SET, because one machine is several kinds of
/// resource at once and collapsing that to a single label is what forced the
/// all-or-nothing verdict.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapabilitySet(Vec<Capability>);

impl CapabilitySet {
    pub fn new(caps: impl IntoIterator<Item = Capability>) -> Self {
        let mut v: Vec<Capability> = caps.into_iter().collect();
        v.sort();
        v.dedup();
        Self(v)
    }
    pub fn offers(&self, capability: &Capability) -> bool {
        self.0.contains(capability)
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = &Capability> {
        self.0.iter()
    }
}

/// Whether work can be divided across nodes — the property that decides if a
/// POOL of modest machines is real capacity or an expensive way to be slow.
///
/// This is the distinction that keeps "twenty office machines are a resource"
/// honest. Both answers are about the same silicon; only the work differs:
///
/// - [`WorkShape::ThroughputDivisible`] — independent units with no serial
///   dependency between them (N persona turns, N video streams through a
///   detector, N embedding batches, N graded benchmark cards). A pool is
///   genuinely better here than one fast machine, and this is where a grid of
///   misfit hardware compounds.
/// - [`WorkShape::LatencyCoupled`] — one logical computation whose parts depend
///   on each other within a deadline (a single forward pass). Splitting it over a
///   LAN pays a network round trip per dependency and loses badly to one capable
///   machine. Sharding a model across an office is this case, which is why the
///   answer to a big model is paging and residency, not scatter.
///
/// A governor that cannot tell these apart will either waste a pool or promise
/// throughput it cannot deliver — so the gate refuses to guess, and the caller
/// must say which it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkShape {
    ThroughputDivisible,
    LatencyCoupled,
}

/// Every requirement a job places, in one place.
/// What a job needs to run at all. Deliberately a floor, not a preference: a
/// preference belongs in ranking, and mixing the two is how a hard gate quietly
/// becomes a soft one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    /// Serving bytes the job cannot run below.
    pub min_bytes: u64,
    /// What KIND of work this is. A byte floor alone cannot express "this is a
    /// vision job" — and without it a node is either eligible for everything it
    /// has room for or nothing at all.
    pub capability: Capability,
    /// Whether the work can be split across nodes, which decides whether a POOL
    /// of small machines is capacity or noise. See [`WorkShape`].
    pub shape: WorkShape,
}

/// What a node says it can provide for this job.
///
/// Both fields are the node's own claims. `node_total_bytes` is carried so an
/// impossible claim is detectable here — before any work is dispatched — rather
/// than only after a delivery fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub advertised_bytes: u64,
    /// The node's own advertised physical ceiling.
    pub node_total_bytes: u64,
    /// What this node claims it can DO, not merely hold. A weak node advertising
    /// `VisionCnn` and not `LlmInference` is making an honest, useful offer — the
    /// byte gate alone would have rendered it as "too small for everything".
    pub capabilities: CapabilitySet,
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
    /// The node does not do this KIND of work. Distinct from `Insufficient` on
    /// purpose: "too small for this job" and "does not do vision at all" are
    /// different facts, and collapsing them is what made a capable-but-small node
    /// look like dead weight. A node refused here may be eligible for the very
    /// next job.
    WrongCapability { wanted: Capability },
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
    // KIND before SIZE. A node that does not do this class of work is not a small
    // node — it is the wrong node, and reporting it as `Insufficient` would tell a
    // caller to look for more memory when more memory would not help. Checking it
    // first also keeps the two verdicts from masking each other: a vision-only box
    // asked for a 35B answers "I don't do that", not "I'm 30 GB short".
    if !offer.capabilities.offers(&requirement.capability) {
        return Eligibility::WrongCapability {
            wanted: requirement.capability.clone(),
        };
    }
    if offer.advertised_bytes < requirement.min_bytes {
        return Eligibility::Insufficient {
            short_by: requirement.min_bytes - offer.advertised_bytes,
        };
    }
    Eligibility::Eligible
}

/// What a POOL of nodes is actually worth for one requirement.
///
/// This exists because "twenty office machines are a resource" is true for some
/// work and false for other work, and a governor that cannot tell which will
/// either strand real capacity or promise throughput it cannot deliver. The
/// answer is a function of [`WorkShape`], which is why that field is on the
/// requirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PoolCapacity {
    /// The work divides, and this many members each cleared the gate on their
    /// own. Capacity scales with the count - a room of modest machines is
    /// genuinely more than one of them here.
    Parallel { members: usize },
    /// The work does not divide. Capacity is ONE eligible member however many
    /// are standing by; the others are not capacity for this job, and counting
    /// them would be the fallacy this type exists to refuse.
    Single,
    /// No member cleared the gate. Deliberately its own variant rather than
    /// `Parallel { members: 0 }` - "there is a pool, of size zero" reads as a
    /// pool, and a caller that pattern-matches on `Parallel` would treat an
    /// empty one as capacity.
    None,
}

/// Decide what a pool of offers is worth for `requirement`.
///
/// **Members are gated individually, and their memory is never summed.** Ten
/// 8 GB nodes are not an 80 GB node: the unit of work still has to fit somewhere.
/// For [`WorkShape::ThroughputDivisible`] the win is CONCURRENCY (N units in
/// flight), not size - so a 24 GB job over a pool of 8 GB machines is
/// [`PoolCapacity::None`], correctly, however many machines there are.
///
/// For [`WorkShape::LatencyCoupled`] the pool adds nothing at all: splitting one
/// forward pass over a LAN pays a round trip per dependency. The answer to a
/// model that does not fit is paging and residency, not scatter.
pub fn pool_capacity(requirement: &Requirement, offers: &[Offer]) -> PoolCapacity {
    let members = offers
        .iter()
        .filter(|o| eligible(requirement, o).is_eligible())
        .count();
    if members == 0 {
        return PoolCapacity::None;
    }
    match requirement.shape {
        WorkShape::ThroughputDivisible => PoolCapacity::Parallel { members },
        WorkShape::LatencyCoupled => PoolCapacity::Single,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gb(n: u64) -> u64 {
        n * 1024 * 1024 * 1024
    }

    /// The original axis: an LLM job, which is one indivisible forward pass.
    fn need(n: u64) -> Requirement {
        need_class(n, Capability::llm_inference(), WorkShape::LatencyCoupled)
    }

    fn need_class(n: u64, capability: Capability, shape: WorkShape) -> Requirement {
        Requirement {
            min_bytes: gb(n),
            capability,
            shape,
        }
    }

    fn offer(advertised: u64, total: u64) -> Offer {
        offer_class(advertised, total, [Capability::llm_inference()])
    }

    fn offer_class(
        advertised: u64,
        total: u64,
        caps: impl IntoIterator<Item = Capability>,
    ) -> Offer {
        Offer {
            advertised_bytes: gb(advertised),
            node_total_bytes: gb(total),
            capabilities: CapabilitySet::new(caps),
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

    /// what this catches: THE LOW-END TIER CONTRACT - a node excluded from one
    /// class being rendered as worthless. This box cannot host a 35B and can
    /// absolutely run a detector; before the class axis existed the byte floor
    /// made those the same verdict, and a small node was dead weight for every
    /// job rather than the wrong node for some of them.
    #[test]
    fn a_small_node_is_eligible_for_a_class_it_actually_serves() {
        let weak = offer_class(6, 6, [Capability::vision_cnn()]);
        let vision = need_class(4, Capability::vision_cnn(), WorkShape::ThroughputDivisible);
        let llm = need(24);
        assert!(eligible(&vision, &weak).is_eligible(), "it does this work");
        assert!(!eligible(&llm, &weak).is_eligible(), "and not that work");
    }

    /// what this catches: the two refusals collapsing into one. "Too small" tells
    /// a caller to look for more memory; for a wrong-class node more memory would
    /// not help, and reporting the size verdict would send the caller after a fix
    /// that cannot work. The class check therefore runs FIRST, and this pins that
    /// ordering with a node that is enormous and still wrong.
    #[test]
    fn the_wrong_class_is_never_reported_as_merely_too_small() {
        let huge_but_vision_only = offer_class(80, 80, [Capability::vision_cnn()]);
        let e = eligible(&need(24), &huge_but_vision_only);
        assert_eq!(
            e,
            Eligibility::WrongCapability {
                wanted: Capability::llm_inference()
            },
            "size never rescues the wrong class, and never masks it either: {e:?}"
        );
    }

    /// what this catches: the void check being demoted below the class check. A
    /// node claiming past its own ceiling is lying whatever class it claims, and
    /// the lie must stay the loudest fact - otherwise an overclaimer could hide
    /// behind a class mismatch and never be recorded as dishonest.
    #[test]
    fn an_impossible_claim_stays_void_even_in_the_wrong_class() {
        let lying_vision_node = offer_class(64, 16, [Capability::vision_cnn()]);
        assert!(eligible(&need(24), &lying_vision_node).is_void());
    }

    /// what this catches: a node with no claims at all being treated as universal
    /// rather than as offering nothing. An empty set must match NOTHING - the
    /// opposite reading (says nothing => can do anything) is how a shell that
    /// advertises no specifics ends up eligible for everything.
    #[test]
    fn claiming_no_class_offers_nothing_rather_than_everything() {
        let silent = offer_class(80, 80, []);
        assert!(!eligible(&need(1), &silent).is_eligible());
    }

    /// what this catches: THE POOL FALLACY. Twenty machines that each fall short
    /// are not one machine that clears it - memory does not sum across a LAN, and
    /// a governor that adds it up will dispatch a job nobody can hold.
    #[test]
    fn a_pool_of_small_machines_is_not_one_big_machine() {
        let pool: Vec<Offer> = (0..20).map(|_| offer(8, 8)).collect();
        let big_job = need_class(
            24,
            Capability::llm_inference(),
            WorkShape::ThroughputDivisible,
        );
        assert_eq!(pool_capacity(&big_job, &pool), PoolCapacity::None);
    }

    /// what this catches: the same silicon being valued identically for work that
    /// divides and work that does not. This is the whole reason WorkShape exists:
    /// twenty machines are twenty lanes of detector frames and are still exactly
    /// one forward pass.
    #[test]
    fn a_pool_is_capacity_only_when_the_work_divides() {
        let pool: Vec<Offer> = (0..20)
            .map(|_| {
                offer_class(
                    8,
                    8,
                    [Capability::vision_cnn(), Capability::llm_inference()],
                )
            })
            .collect();
        let frames = need_class(4, Capability::vision_cnn(), WorkShape::ThroughputDivisible);
        let one_pass = need_class(4, Capability::llm_inference(), WorkShape::LatencyCoupled);
        assert_eq!(
            pool_capacity(&frames, &pool),
            PoolCapacity::Parallel { members: 20 }
        );
        assert_eq!(pool_capacity(&one_pass, &pool), PoolCapacity::Single);
    }

    /// what this catches: an empty pool reading as a pool. `Parallel { members: 0 }`
    /// would satisfy a `matches!(.., Parallel { .. })` check and quietly promise
    /// capacity that is not there.
    #[test]
    fn an_empty_pool_is_none_not_a_pool_of_zero() {
        let ineligible: Vec<Offer> = vec![offer(2, 2)];
        let job = need_class(
            24,
            Capability::llm_inference(),
            WorkShape::ThroughputDivisible,
        );
        assert_eq!(pool_capacity(&job, &ineligible), PoolCapacity::None);
        assert_eq!(pool_capacity(&job, &[]), PoolCapacity::None);
    }
}
