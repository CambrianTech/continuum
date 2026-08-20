//! `system/launch-mode/get` — report the stored launch preference.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::launch_mode::{normalize_mode, LAUNCH_MODE_KEY};

/// The resolved launch mode plus WHERE it came from — so a caller can tell a
/// deliberately-set preference from the unset default.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/LaunchModeGetResult.ts"
)]
pub struct LaunchModeGetResult {
    /// `headless` | `ui` | `auto` — the canonical stored mode (unset ⇒ `auto`).
    pub mode: String,
    /// `config` when read from config.env, `default` when unset/unrecognized.
    pub source: String,
}

crate::action_command! {
    /// Report the stored launch mode (`headless`, `ui`, or `auto`) and whether it
    /// came from config.env or is the unset default. Read-only.
    pub struct SystemLaunchModeGet;
    name: "system/launch-mode/get",
    access: AiSafe,
    params: crate::commands::system::SystemQuery,
    output: LaunchModeGetResult,
    run(_this, _ctx, _p) => {
        // Unset (or unrecognized) ⇒ "auto" — the resolution into a concrete
        // headless/ui is the shell's boot-time job; this reports the stored setting.
        let (mode, source) = match crate::config_env::read(LAUNCH_MODE_KEY) {
            Some(raw) => match normalize_mode(&raw) {
                Some(m) => (m.to_string(), "config"),
                None => ("auto".to_string(), "default"),
            },
            None => ("auto".to_string(), "default"),
        };
        Ok(LaunchModeGetResult { mode, source: source.to_string() })
    }
}
