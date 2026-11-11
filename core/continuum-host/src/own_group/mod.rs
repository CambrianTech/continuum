//! A child process that owns its fate by name, not by the group it was born into (card 59052747).
//! Unix: the child leads its own process group, so a signal aimed at the parent's group (the stop
//! rail's `kill(-core_pid, SIGTERM)`, launchd reaping a job's group on exit) does not reach it.
//! Windows has no such group here; the lane registry and the ownership sweep decide its fate there.
//! Moved from the engine launcher's `#[cfg(unix)]` for `os_branches_in_logic`.

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod os;

/// Make the child spawned by `command` its own process group where the OS has them.
pub fn own_group(command: &mut tokio::process::Command) {
    os::own_group(command)
}
