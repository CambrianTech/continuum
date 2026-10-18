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
    // A directory a carve-out was cut from is listable itself (`literal`, not `subpath`):
    // nothing beneath it is granted by this rule.
    for dir in &list.listable {
        out.push_str(&format!("(allow file-read* (literal \"{}\"))\n", sbpl_string(dir)));
    }
    deny_credentials(&mut out, list);
    out
}

/// Escape a literal path fragment for an SBPL `#"..."` regex.
fn regex_escape(fragment: &str) -> String {
    fragment.chars().flat_map(|c| if ".^$*+?()[]{}|\\".contains(c) { vec!['\\', c] } else { vec![c] }).collect()
}

/// The profile for a HOST-reach resident: everything the operator's process may do, minus
/// the credential roots and scope keys, written as denials only. Deny-capable, so it needs
/// no carved grant: a directory she creates after her shell started is hers like any other.
pub fn seatbelt_host_profile(list: &AllowList) -> String {
    let mut out = String::from("(version 1)\n(allow default)\n");
    deny_credentials(&mut out, list);
    out
}

/// The carve-outs and scope keys, last: within an operation the last match wins.
fn deny_credentials(out: &mut String, list: &AllowList) {
    for root in &list.deny {
        for op in SEATBELT_OPS {
            out.push_str(&format!("(deny {op} (subpath \"{}\"))\n", sbpl_string(root)));
        }
    }
    // A scope's signing key wherever the scope sits (`<repo>/.airc/identity.key`).
    for suffix in super::CREDENTIAL_SUFFIXES {
        let pattern = format!("/{}$", regex_escape(suffix));
        for op in SEATBELT_OPS {
            out.push_str(&format!("(deny {op} (regex #\"{pattern}\"))\n"));
        }
    }
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
    let profile = if confinement.reaches_host() { seatbelt_host_profile(list) } else { seatbelt_profile(&confinement.home, list) };
    let mut cmd = tokio::process::Command::new("/usr/bin/sandbox-exec");
    cmd.arg("-p").arg(profile).arg(program).args(args);
    (cmd, Enforcement::Enforced { mechanism: "seatbelt" })
}
