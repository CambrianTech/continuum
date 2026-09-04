//! `system/launch-mode/set` — persist a new launch preference + emit the change.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::launch_mode::{normalize_mode, LaunchModeState, LAUNCH_MODE_KEY};
use crate::sdk_codegen::CommandError;

/// Params for `system/launch-mode/set` — the mode to persist.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/LaunchModeSetParams.ts"
)]
pub struct LaunchModeSetParams {
    /// One of `headless`, `ui`, `auto`. Anything else is rejected (deny-by-default).
    pub mode: String,
}

/// What `set` applied — the new mode and the one it replaced.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/LaunchModeSetResult.ts"
)]
pub struct LaunchModeSetResult {
    /// The canonical mode now stored in config.env.
    pub mode: String,
    /// The previous mode (empty if it was unset).
    pub previous_mode: String,
    /// Always true on success — the write + change-event fired.
    pub applied: bool,
}

crate::action_command! {
    /// Persist the launch mode (`headless`, `ui`, or `auto`) to config.env and emit
    /// a change event so a running UI can attach or tear down its overlay. Privileged
    /// — it changes how the host launches, not a per-turn action.
    pub struct SystemLaunchModeSet { state: Arc<LaunchModeState> }
    name: "system/launch-mode/set",
    access: Privileged,
    params: LaunchModeSetParams,
    output: LaunchModeSetResult,
    run(this, _ctx, p) => {
        // Deny-by-default: a bad value never reaches config.env.
        let mode = normalize_mode(&p.mode).ok_or_else(|| {
            CommandError::Invalid(format!(
                "invalid mode '{}'. Expected one of: headless, ui, auto",
                p.mode
            ))
        })?;

        let previous = crate::config_env::read(LAUNCH_MODE_KEY)
            .and_then(|raw| normalize_mode(&raw).map(str::to_string))
            .unwrap_or_default();

        crate::config_env::upsert(LAUNCH_MODE_KEY, mode).map_err(CommandError::Internal)?;

        // Fire-and-forget: a running UI tears down/attaches its overlay.
        this.state.publish_changed(mode, &previous);

        Ok(LaunchModeSetResult {
            mode: mode.to_string(),
            previous_mode: previous,
            applied: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: `set` rejects any value outside the three modes BEFORE it
    // touches config.env — the deny-by-default guard the write path leans on so a
    // bad mode is never persisted. Rejection happens pre-I/O, so no env dependency.
    #[tokio::test]
    async fn invalid_mode_is_rejected_before_write() {
        let cmd = SystemLaunchModeSet {
            state: Arc::new(LaunchModeState::new()),
        };
        let err = cmd
            .run(
                &Ctx::default(),
                LaunchModeSetParams {
                    mode: "sideways".into(),
                },
            )
            .await
            .expect_err("invalid mode must error");
        match err {
            CommandError::Invalid(msg) => {
                assert!(
                    msg.contains("headless"),
                    "error should name valid modes: {msg}"
                )
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }
}
