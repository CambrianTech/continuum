//! Unix: the child leads a new process group (pgid = its pid).

pub(super) fn own_group(command: &mut tokio::process::Command) {
    command.process_group(0);
}
