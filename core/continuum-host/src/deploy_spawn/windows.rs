//! Windows deploy consumer spawn — startup permit, schtasks probe, breakaway fallback.

use std::io::{Seek, SeekFrom};
use std::path::Path;
use std::process::Stdio;

use super::{ProbeFn, SpawnPlan, Spawned};

/// What a caller supplies to check whether maintenance mode blocks deployment — returns `Ok(())` when
/// the permit is acquired (deployment may proceed) or `Err` when maintenance recovery evidence is pending.
pub type PermitCheck = fn(&Path) -> Result<(), String>;

fn open_log(path: &Path) -> std::io::Result<std::fs::File> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = std::fs::OpenOptions::new().create(true).write(true).open(path)?;
    file.seek(SeekFrom::End(0))?;
    Ok(file)
}

/// Windows implementation of the deploy spawn. Checks maintenance recovery first (a disabled task is not
/// permission to bypass with a child), then tries the registered scheduler task via `probe`, and falls back
/// to a breakaway direct spawn when no task is available or the user lacks RX on it.
pub(super) fn spawn(plan: &SpawnPlan, probe: ProbeFn) -> Result<Spawned, String> {
    // Acquire maintenance permit — unfinished recovery evidence blocks deployment entirely.
    crate::deploy_spawn::os::acquire_deploy_permit(&plan.cwd)?;

    // FIRST the registered consumer task: the scheduler runs it under the S4U principal, outside this
    // core's job object, and serializes it against its own ten-minute firing — one consumer at a time
    // on this box. `continuum install --supervisor` grants the user RX on the task so this needs no elevation.
    if let Some(mode) = probe(&plan.cwd) {
        return Ok(Spawned { pid: None, mode, child: None });
    }

    // No task (or not runnable by this user): spawn the consumer ourselves, breaking away from any job
    // object first (a child of the core's job dies when the core's reboot ends that job — card 82af11f5),
    // then without when the job forbids it.
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

    let launch = |flags: u32| -> std::io::Result<std::process::Child> {
        let log = open_log(&plan.log)?;
        let err = log.try_clone()?;
        use std::os::windows::process::CommandExt;
        std::process::Command::new(&plan.cli)
            .args(&plan.args)
            .current_dir(&plan.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(err))
            .creation_flags(flags)
            .spawn()
    };

    let (child, how) = match launch(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB) {
        Ok(c) => (c, "breakaway"),
        Err(e) if e.raw_os_error() == Some(5) => (
            launch(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
                .map_err(|e| format!("cannot spawn {} {}: {e}", plan.cli.display(), plan.args.join(" ")))?,
            "child-of-job (breakaway denied)",
        ),
        Err(e) => return Err(format!("cannot spawn {} {}: {e}", plan.cli.display(), plan.args.join(" "))),
    };

    Ok(Spawned {
        pid: Some(child.id()),
        mode: format!("{} ({})", plan.args.join(" "), how),
        child: Some(child),
    })
}

/// The OS-specific permit check — delegates to `continuum_cli_lifecycle::supervisor_install::maintenance`.
#[cfg(windows)]
fn acquire_deploy_permit(cwd: &Path) -> Result<(), String> {
    use continuum_cli_lifecycle::supervisor_install::maintenance::StartupPermit;
    StartupPermit::acquire_deploy(cwd).map_err(|e| format!("maintenance recovery is pending: {e}"))
}
