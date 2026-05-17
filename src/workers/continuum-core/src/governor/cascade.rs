//! Substrate governor cascade evaluator — Lane H PR-3c1 per
//! GENOME-FOUNDRY-SENTINEL #1327 Part 11 §"Adjustment Cascade".
//!
//! PR-3b (#1354) shipped `LocalSubstrateGovernor` that RECORDS
//! pressure signals. This PR-3c1 ships the pure-function CASCADE
//! EVALUATOR — given (current cascade step, incoming signal, time-in-
//! step), decide whether to advance, hold, or retreat.
//!
//! PR-3c2 wires this evaluator into `on_pressure_signal` to actually
//! transition the governor's cascade_step + rewrite policy fields per
//! the action.
//!
//! ## Cascade semantics (from spec)
//!
//! 6 steps, 0 = normal, 5 = max throttle. Each step has:
//! - An **enter** condition (any signal can trigger advance)
//! - An **exit** condition (ALL clear required to retreat — the
//!   hysteresis that prevents oscillation)
//! - A **time-in-step** requirement before further advance (slows
//!   the cascade so brief spikes don't immediately escalate)
//!
//! ## Anti-oscillation: restore-speculation-one-step-later
//!
//! Spec rule: when retreating from step N → step N-1, the
//! speculation level is restored ONE STEP LATER than the rest of the
//! policy. Concretely: drop speculation on advance (step 1), restore
//! on retreat (step 0 → step -1, which is a no-op). The "one step
//! later" semantics: if pressure cleared at step 1, retreat to step 0
//! but keep speculation throttled until the NEXT retreat opportunity.
//! Since step 0 IS the lowest, the restoration happens "naturally" on
//! the next pressure-clear evaluation that confirms sustained calm.
//!
//! This file ships the pure-function evaluator. PR-3c2 wires the
//! `apply_action_to_policy` side-effect.
//!
//! ## Failure-mode discipline
//!
//! - All thresholds are typed + named (no magic floats / ints scattered
//!   through call sites)
//! - `evaluate_next_step` is pure — same inputs → same output. PR-3c2
//!   tests the integration; PR-3c1 tests the rule.
//! - No silent skip on unknown signal kinds — every variant of
//!   `PressureSignal` participates in evaluation, even if some are
//!   no-ops for the current step (`UserActive` doesn't trigger
//!   advance, but the evaluator returns Hold rather than panic).

use crate::governor::types::{PressureSignal, ThermalSeverity};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Cascade step. 0 = normal operation; 1..5 = increasing throttle.
/// The spec enumerates 6 levels (0..5); this enum models them as a
/// transparent newtype so PR-3c2 can compare + bound check.
///
/// Why `u8` not enum: cascade arithmetic (step + 1, step - 1) is
/// frequent; a u8 with `saturating_add`/`saturating_sub` is cleaner
/// than 6 named match arms. The constants below name the canonical
/// values for diagnostic readability.
pub const CASCADE_STEP_MIN: u8 = 0;
pub const CASCADE_STEP_MAX: u8 = 5;

/// Decision the cascade evaluator emits per signal. PR-3c2 wires
/// these into the local governor's `on_pressure_signal` to actually
/// rewrite the policy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[ts(export, export_to = "../../../shared/generated/governor/CascadeAction.ts")]
pub enum CascadeAction {
    /// Keep the current step. The pressure signal didn't cross any
    /// threshold (or didn't cross it for long enough).
    Hold,
    /// Advance one step toward higher throttle. Capped at
    /// CASCADE_STEP_MAX — already-at-max returns Hold.
    Advance,
    /// Retreat one step toward normal. Capped at CASCADE_STEP_MIN —
    /// already-at-min returns Hold.
    Retreat,
    /// Emergency advance to MAX immediately, skipping intermediate
    /// steps. Per spec: thermal Critical + battery < 10% trigger this
    /// to protect hardware/user.
    EmergencyAdvanceToMax,
}

/// Tuneable thresholds for the cascade. Loaded from policy file in
/// PR-3c2 (extends PolicyFile). For PR-3c1, callers pass typed values
/// so the evaluator is testable with any threshold set.
///
/// Pinned to the values from the spec's §"Adjustment Cascade" table;
/// callers may override per-policy (the spec's table is the default
/// for the M-Air anchor + 5090 anchor).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../shared/generated/governor/CascadeThresholds.ts")]
pub struct CascadeThresholds {
    // Step 1: speculation miss + queue depth + VRAM
    pub spec_miss_rate_advance: f32,    // > → advance to step 1
    pub spec_miss_rate_retreat: f32,    // < → retreat from step 1
    #[ts(type = "number")]
    pub inference_queue_depth_advance: u32, // > → advance
    #[ts(type = "number")]
    pub inference_queue_depth_retreat: u32, // < → retreat
    #[ts(type = "number")]
    pub vram_used_pct_advance: u8, // > → advance
    #[ts(type = "number")]
    pub vram_used_pct_retreat: u8, // < → retreat

    // Step 2: system memory + thermal
    #[ts(type = "number")]
    pub system_mem_used_pct_advance: u8,
    #[ts(type = "number")]
    pub system_mem_used_pct_retreat: u8,
    /// Thermal severity at or above which step 2 enters. Step 2's
    /// other enter conditions are step 1 sustained + mem high.
    pub thermal_advance: ThermalSeverity,

    // Step 3: battery + thermal critical
    #[ts(type = "number")]
    pub battery_pct_advance: u8, // < → advance to step 3
    #[ts(type = "number")]
    pub battery_pct_retreat: u8, // > → retreat
    /// Battery percentage that triggers EmergencyAdvanceToMax. Below
    /// this, the cascade jumps straight to MAX regardless of current
    /// step. Default 10% per spec.
    #[ts(type = "number")]
    pub battery_pct_emergency: u8,
}

impl Default for CascadeThresholds {
    fn default() -> Self {
        Self {
            // Step 1 — spec table
            spec_miss_rate_advance: 0.5,
            spec_miss_rate_retreat: 0.3,
            inference_queue_depth_advance: 16,
            inference_queue_depth_retreat: 8,
            vram_used_pct_advance: 85,
            vram_used_pct_retreat: 70,

            // Step 2 — spec table
            system_mem_used_pct_advance: 85,
            system_mem_used_pct_retreat: 70,
            thermal_advance: ThermalSeverity::Hot,

            // Step 3 — spec table
            battery_pct_advance: 15,
            battery_pct_retreat: 25,
            battery_pct_emergency: 10,
        }
    }
}

/// Evaluate the next cascade action given the current step + incoming
/// signal + thresholds. Pure function — no I/O, no time, no globals.
///
/// PR-3c2 will add a `time_in_step_ms` parameter to enforce the
/// "step N must be active > 30s before advancing to step N+1" rule.
/// PR-3c1 evaluates the immediate-trigger conditions (signal exceeds
/// threshold) + leaves the time-based gate for the wiring layer.
///
/// Returns:
/// - `EmergencyAdvanceToMax` for thermal Critical OR battery < emergency_pct
/// - `Advance` if the signal exceeds the advance threshold for the current step
/// - `Retreat` if the signal is below the retreat threshold (sustained-calm
///   logic lands in PR-3c2 via time_in_step)
/// - `Hold` otherwise
pub fn evaluate_next_step(
    current_step: u8,
    signal: &PressureSignal,
    thresholds: &CascadeThresholds,
) -> CascadeAction {
    // Emergency: thermal Critical OR battery below emergency floor.
    // Skips intermediate steps; protects hardware/user.
    if let PressureSignal::Thermal {
        severity: ThermalSeverity::Critical,
    } = signal
    {
        return CascadeAction::EmergencyAdvanceToMax;
    }
    if let PressureSignal::BatteryLow { remaining_pct } = signal {
        if *remaining_pct < thresholds.battery_pct_emergency {
            return CascadeAction::EmergencyAdvanceToMax;
        }
    }

    // Per-step evaluation: each signal kind contributes to specific
    // steps' enter/exit thresholds.
    match (current_step, signal) {
        // Step 0 (normal) — only advance triggers fire.
        (0, PressureSignal::SpeculationMissRate { rate }) => {
            if *rate > thresholds.spec_miss_rate_advance {
                CascadeAction::Advance
            } else {
                CascadeAction::Hold
            }
        }
        (0, PressureSignal::InferenceQueueDepth { depth }) => {
            if *depth > thresholds.inference_queue_depth_advance {
                CascadeAction::Advance
            } else {
                CascadeAction::Hold
            }
        }
        (0, PressureSignal::VRAMHigh { used_pct }) => {
            if *used_pct > thresholds.vram_used_pct_advance {
                CascadeAction::Advance
            } else {
                CascadeAction::Hold
            }
        }

        // Step 1 — speculation throttled. Advance triggers from
        // mem/thermal; retreat triggers from sustained-low signals.
        (1, PressureSignal::SystemMemHigh { used_pct }) => {
            if *used_pct > thresholds.system_mem_used_pct_advance {
                CascadeAction::Advance
            } else {
                CascadeAction::Hold
            }
        }
        (1, PressureSignal::Thermal { severity }) => {
            if *severity >= thresholds.thermal_advance {
                CascadeAction::Advance
            } else if *severity <= ThermalSeverity::Warm {
                // Cooling — may retreat IF other step-1 conditions also clear
                // (PR-3c2 enforces the all-clear retreat rule via state)
                CascadeAction::Retreat
            } else {
                CascadeAction::Hold
            }
        }
        (1, PressureSignal::SpeculationMissRate { rate }) => {
            // Sustained low miss rate → retreat. PR-3c2 enforces sustained-time.
            if *rate < thresholds.spec_miss_rate_retreat {
                CascadeAction::Retreat
            } else {
                CascadeAction::Hold
            }
        }
        (1, PressureSignal::InferenceQueueDepth { depth }) => {
            if *depth < thresholds.inference_queue_depth_retreat {
                CascadeAction::Retreat
            } else {
                CascadeAction::Hold
            }
        }
        (1, PressureSignal::VRAMHigh { used_pct }) => {
            if *used_pct < thresholds.vram_used_pct_retreat {
                CascadeAction::Retreat
            } else {
                CascadeAction::Hold
            }
        }

        // Step 2 — personas + non-realtime deferred. Advance from
        // battery low or sustained step-2 pressure; retreat on mem
        // clear + thermal clear.
        (2, PressureSignal::BatteryLow { remaining_pct }) => {
            if *remaining_pct < thresholds.battery_pct_advance {
                CascadeAction::Advance
            } else {
                CascadeAction::Hold
            }
        }
        (2, PressureSignal::SystemMemHigh { used_pct }) => {
            if *used_pct < thresholds.system_mem_used_pct_retreat {
                CascadeAction::Retreat
            } else {
                CascadeAction::Hold
            }
        }
        (2, PressureSignal::Thermal { severity }) => {
            if *severity <= ThermalSeverity::Warm {
                CascadeAction::Retreat
            } else {
                CascadeAction::Hold
            }
        }

        // Step 3 — working-set L1/L2 shrunk + spill. Retreat from
        // battery recovery + thermal clear.
        (3, PressureSignal::BatteryLow { remaining_pct }) => {
            if *remaining_pct > thresholds.battery_pct_retreat {
                CascadeAction::Retreat
            } else {
                CascadeAction::Hold
            }
        }
        (3, PressureSignal::Thermal { severity }) => {
            if *severity <= ThermalSeverity::Warm {
                CascadeAction::Retreat
            } else {
                CascadeAction::Hold
            }
        }

        // Step 4 — federation pull slowed. Retreat when step 3 clears.
        (4, PressureSignal::BatteryLow { remaining_pct }) => {
            if *remaining_pct > thresholds.battery_pct_retreat {
                CascadeAction::Retreat
            } else {
                CascadeAction::Hold
            }
        }
        (4, PressureSignal::Thermal { severity }) => {
            if *severity <= ThermalSeverity::Warm {
                CascadeAction::Retreat
            } else {
                CascadeAction::Hold
            }
        }

        // Step 5 — consolidation suspended. Retreat on any major
        // clear. PR-3c2 enforces the AND-all-clear rule via state.
        (5, PressureSignal::Thermal { severity }) => {
            if *severity == ThermalSeverity::Cool {
                CascadeAction::Retreat
            } else {
                CascadeAction::Hold
            }
        }
        (5, PressureSignal::BatteryLow { remaining_pct }) => {
            if *remaining_pct > thresholds.battery_pct_retreat {
                CascadeAction::Retreat
            } else {
                CascadeAction::Hold
            }
        }

        // UserActive is informational only — doesn't drive cascade
        // step changes directly. PR-3c2 may use it to weight retreat
        // (favor responsiveness when user is foreground), but for
        // PR-3c1 it's a Hold.
        (_, PressureSignal::UserActive { .. }) => CascadeAction::Hold,

        // Catch-all: any signal/step combo not explicitly handled is
        // Hold. Future cascade-step + signal combos that need
        // explicit handling get tests + match arms added; the default
        // is "do nothing" rather than "panic."
        _ => CascadeAction::Hold,
    }
}

/// Apply a CascadeAction to a current step value, returning the new
/// step (bounded to [CASCADE_STEP_MIN, CASCADE_STEP_MAX]).
///
/// Pure function — separated from `evaluate_next_step` so PR-3c2 can
/// log the (action, old_step, new_step) tuple for telemetry without
/// the evaluator caring.
pub fn apply_action(current_step: u8, action: CascadeAction) -> u8 {
    match action {
        CascadeAction::Hold => current_step,
        CascadeAction::Advance => (current_step + 1).min(CASCADE_STEP_MAX),
        CascadeAction::Retreat => current_step.saturating_sub(1),
        CascadeAction::EmergencyAdvanceToMax => CASCADE_STEP_MAX,
    }
}

// ─── apply_cascade_step_to_policy (PR-3c3) ──────────────────────────

/// Maximum federation pull cadence in seconds. Step 4 advance drops
/// the cadence to this value, slowing federation pulls to once-per-hour
/// when the substrate is under sustained pressure. Per spec.
pub const MAX_FEDERATION_PULL_CADENCE_SECONDS: u32 = 3600;

/// Apply the per-step throttling transformations to a `GovernorPolicy`
/// to produce the next policy. Pure function — same `(base, step)`
/// always produces the same result.
///
/// Per spec §"Adjustment Cascade" table:
///
/// - Step 0: unchanged (normal operation)
/// - Step 1: drop `speculation_aggressiveness` by one notch (toward Off)
/// - Step 2: also `concurrency_caps.personas_concurrent -= 1` (min 1)
///   AND defer non-realtime (sets `cadence_multipliers.delayed` and
///   `.background` to max(current, 2.0))
/// - Step 3: also shrink `tier_sizes.l1_lora_layers` and
///   `tier_sizes.l1_kv_tokens` by 25% (rounded down; min 1)
/// - Step 4: also `federation_pull_cadence.pull_cadence_seconds =
///   MAX_FEDERATION_PULL_CADENCE_SECONDS`
/// - Step 5: also `consolidation_schedule = Manual` (operator must
///   explicitly trigger consolidation; the substrate won't run it on
///   its own under maximum pressure)
///
/// Transformations are CUMULATIVE — step 3 includes step 2's
/// transformations plus step 1's. Apply-at-step-N = apply [step 1, ...
/// step N] to base. Caller passes the BASE policy (the policy as
/// loaded from the file, with cascade_step = 0) so the transformations
/// always start from the same canonical state.
///
/// `policy.cascade_step` is set to the supplied `step` parameter.
/// Other fields (policy_version, hardware_class, committed_at_ms)
/// are passed through unchanged — caller is responsible for bumping
/// version + updating timestamp at publish time.
///
/// ## Anti-oscillation: restore-speculation-one-step-later
///
/// Spec rule per §"Adjustment Cascade": when retreating, restore
/// speculation ONE STEP LATER than the rest of the policy. This
/// function is symmetric (applying step 0 == base policy), so the
/// "one step later" is the CALLER's responsibility: when retreating
/// from N → N-1, call this with `step = N-1` for everything EXCEPT
/// speculation, which uses `step = N` for one more cycle. That logic
/// lives in the wiring layer (PR-3c4 or `LocalSubstrateGovernor`
/// follow-up), not this pure transformation.
pub fn apply_cascade_step_to_policy(
    base: &crate::governor::types::GovernorPolicy,
    step: u8,
) -> crate::governor::types::GovernorPolicy {
    let mut policy = base.clone();
    policy.cascade_step = step.min(CASCADE_STEP_MAX);

    // Step 1+: speculation drop
    if step >= 1 {
        policy.speculation_aggressiveness = drop_speculation_level(base.speculation_aggressiveness);
    }

    // Step 2+: personas_concurrent -= 1, defer non-realtime
    if step >= 2 {
        policy.concurrency_caps.personas_concurrent =
            base.concurrency_caps.personas_concurrent.saturating_sub(1).max(1);
        // delayed + background cadence stretched (max with 2.0 so
        // already-stretched values aren't shrunk)
        policy.cadence_multipliers.delayed = base.cadence_multipliers.delayed.max(2.0);
        policy.cadence_multipliers.background = base.cadence_multipliers.background.max(2.0);
    }

    // Step 3+: shrink l1 by 25%
    if step >= 3 {
        policy.tier_sizes.l1_lora_layers =
            ((base.tier_sizes.l1_lora_layers as f32 * 0.75) as u32).max(1);
        policy.tier_sizes.l1_kv_tokens =
            ((base.tier_sizes.l1_kv_tokens as f32 * 0.75) as u32).max(1);
    }

    // Step 4+: federation cadence to max
    if step >= 4 {
        policy.federation_pull_cadence.pull_cadence_seconds = MAX_FEDERATION_PULL_CADENCE_SECONDS;
    }

    // Step 5: consolidation Manual
    if step >= 5 {
        policy.consolidation_schedule =
            crate::governor::types::ConsolidationSchedule::Manual;
    }

    policy
}

/// Drop the speculation level by one notch toward Off. Pure helper.
/// Off → Off (already minimum), Conservative → Off, Balanced →
/// Conservative, Aggressive → Balanced.
fn drop_speculation_level(
    level: crate::governor::types::SpeculationLevel,
) -> crate::governor::types::SpeculationLevel {
    use crate::governor::types::SpeculationLevel::*;
    match level {
        Off => Off,
        Conservative => Off,
        Balanced => Conservative,
        Aggressive => Balanced,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thresh() -> CascadeThresholds {
        CascadeThresholds::default()
    }

    // ===== Emergency: thermal Critical + battery <emergency =====

    /// What this catches: thermal Critical immediately jumps to MAX
    /// regardless of current step. Protects hardware from sustained
    /// thermal damage.
    #[test]
    fn thermal_critical_triggers_emergency_max() {
        for step in 0..=CASCADE_STEP_MAX {
            let action = evaluate_next_step(
                step,
                &PressureSignal::Thermal {
                    severity: ThermalSeverity::Critical,
                },
                &thresh(),
            );
            assert_eq!(
                action,
                CascadeAction::EmergencyAdvanceToMax,
                "step={step} should emergency-max on thermal Critical"
            );
        }
    }

    /// What this catches: battery below emergency_pct (default 10%)
    /// triggers EmergencyAdvanceToMax. Protects user from system
    /// shutdown mid-task.
    #[test]
    fn battery_below_emergency_triggers_emergency_max() {
        let action = evaluate_next_step(
            0,
            &PressureSignal::BatteryLow { remaining_pct: 9 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::EmergencyAdvanceToMax);
    }

    /// What this catches: battery exactly at emergency_pct (10%) does
    /// NOT trigger emergency (boundary — < emergency, not <=).
    #[test]
    fn battery_at_emergency_pct_boundary_does_not_emergency() {
        let action = evaluate_next_step(
            0,
            &PressureSignal::BatteryLow { remaining_pct: 10 },
            &thresh(),
        );
        assert_ne!(action, CascadeAction::EmergencyAdvanceToMax);
    }

    // ===== Step 0 → Step 1 (speculation + queue + VRAM) =====

    /// What this catches: speculation miss rate > 0.5 at step 0
    /// triggers Advance. Spec table row 1.
    #[test]
    fn spec_miss_high_at_step_0_advances() {
        let action = evaluate_next_step(
            0,
            &PressureSignal::SpeculationMissRate { rate: 0.6 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Advance);
    }

    /// What this catches: speculation miss = 0.5 exactly doesn't advance
    /// (strict > threshold). Boundary test.
    #[test]
    fn spec_miss_at_threshold_doesnt_advance() {
        let action = evaluate_next_step(
            0,
            &PressureSignal::SpeculationMissRate { rate: 0.5 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Hold);
    }

    /// What this catches: inference queue depth > 16 triggers Advance.
    #[test]
    fn inference_queue_high_at_step_0_advances() {
        let action = evaluate_next_step(
            0,
            &PressureSignal::InferenceQueueDepth { depth: 17 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Advance);
    }

    /// What this catches: VRAM > 85% triggers Advance.
    #[test]
    fn vram_high_at_step_0_advances() {
        let action = evaluate_next_step(
            0,
            &PressureSignal::VRAMHigh { used_pct: 90 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Advance);
    }

    /// What this catches: VRAM at 85% (exactly threshold) does NOT
    /// advance. Boundary.
    #[test]
    fn vram_at_threshold_doesnt_advance() {
        let action = evaluate_next_step(
            0,
            &PressureSignal::VRAMHigh { used_pct: 85 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Hold);
    }

    // ===== Step 1 → Step 0 (retreat) =====

    /// What this catches: speculation miss < 0.3 at step 1 triggers
    /// Retreat. Hysteresis: advance was at 0.5, retreat at 0.3 — gap
    /// prevents oscillation around a single threshold.
    #[test]
    fn spec_miss_low_at_step_1_retreats() {
        let action = evaluate_next_step(
            1,
            &PressureSignal::SpeculationMissRate { rate: 0.2 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Retreat);
    }

    /// What this catches: speculation miss between retreat (0.3) and
    /// advance (0.5) thresholds → Hold. The hysteresis gap.
    #[test]
    fn spec_miss_in_hysteresis_gap_holds() {
        for rate in &[0.31, 0.40, 0.49] {
            let action = evaluate_next_step(
                1,
                &PressureSignal::SpeculationMissRate { rate: *rate },
                &thresh(),
            );
            assert_eq!(action, CascadeAction::Hold, "rate {rate} should Hold in gap");
        }
    }

    /// What this catches: inference queue < 8 at step 1 retreats.
    #[test]
    fn inference_queue_low_at_step_1_retreats() {
        let action = evaluate_next_step(
            1,
            &PressureSignal::InferenceQueueDepth { depth: 5 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Retreat);
    }

    /// What this catches: VRAM < 70 at step 1 retreats.
    #[test]
    fn vram_low_at_step_1_retreats() {
        let action = evaluate_next_step(
            1,
            &PressureSignal::VRAMHigh { used_pct: 60 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Retreat);
    }

    // ===== Step 1 → Step 2 (advance on mem + thermal) =====

    /// What this catches: system mem > 85 at step 1 advances to step 2.
    /// Spec table row 2.
    #[test]
    fn system_mem_high_at_step_1_advances() {
        let action = evaluate_next_step(
            1,
            &PressureSignal::SystemMemHigh { used_pct: 90 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Advance);
    }

    /// What this catches: thermal Hot at step 1 advances to step 2.
    #[test]
    fn thermal_hot_at_step_1_advances() {
        let action = evaluate_next_step(
            1,
            &PressureSignal::Thermal {
                severity: ThermalSeverity::Hot,
            },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Advance);
    }

    /// What this catches: thermal Warm or Cool at step 1 → Retreat
    /// (cascade can step down when thermal clears).
    #[test]
    fn thermal_warm_at_step_1_retreats() {
        for severity in &[ThermalSeverity::Warm, ThermalSeverity::Cool] {
            let action = evaluate_next_step(
                1,
                &PressureSignal::Thermal {
                    severity: *severity,
                },
                &thresh(),
            );
            assert_eq!(action, CascadeAction::Retreat, "severity={severity:?} should retreat");
        }
    }

    // ===== Step 2 → Step 3 (advance on battery low) =====

    /// What this catches: battery < 15% at step 2 advances to step 3
    /// (NOT emergency — emergency is < 10%).
    #[test]
    fn battery_low_at_step_2_advances_not_emergency() {
        let action = evaluate_next_step(
            2,
            &PressureSignal::BatteryLow { remaining_pct: 12 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Advance);
    }

    /// What this catches: step 2 retreats on mem-clear.
    #[test]
    fn step_2_retreats_on_mem_clear() {
        let action = evaluate_next_step(
            2,
            &PressureSignal::SystemMemHigh { used_pct: 60 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Retreat);
    }

    // ===== Step 3, 4, 5 — battery + thermal retreat paths =====

    /// What this catches: battery > 25% at steps 3/4 retreats.
    #[test]
    fn battery_recovered_at_steps_3_and_4_retreats() {
        for step in &[3, 4] {
            let action = evaluate_next_step(
                *step,
                &PressureSignal::BatteryLow { remaining_pct: 30 },
                &thresh(),
            );
            assert_eq!(action, CascadeAction::Retreat, "step={step} should retreat");
        }
    }

    /// What this catches: at step 5 (max throttle), only Cool thermal
    /// retreats; Warm or Hot Holds. Strictest retreat condition.
    #[test]
    fn step_5_only_cool_thermal_retreats() {
        let cool = evaluate_next_step(
            5,
            &PressureSignal::Thermal {
                severity: ThermalSeverity::Cool,
            },
            &thresh(),
        );
        assert_eq!(cool, CascadeAction::Retreat);

        for non_cool in &[ThermalSeverity::Warm, ThermalSeverity::Hot] {
            let action = evaluate_next_step(
                5,
                &PressureSignal::Thermal {
                    severity: *non_cool,
                },
                &thresh(),
            );
            assert_eq!(action, CascadeAction::Hold, "severity={non_cool:?} at max step holds");
        }
    }

    // ===== UserActive informational only =====

    /// What this catches: UserActive doesn't drive cascade transitions
    /// in PR-3c1 (signal exists for PR-3c2's user-foreground weighting
    /// but doesn't fire enter/exit).
    #[test]
    fn user_active_holds_at_every_step() {
        for step in 0..=CASCADE_STEP_MAX {
            for foreground in [true, false] {
                let action = evaluate_next_step(
                    step,
                    &PressureSignal::UserActive { foreground },
                    &thresh(),
                );
                assert_eq!(
                    action,
                    CascadeAction::Hold,
                    "step={step} foreground={foreground} should Hold"
                );
            }
        }
    }

    // ===== apply_action =====

    /// What this catches: Hold doesn't move the step.
    #[test]
    fn apply_hold_keeps_step() {
        for step in 0..=CASCADE_STEP_MAX {
            assert_eq!(apply_action(step, CascadeAction::Hold), step);
        }
    }

    /// What this catches: Advance bumps by 1, capped at MAX.
    #[test]
    fn apply_advance_bumps_one_capped_at_max() {
        assert_eq!(apply_action(0, CascadeAction::Advance), 1);
        assert_eq!(apply_action(3, CascadeAction::Advance), 4);
        assert_eq!(apply_action(CASCADE_STEP_MAX, CascadeAction::Advance), CASCADE_STEP_MAX);
    }

    /// What this catches: Retreat drops by 1, saturated at MIN.
    #[test]
    fn apply_retreat_drops_one_saturated_at_min() {
        assert_eq!(apply_action(5, CascadeAction::Retreat), 4);
        assert_eq!(apply_action(1, CascadeAction::Retreat), 0);
        assert_eq!(apply_action(0, CascadeAction::Retreat), 0);
    }

    /// What this catches: EmergencyAdvanceToMax jumps from any step
    /// to MAX in one operation.
    #[test]
    fn apply_emergency_advances_to_max_from_any_step() {
        for step in 0..=CASCADE_STEP_MAX {
            assert_eq!(
                apply_action(step, CascadeAction::EmergencyAdvanceToMax),
                CASCADE_STEP_MAX,
                "step={step} should jump to MAX"
            );
        }
    }

    // ===== Determinism + serde =====

    /// What this catches: pure-function determinism. Same inputs →
    /// same output. PR-3c2 can rely on this for the wire-replay path.
    #[test]
    fn evaluate_is_deterministic() {
        let signal = PressureSignal::SpeculationMissRate { rate: 0.7 };
        let a = evaluate_next_step(0, &signal, &thresh());
        let b = evaluate_next_step(0, &signal, &thresh());
        assert_eq!(a, b);
    }

    /// What this catches: CascadeAction tagged-union round-trips with
    /// `kind` discriminator. PR-3c2 emits these via the trace bus +
    /// the wire shape must round-trip cleanly for replay/inspection.
    #[test]
    fn cascade_action_tagged_union_round_trips() {
        let actions = vec![
            CascadeAction::Hold,
            CascadeAction::Advance,
            CascadeAction::Retreat,
            CascadeAction::EmergencyAdvanceToMax,
        ];
        for a in &actions {
            let j = serde_json::to_string(a).unwrap();
            let back: CascadeAction = serde_json::from_str(&j).unwrap();
            assert_eq!(*a, back);
            assert!(j.contains("\"kind\":\""), "tag missing: {j}");
        }
    }

    /// What this catches: CascadeThresholds default values match the
    /// spec's §"Adjustment Cascade" table. If anyone tunes defaults
    /// without updating the spec, this test catches the drift.
    #[test]
    fn cascade_thresholds_defaults_match_spec_table() {
        let t = CascadeThresholds::default();
        // Spec row 1
        assert_eq!(t.spec_miss_rate_advance, 0.5);
        assert_eq!(t.spec_miss_rate_retreat, 0.3);
        assert_eq!(t.vram_used_pct_advance, 85);
        assert_eq!(t.vram_used_pct_retreat, 70);
        // Spec row 2
        assert_eq!(t.system_mem_used_pct_advance, 85);
        assert_eq!(t.system_mem_used_pct_retreat, 70);
        assert_eq!(t.thermal_advance, ThermalSeverity::Hot);
        // Spec row 3
        assert_eq!(t.battery_pct_advance, 15);
        assert_eq!(t.battery_pct_retreat, 25);
        assert_eq!(t.battery_pct_emergency, 10);
    }

    /// What this catches: emergency signals beat all other path
    /// evaluations. Even at step 0, thermal Critical jumps to MAX —
    /// no "first match wins" with a quieter step-0 path.
    #[test]
    fn emergency_signals_priority_over_step_evaluation() {
        let action = evaluate_next_step(
            0,
            &PressureSignal::Thermal {
                severity: ThermalSeverity::Critical,
            },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::EmergencyAdvanceToMax);
    }

    // ===== apply_cascade_step_to_policy (PR-3c3) =====

    use crate::governor::types::{
        CadenceMultipliers, ConcurrencyCaps, ConsolidationSchedule, FederationCadence,
        GovernorPolicy, HardwareClass, PowerSource, RecallScoreWeights, SpeculationLevel,
        TargetSilicon, ThermalClass, TierSizes,
    };

    fn base_policy_5090() -> GovernorPolicy {
        // Approximation of the spec's 5090 anchor policy. Used as the
        // canonical base for cascade-step tests.
        GovernorPolicy {
            policy_version: 1,
            hardware_class: HardwareClass {
                silicon: TargetSilicon::NvidiaCuda,
                silicon_model: "RTX 5090".into(),
                vram_mb: 32 * 1024,
                system_ram_mb: 64 * 1024,
                power_source: PowerSource::Plugged,
                thermal_class: ThermalClass::Workstation,
                battery_pct: None,
                thermal_headroom_pct: None,
            },
            tier_sizes: TierSizes {
                l1_lora_layers: 8,
                l1_kv_tokens: 16384,
                l2_lora_layers: 16,
                l3_lora_layers: 40,
                l3_engrams: 10240,
            },
            cadence_multipliers: CadenceMultipliers {
                realtime: 1.0,
                delayed: 1.0,
                background: 1.5,
            },
            concurrency_caps: ConcurrencyCaps {
                personas_concurrent: 8,
                inference_lanes: 4,
                foundry_lanes: 1,
                sentinel_lanes: 2,
            },
            speculation_aggressiveness: SpeculationLevel::Aggressive,
            consolidation_schedule: ConsolidationSchedule::Idle,
            federation_pull_cadence: FederationCadence {
                pull_cadence_seconds: 60,
            },
            recall_score_weights: RecallScoreWeights {
                semantic: 0.4,
                outcome_history: 0.3,
                recency: 0.1,
                tier_proximity: 0.1,
                provenance_trust: 0.1,
            },
            cascade_step: 0,
            committed_at_ms: 1000,
        }
    }

    /// What this catches: step 0 == base (cascade unchanged, no
    /// throttling applied). Identity case — pinning that the function
    /// doesn't accidentally modify the base policy when step=0.
    #[test]
    fn apply_step_0_equals_base_except_cascade_step() {
        let base = base_policy_5090();
        let after = apply_cascade_step_to_policy(&base, 0);
        assert_eq!(after.cascade_step, 0);
        assert_eq!(after.speculation_aggressiveness, base.speculation_aggressiveness);
        assert_eq!(
            after.concurrency_caps.personas_concurrent,
            base.concurrency_caps.personas_concurrent
        );
        assert_eq!(after.tier_sizes.l1_lora_layers, base.tier_sizes.l1_lora_layers);
        assert_eq!(after.consolidation_schedule, base.consolidation_schedule);
    }

    /// What this catches: step 1 drops speculation by one notch.
    /// Aggressive → Balanced (then Balanced → Conservative, Conservative
    /// → Off via separate base policies in the next test).
    #[test]
    fn apply_step_1_drops_speculation_aggressive_to_balanced() {
        let base = base_policy_5090();
        assert_eq!(base.speculation_aggressiveness, SpeculationLevel::Aggressive);
        let after = apply_cascade_step_to_policy(&base, 1);
        assert_eq!(after.cascade_step, 1);
        assert_eq!(after.speculation_aggressiveness, SpeculationLevel::Balanced);
    }

    /// What this catches: speculation drop ladder covers every variant.
    /// Aggressive→Balanced, Balanced→Conservative, Conservative→Off,
    /// Off→Off (already minimum).
    #[test]
    fn apply_step_1_speculation_drops_one_notch_per_variant() {
        for (input, expected) in &[
            (SpeculationLevel::Aggressive, SpeculationLevel::Balanced),
            (SpeculationLevel::Balanced, SpeculationLevel::Conservative),
            (SpeculationLevel::Conservative, SpeculationLevel::Off),
            (SpeculationLevel::Off, SpeculationLevel::Off),
        ] {
            let mut base = base_policy_5090();
            base.speculation_aggressiveness = *input;
            let after = apply_cascade_step_to_policy(&base, 1);
            assert_eq!(
                after.speculation_aggressiveness, *expected,
                "from {input:?} should drop to {expected:?}"
            );
        }
    }

    /// What this catches: step 2 personas_concurrent decreases by 1
    /// (5090 has 8 → step 2 = 7). Cumulative with step 1's speculation
    /// drop.
    #[test]
    fn apply_step_2_drops_personas_concurrent_and_keeps_speculation_drop() {
        let base = base_policy_5090();
        let after = apply_cascade_step_to_policy(&base, 2);
        assert_eq!(after.cascade_step, 2);
        assert_eq!(after.concurrency_caps.personas_concurrent, 7); // 8 - 1
        // Cumulative: step 1's speculation drop still applies
        assert_eq!(after.speculation_aggressiveness, SpeculationLevel::Balanced);
    }

    /// What this catches: step 2 personas_concurrent floor at 1.
    /// Defensive — a base with 1 persona shouldn't go to 0 (kills the
    /// inference pool entirely).
    #[test]
    fn apply_step_2_personas_concurrent_floor_at_one() {
        let mut base = base_policy_5090();
        base.concurrency_caps.personas_concurrent = 1;
        let after = apply_cascade_step_to_policy(&base, 2);
        assert_eq!(after.concurrency_caps.personas_concurrent, 1);
    }

    /// What this catches: step 2 stretches non-realtime cadence
    /// multipliers to at least 2.0. Realtime stays unchanged.
    #[test]
    fn apply_step_2_stretches_non_realtime_cadence() {
        let base = base_policy_5090();
        let after = apply_cascade_step_to_policy(&base, 2);
        assert_eq!(after.cadence_multipliers.realtime, base.cadence_multipliers.realtime);
        assert!(after.cadence_multipliers.delayed >= 2.0);
        assert!(after.cadence_multipliers.background >= 2.0);
    }

    /// What this catches: step 2 doesn't SHRINK already-stretched
    /// cadence multipliers. If base already has background = 3.0, step
    /// 2 keeps 3.0 (uses max).
    #[test]
    fn apply_step_2_doesnt_shrink_already_stretched_cadence() {
        let mut base = base_policy_5090();
        base.cadence_multipliers.background = 3.0;
        let after = apply_cascade_step_to_policy(&base, 2);
        assert_eq!(after.cadence_multipliers.background, 3.0);
    }

    /// What this catches: step 3 shrinks l1_lora_layers + l1_kv_tokens
    /// by ~25%. 8 * 0.75 = 6. 16384 * 0.75 = 12288.
    #[test]
    fn apply_step_3_shrinks_l1_25_percent() {
        let base = base_policy_5090();
        let after = apply_cascade_step_to_policy(&base, 3);
        assert_eq!(after.cascade_step, 3);
        assert_eq!(after.tier_sizes.l1_lora_layers, 6); // 8 * 0.75
        assert_eq!(after.tier_sizes.l1_kv_tokens, 12288); // 16384 * 0.75
        // L2/L3 untouched at step 3
        assert_eq!(after.tier_sizes.l2_lora_layers, base.tier_sizes.l2_lora_layers);
    }

    /// What this catches: l1 floor at 1 when base is already small.
    /// 1 * 0.75 = 0.75 → floor 0 → max(0, 1) = 1.
    #[test]
    fn apply_step_3_l1_floors_at_one() {
        let mut base = base_policy_5090();
        base.tier_sizes.l1_lora_layers = 1;
        base.tier_sizes.l1_kv_tokens = 1;
        let after = apply_cascade_step_to_policy(&base, 3);
        assert_eq!(after.tier_sizes.l1_lora_layers, 1);
        assert_eq!(after.tier_sizes.l1_kv_tokens, 1);
    }

    /// What this catches: step 4 federation cadence = max
    /// (MAX_FEDERATION_PULL_CADENCE_SECONDS). Slows pulls to once-
    /// per-hour under sustained pressure.
    #[test]
    fn apply_step_4_maxes_federation_cadence() {
        let base = base_policy_5090();
        assert_eq!(base.federation_pull_cadence.pull_cadence_seconds, 60);
        let after = apply_cascade_step_to_policy(&base, 4);
        assert_eq!(after.cascade_step, 4);
        assert_eq!(
            after.federation_pull_cadence.pull_cadence_seconds,
            MAX_FEDERATION_PULL_CADENCE_SECONDS
        );
    }

    /// What this catches: step 5 consolidation = Manual. Suspends
    /// automatic consolidation under maximum pressure (operator must
    /// explicitly trigger; substrate stops doing it on its own).
    #[test]
    fn apply_step_5_consolidation_manual() {
        let base = base_policy_5090();
        assert_eq!(base.consolidation_schedule, ConsolidationSchedule::Idle);
        let after = apply_cascade_step_to_policy(&base, 5);
        assert_eq!(after.cascade_step, 5);
        assert_eq!(after.consolidation_schedule, ConsolidationSchedule::Manual);
    }

    /// What this catches: step 5 is CUMULATIVE — all prior step
    /// transformations also applied. Speculation dropped + personas
    /// reduced + tier_sizes shrunk + federation maxed + consolidation
    /// Manual. The full-throttle state.
    #[test]
    fn apply_step_5_cumulative_all_transformations() {
        let base = base_policy_5090();
        let after = apply_cascade_step_to_policy(&base, 5);
        // Step 1
        assert_eq!(after.speculation_aggressiveness, SpeculationLevel::Balanced);
        // Step 2
        assert_eq!(after.concurrency_caps.personas_concurrent, 7);
        assert!(after.cadence_multipliers.delayed >= 2.0);
        // Step 3
        assert_eq!(after.tier_sizes.l1_lora_layers, 6);
        // Step 4
        assert_eq!(
            after.federation_pull_cadence.pull_cadence_seconds,
            MAX_FEDERATION_PULL_CADENCE_SECONDS
        );
        // Step 5
        assert_eq!(after.consolidation_schedule, ConsolidationSchedule::Manual);
    }

    /// What this catches: step value > MAX is clamped to MAX. Defensive
    /// against caller bugs (passes 7 instead of 5).
    #[test]
    fn apply_step_above_max_clamps_to_max() {
        let base = base_policy_5090();
        let after = apply_cascade_step_to_policy(&base, 99);
        assert_eq!(after.cascade_step, CASCADE_STEP_MAX);
        // Should have all step-5 transformations
        assert_eq!(after.consolidation_schedule, ConsolidationSchedule::Manual);
    }

    /// What this catches: pure-function determinism. Same inputs →
    /// same output. Tests pin this so the caller can cache the
    /// transformation result if the (base_policy, step) tuple is stable.
    #[test]
    fn apply_cascade_step_is_deterministic() {
        let base = base_policy_5090();
        let a = apply_cascade_step_to_policy(&base, 3);
        let b = apply_cascade_step_to_policy(&base, 3);
        assert_eq!(a, b);
    }

    /// What this catches: applying step N then step 0 to the result
    /// does NOT restore base — the step transformations are NOT
    /// reversible from a transformed policy. Caller MUST keep the
    /// original base + re-apply step 0 from it (which is what the
    /// LocalSubstrateGovernor does — stores base separately from
    /// active).
    #[test]
    fn apply_cascade_step_not_reversible_via_step_0_on_transformed() {
        let base = base_policy_5090();
        let throttled = apply_cascade_step_to_policy(&base, 3);
        let reset_attempt = apply_cascade_step_to_policy(&throttled, 0);
        // step is 0 again
        assert_eq!(reset_attempt.cascade_step, 0);
        // But tier_sizes is STILL shrunk (step 0 doesn't undo step 3's
        // shrink — it just doesn't re-apply it from a now-shrunk base).
        assert_eq!(
            reset_attempt.tier_sizes.l1_lora_layers,
            throttled.tier_sizes.l1_lora_layers,
            "step 0 from transformed policy ≠ base; caller MUST hold base separately"
        );
    }

    /// What this catches: MAX_FEDERATION_PULL_CADENCE_SECONDS const
    /// is the spec's max-cadence value. Drift catcher — if someone
    /// tunes this without updating the spec, test fails.
    #[test]
    fn max_federation_cadence_const_pinned() {
        assert_eq!(MAX_FEDERATION_PULL_CADENCE_SECONDS, 3600);
    }
}
