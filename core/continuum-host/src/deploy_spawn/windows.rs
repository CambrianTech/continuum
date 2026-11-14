//! Windows implementation of deploy consumer spawning.
//!
//! First checks the maintenance startup permit, then tries the registered scheduler task
//! (via `probe`). If no task is available, falls back to a breakaway spawn with creation flags.

use std::io::Seek;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use super::{SpawnPlan, Spawned, ProbeFn, PermitCheck};

pub fn spawn(plan: &SpawnPlan, probe: ProbeFn, permit_check: PermitCheck) -> Result<Spawned, String> {
    // A disabled task is not permission to bypass maintenance with a child.
    // Keep the shared permit across both Scheduler and detached spawn paths.
    permit_check(&plan.cwd).map_err(|e| format!("maintenance recovery pending: {e}"))?;

    // FIRST the registered consumer task: the scheduler runs it under the S4U
    // principal, outside this core's job object, and serializes it against its own
    // ten-minute firing (MultipleInstancesPolicy IgnoreNew) — one consumer at a time
    // on this box, whoever asked. `continuum install --supervisor` grants the user
    // RX on the task so this needs no elevation.
    if let Some(mode) = probe(&plan.cwd) {
        return Ok(Spawned { pid: None, mode, child: None });
    }

    // No task (or not runnable by this user): spawn the consumer ourselves, breaking
    // away from any job object first (a child of the core's job dies when the core's
    // reboot ends that job — card 82af11f5), then without when the job forbids it.
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

    let launch = |flags: u32| -> std::io::Result<std::process::Child> {
        let log = open_log(&plan.log)?;
        let err = log.try_clone()?;
        Command::new(&plan.cli)
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

/// Open a log file for append (create if missing, seek to end). Used by the spawner to feed
/// stdout/stderr of a detached consumer so the core's own logs are not polluted.
fn open_log(path: &Path) -> std::io::Result<std::fs::File> {
    use std::io::Seek;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = std::fs::OpenOptions::new().create(true).write(true).open(path)?;
    file.seek(std::io::SeekFrom::End(0))?;
    Ok(file)
}
