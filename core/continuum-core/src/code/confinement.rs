//! Confinement — what a citizen's hands may touch on this host.
//!
//! ONE policy per caller, enforced on EVERY child process the core spawns for her
//! (`code/shell`, `code/run`, cargo, git) and read by the file verbs that take a path
//! (`code/read` under a confined scope, `vision/look`). The operator is never
//! confined: the core runs as the operator, and the operator's own hands are the
//! operator's. A citizen's are not.
//!
//! Measured on the M5, 2026-10-08/09 (card d598c806): eleven SWE-bench citizens ran
//! 79 `find / -maxdepth N` and 52 `ls -la ~` / `find /Users/joel` through `code/shell`
//! as the operator's uid, with the whole home readable. `find /` crosses Photos,
//! iCloud Drive, Desktop and Documents, so macOS charged the reads to
//! continuum-core-server and raised privacy alerts ("we look like a virus"; Joel: "I
//! would argue we probably are"). Atlas's find returned the operator's resume
//! repository; Joaquin called `vision/look` on `~/Desktop/claude_desktop_config.json`.
//! A dozen minds held the operator's file privileges. On a grid where a citizen
//! learns, and larger bases follow, this has to be a SYSTEM, not a patch.
//!
//! ## The policy (platform-independent)
//!
//! Under the operator's home, a citizen may:
//! - read and write: her workspace, her citizen directory
//!   (`~/.continuum/citizens/peers/<her id>`), the shared caches and toolchain
//!   directories a build writes into (`~/.continuum/cache`, `~/.cargo`, `~/.rustup`,
//!   `~/.cache`, `~/.npm`, `~/.local`), and the process temp directory;
//! - read: the operator's dotfiles and dotdirs (`~/.zshrc` for her login shell, every
//!   toolchain and package manager lives in one), and `~/.continuum` (models,
//!   benchmarks, repos);
//! - never: the operator's secrets (`~/.ssh`, `~/.aws`, `~/.gnupg`, `~/.netrc`,
//!   `~/.kube`, `~/.docker`, `~/.azure`, `~/.config/gcloud`, `~/.pypirc`, `~/.npmrc`,
//!   `~/.continuum/config.env`, `~/.airc`), ANOTHER citizen's directory, or the
//!   operator's own files (Desktop, Documents, Pictures, Library, Development, ...:
//!   everything under the home that is not a dotfile).
//!
//! Outside the home the host is readable (system, toolchains under `/opt`, `/usr`)
//! and only the temp directory is writable.
//!
//! ## The enforcers
//!
//! The policy MATERIALIZES to an allow-list ([`AllowList`]) by reading the home's
//! dot entries once per spawn, so a deny-free mechanism (Linux landlock) and a
//! deny-capable one (macOS seatbelt) enforce the same roots:
//! - **macOS**: `sandbox-exec` with a rendered seatbelt profile ([`seatbelt_profile`]).
//!   Deprecated in name, enforced in fact (verified on macOS 26.5: a denied `ls
//!   ~/Pictures` is "Operation not permitted").
//! - **Linux**: landlock, applied in the child between fork and exec; in-kernel, no
//!   binary dependency. Kernels before 5.13 report [`Enforcement::Unavailable`].
//! - **Windows**: no per-process filesystem sandbox exists at this level; a citizen's
//!   child runs unconfined and EVERY spawn says so by probe
//!   (`shell.confinement.unavailable`). The 5090 needs a different mechanism (a
//!   citizen OS account with ACLs): a card, not a silent gap.
//!
//! The materialized list is also the read scope the file verbs check
//! ([`AllowList::permits_read`]), so `code/read` and `vision/look` refuse by name
//! exactly what the shell would have been refused at the kernel.

use std::path::{Path, PathBuf};

/// Dot entries under the home a citizen must never reach, even though dotfiles are
/// readable in general: credentials and the core's own secrets. Relative to the home.
const SECRET_HOME_ENTRIES: &[&str] = &[
    ".ssh",
    ".aws",
    ".gnupg",
    ".netrc",
    ".kube",
    ".docker",
    ".azure",
    ".config/gcloud",
    ".pypirc",
    ".npmrc",
    ".airc",
    ".continuum/config.env",
];

/// Dot entries under the home a build or an install writes into, shared by every
/// citizen on the host (the ONE cargo cache: see `ShellSession::new`).
const WRITABLE_HOME_ENTRIES: &[&str] = &[
    ".continuum/cache",
    ".cargo",
    ".rustup",
    ".cache",
    ".npm",
    ".local",
];

/// What the file verbs and the enforcers agree a citizen may touch: absolute,
/// canonical roots. Produced once per spawn by [`Confinement::materialize`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowList {
    /// Roots she may read AND write (her workspace, her citizen dir, caches, temp).
    pub write: Vec<PathBuf>,
    /// Roots she may read only (dotfiles, `~/.continuum` minus the exclusions).
    pub read: Vec<PathBuf>,
    /// Roots carved OUT of the above: another citizen's directory, a secret. Last
    /// word on a deny-capable enforcer; on an allow-only enforcer they are simply
    /// not granted (the allow roots are enumerated beneath them).
    pub deny: Vec<PathBuf>,
}

impl AllowList {
    /// Whether `path` (absolute, canonical) is readable under this list. A write root
    /// is hers outright (her citizen directory sits BENEATH the denied peers root, and
    /// wins); a read root is hers unless a deny root beneath it carves the path out.
    pub fn permits_read(&self, path: &Path) -> bool {
        if self.write.iter().any(|r| path.starts_with(r)) {
            return true;
        }
        self.read.iter().any(|r| path.starts_with(r)) && !self.deny.iter().any(|d| path.starts_with(d))
    }
}

/// The policy for one caller. Built by the workspace authority (`ensure_shell`,
/// `code/run`) from the same facts the engine roots on; nothing here reads a
/// params field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confinement {
    /// The operator's home: the boundary the policy is written against.
    home: PathBuf,
    /// Her workspace root, canonical (the engine's root).
    workspace: PathBuf,
    /// Her citizen directory under `~/.continuum/citizens/peers/<id>` when the caller
    /// is a citizen with one; the operator's own tools have none.
    citizen_dir: Option<PathBuf>,
}

impl Confinement {
    /// A citizen's policy: `who` is the caller id the workspace is rooted for (a peer
    /// id when it parses as one), `workspace` her engine root.
    pub fn for_caller(home: &Path, who: &str, workspace: &Path) -> Self {
        let citizen_dir = who
            .parse::<uuid::Uuid>()
            .ok()
            .map(|id| crate::identity::citizen_peer_dir(&home.join(".continuum"), crate::identity::PeerId::from_uuid(id)));
        Self { home: home.to_path_buf(), workspace: workspace.to_path_buf(), citizen_dir }
    }

    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    /// The allow-list for THIS spawn: the home's dot entries are enumerated now, so a
    /// toolchain installed since the last spawn is in. One `read_dir` of the home.
    pub fn materialize(&self) -> AllowList {
        let home = &self.home;
        let mut write: Vec<PathBuf> = vec![self.workspace.clone()];
        write.extend(self.citizen_dir.iter().cloned());
        write.extend(WRITABLE_HOME_ENTRIES.iter().map(|e| home.join(e)));
        write.push(std::env::temp_dir());
        // The host's temp root beneath the per-user temp dir (`/private/var/folders/..`
        // on macOS, `/tmp` on Linux): `code/run` and the compilers write there.
        for root in ["/tmp", "/private/tmp"] {
            let root = PathBuf::from(root);
            if root.is_dir() {
                write.push(root);
            }
        }

        let mut read: Vec<PathBuf> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(home) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.') && name != ".continuum" {
                    read.push(entry.path());
                }
            }
        }
        // `~/.continuum`, minus the citizens' directories (hers is a write root above)
        // and the core's secrets: enumerated one level down so an allow-only enforcer
        // never has to express "all but".
        let continuum = home.join(".continuum");
        if let Ok(entries) = std::fs::read_dir(&continuum) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                match name.as_ref() {
                    "citizens" => {
                        if let Ok(sub) = std::fs::read_dir(entry.path()) {
                            read.extend(sub.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n != "peers")));
                        }
                    }
                    "cache" | "config.env" => {}
                    _ => read.push(entry.path()),
                }
            }
        }

        let mut deny: Vec<PathBuf> = SECRET_HOME_ENTRIES.iter().map(|e| home.join(e)).collect();
        deny.push(continuum.join("citizens").join("peers"));

        canonical_all(&mut write);
        canonical_all(&mut read);
        canonical_all(&mut deny);
        // Her own directory sits BENEATH the denied peers root: it is a write root, and a
        // write root wins (`permits_read`); the seatbelt profile re-grants it after the
        // deny; landlock never granted the peers root at all.
        AllowList { write, read, deny }
    }

    /// Her citizen directory, canonical, when she has one.
    fn own_citizen_dir(&self) -> Option<PathBuf> {
        self.citizen_dir.as_ref().and_then(|d| d.canonicalize().ok())
    }
}

fn canonical_all(paths: &mut Vec<PathBuf>) {
    let mut seen = std::collections::BTreeSet::new();
    paths.retain_mut(|p| match p.canonicalize() {
        Ok(c) => {
            *p = c;
            seen.insert(p.clone())
        }
        Err(_) => false, // an absent root grants nothing
    });
}

/// What the enforcer did for one spawn: the receipt every spawn probe carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Enforcement {
    /// The child is confined by the named mechanism.
    Enforced { mechanism: &'static str },
    /// This host cannot confine a child at this level; the child runs as the operator
    /// and the reason is on the probe. Never silent.
    Unavailable { reason: String },
}

impl Enforcement {
    pub fn is_enforced(&self) -> bool {
        matches!(self, Self::Enforced { .. })
    }
}

/// The two operation classes every rule is written in. NEVER the `file*` wildcard:
/// measured on macOS 26.5, a `(deny file* …)` written AFTER an `(allow file-read* …)`
/// on the same subpath does not win — an operation-specific rule beats the wildcard
/// class regardless of order, and `~/.ssh/id` read through. Last-match-wins holds only
/// between rules of the SAME operation, so deny and allow are always written as the
/// same pair.
const SEATBELT_OPS: [&str; 2] = ["file-read*", "file-write*"];

/// Render the seatbelt profile (macOS `sandbox-exec -p`) for an allow-list. Within an
/// operation, rules are evaluated last-match-wins, so the order is: allow the host, deny
/// the home, grant the roots, deny the carve-outs, re-grant her own directory beneath
/// them.
pub fn seatbelt_profile(home: &Path, list: &AllowList, own_citizen_dir: Option<&Path>) -> String {
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
    if let Some(own) = own_citizen_dir {
        rule(&mut out, "allow", &SEATBELT_OPS, own);
    }
    out
}

fn sbpl_string(p: &Path) -> String {
    p.display().to_string().replace('\\', "\\\\").replace('"', "\\\"")
}

/// THE one way a child is spawned for a caller: confined when `confinement` is hers,
/// the operator's own process when `None`. Every confined spawn carries its receipt as
/// a probe (`shell.confinement` / `shell.confinement.unavailable`), so an unconfined
/// citizen child on a host without a sandbox is a line in the stream, never a silent gap.
pub fn command_for(
    confinement: Option<&Confinement>,
    who: &str,
    program: &Path,
    args: &[std::ffi::OsString],
) -> tokio::process::Command {
    let Some(confinement) = confinement else {
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(args);
        return cmd;
    };
    let (cmd, receipt) = confined_command(confinement, program, args);
    match &receipt {
        Enforcement::Enforced { mechanism } => crate::probe!(
            class = "shell.confinement",
            who = %who,
            mechanism = %mechanism,
            program = %program.display(),
            workspace = %confinement.workspace().display(),
            "a citizen's child is confined to her roots"
        ),
        Enforcement::Unavailable { reason } => crate::probe!(
            class = "shell.confinement.unavailable",
            who = %who,
            reason = %reason,
            program = %program.display(),
            workspace = %confinement.workspace().display(),
            "a citizen's child runs UNCONFINED on this host"
        ),
    }
    cmd
}

/// Confine `cmd` for this spawn: on macOS the program is re-wrapped under
/// `sandbox-exec`; on Linux a landlock ruleset is applied in the child. Returns the
/// receipt for the spawn probe. `program` and `args` are what the caller was about to
/// run; the returned command is what it runs instead.
pub fn confined_command(
    confinement: &Confinement,
    program: &Path,
    args: &[std::ffi::OsString],
) -> (tokio::process::Command, Enforcement) {
    let list = confinement.materialize();
    confined_command_with(confinement, &list, program, args)
}

#[cfg(target_os = "macos")]
fn confined_command_with(
    confinement: &Confinement,
    list: &AllowList,
    program: &Path,
    args: &[std::ffi::OsString],
) -> (tokio::process::Command, Enforcement) {
    let own = confinement.own_citizen_dir();
    let profile = seatbelt_profile(&confinement.home, list, own.as_deref());
    let mut cmd = tokio::process::Command::new("/usr/bin/sandbox-exec");
    cmd.arg("-p").arg(profile).arg(program).args(args);
    (cmd, Enforcement::Enforced { mechanism: "seatbelt" })
}

#[cfg(target_os = "linux")]
fn confined_command_with(
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

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn confined_command_with(
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake home OUTSIDE the process temp dir: temp is a write root for every
    /// citizen (code/run lives there), so a home under it would be wholly permitted
    /// and the tests would pass for the wrong reason (they did, first run).
    fn home_with(entries: &[&str]) -> tempfile::TempDir {
        let outside_temp = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let home = tempfile::Builder::new()
            .prefix(".confinement-test-home-")
            .tempdir_in(outside_temp)
            .expect("test: home");
        for e in entries {
            let p = home.path().join(e);
            if e.ends_with('/') {
                std::fs::create_dir_all(&p).expect("test: dir");
            } else {
                std::fs::create_dir_all(p.parent().expect("test: parent")).expect("test: parent dir");
                std::fs::write(&p, "x").expect("test: file");
            }
        }
        home
    }

    // what this catches (card d598c806): the policy granting a citizen the operator's
    // files, secrets or another citizen's directory, or refusing her own workspace,
    // her own citizen directory, the operator's toolchains or the shared caches.
    #[test]
    fn a_citizens_allow_list_holds_her_roots_and_nothing_of_the_operators() {
        let me = uuid::Uuid::from_u128(7);
        let other = uuid::Uuid::from_u128(8);
        let home = home_with(&[
            ".zshrc",
            ".cargo/registry/",
            ".ssh/id_ed25519",
            ".continuum/config.env",
            ".continuum/models/",
            ".continuum/cache/cargo-target/",
            ".continuum/citizens/humans/",
            &format!(".continuum/citizens/peers/{me}/workspace/"),
            &format!(".continuum/citizens/peers/{other}/workspace/"),
            "Pictures/holiday.jpg",
            "Documents/resume/.git/",
            "Development/continuum/",
        ]);
        let h = home.path().canonicalize().expect("test: canonical");
        let ws = h.join(".continuum/citizens/peers").join(me.to_string()).join("workspace");
        let c = Confinement::for_caller(&h, &me.to_string(), &ws);
        let list = c.materialize();
        let ok = |rel: &str| list.permits_read(&h.join(rel));
        assert!(ok(&format!(".continuum/citizens/peers/{me}/workspace/src.rs")), "her workspace");
        assert!(ok(&format!(".continuum/citizens/peers/{me}/experience.jsonl")), "her citizen dir");
        assert!(ok(".zshrc"), "her login shell's profile");
        assert!(ok(".cargo/registry/index"), "the toolchain");
        assert!(ok(".continuum/models/x.gguf"), "the models");
        assert!(ok(".continuum/cache/cargo-target/debug"), "the shared build cache");
        assert!(ok(".continuum/citizens/humans/joel"), "citizens that are not peers");
        assert!(!ok(&format!(".continuum/citizens/peers/{other}/workspace/src.rs")), "ANOTHER citizen");
        assert!(!ok(".ssh/id_ed25519"), "a secret");
        assert!(!ok(".continuum/config.env"), "the core's secrets");
        assert!(!ok("Pictures/holiday.jpg"), "the operator's photos");
        assert!(!ok("Documents/resume/.git/HEAD"), "the operator's resume");
        assert!(!ok("Development/continuum/Cargo.toml"), "the operator's own checkout");
        assert!(list.write.contains(&ws), "her workspace is writable");
        assert!(list.write.iter().any(|w| w.ends_with(".cargo")), "a build writes the registry");
        assert!(!list.write.iter().any(|w| w.ends_with(".zshrc") || w.ends_with("models")), "dotfiles and models are read-only");
    }

    // what this catches: the seatbelt profile granting before it denies the home, or
    // denying the citizens root after granting hers (last match wins, so order IS the
    // policy), or a profile string that `sandbox-exec` would reject.
    #[test]
    fn the_seatbelt_profile_orders_deny_home_then_grants_then_carve_outs_then_her_own() {
        let me = uuid::Uuid::from_u128(7);
        let home = home_with(&[".zshrc", &format!(".continuum/citizens/peers/{me}/workspace/"), ".ssh/"]);
        let h = home.path().canonicalize().expect("test: canonical");
        let ws = h.join(".continuum/citizens/peers").join(me.to_string()).join("workspace");
        let c = Confinement::for_caller(&h, &me.to_string(), &ws);
        let list = c.materialize();
        let own = c.own_citizen_dir().expect("test: her dir");
        let p = seatbelt_profile(&h, &list, Some(&own));
        let at = |needle: &str| p.find(needle).unwrap_or_else(|| panic!("profile lacks {needle}:\n{p}"));
        let deny_home = at(&format!("(deny file-read* (subpath \"{}\"))", h.display()));
        let grant_ws = at(&format!("(allow file-write* (subpath \"{}\"))", ws.display()));
        let deny_peers = at(&format!("(deny file-read* (subpath \"{}\"))", h.join(".continuum/citizens/peers").display()));
        let deny_ssh = at(&format!("(deny file-read* (subpath \"{}\"))", h.join(".ssh").display()));
        let grant_own = p.rfind(&format!("(allow file-write* (subpath \"{}\"))", own.display())).expect("her own dir granted last");
        assert!(p.starts_with("(version 1)\n(allow default)\n"));
        assert!(deny_home < grant_ws && grant_ws < deny_peers && deny_peers < grant_own, "{p}");
        assert!(deny_ssh > grant_ws, "a secret is denied after the general grants");
        assert!(!p.contains("file* "), "never the wildcard class: a deny in it loses to an earlier specific allow\n{p}");
    }

    // what this catches (the M5 incident, kernel-level): with the profile applied, a
    // child reading the operator's Pictures is refused by the OS, while her workspace
    // and a dotfile read fine. The enforcer is the real `sandbox-exec`, not a mock.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn on_macos_the_child_cannot_read_outside_her_roots() {
        let me = uuid::Uuid::from_u128(7);
        let home = home_with(&[".zshrc", &format!(".continuum/citizens/peers/{me}/workspace/hello.txt"), "Pictures/holiday.jpg", ".ssh/id"]);
        let h = home.path().canonicalize().expect("test: canonical");
        let ws = h.join(".continuum/citizens/peers").join(me.to_string()).join("workspace");
        let c = Confinement::for_caller(&h, &me.to_string(), &ws);
        let script = format!(
            "cat '{ws}/hello.txt' && cat '{h}/.zshrc' && (cat '{h}/Pictures/holiday.jpg' && echo LEAK-PICTURES); (cat '{h}/.ssh/id' && echo LEAK-SSH); (ls '{h}' && echo LEAK-HOME); echo done",
            ws = ws.display(),
            h = h.display()
        );
        let (mut cmd, receipt) = confined_command(&c, Path::new("/bin/sh"), &["-c".into(), script.into()]);
        assert!(receipt.is_enforced(), "{receipt:?}");
        let out = cmd.output().await.expect("test: spawn");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stdout.contains("done"), "stdout={stdout} stderr={stderr}");
        assert!(stdout.starts_with("x"), "her workspace reads: stdout={stdout} stderr={stderr}");
        assert!(!stdout.contains("LEAK-PICTURES"), "the operator's photos: stdout={stdout}");
        assert!(!stdout.contains("LEAK-SSH"), "a secret: stdout={stdout}");
        assert!(!stdout.contains("LEAK-HOME"), "listing the home: stdout={stdout}");
        assert!(stderr.contains("Operation not permitted"), "the refusal is the OS's: stderr={stderr}");
    }
}
