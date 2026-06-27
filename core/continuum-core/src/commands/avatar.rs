//! `avatar/snapshot` — render a persona's 3D avatar to a profile PNG.
//!
//! Stateless: the Bevy-render domain logic lives on
//! [`crate::modules::avatar::AvatarModule`] (shared with its tick-driven
//! auto-refresh); this command orchestrates the on-disk cache check and runs the
//! blocking capture off the async thread.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Params for `avatar/snapshot`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/avatar/AvatarSnapshotParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct AvatarSnapshotParams {
    /// Persona identity (uniqueId) to render — selects the VRM model and the
    /// output filename (`~/.continuum/avatars/<identity>.png`).
    pub identity: String,
    /// Render width in pixels. Defaults to 480.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub width: Option<u32>,
    /// Render height in pixels. Defaults to 480.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub height: Option<u32>,
    /// Re-render even if a snapshot already exists on disk.
    #[serde(default)]
    pub force: bool,
}

/// Result of `avatar/snapshot`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/avatar/AvatarSnapshotResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct AvatarSnapshotResult {
    /// HTTP path the rendered PNG is served at (`/avatars/<identity>.png`).
    pub path: String,
    /// `true` when an existing on-disk snapshot was returned without re-rendering.
    pub cached: bool,
}

crate::action_command! {
    /// Render a persona's 3D avatar to a profile PNG. Allocates a temporary Bevy
    /// render slot, loads the persona's VRM model, waits for a clean frame, and
    /// saves it under `~/.continuum/avatars/<identity>.png` (served at
    /// `/avatars/<identity>.png`). Returns a cached snapshot if one exists unless
    /// `force` is set.
    pub struct AvatarSnapshot;
    name: "avatar/snapshot",
    access: Privileged,
    params: AvatarSnapshotParams,
    output: AvatarSnapshotResult,
    run(_this, _ctx, p) => {
        let identity = p.identity;
        let width = p.width.unwrap_or(480);
        let height = p.height.unwrap_or(480);

        let avatar_dir = dirs::home_dir()
            .ok_or_else(|| {
                crate::sdk_codegen::CommandError::Internal(
                    "Cannot determine home directory".to_string(),
                )
            })?
            .join(".continuum")
            .join("avatars");

        let png_path = avatar_dir.join(format!("{identity}.png"));
        if png_path.exists() && !p.force {
            return Ok(AvatarSnapshotResult {
                path: format!("/avatars/{identity}.png"),
                cached: true,
            });
        }

        // Bevy slot allocation + frame capture is blocking — off the async thread.
        let relative_path = tokio::task::spawn_blocking(move || {
            crate::modules::avatar::AvatarModule::capture_snapshot(
                &identity, width, height, &avatar_dir,
            )
        })
        .await
        .map_err(|e| {
            crate::sdk_codegen::CommandError::Internal(format!("Snapshot task panicked: {e}"))
        })??;

        Ok(AvatarSnapshotResult {
            path: relative_path,
            cached: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — avatar/snapshot allocates a heavy Bevy
    // render slot and writes a PNG to disk by identity, so it is Privileged, never
    // AiSafe (not a persona toolbelt surface).
    #[test]
    fn name_and_access_wired() {
        assert_eq!(AvatarSnapshot::NAME, "avatar/snapshot");
        assert!(matches!(
            AvatarSnapshot::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }
}
