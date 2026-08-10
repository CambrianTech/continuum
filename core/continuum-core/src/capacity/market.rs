//! The market layer — the economy on the identity spine, in the deterministic simulator.
//!
//! Participants IPO on their SPECS ([`entry_price`], no track record needed), then TRADE on
//! their measured DELIVERY: every job appends an [`Invoice`] to an append-only [`Ledger`] (the
//! Merkle chain, order-preserving here), and [`reputation`] re-prices them. Routing minimizes the
//! RISK-ADJUSTED price ([`expected_cost`] = entry ÷ reputation), so an unreliable node is
//! EXPENSIVE per unit of delivered value and gets AVOIDED — not chosen because it looks cheap.
//! Honesty is the dominant strategy by construction: the cheapest way to win jobs is to actually
//! deliver, because the ledger re-prices you from what you did, not what you claimed.
//!
//! Supply-side pricing lives here. The DEMAND-side scarcity multiplier (λ / market-clearing under
//! contention) plugs in at [`clearing_price`] — that seam is the grid-market lane (BigMama's λ).
//! Deterministic: same participants + same job stream → same outcome, always. Zero hardware.
//! Sibling of [`super::sim`] (the capacity simulator); this is the money on top of the capacity.

/// Advertised hardware — the prospectus. [`entry_price`] is a pure, MONOTONE function of this
/// (the IPO valuation over comparable specs). A liar can inflate these numbers, but the first
/// [`Invoice`] measures reality and [`reputation`] corrects the price — the spec is only the prior.
#[derive(Debug, Clone, Copy)]
pub struct Specs {
    pub vram_gb: u64,
    /// silicon-class × cores, collapsed to one comparable scalar (bytes/names aren't comparable).
    pub compute_score: u64,
    /// perf-per-watt, 1..=100 (higher = better). The efficiency axis the market pays a premium for.
    pub power_efficiency: u64,
}

/// The IPO valuation: objective, hardware-derived, needs no reputation. MONOTONE in every good
/// axis — that monotonicity is the invariant; the exact coefficients are a free knob.
pub fn entry_price(s: &Specs) -> u64 {
    (s.vram_gb * 100 + s.compute_score * 10) * s.power_efficiency.max(1) / 100
}

/// One fulfilled job's receipt — MEASURED, not advertised. In production a signed, hash-linked
/// forge-alloy artifact; here the measured facts + append order (the chain).
#[derive(Debug, Clone, Copy)]
pub struct Invoice {
    /// Did it actually deliver what was required? A liar ADVERTISES fit and fails here.
    pub met_requirement: bool,
    /// Measured quality 0..=1 (a speed/correctness/reliability composite).
    pub quality: f32,
}

/// A participant's append-only ledger — the Merkle chain, order-preserving.
#[derive(Debug, Default, Clone)]
pub struct Ledger {
    pub invoices: Vec<Invoice>,
}

/// Reputation from the ledger: mean measured quality, with an UNMET requirement scored a hard 0.
/// Cold start = 1.0 (neutral) so a fresh node isn't punished before it has a record — the spec
/// alone carries the entry price until delivery speaks.
pub fn reputation(l: &Ledger) -> f32 {
    if l.invoices.is_empty() {
        return 1.0;
    }
    let sum: f32 = l
        .invoices
        .iter()
        .map(|i| if i.met_requirement { i.quality } else { 0.0 })
        .sum();
    sum / l.invoices.len() as f32
}

/// The RISK-ADJUSTED price the router minimizes: the entry ask ÷ the odds it delivers. A reliable
/// node (rep→1) quotes near its entry; an unreliable one (rep→0) is near-infinite and is AVOIDED,
/// which is why a low reputation can never win by underpricing. This is [`clearing_price`] with λ=1.
pub fn expected_cost(s: &Specs, l: &Ledger) -> f32 {
    entry_price(s) as f32 / reputation(l).max(0.01)
}

/// The final price a customer pays = the supply-side risk-adjusted ask × the DEMAND-side scarcity
/// multiplier `lambda`. λ is the grid-market-clearing price of the contended seam (BigMama's lane);
/// this crate proves the supply side with λ = 1.0. Kept as the single named seam so λ drops in
/// without touching the routing or the convergence invariants.
pub fn clearing_price(s: &Specs, l: &Ledger, lambda: f32) -> f32 {
    expected_cost(s, l) * lambda
}

/// A market participant. The `true_*` fields are HIDDEN ground truth — what it ACTUALLY delivers,
/// which a liar's advertisement contradicts. The market only ever learns them through invoices.
#[derive(Debug, Clone)]
pub struct Participant {
    pub name: &'static str,
    pub specs: Specs,
    /// Measured quality (0..=1) on the jobs it DOES fulfill.
    pub true_quality: f32,
    /// Deterministic flakiness: 0 = never fails; N ≥ 2 = fails 1 job in every N (first N-1 succeed);
    /// 1 = fails EVERY job (a pure liar / dead shell that advertised fit).
    pub fails_every: u64,
    pub ledger: Ledger,
    pub earned: u64,
    /// How many jobs have routed to it (drives the deterministic flakiness pattern).
    assigned: u64,
}

impl Participant {
    pub fn new(name: &'static str, specs: Specs, true_quality: f32, fails_every: u64) -> Self {
        Self {
            name,
            specs,
            true_quality,
            fails_every,
            ledger: Ledger::default(),
            earned: 0,
            assigned: 0,
        }
    }

    /// Whether THIS assignment meets the requirement, from the hidden truth (deterministic).
    fn delivers_now(&self) -> bool {
        match self.fails_every {
            0 => true,
            1 => false,
            n => (self.assigned + 1) % n != 0, // first n-1 succeed, every n-th fails
        }
    }
}

/// Play `jobs` identical asks through the market with λ = 1. Each routes to the lowest
/// [`expected_cost`]; the winner delivers per its HIDDEN truth, appends the measured invoice, and
/// is PAID its quoted entry price only when it actually met the requirement. Deterministic.
pub fn run_market(participants: &mut [Participant], jobs: usize) {
    for _ in 0..jobs {
        let Some(w) = (0..participants.len()).min_by(|&a, &b| {
            expected_cost(&participants[a].specs, &participants[a].ledger)
                .partial_cmp(&expected_cost(&participants[b].specs, &participants[b].ledger))
                .expect("entry prices and reputations are finite")
        }) else {
            break;
        };
        let p = &mut participants[w];
        let met = p.delivers_now();
        p.assigned += 1;
        let quality = if met { p.true_quality } else { 0.0 };
        p.ledger.invoices.push(Invoice { met_requirement: met, quality });
        if met {
            p.earned += entry_price(&p.specs);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs(vram_gb: u64, compute: u64, eff: u64) -> Specs {
        Specs { vram_gb, compute_score: compute, power_efficiency: eff }
    }

    // what this catches: THE IPO VALUATION IS MONOTONE. entry_price must rise with every good axis
    // (VRAM, compute, efficiency) — a better rig opens at a higher price with no track record. If a
    // bigger/faster/more-efficient node ever priced LOWER, the cold-start valuation is broken.
    #[test]
    fn entry_price_is_monotone_in_specs() {
        let small = specs(16, 100, 50);
        let big = specs(64, 400, 50);
        let efficient = specs(16, 100, 90);
        assert!(entry_price(&big) > entry_price(&small), "more VRAM+compute → higher entry");
        assert!(entry_price(&efficient) > entry_price(&small), "better perf/watt → higher entry");
    }

    // what this catches: A LIAR COLLAPSES. A node with cheap specs that ADVERTISES fit but always
    // fails wins the first job by being cheapest — then its ledger measures the truth, reputation
    // craters, its risk-adjusted price goes near-infinite, and it never wins again. It is NEVER paid
    // (only delivery earns), so a fraudulent prospectus self-corrects: earned=0, reputation→0.
    #[test]
    fn a_liar_collapses_to_zero_earnings_and_delists() {
        let mut m = vec![
            Participant::new("liar", specs(16, 100, 50), 0.0, 1), // cheapest, ALWAYS fails
            Participant::new("honest", specs(24, 150, 50), 1.0, 0), // pricier, reliable
        ];
        run_market(&mut m, 20);
        let liar = &m[0];
        let honest = &m[1];
        assert_eq!(liar.earned, 0, "a node that never delivers is never paid");
        assert!(reputation(&liar.ledger) < 0.05, "measured failures crater reputation");
        assert_eq!(liar.ledger.invoices.len(), 1, "it wins ONCE, is exposed, then delisted");
        assert!(honest.earned > 0, "the reliable node takes over and earns");
    }

    // what this catches: RELIABILITY BEATS FLAKINESS at equal price — the efficiency incentive. Two
    // equally-priced nodes; the reliable one's risk-adjusted price stays low while the flaky one's
    // climbs after its first miss, so the reliable one wins nearly every job and out-earns it. This
    // is "a modest rock-solid node out-earns a flaky flagship" isolated to the reliability axis.
    #[test]
    fn a_reliable_node_out_earns_an_equal_priced_flaky_one() {
        let s = specs(24, 150, 50);
        let mut m = vec![
            Participant::new("flaky", s, 1.0, 3),    // listed first; fails 1 in 3
            Participant::new("reliable", s, 1.0, 0), // never fails
        ];
        run_market(&mut m, 30);
        let flaky = &m[0];
        let reliable = &m[1];
        assert!(
            reliable.earned > flaky.earned,
            "the reliable node out-earns the equal-priced flaky one (reliable={}, flaky={})",
            reliable.earned,
            flaky.earned
        );
        assert!(reliable.ledger.invoices.len() > flaky.ledger.invoices.len(), "and wins more jobs");
    }

    // what this catches: SYBIL SHELLS NEVER ACCRUE. Spinning up N fake identities (cheap specs, no
    // real delivery) yields N zero-earners, not N reputations — each must actually deliver measured
    // jobs to earn, and the ledger's re-runnable proof can't be faked. One honest node takes it all.
    #[test]
    fn n_sybil_shells_are_n_zero_earners_not_n_reputations() {
        let mut m = vec![
            Participant::new("shell-a", specs(8, 10, 10), 0.0, 1),
            Participant::new("shell-b", specs(8, 10, 10), 0.0, 1),
            Participant::new("shell-c", specs(8, 10, 10), 0.0, 1),
            Participant::new("honest", specs(24, 150, 50), 1.0, 0),
        ];
        run_market(&mut m, 40);
        for shell in &m[0..3] {
            assert_eq!(shell.earned, 0, "a shell that never delivers never earns ({})", shell.name);
            assert!(reputation(&shell.ledger) < 0.05, "a shell accrues no reputation ({})", shell.name);
        }
        assert!(m[3].earned > 0, "the one honest node captures the market");
    }
}
