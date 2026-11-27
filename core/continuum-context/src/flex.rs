//! A prompt's claims on its window, shared by the one flexbox (`continuum_sage::place::Flexbox`, the solver
//! memory shares bytes with: UNIFIED-PAGING.md, "Room by priority"). Moved from `continuum-grounding::rag_budget`;
//! the algorithm itself now lives once, in continuum-sage. Pure: no I/O, no adapter instance.
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
/// could take, and whether a required claim escalates.
#[derive(Debug, Clone, PartialEq)]
pub struct FlexAllocation {
    pub allocations: Vec<SourceAllocation>,
    pub unallocated: u32,
    /// A required claim got less than its floor.
    pub escalation_needed: bool,
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

/// The sum of `claims`' floors, saturating. Never her need: that is the window her act's
/// allocation chose (`allocation::need`), and a sum of requested floors overstated it.
pub fn floors(claims: &[FlexClaim]) -> u32 {
    claims.iter().map(|c| c.floor_tokens).fold(0, u32::saturating_add)
}

/// `available` tokens shared across `claims` by the one flexbox (`continuum_sage::place::Flexbox`, the
/// solver memory shares bytes with too). This adapter holds only the diff: a prompt's claims are named
/// by string and counted in `u32` tokens, and equal priorities are ordered by name.
pub fn flex_allocate(available: u32, claims: &[FlexClaim]) -> FlexAllocation {
    use continuum_sage::place::{Flex, Flexbox, Landed, Tokens};
    let tokens = |n: u32| Tokens(u64::from(n));
    let flex: Vec<Flex<Tokens>> = claims
        .iter()
        .map(|c| Flex { priority: c.priority, floor: tokens(c.floor_tokens), min: tokens(c.min_tokens), max: tokens(c.max_tokens), required: c.required })
        .collect();
    let mut shares = Vec::with_capacity(claims.len());
    let shared = Flexbox::default().share(tokens(available), &flex, |a, b| claims[a].source_id.cmp(&claims[b].source_id), &mut shares);
    let narrow = |t: Tokens| u32::try_from(t.0).unwrap_or(available); // unwrap_or: a share never exceeds `available`, the bound it already keeps
    let allocations = claims
        .iter()
        .zip(&shares)
        .map(|(claim, share)| SourceAllocation {
            source_id: claim.source_id.clone(),
            allocated_tokens: narrow(share.amount),
            requested_floor: claim.floor_tokens,
            requested_min: claim.min_tokens,
            requested_max: claim.max_tokens,
            required: claim.required,
            state: match share.landed {
                Landed::Satisfied => AllocationState::Satisfied,
                Landed::FloorOnly => AllocationState::FloorOnly,
                Landed::Dropped => AllocationState::Dropped,
                Landed::UnderProvisioned => AllocationState::UnderProvisioned,
            },
        })
        .collect();
    FlexAllocation { allocations, unallocated: narrow(shared.unallocated), escalation_needed: shared.escalation_needed }
}

#[cfg(test)]
mod tests;
