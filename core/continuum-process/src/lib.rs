//! Owned process execution and generation-safe termination, below the engine.
mod owned;
#[cfg(test)]
use owned::read_stream;
pub use owned::{
    command_name, confirm_gone, descendants, end_tree, holding, kill9, kill_if_same, recheck,
    run_owned, run_owned_with_input, signal_if_same, still_present, Answer, CaptureEnd, Captured,
    Ended, OwnedOutput, RunCapture, StreamCapture, Unanswered, Why, KILL_POLL,
};
#[cfg(test)]
mod tests;
/// Parse `tasklist /NH /FO CSV` output to the image name. Split out as a pure
/// function so the parse is testable on any host — the shell-out is not.
///
/// A miss must be `None`, never a guess: tasklist answers a NO-MATCH filter with
/// the prose line `INFO: No tasks are running which match the specified criteria.`
/// on stdout AND exit status 0, so "success" does not imply "found".
#[cfg(any(windows, test))]
pub(crate) fn parse_tasklist_image_name(stdout: &str) -> Option<String> {
    let line = stdout.lines().find(|l| l.trim_start().starts_with('"'))?;
    let name = line.trim().trim_start_matches('"');
    let name = name.split('"').next()?.trim();
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}
