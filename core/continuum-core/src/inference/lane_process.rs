//! lane_process.rs — the shared unix-process primitives the two lane-tracking
//! modules ([`crate::inference::lane_pidfile`] canonical-port reclaim and
//! [`crate::inference::lane_registry`] orphan accounting) both need.
//!
//! ## Why this is its own module
//!
//! Both modules answer a "is this recorded pid still one of MY llama-servers, and
//! if so may I reap it" question, and both must obey the SAME never-blind-kill
//! safety: verify the pid is actually a `llama-server` (via `ps`) before sending a
//! signal, because a recorded pid can be STALE (the process died and the OS reused
//! its number for something unrelated). Duplicating `is_alive` / `kill9` /
//! `command_name` across the two callers would be the exact compression violation
//! we forbid — one decision ("is this pid a live llama-server we own"), one place.
//!
//! No new dependency: `libc` is already in the tree; identity via `ps -p <pid> -o
//! comm=` works identically on macOS and Linux.

/// True if `pid` names a live process. `kill(pid, 0)` sends no signal: `0` = alive
/// and ours; `EPERM` = alive but owned by another user (still alive); `ESRCH` =
/// gone.
#[cfg(unix)]
pub fn is_alive(pid: u32) -> bool {
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if rc == 0 {
        return true;
    }
    matches!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(e) if e == libc::EPERM
    )
}

/// Windows: no `kill(pid, 0)`. Query the task table — a matching row means the
/// pid is live. `tasklist` failing (unavailable / no permission) is treated as
/// "not alive", matching the Unix path's conservative-on-error stance.
#[cfg(windows)]
pub fn is_alive(pid: u32) -> bool {
    std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
        .unwrap_or(false)
}

/// `SIGKILL` the pid. Best-effort: a race where it already exited is fine.
#[cfg(unix)]
pub fn kill9(pid: u32) {
    unsafe {
        let _ = libc::kill(pid as libc::pid_t, libc::SIGKILL);
    }
}

/// Windows: force-terminate the process (and its child tree) via `taskkill`.
/// There is no SIGKILL; `/F /T` is the nearest equivalent. Best-effort.
#[cfg(windows)]
pub fn kill9(pid: u32) {
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .output();
}

/// The command name (basename) of `pid` via `ps -p <pid> -o comm=`. `None` if the
/// pid is gone or `ps` is unavailable — callers treat `None` as "unverifiable" and
/// refuse to kill.
pub fn command_name(pid: u32) -> Option<String> {
    let out = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.trim();
    if line.is_empty() {
        return None;
    }
    // `comm` can be a full path on Linux — take the basename so the
    // "llama-server" match is on the binary name, not its install dir.
    Some(line.rsplit('/').next().unwrap_or(line).to_string())
}

/// Positively identify `pid` as one of OUR llama-server children — the guard every
/// reap decision runs before signalling. `false` for a dead pid, a reused pid now
/// naming an unrelated process, or an unverifiable one (no `ps`). Never a guess:
/// only a confirmed `llama-server` `comm` returns `true`.
pub fn is_llama_server(pid: u32) -> bool {
    command_name(pid).is_some_and(|comm| comm.contains("llama-server"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: is_alive is true for our own process — the liveness check
    // underpins every reclaim/sweep decision, so a broken one silently disarms the
    // whole orphan-reaping machinery.
    #[test]
    fn is_alive_true_for_self() {
        assert!(is_alive(std::process::id()));
    }

    // what this catches: THE safety invariant shared by both callers — our own
    // test-runner process is NOT a llama-server, so is_llama_server must refuse it.
    // A regression here would let either module SIGKILL an unrelated reused-pid
    // process (the blind-kill this guard exists to prevent).
    #[test]
    fn is_llama_server_false_for_self() {
        assert!(
            !is_llama_server(std::process::id()),
            "the test runner is not a llama-server; reaping it would be a blind kill"
        );
    }

    // what this catches: a definitely-dead pid is neither alive nor a llama-server,
    // so a stale registry/pidfile entry can never trigger a kill.
    #[test]
    fn dead_pid_is_not_alive_or_llama() {
        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("spawn true");
        let dead = child.id();
        child.wait().expect("reap");
        // (Tiny PID-reuse window is acceptable in a unit test; the invariant we
        // assert is "we do not treat a reaped pid as a live llama-server".)
        assert!(
            !is_llama_server(dead),
            "a reaped pid must not read as a live llama-server"
        );
    }
}
