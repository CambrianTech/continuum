//! macOS: the child runs under `sandbox-exec` with the seatbelt profile.

use std::path::Path;

use super::{seatbelt_profile, AllowList, Enforcement};

pub(super) fn confine(home: &Path, list: &AllowList, program: &Path, args: &[std::ffi::OsString]) -> (tokio::process::Command, Enforcement) {
    let profile = seatbelt_profile(home, list);
    let mut cmd = tokio::process::Command::new("/usr/bin/sandbox-exec");
    cmd.arg("-p").arg(profile).arg(program).args(args);
    (cmd, Enforcement::Enforced { mechanism: "seatbelt" })
}
