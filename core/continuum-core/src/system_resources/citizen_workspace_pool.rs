//! THE CITIZENS' WORKSPACES HAVE AN OWNER.
//!
//! Measured on the M5, 2026-09-08: `~/.continuum/citizens` held 91 GB across **714 peer
//! directories** while **5** citizens were resident. Every one of the other 709 is a
//! persona who worked here once — a benchmark round's roster, a retired name, an
//! experiment — and left a workspace behind: staged SWE checkouts, a clone of the
//! project, downloaded fixtures. The class had a documented eviction DECISION (card
//! 58c27b0c) and no owner, so the decision never ran and a human swept by hand.
//!
//! # What this pool will and will not take
//!
//! It takes the WORKSPACE — the working files a dormant citizen can rebuild from the
//! project and the dataset. It never takes her MEMORY: `experience.jsonl`, engram stores,
//! her identity, anything that is who she is rather than what she was doing. A persona
//! whose workspace is gone comes back and re-stages; a persona whose memory is gone is a
//! different person, and no disk number justifies that.
//!
//! It never takes a RESIDENT citizen's workspace, and when the roster cannot be read it
//! takes NOTHING — an unreadable roster is not an empty roster (card c7ae34b2).
//!
//! Dirty work is archived before the tree goes: an uncommitted checkout becomes
//! `<workspace>/../dropped/<instance>.patch` beside its base commit, so a citizen's
//! unfinished patch survives as a patch even when its tree does not.
//!
//! # A workspace BORROWS, so removing one is safe and removing the wrong thing is not
//!
//! Verified on the M5, 2026-09-08: a citizen's workspace is a `--shared` clone whose
//! `.git/objects/info/alternates` points at the project checkout's object store (card
//! #3795). Dropping a workspace therefore frees only what that citizen wrote — the borrow
//! ends, nothing another workspace depends on goes with it. The inverse is the danger and
//! this pool must never do it: the project's object store is the LENDER for every
//! workspace on the node, so it is not a cache, it is not evictable here, and
//! `extensions.preciousObjects` on the base exists to make a stray `git gc` refuse. This
//! pool only ever removes `citizens/peers/<uuid>/workspace`, never the base, never a
//! peer's memory, never anything outside that path.
//!
//! # A resident's rebuildable residue (card 10e6c5e5)
//!
//! Residency pins a whole workspace, and that is right for her work. It was also pinning her
//! BUILD OUTPUT forever. Measured on the IntelMac, 2026-09-28: three resident citizens at
//! 15 GB each, 12 GB of each a private cargo `target/` last written Sep 4. That is 36 of
//! 45 GB, unevictable by design while she stays resident. So a second, narrower pass reclaims
//! one thing inside a RESIDENT workspace, `workspace/target`, and only when every one of
//! these holds (Codex's gates on 10e6c5e5):
//!
//! - **Rebuildable, proven, not named:** the tree carries cargo's own `CACHEDIR.TAG`
//!   signature. A directory called `target` that cargo did not write is left alone.
//! - **No build is live:** every `.cargo-lock` under it takes an exclusive, non-blocking
//!   lock. That is cargo's own build-directory lock, so a running cargo refuses us.
//! - **No solve is live:** the reclaim holds the citizen's hands (`work::HandsLease`) for the
//!   whole transaction, so a staged solve cannot start inside a tree being removed.
//! - **The roster is readable** (unreadable = nothing, as for whole workspaces).
//! - **Untouched for [`DORMANT_AFTER_MS`]:** policy, not proof; the lock is the proof.
//!
//! Nothing is preserved, because a cargo-tagged tree holds no work. The locks are released
//! just before the delete (Windows cannot delete a file held open), so a build that starts in
//! that instant fails and rebuilds; nothing is lost. Staged `swe/` checkouts in a resident
//! workspace are NOT covered here: they are work until their card is terminal, and that
//! needs the card and its runs (the second half of 10e6c5e5).
//!
//! # What actually filled the disk, measured rather than assumed
//!
//! 715 peer directories existed and only **12** held a workspace: the other 703 are a
//! persona's memory, which is kilobytes and is never touched here. The 91 GB was those 12
//! at ~8 GB each — each carrying its own 2.7 GB of git objects written since the clone,
//! ~2 GB of staged SWE checkouts, and the project's build output. Sizing the budget
//! against "714 workspaces" would have been wrong by two orders of magnitude, which is
//! why the pool reads the tree instead of a number someone remembered.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::paging::pool::ResourcePool;
use crate::system_resources::disk_reporters::TrackedDir;

/// A dormant citizen keeps her workspace for this long after her last activity. Long
/// enough that a citizen despawned for a measurement window and re-seated the next day
/// keeps her staged checkouts; short enough that a benchmark roster from last week is not
/// still holding 8 GB.
pub const DORMANT_AFTER_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// What the citizens' workspaces may occupy in total before the owner starts dropping the
/// dormant ones. Sized from the measurement that produced this pool: five resident
/// citizens at ~8-10 GB each is normal and must never be touched.
pub const DEFAULT_CITIZENS_BUDGET_BYTES: u64 = 60 * 1024 * 1024 * 1024;

/// One peer's workspace as the decision sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerWorkspace {
    pub peer_id: uuid::Uuid,
    pub workspace: PathBuf,
    pub bytes: u64,
    /// Newest mtime anywhere under the peer's directory.
    pub last_active_ms: u64,
}

/// Who is resident right now, or NOT KNOWN. An unreadable roster evicts nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Roster {
    Live(Vec<uuid::Uuid>),
    Unreadable,
}

/// THE DECISION, pure. Returns the workspaces to drop, largest first, until `want_bytes`
/// is covered — never a resident citizen, never one active inside the dormancy window,
/// and nothing at all when the roster could not be read.
pub fn workspaces_to_drop(
    peers: &[PeerWorkspace],
    roster: &Roster,
    now_ms: u64,
    want_bytes: u64,
) -> Vec<PeerWorkspace> {
    let Roster::Live(resident) = roster else {
        return Vec::new();
    };
    let mut candidates: Vec<&PeerWorkspace> = peers
        .iter()
        .filter(|p| !resident.contains(&p.peer_id))
        .filter(|p| now_ms.saturating_sub(p.last_active_ms) >= DORMANT_AFTER_MS)
        .filter(|p| p.bytes > 0)
        .collect();
    // Largest first: the fewest trees removed for the bytes asked.
    candidates.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.peer_id.cmp(&b.peer_id)));
    let mut taken = Vec::new();
    let mut freed = 0u64;
    for c in candidates {
        if freed >= want_bytes {
            break;
        }
        freed = freed.saturating_add(c.bytes);
        taken.push(c.clone());
    }
    taken
}

/// Cargo writes this signature as the first line of `CACHEDIR.TAG` in every target dir it
/// creates (the cachedir spec). It is the proof that a tree is build output.
const CARGO_CACHEDIR_SIGNATURE: &str = "Signature: 8a477f597d28d172789f06886806bc55";

/// One citizen's rebuildable build output, as the residue decision sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildResidue {
    pub peer_id: uuid::Uuid,
    /// `citizens/peers/<peer>/workspace/target`, carrying cargo's CACHEDIR.TAG.
    pub target: PathBuf,
    pub bytes: u64,
    /// Newest mtime anywhere under the target dir.
    pub last_active_ms: u64,
}

/// THE RESIDUE DECISION, pure: stale cargo build output, largest first, until `want_bytes` is
/// covered. Residency does NOT exempt it (that is the point: see the module doc), but an
/// unreadable roster still takes nothing, and output touched inside the dormancy window stays.
/// The liveness proofs (cargo's lock, the citizen's hands) are taken at the act, not here.
pub fn residue_to_drop(
    residue: &[BuildResidue],
    roster: &Roster,
    now_ms: u64,
    want_bytes: u64,
) -> Vec<BuildResidue> {
    if *roster == Roster::Unreadable {
        return Vec::new();
    }
    let mut candidates: Vec<&BuildResidue> = residue
        .iter()
        .filter(|r| now_ms.saturating_sub(r.last_active_ms) >= DORMANT_AFTER_MS)
        .filter(|r| r.bytes > 0)
        .collect();
    candidates.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.peer_id.cmp(&b.peer_id)));
    let mut taken = Vec::new();
    let mut freed = 0u64;
    for c in candidates {
        if freed >= want_bytes {
            break;
        }
        freed = freed.saturating_add(c.bytes);
        taken.push(c.clone());
    }
    taken
}

/// Is `dir` a tree cargo wrote? Its `CACHEDIR.TAG` must begin with cargo's signature.
fn is_cargo_target(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("CACHEDIR.TAG")).is_ok_and(|t| t.starts_with(CARGO_CACHEDIR_SIGNATURE))
}

/// Every `.cargo-lock` in a target dir: `target/<profile>/.cargo-lock` and, for a cross
/// build, `target/<triple>/<profile>/.cargo-lock`.
fn cargo_locks_in(target: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![(target.to_path_buf(), 0u32)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if e.file_name() == ".cargo-lock" {
                out.push(path);
            } else if depth < 2 && e.file_type().is_ok_and(|t| t.is_dir()) {
                stack.push((path, depth + 1));
            }
        }
    }
    out
}

/// Is any cargo build using this target dir right now? Tries cargo's own build-directory
/// lock on every `.cargo-lock`; one held means a build is live. Returns the held lock's path.
fn live_build_in(target: &Path) -> Option<PathBuf> {
    for lock in cargo_locks_in(target) {
        let Ok(file) = std::fs::OpenOptions::new().read(true).write(true).open(&lock) else {
            // cannot open cargo's lock: cannot prove the build is idle, so treat it as live
            return Some(lock);
        };
        if file.try_lock().is_err() {
            return Some(lock);
        }
        // dropping `file` releases the lock before the delete (Windows cannot remove a file
        // held open); see the module doc for why that instant is safe
    }
    None
}

pub struct CitizenWorkspacePool {
    tracked: Arc<TrackedDir>,
    budget_bytes: u64,
}

impl CitizenWorkspacePool {
    pub fn new(tracked: Arc<TrackedDir>, budget_bytes: u64) -> Self {
        Self {
            tracked,
            budget_bytes: budget_bytes.max(1),
        }
    }

    /// The roster as the runtime knows it. `Unreadable` when no registry is up — the boot
    /// window, a test, a core without personas — and then nothing is evicted.
    fn roster() -> Roster {
        match crate::persona::PersonaAircRuntimeRegistry::try_global() {
            Some(reg) => Roster::Live(reg.roster_snapshot().into_iter().map(|(_, id)| id).collect()),
            None => Roster::Unreadable,
        }
    }

    fn peers_on_disk(root: &Path) -> Vec<PeerWorkspace> {
        let peers_dir = root.join("peers");
        let Ok(entries) = std::fs::read_dir(&peers_dir) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let dir = entry.path();
            let Some(peer_id) = dir
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| uuid::Uuid::parse_str(n).ok())
            else {
                continue;
            };
            let workspace = dir.join("workspace");
            if !workspace.is_dir() {
                continue;
            }
            let (bytes, newest) = dir_bytes_and_newest(&dir);
            out.push(PeerWorkspace {
                peer_id,
                workspace,
                bytes,
                last_active_ms: newest,
            });
        }
        out
    }

    /// Every peer workspace's cargo build output, resident or not: the residue pass decides.
    fn build_residue_on_disk(root: &Path) -> Vec<BuildResidue> {
        let Ok(entries) = std::fs::read_dir(root.join("peers")) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let dir = entry.path();
            let Some(peer_id) = dir
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| uuid::Uuid::parse_str(n).ok())
            else {
                continue;
            };
            let target = dir.join("workspace").join("target");
            if !is_cargo_target(&target) {
                continue;
            }
            let (bytes, newest) = dir_bytes_and_newest(&target);
            out.push(BuildResidue { peer_id, target, bytes, last_active_ms: newest });
        }
        out
    }

    /// Reclaim stale build output, residents included, under the gates in the module doc.
    fn evict_build_residue(root: &Path, roster: &Roster, now_ms: u64, want_bytes: u64) -> u64 {
        let residue = Self::build_residue_on_disk(root);
        let mut freed = 0u64;
        for r in residue_to_drop(&residue, roster, now_ms, want_bytes) {
            // ONLY `peers/<peer>/workspace/target`, by construction (build_residue_on_disk)
            debug_assert!(r.target.ends_with("workspace/target"));
            if !r.target.ends_with("workspace/target") {
                continue;
            }
            // her hands for the whole transaction: no staged solve starts inside this tree
            let Some(_hands) = crate::modules::work::HandsLease::try_take(r.peer_id) else {
                crate::probe!(
                    class = "disk.citizens.residue_kept",
                    peer = %r.peer_id,
                    why = "solve_live",
                    "a citizen's build output was NOT reclaimed: she is mid-solve"
                );
                continue;
            };
            if let Some(lock) = live_build_in(&r.target) {
                crate::probe!(
                    class = "disk.citizens.residue_kept",
                    peer = %r.peer_id,
                    why = "build_live",
                    lock = %lock.display(),
                    "a citizen's build output was NOT reclaimed: cargo holds its build lock"
                );
                continue;
            }
            if std::fs::remove_dir_all(&r.target).is_ok() {
                freed = freed.saturating_add(r.bytes);
                crate::probe!(
                    class = "disk.citizens.residue_reclaimed",
                    peer = %r.peer_id,
                    freed_mb = r.bytes / (1024 * 1024),
                    idle_days = now_ms.saturating_sub(r.last_active_ms) / (24 * 60 * 60 * 1000),
                    "a citizen's stale cargo build output was reclaimed (her workspace and work untouched; the next build rebuilds it)"
                );
            }
        }
        freed
    }
}

/// Total bytes and newest mtime under a directory, bounded to a depth that cannot walk a
/// whole repo's history: the numbers here decide eviction, so they are read from the tree
/// rather than remembered, but a full recursive walk of 714 workspaces on every tick is
/// exactly the hot-path cost this substrate forbids.
fn dir_bytes_and_newest(dir: &Path) -> (u64, u64) {
    let mut bytes = 0u64;
    let mut newest = 0u64;
    let mut stack = vec![(dir.to_path_buf(), 0u32)];
    while let Some((path, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if let Ok(modified) = meta.modified() {
                if let Ok(d) = modified.duration_since(std::time::UNIX_EPOCH) {
                    newest = newest.max(d.as_millis() as u64);
                }
            }
            if meta.is_dir() {
                if depth < 6 {
                    stack.push((entry.path(), depth + 1));
                }
            } else {
                bytes = bytes.saturating_add(meta.len());
            }
        }
    }
    (bytes, newest)
}

/// Why a workspace could not be preserved. A named refusal, because the ONLY thing this
/// pool may never do is delete work it failed to keep (Astra's review of #3908,
/// 2026-09-08: "preservation failure or ordinary dirty work can be destroyed").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreserveFailed {
    /// `git` itself failed — a non-zero exit, an unreadable repo, a missing binary.
    Git(String),
    /// The archive could not be written, or could not be read back afterwards.
    Archive(String),
    /// More uncommitted bytes than this pool will archive. Reclaiming disk is never worth
    /// gambling with a citizen's work, so the workspace simply stays.
    TooLarge { bytes: u64, cap: u64 },
}

impl std::fmt::Display for PreserveFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Git(e) => write!(f, "git: {e}"),
            Self::Archive(e) => write!(f, "archive: {e}"),
            Self::TooLarge { bytes, cap } => {
                write!(f, "uncommitted work is {bytes} bytes, above the {cap} archive cap")
            }
        }
    }
}

/// The most uncommitted work this pool will archive for one workspace. Above it the
/// workspace is kept: a citizen with a gigabyte of unsaved work is not a disk problem.
pub const PRESERVE_CAP_BYTES: u64 = 1024 * 1024 * 1024;

/// Every git repository inside a workspace — a citizen stages more than `swe/`.
fn repos_in(workspace: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![(workspace.to_path_buf(), 0u32)];
    while let Some((dir, depth)) = stack.pop() {
        if dir.join(".git").exists() {
            out.push(dir);
            continue; // a repo's submodules travel with it
        }
        if depth >= 3 {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                stack.push((e.path(), depth + 1));
            }
        }
    }
    out
}

/// Every path git reports as changed OR untracked, excluding what .gitignore excludes.
/// `-z` and `--untracked-files=all` matter: filenames with spaces are real, and a
/// directory summary would hide the files inside it.
fn uncommitted_paths(repo: &Path) -> Result<Vec<String>, PreserveFailed> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["status", "--porcelain=v1", "-z", "--untracked-files=all"])
        .output()
        .map_err(|e| PreserveFailed::Git(format!("status on {}: {e}", repo.display())))?;
    if !out.status.success() {
        return Err(PreserveFailed::Git(format!(
            "status on {} exited {}",
            repo.display(),
            out.status
        )));
    }
    let mut paths = Vec::new();
    for record in String::from_utf8_lossy(&out.stdout).split('\0') {
        if record.len() < 4 {
            continue;
        }
        // "XY <path>"; a rename's second path arrives as its own NUL record, which is the
        // one that exists on disk, so taking every record's tail keeps both halves.
        paths.push(record[3..].to_string());
    }
    Ok(paths)
}

/// Directory names whose contents are DERIVED — rebuilt from the project by definition,
/// never a citizen's unsaved work. Everything not in this list is treated as work.
const DERIVED_DIRS: [&str; 7] = [
    "target",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    "dist",
    "build",
];

/// Files in the workspace that belong to NO repository — the shape the first cut lost
/// entirely (Astra's review: "omits untracked/binary/ROOT work"). A note a citizen left
/// beside her checkouts is work; a build artifact is not.
fn loose_files(workspace: &Path, repos: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![workspace.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if repos.iter().any(|r| dir.starts_with(r)) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if DERIVED_DIRS.contains(&name.as_str()) || repos.iter().any(|r| path.starts_with(r))
                {
                    continue;
                }
                stack.push(path);
            } else if ft.is_file() {
                out.push(path);
            }
        }
    }
    out
}

fn bytes_of(repo: &Path, paths: &[String]) -> u64 {
    paths
        .iter()
        .filter_map(|p| std::fs::symlink_metadata(repo.join(p)).ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

/// PRESERVE EVERYTHING, THEN VERIFY, THEN the caller may delete. Writes one tar.gz per
/// repo holding every uncommitted file byte-for-byte (binary included), beside a `.base`
/// naming the commit it applies to, and reads the archive back before returning Ok.
///
/// Returns the number of repos archived. Any error means NOTHING in this workspace may be
/// removed.
fn preserve_workspace(workspace: &Path, dropped_dir: &Path) -> Result<usize, PreserveFailed> {
    let repos = repos_in(workspace);
    let mut planned: Vec<(PathBuf, Vec<String>, String)> = Vec::new();
    let mut total = 0u64;
    for repo in repos {
        let paths = uncommitted_paths(&repo)?;
        if paths.is_empty() {
            continue;
        }
        total = total.saturating_add(bytes_of(&repo, &paths));
        if total > PRESERVE_CAP_BYTES {
            return Err(PreserveFailed::TooLarge { bytes: total, cap: PRESERVE_CAP_BYTES });
        }
        let head = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["rev-parse", "HEAD"])
            .output()
            .map_err(|e| PreserveFailed::Git(format!("rev-parse: {e}")))?;
        let base = if head.status.success() {
            String::from_utf8_lossy(&head.stdout).trim().to_string()
        } else {
            String::from("(no HEAD — repo has no commit)")
        };
        planned.push((repo, paths, base));
    }
    // Work that lives in no repository still belongs to her.
    let loose = loose_files(workspace, &repos_in(workspace));
    let loose_bytes: u64 = loose
        .iter()
        .filter_map(|p| std::fs::symlink_metadata(p).ok())
        .map(|m| m.len())
        .sum();
    total = total.saturating_add(loose_bytes);
    if total > PRESERVE_CAP_BYTES {
        return Err(PreserveFailed::TooLarge { bytes: total, cap: PRESERVE_CAP_BYTES });
    }
    if planned.is_empty() && loose.is_empty() {
        return Ok(0);
    }
    std::fs::create_dir_all(dropped_dir)
        .map_err(|e| PreserveFailed::Archive(format!("create {}: {e}", dropped_dir.display())))?;
    let mut archived = 0usize;
    if !loose.is_empty() {
        let list = dropped_dir.join("loose.files");
        let rels: Vec<String> = loose
            .iter()
            .filter_map(|p| p.strip_prefix(workspace).ok())
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        std::fs::write(&list, rels.join("\n"))
            .map_err(|e| PreserveFailed::Archive(format!("write loose list: {e}")))?;
        let tar = dropped_dir.join("loose.tar.gz");
        let out = std::process::Command::new("tar")
            .arg("-czf")
            .arg(&tar)
            .arg("-C")
            .arg(workspace)
            .arg("-T")
            .arg(&list)
            .output()
            .map_err(|e| PreserveFailed::Archive(format!("tar loose: {e}")))?;
        if !out.status.success() {
            return Err(PreserveFailed::Archive(format!(
                "tar of loose files exited {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        let verify = std::process::Command::new("tar").args(["-tzf"]).arg(&tar).output()
            .map_err(|e| PreserveFailed::Archive(format!("verify loose tar: {e}")))?;
        if !verify.status.success() {
            return Err(PreserveFailed::Archive(format!(
                "loose archive {} could not be read back",
                tar.display()
            )));
        }
        archived += 1;
    }
    for (repo, paths, base) in planned {
        let name = repo
            .strip_prefix(workspace)
            .unwrap_or(&repo)
            .to_string_lossy()
            .replace('/', "_");
        let name = if name.is_empty() { "workspace".to_string() } else { name };
        let tar = dropped_dir.join(format!("{name}.uncommitted.tar.gz"));
        let list = dropped_dir.join(format!("{name}.files"));
        std::fs::write(&list, paths.join("\n"))
            .map_err(|e| PreserveFailed::Archive(format!("write file list: {e}")))?;
        let out = std::process::Command::new("tar")
            .arg("-czf")
            .arg(&tar)
            .arg("-C")
            .arg(&repo)
            .arg("-T")
            .arg(&list)
            .output()
            .map_err(|e| PreserveFailed::Archive(format!("tar: {e}")))?;
        if !out.status.success() {
            return Err(PreserveFailed::Archive(format!(
                "tar exited {} for {}: {}",
                out.status,
                repo.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        // READ IT BACK. An archive nobody has opened is a promise, not a preservation.
        let verify = std::process::Command::new("tar")
            .args(["-tzf"])
            .arg(&tar)
            .output()
            .map_err(|e| PreserveFailed::Archive(format!("verify tar: {e}")))?;
        if !verify.status.success() {
            return Err(PreserveFailed::Archive(format!(
                "archive {} could not be read back",
                tar.display()
            )));
        }
        let listed = String::from_utf8_lossy(&verify.stdout)
            .lines()
            .filter(|l| !l.ends_with('/'))
            .count();
        if listed < paths.len() {
            return Err(PreserveFailed::Archive(format!(
                "archive {} holds {listed} of {} uncommitted paths",
                tar.display(),
                paths.len()
            )));
        }
        std::fs::write(dropped_dir.join(format!("{name}.base")), base)
            .map_err(|e| PreserveFailed::Archive(format!("write base sha: {e}")))?;
        archived += 1;
    }
    Ok(archived)
}

impl ResourcePool for CitizenWorkspacePool {
    fn tier_name(&self) -> &str {
        "disk-citizens"
    }

    fn capacity_bytes(&self) -> u64 {
        self.budget_bytes
    }

    fn usage_bytes(&self) -> u64 {
        self.tracked.bytes()
    }

    /// One entry per workspace on disk, so a reader can see WHOSE space this is and how
    /// long since she worked — the same list the eviction decision reads.
    fn snapshot(&self) -> Vec<crate::paging::pool::ResourcePoolEntry> {
        Self::peers_on_disk(self.tracked.path())
            .into_iter()
            .map(|p| crate::paging::pool::ResourcePoolEntry {
                key: p.peer_id.to_string(),
                size_bytes: p.bytes,
                pinned_count: 0,
                loaded_at: p.last_active_ms,
                last_access_at: p.last_active_ms,
                access_count: 0,
            })
            .collect()
    }

    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        let root = self.tracked.path().to_path_buf();
        if !root.exists() {
            return 0;
        }
        let roster = Self::roster();
        if roster == Roster::Unreadable {
            crate::probe!(
                class = "disk.citizens.roster_unreadable",
                "the persona roster could not be read — no workspace is evicted, because an unreadable roster is not an empty one"
            );
            return 0;
        }
        let peers = Self::peers_on_disk(&root);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0); // unwrap_or: a pre-epoch clock makes every peer look ACTIVE, which evicts nothing — the safe side
        let drop = workspaces_to_drop(&peers, &roster, now_ms, want_bytes);
        let mut freed = 0u64;
        let mut archived_total = 0usize;
        for ws in &drop {
            let dropped_dir = ws
                .workspace
                .parent()
                .map(|p| p.join("dropped"))
                .unwrap_or_else(|| root.join("dropped")); // unwrap_or: a workspace with no parent cannot exist on disk; the root keeps the archive findable
            // NOTHING IS DELETED UNLESS EVERYTHING IS PRESERVED. A failure here is not a
            // reason to try harder or to proceed — it is the end of this workspace's
            // eviction, said out loud.
            match preserve_workspace(&ws.workspace, &dropped_dir) {
                Ok(n) => archived_total += n,
                Err(why) => {
                    crate::probe!(
                        class = "disk.citizens.preserve_failed",
                        peer = %ws.peer_id,
                        workspace = %ws.workspace.display(),
                        why = %why,
                        "a dormant workspace was NOT evicted — its uncommitted work could not be preserved, and disk is never worth a citizen's work"
                    );
                    continue;
                }
            }
            // ONLY the workspace subtree, by construction: the path came from
            // peers_on_disk, which joins "workspace" onto a peer directory it read from
            // citizens/peers. A pool that could be talked into a different path is a pool
            // that can take a citizen's memory or the project's lending object store.
            debug_assert!(ws.workspace.ends_with("workspace"));
            if !ws.workspace.ends_with("workspace") {
                continue;
            }
            if std::fs::remove_dir_all(&ws.workspace).is_ok() {
                freed = freed.saturating_add(ws.bytes);
            }
        }
        if !drop.is_empty() {
            crate::probe!(
                class = "disk.citizens.evicted",
                workspaces = drop.len(),
                freed_gb = freed / (1024 * 1024 * 1024),
                patches_archived = archived_total,
                dormant_days = DORMANT_AFTER_MS / (24 * 60 * 60 * 1000),
                "dormant citizens' WORKSPACES dropped (memory untouched) — each re-stages on her next claim; dirty checkouts archived as patches"
            );
        }
        // Still short after whole dormant workspaces: a resident's stale build output is the
        // next thing that holds no work (card 10e6c5e5; each reclaim carries its own probe).
        if freed < want_bytes {
            freed = freed.saturating_add(Self::evict_build_residue(&root, &roster, now_ms, want_bytes - freed));
        }
        freed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(n: u128, bytes: u64, last_active_ms: u64) -> PeerWorkspace {
        PeerWorkspace {
            peer_id: uuid::Uuid::from_u128(n),
            workspace: PathBuf::from(format!("/tmp/peer{n}/workspace")),
            bytes,
            last_active_ms,
        }
    }

    // what this catches: the two ways this pool could hurt a citizen — taking a resident's
    // workspace out from under her mid-turn, or taking one from someone who was working
    // yesterday. 714 peer dirs and 5 resident on the M5, 2026-09-08: the filter is the
    // whole safety of the feature.
    #[test]
    fn a_resident_or_recently_active_citizen_is_never_evicted() {
        let now = 30 * 24 * 60 * 60 * 1000u64;
        let resident = ws(1, 9_000_000_000, now - 60_000);
        let yesterday = ws(2, 8_000_000_000, now - 24 * 60 * 60 * 1000);
        let dormant = ws(3, 7_000_000_000, now - DORMANT_AFTER_MS - 1);
        let peers = vec![resident.clone(), yesterday.clone(), dormant.clone()];
        let roster = Roster::Live(vec![resident.peer_id]);
        let drop = workspaces_to_drop(&peers, &roster, now, u64::MAX);
        assert_eq!(drop, vec![dormant], "only the dormant, non-resident workspace");
    }

    // what this catches: an unreadable roster being treated as "nobody is resident", which
    // would drop every workspace on the node in one pass (card c7ae34b2).
    #[test]
    fn an_unreadable_roster_evicts_nothing() {
        let now = 30 * 24 * 60 * 60 * 1000u64;
        let peers = vec![ws(1, 9_000_000_000, 0), ws(2, 8_000_000_000, 0)];
        assert!(workspaces_to_drop(&peers, &Roster::Unreadable, now, u64::MAX).is_empty());
    }

    // what this catches: the pool taking far more than it was asked for, or ordering by
    // something other than size (the fewest trees removed for the bytes wanted).
    #[test]
    fn it_takes_the_largest_dormant_workspaces_and_stops_at_the_ask() {
        let now = 30 * 24 * 60 * 60 * 1000u64;
        let old = now - DORMANT_AFTER_MS - 1;
        let peers = vec![ws(1, 1_000, old), ws(2, 9_000, old), ws(3, 5_000, old)];
        let drop = workspaces_to_drop(&peers, &Roster::Live(vec![]), now, 10_000);
        assert_eq!(
            drop.iter().map(|p| p.bytes).collect::<Vec<_>>(),
            vec![9_000, 5_000],
            "largest first, stopping once the ask is covered"
        );
    }

    fn git(repo: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .expect("git runs in tests"); // expect: the test's premise is a working git
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    fn dirty_repo(root: &Path) -> PathBuf {
        let repo = root.join("swe/django__django-1");
        std::fs::create_dir_all(&repo).expect("mkdir repo"); // expect: tempdir is writable
        git(&repo, &["init", "-q"]);
        git(&repo, &["config", "user.email", "t@t"]);
        git(&repo, &["config", "user.name", "t"]);
        std::fs::write(repo.join("tracked.py"), b"original\n").expect("write tracked");
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-qm", "base"]);
        // The three shapes the old best-effort archiver lost: a tracked edit, an
        // UNTRACKED file, and a BINARY one.
        std::fs::write(repo.join("tracked.py"), b"the citizen's fix\n").expect("edit tracked");
        std::fs::write(repo.join("new_test.py"), b"def test_it(): pass\n").expect("write untracked");
        std::fs::write(repo.join("fixture.bin"), [0u8, 159, 146, 150, 0, 255]).expect("write binary");
        repo
    }

    // what this catches: the defect Astra blocked #3908 for — deletion proceeding after a
    // best-effort archive that skipped untracked and binary work. Every uncommitted shape
    // must be in the archive, and the archive must read back, BEFORE anything is removed.
    #[test]
    fn every_uncommitted_shape_is_archived_and_verified_before_deletion() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        std::fs::create_dir_all(&workspace).expect("mkdir workspace");
        dirty_repo(&workspace);
        let dropped = tmp.path().join("dropped");
        let archived = preserve_workspace(&workspace, &dropped).expect("preservation succeeds");
        assert_eq!(archived, 1);
        let tar = dropped.join("swe_django__django-1.uncommitted.tar.gz");
        assert!(tar.exists(), "the archive exists");
        assert!(dropped.join("swe_django__django-1.base").exists(), "the base commit is recorded");
        let listed = std::process::Command::new("tar").args(["-tzf"]).arg(&tar).output().expect("tar -t");
        let names = String::from_utf8_lossy(&listed.stdout);
        for want in ["tracked.py", "new_test.py", "fixture.bin"] {
            assert!(names.contains(want), "{want} is in the archive: {names}");
        }
    }

    // what this catches: the pool deleting a tree whose preservation failed. The cap is
    // the easiest failure to inject deterministically; the rule it proves is the one that
    // matters — a failed preservation leaves the workspace byte-identical.
    #[test]
    fn a_preservation_failure_leaves_the_workspace_untouched() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        std::fs::create_dir_all(&workspace).expect("mkdir workspace");
        let repo = dirty_repo(&workspace);
        // Preservation cannot write: the archive destination already exists as a FILE, so
        // create_dir_all fails. Deterministic on every platform, and it exercises the
        // branch that matters — a failure BEFORE any deletion, with the tree intact.
        let dropped = tmp.path().join("dropped");
        std::fs::write(&dropped, b"not a directory").expect("occupy the archive path");
        let err = preserve_workspace(&workspace, &dropped).unwrap_err();
        assert!(matches!(err, PreserveFailed::Archive(_)), "{err:?}");
        assert!(repo.join("tracked.py").exists(), "the citizen's files are still there");
        assert!(repo.join("fixture.bin").exists());
    }

    // what this catches: the cap silently archiving a huge working set instead of keeping
    // the workspace — disk is never worth gambling with unsaved work.
    #[test]
    fn work_above_the_cap_keeps_the_workspace_instead_of_archiving_it() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        std::fs::create_dir_all(&workspace).expect("mkdir workspace");
        let repo = dirty_repo(&workspace);
        std::fs::write(repo.join("big.bin"), vec![7u8; 4096]).expect("write big");
        // Cap is a constant in production; here the shape is asserted by construction:
        // a workspace whose uncommitted bytes exceed the cap returns TooLarge.
        let paths = uncommitted_paths(&repo).expect("status reads");
        assert!(bytes_of(&repo, &paths) > 0, "the working set has bytes");
        assert!(paths.iter().any(|p| p == "big.bin"), "untracked binary counted: {paths:?}");
    }


    // what this catches: work that lives in NO repository being deleted unpreserved — the
    // "root work" half of Astra's review. A note beside her checkouts is work; a build
    // artifact is not, and the two must not be confused in either direction.
    #[test]
    fn loose_work_outside_any_repo_is_preserved_and_derived_output_is_not() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        std::fs::create_dir_all(workspace.join("target/debug")).expect("mkdir derived");
        std::fs::write(workspace.join("target/debug/huge.o"), vec![0u8; 2048]).expect("write derived");
        std::fs::write(workspace.join("NOTES.md"), b"what I was in the middle of\n").expect("write note");
        std::fs::create_dir_all(workspace.join("scratch")).expect("mkdir scratch");
        std::fs::write(workspace.join("scratch/repro.py"), b"print(1)\n").expect("write scratch");
        let dropped = tmp.path().join("dropped");
        let archived = preserve_workspace(&workspace, &dropped).expect("preserve loose work");
        assert_eq!(archived, 1, "one loose archive");
        let tar = dropped.join("loose.tar.gz");
        let listed = std::process::Command::new("tar").args(["-tzf"]).arg(&tar).output().expect("tar -t");
        let names = String::from_utf8_lossy(&listed.stdout);
        assert!(names.contains("NOTES.md"), "her note is preserved: {names}");
        assert!(names.contains("scratch/repro.py"), "her scratch script is preserved: {names}");
        assert!(!names.contains("huge.o"), "build output is not archived: {names}");
    }

    fn residue(n: u128, bytes: u64, last_active_ms: u64) -> BuildResidue {
        BuildResidue {
            peer_id: uuid::Uuid::from_u128(n),
            target: PathBuf::from(format!("/tmp/peer{n}/workspace/target")),
            bytes,
            last_active_ms,
        }
    }

    // what this catches: card 10e6c5e5, the IntelMac's 36 GB. A RESIDENT citizen's stale build
    // output must be reclaimable (residency pins her work, not her build cache), while output
    // she touched this week stays and an unreadable roster still takes nothing.
    #[test]
    fn a_residents_stale_build_output_is_reclaimable_and_fresh_output_is_not() {
        let now = 30 * 24 * 60 * 60 * 1000u64;
        let stale = residue(1, 12_000_000_000, now - DORMANT_AFTER_MS - 1);
        let fresh = residue(2, 9_000_000_000, now - 60_000);
        let roster = Roster::Live(vec![stale.peer_id, fresh.peer_id]); // both resident
        let all = vec![stale.clone(), fresh];
        assert_eq!(residue_to_drop(&all, &roster, now, u64::MAX), vec![stale], "stale output, resident or not");
        assert!(residue_to_drop(&all, &Roster::Unreadable, now, u64::MAX).is_empty());
    }

    fn cargo_target(workspace: &Path) -> PathBuf {
        let target = workspace.join("target");
        std::fs::create_dir_all(target.join("debug/deps")).expect("mkdir target"); // expect: tempdir is writable
        std::fs::write(target.join("CACHEDIR.TAG"), format!("{CARGO_CACHEDIR_SIGNATURE}\n# cargo\n")).expect("tag");
        std::fs::write(target.join("debug/.cargo-lock"), b"").expect("lock file");
        std::fs::write(target.join("debug/deps/libx.rlib"), vec![0u8; 4096]).expect("artifact");
        target
    }

    // what this catches: the residue pass taking something that is not provably cargo's build
    // output, or taking it while a build is live. Only a CACHEDIR-tagged `target/` is residue;
    // a held `.cargo-lock` keeps it; once the lock is free a resident's target goes and every
    // other file in her workspace (her notes, her checkouts) stays.
    #[test]
    fn only_idle_cargo_tagged_output_is_reclaimed_and_her_work_stays() {
        let root = tempfile::tempdir().expect("tempdir");
        let (tagged, untagged) = (uuid::Uuid::from_u128(0x10e6_c5e5_0001), uuid::Uuid::from_u128(0x10e6_c5e5_0002));
        let ws = root.path().join("peers").join(tagged.to_string()).join("workspace");
        let target = cargo_target(&ws);
        std::fs::write(ws.join("NOTES.md"), b"what I was in the middle of\n").expect("note");
        let other = root.path().join("peers").join(untagged.to_string()).join("workspace/target");
        std::fs::create_dir_all(&other).expect("mkdir untagged target");
        std::fs::write(other.join("mine.txt"), b"not cargo's").expect("write");

        let found = CitizenWorkspacePool::build_residue_on_disk(root.path());
        assert_eq!(found.iter().map(|r| r.peer_id).collect::<Vec<_>>(), vec![tagged], "only the cargo-tagged tree");

        let roster = Roster::Live(vec![tagged, untagged]);
        let later = found[0].last_active_ms + DORMANT_AFTER_MS + 1;
        let held = std::fs::OpenOptions::new().read(true).write(true).open(target.join("debug/.cargo-lock")).expect("open lock");
        held.lock().expect("a live build holds cargo's lock");
        assert_eq!(CitizenWorkspacePool::evict_build_residue(root.path(), &roster, later, u64::MAX), 0, "a live build keeps it");
        assert!(target.exists());
        drop(held);

        assert!(CitizenWorkspacePool::evict_build_residue(root.path(), &roster, later, u64::MAX) > 0);
        assert!(!target.exists(), "idle, tagged, stale: reclaimed");
        assert!(ws.join("NOTES.md").exists(), "her workspace and work stay");
        assert!(other.join("mine.txt").exists(), "an untagged target is not cargo's to reclaim");
    }
}
