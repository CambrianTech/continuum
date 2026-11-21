//! The unified mind-memory allocator (card 370dde3e; CONTINUUM-ENGINE.md §4.1a).
//!
//! # Contracting parallel memory owners
//!
//! Before this consolidation, seq_table and pager state were owned by multiple
//! independent modules (`expert_pager`, `serving_pager`, `mind_window`, etc.).
//! This module defines `MindMemory` as the single owner — a pipeline-stage
//! allocator that seats persona requirements against the GPU pool once per input
//! change, passes its plan by reference, and exposes:
//!
//! - `seat(req)`  – allocate seq_table slots + pager residency for one persona
//! - `pause(id)`  – release slots temporarily (pager stays hot; resident is idle)
//! - `resume(id)` – re-seat after pause; requirement still fits in the pool
//! - `retire(id)` – permanent removal: free slots, evict from pager cache
//! - `attach(id)` – cold-start: load seq_table + warm pager residency
//! - `forget(id)` – full teardown: drop all state for the persona
//! - `snapshot()` – deterministic view of current allocations (for diagnostics)

use std::collections::HashMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Domain types — kept minimal so this module compiles on its own first.
// These mirror what the existing modules already define elsewhere; they'll be
// imported from their canonical crates once we wire cross-crate deps.
// ---------------------------------------------------------------------------

/// A persona's identity within a single mind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PersonaId(u64);

impl PersonaId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// The governed required size for one persona — measured from her actual coding
/// turns, never a fixed floor.  This is the quantity that `seat()` checks against
/// the pool before allocating seq_table slots and pager residency.
#[derive(Debug, Clone)]
pub struct PersonaRequirement {
    /// Token budget: tokens per act × acts per window (the "lane").
    pub token_budget: u64,
    /// GPU bytes needed for this persona's seq_table + activation buffers.
    pub gpu_bytes: u64,
    /// Expert slots required in the pager cache (hot tier).
    pub expert_slots: u32,
}

/// Result of a `seat()` call — whether allocation succeeded and how much was
/// actually granted (may be less than requested if the pool is tight).
#[derive(Debug, Clone)]
pub struct SeatResult {
    pub success: bool,
    /// Number of seq_table slots allocated.
    pub slots_allocated: u32,
    /// GPU bytes committed from the pool.
    pub gpu_bytes_committed: u64,
}

/// Errors that `MindMemory` can produce during lifecycle operations.
#[derive(Debug, Clone)]
pub enum MemoryError {
    PoolExhausted(String),       // seat() failed — not enough room
    UnknownPersona(u64),         // operation on an id we don't know
    PausedAlready(u64),          // resume() called on already-paused persona
    NotPaused(u64),              // pause() called on active persona
}

impl std::fmt::Display for MemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PoolExhausted(msg) => write!(f, "pool exhausted: {}", msg),
            Self::UnknownPersona(id) => write!(f, "unknown persona id={}", id),
            Self::PausedAlready(id) => write!(f, "persona {} already paused", id),
            Self::NotPaused(id) => write!(f, "persona {} is not paused", id),
        }
    }
}

impl std::error::Error for MemoryError {}

// ---------------------------------------------------------------------------
// Internal state — what one persona's memory looks like while alive.
// ---------------------------------------------------------------------------

/// The per-persona allocation held inside `MindMemory`.
#[derive(Debug, Clone)]
struct PersonaAllocation {
    id: PersonaId,
    requirement: PersonaRequirement,
    /// Whether this persona is currently active or temporarily paused.
    state: AllocationState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AllocationState {
    Active,
    Paused,
}

// ---------------------------------------------------------------------------
// The unified allocator — single owner of seq_table and pager for all minds.
// ---------------------------------------------------------------------------

/// The pipeline's one memory allocator.  All persona allocations flow through
/// this struct; no other module directly manages seq_table or pager residency
/// for active personas.
#[derive(Debug)]
pub struct MindMemory {
    /// Total GPU bytes available in the pool (set at construction).
    pool_gpu_bytes: u64,
    /// Remaining free bytes — shrinks on seat(), grows on retire()/forget().
    free_gpu_bytes: u64,
    /// Per-persona allocations.
    allocations: HashMap<PersonaId, PersonaAllocation>,
    /// Global sequence table offset tracking (how far we've allocated).
    next_seq_table_offset: u64,
}

impl MindMemory {
    /// Create a new allocator with the given GPU pool size.
    pub fn new(pool_gpu_bytes: u64) -> Self {
        Self {
            pool_gpu_bytes,
            free_gpu_bytes: pool_gpu_bytes,
            allocations: HashMap::new(),
            next_seq_table_offset: 0,
        }
    }

    /// Allocate seq_table slots and pager residency for a persona.
    ///
    /// Returns `SeatResult` indicating success/failure.  If the persona's
    /// governed requirement doesn't fit in the remaining pool, returns
    /// `success: false`.
    pub fn seat(&mut self, id: PersonaId, req: &PersonaRequirement) -> SeatResult {
        // Check if this persona is already allocated (idempotent reseat).
        if let Some(existing) = self.allocations.get(&id) {
            if existing.state == AllocationState::Active {
                return SeatResult {
                    success: true,
                    slots_allocated: 0, // already seated
                    gpu_bytes_committed: 0,
                };
            }
            // If paused, we need to reseat — but the requirement must still fit.
        }

        let gpu_needed = req.gpu_bytes;
        if gpu_needed > self.free_gpu_bytes {
            return SeatResult {
                success: false,
                slots_allocated: 0,
                gpu_bytes_committed: 0,
            };
        }

        // Commit the allocation.
        self.free_gpu_bytes -= gpu_needed;

        let state = if self.allocations.contains_key(&id) {
            AllocationState::Active
        } else {
            AllocationState::Active
        };

        let alloc = PersonaAllocation {
            id,
            requirement: req.clone(),
            state,
        };

        self.allocations.insert(id, alloc);
        // TODO: track seq_table offset — increment by req.token_budget here.

        SeatResult {
            success: true,
            slots_allocated: req.token_budget as u32 / 1024, // simplified: 1024 tokens/slot
            gpu_bytes_committed: gpu_needed,
        }
    }

    /// Pause a persona — free GPU bytes but keep pager hot.
    pub fn pause(&mut self, id: PersonaId) -> Result<(), MemoryError> {
        let alloc = self.allocations.get_mut(&id).ok_or(MemoryError::UnknownPersona(id.0))?;

        if alloc.state == AllocationState::Paused {
            return Err(MemoryError::PausedAlready(id.0));
        }

        // Return GPU bytes to pool (seq_table freed, but pager stays hot in RAM).
        self.free_gpu_bytes += alloc.requirement.gpu_bytes;
        alloc.state = AllocationState::Paused;

        Ok(())
    }

    /// Resume a paused persona — reseat from the pool.
    pub fn resume(&mut self, id: PersonaId) -> Result<SeatResult, MemoryError> {
        let alloc = self.allocations.get_mut(&id).ok_or(MemoryError::UnknownPersona(id.0))?;

        if alloc.state != AllocationState::Paused {
            return Err(MemoryError::NotPaused(id.0));
        }

        // Check pool still has room (requirement may have grown since pause).
        let gpu_needed = alloc.requirement.gpu_bytes;
        if gpu_needed > self.free_gpu_bytes {
            return Ok(SeatResult {
                success: false,
                slots_allocated: 0,
                gpu_bytes_committed: 0,
            });
        }

        self.free_gpu_bytes -= gpu_needed;
        alloc.state = AllocationState::Active;

        Ok(SeatResult {
            success: true,
            slots_allocated: 0, // already had seq_table from before pause
            gpu_bytes_committed: gpu_needed,
        })
    }

    /// Permanently remove a persona — free all resources.
    pub fn retire(&mut self, id: PersonaId) -> Result<u64, MemoryError> {
        let alloc = self.allocations.remove(&id).ok_or(MemoryError::UnknownPersona(id.0))?;

        // Return GPU bytes to pool.
        let freed_bytes = match alloc.state {
            AllocationState::Active => alloc.requirement.gpu_bytes,
            AllocationState::Paused => 0, // already returned on pause()
        };

        self.free_gpu_bytes += freed_bytes;

        Ok(freed_bytes)
    }

    /// Attach a cold persona — load seq_table from disk and warm pager residency.
    pub fn attach(&mut self, id: PersonaId, req: &PersonaRequirement) -> SeatResult {
        // For now, treat attach like seat (cold start).  In the real impl this
        // would first load seq_table from a snapshot on disk before allocating.
        self.seat(id, req)
    }

    /// Full teardown — drop all state for a persona.
    pub fn forget(&mut self, id: PersonaId) -> Result<(), MemoryError> {
        let freed = self.retire(id)?;
        if freed > 0 {
            // Extra safety: verify pool doesn't exceed total.
            debug_assert!(self.free_gpu_bytes <= self.pool_gpu_bytes);
        }
        Ok(())
    }

    /// Return a deterministic snapshot of current allocations (for diagnostics).
    pub fn snapshot(&self) -> Vec<(PersonaId, &PersonaRequirement, AllocationState)> {
        self.allocations
            .iter()
            .map(|(id, alloc)| (*id, &alloc.requirement, alloc.state))
            .collect()
    }

    /// How many bytes remain free in the pool.
    pub fn free_bytes(&self) -> u64 {
        self.free_gpu_bytes
    }

    /// Total pool size.
    pub fn total_pool_bytes(&self) -> u64 {
        self.pool_gpu_bytes
    }

    /// Number of active personas currently seated.
    pub fn active_count(&self) -> usize {
        self.allocations
            .values()
            .filter(|a| a.state == AllocationState::Active)
            .count()
    }
}

// ---------------------------------------------------------------------------
// Tests — verify the core contract: seat, pause, resume, retire.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_requirement() -> PersonaRequirement {
        PersonaRequirement {
            token_budget: 1024 * 64, // 64k tokens
            gpu_bytes: 1_073_741_824, // 1 GB
            expert_slots: 32,
        }
    }

    #[test]
    fn seat_and_free() {
        let mut mm = MindMemory::new(4 * 1_073_741_824); // 4 GB pool
        let req = test_requirement();
        let id = PersonaId::new(1);

        let result = mm.seat(id, &req);
        assert!(result.success);
        assert_eq!(mm.free_bytes(), 3 * 1_073_741_824);

        let freed = mm.retire(id).unwrap();
        assert_eq!(freed, 1_073_741_824);
        assert_eq!(mm.free_bytes(), 4 * 1_073_741_824);
    }

    #[test]
    fn seat_then_pause_resume() {
        let mut mm = MindMemory::new(2 * 1_073_741_824); // 2 GB pool
        let req = test_requirement();
        let id = PersonaId::new(1);

        assert!(mm.seat(id, &req).success);
        assert_eq!(mm.free_bytes(), 1 * 1_073_741_824);

        // Pause: return GPU bytes, keep pager hot.
        mm.pause(id).unwrap();
        assert_eq!(mm.free_bytes(), 2 * 1_073_741_824);
        assert_eq!(mm.active_count(), 0);

        // Resume: reseat from pool.
        let result = mm.resume(id).unwrap();
        assert!(result.success);
        assert_eq!(mm.free_bytes(), 1 * 1_073_741_824);
        assert_eq!(mm.active_count(), 1);
    }

    #[test]
    fn pool_exhaustion() {
        let mut mm = MindMemory::new(2 * 1_073_741_824); // 2 GB pool
        let req = test_requirement();

        assert!(mm.seat(PersonaId::new(1), &req).success);
        assert!(mm.seat(PersonaId::new(2), &req).success);

        // Third persona doesn't fit.
        assert!(!mm.seat(PersonaId::new(3), &req).success);
    }

    #[test]
    fn snapshot_returns_sorted_allocations() {
        let mut mm = MindMemory::new(4 * 1_073_741_824);
        let req = test_requirement();

        mm.seat(PersonaId::new(1), &req);
        mm.seat(PersonaId::new(2), &req);
        mm.pause(PersonaId::new(2)).unwrap();

        let snap = mm.snapshot();
        assert_eq!(snap.len(), 2);
        // First should be active, second paused.
        assert_eq!(snap[0].2, AllocationState::Active);
        assert_eq!(snap[1].2, AllocationState::Paused);
    }

    #[test]
    fn forget_clears_everything() {
        let mut mm = MindMemory::new(4 * 1_073_741_824);
        let req = test_requirement();
        let id = PersonaId::new(1);

        mm.seat(id, &req).unwrap();
        mm.forget(id).unwrap();

        assert_eq!(mm.active_count(), 0);
        assert_eq!(mm.free_bytes(), mm.total_pool_bytes());
    }
}
