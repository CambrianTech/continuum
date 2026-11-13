//! Physical conservation (ONE-ACT stage 5): on unified memory, every engine's weights, every
//! mind's resident KV and the staging the engine allocates are counted ONCE against the device.
//! Computed from [`ModelFootprint`], the same numbers the planner sizes with (`kv_at`,
//! `compute_buffer_per_lane`), so a scenario that passes here is one the planner's own arithmetic
//! agrees fits.

use crate::inference::planning::serving_plan::ModelFootprint;

/// One engine resident on the device: its model, the KV tokens its pool holds, and the sequences
/// it reserves staging (compute buffers) for.
#[derive(Debug, Clone, Copy)]
pub struct EngineResidency<'a> {
    pub footprint: &'a ModelFootprint,
    pub kv_tokens: u64,
    pub sequences: u32,
}

impl EngineResidency<'_> {
    /// Weights + resident KV + staging, in bytes.
    pub fn bytes(&self) -> u64 {
        let kv = self.footprint.kv_per_token.saturating_mul(self.kv_tokens);
        let staging = self.footprint.compute_buffer_per_lane().saturating_mul(u64::from(self.sequences));
        self.footprint.weights_bytes.saturating_add(kv).saturating_add(staging)
    }
}

/// What the device holds after every resident engine is counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conservation {
    pub device_bytes: u64,
    pub resident_bytes: u64,
}

impl Conservation {
    /// The accounting: weights + resident KV + staging of every engine on the device.
    pub fn of(device_bytes: u64, engines: &[EngineResidency<'_>]) -> Self {
        let resident_bytes = engines.iter().map(EngineResidency::bytes).fold(0u64, u64::saturating_add);
        Self { device_bytes, resident_bytes }
    }

    /// The invariant: nothing resident spends bytes the device does not have.
    pub fn holds(&self) -> bool {
        self.resident_bytes <= self.device_bytes
    }

    /// Bytes left on the device; zero when overspent.
    pub fn headroom_bytes(&self) -> u64 {
        self.device_bytes.saturating_sub(self.resident_bytes)
    }
}
