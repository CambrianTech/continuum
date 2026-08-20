//! `WebShot` — outlier A: capture a web page by driving a headless
//! **Chromium-family** browser with its built-in `--screenshot` flag. No
//! browser-automation crate; the same locate-binary → spawn → collect-PNG shape as
//! the mobile adapters.
//!
//! "Chromium-family" — not just Chrome. Chrome, Chromium, Edge, Brave, Vivaldi and
//! Opera/Opera GX are all the same Chromium engine and all accept
//! `--headless=new --screenshot`, so we DISCOVER the family rather than hardcode a
//! single browser. For anything atypical (a custom build, an unlisted install
//! path, a non-default browser), [`BROWSER_BIN_ENV`] is the guaranteed override —
//! point it at any Chromium-based binary and it's used verbatim, first.
//!
//! Note on reliability: Chrome/Chromium are the best-tested for headless capture.
//! The others usually work but occasionally lag a Chromium release behind on
//! headless flags — if a non-Chrome browser misbehaves, install Chromium (or point
//! the env var at one) and the same code path just works.
//!
//! The headless run renders the page and writes a PNG to `--screenshot=<path>`.
//! Element-level cropping (a CSS selector) needs the DevTools protocol, which is a
//! later slice — for now this captures the page at the requested viewport.

use async_trait::async_trait;
use tokio::process::Command;

use super::screenshotter::{find_binary, Availability, CaptureRequest, Screenshotter};

/// Env override for the Chromium-based browser binary (absolute path). The escape
/// hatch for any atypical install — checked first, used verbatim.
const BROWSER_BIN_ENV: &str = "CONTINUUM_BROWSER_BIN";

/// Upper bound on a capture. Only ever hit on genuine failure (URL unreachable,
/// page never renders) — a success returns as soon as the PNG is written, via
/// polling, regardless of whether the browser exits.
const CAPTURE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// How often to check whether the screenshot file has been written + settled.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(150);

/// Let an async / single-page app settle before the screenshot. Headless Chrome
/// captures at the load event by default — far too early for a JS app that mounts,
/// opens a WebSocket, waits for the first state frame, and only THEN renders (an
/// empty page otherwise — the failure this fixes). `--virtual-time-budget` advances
/// the page's timers/microtasks and holds the screenshot until the budget is spent
/// or the page goes idle, so the RENDERED content is what lands in the PNG. 3s is
/// ample for a localhost socket round-trip + first paint; a genuinely stuck page
/// still bounds out via `CAPTURE_TIMEOUT`.
const RENDER_SETTLE_MS: u64 = 3000;

/// Where a Chromium-family browser typically lives, in probe order. macOS app
/// bundles first (no `$PATH` entry), then the Linux bare names resolved via
/// `$PATH`. All are the same engine and honor `--headless=new --screenshot`; the
/// order is "most-tested-for-headless first" so a host with several installed
/// picks the most reliable one. Atypical browsers are served by the env override.
const BROWSER_CANDIDATES: &[&str] = &[
    // macOS bundles — Chrome/Chromium first (best-tested headless), then the rest
    // of the family alphabetically.
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
    "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    // Both Opera and Opera GX name their executable "Opera" inside the bundle.
    "/Applications/Opera.app/Contents/MacOS/Opera",
    "/Applications/Opera GX.app/Contents/MacOS/Opera",
    "/Applications/Vivaldi.app/Contents/MacOS/Vivaldi",
    // Linux $PATH names.
    "google-chrome",
    "google-chrome-stable",
    "chromium",
    "chromium-browser",
    "brave-browser",
    "microsoft-edge",
    "vivaldi",
];

pub struct WebShot;

impl WebShot {
    fn locate() -> Option<std::path::PathBuf> {
        locate_browser()
    }
}

/// Locate the host's Chromium-family browser (env override → known bundles/`$PATH`).
/// Shared so the `web/*` hands can drive the SAME real browser for page CONTENT
/// (`--dump-dom`) that this module drives for screenshots — a real browser renders JS
/// and isn't bot-blocked the way a raw HTTP scrape is. `None` ⟹ no browser installed.
pub(crate) fn locate_browser() -> Option<std::path::PathBuf> {
    find_binary(Some(BROWSER_BIN_ENV), BROWSER_CANDIDATES)
}

#[async_trait]
impl Screenshotter for WebShot {
    fn target(&self) -> &'static str {
        "web"
    }

    async fn availability(&self) -> Availability {
        match Self::locate() {
            Some(_) => Availability::Ready,
            None => Availability::Unavailable(format!(
                "no Chromium-based browser found for the `web` target. Install Chromium \
                 or Google Chrome (best-tested for headless capture), or — for any \
                 Chromium-based browser already installed (Brave, Edge, Vivaldi, \
                 Opera/Opera GX) — point {BROWSER_BIN_ENV} at its binary, e.g.\n  \
                 {BROWSER_BIN_ENV}=\"/Applications/Opera GX.app/Contents/MacOS/Opera\""
            )),
        }
    }

    async fn capture(&self, req: &CaptureRequest) -> Result<(), String> {
        let chrome = Self::locate().ok_or_else(|| {
            "the Chromium-based browser disappeared between availability check and capture"
                .to_string()
        })?;
        let url = req.url.as_deref().ok_or_else(|| {
            "the `web` target needs a `url` to load (e.g. http://localhost:5173 or a \
             file:// path)"
                .to_string()
        })?;

        let out = req.out_path.to_string_lossy().to_string();
        // Spawn (not `.status()`) because some Chromium browsers — verified with
        // Opera GX — WRITE the PNG correctly but then never exit headless. So we
        // can't wait for exit; we treat **file-written-and-stable** as the success
        // signal. We POLL for that so a fast browser (Chrome exits, file's there)
        // and a hanging one (Opera GX, file appears in ~1-2s) both return promptly
        // — the full timeout is only ever hit on a genuine failure.
        let mut child = Command::new(&chrome)
            .arg("--headless=new")
            .arg("--disable-gpu")
            .arg("--hide-scrollbars")
            // Wait for the app to render (mount → WS connect → first frame) before
            // the screenshot fires — otherwise we capture a blank pre-render page.
            .arg(format!("--virtual-time-budget={RENDER_SETTLE_MS}"))
            .arg(format!("--window-size={},{}", req.width, req.height))
            .arg(format!("--screenshot={out}"))
            .arg(url)
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("failed to spawn the browser ({}): {e}", chrome.display()))?;

        let outcome = tokio::time::timeout(CAPTURE_TIMEOUT, async {
            // Require the file to be non-empty AND the same size across two polls,
            // so we never read a half-flushed PNG.
            let mut last_size: Option<u64> = None;
            loop {
                let size = std::fs::metadata(&req.out_path).map(|m| m.len()).ok();
                if let Some(sz) = size {
                    if sz > 0 && last_size == Some(sz) {
                        return; // written and stable — done
                    }
                }
                last_size = size;

                // If the browser exited on its own (Chrome's path), one more
                // settle poll then we're done regardless.
                if matches!(child.try_wait(), Ok(Some(_))) {
                    if std::fs::metadata(&req.out_path)
                        .map(|m| m.len())
                        .unwrap_or(0)
                        > 0
                    {
                        return;
                    }
                }
                tokio::time::sleep(POLL_INTERVAL).await;
            }
        })
        .await;

        // Stop the browser whether it hung or we're done; the file check is the
        // real verdict either way.
        let _ = child.kill().await;

        match std::fs::metadata(&req.out_path) {
            Ok(m) if m.len() > 0 => Ok(()),
            _ => Err(format!(
                "the browser ({}) wrote no screenshot at {out} capturing {url}{} — check \
                 the URL is reachable and the page renders",
                chrome.display(),
                if outcome.is_err() {
                    " (timed out before any image appeared)"
                } else {
                    ""
                }
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the adapter identifies as "web" and, when no browser is
    // installed, reports Unavailable with an actionable reason that names the
    // Chromium family AND the env override (so an atypical browser like Opera GX is
    // covered) — not Ready, not a panic — the public-user path on a fresh machine.
    #[tokio::test]
    async fn unavailable_names_the_family_and_override_when_absent() {
        // Force the locate() miss regardless of host: point the override at a
        // non-file and ensure no candidate resolves by clearing PATH for the call.
        let prev_path = std::env::var_os("PATH");
        std::env::set_var(BROWSER_BIN_ENV, "/definitely/not/a/browser");
        std::env::set_var("PATH", "");
        let avail = WebShot.availability().await;
        if let Some(p) = prev_path {
            std::env::set_var("PATH", p);
        }
        std::env::remove_var(BROWSER_BIN_ENV);

        assert_eq!(WebShot.target(), "web");
        match avail {
            Availability::Unavailable(msg) => {
                assert!(msg.contains("Chromium"), "names the family: {msg}");
                assert!(msg.contains(BROWSER_BIN_ENV), "tells how to fix: {msg}");
            }
            // If the host genuinely has a browser on an absolute candidate path
            // this could be Ready — accept that rather than fail spuriously.
            Availability::Ready => {}
        }
    }
}
