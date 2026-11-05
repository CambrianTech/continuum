//! The flexbox: ONE algorithm that shares a budget among claims by floor, min, max and priority
//! (moved here from `continuum-grounding::rag_budget`, MIND-CONTEXT's module table: "flex moves
//! into continuum-context as the one allocator"). Pure: no I/O, no adapter instance.
//!
//! Its one production caller is the act's allocation ([`crate::allocation::allocate`]), which
//! runs it ONCE per act over everything a mind's prompt holds (ONE-ACT stage 2).
//!
//! Doctrine, unchanged from its first home: no clipping. When the budget is tight a claim is
//! dropped WHOLE (an optional one) or marked `UnderProvisioned` (a required one) and the caller
//! escalates; the allocator never knows what a complete unit looks like, only token counts.

use serde::{Deserialize, Serialize};

/// One claim on a budget. Sent INTO the allocator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlexClaim {
    /// Stable identifier of the claimant.
    pub source_id: String,
    /// Priority weight 1-10, higher = more important: the flex-grow share when distributing
    /// free tokens, and the order of the floor and min passes.
    pub priority: u8,
    /// UNCONDITIONAL minimum: honored first, or the claim escalates to `UnderProvisioned`
    /// (required) or is `Dropped` (optional).
    pub floor_tokens: u32,
    /// Flex-basis: topped up to in priority order before anything grows.
    pub min_tokens: u32,
    /// Flex-cap: never more than this, whatever is free.
    pub max_tokens: u32,
    /// A required claim that cannot get its floor escalates; an optional one is dropped whole.
    pub required: bool,
}

/// Per-claim outcome, reported back from the allocator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceAllocation {
    pub source_id: String,
    pub allocated_tokens: u32,
    pub requested_floor: u32,
    pub requested_min: u32,
    pub requested_max: u32,
    /// Whether the claim was required: its floor is never shared away by any consumer of the plan.
    pub required: bool,
    pub state: AllocationState,
}

/// Where a claim landed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllocationState {
    /// Got at least its min.
    Satisfied,
    /// Got at least its floor, less than its min.
    FloorOnly,
    /// An optional claim that got nothing: its content does not enter this turn.
    Dropped,
    /// A required claim that got less than its floor: the caller MUST escalate.
    UnderProvisioned,
}

/// What [`flex_allocate`] decided: every claim's grant in INPUT order, the tokens no claim
/// could take, and the escalation/warning receipts.
#[derive(Debug, Clone, PartialEq)]
pub struct FlexAllocation {
    pub allocations: Vec<SourceAllocation>,
    pub unallocated: u32,
    /// A required claim got less than its floor.
    pub escalation_needed: bool,
    pub warnings: Vec<String>,
}

impl FlexAllocation {
    /// The grant of the claim named `source_id`; 0 when no such claim was made.
    pub fn granted(&self, source_id: &str) -> u32 {
        self.allocations
            .iter()
            .find(|a| a.source_id == source_id)
            .map_or(0, |a| a.allocated_tokens) // map_or: no such claim was granted nothing
    }

    /// The outcome of the claim named `source_id`, when one was made.
    pub fn of(&self, source_id: &str) -> Option<&SourceAllocation> {
        self.allocations.iter().find(|a| a.source_id == source_id)
    }
}

/// What one turn needs of a lane: the sum of its claims' FLOORS, never what the budget granted.
/// The flexbox grows a turn into whatever window it is given, so a grant measured on a 102k
/// lane reads as a 48.6k "need" and refuses a second lane at 46k (the M5, 2026-10-18).
pub fn turn_need(claims: &[FlexClaim]) -> u32 {
    claims.iter().map(|c| c.floor_tokens).fold(0, u32::saturating_add)
}

/// THE flexbox algorithm, pure: `available` tokens shared across `claims`.
///
/// 1. **Floor pass**: required floors first (priority order), then optional floors. A required
///    claim that cannot get its floor takes what remains and is `UnderProvisioned`
///    (escalation); an optional one is `Dropped`.
/// 2. **Min pass**: top up to `min_tokens` in priority order, capped at `max_tokens`.
/// 3. **Grow pass**: distribute what remains by priority weight, capped at `max_tokens`,
///    iterating until nothing moves.
pub fn flex_allocate(available: u32, claims: &[FlexClaim]) -> FlexAllocation {
    let mut warnings = Vec::new();
    // Stable sort by priority desc, then by source_id for deterministic tie-break: the output
    // must not depend on slice ordering. `order[k]` is the input index of the k-th claim in
    // priority order.
    let mut order: Vec<usize> = (0..claims.len()).collect();
    order.sort_by(|&a, &b| {
        claims[b]
            .priority
            .cmp(&claims[a].priority)
            .then(claims[a].source_id.cmp(&claims[b].source_id))
    });
    let sorted: Vec<&FlexClaim> = order.iter().map(|&i| &claims[i]).collect();

    let mut alloc: Vec<u32> = vec![0; sorted.len()];
    let mut state: Vec<AllocationState> = vec![AllocationState::Dropped; sorted.len()];
    let mut remaining: u32 = available;
    let mut escalation_needed = false;

    // ---- Pass 1: floors (unconditional) ----
    let required_floor_sum: u32 = sorted.iter().filter(|s| s.required).map(|s| s.floor_tokens).fold(0, u32::saturating_add);
    if required_floor_sum > available {
        warnings.push(format!(
            "required floor sum ({required_floor_sum}) exceeds available ({available}); some required sources UnderProvisioned"
        ));
    }
    for (i, source) in sorted.iter().enumerate() {
        if !source.required {
            continue;
        }
        if source.floor_tokens <= remaining {
            alloc[i] = source.floor_tokens;
            remaining -= source.floor_tokens;
            state[i] = AllocationState::FloorOnly;
        } else {
            alloc[i] = remaining;
            remaining = 0;
            state[i] = AllocationState::UnderProvisioned;
            escalation_needed = true;
        }
    }
    for (i, source) in sorted.iter().enumerate() {
        if source.required {
            continue;
        }
        if source.floor_tokens == 0 {
            // A zero floor is trivially satisfied: eligible for min and grow.
            state[i] = AllocationState::FloorOnly;
            continue;
        }
        if source.floor_tokens <= remaining {
            alloc[i] = source.floor_tokens;
            remaining -= source.floor_tokens;
            state[i] = AllocationState::FloorOnly;
        } else {
            warnings.push(format!(
                "optional source `{}` dropped — floor {} > remaining {}",
                source.source_id, source.floor_tokens, remaining
            ));
        }
    }

    // ---- Pass 2: min, in priority order ----
    for (i, source) in sorted.iter().enumerate() {
        if matches!(state[i], AllocationState::Dropped | AllocationState::UnderProvisioned) {
            continue;
        }
        let needed = source.min_tokens.saturating_sub(alloc[i]);
        let granted = needed.min(remaining).min(source.max_tokens.saturating_sub(alloc[i]));
        alloc[i] += granted;
        remaining -= granted;
        if alloc[i] >= source.min_tokens {
            state[i] = AllocationState::Satisfied;
        }
    }

    // ---- Pass 3: grow by priority weight, capped at max ----
    loop {
        let active: Vec<usize> = sorted
            .iter()
            .enumerate()
            .filter(|(i, s)| {
                !matches!(state[*i], AllocationState::Dropped | AllocationState::UnderProvisioned) && alloc[*i] < s.max_tokens
            })
            .map(|(i, _)| i)
            .collect();
        if active.is_empty() || remaining == 0 {
            break;
        }
        let priority_sum: u32 = active.iter().map(|&i| sorted[i].priority as u32).sum();
        if priority_sum == 0 {
            break;
        }
        let mut moved = 0u32;
        for &i in &active {
            let share = ((remaining as u64) * (sorted[i].priority as u64) / (priority_sum as u64)) as u32;
            let grant = share.min(sorted[i].max_tokens - alloc[i]);
            if grant > 0 {
                alloc[i] += grant;
                moved += grant;
            }
        }
        if moved == 0 {
            // No share moved (remaining / priority_sum = 0): the highest-priority active claim
            // takes one token, so the loop ends deterministically.
            let i = active[0];
            if sorted[i].max_tokens > alloc[i] && remaining > 0 {
                alloc[i] += 1;
                moved = 1;
            } else {
                break;
            }
        }
        remaining = remaining.saturating_sub(moved);
    }

    // Report in INPUT order (not priority order) for caller ergonomics.
    let mut by_input: Vec<(u32, AllocationState)> = vec![(0, AllocationState::Dropped); claims.len()];
    for (k, &input) in order.iter().enumerate() {
        by_input[input] = (alloc[k], state[k]);
    }
    let total_allocated = by_input.iter().map(|(tokens, _)| *tokens).fold(0u32, u32::saturating_add);
    let allocations = claims
        .iter()
        .zip(by_input)
        .map(|(src, (tokens, st))| SourceAllocation {
            source_id: src.source_id.clone(),
            allocated_tokens: tokens,
            requested_floor: src.floor_tokens,
            requested_min: src.min_tokens,
            requested_max: src.max_tokens,
            required: src.required,
            state: st,
        })
        .collect();
    FlexAllocation {
        allocations,
        unallocated: available.saturating_sub(total_allocated),
        escalation_needed,
        warnings,
    }
}

#[cfg(test)]
mod tests;
