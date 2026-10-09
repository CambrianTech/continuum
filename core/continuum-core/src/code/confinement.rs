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
//! - read and write: her workspace (wherever the workspace authority rooted it, a
//!   card worktree under `~/.airc/worktrees` included), her citizen directory
//!   (`~/.continuum/citizens/peers/<her id>`), the toolchain and cache directories a
//!   build writes into ([`WRITABLE_HOME_ENTRIES`]), and the process temp directory;
//! - read: the entries a login shell and a build read ([`READABLE_HOME_ENTRIES`]: her
//!   shell's rc files, `.gitconfig`, the package managers' homes, the non-dot
//!   toolchain installs like `miniconda3`), and `~/.continuum` (models, benchmarks,
//!   repos);
//! - never: anything else under the home. The dot entries are an ALLOWLIST, not a
//!   readable `~/.*` with a denylist (Cormac on #4882): a denylist missed
//!   `~/.config/gh/hosts.yml`, `~/.git-credentials`, `~/.claude`, `~/.codex`, the
//!   shell histories. A credential file INSIDE a granted directory is carved out by
//!   name ([`CARVED_OUT_HOME_ENTRIES`]: `~/.cache/huggingface/token`,
//!   `~/.config/git/credentials`, `~/.continuum/config.env`), as is every OTHER
//!   citizen's directory. The operator's own files (Desktop, Documents, Pictures,
//!   Library, Development, ...) are simply never granted.
//!
//! Outside the home the host is readable (system, toolchains under `/opt`, `/usr`)
//! and only the temp directory is writable.
//!
//! Not confined by this module: `code/git/*`, which runs fixed-argument git through
//! `git_bridge` at her checkout (add, commit, diff, apply, log, push, status: never a
//! command of hers), and the core's own workspace sync. Confining those spawns too is
//! part of the follow-up with the Windows mechanism.
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

/// Home entries a build or an install writes into, shared by every citizen on the
/// host (the ONE cargo cache: see `ShellSession::new`). Relative to the home. An
/// absent entry grants nothing.
const WRITABLE_HOME_ENTRIES: &[&str] = &[
    ".continuum/cache",
    ".cargo",
    ".rustup",
    ".local",
    ".npm",
    ".nvm",
    ".pyenv",
    ".conda",
    ".virtualenvs",
    ".cache/pip",
    ".cache/uv",
    ".cache/go-build",
    ".cache/pre-commit",
    ".cache/huggingface",
];

/// Home entries a login shell and a build READ: her shell's rc files, git's config,
/// the package managers' and version managers' homes, the non-dot toolchain installs
/// a developer keeps in the home. Relative to the home; an absent entry grants
/// nothing. Everything under the home that is not here, not writable and not
/// `~/.continuum` is not hers (the operator's secrets and files alike).
const READABLE_HOME_ENTRIES: &[&str] = &[
    ".zshrc",
    ".zprofile",
    ".zshenv",
    ".zlogin",
    ".bashrc",
    ".bash_profile",
    ".bash_login",
    ".profile",
    ".oh-my-zsh",
    ".zsh",
    ".p10k.zsh",
    ".gitconfig",
    ".gitignore_global",
    ".tool-versions",
    ".config/git",
    ".config/pip",
    ".config/uv",
    ".asdf",
    ".sdkman",
    ".volta",
    ".bun",
    ".deno",
    ".go",
    ".gem",
    ".m2",
    ".gradle",
    ".julia",
    ".rbenv",
    "miniconda3",
    "anaconda3",
    "miniforge3",
    "mambaforge",
    "go",
    "flutter",
];

/// Credential files INSIDE a granted directory: never hers, carved out by name on a
/// deny-capable enforcer and never granted on an allow-only one (the granted roots
/// are enumerated beneath them where that is needed). Relative to the home.
const CARVED_OUT_HOME_ENTRIES: &[&str] = &[
    ".cache/huggingface/token",
    ".config/git/credentials",
    ".continuum/config.env",
];

/// What the file verbs and the enforcers agree a citizen may touch: absolute,
/// canonical roots. Produced once per spawn by [`Confinement::materialize`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowList {
    /// Roots she may read AND write (her workspace, her citizen dir, caches, temp).
    pub write: Vec<PathBuf>,
    /// Roots she may read only (dotfiles, `~/.continuum` minus the exclusions).
    pub read: Vec<PathBuf>,
    /// Credential files carved OUT of a granted root ([`CARVED_OUT_HOME_ENTRIES`]).
    /// The last word everywhere: a deny-capable enforcer writes them after every
    /// grant; an allow-only enforcer never sees them, because a granted root that
    /// contains one is already replaced by its children minus the carve-out
    /// ([`carve`]). Another citizen's directory needs no carve-out: it is never
    /// granted (the citizens root is enumerated without `peers`).
    pub deny: Vec<PathBuf>,
}

impl AllowList {
    /// Whether `path` (absolute, canonical) is readable under this list: inside a
    /// granted root and not a carve-out. A carve-out is the last word, write root or
    /// not (`~/.cache/huggingface/token` sits inside a write root).
    pub fn permits_read(&self, path: &Path) -> bool {
        !self.deny.iter().any(|d| path.starts_with(d))
            && self.write.iter().chain(self.read.iter()).any(|r| path.starts_with(r))
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

    /// The allow-list for THIS spawn, from the host as it is now (a toolchain installed
    /// since the last spawn is in): the named home entries that exist, plus
    /// `~/.continuum` enumerated one level down so an allow-only enforcer never has to
    /// express "all but".
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

        let mut read: Vec<PathBuf> = READABLE_HOME_ENTRIES.iter().map(|e| home.join(e)).collect();
        // `~/.continuum`, minus the citizens' directories (hers is a write root above),
        // the shared cache (a write root above) and the core's secrets.
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

        let mut deny: Vec<PathBuf> = CARVED_OUT_HOME_ENTRIES.iter().map(|e| home.join(e)).collect();

        canonical_all(&mut write);
        canonical_all(&mut read);
        canonical_all(&mut deny);
        carve(&mut write, &deny);
        carve(&mut read, &deny);
        AllowList { write, read, deny }
    }

}

/// Replace every granted root that CONTAINS a carve-out by its children minus the
/// carve-out (down to the carve-out's parent), so an allow-only enforcer grants
/// `~/.cache/huggingface/hub` and never `~/.cache/huggingface/token`. A root equal to
/// a carve-out is dropped. Children that appear after this spawn are seen by the next.
fn carve(roots: &mut Vec<PathBuf>, deny: &[PathBuf]) {
    let mut out: Vec<PathBuf> = Vec::with_capacity(roots.len());
    let mut pending: Vec<PathBuf> = std::mem::take(roots);
    while let Some(root) = pending.pop() {
        if deny.iter().any(|d| d == &root) {
            continue;
        }
        let Some(inside) = deny.iter().find(|d| d.starts_with(&root) && *d != &root) else {
            out.push(root);
            continue;
        };
        // The child of `root` on the way to the carve-out descends; its siblings are
        // granted whole.
        let Some(next) = inside.strip_prefix(&root).ok().and_then(|rel| rel.components().next()).map(|c| root.join(c.as_os_str())) else {
            out.push(root);
            continue;
        };
        if let Ok(entries) = std::fs::read_dir(&root) {
            for child in entries.flatten().map(|e| e.path()) {
                if child == next {
                    pending.push(child);
                } else {
                    out.push(child);
                }
            }
        }
    }
    out.sort();
    out.dedup();
    *roots = out;
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
    let profile = seatbelt_profile(&confinement.home, list);
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

    // what this catches (card d598c806; Cormac on #4882): the policy granting a citizen
    // the operator's files, a credential (named or inside a granted directory), or
    // another citizen's directory; or refusing her own workspace (a card worktree under
    // `~/.airc`, a denied-by-omission dot entry), her citizen directory, the toolchains
    // or the shared caches.
    #[test]
    fn a_citizens_allow_list_holds_her_roots_and_nothing_of_the_operators() {
        let me = uuid::Uuid::from_u128(7);
        let other = uuid::Uuid::from_u128(8);
        let home = home_with(&[
            ".zshrc",
            ".zsh_history",
            ".gitconfig",
            ".git-credentials",
            ".cargo/registry/",
            ".cache/huggingface/token",
            ".cache/huggingface/hub/",
            ".cache/pip/",
            ".config/gh/hosts.yml",
            ".config/git/config",
            ".config/git/credentials",
            ".claude/credentials.json",
            ".ssh/id_ed25519",
            ".airc/worktrees/ab12cd34/src/",
            ".airc/identity/key",
            ".continuum/config.env",
            ".continuum/models/",
            ".continuum/cache/cargo-target/",
            ".continuum/citizens/humans/",
            &format!(".continuum/citizens/peers/{me}/workspace/"),
            &format!(".continuum/citizens/peers/{other}/workspace/"),
            "miniconda3/envs/",
            "Pictures/holiday.jpg",
            "Documents/resume/.git/",
            "Development/continuum/",
        ]);
        let h = home.path().canonicalize().expect("test: canonical");
        let card_worktree = h.join(".airc/worktrees/ab12cd34");
        let c = Confinement::for_caller(&h, &me.to_string(), &card_worktree);
        let list = c.materialize();
        let ok = |rel: &str| list.permits_read(&h.join(rel));
        // hers
        assert!(ok(".airc/worktrees/ab12cd34/src/lib.rs"), "her card worktree, under a dot entry that is not granted");
        assert!(ok(&format!(".continuum/citizens/peers/{me}/experience.jsonl")), "her citizen dir, under a peers root that is never granted whole");
        assert!(ok(".zshrc"), "her login shell's profile");
        assert!(ok(".gitconfig"), "git's config");
        assert!(ok(".cargo/registry/index"), "the toolchain");
        assert!(ok(".cache/pip/wheels"), "a package cache");
        assert!(ok(".cache/huggingface/hub/x.gguf"), "the model cache");
        assert!(ok(".config/git/config"), "git's config dir");
        assert!(ok("miniconda3/envs/py311"), "a non-dot toolchain install");
        assert!(ok(".continuum/models/x.gguf"), "the models");
        assert!(ok(".continuum/cache/cargo-target/debug"), "the shared build cache");
        assert!(ok(".continuum/citizens/humans/joel"), "citizens that are not peers");
        // never
        assert!(!ok(".airc/identity/key"), "airc's identity, beside her worktree");
        assert!(!ok(&format!(".continuum/citizens/peers/{other}/workspace/src.rs")), "ANOTHER citizen");
        assert!(!ok(".ssh/id_ed25519"), "a secret");
        assert!(!ok(".git-credentials"), "git credentials");
        assert!(!ok(".config/gh/hosts.yml"), "the GitHub token");
        assert!(!ok(".config/git/credentials"), "a credential inside a granted dir");
        assert!(!ok(".cache/huggingface/token"), "a credential inside a WRITE root");
        assert!(!ok(".claude/credentials.json"), "an agent's auth");
        assert!(!ok(".zsh_history"), "a shell history");
        assert!(!ok(".continuum/config.env"), "the core's secrets");
        assert!(!ok("Pictures/holiday.jpg"), "the operator's photos");
        assert!(!ok("Documents/resume/.git/HEAD"), "the operator's resume");
        assert!(!ok("Development/continuum/Cargo.toml"), "the operator's own checkout");
        // write vs read
        assert!(list.write.contains(&card_worktree), "her workspace is writable");
        assert!(list.write.iter().any(|w| w.ends_with(".cargo")), "a build writes the registry");
        assert!(!list.write.iter().any(|w| w.ends_with(".zshrc") || w.ends_with("models") || w.ends_with("miniconda3")), "rc files, models and installs are read-only");
    }

    // what this catches: the seatbelt profile granting before it denies the home, or a
    // carve-out written before the grant of the root that held it (it would lose: last
    // match wins within an operation), or a granted root still containing a carve-out,
    // or a `file*` rule (which loses to any earlier specific allow).
    #[test]
    fn the_seatbelt_profile_denies_the_home_then_grants_then_carves_out_last() {
        let me = uuid::Uuid::from_u128(7);
        let home = home_with(&[".zshrc", ".cache/huggingface/token", ".cache/huggingface/hub/", &format!(".continuum/citizens/peers/{me}/workspace/")]);
        let h = home.path().canonicalize().expect("test: canonical");
        let ws = h.join(".continuum/citizens/peers").join(me.to_string()).join("workspace");
        let c = Confinement::for_caller(&h, &me.to_string(), &ws);
        let list = c.materialize();
        let p = seatbelt_profile(&h, &list);
        let at = |needle: &str| p.find(needle).unwrap_or_else(|| panic!("profile lacks {needle}:\n{p}"));
        let deny_home = at(&format!("(deny file-read* (subpath \"{}\"))", h.display()));
        let read_zshrc = at(&format!("(allow file-read* (subpath \"{}\"))", h.join(".zshrc").display()));
        let grant_hub = at(&format!("(allow file-write* (subpath \"{}\"))", h.join(".cache/huggingface/hub").display()));
        let grant_ws = at(&format!("(allow file-write* (subpath \"{}\"))", ws.display()));
        let deny_token = at(&format!("(deny file-read* (subpath \"{}\"))", h.join(".cache/huggingface/token").display()));
        assert!(p.starts_with("(version 1)\n(allow default)\n"));
        assert!(deny_home < read_zshrc && read_zshrc < grant_ws && grant_hub < deny_token, "{p}");
        assert!(!p.contains(&format!("(subpath \"{}\"))", h.join(".cache/huggingface").display())), "the root holding a carve-out is granted by its children, never whole:\n{p}");
        assert!(!list.write.iter().chain(list.read.iter()).any(|r| r.ends_with("huggingface")), "same on the allow-only side");
        assert!(!p.contains("file* "), "never the wildcard class: a deny in it loses to an earlier specific allow\n{p}");
    }

    // what this catches (the M5 incident, kernel-level): with the profile applied, a
    // child reading the operator's Pictures is refused by the OS, while her workspace
    // and a dotfile read fine. The enforcer is the real `sandbox-exec`, not a mock.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn on_macos_the_child_cannot_read_outside_her_roots() {
        let me = uuid::Uuid::from_u128(7);
        // Her workspace is a card worktree under `~/.airc` (Cormac on #4882): a dot entry
        // the policy never grants, so only the write-root-wins ordering lets her in.
        let home = home_with(&[
            ".zshrc",
            ".airc/worktrees/ab12cd34/hello.txt",
            ".airc/identity/key",
            ".config/gh/hosts.yml",
            ".cache/huggingface/token",
            ".cache/huggingface/hub/model.bin",
            &format!(".continuum/citizens/peers/{me}/experience.jsonl"),
            "Pictures/holiday.jpg",
            ".ssh/id",
        ]);
        let h = home.path().canonicalize().expect("test: canonical");
        let ws = h.join(".airc/worktrees/ab12cd34");
        let c = Confinement::for_caller(&h, &me.to_string(), &ws);
        let leak = |rel: &str, tag: &str| format!("(cat '{}/{rel}' && echo LEAK-{tag}); ", h.display());
        let script = format!(
            "cat '{ws}/hello.txt' && cat '{h}/.zshrc' && cat '{h}/.cache/huggingface/hub/model.bin' && cat '{h}/.continuum/citizens/peers/{me}/experience.jsonl'; \
             {p}{s}{a}{g}{t}(ls '{h}' && echo LEAK-HOME); echo done",
            ws = ws.display(),
            h = h.display(),
            p = leak("Pictures/holiday.jpg", "PICTURES"),
            s = leak(".ssh/id", "SSH"),
            a = leak(".airc/identity/key", "AIRC-IDENTITY"),
            g = leak(".config/gh/hosts.yml", "GH-TOKEN"),
            t = leak(".cache/huggingface/token", "HF-TOKEN"),
        );
        let (mut cmd, receipt) = confined_command(&c, Path::new("/bin/sh"), &["-c".into(), script.into()]);
        assert!(receipt.is_enforced(), "{receipt:?}");
        let out = cmd.output().await.expect("test: spawn");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stdout.contains("done"), "stdout={stdout} stderr={stderr}");
        assert!(stdout.starts_with("xxxx"), "her worktree, her rc file, the model cache and her citizen dir read: stdout={stdout} stderr={stderr}");
        for tag in ["PICTURES", "SSH", "AIRC-IDENTITY", "GH-TOKEN", "HF-TOKEN", "HOME"] {
            assert!(!stdout.contains(&format!("LEAK-{tag}")), "{tag} leaked: stdout={stdout}");
        }
        assert!(stderr.contains("Operation not permitted"), "the refusal is the OS's: stderr={stderr}");
    }
}
