//! macOS: the seatbelt adapter. The profile renderer is pure (rendered and tested on every
//! platform); the spawn under `sandbox-exec` exists on macOS only.

use std::path::Path;

use super::AllowList;
#[cfg(target_os = "macos")]
use super::{Confinement, Enforcement};

/// The two operation classes every rule is written in. NEVER the `file*` wildcard:
/// measured on macOS 26.5, a `(deny file* …)` written AFTER an `(allow file-read* …)`
/// on the same subpath does not win — an operation-specific rule beats the wildcard
/// class regardless of order, and `~/.ssh/id` read through. Last-match-wins holds only
/// between rules of the SAME operation, so deny and allow are always written as the
/// same pair.
const SEATBELT_OPS: [&str; 2] = ["file-read*", "file-write*"];

/// Render the seatbelt profile (macOS `sandbox-exec -p`) for an allow-list. Within an
/// operation, rules are evaluated last-match-wins, so the order is: allow the host, deny
/// the home, grant every root, deny the carve-outs last. The carve-outs are already
/// absent from the granted roots ([`carve`]); writing them again is the belt to that
/// brace. Nothing is ever granted beneath a deny, so order never cuts against a root of
/// hers (Cormac on #4882: a first draft denied `~/.airc` and re-granted only her citizen
/// dir after it, refusing her own card worktree under `~/.airc/worktrees`).
pub fn seatbelt_profile(home: &Path, list: &AllowList) -> String {
    let mut out = String::from("(version 1)\n(allow default)\n");
    let rule = |out: &mut String, verdict: &str, ops: &[&str], root: &Path| {
        for op in ops {
            out.push_str(&format!("({verdict} {op} (subpath \"{}\"))\n", sbpl_string(root)));
        }
    };
    rule(&mut out, "deny", &SEATBELT_OPS, home);
    out.push_str(&format!("(allow file-read-metadata (literal \"{}\"))\n", sbpl_string(home)));
    // Path lookup needs the ancestors of a granted root; subpath grants cover
    // descendants, the ancestors get metadata only.
    for root in list.read.iter().chain(list.write.iter()) {
        for ancestor in root.ancestors().skip(1).take_while(|a| a.starts_with(home) && *a != home) {
            out.push_str(&format!("(allow file-read-metadata (literal \"{}\"))\n", sbpl_string(ancestor)));
        }
    }
    for root in &list.read {
        rule(&mut out, "allow", &SEATBELT_OPS[..1], root);
    }
    for root in &list.write {
        rule(&mut out, "allow", &SEATBELT_OPS, root);
    }
    for root in &list.deny {
        rule(&mut out, "deny", &SEATBELT_OPS, root);
    }
    out
}

fn sbpl_string(p: &Path) -> String {
    p.display().to_string().replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(target_os = "macos")]
pub(super) fn confined_command_with(
    confinement: &Confinement,
    list: &AllowList,
    program: &Path,
    args: &[std::ffi::OsString],
) -> (tokio::process::Command, Enforcement) {
    let profile = seatbelt_profile(&confinement.home, list);
    let mut cmd = tokio::process::Command::new("/usr/bin/sandbox-exec");
    cmd.arg("-p").arg(profile).arg(program).args(args);
    (cmd, Enforcement::Enforced { mechanism: "seatbelt" })
}
