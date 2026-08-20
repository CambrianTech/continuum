//! `desktop` — open the Continuum UI in the operator's NATURAL default browser
//! (their choice, a real visible window — NOT the headless browser
//! `interface/capture` drives for screenshots), managed as a single-instance
//! resource so repeated calls DON'T spawn a new tab every time.
//!
//! Start-simple: a lightweight on-disk session marker is the "lease" — one desktop
//! per machine. A second `uu desktop` for the same URL is a no-op ("already open")
//! so it never opens a duplicate window; `focus:true` forces a re-open. A later
//! slice promotes this to a first-class [`crate::resources`] / ResourceGovernor
//! (#56) consumer alongside VRAM / serving / Bevy, so the desktop window is
//! allocated + reclaimed like any other machine resource. Owner-scoped: the local
//! `uu` operator opens their own box's desktop; no persona identity required.

use std::path::PathBuf;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};

/// Env override for the URL `desktop` opens. Default = the local web client pointed
/// at the default WS ingress ([[ws-on-by-default]]).
const DESKTOP_URL_ENV: &str = "CONTINUUM_DESKTOP_URL";
const DEFAULT_DESKTOP_URL: &str = "http://localhost:5173/?core=ws://127.0.0.1:8974";

/// Inputs to `desktop`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/desktop/DesktopParams.ts"
)]
pub struct DesktopParams {
    /// URL to open. Omit for the default local web client
    /// (`CONTINUUM_DESKTOP_URL`, else the localhost dev URL).
    #[ts(optional)]
    pub url: Option<String>,
    /// Re-open / focus even when a session for this URL is already active. Default
    /// false — an active session is a no-op, so we never spawn a duplicate window.
    #[ts(optional)]
    pub focus: Option<bool>,
}

/// Result of `desktop`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/desktop/DesktopResult.ts"
)]
pub struct DesktopResult {
    /// The URL that is now open.
    pub url: String,
    /// True if THIS call launched the browser; false if an existing session was
    /// reused — the "don't spawn a new window every time" contract.
    pub opened: bool,
    /// Human-readable outcome.
    pub status: String,
}

/// The single-instance session marker — the simple "lease" (one desktop per box).
#[derive(Debug, Serialize, Deserialize)]
struct DesktopSession {
    url: String,
    #[serde(default)]
    opened_at_ms: u64,
}

fn session_path() -> Result<PathBuf, CommandError> {
    let home = dirs::home_dir().ok_or_else(|| {
        CommandError::Internal("no home directory for the desktop session marker".into())
    })?;
    Ok(home.join(".continuum").join("desktop").join("session.json"))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Resolve the URL: explicit param → `CONTINUUM_DESKTOP_URL` → built-in default.
/// Blank/whitespace is treated as unset so we never open an empty URL.
fn resolve_url(param: Option<String>) -> String {
    param
        .filter(|u| !u.trim().is_empty())
        .or_else(|| {
            std::env::var(DESKTOP_URL_ENV)
                .ok()
                .filter(|u| !u.trim().is_empty())
        })
        .unwrap_or_else(|| DEFAULT_DESKTOP_URL.to_string())
}

/// Open `url` in the OS default browser — the user's browser of CHOICE, never a
/// hardcoded one. macOS `open`, Linux `xdg-open`, Windows `cmd /c start`. Fails
/// loud with a named cause on an unknown OS or a spawn failure
/// ([[fallbacks-are-illegal-fail-loud]]).
fn open_in_default_browser(url: &str) -> Result<&'static str, CommandError> {
    let (bin, args): (&'static str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else if cfg!(target_os = "linux") {
        ("xdg-open", vec![url])
    } else if cfg!(target_os = "windows") {
        // `start` needs an (empty) window-title arg first; `cmd /C` hosts it.
        ("cmd", vec!["/C", "start", "", url])
    } else {
        return Err(CommandError::Internal(
            "no known default-browser opener for this operating system".into(),
        ));
    };
    std::process::Command::new(bin)
        .args(&args)
        .spawn()
        .map_err(|e| {
            CommandError::Internal(format!("failed to open the browser via `{bin}`: {e}"))
        })?;
    Ok(bin)
}

/// `desktop` — open the UI in the natural browser, single-instance. AiSafe;
/// operator-scoped (the local `uu` caller opens their own box's desktop).
#[derive(Default)]
pub struct Desktop;

#[async_trait]
impl ActionCommand for Desktop {
    const NAME: &'static str = "desktop";
    const DESCRIPTION: &'static str =
        "Open the Continuum UI in your natural default browser — a real window, your \
         browser of choice, NOT headless. Single-instance: a second call for the same \
         URL is a no-op (\"already open\") so it never spawns a duplicate tab; pass \
         `focus:true` to force a re-open. Omit `url` for the local web client.";
    type Params = DesktopParams;
    type Output = DesktopResult;

    async fn run(&self, _ctx: &Ctx, params: DesktopParams) -> Result<DesktopResult, CommandError> {
        let url = resolve_url(params.url);
        let focus = params.focus.unwrap_or(false);
        let path = session_path()?;

        // Reuse an active session for the SAME url unless focus forces a re-open —
        // this is the "don't constantly spawn new open url" guard, the resource
        // single-instance contract. Best-effort read: a missing/garbled marker just
        // means "not tracked as open" → we open.
        if !focus {
            if let Some(session) = std::fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<DesktopSession>(&b).ok())
            {
                if session.url == url {
                    return Ok(DesktopResult {
                        url,
                        opened: false,
                        status: "already open in your browser (pass focus:true to re-open)".into(),
                    });
                }
            }
        }

        open_in_default_browser(&url)?;

        // Record the lease. The open already succeeded, so a marker-write failure is
        // named, not swallowed — a future call would just re-open (harmless).
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| CommandError::Internal(format!("create desktop session dir: {e}")))?;
        }
        let session = DesktopSession {
            url: url.clone(),
            opened_at_ms: now_ms(),
        };
        let bytes = serde_json::to_vec_pretty(&session)
            .map_err(|e| CommandError::Internal(format!("encode desktop session: {e}")))?;
        std::fs::write(&path, bytes)
            .map_err(|e| CommandError::Internal(format!("write desktop session marker: {e}")))?;

        Ok(DesktopResult {
            url,
            opened: true,
            status: "opened in your default browser".into(),
        })
    }
}
crate::register_stateless_command!(Desktop);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: reachable as `uu desktop` (name mirrors the verb typed).
    #[test]
    fn name_is_desktop() {
        assert_eq!(Desktop::NAME, "desktop");
    }

    // what this catches: url precedence — explicit param wins; blank/whitespace is
    // treated as unset (never opens an empty URL); the default targets the local
    // web client on the default WS ingress.
    #[test]
    fn resolve_url_precedence_and_default() {
        assert_eq!(resolve_url(Some("https://x.test".into())), "https://x.test");
        assert_eq!(resolve_url(Some("   ".into())), DEFAULT_DESKTOP_URL);
        assert_eq!(resolve_url(None), DEFAULT_DESKTOP_URL);
        assert!(DEFAULT_DESKTOP_URL.starts_with("http://localhost"));
        assert!(DEFAULT_DESKTOP_URL.contains("core=ws://"));
    }
}
