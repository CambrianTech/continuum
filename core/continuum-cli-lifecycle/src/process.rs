//! The one launch policy for a child process the user never sees.
//!
//! Every background launch (a probe of `nvidia-smi` or `vulkaninfo`, the deploy tracker's
//! git and gh calls, an installer step, airc's autostart) is built here, so on Windows none
//! of them flashes a console window from a hidden service or the deploy consumer. Joel
//! asked for ONE launch policy; three hand-set flags had already drifted apart.

/// A command that never opens a console window on Windows, with stdin closed. A no-op
/// beyond stdin elsewhere.
/// True if `pid` names a live process: the ONE liveness check, shared by the deploy claim (here)
/// and the core's lane reaping (`inference::lane_process` re-exports it).
pub fn is_alive(pid: u32) -> bool {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    if pid == 0 {
        return false;
    }
    let target = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[target]), true, ProcessRefreshKind::nothing());
    sys.process(target).is_some()
}

pub fn quiet_command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    #[allow(unused_mut)]
    let mut command = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command.stdin(std::process::Stdio::null());
    command
}
