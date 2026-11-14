//! Unix implementation of deploy consumer spawning.
//!
//! Launches a detached process in its own group with stdout/stderr redirected to a log file.
//! No probe or permit callbacks are needed on this platform — they are stubbed by the caller.

use std::io::Seek;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use super::{SpawnPlan, Spawned, ProbeFn, PermitCheck};

pub fn spawn(plan: &SpawnPlan, _probe: ProbeFn, _permit_check: PermitCheck) -> Result<Spawned, String> {
    let log = open_log(&plan.log).map_err(|e| format!("cannot open {}: {e}", plan.log.display()))?;
    let err = log.try_clone().map_err(|e| format!("cannot clone the log handle: {e}"))?;

    // A NEW PROCESS GROUP: the reboot the consumer runs stops the core by signalling
    // the core's group (the start script's setsid), and the consumer must not be in it.
    let child = Command::new(&plan.cli)
        .args(&plan.args)
        .current_dir(&plan.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err))
        .process_group(0)
        .spawn()
        .map_err(|e| format!("cannot spawn {} {}: {e}", plan.cli.display(), plan.args.join(" ")))?;

    Ok(Spawned {
        pid: Some(child.id()),
        mode: format!("{} (new process group)", plan.args.join(" ")),
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
