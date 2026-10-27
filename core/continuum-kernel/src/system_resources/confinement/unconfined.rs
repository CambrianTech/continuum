//! Every platform without a per-process filesystem sandbox adapter yet (Windows today):
//! the child runs as the operator, and the receipt says so on every spawn.

use std::path::Path;

use super::{AllowList, Confinement, Enforcement};

pub(super) fn confined_command_with(
    _confinement: &Confinement,
    _list: &AllowList,
    program: &Path,
    args: &[std::ffi::OsString],
) -> (tokio::process::Command, Enforcement) {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args);
    (
        cmd,
        Enforcement::Unavailable {
            reason: "no per-process filesystem sandbox on this platform: the child runs as the operator (a citizen OS account with ACLs is the Windows mechanism, carded)".into(),
        },
    )
}
