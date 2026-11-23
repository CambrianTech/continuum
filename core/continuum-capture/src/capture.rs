//! Where captures land: the one fixture-directory policy shared by every live capture
//! (persona turns, prompt and workspace captures) and their readers (replay, introspect,
//! datasets). Moved down from `persona::recorder` so cognition's captures name no persona
//! module.

use std::path::PathBuf;

/// Shared fixture policy for live capture and its readers: `relative` (e.g.
/// `fixtures/prompt-captures`) under the one resolved continuum home, so `$CONTINUUM_HOME`
/// moves every capture writer and reader together (card 3c8b03ae). No home, no captures.
pub fn fixture_dir(relative: &str) -> Option<PathBuf> {
    continuum_kernel::paths::continuum_home().ok().map(|home| home.join(relative))
}

/// Where prompt-call frames live: the `prompt-captures` scope of the `fixtures` disk class, one
/// directory per persona below it. The one place that scope is named; the writer, its retention and
/// every reader resolve it here.
pub fn prompt_class() -> std::io::Result<continuum_kernel::system_resources::disk_reporters::ClassWriter> {
    continuum_kernel::system_resources::disk_reporters::ClassWriter::of("fixtures")?.within("prompt-captures")
}
