//! Where captures land: the one fixture-directory policy shared by every live capture
//! (persona turns, prompt and workspace captures) and their readers (replay, introspect,
//! datasets). Moved down from `persona::recorder` so cognition's captures name no persona
//! module.

use std::path::PathBuf;

/// Shared fixture policy for live capture and its readers. Explicit HOME keeps
/// its existing override semantics; native hosts need not inherit a shell HOME.
pub fn fixture_dir(relative: &str) -> Option<PathBuf> {
    crate::paths::home_dir().map(|h| h.join(relative))
}
