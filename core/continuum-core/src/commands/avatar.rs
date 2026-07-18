//! `avatar/snapshot` — render a persona's 3D avatar to a profile PNG.
//!
//! Stateless: the Bevy-render domain logic lives on
//! [`crate::modules::avatar::AvatarModule`] (shared with its tick-driven
//! auto-refresh); this command orchestrates the on-disk cache check and runs the
//! blocking capture off the async thread.

use crate::live::video::bevy_renderer::{Emotion, Gesture};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The persona's PINNED avatar VRM path (#174) — her true, durable face. Reading the
/// pin keeps the snapshot STABLE regardless of whether the in-memory gender roster is
/// warm ([[never-thrash-sticky-hysteresis-on-every-lane]]). Two sources, live-first:
/// the running runtime's home (fast), else a scan of the durable seeds by persona_id
/// (covers the cold window right after a reboot, before she has spawned). `None` (not
/// a UUID, unpinned, or the VRM file is missing) → the render falls back to the
/// deterministic selection inside `capture_snapshot`.
async fn pinned_vrm_for(identity: &str) -> Option<std::path::PathBuf> {
    let uuid = uuid::Uuid::parse_str(identity).ok()?;
    let vrm = match pin_from_live_runtime(uuid).await {
        Some(v) => v,
        None => pin_from_seed_scan(uuid).await?,
    };
    let path = crate::live::avatar::catalog::avatar_model_path(&vrm);
    path.exists().then_some(path)
}

/// Read the pin off the persona's LIVE runtime (fast path, the common case).
async fn pin_from_live_runtime(uuid: uuid::Uuid) -> Option<String> {
    let runtime = crate::persona::PersonaAircRuntimeRegistry::try_global()?.get(uuid)?;
    let seed_path = runtime.home().parent()?.join("seed.json");
    let seed = crate::persona::seed::read_seed(&seed_path).await.ok()?;
    seed.avatar_vrm().map(str::to_string)
}

/// Cold-window fallback (#174): the persona isn't live yet (just-rebooted, pre-spawn),
/// so scan the durable persona seeds and match by persona_id. The pin survives on
/// disk, so a cold render is still correct — no re-derivation, no thrash.
async fn pin_from_seed_scan(uuid: uuid::Uuid) -> Option<String> {
    let root = dirs::home_dir()?.join(".continuum");
    let dir = crate::context::citizen_path::citizens_kind_dir(
        &root,
        crate::identity::IdentityKind::Persona,
    );
    let mut entries = tokio::fs::read_dir(&dir).await.ok()?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let seed_path = entry.path().join("seed.json");
        if let Ok(seed) = crate::persona::seed::read_seed(&seed_path).await {
            if seed.persona_id() == uuid {
                return seed.avatar_vrm().map(str::to_string);
            }
        }
    }
    None
}

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
    /// Glass box (#172): render a facial EXPRESSION instead of the idle neutral face
    /// — `neutral | happy | sad | angry | surprised | relaxed`. Cached under a
    /// state-suffixed filename so each expression is independently inspectable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub expression: Option<Emotion>,
    /// Glass box (#172): render an upper-body POSE/gesture — `none | wave | think |
    /// nod | shrug | point | open_hands`. Fires alongside the expression (arms vs face).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pose: Option<Gesture>,
    /// Glass box (#172): render the MOUTH at an openness weight `0.0` (closed) …
    /// `1.0` (wide) — the viseme / lip-sync dev knob. This is the SAME signal the
    /// streaming-TTS→avatar path drives, so it makes lip-sync developable by eye.
    /// Combine with `expression`/`pose`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub mouth: Option<f32>,
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
        let expression = p.expression;
        let pose = p.pose;
        let mouth = p.mouth;

        let avatar_dir = dirs::home_dir()
            .ok_or_else(|| {
                crate::sdk_codegen::CommandError::Internal(
                    "Cannot determine home directory".to_string(),
                )
            })?
            .join(".continuum")
            .join("avatars");

        // State-suffixed filename so an expression/pose caches independently of the
        // neutral profile (and each other): `<identity>[-<expr>][-<pose>].png`.
        let mut stem = identity.clone();
        if let Some(e) = expression {
            stem.push('-');
            stem.push_str(&format!("{e:?}").to_lowercase());
        }
        if let Some(g) = pose {
            stem.push('-');
            stem.push_str(&format!("{g:?}").to_lowercase());
        }
        if let Some(m) = mouth {
            stem.push('-');
            stem.push_str(&format!("mouth{}", (m.clamp(0.0, 1.0) * 100.0) as u32));
        }

        let png_path = avatar_dir.join(format!("{stem}.png"));
        if png_path.exists() && !p.force {
            return Ok(AvatarSnapshotResult {
                path: format!("/avatars/{stem}.png"),
                cached: true,
            });
        }

        // #174: resolve her DURABLE pinned VRM here (async seed read), so the render
        // uses her true, stable face regardless of whether the in-memory gender roster
        // is warm. Resolved BEFORE spawn_blocking (which is sync). None → fall back.
        let pinned = pinned_vrm_for(&identity).await;

        // Bevy slot allocation + frame capture is blocking — off the async thread.
        let stem_for_task = stem.clone();
        let relative_path = tokio::task::spawn_blocking(move || {
            crate::modules::avatar::AvatarModule::capture_snapshot(
                &identity, width, height, &avatar_dir, pinned, expression, pose, mouth,
                &stem_for_task,
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
