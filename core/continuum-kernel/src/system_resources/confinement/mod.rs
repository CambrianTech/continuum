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
//! scope's signing key, [`CREDENTIAL_SUFFIXES`]) and other citizens' private state, their
//! minds (Joel 2026-10-17 "Don't restrict them"; 2026-10-18 "keep creds off-limits").
//! Every citizen's workspace is the commons, read and written by all of them (2026-10-24:
//! "they cannot be locked out of things so readily"). That boundary is
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
pub fn is_scoped_credential(path: &Path) -> bool {
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
    /// The whole host except the credential paths and other citizens' private state: this
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

    /// What a host-reach caller may never touch, canonical: the credential roots and every
    /// other citizen's PRIVATE state (her mind: airc state, lived experience, memory),
    /// everything in another citizen's directory except her workspace. The file verbs check
    /// this without materializing the whole host grant.
    ///
    /// Another citizen's workspace is the commons, never denied, read or write: a reviewer
    /// reads and runs the code he was asked to review (Atlas on the M5, 2026-10-24: ~35 acts
    /// refused as "escapes workspace" for paths inside the checkout his review lane rooted
    /// him in), a citizen repairs a colleague's broken checkout or pushes a fix to her
    /// branch, and minds learn from each other's code; git keeps every change recoverable.
    /// Joel: "they cannot be locked out of things so readily nor harnessed"; "everything
    /// you've just done we'd expect them to also be able to do here to fix each other". A
    /// citizen's thoughts and every credential stay out of reach.
    pub fn host_denials(&self) -> Vec<PathBuf> {
        let mut deny = self.credential_roots();
        deny.extend(self.other_citizens().flat_map(|dir| {
            std::fs::read_dir(&dir)
                .map(|entries| entries.flatten().map(|e| e.path()).filter(|p| p.file_name() != Some(WORKSPACE.as_ref())).collect::<Vec<_>>())
                .unwrap_or_default() // unwrap_or_default: an unreadable citizen dir holds nothing to deny beneath it
        }));
        canonical_all(&mut deny);
        deny
    }

    /// Every other citizen's workspace: the commons she reads and writes like her own.
    ///
    /// Only a REAL directory counts, never a symlink, and only when its real path stays inside
    /// that citizen's own directory: a citizen controls her directory, so a `workspace` she
    /// re-points at another citizen's private state or a credential root would otherwise be
    /// granted as a read-write root (independent review of #5383, 2026-10-24).
    pub fn shared_workspaces(&self) -> Vec<PathBuf> {
        let mut shared: Vec<PathBuf> = self.other_citizens().filter_map(|dir| shared_workspace_of(&dir)).collect();
        canonical_all(&mut shared);
        shared
    }

    /// The directories of every citizen on this node but her.
    fn other_citizens(&self) -> impl Iterator<Item = PathBuf> {
        let peers = crate::identity::citizen_peers_dir(&self.continuum_root);
        let mine = self.citizen_dir.as_deref().and_then(|d| d.canonicalize().ok());
        std::fs::read_dir(&peers)
            .into_iter()
            .flat_map(|entries| entries.flatten().map(|e| e.path()))
            .filter(move |p| p.canonicalize().ok() != mine)
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
            // enforcer grants every sibling (other citizens' workspaces included) and never a
            // credential or another citizen's private state.
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
        // Every other citizen's workspace, the commons (`shared_workspaces`); their private
        // state stays ungranted, since the citizens root is enumerated without `peers`.
        write.extend(self.shared_workspaces());
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

        // A resident's card checkout is a clone that borrows objects from this node's repo
        // mirror through its alternates (card d166e4f2). Her git reads those objects, so each
        // mirror's objects directory is a READ-only root: never a write root, and nothing else
        // of the mirror (refs, config, hooks). Every write she makes lands in her own .git.
        read.extend(mirror_object_dirs(home));

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

/// The objects directory of every repo mirror under `~/.airc/mirrors/<owner>/<repo>.git`,
/// the only part of a mirror a card clone's alternates read (card d166e4f2). Listed at
/// materialize time, like `~/.continuum` above, so a mirror created after this spawn is
/// granted to the next one.
fn mirror_object_dirs(home: &Path) -> Vec<PathBuf> {
    let Ok(owners) = std::fs::read_dir(home.join(".airc").join("mirrors")) else {
        return Vec::new();
    };
    owners
        .flatten()
        .filter_map(|owner| std::fs::read_dir(owner.path()).ok())
        .flat_map(|repos| repos.flatten().map(|repo| repo.path().join("objects")))
        .filter(|objects| objects.is_dir())
        .collect()
}

/// The one entry of a citizen's directory other citizens may read and write.
const WORKSPACE: &str = "workspace";

/// `citizen_dir`'s workspace when it is a real directory (not a symlink) whose real path lies
/// inside `citizen_dir`'s real path; `None` otherwise (absent, a link, or escaping).
fn shared_workspace_of(citizen_dir: &Path) -> Option<PathBuf> {
    let ws = citizen_dir.join(WORKSPACE);
    let meta = std::fs::symlink_metadata(&ws).ok()?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return None;
    }
    let (real, home) = (ws.canonicalize().ok()?, citizen_dir.canonicalize().ok()?);
    real.starts_with(&home).then_some(real)
}

/// Replace every granted root that CONTAINS a carve-out by its children minus the
/// carve-out (down to the carve-out's parent), so an allow-only enforcer grants
/// `~/.cache/huggingface/hub` and never `~/.cache/huggingface/token`. A root equal to
/// a carve-out, or inside one, is dropped. Children that appear after this spawn are seen by the next.
/// Returns the directories it descended through: listable, never granted beneath.
fn carve(roots: &mut Vec<PathBuf>, deny: &[PathBuf]) -> Vec<PathBuf> {
    let mut descended: Vec<PathBuf> = Vec::new();
    let mut out: Vec<PathBuf> = Vec::with_capacity(roots.len());
    let mut pending: Vec<PathBuf> = std::mem::take(roots);
    while let Some(root) = pending.pop() {
        // A root equal to a carve-out, or INSIDE one, is never granted: an allow-only
        // enforcer (landlock) would otherwise hand over what the carve-out denies.
        if deny.iter().any(|d| root.starts_with(d)) {
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
        Enforcement::Enforced { mechanism } => continuum_base::probe!(
            class = "shell.confinement",
            who = %who,
            mechanism = %mechanism,
            program = %program.display(),
            workspace = %confinement.workspace().display(),
            "a citizen's child is confined to her roots"
        ),
        Enforcement::Unavailable { reason } => continuum_base::probe!(
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
mod tests;
