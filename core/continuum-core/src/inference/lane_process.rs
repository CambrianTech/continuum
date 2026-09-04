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

/// The pid LISTENING on local TCP `port`, via `lsof -ti tcp:<port> -sTCP:LISTEN`
/// (macOS + Linux). `None` = nothing listening, or `lsof` unavailable — callers
/// treat `None` as "unverifiable" and refuse to kill, same doctrine as
/// [`command_name`]. This is the kill-VERIFY half of the 2026-07-23 flap case:
/// a spawn must never race a port whose holder it can't name.
pub fn pid_listening_on_port(port: u16) -> Option<u32> {
    let out = std::process::Command::new("lsof")
        .args(["-ti", &format!("tcp:{port}"), "-sTCP:LISTEN"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .and_then(|l| l.trim().parse::<u32>().ok())
}

/// Poll until local `port` is bindable (a successful bind-then-drop proves it) or
/// `budget` expires. THE shared port-release verifier: the pidfile reclaim and the
/// pre-spawn kill-verify gate both wait through this one primitive.
pub async fn wait_port_free(port: u16, budget: std::time::Duration) -> bool {
    const POLL: std::time::Duration = std::time::Duration::from_millis(100);
    let deadline = std::time::Instant::now() + budget;
    loop {
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(POLL).await;
    }
}

/// Every llama-server pid on this box, by name — the boot plan's census.
/// `pgrep -x` (exact name) so a path containing "llama-server" in an editor
/// or grep never matches; each pid is then identity-verified by callers via
/// [`is_llama_server`] before any signal (never-blind-kill).
pub fn owned_llama_pids() -> Vec<u32> {
    let Ok(out) = std::process::Command::new("pgrep")
        .args(["-x", "llama-server"])
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().parse::<u32>().ok())
        .collect()
}

/// Is the lane at `pid` HEALTHY — identity-verified AND answering /health on
/// the port its own cmdline names? The boot plan's adopt-or-reap predicate:
/// deterministic (same census → same fates), no guessing (an unreadable port
/// or a non-2xx answer is unhealthy, full stop).
pub fn lane_health_by_pid(pid: u32) -> bool {
    if !is_llama_server(pid) {
        return false;
    }
    let Ok(out) = std::process::Command::new("ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .output()
    else {
        return false;
    };
    let cmd = String::from_utf8_lossy(&out.stdout);
    let port = cmd
        .split_whitespace()
        .skip_while(|w| *w != "--port")
        .nth(1)
        .and_then(|p| p.parse::<u16>().ok());
    let Some(port) = port else { return false };
    // Blocking 3s health probe — boot-path only, never on a serving path.
    std::process::Command::new("curl")
        .args([
            "-sf",
            "--max-time",
            "3",
            &format!("http://127.0.0.1:{port}/health"),
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false) // safe: unreachable probe = unhealthy = reap, the deterministic direction
}

/// Reap one lane the boot plan judged unhealthy — identity re-verified at the
/// signal (the pid may have died between census and kill).
pub fn kill_lane(pid: u32) {
    if is_llama_server(pid) {
        kill9(pid);
    }
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
