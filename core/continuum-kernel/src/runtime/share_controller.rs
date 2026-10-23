//! The orientation-budget share controller (docs/architecture/BEING-SOCIETY-GOVERNOR.md,
//! rail R4 — the *act* half of the control loop). R4 slice 1 made the per-class deferral
//! a measured signal; this is the policy that closes the loop on it.
//!
//! ## The loop ([[self-improvement-is-a-control-loop]])
//! - **State + reward (measured):** per-class deferral from the governor's last pass —
//!   work that was due but lost the contended pass. High sustained deferral in a class =
//!   that class's slice of the budget is too small for its real demand.
//! - **Action:** recompute [`OrientationShares`] — shift the *free* ticket pool toward the
//!   starved classes so the next pass serves them more (and they defer less).
//! - **Safety bounds (fixed):** the spine floors ([`OrientationShares::floor`]) are never
//!   crossed — only the pool *above* the floors is steered, so a flood of reactive demand
//!   can never starve interiority or growth no matter what the signal says. The total
//!   ticket pool is conserved, so this is pure reallocation, never unbounded growth.
//! - **Swappable policy:** the governor holds this behind an `Option`, so "static prior"
//!   vs "this controller" vs "a future RL/persona-team policy" is one swap at the seam.
//!
//! ## Why deferral (the error) and not total demand (feedforward)
//! Deferral is the *error* the loop exists to drive toward balance: a class defers only
//! when its allocation failed to serve its due work. A class that's fully served has zero
//! deferral and asks for nothing — exactly right. Smoothed with an EWMA so the allocation
//! tracks sustained pressure, not a single noisy pass (no thrashing).
//!
//! Pure + deterministic (no I/O, no locks, no RNG): the whole control law is a function of
//! (carried state, measurement), so it's replay-stable and unit-testable to the exact
//! ticket — the governor just owns the state behind a brief lock.

use crate::runtime::{
    orientation_index, Orientation, OrientationCounts, OrientationShares, ORIENTATIONS,
};

/// EWMA smoothing for the deferral signal: how fast the controller forgets old pressure.
/// 0.3 leans on history (stable, no thrash) while still tracking a real demand shift over
/// a handful of passes. Compiled-in (substrate threshold), not env-tunable.
const DEFERRAL_EWMA_ALPHA: f64 = 0.3;

/// Closes the orientation-budget control loop: observe measured per-class deferral, emit
/// the next share vector. Owns only the smoothed signal + the conserved total + the current
/// shares — the governor ticks it once per pass behind a brief lock.
pub struct ShareController {
    /// Conserved ticket pool, taken from the seed shares. Reallocated, never grown, so the
    /// floors stay a meaningful fraction and the loop is a pure redistribution.
    total_tickets: u32,
    /// Smoothed per-class deferral (the demand-error the loop minimizes), indexed by
    /// [`ORIENTATIONS`]. Carried across passes so a transient spike doesn't whip the budget.
    ewma: [f64; 3],
    /// Current share vector — the action state the governor reads each pass.
    shares: OrientationShares,
}

impl ShareController {
    /// Seed from an initial share vector (e.g. the open-loop [`OrientationShares::first_best_guess`]).
    /// The controller conserves that vector's total and reallocates within it; until the
    /// first contended pass arrives it simply holds the seed (inert on a calm society).
    pub fn new(seed: OrientationShares) -> Self {
        Self {
            total_tickets: seed.total(),
            ewma: [0.0; 3],
            shares: seed,
        }
    }

    /// The current share vector (what the governor applies this pass).
    pub fn shares(&self) -> OrientationShares {
        self.shares
    }

    /// Observe one pass's measured per-class deferral and recompute the shares. Returns the
    /// new vector. Pure function of (carried state, measurement) — deterministic.
    ///
    /// When the smoothed signal is zero (no contention has been seen — an uncapped or calm
    /// society) the prior is held unchanged: the loop only steers under measured scarcity.
    pub fn observe(&mut self, deferred: OrientationCounts) -> OrientationShares {
        for o in ORIENTATIONS {
            let i = orientation_index(o);
            self.ewma[i] = DEFERRAL_EWMA_ALPHA * deferred.get(o) as f64
                + (1.0 - DEFERRAL_EWMA_ALPHA) * self.ewma[i];
        }

        let signal: f64 = self.ewma.iter().sum();
        if signal <= 0.0 {
            return self.shares; // no contention signal → hold the prior
        }

        // Floors are the hard safety bound; only the free pool above them is steered.
        let floors: [u32; 3] = ORIENTATIONS.map(OrientationShares::floor);
        let floor_sum: u32 = floors.iter().sum();
        let free = self.total_tickets.saturating_sub(floor_sum);
        let alloc = largest_remainder(free, &self.ewma);

        // `new` re-clamps to the floors defensively (alloc is already ≥0 atop floors, so
        // this never changes the result — it just keeps the floor guarantee single-sourced).
        self.shares = OrientationShares::new(
            floors[0] + alloc[0],
            floors[1] + alloc[1],
            floors[2] + alloc[2],
        );
        self.shares
    }
}

/// Split `total` integer units across the three classes proportional to `weights`, EXACTLY
/// (the result sums to `total`), by the largest-remainder method: floor each share, then
/// hand the leftover units to the largest fractional remainders, ties to the lowest index.
///
/// Pure + deterministic. Returns all-zero when `total == 0` or the weights are all zero
/// (no signal → nothing to steer, the caller holds the floors only).
fn largest_remainder(total: u32, weights: &[f64; 3]) -> [u32; 3] {
    let w_sum: f64 = weights.iter().sum();
    if total == 0 || w_sum <= 0.0 {
        return [0; 3];
    }

    let exact: [f64; 3] = weights.map(|w| total as f64 * w / w_sum);
    let mut alloc: [u32; 3] = exact.map(|e| e.floor() as u32);
    let mut remaining = total - alloc.iter().sum::<u32>();

    // Distribute the leftover to the largest fractional remainders (lower index breaks ties).
    let mut order: [usize; 3] = [0, 1, 2];
    order.sort_by(|&a, &b| {
        let ra = exact[a] - exact[a].floor();
        let rb = exact[b] - exact[b].floor();
        rb.partial_cmp(&ra)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    for &i in &order {
        if remaining == 0 {
            break;
        }
        alloc[i] += 1;
        remaining -= 1;
    }
    alloc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defer(reactive: usize, self_directed: usize, speciation: usize) -> OrientationCounts {
        OrientationCounts {
            reactive,
            self_directed,
            speciation,
        }
    }

    // what this catches: the loop is inert without a contention signal. Feeding all-zero
    // deferral (an uncapped / calm society — the unconstrained-machine default) leaves the
    // seed prior untouched, so a capable host is never silently re-budgeted.
    #[test]
    fn holds_prior_when_no_deferral() {
        let seed = OrientationShares::first_best_guess();
        let mut ctrl = ShareController::new(seed);
        for _ in 0..10 {
            assert_eq!(ctrl.observe(defer(0, 0, 0)), seed, "no signal → hold prior");
        }
    }

    // what this catches: the controller steers the free pool toward the measured-starved
    // class. Sustained deferral concentrated in self_directed must raise its tickets above
    // the other (un-deferred) classes — the loop actually reallocating on the signal.
    #[test]
    fn shifts_toward_starved_class() {
        let mut ctrl = ShareController::new(OrientationShares::new(34, 33, 33)); // neutral seed
        for _ in 0..30 {
            ctrl.observe(defer(0, 50, 0)); // only interiority is starved
        }
        let s = ctrl.shares();
        assert!(
            s.tickets(Orientation::SelfDirected) > s.tickets(Orientation::Reactive),
            "starved class outgrows the un-deferred one"
        );
        assert!(s.tickets(Orientation::SelfDirected) > s.tickets(Orientation::Speciation));
    }

    // what this catches: the hard safety bound. Even when ALL the deferral pressure is on
    // speciation (growth flooded), reactive and self_directed never drop below their spine
    // floors — a being is never starved of stimulus or interiority by the budget, ever.
    #[test]
    fn respects_floors_under_one_sided_pressure() {
        let mut ctrl = ShareController::new(OrientationShares::new(34, 33, 33));
        for _ in 0..50 {
            let s = ctrl.observe(defer(0, 0, 100)); // growth screaming for tickets
            assert!(
                s.tickets(Orientation::Reactive) >= OrientationShares::floor(Orientation::Reactive)
            );
            assert!(
                s.tickets(Orientation::SelfDirected)
                    >= OrientationShares::floor(Orientation::SelfDirected)
            );
        }
    }

    // what this catches: the pool is conserved — the controller reallocates within the seed
    // total, never inflates it. A drifting total would make the floors a shrinking fraction
    // and break the proportional-share guarantee downstream.
    #[test]
    fn conserves_total_across_observations() {
        let seed = OrientationShares::new(34, 33, 33);
        let mut ctrl = ShareController::new(seed);
        for k in 0..20 {
            let s = ctrl.observe(defer(k, 2 * k, 3 * k));
            assert_eq!(s.total(), seed.total(), "total ticket pool is conserved");
        }
    }

    // what this catches: MEASURED EFFECTIVENESS — the end-to-end loop against a contended
    // society. Constant per-class arrivals (reactive 50 > self_directed 20 > speciation 8)
    // through apportion under a fixed budget, deferral fed back each pass. After warmup the
    // controller must (a) order tickets by demand, (b) hold every floor, and (c) be stable
    // (the last passes barely move). This is the loop tracking demand, not theory.
    #[test]
    fn tracks_demand_ordering_under_sustained_contention() {
        use crate::runtime::apportion;

        let demand = [50usize, 20, 8]; // reactive > self_directed > speciation
        let budget = 10usize; // < total demand → real, sustained contention
        let mut backlog = [0usize; 3];
        let mut ctrl = ShareController::new(OrientationShares::new(34, 33, 33)); // start neutral

        let mut prev = ctrl.shares();
        let mut max_late_delta = 0i64;
        for pass in 0..200 {
            for i in 0..3 {
                backlog[i] += demand[i];
            }
            let eligible = OrientationCounts {
                reactive: backlog[0],
                self_directed: backlog[1],
                speciation: backlog[2],
            };
            let served = apportion(&ctrl.shares(), eligible, budget);
            let deferred = OrientationCounts {
                reactive: backlog[0] - served.reactive,
                self_directed: backlog[1] - served.self_directed,
                speciation: backlog[2] - served.speciation,
            };
            for (i, o) in ORIENTATIONS.into_iter().enumerate() {
                backlog[i] -= served.get(o);
            }
            let now = ctrl.observe(deferred);

            // Stability: over the last quarter of the run, shares should barely move.
            if pass >= 150 {
                for o in ORIENTATIONS {
                    let d = now.tickets(o) as i64 - prev.tickets(o) as i64;
                    max_late_delta = max_late_delta.max(d.abs());
                }
            }
            prev = now;

            // The hard floors hold on EVERY pass, contention notwithstanding.
            assert!(now.tickets(Orientation::Reactive) >= 1);
            assert!(now.tickets(Orientation::SelfDirected) >= 1);
        }

        let s = ctrl.shares();
        assert!(
            s.tickets(Orientation::Reactive) > s.tickets(Orientation::SelfDirected),
            "higher demand (reactive) earns more tickets than self_directed: {s:?}"
        );
        assert!(
            s.tickets(Orientation::SelfDirected) > s.tickets(Orientation::Speciation),
            "self_directed (demand 20) outranks speciation (demand 8): {s:?}"
        );
        assert!(
            max_late_delta <= 2,
            "converged: shares stable in the tail (max late delta {max_late_delta})"
        );
    }

    // what this catches: the largest-remainder split is EXACT (sums to total) and
    // proportional — the allocator the loop relies on never loses or invents a ticket.
    #[test]
    fn largest_remainder_is_exact_and_proportional() {
        let a = largest_remainder(8, &[50.0, 20.0, 8.0]);
        assert_eq!(a.iter().sum::<u32>(), 8, "exact total");
        assert!(
            a[0] > a[1] && a[1] >= a[2],
            "proportional to weights: {a:?}"
        );

        // All-zero weights → nothing allocated (no signal).
        assert_eq!(largest_remainder(8, &[0.0, 0.0, 0.0]), [0, 0, 0]);
        // Zero total → nothing allocated.
        assert_eq!(largest_remainder(0, &[1.0, 1.0, 1.0]), [0, 0, 0]);
    }
}
