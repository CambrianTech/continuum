//! Pressure bridge — maps PressureBroker alerts to governor signals.
//!
//! Lane H PR-4 of the substrate governor stack. The broker (CBAR-SUBSTRATE
//! Lane E) emits `PressureAlert` events whenever a registered pool crosses
//! the broker's threshold OR relief eviction fires. The governor's cascade
//! consumes typed `PressureSignal` enums. This module is the pure-function
//! bridge between the two surfaces.
//!
//! Per GENOME-FOUNDRY-SENTINEL.md Part 11 line 1121: "PressureBroker
//! informs the SubstrateGovernor. Pressure signals from the broker drive
//! the governor's adjustment cascade. The broker keeps owning admission;
//! the governor owns sizing."
//!
//! ## Scope of this PR
//!
//! - `alert_to_signal` — pure function: PressureAlert → Option<PressureSignal>
//! - `governor_alert_sink` — factory: wraps a governor as an `AlertSink`
//!   the broker can register via `PressureBroker::add_alert_sink`
//!
//! ## NOT in this PR
//!
//! - Wiring the sink into `PressureBrokerModule`'s boot path. That lives
//!   in a follow-up; the bridge is the data-side primitive, the wiring is
//!   a separate concern (lets reviewers reason about each independently).
//! - Pool-name-aware mapping (e.g. `vram` pool → `VRAMHigh`, `docker`
//!   pool → `DiskHigh` if/when that variant lands). Today's broker
//!   pools are memory-adjacent (DockerTierPool disk usage,
//!   HFCacheTierPool disk usage, GPU pool VRAM via GpuMemoryManager);
//!   `SystemMemHigh` is the conservative single-mapping that the
//!   cascade reacts to identically. Refinement is a follow-up once
//!   pool tier_name conventions stabilize.
//!
//! ## Failure-mode discipline
//!
//! Same posture as the rest of Lane H: no silent default-on-error. The
//! mapping is total (every alert produces either Some signal or None
//! explicitly), and the sink forwards only when Some. Normal / Warning
//! tier alerts produce None — the cascade explicitly only reacts to
//! High+ per the spec's threshold table (Part 11 §"Adjustment Cascade").

use crate::governor::types::PressureSignal;
use crate::governor::SubstrateGovernor;
use crate::paging::broker::{AlertSink, PressureAlert};
use std::sync::Arc;

/// Pure mapping: PressureBroker's alert → optional governor signal.
///
/// Returns `None` for tiers the cascade does not react to (Normal,
/// Warning). The cascade's enter thresholds (Part 11 §"Adjustment
/// Cascade") all start at High or above — Normal / Warning are
/// observational tiers the broker logs but the governor does not
/// step on.
///
/// Clamps `pressure` to the `[0.0, 1.0]` range before converting to
/// percent so a transient over-1.0 (capacity 0 edge cases) maps to 100%
/// and a negative artifact maps to 0% — both are correct conservative
/// answers; neither should panic the cascade.
pub fn alert_to_signal(alert: &PressureAlert) -> Option<PressureSignal> {
    match alert.tier.as_str() {
        "high" | "critical" => {
            let clamped = alert.pressure.clamp(0.0, 1.0);
            let used_pct = (clamped * 100.0).round() as u8;
            Some(PressureSignal::SystemMemHigh { used_pct })
        }
        // Normal / Warning are observational — broker logs the alert,
        // governor does not step. Unknown tier strings also return None
        // (future broker tier additions degrade safely; the cascade
        // ignores what it can't classify rather than guessing).
        _ => None,
    }
}

/// Factory: wrap a governor in an `AlertSink` the broker can register.
///
/// The returned closure captures an `Arc<dyn SubstrateGovernor>` so the
/// sink can be passed to multiple brokers if needed (a deployment may
/// have separate brokers per resource class one day). The sink:
///
/// 1. Calls `alert_to_signal` to convert the alert.
/// 2. If `Some`, forwards via `governor.on_pressure_signal`.
/// 3. If `None`, drops the alert silently — by design; the broker
///    already logged it at WARN level and the cascade does not react
///    to that tier.
///
/// Sinks run synchronously inside the broker's `relieve()` call, so the
/// governor's `on_pressure_signal` must be cheap (per the trait
/// contract: cascade evaluation < 10 μs per signal). The local
/// governor already meets this; this sink adds only the `alert_to_signal`
/// hop on top.
pub fn governor_alert_sink(governor: Arc<dyn SubstrateGovernor>) -> AlertSink {
    Arc::new(move |alert: PressureAlert| {
        if let Some(signal) = alert_to_signal(&alert) {
            governor.on_pressure_signal(signal);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governor::types::{GovernorPolicy, GovernorSnapshot, HardwareClass, PressureSignal};
    use crate::governor::PolicySelectionError;
    use std::sync::Mutex;

    // ─── alert_to_signal: tier filtering ──────────────────────────────

    fn alert_at(tier: &str, pressure: f64) -> PressureAlert {
        PressureAlert {
            tier_name: "fake-pool".to_string(),
            pressure,
            tier: tier.to_string(),
            bytes_freed: 0,
            action_taken: false,
            at_ms: 0,
        }
    }

    /// What this catches: Normal-tier alerts produce no signal. The
    /// cascade is observational at Normal; emitting a signal here would
    /// constantly fire `on_pressure_signal` on a quiet system and burn
    /// the cascade-transition counter for no reason.
    #[test]
    fn normal_tier_returns_none() {
        assert_eq!(alert_to_signal(&alert_at("normal", 0.30)), None);
    }

    /// What this catches: Warning-tier alerts produce no signal either.
    /// Per spec the cascade only enters its first throttled step at
    /// High+ (warning is "approaching, not crossing"). If a future
    /// design wants Warning to drive a soft-throttle, that's a different
    /// PR — surface the change in the bridge's mapping table here.
    #[test]
    fn warning_tier_returns_none() {
        assert_eq!(alert_to_signal(&alert_at("warning", 0.70)), None);
    }

    /// What this catches: High-tier alerts produce `SystemMemHigh` with
    /// the alert pressure rounded to percent. The whole point of the
    /// bridge — without this, the broker's High alerts never reach the
    /// governor and the cascade never steps.
    #[test]
    fn high_tier_returns_system_mem_high() {
        let signal = alert_to_signal(&alert_at("high", 0.85));
        assert_eq!(signal, Some(PressureSignal::SystemMemHigh { used_pct: 85 }));
    }

    /// What this catches: Critical-tier alerts also produce
    /// `SystemMemHigh` (same variant — cascade differentiates response
    /// by used_pct, not by signal subtype). Critical fires the cascade's
    /// final step via the same code path High does.
    #[test]
    fn critical_tier_returns_system_mem_high() {
        let signal = alert_to_signal(&alert_at("critical", 0.97));
        assert_eq!(signal, Some(PressureSignal::SystemMemHigh { used_pct: 97 }));
    }

    /// What this catches: unknown tier strings degrade safely to None.
    /// If the broker adds a new tier label without updating the bridge,
    /// the cascade ignores it (silent-degrade is correct here because
    /// the broker already logged the alert at WARN; the governor just
    /// declines to react to a tier it doesn't classify).
    #[test]
    fn unknown_tier_returns_none() {
        assert_eq!(alert_to_signal(&alert_at("emergency", 0.99)), None);
        assert_eq!(alert_to_signal(&alert_at("", 0.99)), None);
    }

    // ─── alert_to_signal: pressure clamping ───────────────────────────

    /// What this catches: pressure > 1.0 clamps to used_pct = 100. The
    /// broker emits pressure as a ratio normally in [0,1] but capacity-0
    /// edge cases or transient over-budget snapshots can push it higher.
    /// Without clamping, `(1.5 * 100.0) as u8` would overflow / wrap and
    /// produce a nonsense used_pct value the cascade would step on.
    #[test]
    fn pressure_above_one_clamps_to_100_pct() {
        let signal = alert_to_signal(&alert_at("critical", 1.5));
        assert_eq!(
            signal,
            Some(PressureSignal::SystemMemHigh { used_pct: 100 })
        );
    }

    /// What this catches: negative pressure clamps to used_pct = 0. A
    /// negative artifact from a buggy pool implementation shouldn't
    /// propagate as a nonsense large unsigned value (`(-0.5 * 100.0) as
    /// u8` wraps to 206 on most targets). Clamp to 0 — the High tier
    /// label keeps the signal in scope, but the percent is honest.
    #[test]
    fn pressure_below_zero_clamps_to_zero_pct() {
        let signal = alert_to_signal(&alert_at("high", -0.5));
        assert_eq!(signal, Some(PressureSignal::SystemMemHigh { used_pct: 0 }));
    }

    /// What this catches: pressure rounding (0.855 → 86, not 85). The
    /// cascade's enter-thresholds are on percent boundaries; without
    /// `.round()` the integer truncation would shift every alert one
    /// step toward the lower tier.
    #[test]
    fn pressure_rounds_to_nearest_pct() {
        let signal = alert_to_signal(&alert_at("high", 0.855));
        assert_eq!(signal, Some(PressureSignal::SystemMemHigh { used_pct: 86 }));
    }

    // ─── governor_alert_sink: forwarding ──────────────────────────────

    /// Test double — records every signal the bridge forwards. Trait
    /// methods are all `&self`; the recorded signals live behind a Mutex
    /// so tests can assert on what the sink dispatched.
    struct RecordingGovernor {
        signals: Mutex<Vec<PressureSignal>>,
    }

    impl RecordingGovernor {
        fn new() -> Self {
            Self {
                signals: Mutex::new(Vec::new()),
            }
        }

        fn recorded(&self) -> Vec<PressureSignal> {
            self.signals.lock().unwrap().clone()
        }
    }

    impl SubstrateGovernor for RecordingGovernor {
        fn current_policy(&self) -> Arc<GovernorPolicy> {
            unimplemented!("not exercised in pressure_bridge tests")
        }

        fn on_hardware_detected(&self, _hw: HardwareClass) -> Result<(), PolicySelectionError> {
            unimplemented!("not exercised in pressure_bridge tests")
        }

        fn on_pressure_signal(&self, signal: PressureSignal) {
            self.signals.lock().unwrap().push(signal);
        }

        fn snapshot(&self) -> GovernorSnapshot {
            unimplemented!("not exercised in pressure_bridge tests")
        }
    }

    /// What this catches: High-tier alert forwards to governor.
    /// Integration check that the sink composes `alert_to_signal` +
    /// `governor.on_pressure_signal` correctly — without this, a
    /// regression in the closure body would break the bridge silently.
    #[test]
    fn sink_forwards_high_tier_to_governor() {
        let governor = Arc::new(RecordingGovernor::new());
        let sink = governor_alert_sink(governor.clone() as Arc<dyn SubstrateGovernor>);
        sink(alert_at("high", 0.88));
        assert_eq!(
            governor.recorded(),
            vec![PressureSignal::SystemMemHigh { used_pct: 88 }]
        );
    }

    /// What this catches: Critical-tier alert also forwards (same path
    /// as High in the current bridge; pinned to prevent a future
    /// refactor accidentally gating only on "high").
    #[test]
    fn sink_forwards_critical_tier_to_governor() {
        let governor = Arc::new(RecordingGovernor::new());
        let sink = governor_alert_sink(governor.clone() as Arc<dyn SubstrateGovernor>);
        sink(alert_at("critical", 0.96));
        assert_eq!(
            governor.recorded(),
            vec![PressureSignal::SystemMemHigh { used_pct: 96 }]
        );
    }

    /// What this catches: Normal-tier alert does NOT call the governor.
    /// Critical for cascade-transition-counter hygiene — every spurious
    /// `on_pressure_signal` call bumps the counter and pollutes the
    /// snapshot's diagnostic value.
    #[test]
    fn sink_does_not_forward_normal_tier() {
        let governor = Arc::new(RecordingGovernor::new());
        let sink = governor_alert_sink(governor.clone() as Arc<dyn SubstrateGovernor>);
        sink(alert_at("normal", 0.30));
        assert_eq!(governor.recorded(), vec![]);
    }

    /// What this catches: Warning-tier also does not forward. Same
    /// reasoning as the Normal test; pinned separately so a future
    /// "warning forwards a SoftThrottle signal" change must update this
    /// test deliberately.
    #[test]
    fn sink_does_not_forward_warning_tier() {
        let governor = Arc::new(RecordingGovernor::new());
        let sink = governor_alert_sink(governor.clone() as Arc<dyn SubstrateGovernor>);
        sink(alert_at("warning", 0.72));
        assert_eq!(governor.recorded(), vec![]);
    }

    /// What this catches: multiple alerts forward in order. Sinks may
    /// be called rapid-fire (one per pool per broker tick during a
    /// pressure event); the sink must be reentrant and the governor
    /// must see each signal — no coalescing at the bridge layer.
    #[test]
    fn sink_forwards_multiple_alerts_in_order() {
        let governor = Arc::new(RecordingGovernor::new());
        let sink = governor_alert_sink(governor.clone() as Arc<dyn SubstrateGovernor>);
        sink(alert_at("high", 0.82));
        sink(alert_at("critical", 0.97));
        sink(alert_at("normal", 0.10)); // skipped
        sink(alert_at("high", 0.90));
        assert_eq!(
            governor.recorded(),
            vec![
                PressureSignal::SystemMemHigh { used_pct: 82 },
                PressureSignal::SystemMemHigh { used_pct: 97 },
                PressureSignal::SystemMemHigh { used_pct: 90 },
            ]
        );
    }

    /// What this catches: sink survives sharing across closures (Arc
    /// cloning the underlying governor). Pins that the factory's
    /// closure captures the Arc, not a borrow — otherwise sinks could
    /// not outlive their construction scope and could not be registered
    /// with a broker that lives longer than the construction site.
    #[test]
    fn sink_is_send_and_callable_after_construction_scope() {
        let governor = Arc::new(RecordingGovernor::new());
        let sink_holder: AlertSink = {
            let g = governor.clone();
            governor_alert_sink(g as Arc<dyn SubstrateGovernor>)
        };
        // construction scope is gone; sink should still be callable
        sink_holder(alert_at("high", 0.85));
        assert_eq!(
            governor.recorded(),
            vec![PressureSignal::SystemMemHigh { used_pct: 85 }]
        );
    }
}
