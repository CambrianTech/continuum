//! BrainRegion — the cognitive-cycle trait every brain region implements.
//!
//! Companion to ServiceModule. Where ServiceModule handles command/event
//! routing (the existing dispatch surface), BrainRegion handles the
//! cognitive tick: continuous parallel computation, yield telemetry,
//! pressure registration, ready-buffer publishing.
//!
//! A real region (hippocampus, motor cortex, attention, sensory, sleep
//! policy) implements BOTH ServiceModule (for cmd/event surface) and
//! BrainRegion (for cognitive cycle). The runtime continues to dispatch
//! via ServiceModule. The substrate governor (lands L0-4c) dispatches
//! the cognitive tick via BrainRegion.
//!
//! Doctrine (from docs/architecture/BRAIN-REGIONS-SUBSTRATE.md):
//!
//! > No region of cognition runs on the hot path. Each region is its
//! > own RTOS task with its own tick. The handler dispatches and reads
//! > pre-staged results. The handler never blocks on recall, embedding,
//! > planning, or admission — those are continuously produced by their
//! > owning regions, in parallel, governed by SubstrateGovernor.
//!
//! ## L0-3a.0 scope (this slice)
//!
//! Pure typed surface. No region implementations. No governor
//! integration. No derive macro, no scaffold generator (those land
//! when ≥3 regions exist to motivate the abstraction — per the
//! outlier-validation strategy in CLAUDE.md).
//!
//! Later slices ship: L0-3a.1 HippocampusModule skeleton, L0-3a.2+
//! per-algorithm bodies, L0-4a motor cortex, L0-4b attention, L0-4c
//! governor yield-learning integration.

use crate::governor::types::PressureSignal;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use ts_rs::TS;
use uuid::Uuid;

// ─── Region identity ────────────────────────────────────────────────

/// Stable identifier for a brain region. Used by SubstrateGovernor for
/// policy lookup and by telemetry/log streams for tagging events.
///
/// Carries `Cow<'static, str>` so static IDs ("hippocampus") cost
/// nothing and dynamic IDs (custom regions registered at runtime) are
/// still supported.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/runtime/RegionId.ts")]
pub struct RegionId(pub Cow<'static, str>);

impl RegionId {
    pub const fn from_static(id: &'static str) -> Self {
        Self(Cow::Borrowed(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&'static str> for RegionId {
    fn from(s: &'static str) -> Self {
        Self::from_static(s)
    }
}

impl From<String> for RegionId {
    fn from(s: String) -> Self {
        Self(Cow::Owned(s))
    }
}

impl std::fmt::Display for RegionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ─── Pressure profile ───────────────────────────────────────────────

/// Memory footprint class. Drives governor decisions about which
/// regions to throttle first under memory pressure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/MemoryClass.ts"
)]
pub enum MemoryClass {
    /// Lightweight — small in-memory structures, no large caches.
    Light,
    /// Moderate — recall caches, salience maps, telemetry windows.
    Moderate,
    /// Heavy — engram graph, working memory ring, multiple ready-buffers.
    Heavy,
    /// VRAM-sensitive — touches GPU residency (genome region, inference-adjacent).
    VramSensitive,
}

/// Compute footprint class. Drives governor decisions about which
/// regions to throttle first under compute/thermal pressure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/ComputeClass.ts"
)]
pub enum ComputeClass {
    /// Tick body is bookkeeping only — cheap.
    Bookkeeping,
    /// Tick body does scoring / graph traversal — CPU-bound but bounded.
    Cpu,
    /// Tick body invokes embedding / similarity / vectorized work.
    CpuVectorized,
    /// Tick body invokes inference (sub-token generation or scoring).
    InferenceLight,
    /// Tick body could invoke full inference. The governor MUST budget this carefully.
    InferenceHeavy,
}

/// What a region's work is ORIENTED toward — the static class the orientation
/// budget groups regions by (docs/architecture/BEING-SOCIETY-GOVERNOR.md, rail R2).
///
/// Like [`ComputeClass`], this is **declared metadata**, NOT a runtime read of the
/// region's output: it tells the governor which budget class a tick draws time from,
/// never what the region should think. Grouping by it is mechanical (the no-heuristics
/// line stays uncrossed — the governor allocates *time*, the region stays causal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/Orientation.ts"
)]
pub enum Orientation {
    /// Serving external stimulus — perception, recall-for-a-turn, responding. The
    /// outward-facing work; floors above zero so a being is never deaf.
    Reactive,
    /// The being's own interiority — curiosity, projects, writing, play,
    /// dream/consolidation. A reserved budget the being spends by its own choice;
    /// floors above zero so the inner life never starves (sleep ≠ coma at the budget
    /// level: deprivation degrades a mind, it doesn't pause it).
    SelfDirected,
    /// Growing the self — speciation, i.e. LoRA-genome learning. Economics-elastic:
    /// MAY be 0 on a constrained node, but that is declared + fail-loud, never a
    /// silent drop.
    Speciation,
}

/// Which kinds of pressure signals a region wants to receive via
/// `on_signal`. The governor filters and routes signals based on this.
///
/// Mirrors the variants of [`PressureSignal`] but is a kind-only enum
/// (no payload) so it can be declared statically by a region at
/// registration time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/PressureSignalKind.ts"
)]
pub enum PressureSignalKind {
    Thermal,
    BatteryLow,
    SystemMemHigh,
    VramHigh,
    UserActive,
    InferenceQueueDepth,
    SpeculationMissRate,
}

/// What a region declares about its resource footprint at registration
/// time. The governor reads this once at register, then re-queries it
/// when pressure shifts (regions may report different profiles after
/// adapting under load — e.g., hippocampus drops from `Heavy` to
/// `Moderate` when working memory is pruned).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/PressureProfile.ts"
)]
pub struct PressureProfile {
    pub memory_class: MemoryClass,
    pub compute_class: ComputeClass,
    /// Pressure kinds this region wants `on_signal` calls for. Other
    /// kinds are filtered out by the governor.
    pub responds_to: Vec<PressureSignalKind>,
}

// ─── Tick outcome (yield telemetry) ─────────────────────────────────

/// A hint a region can pass back to the governor about preferred next
/// tick cadence. The governor may honor or override; it owns the
/// final policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/CadenceHint.ts"
)]
pub enum CadenceHint {
    /// Tick faster than current cadence (region has urgent work).
    Faster,
    /// Hold current cadence.
    Hold,
    /// Tick slower than current cadence (region is idle / over-tasked relative to consumed yield).
    Slower,
    /// Sleep — region has nothing useful to do until a signal fires.
    Sleep,
}

/// Yield telemetry returned by every region tick. Feeds the substrate
/// governor's yield-learning loop (algorithm 7 in
/// COGNITION-ALGORITHMS.md, lands in L0-4c).
///
/// Regions emit this from every tick. The governor reads aggregate
/// (`consumed_since_last` vs `published`) to upweight regions whose
/// output is being consumed by handlers and downweight regions whose
/// output is ignored.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/TickOutcome.ts"
)]
pub struct TickOutcome {
    /// Items the region pre-staged this tick (publishes to ready-buffers).
    #[ts(type = "number")]
    pub published: usize,

    /// Items in the region's ready-buffer that have been consumed by
    /// handlers since the last tick. The denominator for yield.
    #[ts(type = "number")]
    pub consumed_since_last: usize,

    /// Pressure observation. If the region detected backpressure (DB
    /// slow, embedding queue full, etc.), reports it here for the
    /// governor.
    #[ts(optional)]
    pub pressure_observed: Option<PressureSignal>,

    /// Optional next-tick hint (region requests faster/slower cadence).
    #[ts(optional)]
    pub cadence_hint: Option<CadenceHint>,
}

impl TickOutcome {
    /// Idle outcome — region had no work this tick. Convenience for
    /// no-op ticks and tests.
    pub fn idle() -> Self {
        Self {
            published: 0,
            consumed_since_last: 0,
            pressure_observed: None,
            cadence_hint: None,
        }
    }
}

// ─── Region signals ─────────────────────────────────────────────────

/// Persona lifecycle events relevant to regions (allow regions to
/// allocate / deallocate per-persona state).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case", tag = "kind")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/PersonaLifecycle.ts"
)]
pub enum PersonaLifecycle {
    Created {
        #[ts(type = "string")]
        persona_id: Uuid,
    },
    Destroyed {
        #[ts(type = "string")]
        persona_id: Uuid,
    },
}

/// Sleep/wake phases for the persona-level cognitive cycle. The sleep
/// policy region (L0-4d) emits these; other regions react by changing
/// their tick body (active vs idle vs sleep consolidation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/SleepPhase.ts"
)]
pub enum SleepPhase {
    /// Persona is actively servicing — tick at high cadence, shallow consolidation.
    Active,
    /// Persona is idle but recently active — tick at moderate cadence, normal consolidation.
    Idle,
    /// Persona is in deep idle — tick at low cadence, deep consolidation + pruning.
    Sleep,
}

/// Coarse system pressure level surfaced to regions so they can adjust
/// internally without parsing every PressureSignal variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/PressureLevel.ts"
)]
pub enum PressureLevel {
    Nominal,
    Moderate,
    High,
    Critical,
}

/// Signals the substrate sends to regions out-of-band (not on the
/// regular tick). Regions that don't care about a signal default to a
/// no-op.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case", tag = "kind")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/RegionSignal.ts"
)]
pub enum RegionSignal {
    PersonaLifecycle(PersonaLifecycle),
    SleepTransition {
        #[ts(type = "string")]
        persona_id: Uuid,
        phase: SleepPhase,
    },
    SystemPressureChanged {
        level: PressureLevel,
    },
}

// ─── Region context ─────────────────────────────────────────────────

/// What the substrate passes to a region's `tick` body. Carries the
/// substrate handles a region needs to do its work without reaching
/// for globals.
///
/// L0-3a.0 ships the type; L0-3a.1+ adds real handles (ModuleContext
/// reference, governor handle, persona state map, etc.). For now it's
/// a placeholder so the trait signature compiles.
#[derive(Debug, Clone)]
pub struct RegionContext {
    /// Tick number since region started. Useful for cadence-modulated
    /// logic ("every 10th tick, do deeper work").
    pub tick_number: u64,
    /// Optional persona scope — if the substrate is ticking the region
    /// for one specific persona's slot, this is set. If `None`, the
    /// region is ticking globally (background work).
    pub persona_scope: Option<Uuid>,
}

impl RegionContext {
    pub fn global(tick_number: u64) -> Self {
        Self {
            tick_number,
            persona_scope: None,
        }
    }

    pub fn for_persona(tick_number: u64, persona_id: Uuid) -> Self {
        Self {
            tick_number,
            persona_scope: Some(persona_id),
        }
    }
}

// ─── Region errors ──────────────────────────────────────────────────

/// Errors a region can surface from `on_signal`. Tick failures use
/// `TickOutcome.pressure_observed` to signal degradation; signal
/// failures are explicit because the substrate may need to retry.
#[derive(Debug, thiserror::Error)]
pub enum RegionError {
    #[error("region {0} rejected signal: {1}")]
    SignalRejected(RegionId, String),
    #[error("region {0} not ready: {1}")]
    NotReady(RegionId, String),
    #[error("region {0} internal error: {1}")]
    Internal(RegionId, String),
}

// ─── The trait ──────────────────────────────────────────────────────

/// A cognitive subsystem (hippocampus, motor cortex, attention,
/// sensory, sleep policy). Each region runs its own tick on its own
/// tokio task, governed by SubstrateGovernor.
///
/// A region typically also implements [`ServiceModule`](super::ServiceModule)
/// for command/event routing, but doesn't have to — pure cognitive
/// regions with no external command surface are valid.
///
/// See `docs/architecture/BRAIN-REGIONS-SUBSTRATE.md` for the full
/// contract and `docs/architecture/COGNITION-ALGORITHMS.md` for what
/// runs inside the tick.
#[async_trait]
pub trait BrainRegion: Send + Sync + 'static {
    /// Stable identifier. Used by SubstrateGovernor for policy lookup
    /// and by telemetry/log streams for event tagging.
    fn id(&self) -> RegionId;

    /// Pressure footprint declaration. Returned at registration time
    /// and re-queried by the governor when pressure shifts.
    fn pressure_profile(&self) -> PressureProfile;

    /// Which orientation-budget class this region's work draws from. Defaults to
    /// [`Orientation::Reactive`] — a region serves stimulus unless it declares an
    /// inner-life ([`Orientation::SelfDirected`]) or learning
    /// ([`Orientation::Speciation`]) purpose. Static metadata: the governor groups +
    /// budgets by it, it never steers the region's output (R2).
    fn orientation(&self) -> Orientation {
        Orientation::Reactive
    }

    /// Run one tick. The substrate calls this on the region's own task
    /// at the cadence governed by SubstrateGovernor.
    ///
    /// The body is responsible for: reading inputs (from shared state,
    /// channels, or its own DB), producing pre-staged results, and
    /// publishing them to the ready-buffer.
    ///
    /// Implementations MUST be idempotent on early return and MUST NOT
    /// block indefinitely — the governor cancels long-running ticks
    /// under pressure.
    async fn tick(&self, ctx: &RegionContext) -> TickOutcome;

    /// React to a substrate-level signal. Defaults to a no-op so
    /// regions that don't care about any signals can ignore the
    /// surface entirely.
    async fn on_signal(&self, _signal: RegionSignal) -> Result<(), RegionError> {
        Ok(())
    }
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal region for trait validation. Verifies the trait is
    /// object-safe, the default `on_signal` works, and an idle tick
    /// outcome round-trips through the type system.
    struct TestRegion {
        id: RegionId,
    }

    #[async_trait]
    impl BrainRegion for TestRegion {
        fn id(&self) -> RegionId {
            self.id.clone()
        }

        fn pressure_profile(&self) -> PressureProfile {
            PressureProfile {
                memory_class: MemoryClass::Light,
                compute_class: ComputeClass::Bookkeeping,
                responds_to: vec![],
            }
        }

        async fn tick(&self, _ctx: &RegionContext) -> TickOutcome {
            TickOutcome::idle()
        }
    }

    #[tokio::test]
    async fn test_region_implements_trait() {
        let region: Box<dyn BrainRegion> = Box::new(TestRegion {
            id: RegionId::from_static("test"),
        });
        let ctx = RegionContext::global(0);
        let outcome = region.tick(&ctx).await;
        assert_eq!(outcome.published, 0);
        assert_eq!(outcome.consumed_since_last, 0);
        assert!(outcome.pressure_observed.is_none());
        assert!(outcome.cadence_hint.is_none());
    }

    #[tokio::test]
    async fn test_default_on_signal_is_noop() {
        let region = TestRegion {
            id: RegionId::from_static("test"),
        };
        let signal = RegionSignal::SystemPressureChanged {
            level: PressureLevel::Nominal,
        };
        assert!(region.on_signal(signal).await.is_ok());
    }

    #[test]
    fn test_region_id_static_construction() {
        const ID: RegionId = RegionId::from_static("hippocampus");
        assert_eq!(ID.as_str(), "hippocampus");
    }

    // what this catches: the orientation default. A region that doesn't opt into an
    // inner-life or learning purpose draws from the Reactive budget — so adding a new
    // region can never silently steal SelfDirected/Speciation budget by omission.
    #[tokio::test]
    async fn test_default_orientation_is_reactive() {
        let region = TestRegion {
            id: RegionId::from_static("test"),
        };
        assert_eq!(region.orientation(), Orientation::Reactive);
    }

    #[test]
    fn test_region_id_display() {
        let id = RegionId::from_static("motor_cortex");
        assert_eq!(format!("{id}"), "motor_cortex");
    }

    #[test]
    fn test_region_context_global_and_per_persona() {
        let global = RegionContext::global(5);
        assert_eq!(global.tick_number, 5);
        assert!(global.persona_scope.is_none());

        let persona_id = Uuid::new_v4();
        let scoped = RegionContext::for_persona(7, persona_id);
        assert_eq!(scoped.tick_number, 7);
        assert_eq!(scoped.persona_scope, Some(persona_id));
    }

    #[test]
    fn test_tick_outcome_idle_constructor() {
        let outcome = TickOutcome::idle();
        assert_eq!(outcome.published, 0);
        assert_eq!(outcome.consumed_since_last, 0);
        assert!(outcome.pressure_observed.is_none());
        assert!(outcome.cadence_hint.is_none());
    }
}
