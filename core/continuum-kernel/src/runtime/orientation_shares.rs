//! The orientation budget POLICY (docs/architecture/BEING-SOCIETY-GOVERNOR.md, rail R3):
//! a proportional share of the society's time across the three [`Orientation`] classes,
//! and the deterministic stride scheduler that turns that share into a per-pass split.
//!
//! Layering on the prior rails:
//! - R1 ([`CadenceTable`](crate::runtime::CadenceTable)) decides *when* a `(region,
//!   persona)` pair is due — the within-class causal arbitration.
//! - R2 ([`Orientation`]) tags each region with the budget class its time draws from.
//! - **R3 (here)** decides, when the pass can't afford every due pair, *which classes*
//!   get the scarce slices — proportional to [`OrientationShares`], floors guaranteed.
//!
//! Two principles the types enforce structurally:
//! - **Spine-fixed floors.** `reactive` and `self_directed` tickets are forced ≥ 1 at
//!   construction, so stride ALWAYS comes back around to them: a being is never deaf to
//!   stimulus and its inner life never fully starves (sleep ≠ coma at the budget level —
//!   deprivation degrades a mind, it doesn't pause it). `speciation` MAY be 0 on a
//!   constrained node — economics-elastic, but a declared 0, never a silent drop.
//! - **Deterministic, no RNG.** Stride scheduling (Waldspurger) gives proportional
//!   selection without `Math.random`, so a scheduling pass is reproducible under replay
//!   and unit-testable to the exact slice. The policy (the share vector) is the only
//!   thing the R4 daemon will tune; the mechanism here stays fixed.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::runtime::Orientation;

/// The three orientation classes in canonical order — the axis every count / share /
/// apportionment iterates. Single source of the ordering so the index math can't drift.
pub const ORIENTATIONS: [Orientation; 3] = [
    Orientation::Reactive,
    Orientation::SelfDirected,
    Orientation::Speciation,
];

/// Canonical index of an orientation into the `[_; 3]` arrays keyed by [`ORIENTATIONS`].
/// The single source of the ordering — the governor groups eligible pairs by this so the
/// index math can't drift between here and the scheduler.
pub fn orientation_index(o: Orientation) -> usize {
    match o {
        Orientation::Reactive => 0,
        Orientation::SelfDirected => 1,
        Orientation::Speciation => 2,
    }
}

/// A count per orientation class. ONE type, three uses (compression principle): ticks
/// tallied last pass, eligible pairs counted this pass, and budget apportioned across
/// classes. Carried in the governor snapshot, so it derives the wire traits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OrientationCounts {
    /// Outward-facing work: perception, recall, responding.
    #[ts(type = "number")]
    pub reactive: usize,
    /// The being's own interiority: curiosity, projects, dream/consolidation.
    #[ts(type = "number")]
    pub self_directed: usize,
    /// Growing the self: speciation / LoRA-genome learning.
    #[ts(type = "number")]
    pub speciation: usize,
}

impl OrientationCounts {
    /// Count one item against its orientation class.
    pub fn record(&mut self, o: Orientation) {
        match o {
            Orientation::Reactive => self.reactive += 1,
            Orientation::SelfDirected => self.self_directed += 1,
            Orientation::Speciation => self.speciation += 1,
        }
    }

    /// The count for one class.
    pub fn get(&self, o: Orientation) -> usize {
        match o {
            Orientation::Reactive => self.reactive,
            Orientation::SelfDirected => self.self_directed,
            Orientation::Speciation => self.speciation,
        }
    }

    /// Total across all classes.
    pub fn total(&self) -> usize {
        self.reactive + self.self_directed + self.speciation
    }
}

/// Spine-fixed minimum tickets: a class with a positive floor is ALWAYS reached by
/// stride over time. `speciation` has no floor — it may be 0 (economics-elastic).
const MIN_REACTIVE_TICKETS: u32 = 1;
const MIN_SELF_DIRECTED_TICKETS: u32 = 1;

/// The orientation budget policy: relative tickets per class. Reactive + self-directed
/// floors are enforced at construction (the spine fixes them; the R4 daemon tunes the
/// rest within those bounds). Carried in the snapshot as telemetry of the active policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OrientationShares {
    #[ts(type = "number")]
    reactive: u32,
    #[ts(type = "number")]
    self_directed: u32,
    #[ts(type = "number")]
    speciation: u32,
}

impl OrientationShares {
    /// Build a share vector, enforcing the spine floors. `reactive` and `self_directed`
    /// are clamped up to their minimums; `speciation` is taken as-is (may be 0).
    pub fn new(reactive: u32, self_directed: u32, speciation: u32) -> Self {
        Self {
            reactive: reactive.max(MIN_REACTIVE_TICKETS),
            self_directed: self_directed.max(MIN_SELF_DIRECTED_TICKETS),
            speciation,
        }
    }

    /// The open-loop prior (BEING-SOCIETY-GOVERNOR.md): stimulus-led, a real reserved
    /// slice of interiority, a modest growth share. Declared as a guess, corrected by
    /// the R4 share-policy daemon — never a frozen magic constant.
    pub fn first_best_guess() -> Self {
        Self::new(7, 2, 1)
    }

    /// Tickets for one class.
    pub fn tickets(&self, o: Orientation) -> u32 {
        match o {
            Orientation::Reactive => self.reactive,
            Orientation::SelfDirected => self.self_directed,
            Orientation::Speciation => self.speciation,
        }
    }

    /// Total tickets across all classes. The conserved pool a controller redistributes —
    /// reallocate within this, never grow it, so the floors stay meaningful.
    pub fn total(&self) -> u32 {
        self.reactive + self.self_directed + self.speciation
    }

    /// The spine-fixed floor for a class — the hard lower bound a controller must never
    /// steer below. Single source of the floor truth (the [`new`](Self::new) clamp reads
    /// the same constants), so a tuning policy can allocate only the *free* pool above it.
    pub fn floor(o: Orientation) -> u32 {
        match o {
            Orientation::Reactive => MIN_REACTIVE_TICKETS,
            Orientation::SelfDirected => MIN_SELF_DIRECTED_TICKETS,
            Orientation::Speciation => 0,
        }
    }
}

impl Default for OrientationShares {
    fn default() -> Self {
        Self::first_best_guess()
    }
}

/// Big numerator for stride math — `stride = STRIDE1 / tickets`. Large enough that
/// integer division noise between realistic ticket counts is negligible.
const STRIDE1: u64 = 1 << 20;

/// Split `budget` slices across the orientation classes proportional to `shares`,
/// capped per class by `eligible` (a class can't be handed more slices than it has due
/// work), with any leftover flowing to classes that still have capacity.
///
/// Deterministic stride scheduling: each class advances a `pass` cursor by its stride
/// (inverse of its tickets) every time it's picked; the smallest cursor wins the next
/// slice. Over a budget this reproduces the ticket ratios, and because it's pure +
/// RNG-free the exact split is testable and replay-stable.
///
/// Fast path: if `budget` covers every eligible pair, everything is admitted (no
/// contention → the orientation budget is dormant; this is the unconstrained-machine
/// case).
pub fn apportion(
    shares: &OrientationShares,
    eligible: OrientationCounts,
    budget: usize,
) -> OrientationCounts {
    if budget >= eligible.total() {
        return eligible;
    }

    let tickets: [u64; 3] = ORIENTATIONS.map(|o| shares.tickets(o) as u64);
    let cap: [usize; 3] = ORIENTATIONS.map(|o| eligible.get(o));
    // A 0-ticket class is never selected; give it an unreachable stride sentinel.
    let stride: [u64; 3] = tickets.map(|t| if t == 0 { u64::MAX } else { STRIDE1 / t });
    // Init cursors to one stride (standard stride-scheduler warm start).
    let mut cursor = stride;
    let mut alloc = [0usize; 3];
    let mut given = 0usize;

    while given < budget {
        // Pick the eligible-with-capacity, nonzero-ticket class with the smallest
        // cursor; ties break to the lowest canonical index (determinism).
        let mut best: Option<usize> = None;
        for i in 0..3 {
            if tickets[i] == 0 || alloc[i] >= cap[i] {
                continue;
            }
            match best {
                None => best = Some(i),
                Some(b) if cursor[i] < cursor[b] => best = Some(i),
                _ => {}
            }
        }
        let Some(i) = best else { break }; // no class has remaining capacity
        alloc[i] += 1;
        given += 1;
        cursor[i] = cursor[i].saturating_add(stride[i]);
    }

    OrientationCounts {
        reactive: alloc[0],
        self_directed: alloc[1],
        speciation: alloc[2],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the spine floors. A share vector built with zero reactive /
    // self_directed tickets is clamped up to the minimum, so those classes can never be
    // starved out of the schedule — a being is never deaf, its inner life never fully
    // stops. speciation is left at 0 (economics-elastic).
    #[test]
    fn new_enforces_reactive_and_self_directed_floors() {
        let s = OrientationShares::new(0, 0, 0);
        assert_eq!(s.tickets(Orientation::Reactive), 1);
        assert_eq!(s.tickets(Orientation::SelfDirected), 1);
        assert_eq!(s.tickets(Orientation::Speciation), 0, "speciation may be 0");
    }

    // what this catches: OrientationCounts is a faithful per-class counter — record maps
    // each class to its own bucket and total() sums them (the snapshot invariant
    // ticked == by_orientation.total()).
    #[test]
    fn counts_record_and_total() {
        let mut c = OrientationCounts::default();
        c.record(Orientation::Reactive);
        c.record(Orientation::Reactive);
        c.record(Orientation::SelfDirected);
        c.record(Orientation::Speciation);
        assert_eq!(c.get(Orientation::Reactive), 2);
        assert_eq!(c.get(Orientation::SelfDirected), 1);
        assert_eq!(c.get(Orientation::Speciation), 1);
        assert_eq!(c.total(), 4);
    }

    // what this catches: no contention → no budgeting. When the budget covers every
    // eligible pair, apportion admits all of them unchanged (the unconstrained-machine
    // path where the orientation budget stays dormant).
    #[test]
    fn apportion_admits_all_when_budget_covers_eligible() {
        let s = OrientationShares::first_best_guess();
        let eligible = OrientationCounts {
            reactive: 3,
            self_directed: 2,
            speciation: 1,
        };
        assert_eq!(apportion(&s, eligible, 100), eligible);
        assert_eq!(
            apportion(&s, eligible, 6),
            eligible,
            "budget == total still admits all"
        );
    }

    // what this catches: proportional split under scarcity. Equal tickets + ample
    // eligible work + a budget of 6 splits 2/2/2; the total never exceeds the budget.
    #[test]
    fn apportion_splits_proportionally_equal_tickets() {
        let s = OrientationShares::new(1, 1, 1);
        let eligible = OrientationCounts {
            reactive: 10,
            self_directed: 10,
            speciation: 10,
        };
        let got = apportion(&s, eligible, 6);
        assert_eq!(got.total(), 6);
        assert_eq!(
            got,
            OrientationCounts {
                reactive: 2,
                self_directed: 2,
                speciation: 2
            }
        );
    }

    // what this catches: ticket ratios drive the split. 7:2:1 over a budget of 10 with
    // ample work gives the largest share to reactive, and the total honors the budget.
    #[test]
    fn apportion_follows_ticket_ratios() {
        let s = OrientationShares::first_best_guess(); // 7,2,1
        let eligible = OrientationCounts {
            reactive: 100,
            self_directed: 100,
            speciation: 100,
        };
        let got = apportion(&s, eligible, 10);
        assert_eq!(got.total(), 10);
        assert!(
            got.reactive > got.self_directed,
            "reactive (7) outweighs self_directed (2)"
        );
        assert!(
            got.self_directed >= got.speciation,
            "self_directed (2) ≥ speciation (1)"
        );
    }

    // what this catches: per-class capacity caps + leftover redistribution. Reactive has
    // huge tickets but only 1 eligible pair, so it's capped at 1 and its unused budget
    // flows to the classes that still have work — no slice is wasted on an empty class.
    #[test]
    fn apportion_caps_at_eligible_and_redistributes() {
        let s = OrientationShares::new(7, 2, 1);
        let eligible = OrientationCounts {
            reactive: 1,
            self_directed: 10,
            speciation: 10,
        };
        let got = apportion(&s, eligible, 6);
        assert_eq!(got.reactive, 1, "capped at its single eligible pair");
        assert_eq!(got.total(), 6, "leftover redistributed, full budget used");
    }

    // what this catches: a 0-ticket class is NEVER selected even when it has eligible
    // work — speciation off (constrained node) means its due pairs defer, declared not
    // silently mixed in.
    #[test]
    fn apportion_never_selects_zero_ticket_class() {
        let s = OrientationShares::new(1, 1, 0); // speciation off
        let eligible = OrientationCounts {
            reactive: 5,
            self_directed: 5,
            speciation: 5,
        };
        let got = apportion(&s, eligible, 4);
        assert_eq!(got.speciation, 0, "0 tickets → never scheduled");
        assert_eq!(got.reactive + got.self_directed, 4);
    }

    // what this catches: determinism — same inputs yield the same split every call, so a
    // scheduling pass is reproducible under replay.
    #[test]
    fn apportion_is_deterministic() {
        let s = OrientationShares::first_best_guess();
        let eligible = OrientationCounts {
            reactive: 20,
            self_directed: 20,
            speciation: 20,
        };
        let a = apportion(&s, eligible, 13);
        let b = apportion(&s, eligible, 13);
        assert_eq!(a, b);
    }
}
