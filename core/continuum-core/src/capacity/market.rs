//! The market layer — the economy on the identity spine, in the deterministic simulator.
//!
//! Two adversarial halves, ONE identity spine, ONE reputation type.
//!
//! **This module owns the IPO.** [`entry_price`] values a participant on its advertised [`Specs`] —
//! a pure, MONOTONE valuation that needs no track record, the prospectus a fresh node lists on.
//!
//! **[`super::settlement`] owns the reputation.** [`Reputation`](super::settlement::Reputation) is
//! the append-only verification record, and its
//! [`trust_lower_bound`](super::settlement::Reputation::trust_lower_bound) is the Beta-prior discount
//! that prices the UNPROVEN as bounded exploration (≈0.2), a proven performer near 1.0, and a
//! demonstrated failure near 0.0. There is exactly ONE reputation type in the crate; the market does
//! not keep a second ledger — it prices on settlement's, so the pricing side and the audit side can
//! never drift out of agreement.
//!
//! Routing minimizes the RISK-ADJUSTED price [`expected_cost`] = entry ÷ trust: a reliable node
//! quotes near its entry, an unproven one at ≈5× it, a demonstrated failure at ≈25×. So the cheapest
//! way to win jobs is to actually deliver — honesty is the dominant strategy by construction, and a
//! freshly-minted identity can no longer out-price a PROVEN incumbent of comparable specs (the
//! sybil-churn defense, proven below against settlement's real `trust_lower_bound`).
//!
//! The DEMAND-side scarcity multiplier (λ / market-clearing under contention) plugs in at
//! [`clearing_price`] — BigMama's grid-market lane. Deterministic: same participants + same job
//! stream → same outcome. Zero hardware. Sibling of [`super::sim`] (capacity); this is the money on
//! top of the capacity, and [`super::settlement`] is the audit that keeps the money honest.
//!
//! ## The boundary this proves — and the one it does NOT (honest scope)
//! The tests hold SPECS CONSTANT so they isolate the reputation axis: within a capability class, a
//! proven node cannot be starved by churned fresh identities. What `trust_lower_bound` alone does
//! NOT bound is CROSS-CLASS churn: a liar can advertise arbitrarily CHEAP specs (a low
//! [`entry_price`]) and, absent a comparable-entry proven competitor, under-price everyone on raw
//! advertisement while never delivering. Reputation pricing is the PRICE lever; closing that residual
//! needs two orthogonal levers it deliberately leaves to their owners: a capability/requirement
//! FILTER in routing (a node is only offered jobs its specs actually meet — collapsing cross-class
//! into same-class, where the defense holds) and a MINT STAKE (minting an identity must cost
//! something, so N shells cost N stakes, not N free probes). Named here so neither is silently
//! assumed already solved.

use crate::capacity::grid_budget::BudgetSource;
use crate::capacity::settlement::{Reputation, Settlement, Verdict};

/// Advertised hardware — the prospectus. [`entry_price`] is a pure, MONOTONE function of this (the
/// IPO valuation over comparable specs). A liar can inflate these numbers, but delivery is settled by
/// [`super::settlement`] and [`Reputation::trust_lower_bound`](super::settlement::Reputation::trust_lower_bound)
/// re-prices from the record — the spec is only the prior.
#[derive(Debug, Clone, Copy)]
pub struct Specs {
    pub vram_gb: u64,
    /// silicon-class × cores, collapsed to one comparable scalar (bytes/names aren't comparable).
    pub compute_score: u64,
    /// perf-per-watt, 1..=100 (higher = better). The efficiency axis the market pays a premium for.
    pub power_efficiency: u64,
}

/// The IPO valuation: objective, hardware-derived, needs no reputation. MONOTONE in every good axis —
/// that monotonicity is the invariant; the exact coefficients are a free knob.
pub fn entry_price(s: &Specs) -> u64 {
    (s.vram_gb * 100 + s.compute_score * 10) * s.power_efficiency.max(1) / 100
}

/// The RISK-ADJUSTED price the router minimizes: the entry ask ÷ the settled trust that it delivers.
/// Prices on settlement's [`Reputation::trust_lower_bound`](super::settlement::Reputation::trust_lower_bound)
/// — the Beta-prior discount — so an UNPROVEN seller is priced at entry ÷ ≈0.2 = ≈5× (bounded
/// exploration, never a free win) and a demonstrated failure at entry ÷ ≈0.04 = ≈25× (avoided).
/// `trust_lower_bound` is floored above 0 by the prior, so this never divides by zero.
pub fn expected_cost(s: &Specs, rep: &Reputation) -> f64 {
    entry_price(s) as f64 / rep.trust_lower_bound()
}

/// The final price a customer pays = the supply-side risk-adjusted ask × the DEMAND-side scarcity
/// multiplier `lambda`. λ is the grid-market-clearing price of the contended seam (BigMama's lane);
/// this crate proves the supply side with λ = 1.0. Kept as the single named seam so λ drops in without
/// touching the routing or the convergence invariants.
pub fn clearing_price(s: &Specs, rep: &Reputation, lambda: f64) -> f64 {
    expected_cost(s, rep) * lambda
}

/// The MINT STAKE as an "earn your way in" GRADUATION GATE — my #396 identity lane, the second and
/// independent defense against CROSS-class churn (the capability filter removes the hiding terrain; this
/// gate requires proven delivery to enter paid work). It is deliberately NOT a currency bond: the entry
/// cost is REAL delivered work, never burnt hashing — anti-Bitcoin by construction. A fresh identity may
/// take PROBE jobs to build a record, but may not bid on PAID work until it has honored `probe_quota`
/// deliveries. So minting N identities costs N × probe_quota real deliveries, and a churner who abandons
/// each identity after it fails never graduates any of them — the paid market never sees a shell. This
/// is exactly Joel's "minting identities buys a bounded number of probes," made a hard gate.
pub fn graduated(rep: &Reputation, probe_quota: u32) -> bool {
    rep.honored >= probe_quota
}

/// A market participant. `fails_every` is HIDDEN ground truth — what it ACTUALLY does, which a liar's
/// advertised specs contradict. The market only ever learns it through settled deliveries, folded into
/// `reputation` (settlement's type — the ONE reputation, not a second ledger).
#[derive(Debug, Clone)]
pub struct Participant {
    pub name: String,
    pub specs: Specs,
    /// Deterministic delivery truth: 0 = never fails; N ≥ 2 = fails 1 job in every N (first N-1
    /// succeed); 1 = fails EVERY job (a pure liar / dead shell that advertised fit).
    pub fails_every: u64,
    /// Standing, owned by [`super::settlement`] — priced on, never a parallel copy.
    pub reputation: Reputation,
    pub earned: u64,
    /// How many jobs have routed here (drives the deterministic flakiness pattern).
    assigned: u64,
}

impl Participant {
    pub fn new(name: impl Into<String>, specs: Specs, fails_every: u64) -> Self {
        Self {
            name: name.into(),
            specs,
            fails_every,
            reputation: Reputation::default(),
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

    /// Route one job here: it delivers per its hidden truth, the outcome SETTLES into its
    /// [`Reputation`](super::settlement::Reputation), and it is PAID its entry price only on an
    /// honored delivery. The settlement is what a fresh mint cannot fake — the whole defense.
    fn fulfill_one(&mut self) {
        let met = self.delivers_now();
        self.assigned += 1;
        let verdict = if met {
            Verdict::Honored
        } else {
            Verdict::Failed
        };
        self.reputation.record(&Settlement {
            seller: BudgetSource::Peer(self.name.clone()),
            verdict,
        });
        if met {
            self.earned += entry_price(&self.specs);
        }
    }
}

/// Play `jobs` identical asks through the market with λ = 1. Each routes to the lowest
/// [`expected_cost`]; the winner delivers per its hidden truth and settles. Deterministic:
/// on tied cost the first participant wins (`min_by` returns the first minimum).
pub fn run_market(participants: &mut [Participant], jobs: usize) {
    for _ in 0..jobs {
        let Some(w) = (0..participants.len()).min_by(|&a, &b| {
            expected_cost(&participants[a].specs, &participants[a].reputation)
                .partial_cmp(&expected_cost(
                    &participants[b].specs,
                    &participants[b].reputation,
                ))
                .expect("entry prices and trust bounds are finite")
        }) else {
            break;
        };
        participants[w].fulfill_one();
    }
}

/// Play `jobs` PAID asks: only GRADUATED participants (honored ≥ `probe_quota`) are eligible, and among
/// them the lowest [`expected_cost`] wins. Ungraduated identities — including every freshly-minted shell —
/// cannot bid on paid work at all, so cross-class churn cannot reach the paid market no matter how cheaply
/// it advertises. The probe deliveries that graduate an identity are its real cost of entry. Deterministic.
pub fn run_paid_market(participants: &mut [Participant], jobs: usize, probe_quota: u32) {
    for _ in 0..jobs {
        let Some(w) = (0..participants.len())
            .filter(|&i| graduated(&participants[i].reputation, probe_quota))
            .min_by(|&a, &b| {
                expected_cost(&participants[a].specs, &participants[a].reputation)
                    .partial_cmp(&expected_cost(
                        &participants[b].specs,
                        &participants[b].reputation,
                    ))
                    .expect("entry prices and trust bounds are finite")
            })
        else {
            break; // no graduated participant is eligible for paid work this tick
        };
        participants[w].fulfill_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs(vram_gb: u64, compute: u64, eff: u64) -> Specs {
        Specs {
            vram_gb,
            compute_score: compute,
            power_efficiency: eff,
        }
    }

    /// A pre-seeded settlement reputation: `honored` honored + `failed` failed settled transactions.
    /// The way a test says "this node is already PROVEN (or already spotty)" — the incumbency the
    /// sybil defense turns on.
    fn with_record(honored: u32, failed: u32) -> Reputation {
        let mut r = Reputation::default();
        for _ in 0..honored {
            r.record(&Settlement {
                seller: BudgetSource::Local,
                verdict: Verdict::Honored,
            });
        }
        for _ in 0..failed {
            r.record(&Settlement {
                seller: BudgetSource::Local,
                verdict: Verdict::Failed,
            });
        }
        r
    }

    // what this catches: THE IPO VALUATION IS MONOTONE. entry_price must rise with every good axis
    // (VRAM, compute, efficiency) — a better rig opens at a higher price with no track record. If a
    // bigger/faster/more-efficient node ever priced LOWER, the cold-start valuation is broken.
    #[test]
    fn entry_price_is_monotone_in_specs() {
        let small = specs(16, 100, 50);
        let big = specs(64, 400, 50);
        let efficient = specs(16, 100, 90);
        assert!(
            entry_price(&big) > entry_price(&small),
            "more VRAM+compute → higher entry"
        );
        assert!(
            entry_price(&efficient) > entry_price(&small),
            "better perf/watt → higher entry"
        );
    }

    // what this catches: A LIAR IS EXPOSED AND DELISTED. Same specs as the honest node (isolating the
    // REPUTATION axis, not the spec axis): the fresh liar wins the opening tie by being equally priced,
    // then its FIRST settled failure prices it out via trust_lower_bound (0.2 → 0.167 < the honest
    // node's fresh 0.2), and the honest node takes over. It is NEVER paid (only delivery earns), so a
    // fraudulent prospectus self-corrects in one job: earned=0, one win, then delisted.
    #[test]
    fn a_liar_wins_the_opening_tie_then_delists() {
        let s = specs(24, 150, 50);
        let mut m = vec![
            Participant::new("liar", s, 1), // index 0: wins the fresh-vs-fresh tie, ALWAYS fails
            Participant::new("honest", s, 0), // reliable
        ];
        run_market(&mut m, 20);
        let liar = &m[0];
        let honest = &m[1];
        assert_eq!(liar.earned, 0, "a node that never delivers is never paid");
        assert_eq!(
            liar.reputation.settled, 1,
            "it wins the opening tie ONCE, then its failure prices it out"
        );
        assert_eq!(
            liar.reputation.honored, 0,
            "and that one settled job was a failure"
        );
        assert!(honest.earned > 0, "the reliable node takes over and earns");
    }

    // what this catches: RELIABILITY BEATS FLAKINESS at equal price, when both are ESTABLISHED. Two
    // same-spec nodes with real records — a clean 5/5 vs a spotty 3-of-5. The reliable node's higher
    // verification rate makes its risk-adjusted price strictly lower, so it captures the market and
    // out-earns. (Under a verification-RATE reputation an UNPROVEN node cannot displace an established
    // one without exploration — the honest boundary — so the invariant is stated on equal footing:
    // established-reliable out-earns established-flaky. This is "a rock-solid node out-earns a flaky
    // one" isolated to the reliability axis.)
    #[test]
    fn an_established_reliable_node_out_earns_an_established_flaky_one() {
        let s = specs(24, 150, 50);
        let mut flaky = Participant::new("flaky", s, 3);
        flaky.reputation = with_record(3, 2); // spotty record: 3 of 5
        let mut reliable = Participant::new("reliable", s, 0);
        reliable.reputation = with_record(5, 0); // clean record: 5 of 5
        let mut m = vec![flaky, reliable]; // flaky listed FIRST — reliable must still win
        run_market(&mut m, 20);
        let flaky = &m[0];
        let reliable = &m[1];
        assert!(
            reliable.earned > flaky.earned,
            "the higher-verification-rate node out-earns the equal-priced flaky one (reliable={}, flaky={})",
            reliable.earned,
            flaky.earned
        );
    }

    // what this catches: A FIXED SET OF SYBIL SHELLS NEVER WINS against a PROVEN incumbent. Same specs
    // (isolating reputation): three fresh always-fail shells vs one honest node with a 10-of-10 record.
    // The incumbent's trust (≈0.73) prices it well under the shells' fresh 0.2, so it wins every job;
    // the shells never deliver, never earn, never accrue a reputation. Minting three identities bought
    // three zero-earners, not three reputations.
    #[test]
    fn fixed_sybil_shells_never_win_against_a_proven_incumbent() {
        let s = specs(24, 150, 50);
        let mut honest = Participant::new("honest", s, 0);
        honest.reputation = with_record(10, 0);
        let mut m = vec![
            Participant::new("shell-a", s, 1),
            Participant::new("shell-b", s, 1),
            Participant::new("shell-c", s, 1),
            honest,
        ];
        run_market(&mut m, 40);
        for shell in &m[0..3] {
            assert_eq!(
                shell.earned, 0,
                "a shell that never delivers never earns ({})",
                shell.name
            );
            assert_eq!(
                shell.reputation.settled, 0,
                "and it never even wins a job to settle ({})",
                shell.name
            );
        }
        assert!(
            m[3].earned > 0,
            "the proven incumbent captures the whole market"
        );
    }

    // ── The sybil-CHURN attack, and the real settlement defense (BigMama's lane, now WIRED) ──
    // A FIXED set of shells is exposed once and dies; the REAL attack is CHURN — identity creation is
    // cheap, so an attacker mints a FRESH unproven identity every job. Under the OLD "unknown = perfect"
    // pricing (reputation 1.0 for an empty ledger) every fresh identity was the cheapest bid and won,
    // and the grid-market sim measured it: 100/100 jobs absorbed, the honest incumbent fully starved.
    // The fix is settlement's trust_lower_bound: a fresh identity is priced at ≈0.2 (bounded
    // exploration), not 1.0. These two tests prove it against her REAL Reputation type.

    /// Route `jobs` one at a time; before each, the attacker mints a FRESH shell (new identity, empty
    /// reputation) via `mint`. The buyer switches to the newcomer only if it is STRICTLY cheaper —
    /// a tie keeps the standing identity (the mild, deliberate "don't churn on ties" rule). Returns how
    /// many jobs the fresh shells absorbed: UNBOUNDED when unknown is priced as perfect, ≈0 once it is
    /// priced as bounded exploration below a proven incumbent.
    fn shells_absorbed_under_churn(
        incumbent: &mut Participant,
        mint: impl Fn() -> Participant,
        jobs: usize,
    ) -> usize {
        let mut absorbed = 0;
        for _ in 0..jobs {
            let shell = mint();
            if expected_cost(&shell.specs, &shell.reputation)
                < expected_cost(&incumbent.specs, &incumbent.reputation)
            {
                absorbed += 1; // a fresh non-deliverer strictly under-priced the incumbent → job lost
            } else {
                incumbent.fulfill_one(); // incumbent wins → delivers + settles, compounding its record
            }
        }
        absorbed
    }

    // what this catches: SYBIL CHURN CANNOT STARVE A PROVEN INCUMBENT. Same specs as the incumbent
    // (isolating the reputation axis from the spec axis — the confound that made an earlier version of
    // this a false red). With the incumbent proven (10 honored → trust ≈0.73) and every shell fresh
    // (trust 0.2, ≈5× the risk-adjusted price at equal specs), the incumbent wins EVERY job — 0/100
    // absorbed, versus 100/100 under the old unknown-as-perfect pricing. This is the exact red spec
    // BigMama's trust_lower_bound turns green, now RUN against her real settlement::Reputation.
    #[test]
    fn sybil_churn_cannot_starve_a_proven_incumbent() {
        let s = specs(24, 150, 50);
        let mut incumbent = Participant::new("incumbent", s, 0);
        incumbent.reputation = with_record(10, 0);
        let absorbed = shells_absorbed_under_churn(
            &mut incumbent,
            || Participant::new("shell", s, 1), // fresh identity, same class, always fails
            100,
        );
        assert_eq!(
            absorbed, 0,
            "a proven incumbent is starved by churn only when unknown is priced as perfect; \
             trust_lower_bound prices a fresh mint at ≈5× the incumbent's risk-adjusted ask (absorbed={absorbed}/100)"
        );
    }

    // what this catches: A STAYER LOCKS OUT CHURN FROM JOB ONE — the defense doesn't even need a
    // pre-seeded incumbent. An honest node that merely STAYS (same class, starts fresh) wins the
    // opening tie under the ties-to-the-standing-identity rule, delivers, and its trust compounds
    // (0.2 → 0.33 → …) while every churned shell keeps re-paying the fresh 0.2. A churner who abandons
    // each identity never compounds, so it can never overtake a node that stays and delivers: honesty
    // is the dominant strategy at the identity level, not just the job level.
    #[test]
    fn a_stayer_locks_out_churn_from_job_one() {
        let s = specs(24, 150, 50);
        let mut stayer = Participant::new("stayer", s, 0); // starts fresh — no incumbency handed to it
        let absorbed =
            shells_absorbed_under_churn(&mut stayer, || Participant::new("shell", s, 1), 100);
        assert_eq!(
            absorbed, 0,
            "a node that stays and delivers wins the opening tie and compounds; churn never gets in (absorbed={absorbed}/100)"
        );
        assert_eq!(
            stayer.reputation.honored, 100,
            "the stayer delivered every job it kept"
        );
    }

    // ── The mint stake (my #396 lane): the graduation gate on the PAID market ──

    // what this catches: THE MINT STAKE KEEPS UNGRADUATED SHELLS OUT OF THE PAID MARKET — the second,
    // independent defense against CROSS-class churn (the one trust_lower_bound alone did NOT bound). A
    // cheap-spec cross-class liar attempts its probe jobs, FAILS them all (honored stays 0), so it never
    // graduates and is never eligible to bid on paid work — cheap specs cannot buy in. Minting a fresh
    // cheap shell per job changes nothing: every fresh identity starts ungraduated, so N shells are N
    // ineligibles, not N cheap winners. The stake = N × probe_quota real deliveries a churner never pays.
    #[test]
    fn the_mint_stake_keeps_ungraduated_shells_out_of_the_paid_market() {
        const PROBE_QUOTA: u32 = 3;
        let mut incumbent = Participant::new("incumbent", specs(24, 150, 50), 0);
        incumbent.reputation = with_record(PROBE_QUOTA, 0); // graduated by real deliveries
        let mut shell = Participant::new("cheap-liar", specs(8, 10, 10), 1); // cross-class cheap shell
        for _ in 0..PROBE_QUOTA {
            shell.fulfill_one(); // attempts its probes, fails all → honored stays 0
        }
        assert!(
            !graduated(&shell.reputation, PROBE_QUOTA),
            "a liar never graduates — it never honors a probe (honored={})",
            shell.reputation.honored
        );
        let mut m = vec![shell, incumbent];
        run_paid_market(&mut m, 100, PROBE_QUOTA);
        assert_eq!(
            m[0].earned, 0,
            "an ungraduated cheap shell is INELIGIBLE for paid work — cheap specs can't buy in"
        );
        assert!(
            m[1].earned > 0,
            "the graduated incumbent takes the paid market"
        );
    }

    // what this catches: THE GATE IS A DOORWAY, NOT A WALL — it admits a proven newcomer, never excludes
    // it. A fresh identity starts outside the paid market, but delivering its probe jobs graduates it, and
    // then it earns on paid work. This is the other half of "earn your way in": the stake bounds churn
    // WITHOUT barring newcomers, which is the failure mode of pricing unknown as worst.
    #[test]
    fn the_gate_admits_a_proven_newcomer_it_never_excludes() {
        const PROBE_QUOTA: u32 = 3;
        let mut newcomer = Participant::new("newcomer", specs(24, 150, 50), 0);
        assert!(
            !graduated(&newcomer.reputation, PROBE_QUOTA),
            "a fresh identity starts OUTSIDE the paid market"
        );
        newcomer.reputation = with_record(PROBE_QUOTA, 0); // it DELIVERED its probe jobs — earned its way in
        assert!(graduated(&newcomer.reputation, PROBE_QUOTA));
        let mut m = vec![newcomer];
        run_paid_market(&mut m, 10, PROBE_QUOTA);
        assert!(m[0].earned > 0, "once graduated by real delivery, the newcomer earns — the gate is a doorway, not a wall");
    }
}
