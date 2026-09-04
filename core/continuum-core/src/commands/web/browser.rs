//! Drive the host's REAL headless browser for web content.
//!
//! The web is hostile to HTTP scrapers — search engines serve bot-block/"anomaly"
//! pages to `reqwest`-shaped requests, and modern pages render their content with
//! JS that a raw GET never runs. So the persona's web hands (`web/search`,
//! `web/fetch`) drive the same real Chromium-family browser the screenshotter
//! already locates ([`crate::commands::interface::capture::web::locate_browser`]),
//! but with `--dump-dom` instead of `--screenshot`: Chrome loads the URL, runs its
//! JS, and prints the RENDERED DOM to stdout. A real browser gets real results.

use std::time::Duration;

use tokio::process::Command;

use crate::sdk_codegen::CommandError;

/// A believable desktop UA — some engines vary output by UA even for a real browser.
pub(crate) const RENDER_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
     AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// Hard ceiling on a single render (a stuck page can't hang the tool forever).
const RENDER_TIMEOUT: Duration = Duration::from_secs(30);

/// Load `url` in the host's headless browser and return its RENDERED DOM (JS executed).
///
/// `settle_ms` is Chrome's virtual-time budget — how long to let the page render before
/// the DOM is dumped. Fails LOUD (never a silent empty) if no browser is installed, the
/// render times out, or the DOM comes back empty.
pub(crate) async fn render_dom(url: &str, settle_ms: u64) -> Result<String, CommandError> {
    let chrome = crate::commands::interface::capture::web::locate_browser().ok_or_else(|| {
        CommandError::Internal(
            "no Chromium-family browser found to drive the web. Install Google Chrome or \
             Chromium, or set CONTINUUM_BROWSER_BIN to a Chromium-based browser's binary."
                .to_string(),
        )
    })?;

    let run = Command::new(&chrome)
        .arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--no-sandbox")
        .arg("--hide-scrollbars")
        .arg("--disable-blink-features=AutomationControlled")
        .arg(format!("--user-agent={RENDER_UA}"))
        .arg(format!("--virtual-time-budget={settle_ms}"))
        .arg("--dump-dom")
        .arg(url)
        .kill_on_drop(true)
        .output();

    let out = tokio::time::timeout(RENDER_TIMEOUT, run)
        .await
        .map_err(|_| CommandError::Internal(format!("browser render timed out for '{url}'")))?
        .map_err(|e| {
            CommandError::Internal(format!(
                "failed to spawn the browser ({}): {e}",
                chrome.display()
            ))
        })?;

    let dom = String::from_utf8_lossy(&out.stdout).into_owned();
    if dom.trim().is_empty() {
        return Err(CommandError::Internal(format!(
            "the browser returned an empty DOM for '{url}' (the render likely failed)"
        )));
    }
    Ok(dom)
}
