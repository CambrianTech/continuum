//! Linux: the landlock adapter, applied in the child between fork and exec.

use std::path::{Path, PathBuf};

use super::{AllowList, Confinement, Enforcement};

pub(super) fn confined_command_with(
    _confinement: &Confinement,
    list: &AllowList,
    program: &Path,
    args: &[std::ffi::OsString],
) -> (tokio::process::Command, Enforcement) {
    use landlock::{Access, AccessFs, Ruleset, RulesetAttr, RulesetCreatedAttr, ABI};
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args);
    let abi = ABI::V2;
    let read: Vec<PathBuf> = ["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc", "/opt", "/proc", "/dev", "/sys", "/var", "/run", "/nix"]
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .chain(list.read.iter().cloned())
        .collect();
    let write: Vec<PathBuf> = list.write.clone();
    // landlock is allow-only: the deny roots are simply never granted (the read roots
    // were enumerated beneath them), so nothing here reads `list.deny`.
    let compatible = Ruleset::default().handle_access(AccessFs::from_all(abi)).is_ok();
    if !compatible {
        return (cmd, Enforcement::Unavailable { reason: "landlock: this kernel does not offer the filesystem ABI (needs 5.13+)".into() });
    }
    // SAFETY: the closure runs in the forked child before exec; it only builds and
    // applies a landlock ruleset (no allocation-sensitive state shared with the parent
    // is touched after fork beyond what landlock's own calls do).
    unsafe {
        cmd.pre_exec(move || {
            let ruleset = Ruleset::default()
                .handle_access(AccessFs::from_all(abi))
                .map_err(std::io::Error::other)?
                .create()
                .map_err(std::io::Error::other)?
                .add_rules(landlock::path_beneath_rules(&read, AccessFs::from_read(abi)))
                .map_err(std::io::Error::other)?
                .add_rules(landlock::path_beneath_rules(&write, AccessFs::from_all(abi)))
                .map_err(std::io::Error::other)?;
            ruleset.restrict_self().map_err(std::io::Error::other)?;
            Ok(())
        });
    }
    (cmd, Enforcement::Enforced { mechanism: "landlock" })
}
