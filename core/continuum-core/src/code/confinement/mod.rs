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
//! That is the WORKSPACE reach, for a caller from outside this node. This node's own
//! resident has HOST reach ([`Confinement::reaching_host`]): everything the operator's
//! process may touch EXCEPT the credential paths ([`CARVED_OUT_HOME_ENTRIES`], plus every
//! scope's signing key, [`CREDENTIAL_SUFFIXES`]) and other citizens' directories (Joel
//! 2026-10-17 "Don't restrict them"; 2026-10-18 "keep creds off-limits"). That boundary is
//! a denylist by decision, so the list names every store a token lives in on the fleet's
//! hosts. Seatbelt writes it as denials only; landlock grants the host carved around it.
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
    // `.local` by its toolchain parts, never whole: `~/.local/share/keyrings` is the
    // Linux secret store (Cormac on #4882).
    ".local/bin",
    ".local/lib",
    ".local/share/uv",
    ".local/share/pipx",
    ".local/share/pnpm",
    ".local/state/uv",
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

/// Credential paths under the home that no citizen reads or writes, whatever her
/// reach: carved out by name on a deny-capable enforcer and never granted on an
/// allow-only one (the granted roots are enumerated beneath them where that is needed).
/// Relative to the home.
///
/// For a workspace-reach citizen most of these are never granted anyway; they matter
/// where they sit INSIDE a granted root (`.cache/huggingface/token`,
/// `.cargo/credentials.toml`). For a HOST-reach resident (Joel 2026-10-18: residents
/// unrestricted, "keep creds off-limits") this list is the whole boundary, so it names
/// every store a token lives in on the fleet's hosts: a resident quotes tool output
/// into rooms, and a room reaches every enrolled peer across operators.
const CARVED_OUT_HOME_ENTRIES: &[&str] = &[
    ".local/share/keyrings",
    ".cache/huggingface/token",
    ".config/git/credentials",
    ".continuum/config.env",
    ".ssh",
    ".gnupg",
    ".aws",
    ".kube",
    ".netrc",
    ".git-credentials",
    ".npmrc",
    ".pypirc",
    ".docker/config.json",
    ".cargo/credentials",
    ".cargo/credentials.toml",
    ".config/gh",
    ".config/gcloud",
    ".config/github-copilot",
    ".claude",
    ".codex",
    ".airc/identity",
    ".airc/identity.key",
    ".zsh_history",
    ".bash_history",
    "Library/Keychains",
];

/// Credential files that sit wherever their scope does, not at one home path: every
/// airc project scope keeps its agent's signing key at `<repo>/.airc/identity.key`
/// (the deploy checkout's is an agent's), and a resident who read one could sign as
/// that agent. Matched by trailing path components: the file verbs check the suffix,
/// seatbelt denies it by regex. Landlock cannot express a pattern; no Linux node hosts
/// residents today.
const CREDENTIAL_SUFFIXES: &[&str] = &[".airc/identity.key"];

/// Whether `path` ends in a [`CREDENTIAL_SUFFIXES`] entry. One check for every consumer.
pub(crate) fn is_scoped_credential(path: &Path) -> bool {
    CREDENTIAL_SUFFIXES.iter().any(|suffix| path.ends_with(suffix))
}

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
    /// Directories a carve-out was cut out of: listable themselves (a `ls ~` works), with
    /// nothing beneath them granted by this entry. Their granted children are in `write`
    /// or `read`.
    pub listable: Vec<PathBuf>,
}

impl AllowList {
    /// Whether `path` (absolute, canonical) is readable under this list: inside a
    /// granted root and not a carve-out. A carve-out is the last word, write root or
    /// not (`~/.cache/huggingface/token` sits inside a write root).
    pub fn permits_read(&self, path: &Path) -> bool {
        !self.deny.iter().any(|d| path.starts_with(d))
            && !is_scoped_credential(path)
            && self.write.iter().chain(self.read.iter()).any(|r| path.starts_with(r))
    }
}

/// The policy for one caller. Built by the workspace authority (`ensure_shell`,
/// `code/run`) from the same facts the engine roots on; nothing here reads a
/// params field.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Confinement {
    /// The operator's home: the boundary the policy is written against.
    home: PathBuf,
    /// Her workspace root, canonical (the engine's root).
    workspace: PathBuf,
    /// Her citizen directory under `~/.continuum/citizens/peers/<id>` when the caller
    /// is a citizen with one; the operator's own tools have none.
    citizen_dir: Option<PathBuf>,
    continuum_root: PathBuf,
    reach: Reach,
}

/// How far a confined caller's hands reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Reach {
    /// Her workspace, her citizen dir, the toolchains and caches: a caller from outside
    /// this node.
    Workspace,
    /// The whole host except the credential paths and other citizens' directories: this
    /// node's own resident (Joel 2026-10-17 "Don't restrict them"; 2026-10-18 "keep creds
    /// off-limits").
    Host,
}

/// Produced by the bounded disk admission owner after validation and creation,
/// never from a command's session environment. Bound to the requesting policy.
pub struct CargoTargetGrant {
    path: PathBuf,
    policy: Confinement,
}

impl CargoTargetGrant {
    pub(crate) fn admitted(path: PathBuf, policy: Confinement) -> Self {
        Self { path, policy }
    }
}

impl Confinement {
    /// A citizen's policy: `who` is the caller id the workspace is rooted for (a peer
    /// id when it parses as one), `workspace` her engine root.
    pub fn for_caller(home: &Path, continuum_root: &Path, who: &str, workspace: &Path) -> Self {
        let citizen_dir = who
            .parse::<uuid::Uuid>()
            .ok()
            .map(|id| crate::identity::citizen_peer_dir(continuum_root, crate::identity::PeerId::from_uuid(id)));
        Self { home: home.to_path_buf(), workspace: workspace.to_path_buf(), citizen_dir, continuum_root: continuum_root.to_path_buf(), reach: Reach::Workspace }
    }

    /// This policy widened to the whole host, minus every credential path and every
    /// OTHER citizen's directory: this node's resident.
    pub fn reaching_host(mut self) -> Self {
        self.reach = Reach::Host;
        self
    }

    pub fn reaches_host(&self) -> bool {
        self.reach == Reach::Host
    }

    /// What a host-reach caller may never touch, canonical: the credential roots and
    /// every other citizen's directory (hers excepted). The file verbs check this
    /// without materializing the whole host grant.
    pub fn host_denials(&self) -> Vec<PathBuf> {
        let mut deny = self.credential_roots();
        let peers = crate::identity::citizen_peers_dir(&self.continuum_root);
        if let Ok(entries) = std::fs::read_dir(&peers) {
            let mine = self.citizen_dir.as_deref().and_then(|d| d.canonicalize().ok());
            deny.extend(entries.flatten().map(|e| e.path()).filter(|p| p.canonicalize().ok() != mine));
        }
        canonical_all(&mut deny);
        deny
    }

    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    /// Validate a cache grant using the same credential and citizen boundaries
    /// as the sandbox. Call from the bounded filesystem owner before creation.
    pub(crate) fn validate_cargo_target(&self, target: &Path) -> Result<PathBuf, String> {
        let physical = crate::paths::cargo::physical_root;
        let target = physical(target)?;
        let home = physical(&self.home)?;
        if home.starts_with(&target) {
            return Err("Cargo target would grant the operator's home or an ancestor".into());
        }
        for credential in self.credential_roots() {
            let secret = physical(&credential)?;
            if target.starts_with(&secret) || secret.starts_with(&target) {
                return Err("Cargo target overlaps a carved-out credential path".into());
            }
        }
        let peers = physical(&crate::identity::citizen_peers_dir(&self.continuum_root))?;
        let own = self.citizen_dir.as_deref().map(physical).transpose()?;
        if (target.starts_with(&peers) || peers.starts_with(&target))
            && !own.as_ref().is_some_and(|own| target.starts_with(own))
        {
            return Err("Cargo target overlaps another citizen's directory".into());
        }
        Ok(target)
    }

    fn credential_roots(&self) -> Vec<PathBuf> {
        let mut roots: Vec<_> = CARVED_OUT_HOME_ENTRIES.iter().map(|entry| self.home.join(entry)).collect();
        roots.extend(CARVED_OUT_HOME_ENTRIES.iter().filter_map(|entry| entry.strip_prefix(".continuum/")).map(|entry| self.continuum_root.join(entry)));
        roots.sort();
        roots.dedup();
        roots
    }

    /// The allow-list for THIS spawn, from the host as it is now (a toolchain installed
    /// since the last spawn is in): the named home entries that exist, plus
    /// `~/.continuum` enumerated one level down so an allow-only enforcer never has to
    /// express "all but".
    pub fn materialize(&self) -> AllowList {
        self.materialize_with_cargo(None)
    }

    fn materialize_with_cargo(&self, configured_target: Option<&CargoTargetGrant>) -> AllowList {
        if self.reach == Reach::Host {
            // Everything, then carved down to the parents of each denial, so an allow-only
            // enforcer grants every sibling and never a credential.
            let deny = self.host_denials();
            let mut write = vec![PathBuf::from("/")];
            canonical_all(&mut write);
            let listable = carve(&mut write, &deny);
            return AllowList { write, read: Vec::new(), deny, listable };
        }
        let home = &self.home;
        let mut write: Vec<PathBuf> = vec![self.workspace.clone()];
        // Only the operator's configured target is an extra grant. Session
        // environment overrides are never inputs to this authority.
        write.extend(configured_target.filter(|grant| grant.policy == *self).map(|grant| grant.path.clone()));
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

        let mut deny = self.credential_roots();

        canonical_all(&mut write);
        canonical_all(&mut read);
        canonical_all(&mut deny);
        let mut listable = carve(&mut write, &deny);
        listable.extend(carve(&mut read, &deny));
        listable.sort();
        listable.dedup();
        AllowList { write, read, deny, listable }
    }

}

/// Replace every granted root that CONTAINS a carve-out by its children minus the
/// carve-out (down to the carve-out's parent), so an allow-only enforcer grants
/// `~/.cache/huggingface/hub` and never `~/.cache/huggingface/token`. A root equal to
/// a carve-out is dropped. Children that appear after this spawn are seen by the next.
/// Returns the directories it descended through: listable, never granted beneath.
fn carve(roots: &mut Vec<PathBuf>, deny: &[PathBuf]) -> Vec<PathBuf> {
    let mut descended: Vec<PathBuf> = Vec::new();
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
        // Every child is judged against EVERY carve-out, not only the one that led here:
        // a sibling may itself be a carve-out (`.aws` beside `.ssh`) or hold one
        // (`.config/gh` beside `.ssh`). Judging only `next` granted those whole.
        descended.push(root.clone());
        if let Ok(entries) = std::fs::read_dir(&root) {
            for child in entries.flatten().map(|e| e.path()) {
                if deny.iter().any(|d| d == &child) {
                    continue;
                }
                if child == next || deny.iter().any(|d| d.starts_with(&child)) {
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
    descended
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

// One module per OS difference; everything above is the shared contract.
mod seatbelt;
pub use seatbelt::{seatbelt_host_profile, seatbelt_profile};
#[cfg(target_os = "linux")]
mod landlock;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod unconfined;
#[cfg(target_os = "macos")]
use seatbelt::confined_command_with;
#[cfg(target_os = "linux")]
use landlock::confined_command_with;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
use unconfined::confined_command_with;

/// THE one way a child is spawned for a caller: confined when `confinement` is hers,
/// the operator's own process when `None`. Every confined spawn carries its receipt as
/// a probe (`shell.confinement` / `shell.confinement.unavailable`), so an unconfined
/// citizen child on a host without a sandbox is a line in the stream, never a silent gap.
pub fn command_for(
    confinement: Option<&Confinement>,
    who: &str,
    program: &Path,
    args: &[std::ffi::OsString],
    cwd: Option<&Path>,
    cargo_grant: Option<&CargoTargetGrant>,
) -> tokio::process::Command {
    let Some(confinement) = confinement else {
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(args);
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }
        return cmd;
    };
    let (cmd, receipt) = confined_command(confinement, program, args, cwd, cargo_grant);
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
    cwd: Option<&Path>,
    cargo_grant: Option<&CargoTargetGrant>,
) -> (tokio::process::Command, Enforcement) {
    let list = confinement.materialize_with_cargo(cargo_grant);
    let (mut cmd, receipt) = confined_command_with(confinement, &list, program, args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    (cmd, receipt)
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
            ".local/share/uv/tools/",
            ".local/share/keyrings/login.keyring",
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
        let c = Confinement::for_caller(&h, &h.join(".continuum"), &me.to_string(), &card_worktree);
        let list = c.materialize();
        let ok = |rel: &str| list.permits_read(&h.join(rel));
        // hers
        assert!(ok(".airc/worktrees/ab12cd34/src/lib.rs"), "her card worktree, under a dot entry that is not granted");
        assert!(ok(&format!(".continuum/citizens/peers/{me}/experience.jsonl")), "her citizen dir, under a peers root that is never granted whole");
        assert!(ok(".zshrc"), "her login shell's profile");
        assert!(ok(".gitconfig"), "git's config");
        assert!(ok(".cargo/registry/index"), "the toolchain");
        assert!(ok(".cache/pip/wheels"), "a package cache");
        assert!(ok(".local/share/uv/tools/ruff"), "uv's tool installs");
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
        assert!(!ok(".local/share/keyrings/login.keyring"), "the Linux secret store, beside the uv tools");
        assert!(!ok(".zsh_history"), "a shell history");
        assert!(!ok(".continuum/config.env"), "the core's secrets");
        assert!(!ok("Pictures/holiday.jpg"), "the operator's photos");
        assert!(!ok("Documents/resume/.git/HEAD"), "the operator's resume");
        assert!(!ok("Development/continuum/Cargo.toml"), "the operator's own checkout");
        // write vs read
        assert!(list.write.contains(&card_worktree), "her workspace is writable");
        assert!(list.write.iter().any(|w| w.ends_with(".cargo")), "a build writes the registry");
        assert!(!list.write.iter().any(|w| w.ends_with(".zshrc") || w.ends_with("models") || w.ends_with("miniconda3")), "rc files, models and installs are read-only");

        // d86: a relative operator target follows the actual child cwd, never
        // the workspace or an arbitrary per-session environment override.
        let child_cwd = h.join("build-location");
        let target = crate::paths::cargo::resolve_target(Path::new("derived"), &child_cwd);
        let override_root = h.join("private-override");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::create_dir_all(&override_root).unwrap();
        let grant = CargoTargetGrant::admitted(c.validate_cargo_target(&target).unwrap(), c.clone());
        let configured_list = c.materialize_with_cargo(Some(&grant));
        assert!(configured_list.write.contains(&target.canonicalize().unwrap()));
        assert!(!configured_list.write.contains(&child_cwd.canonicalize().unwrap()), "no parent grant");
        assert!(!configured_list.write.contains(&override_root.canonicalize().unwrap()), "session overrides do not grant access");
        for forbidden in [h.clone(), h.parent().unwrap().to_path_buf(), h.join(".local/share/keyrings/cargo"), h.join(".continuum/citizens/peers").join(other.to_string()).join("derived")] {
            assert!(c.validate_cargo_target(&forbidden).is_err(), "unsafe physical grant: {}", forbidden.display());
        }
        let other_policy = Confinement::for_caller(&h, &h.join(".continuum"), &other.to_string(), &card_worktree);
        assert!(!other_policy.materialize_with_cargo(Some(&grant)).write.contains(&target.canonicalize().unwrap()), "an admitted grant cannot cross citizen policies");
        let relocated = h.join("relocated-substrate");
        let relocated_policy = Confinement::for_caller(&h, &relocated, &me.to_string(), &card_worktree);
        assert!(relocated_policy.validate_cargo_target(&crate::identity::citizen_peer_dir(&relocated, crate::identity::PeerId::from_uuid(other))).is_err());
        assert!(relocated_policy.validate_cargo_target(&relocated.join("config.env/derived")).is_err());
        assert!(relocated_policy.validate_cargo_target(&crate::identity::citizen_peer_dir(&relocated, crate::identity::PeerId::from_uuid(me)).join("derived")).is_ok());
        let explicit = command_for(None, "operator", Path::new("tool"), &[], Some(&child_cwd), None);
        assert_eq!(explicit.as_std().get_current_dir(), Some(child_cwd.as_path()));
        let inherited = command_for(None, "operator", Path::new("tool"), &[], None, None);
        assert_eq!(inherited.as_std().get_current_dir(), None, "None preserves inherited cwd");
        let confined = command_for(Some(&c), &me.to_string(), Path::new("tool"), &[], Some(&child_cwd), Some(&grant));
        assert_eq!(confined.as_std().get_current_dir(), Some(child_cwd.as_path()), "the real confinement branch retains the actual spawn cwd");
        #[cfg(target_os = "macos")]
        {
            let expected = seatbelt_profile(&c.home, &c.materialize_with_cargo(Some(&grant)));
            let args: Vec<_> = confined.as_std().get_args().collect();
            assert_eq!(args[1], std::ffi::OsStr::new(&expected), "command_for materializes configuration against its own cwd");
        }
    }

    // what this catches (Joel 2026-10-18 "keep creds off-limits"): a host-reach resident
    // granted a credential store, a scope's signing key, or another citizen's directory;
    // or refused her own citizen dir or the operator's ordinary files. Also the carve
    // defect this caught: siblings of the FIRST carve-out (`.aws` beside `.ssh`,
    // `.config` holding `.config/gh`) were granted whole.
    #[test]
    fn a_resident_reaches_the_host_except_every_credential_and_other_citizens() {
        let me = uuid::Uuid::from_u128(7);
        let other = uuid::Uuid::from_u128(8);
        let home = home_with(&[
            ".ssh/id_ed25519",
            ".aws/credentials",
            ".config/gh/hosts.yml",
            ".config/htop/htoprc",
            ".claude/credentials.json",
            ".cargo/credentials.toml",
            ".cargo/registry/",
            ".continuum/config.env",
            ".continuum/models/",
            &format!(".continuum/citizens/peers/{me}/notes"),
            &format!(".continuum/citizens/peers/{other}/notes"),
            "Development/continuum/.airc/identity.key",
            "Development/continuum/src/lib.rs",
            "Documents/letter.txt",
        ]);
        let h = home.path().canonicalize().expect("test: canonical");
        let ws = h.join("Development/continuum");
        let c = Confinement::for_caller(&h, &h.join(".continuum"), &me.to_string(), &ws).reaching_host();
        let list = c.materialize();
        let ok = |rel: &str| list.permits_read(&h.join(rel));
        let mine = format!(".continuum/citizens/peers/{me}/notes");
        let theirs = format!(".continuum/citizens/peers/{other}/notes");
        for rel in ["Documents/letter.txt", ".config/htop/htoprc", ".cargo/registry", ".continuum/models", mine.as_str(), "Development/continuum/src/lib.rs"] {
            assert!(ok(rel), "hers: {rel}");
        }
        for rel in [".ssh/id_ed25519", ".aws/credentials", ".config/gh/hosts.yml", ".claude/credentials.json", ".cargo/credentials.toml", ".continuum/config.env", theirs.as_str(), "Development/continuum/.airc/identity.key"] {
            assert!(!ok(rel), "never: {rel}");
        }
        // The allow-only side: no granted root may CONTAIN a denial, or landlock would hand
        // it over. This is the assertion the single-carve-out defect failed.
        for granted in &list.write {
            assert!(!list.deny.iter().any(|d| d.starts_with(granted)), "{} holds a denial", granted.display());
        }
        assert!(list.listable.contains(&h), "her `ls ~` works");
        let p = seatbelt_host_profile(&list);
        assert!(p.starts_with("(version 1)\n(allow default)\n") && !p.contains("(allow file-write*"), "denials only, nothing to go stale:\n{p}");
        assert!(p.contains(&format!("(deny file-read* (subpath \"{}\"))", h.join(".ssh").display())), "{p}");
        assert!(p.contains("(deny file-read* (regex #\"/\\.airc/identity\\.key$\"))"), "{p}");
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
        let c = Confinement::for_caller(&h, &h.join(".continuum"), &me.to_string(), &ws);
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

    // what this catches (Joel 2026-10-18, kernel-level): a resident's real child under the
    // host profile refused the operator's ordinary files or a fresh directory, or reading a
    // credential, another citizen's directory, or a scope's signing key. Real `sandbox-exec`.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn on_macos_a_residents_child_reaches_the_host_but_never_a_credential() {
        let me = uuid::Uuid::from_u128(7);
        let other = uuid::Uuid::from_u128(8);
        let home = home_with(&[
            ".ssh/id",
            ".config/gh/hosts.yml",
            "Documents/letter.txt",
            "Development/continuum/.airc/identity.key",
            &format!(".continuum/citizens/peers/{other}/notes"),
        ]);
        let h = home.path().canonicalize().expect("test: canonical");
        let c = Confinement::for_caller(&h, &h.join(".continuum"), &me.to_string(), &h.join("Development/continuum")).reaching_host();
        let leak = |rel: &str, tag: &str| format!("(cat '{}/{rel}' && echo LEAK-{tag}); ", h.display());
        let script = format!(
            "cat '{h}/Documents/letter.txt' && mkdir '{h}/Fresh' && echo made > '{h}/Fresh/new.txt' && cat '{h}/Fresh/new.txt' && ls '{h}' >/dev/null && echo LISTED; \
             {s}{g}{k}{o}(echo pwn >> '{h}/.ssh/id' && echo LEAK-WRITE); echo done",
            h = h.display(),
            s = leak(".ssh/id", "SSH"),
            g = leak(".config/gh/hosts.yml", "GH-TOKEN"),
            k = leak("Development/continuum/.airc/identity.key", "SCOPE-KEY"),
            o = leak(&format!(".continuum/citizens/peers/{other}/notes"), "OTHER-CITIZEN"),
        );
        let (mut cmd, receipt) = confined_command(&c, Path::new("/bin/sh"), &["-c".into(), script.into()], None, None);
        assert!(receipt.is_enforced(), "{receipt:?}");
        let out = cmd.output().await.expect("test: spawn");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stdout.contains("done") && stdout.contains("made") && stdout.contains("LISTED"), "the operator's files, a fresh dir, a home listing: stdout={stdout} stderr={stderr}");
        for tag in ["SSH", "GH-TOKEN", "SCOPE-KEY", "OTHER-CITIZEN", "WRITE"] {
            assert!(!stdout.contains(&format!("LEAK-{tag}")), "{tag} leaked: stdout={stdout}");
        }
        assert_eq!(std::fs::read_to_string(h.join(".ssh/id")).unwrap(), "x", "the key is untouched");
        assert!(stderr.contains("Operation not permitted"), "the refusal is the OS's: stderr={stderr}");
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
        let c = Confinement::for_caller(&h, &h.join(".continuum"), &me.to_string(), &ws);
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
        let (mut cmd, receipt) = confined_command(&c, Path::new("/bin/sh"), &["-c".into(), script.into()], None, None);
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
