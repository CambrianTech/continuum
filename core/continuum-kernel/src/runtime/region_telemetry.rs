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
    export_to = "../../../protocol/typescript/runtime/RegionTelemetry.ts"
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

