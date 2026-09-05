//! The SECOND half of the debug-build gate. The first half asks the binary for
//! `--version` before launch; when that probe times out (it hangs
//! uninterruptibly on the Intel box, 2026-09-05) the lane launches unverified.
//! llama-server prints the same warning on its OWN stderr at startup —
//! "warning: DEBUG BUILD (asserts enabled) -- performance numbers from this
//! process are not valid" — and the daemon already drains that stderr line by
//! line, so a watch on it closes the hole: a debug build never serves through
//! either path. Build for speed (Joel, 2026-09-04).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::child_log::LineWatch;

/// Set once the server's startup output names itself a debug build.
#[derive(Clone, Default)]
pub struct DebugBuildFlag(Arc<AtomicBool>);

impl DebugBuildFlag {
    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

pub struct DebugBuildWatch {
    flag: DebugBuildFlag,
    seen: bool,
}

impl DebugBuildWatch {
    pub fn new(flag: DebugBuildFlag) -> Self {
        Self { flag, seen: false }
    }
}

impl LineWatch for DebugBuildWatch {
    fn observe(&mut self, line: &str) {
        if self.seen || !line.contains("DEBUG BUILD") {
            return;
        }
        self.seen = true;
        self.flag.0.store(true, Ordering::Relaxed);
        crate::probe!(
            class = "serving.debug_build_detected",
            line = %line.trim(),
            "the spawned server named itself a DEBUG build on its own stderr"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the startup line drifting from the watch — the warning
    // llama-server prints must set the flag; ordinary startup lines must not.
    #[test]
    fn the_startup_warning_sets_the_flag_and_ordinary_lines_do_not() {
        let flag = DebugBuildFlag::default();
        let mut w = DebugBuildWatch::new(flag.clone());
        w.observe("build: 10229 (a28ee566c) with AppleClang");
        assert!(!flag.is_set());
        w.observe("warning: DEBUG BUILD (asserts enabled) -- performance numbers from this process are not valid");
        assert!(flag.is_set());
    }
}
