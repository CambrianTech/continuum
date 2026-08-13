//! `interface/capture` — screenshot a render TARGET the server drives: a headless
//! web page, an iOS Simulator, or an Android emulator. This is the app-dev preview
//! loop's eyes — build → run → SEE it — and the first server-side screenshot in the
//! Rust core.
//!
//! Distinct from `interface/screenshot` (the `WireShape::Provided` command that
//! routes OUT to a connected client to capture that client's own live UI). This
//! one runs on the server, drives a surface the persona is building, and is backed
//! by the polymorphic [`screenshotter::Screenshotter`] adapter registry — one
//! interface, three targets, each proven against a real CLI (`chrome --headless`,
//! `xcrun simctl`, `adb`).
//!
//! Output is written to a per-persona capture directory (the directory layout IS
//! the scope, like the tool-output spill dir), and the path is returned. A later
//! slice routes the saved image through the media/vision pipeline so the persona
//! PERCEIVES it — a capable model sees the pixels, a lesser one gets a description
//! (the same ramp-up/ramp-down the genome gives the rest of cognition).

pub mod android;
pub mod ios;
pub mod screenshotter;
pub mod web;

use std::path::PathBuf;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};
use screenshotter::{png_dimensions, Availability, CaptureRequest};

/// Default viewport for the web target, in CSS pixels.
const DEFAULT_WIDTH: u32 = 1280;
const DEFAULT_HEIGHT: u32 = 800;

/// Inputs to `interface/capture`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/interface/CaptureParams.ts")]
pub struct CaptureParams {
    /// Which surface to capture: `"web"`, `"ios"`, or `"android"`.
    pub target: String,
    /// Web only: the URL / local dev-server address to load
    /// (e.g. `"http://localhost:5173"` or a `file://` path).
    #[ts(optional)]
    pub url: Option<String>,
    /// Web only: viewport width in CSS px (default 1280).
    #[ts(optional)]
    #[ts(type = "number")]
    pub width: Option<u32>,
    /// Web only: viewport height in CSS px (default 800).
    #[ts(optional)]
    #[ts(type = "number")]
    pub height: Option<u32>,
    /// Mobile only: device identifier (iOS udid / Android adb serial). Omit to use
    /// the single booted simulator / attached device.
    #[ts(optional)]
    pub device: Option<String>,
}

/// Result of a capture: where the PNG landed and its real dimensions.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/interface/CaptureResult.ts")]
pub struct CaptureResult {
    /// Echo of the captured target.
    pub target: String,
    /// Absolute path to the saved PNG (under the per-persona capture dir).
    pub path: String,
    /// Captured pixel width (0 if the PNG header couldn't be read).
    #[ts(type = "number")]
    pub width: u32,
    /// Captured pixel height (0 if the PNG header couldn't be read).
    #[ts(type = "number")]
    pub height: u32,
    /// PNG size on disk in bytes.
    #[ts(type = "number")]
    pub bytes: usize,
}

/// Root for all captures, under the `~/.continuum` convention. Public so the boot
/// path can register it with the `PressureBroker` for eviction (single source for
/// the path). Fails loud with no home dir [[fallbacks-are-illegal-fail-loud]].
pub fn capture_root() -> std::io::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no home directory — cannot store captures",
        )
    })?;
    Ok(home.join(".continuum").join("captures"))
}

/// Capture-dir scope for a caller: a persona's `peer_id`, or `"local"` for the
/// owner-by-locality operator. The local Unix-socket `uu` path carries no persona
/// identity ([[handle_client]] stamps `None` — "the operator on the box"), yet the
/// operator is a first-class caller who can `uu interface/capture` any interface
/// from a CLI call. A remote (TCP) caller always carries a `peer_id`, so `"local"`
/// only ever names the trusted local socket. Not a fallback
/// ([[fallbacks-are-illegal-fail-loud]]): the operator is a real caller, just not a
/// persona — so we name their scope, never mint a ghost citizen or silently no-op.
fn capture_scope(caller: Option<&crate::routing::auth_policy::CallerIdentity>) -> String {
    caller
        .map(|c| c.peer_id.to_string())
        .unwrap_or_else(|| "local".to_string())
}

/// `interface/capture` — drive a target and screenshot it. AiSafe; scopes output
/// to the caller (a persona's peer_id, or `"local"` for the box operator).
#[derive(Default)]
pub struct Capture;

#[async_trait]
impl ActionCommand for Capture {
    const NAME: &'static str = "interface/capture";
    const DESCRIPTION: &'static str =
        "Screenshot a target you are building: `target:\"web\"` (a headless browser \
         loading a `url`), `target:\"ios\"` (the booted iOS Simulator), or \
         `target:\"android\"` (the attached emulator). Use this in the build→run→see \
         loop to verify how your app/site actually renders. Returns the saved PNG \
         path. If the target's tooling isn't installed, you get a clear message \
         naming what to install (Chrome / Xcode / adb).";
    type Params = CaptureParams;
    type Output = CaptureResult;

    async fn run(&self, ctx: &Ctx, params: CaptureParams) -> Result<CaptureResult, CommandError> {
        // Scope the capture to the caller — a persona's peer_id, or "local" for the
        // owner-by-locality operator (`uu interface/capture` from the box).
        let scope = capture_scope(ctx.caller.as_ref());

        // Resolve the adapter — fails loud listing valid targets on a typo.
        let adapter = screenshotter::resolve(&params.target).map_err(CommandError::Invalid)?;

        // Probe before capturing — the public-user path. An Unavailable target
        // returns its actionable reason, never a silent no-op.
        if let Availability::Unavailable(reason) = adapter.availability().await {
            return Err(CommandError::Invalid(reason));
        }

        // Caller-scoped capture dir = the scope.
        let dir = capture_root()
            .map_err(|e| CommandError::Internal(e.to_string()))?
            .join(&scope);
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| CommandError::Internal(format!("create capture dir: {e}")))?;
        let out_path = dir.join(format!("{}-{}.png", params.target, Uuid::new_v4()));

        let req = CaptureRequest {
            url: params.url.clone(),
            width: params.width.unwrap_or(DEFAULT_WIDTH),
            height: params.height.unwrap_or(DEFAULT_HEIGHT),
            device: params.device.clone(),
            out_path: out_path.clone(),
        };
        adapter.capture(&req).await.map_err(CommandError::Internal)?;

        let bytes = tokio::fs::read(&out_path)
            .await
            .map_err(|e| CommandError::Internal(format!("read captured png: {e}")))?;
        let (width, height) = png_dimensions(&bytes).unwrap_or((0, 0));

        Ok(CaptureResult {
            target: params.target,
            path: out_path.to_string_lossy().to_string(),
            width,
            height,
            bytes: bytes.len(),
        })
    }
}
crate::register_stateless_command!(Capture);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: name mirrors path so the persona reaches it by the name
    // it'd guess, and the registration is the typed-path verb (not Provided).
    #[test]
    fn name_is_interface_capture() {
        assert_eq!(Capture::NAME, "interface/capture");
    }

    // what this catches: capture scoping — a persona scopes to its peer_id, and the
    // owner-by-locality operator (no persona caller, the local `uu` path) scopes to
    // "local" instead of being refused. That "local" scope is what lets
    // `uu interface/capture` screenshot any interface from a CLI call.
    #[test]
    fn caller_scope_is_persona_id_or_local() {
        assert_eq!(capture_scope(None), "local");
        let pid = Uuid::from_u128(7);
        let persona = crate::routing::auth_policy::CallerIdentity::local_persona(
            crate::identity::PeerId::from_uuid(pid),
        );
        assert_eq!(capture_scope(Some(&persona)), pid.to_string());
    }

    // what this catches: an unknown target is rejected with a named, listing
    // error BEFORE any availability probe or capture attempt.
    #[tokio::test]
    async fn unknown_target_fails_loud() {
        let ctx = Ctx {
            caller: Some(crate::routing::auth_policy::CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(Uuid::nil()))),
            ..Default::default()
        };
        let err = Capture
            .run(
                &ctx,
                CaptureParams {
                    target: "hologram".into(),
                    ..Default::default()
                },
            )
            .await
            .expect_err("unknown target must fail");
        match err {
            CommandError::Invalid(msg) => {
                assert!(msg.contains("hologram"), "names the bad target: {msg}");
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }
}
