//! The flexbox: ONE algorithm that shares an amount among claims by priority, floor, min and max, over
//! any [`Quantity`] (ONE-BUDGET.md contract 3; UNIFIED-PAGING.md, "Room by priority"). A mind's prompt
//! shares its tokens with it and a place shares its bytes with it: the same passes, the same answers.
//! Pure: no I/O, no allocation but the caller's buffers, which it reuses.
//!
//! No clipping: when the amount is tight an optional claim is dropped whole and a required one is marked
//! [`Landed::UnderProvisioned`], for its caller to escalate.
//!
//! 1. **Floors**, required claims first, then optional ones, each in priority order.
//! 2. **Mins**, topped up in priority order, never past a claim's max.
//! 3. **Grow**: what remains is shared by priority weight, never past a claim's max, until nothing moves.

use std::cmp::Ordering;

use super::quantity::Quantity;

/// One claim: its priority (higher is more important: the grow weight and the order of the floor and min
/// passes), its floor (honored first or it escalates or is dropped), its min (topped up to before anything
/// grows) and its max (never more), and whether it is required.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flex<Q> {
    pub priority: u8,
    pub floor: Q,
    pub min: Q,
    pub max: Q,
    pub required: bool,
}

/// Where a claim landed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Landed {
    /// Got at least its min.
    Satisfied,
    /// Got at least its floor, less than its min.
    FloorOnly,
    /// An optional claim that got nothing.
    Dropped,
    /// A required claim that got less than its floor: its caller escalates.
    UnderProvisioned,
}

/// One claim's share, in the order the claims were given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Share<Q> {
    pub amount: Q,
    pub landed: Landed,
}

/// What a sharing left over, and whether a required claim fell short of its floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shared<Q> {
    pub unallocated: Q,
    pub escalation_needed: bool,
}

/// The passes' working order, kept by the caller between sharings so a sharing allocates nothing past its
/// high-water mark.
#[derive(Debug, Default)]
pub struct Flexbox {
    order: Vec<usize>,
    active: Vec<usize>,
    held: Vec<u64>,
}

impl Flexbox {
    /// Share `available` among `claims` into `shares` (cleared first; one per claim, in input order). Claims
    /// of equal priority are ordered by `tie` (their input indices), so the answer never depends on how a
    /// caller happened to order them.
    pub fn share<Q: Quantity>(
        &mut self,
        available: Q,
        claims: &[Flex<Q>],
        tie: impl Fn(usize, usize) -> Ordering,
        shares: &mut Vec<Share<Q>>,
    ) -> Shared<Q> {
        let order = &mut self.order;
        order.clear();
        order.extend(0..claims.len());
        order.sort_by(|&a, &b| claims[b].priority.cmp(&claims[a].priority).then_with(|| tie(a, b)));
        shares.clear();
        shares.extend(claims.iter().map(|_| Share { amount: Q::of(0), landed: Landed::Dropped }));
        let held = &mut self.held; // indexed by input
        held.clear();
        held.resize(claims.len(), 0);
        let mut remaining = available.units();
        let mut escalation_needed = false;

        // floors: required, then optional
        for required in [true, false] {
            for &i in order.iter().filter(|&&i| claims[i].required == required) {
                let floor = claims[i].floor.units();
                if floor <= remaining {
                    held[i] = floor;
                    remaining -= floor;
                    shares[i].landed = Landed::FloorOnly;
                } else if required {
                    held[i] = remaining;
                    remaining = 0;
                    shares[i].landed = Landed::UnderProvisioned;
                    escalation_needed = true;
                }
            }
        }
        let growing = |landed: Landed| matches!(landed, Landed::FloorOnly | Landed::Satisfied);

        // mins, in priority order
        for &i in order.iter() {
            if !growing(shares[i].landed) {
                continue;
            }
            let (min, max) = (claims[i].min.units(), claims[i].max.units());
            let granted = min.saturating_sub(held[i]).min(remaining).min(max.saturating_sub(held[i]));
            held[i] += granted;
            remaining -= granted;
            if held[i] >= min {
                shares[i].landed = Landed::Satisfied;
            }
        }

        // grow by priority weight, capped at max
        let active = &mut self.active;
        loop {
            active.clear();
            active.extend(order.iter().copied().filter(|&i| growing(shares[i].landed) && held[i] < claims[i].max.units()));
            if active.is_empty() || remaining == 0 {
                break;
            }
            let weights: u64 = active.iter().map(|&i| u64::from(claims[i].priority)).sum();
            if weights == 0 {
                break;
            }
            let mut moved = 0u64;
            for &i in active.iter() {
                let part = u128::from(remaining) * u128::from(claims[i].priority) / u128::from(weights);
                let grant = u64::try_from(part).unwrap_or(remaining).min(claims[i].max.units() - held[i]); // unwrap_or: a share is at most `remaining` (priority over the weights it is part of), the bound it already keeps
                held[i] += grant;
                moved += grant;
            }
            if moved == 0 {
                // fewer units left than weights: the first claim in priority order takes the rest it can hold
                let i = active[0];
                let grant = remaining.min(claims[i].max.units() - held[i]);
                held[i] += grant;
                moved = grant;
            }
            remaining = remaining.saturating_sub(moved);
        }

        for (share, &amount) in shares.iter_mut().zip(held.iter()) {
            share.amount = Q::of(amount);
        }
        Shared { unallocated: Q::of(remaining), escalation_needed }
    }
}
