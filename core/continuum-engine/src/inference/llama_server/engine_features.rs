//! What the RUNNING engine can do beyond a stock llama-server, decided from the build the lane
//! itself reports (`/props` `build_info`): never a table, an env var or a constant `true`.
//!
//! `tail_tools` (CambrianTech/llama.cpp#75) is a request field only: the fork's server names it
//! in no `/props` key and no `--help` flag, so nothing about the engine says it outright. The
//! evidence is the revision. A lane whose reported commit IS the commit this tree pins runs the
//! pinned source, and source-hygiene's `vendor_pin` proves that source parses `tail_tools`. Any
//! other build (an older lane a deploy left running, an operator's engine, a stock server, a
//! `build_info` naming no credible commit) is not evidence and answers `false`, so a mind's
//! learned tool head stays a shadow there instead of sending verbs a server would read as text.

/// The fork commit this tree pins, compiled in from the one file a PR edits to move it.
const PINNED_ENGINE: &str = include_str!("../../../../vendor/LLAMA_CPP_PIN");

/// The `build_info` the serving lane last reported. Written by the gateway reconcile from the
/// same `/props` read that converges the engine; a probe error leaves the last reading, so a
/// transient hiccup never flips her tool surface (and with it her cached prefix) for a tick.
static SERVED_ENGINE_BUILD: parking_lot::Mutex<Option<String>> = parking_lot::Mutex::new(None);

/// Record what the serving lane's `/props` named (`None`: it named no build).
pub(super) fn record_served_engine_build(build: Option<&str>) {
    *SERVED_ENGINE_BUILD.lock() = build.map(str::to_owned);
}

/// PURE: whether a lane reporting `build_info` (`b10765-1c6681593`) runs the `pinned` commit,
/// whose source carries `tail_tools`. Both sides must be credible SHAs (the ONE SHA rule,
/// `deploy_provenance::sha_matches`); an unreadable build is not support.
pub(crate) fn build_takes_tail_tools(build_info: &str, pinned: &str) -> bool {
    continuum_cli_lifecycle::deploy_provenance::sha_matches(super::engine_install::build_commit(build_info), pinned.trim())
}

/// Whether the serving lane's own reported build takes `tail_tools`. `false` until the gateway
/// reconcile has read a build, and for any build other than the pinned one.
pub fn serving_engine_takes_tail_tools() -> bool {
    SERVED_ENGINE_BUILD
        .lock()
        .as_deref()
        .is_some_and(|build| build_takes_tail_tools(build, PINNED_ENGINE))
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a learned tool head sent to an engine that cannot call a tail verb
    // (an older lane a deploy left up, a stock or operator build, an unreadable build_info),
    // or withheld from the lane that runs the pinned fork and does.
    #[test]
    fn only_a_lane_reporting_the_pinned_build_takes_tail_tools() {
        let pinned = PINNED_ENGINE.trim();
        let short: String = pinned.chars().take(9).collect(); // the short commit llama.cpp stamps into build_info
        assert!(build_takes_tail_tools(&format!("b10765-{short}"), PINNED_ENGINE));
        assert!(build_takes_tail_tools(pinned, PINNED_ENGINE));
        for other in ["b10765-e7e1077b3", "b10765-unknown", "b10765-", "", "unknown"] {
            assert!(!build_takes_tail_tools(other, PINNED_ENGINE), "{other:?} must not take tail_tools");
        }
    }
}
