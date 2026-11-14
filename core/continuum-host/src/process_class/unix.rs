pub(super) fn below_the_foreground(cmd: &mut tokio::process::Command) {
    // SAFETY: setpriority is async-signal-safe and touches only the forked child, before exec.
    unsafe {
        cmd.pre_exec(|| {
            // A failure keeps the parent's class: the child still runs, at the foreground's priority.
            libc::setpriority(libc::PRIO_PROCESS, 0, libc::PRIO_MAX);
            Ok(())
        });
    }
}
