//! The scheduling class a mind's child runs in: below the engine that serves her, so a build or a
//! test suite she runs takes every core the engine is not using and yields the moment it is.
//!
//! On the M5 (2026-11-10) a resident's `cargo test` of the whole workspace ran at the engine's own
//! priority beside serving: load 36, 4.5 GB swapped, decode at 3.4 tokens/s against the 56 it
//! serves at rest, and the four residents' acts fell from 39 to 10 per 15 minutes. Her build is
//! never refused or capped for it (residents build freely); it is scheduled, as an OS schedules a
//! background job. What differs by OS is only the call: Unix lowers the child to the OS's lowest
//! ordinary priority (`PRIO_MAX`, macOS and Linux), which still runs on every idle core (macOS's
//! `PRIO_DARWIN_BG` would pin it to the efficiency cores even on an idle machine); Windows gives
//! it the idle priority class; elsewhere the child keeps its parent's class.

#[cfg_attr(any(target_os = "macos", target_os = "linux"), path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(not(any(target_os = "macos", target_os = "linux", windows)), path = "other.rs")]
mod os;

/// Run `cmd`'s process, and everything it starts, below the foreground (module doc).
pub fn below_the_foreground(cmd: &mut tokio::process::Command) {
    os::below_the_foreground(cmd);
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a mind's child left at its parent's priority beside serving (the M5,
    // 2026-11-10, a resident build at the engine's priority dropped decode to 3.4 tokens/s). The
    // child reports a lower priority than this process, wherever an adapter sets one.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[tokio::test]
    async fn a_child_runs_below_its_parent() {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.args(["-c", "ps -o nice= -p $$"]);
        below_the_foreground(&mut cmd);
        let out = cmd.output().await.expect("test: sh runs"); // JUSTIFIED: test fixture
        let child: i32 = String::from_utf8_lossy(&out.stdout).trim().parse().expect("test: ps prints a nice value"); // JUSTIFIED: test fixture
        let parent = unsafe { libc::getpriority(libc::PRIO_PROCESS, 0) };
        assert!(child > parent, "child nice {child} is below parent nice {parent}");
    }
}
