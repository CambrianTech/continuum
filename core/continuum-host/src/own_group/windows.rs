//! Windows: no process group to leave; ownership by name is the whole story.

pub(super) fn own_group(_command: &mut tokio::process::Command) {}
