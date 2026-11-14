//! Unix deploy consumer spawn — new process group, log redirect.

use std::io::{Seek, SeekFrom};
use std::path::Path;

use super::{ProbeFn, SpawnPlan, Spawned};

/// Open the child's log file for append: create parent dirs if needed, open with write+create,
/// seek to end so subsequent writes append. On Unix this is a simple `OpenOptions` call; what
/// differs by OS is only how the process itself is launched (`spawn`).
fn open_log(path: &Path) -> std::io::Result<std::fs::File> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = std::fs::OpenOptions::new().create(true).write(true).open(path)?;
    file.seek(SeekFrom::End(0))?;
    Ok(file)
}

/// Unix implementation of the deploy spawn. A new process group ensures the child survives when
/// the core's group is terminated (a reboot kills the core's group, and the consumer must not be in it).
pub(super) fn spawn(plan: &SpawnPlan, _probe: ProbeFn) -> Result<Spawned, String> {
    let log = open_log(&plan.log).map_err(|e| format!("cannot open {}: {e}", plan.log.display()))?;
    let err = log.try_clone().map_err(|e| format!("cannot clone the log handle: {e}"))?;

    use std::os::unix::process::CommandExt;

    let child = std::process::Command::new(&plan.cli)
        .args(&plan.args)
        .current_dir(&plan.cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(log))
        .stderr(std::process::Stdio::from(err))
        .process_group(0)
        .spawn()
        .map_err(|e| format!("cannot spawn {} {}: {e}", plan.cli.display(), plan.args.join(" ")))?;

    Ok(Spawned {
        pid: Some(child.id()),
        mode: format!("{} (new process group)", plan.args.join(" ")),
        child: Some(child),
    })
}
