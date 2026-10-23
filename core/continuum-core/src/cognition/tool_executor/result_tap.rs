//! The act seam's one TAP: a surface that renders what a persona's hands returned (the
//! human's canvas, today) installs a function here at boot, and the tool executor hands it
//! each decoded result. The mind owns the seam and never names the surface; an uninstalled
//! tap means no such surface runs in this process (a test, a tool), which is not an error.

use parking_lot::RwLock;

/// Sees one decoded tool result: the persona who acted, the canonical command, the result.
pub type ToolResultTap = fn(persona: &str, tool: &str, result: &serde_json::Value);

static TAP: RwLock<Option<ToolResultTap>> = RwLock::new(None);

/// Install the one tap. A second install replaces the first.
pub fn install(tap: ToolResultTap) {
    *TAP.write() = Some(tap);
}

/// Hand one result to the installed tap, if any.
pub(crate) fn tap(persona: &str, tool: &str, result: &serde_json::Value) {
    if let Some(tap) = *TAP.read() {
        tap(persona, tool, result);
    }
}
