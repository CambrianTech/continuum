//! `cognition/set-sleep-mode` — put a persona into a voluntary attention mode (typed,
//! dep-holding).
//!
//! Sets the persona's [`SleepState`](crate::persona::evaluator::SleepState) — the mode
//! that drives Gate 4 of `full_evaluate` (active / mentioned-only / human-only / sleeping /
//! until-topic), with an optional auto-wake after `duration_minutes`. Captures the owning
//! module's [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use uuid::Uuid;

use crate::modules::cognition::CognitionState;
use crate::persona::evaluator::{SleepMode, SleepState};
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/SetSleepModeParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct SetSleepModeParams {
    /// Persona whose attention mode is set.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// The voluntary sleep mode to enter.
    pub mode: SleepMode,
    /// Human-readable reason for the mode change.
    #[serde(default)]
    pub reason: String,
    /// Optional auto-wake after this many minutes.
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub duration_minutes: Option<f64>,
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/SetSleepModeResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct SetSleepModeResult {
    pub set: bool,
    pub previous_mode: SleepMode,
    pub new_mode: SleepMode,
    #[ts(optional, type = "number")]
    pub wake_at_ms: Option<u64>,
}

crate::action_command! {
    /// Put the persona into a voluntary attention mode (active / mentioned-only /
    /// human-only / sleeping / until-topic), optionally auto-waking after a duration.
    /// Host-invoked.
    pub struct SetSleepMode { state: Arc<CognitionState> }
    name: "cognition/set-sleep-mode",
    access: Internal,
    params: SetSleepModeParams,
    output: SetSleepModeResult,
    run(this, _ctx, p) => {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| CommandError::Internal(format!("system clock before UNIX epoch: {e}")))?
            .as_millis() as u64;

        let wake_at_ms = p.duration_minutes.map(|d| now_ms + (d * 60_000.0) as u64);

        let mut persona = this.state.get_or_create_persona(p.persona_id);
        let previous_mode = persona.sleep_state.mode;

        persona.sleep_state = SleepState {
            mode: p.mode,
            reason: p.reason.clone(),
            set_at_ms: now_ms,
            wake_at_ms,
        };

        crate::log_info!(
            "module",
            "cognition",
            "set-sleep-mode {}: {:?} → {:?} (reason: {})",
            p.persona_id,
            previous_mode,
            p.mode,
            p.reason
        );

        Ok(SetSleepModeResult {
            set: true,
            previous_mode,
            new_mode: p.mode,
            wake_at_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. set-sleep-mode is host-driven
    // cognition IPC, so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(SetSleepMode::NAME, "cognition/set-sleep-mode");
        assert_eq!(SetSleepMode::ACCESS, AccessLevel::Internal);
    }
}
