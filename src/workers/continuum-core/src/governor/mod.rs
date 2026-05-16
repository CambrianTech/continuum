//! Substrate Governor — Lane H from GENOME-FOUNDRY-SENTINEL #1327
//! Part 11. The DVFS layer for the AI substrate. ONE Rust subsystem
//! that makes "same code on MacBook Air and RTX 5090" real.
//!
//! See `types.rs` docstring for the full scope statement. PR-1 (this
//! commit) ships the typed surface + a hardware-classification bridge
//! from `inference_capability::hw_probe` (PIECE-5 PR-3 #1335) to
//! `HardwareClass`.

pub mod types;

pub use types::{
    classify_hardware, CadenceMultipliers, ConcurrencyCaps, ConsolidationSchedule,
    FederationCadence, GovernorPolicy, GovernorSnapshot, HardwareClass, PowerSource,
    PressureSignal, RecallScoreWeights, SpeculationLevel, TargetSilicon, ThermalClass,
    ThermalSeverity, TierSizes,
};

/// The trait every Substrate Governor implementation must satisfy.
///
/// PR-1 (this commit) ships the trait signature only — no concrete
/// implementation. PR-2 (tier-stores) doesn't need it. PR-3 (TOML
/// policy loader + cascade) ships the reference `LocalSubstrateGovernor`
/// impl that other modules depend on.
///
/// The governor never blocks reads. `current_policy()` is a wait-free
/// `Arc` clone. Writes hold a small mutex (under a microsecond) and
/// publish via `arc_swap`. A composer reading the policy 1000× per
/// turn pays no contention cost.
pub trait SubstrateGovernor: Send + Sync {
    /// Current policy. Cheap read: returns `Arc` to immutable snapshot
    /// so callers can hold without contention. Policy is rewritten
    /// under pressure, never mutated in place.
    fn current_policy(&self) -> std::sync::Arc<GovernorPolicy>;

    /// Called once at boot, and any time hardware changes (eGPU plug,
    /// power source change, thermal class change).
    fn on_hardware_detected(&self, hw: HardwareClass);

    /// Called by `PressureBroker` when a typed signal crosses a
    /// threshold. Governor decides whether to step the cascade, hold,
    /// or reverse. See Part 11 §"Adjustment Cascade" in
    /// GENOME-FOUNDRY-SENTINEL.md.
    fn on_pressure_signal(&self, signal: PressureSignal);

    /// Snapshot for VDD report emission + human inspection.
    fn snapshot(&self) -> GovernorSnapshot;
}
