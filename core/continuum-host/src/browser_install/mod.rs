//! Where this OS installs a Chromium-family browser beyond `$PATH`: the one OS difference in
//! finding the browser the `web` capture target and the `web/*` hands drive. The caller searches
//! these first, then its shared list (macOS bundles, `$PATH` names). Moved out of
//! `capture/web.rs`'s `#[cfg(windows)]` branches (card 4f7d54ee).

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod os;

/// This OS's browser install locations, most preferred first. Empty where the shared list
/// already covers the platform.
pub fn install_candidates() -> Vec<String> {
    os::install_candidates()
}
