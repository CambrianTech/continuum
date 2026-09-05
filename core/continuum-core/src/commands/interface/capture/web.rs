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
/// Windows install locations, built from the environment rather than hardcoded.
///
/// BROWSER_CANDIDATES is macOS bundles + Linux `$PATH` names and had NO Windows
/// arm, so `locate_browser()` returned None on every Windows host — including
/// hosts that DO satisfy the "every machine has a Chromium" expectation, because
/// Windows ships Microsoft Edge (Chromium) with the OS. Measured on BigMama
/// 2026-09-05: Edge present at
/// `C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe`, `web/search`
/// failing 100% with "no web-search provider is available" and citizens burning
/// turns on it.
///
/// Paths are composed from `%ProgramFiles%` / `%ProgramFiles(x86)%` /
/// `%LOCALAPPDATA%` — never a literal `C:\`, which is wrong on any machine whose
/// system drive is not C: and is the hardcoded-path trap this repo has been bitten
/// by before. Forward slashes so `find_binary`'s `contains('/')` path branch takes
/// them; Windows APIs accept either separator.
#[cfg(windows)]
fn windows_browser_candidates() -> Vec<String> {
    // Chrome first (best-tested headless), then Edge — universally present, which
    // is what makes the expectation true on this platform — then the rest.
    const RELATIVE: &[&str] = &[
        "Google/Chrome/Application/chrome.exe",
        "Chromium/Application/chrome.exe",
        "Microsoft/Edge/Application/msedge.exe",
        "BraveSoftware/Brave-Browser/Application/brave.exe",
        "Vivaldi/Application/vivaldi.exe",
    ];
    let roots: Vec<String> = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .map(|r| r.replace('\\', "/"))
        .collect();
    let mut out = Vec::with_capacity(roots.len() * RELATIVE.len());
    for rel in RELATIVE {
        for root in &roots {
            out.push(format!("{root}/{rel}"));
        }
    }
    out
}

pub(crate) fn locate_browser() -> Option<std::path::PathBuf> {
    // The env override wins everywhere; then this platform's real install
    // locations; then the shared list. Windows was previously unrepresented in
    // that list, so it fell straight through to None.
    #[cfg(windows)]
    {
        let owned = windows_browser_candidates();
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
        if let Some(p) = find_binary(Some(BROWSER_BIN_ENV), &refs) {
            return Some(p);
        }
    }
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

    // what this catches: BROWSER_CANDIDATES is macOS bundle paths + Linux $PATH
    // names and has NO Windows entry, so `locate_browser()` returned None on
    // EVERY Windows host — including the ones that satisfy the "every machine has
    // a Chromium" expectation, because Windows ships Edge with the OS. That made
    // `web/search` (registered ai-safe, so offered to every citizen) fail 100% of
    // the time on this platform. Found on BigMama 2026-09-05 with Edge installed
    // at "%ProgramFiles(x86)%/Microsoft/Edge/Application/msedge.exe".
    #[cfg(windows)]
    #[test]
    fn windows_has_browser_candidates_built_from_the_environment() {
        let cands = super::windows_browser_candidates();
        assert!(
            !cands.is_empty(),
            "Windows had ZERO browser candidates — the regression this pins"
        );
        // DERIVED from %ProgramFiles%/%ProgramFiles(x86)%/%LOCALAPPDATA%, so a
        // machine whose system drive is not C: still gets correct paths. Asserting
        // "does not start with C:/" would be the wrong test — on this box those env
        // vars ARE C:\Program Files, so that assertion fails on a correct
        // implementation and cannot tell a hardcoded drive from an env var that
        // happens to hold one. (Written, failed exactly that way, and rewritten —
        // 2026-09-05.) The property that actually matters is derivation:
        let roots: Vec<String> = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .map(|r| r.replace('\\', "/"))
            .collect();
        assert!(!roots.is_empty(), "Windows always sets at least one of these");
        assert!(
            cands
                .iter()
                .all(|c| roots.iter().any(|r| c.starts_with(r.as_str()))),
            "every candidate must be rooted at an environment path {roots:?}: {cands:?}"
        );
        assert!(
            cands.iter().any(|c| c.ends_with("msedge.exe")),
            "Edge is the one browser Windows always ships; it must be a candidate"
        );
        // find_binary() routes anything containing '/' down its path branch.
        assert!(
            cands.iter().all(|c| c.contains('/')),
            "forward slashes required or find_binary treats these as $PATH names"
        );

        // THE END-TO-END CLAIM, not just that candidates were generated: Windows
        // ships Edge (Chromium) with the OS, so the "every machine has a Chromium"
        // expectation is SATISFIED here and `locate_browser()` must find it. Before
        // this change it returned None on every Windows host regardless of what was
        // installed, because the candidate list had no Windows arm at all.
        //
        // If this ever fails on a real Windows box, that is worth knowing loudly:
        // it means either Edge was removed or the install layout moved, and
        // `web/search` is dark for every citizen on that host.
        assert!(
            super::locate_browser().is_some(),
            "no Chromium found on a Windows host — Edge ships with the OS, so either \
             the layout moved or CONTINUUM_BROWSER_BIN is needed. Candidates: {cands:?}"
        );
    }
}
