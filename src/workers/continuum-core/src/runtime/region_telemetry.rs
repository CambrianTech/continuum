//! RegionTelemetry — the structured event shape every brain region
//! emits per tick.
//!
//! Mandatory for every region. It's the only path the substrate
//! governor's yield-learning loop (algorithm 7) has into the regions
//! and the only operator surface for debugging cognitive cycles.
//!
//! Doctrine (from docs/architecture/BRAIN-REGIONS-SUBSTRATE.md):
//!
//! > Telemetry is mandatory for every region; it's the only way the
//! > yield-learning loop and the operator debugging path work. The
//! > derive macro generates the telemetry emission automatically.
//!
//! The derive macro lands later (once ≥3 regions exist to motivate
//! it); this slice ships the typed struct so regions can emit
//! manually.

use super::brain_region::RegionId;
use crate::governor::types::PressureSignal;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};
use ts_rs::TS;
use uuid::Uuid;

/// Per-tick telemetry shape every brain region emits.
///
/// Emitted on every tick. The substrate routes it to:
///
/// - **The governor** — reads `consumed_since_last` / `published` to
///   tune region budget (yield-learning loop, algorithm 7).
/// - **The operator surface** — `./jtag region/stats` / `region/yield`
///   read aggregate telemetry across personas.
/// - **The substrate event stream** — `RegionTickCompleted` and
///   `ReadyBufferUpdated` events for cross-region awareness.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/runtime/RegionTelemetry.ts"
)]
pub struct RegionTelemetry {
    /// Which region this came from. Stable string id.
    pub region_id: RegionId,

    /// Persona scope. `None` means the tick was global (background
    /// work not tied to a specific persona).
    #[ts(type = "string | null")]
    pub persona_id: Option<Uuid>,

    /// When this tick started (wall clock).
    #[ts(type = "string")]
    pub tick_started_at: SystemTime,

    /// How long the tick body ran.
    #[ts(type = "string")]
    pub tick_duration: Duration,

    /// Items the region published to ready-buffers this tick.
    #[ts(type = "number")]
    pub published: usize,

    /// Items in the region's ready-buffers consumed by handlers since
    /// the last tick.
    #[ts(type = "number")]
    pub consumed_since_last: usize,

    /// Handler `peek` calls that returned `None` since the last tick.
    /// Signals to the governor that the region should be upweighted
    /// (handlers are asking for stuff that's not staged yet).
    #[ts(type = "number")]
    pub buffer_misses_since_last: usize,

    /// Pressure the region observed (DB slow, embedding queue full,
    /// etc.). Surfaced to the governor for cascade evaluation.
    #[ts(optional)]
    pub pressure_observed: Option<PressureSignal>,
}

impl RegionTelemetry {
    /// Compute the consumption fraction. Used by the governor to
    /// upweight or downweight a region's budget. Returns `None` when
    /// `published` is zero (no signal this tick — preserve prior
    /// estimate rather than introducing a zero).
    pub fn consumption_fraction(&self) -> Option<f32> {
        if self.published == 0 {
            None
        } else {
            Some(self.consumed_since_last as f32 / self.published as f32)
        }
    }

    /// Whether handlers were asking for data the region hadn't staged.
    /// A positive value here is the governor's signal to give the
    /// region more budget.
    pub fn had_buffer_misses(&self) -> bool {
        self.buffer_misses_since_last > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(published: usize, consumed: usize, misses: usize) -> RegionTelemetry {
        RegionTelemetry {
            region_id: RegionId::from_static("test"),
            persona_id: Some(Uuid::nil()),
            tick_started_at: SystemTime::UNIX_EPOCH,
            tick_duration: Duration::from_millis(1),
            published,
            consumed_since_last: consumed,
            buffer_misses_since_last: misses,
            pressure_observed: None,
        }
    }

    #[test]
    fn test_consumption_fraction_with_publishes() {
        let t = sample(10, 7, 0);
        assert_eq!(t.consumption_fraction(), Some(0.7));
    }

    #[test]
    fn test_consumption_fraction_zero_published_returns_none() {
        let t = sample(0, 0, 3);
        assert_eq!(t.consumption_fraction(), None);
    }

    #[test]
    fn test_consumption_fraction_full_consumption() {
        let t = sample(5, 5, 0);
        assert_eq!(t.consumption_fraction(), Some(1.0));
    }

    #[test]
    fn test_had_buffer_misses_true_when_positive() {
        let t = sample(10, 5, 1);
        assert!(t.had_buffer_misses());
    }

    #[test]
    fn test_had_buffer_misses_false_when_zero() {
        let t = sample(10, 5, 0);
        assert!(!t.had_buffer_misses());
    }
}
