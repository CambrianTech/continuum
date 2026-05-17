//! Substrate Governor — Lane H from GENOME-FOUNDRY-SENTINEL #1327
//! Part 11. The DVFS layer for the AI substrate. ONE Rust subsystem
//! that makes "same code on MacBook Air and RTX 5090" real.
//!
//! See `types.rs` docstring for the full scope statement. PR-1 ships
//! the typed surface + a hardware-classification bridge
//! from `inference_capability::hw_probe` (PIECE-5 PR-3 #1335) to
//! `HardwareClass`.

pub mod cascade;
pub mod local;
pub mod policy_file;
pub mod policy_selector;
pub mod policy_watcher;
pub mod pressure_bridge;
pub mod types;

pub use cascade::{
    CASCADE_STEP_MAX, CASCADE_STEP_MIN, CascadeAction, CascadeThresholds, apply_action,
    evaluate_next_step,
};
pub use local::LocalSubstrateGovernor;
pub use policy_file::{
    PolicyFile, PolicyFileError, into_governor_policy, load_policy_file, parse_policy_text,
};
pub use policy_selector::{
    PolicySelectionError, hardware_fingerprint, policy_matches_hardware, select_policy,
};
pub use policy_watcher::{
    PolicyDirectoryError, PolicyDirectoryWatcher, load_policy_directory, reload_policy_candidates,
    watch_policy_directory,
};
pub use pressure_bridge::{alert_to_signal, governor_alert_sink};
pub use types::{
    CadenceMultipliers, ConcurrencyCaps, ConsolidationSchedule, FederationCadence, GovernorPolicy,
    GovernorSnapshot, HardwareClass, PowerSource, PressureSignal, RecallScoreWeights,
    SpeculationLevel, TargetSilicon, ThermalClass, ThermalSeverity, TierSizes, classify_hardware,
};

/// The trait every Substrate Governor implementation must satisfy.
///
/// PR-1 shipped the trait signature only — no concrete implementation.
/// PR-2 ships policy parsing. The cascade slice ships the reference
/// `LocalSubstrateGovernor` impl that other modules depend on.
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
    /// power source change, thermal class change). Selection failure is
    /// returned to the caller; the governor never silently invents a
    /// default policy.
    fn on_hardware_detected(
        &self,
        hw: HardwareClass,
    ) -> Result<(), policy_selector::PolicySelectionError>;

    /// Called by `PressureBroker` when a typed signal crosses a
    /// threshold. Governor decides whether to step the cascade, hold,
    /// or reverse. See Part 11 §"Adjustment Cascade" in
    /// GENOME-FOUNDRY-SENTINEL.md.
    fn on_pressure_signal(&self, signal: PressureSignal);

    /// Snapshot for VDD report emission + human inspection.
    fn snapshot(&self) -> GovernorSnapshot;
}
