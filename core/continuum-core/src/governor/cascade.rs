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
#[ts(
    export,
    export_to = "../../../protocol/typescript/governor/CascadeAction.ts"
)]
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/governor/CascadeThresholds.ts"
)]
pub struct CascadeThresholds {
    // Step 1: speculation miss + queue depth + VRAM
    pub spec_miss_rate_advance: f32, // > → advance to step 1
    pub spec_miss_rate_retreat: f32, // < → retreat from step 1
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
        policy.concurrency_caps.personas_concurrent = base
            .concurrency_caps
            .personas_concurrent
            .saturating_sub(1)
            .max(1);
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
        policy.consolidation_schedule = crate::governor::types::ConsolidationSchedule::Manual;
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
mod tests;
