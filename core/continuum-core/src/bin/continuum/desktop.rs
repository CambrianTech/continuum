//! The desktop (web client) port, URL and the receipt line naming its state.

use super::*;

/// The desktop display manager's port — `CONTINUUM_UI_PORT`, else the
/// documented default beside WS 8974 (http::desktop). ONE place; the
/// `desktop` verb and the start/reboot receipt both read it.
pub(super) fn desktop_port() -> u16 {
    std::env::var("CONTINUUM_UI_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(8975) // unwrap_or: the display manager's documented default
}

pub(super) fn desktop_url() -> String {
    format!("http://127.0.0.1:{}/", desktop_port())
}

/// Bounded (1 s) "is the greeter answering" probe — on a deploy path, so it
/// has a bound and a named outcome, never a hang.
pub(super) async fn desktop_answering() -> bool {
    matches!(
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            tokio::net::TcpStream::connect(("127.0.0.1", desktop_port())),
        )
        .await,
        Ok(Ok(_))
    )
}

/// The line a verified start/reboot ends with: WHERE the desktop is. A user
/// must never have to know a port (Joel, 2026-09-05: "remembering port is
/// bush league") — the CLI says the address, and `uu desktop` opens it.
/// The line a verified start/reboot ends with: WHERE the desktop is. A user
/// must never have to know a port (Joel, 2026-09-05: "remembering port is
/// bush league") — the CLI says the address, and `uu desktop` opens it.
pub(super) async fn desktop_receipt_line() -> String {
    // Configured = the core can find a dist: `CONTINUUM_UI_DIST` in this environment
    // (a direct launch) or pinned in config.env (the file the core applies to itself on
    // every boot — the only route to a supervised core). Without it, "lands in the
    // background" was a promise nothing kept: the M5 printed it on eight consecutive
    // deploys (2026-09-26) while the core booted `desktop.dm.unconfigured` each time.
    let configured = std::env::var("CONTINUUM_UI_DIST").ok().filter(|v| !v.trim().is_empty()).is_some()
        || continuum_base::config_env::read("CONTINUUM_UI_DIST").is_some();

    // Read the dist manifest written by start-server.sh after a successful web build.
    let dist_path = std::env::var("CONTINUUM_UI_DIST")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| continuum_base::config_env::read("CONTINUUM_UI_DIST"));

    let manifest_info = dist_path.as_deref().map(dist_manifest_info);

    desktop_receipt(desktop_answering().await, configured, desktop_port(), manifest_info.as_deref())
}

/// The receipt, pure over what was observed: answering wins; a configured dist that is
/// not answering yet is a build in flight; no dist configured is named as such, with the
/// verb that fixes it. Never a promise the launch path cannot keep.
pub(super) fn desktop_receipt(answering: bool, configured: bool, port: u16, manifest_info: Option<&str>) -> String {
    let suffix = manifest_info.map(|s| s.to_string()).unwrap_or_default();
    if answering {
        format!("🖥  desktop: http://127.0.0.1:{port}/   (`uu desktop` opens it){suffix}")
    } else if configured {
        format!(
            "🖥  desktop: not serving yet on :{port} — the web build lands in the background; \
             `uu desktop` opens it once it does{suffix}"
        )
    } else {
        format!(
            "🖥  desktop: not configured — no CONTINUUM_UI_DIST in ~/.continuum/config.env; \
             `continuum start` from the source tree pins it and builds the web client (:{port} once it does)"
        )
    }
}

/// Reads `.dist-manifest` from the dist directory (written by start-server.sh) and returns a
/// short string like " dist sha abc1234567 (built 5m ago)", or None if absent/invalid.
fn dist_manifest_info(dist_path: &str) -> Option<String> {
    let manifest = format!("{dist_path}/.dist-manifest");
    let content = std::fs::read_to_string(&manifest).ok()?;

    let mut sha = String::new();
    let mut built_at: u64 = 0;

    for line in content.lines() {
        if line.starts_with("sha=") {
            sha = line.trim_start_matches("sha=").trim().to_string();
        } else if line.starts_with("built_at=") {
            built_at = line.trim_start_matches("built_at=").trim().parse().ok()?;
        }
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let age = now.saturating_sub(built_at);

    let age_str = if age < 60 {
        format!("{age}s ago")
    } else if age < 3600 {
        format!("{}m {}s ago", age / 60, age % 60)
    } else {
        let h = age / 3600;
        let m = (age % 3600) / 60;
        if m > 0 {
            format!("{h}h {m}m ago")
        } else {
            format!("{h}h ago")
        }
    };

    Some(format!(" dist sha {} ({})", &sha[..8.min(sha.len())], age_str))
}
