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
//! 45 GB, unevictable by design while she stays resident.
//!
//! It is reclaimed ONLY on request, through `system/reclaim-build-residue`, never by pressure
//! relief (Codex's decision on #4528). Cargo 1.95 locks only `target/<profile>/.cargo-lock`,
//! never the tree, so no lock this pass can take spans a build that starts a NEW profile, and a
//! session may point its own CARGO_TARGET_DIR anywhere. Every substrate cargo writes to the
//! shared cache, which is why these trees are residue, but "no writer" is not provable from
//! outside cargo. So the reclaim is an explicit, quiescent act, and even then it takes one
//! thing inside a workspace, `workspace/target`, only when every one of these holds:
//!
//! - **Rebuildable, proven, not named:** the tree carries the cachedir signature AND cargo's
//!   `.rustc_info.json`. A directory called `target` that cargo did not write is left alone.
//! - **No existing profile is building:** every `.cargo-lock` under it takes an exclusive,
//!   non-blocking lock (cargo's own per-profile lock). A DEFENSE, not a proof: it cannot see
//!   a build that starts a profile with no lock file yet.
//! - **The node is quiescent:** no cargo or rustc anywhere on the host, no deploy and no
//!   citizen turn in flight, checked before the pass and again before each take. A snapshot,
//!   not a proof (see [`node_quiescence`]).
//! - **No solve is live:** the reclaim holds the citizen's hands (`hands_lease::PersonaHandsLease`) for the
//!   whole transaction, so a staged solve cannot start inside a tree being removed.
//! - **The roster is readable** (unreadable = nothing, as for whole workspaces).
//! - **Untouched for [`DORMANT_AFTER_MS`]:** policy.
//!
//! None of these, alone or together, proves no writer exists (Codex on #4528); together they
//! narrow the race to a build that starts in the instant before a take, which is why the
//! reclaim is operator-invoked, never automatic.
//!
//! Nothing is preserved, because a cargo-owned tree holds no work. Cargo's locks are HELD
//! across the take: the tree is renamed aside under them, so no build of an EXISTING profile
//! is inside it when it is deleted (Windows cannot rename under a held lock, so there the
//! reclaim is deferred).
//! Any read error on the way refuses. Staged `swe/` checkouts in a resident
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

use continuum_base::time::EpochMs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::paging::pool::ResourcePool;
use crate::system_resources::disk_reporters::TrackedDir;

pub(crate) mod cargo_target;
pub(crate) use cargo_target::is_cargo_target;
use cargo_target::{hold_cargo_locks, take_target, ResidueKept, CARGO_CACHEDIR_SIGNATURE, PARKED_PREFIX};
#[cfg(test)]
use cargo_target::HeldCargoLocks;

/// A dormant citizen keeps her workspace for this long after her last activity. Long
/// enough that a citizen despawned for a measurement window and re-seated the next day
/// keeps her staged checkouts; short enough that a benchmark roster from last week is not
/// still holding 8 GB.
pub const DORMANT_AFTER_MS: u64 = 7 * 24 * 60 * 60 * 1000;

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
        match crate::runtime::extensions::roster() {
            Some(ids) => Roster::Live(ids.into_iter().collect()),
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
            let (bytes, newest) = dir_bytes_and_newest_to(&target, u32::MAX);
            out.push(BuildResidue { peer_id, target, bytes, last_active_ms: newest });
        }
        out
    }

    /// Delete trees an earlier take parked but did not finish deleting (a crash mid-delete).
    /// Only `peers/<peer>/workspace/.target-reclaim-*`, which nothing builds into.
    fn sweep_parked(root: &Path) -> u64 {
        let Ok(peers) = std::fs::read_dir(root.join("peers")) else {
            return 0;
        };
        let mut freed = 0u64;
        for peer in peers.flatten() {
            let Ok(entries) = std::fs::read_dir(peer.path().join("workspace")) else { continue };
            for e in entries.flatten() {
                if e.file_name().to_string_lossy().starts_with(PARKED_PREFIX) {
                    let (bytes, _) = dir_bytes_and_newest_to(&e.path(), u32::MAX);
                    match std::fs::remove_dir_all(e.path()) {
                        Ok(()) => freed = freed.saturating_add(bytes),
                        Err(err) => continuum_base::probe!(
                            class = "disk.citizens.residue_kept",
                            why = "delete_incomplete",
                            detail = %format!("{}: {err}", e.path().display()),
                            "a parked build tree still could not be deleted; the next pass tries again"
                        ),
                    }
                }
            }
        }
        freed
    }

    /// The explicit reclaim (`system/reclaim-build-residue`): every stale cargo build tree under
    /// `root`, residents included, each reported. With `apply = false` nothing is touched: each
    /// tree is judged under the same gates and reported as it WOULD be decided.
    pub fn reclaim_build_residue(root: &Path, home: &Path, now_ms: u64, apply: bool) -> Vec<ResidueOutcome> {
        let home = home.to_path_buf();
        Self::reclaim_with_roster(root, &Self::roster(), now_ms, apply, &move || node_quiescence(&home))
    }

    /// `quiescent` is the node-wide gate (Fable on #4528: "quiescent" must be enforced, not
    /// a doc sentence): `Err(why)` refuses. It is asked once before anything is judged and
    /// again immediately before each take, because a build or turn can start in between.
    fn reclaim_with_roster(
        root: &Path,
        roster: &Roster,
        now_ms: u64,
        apply: bool,
        quiescent: &dyn Fn() -> Result<(), String>,
    ) -> Vec<ResidueOutcome> {
        if *roster == Roster::Unreadable {
            return vec![ResidueOutcome::refused_all("the persona roster could not be read, so nothing is touched")];
        }
        if let Err(why) = quiescent() {
            if apply {
                return vec![ResidueOutcome::refused_all(&format!("the node is not quiescent: {why}"))];
            }
            // a dry run still reports every tree, and says the apply would be refused now
            let mut out = vec![ResidueOutcome::refused_all(&format!("an apply now would be refused: {why}"))];
            out.extend(Self::reclaim_with_roster(root, roster, now_ms, false, &|| Ok(())).into_iter());
            return out;
        }
        let mut out = Vec::new();
        if apply {
            let swept = Self::sweep_parked(root);
            if swept > 0 {
                out.push(ResidueOutcome::swept(swept));
            }
        }
        let residue = Self::build_residue_on_disk(root);
        for r in residue_to_drop(&residue, roster, now_ms, u64::MAX) {
            let idle_days = now_ms.saturating_sub(r.last_active_ms) / (24 * 60 * 60 * 1000);
            // ONLY `peers/<peer>/workspace/target`, by construction (build_residue_on_disk)
            if !r.target.ends_with("workspace/target") {
                continue;
            }
            // her hands for the whole transaction: no staged solve starts inside this tree
            let Some(_hands) = crate::system_resources::hands_lease::PersonaHandsLease::try_take(r.peer_id) else {
                out.push(ResidueOutcome::of(&r, idle_days, Err("solve_live".into())));
                continue;
            };
            let decided = match hold_cargo_locks(&r.target) {
                Err(kept) => Err(format!("{}: {}", kept.why(), kept.detail())),
                Ok(_locks) if !apply => Ok(false), // judged idle; a dry run takes nothing
                // asked again right before the take: a build or turn may have started since
                Ok(locks) => match quiescent() {
                    Err(why) => Err(format!("not_quiescent: {why}")),
                    Ok(()) => take_target(&r.target, locks)
                        .map(|()| true)
                        .map_err(|kept| format!("{}: {}", kept.why(), kept.detail())),
                },
            };
            let outcome = ResidueOutcome::of(&r, idle_days, decided);
            continuum_base::probe!(
                class = "disk.citizens.residue",
                peer = %r.peer_id,
                decision = outcome.decision.as_str(),
                bytes = r.bytes,
                idle_days,
                "an explicit build-residue reclaim decided one citizen's cargo target"
            );
            out.push(outcome);
        }
        out
    }
}

/// Is the node quiet enough to take build trees (Fable's gates on #4528)? Each gate is a
/// SNAPSHOT, not a proof of ownership (Codex): it is taken right before each take, so it
/// narrows what can race the rename to a build or turn that starts in the instant between the
/// check and the rename. It reaches writers lock enumeration cannot see (a session's own
/// `--target-dir`, an external build, a cargo starting a new profile) only as far as they are
/// running when it looks. That residual is why this is an explicit, operator-invoked act and
/// never pressure relief.
/// - no `cargo` or `rustc` process anywhere on the host (`deploy_claim::is_compiler`);
/// - no deploy in flight (`deploy_claim::in_flight(..).excludes_deploy()`);
/// - no citizen turn in flight (`turn_ingress::in_flight()`).
/// `Err` names the first gate that refuses.
pub(crate) fn node_quiescence(home: &Path) -> Result<(), String> {
    let Some(turns) = crate::runtime::extensions::turns_in_flight() else {
        return Err("citizen_turns_unknown: nothing installed the turn count".to_string());
    };
    if turns > 0 {
        return Err(format!("citizen_turn_in_flight: {turns}"));
    }
    let now_ms = EpochMs::now().millis();
    if continuum_cli_lifecycle::deploy_claim::in_flight(home, now_ms).excludes_deploy() {
        return Err("deploy_in_flight".to_string());
    }
    let mut sys = sysinfo::System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    if let Some((pid, name)) = sys.processes().iter().find_map(|(pid, p)| {
        let name = p.name().to_string_lossy().into_owned();
        continuum_cli_lifecycle::deploy_claim::is_compiler(&name).then(|| (pid.as_u32(), name))
    }) {
        return Err(format!("compiler_running: {name} (pid {pid})"));
    }
    Ok(())
}

/// What the explicit reclaim decided for one tree (or for the whole pass).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/system/ResidueOutcome.ts")]
pub struct ResidueOutcome {
    /// The citizen (her peer id), or empty for a pass-wide line.
    pub peer: String,
    pub path: String,
    #[ts(type = "number")]
    pub bytes: u64,
    #[ts(type = "number")]
    pub idle_days: u64,
    /// `reclaimed`, `would_reclaim` (dry run, proven idle), `kept`, `swept` (parked leftovers),
    /// or `refused` (nothing judged).
    pub decision: String,
    /// Why it was kept or refused (`solve_live`, `build_live: <lock>`, `unreadable: …`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub why: Option<String>,
}

impl ResidueOutcome {
    fn of(r: &BuildResidue, idle_days: u64, decided: Result<bool, String>) -> Self {
        let (decision, why) = match decided {
            Ok(true) => ("reclaimed", None),
            Ok(false) => ("would_reclaim", None),
            Err(why) => ("kept", Some(why)),
        };
        Self {
            peer: r.peer_id.to_string(),
            path: r.target.display().to_string(),
            bytes: r.bytes,
            idle_days,
            decision: decision.to_string(),
            why,
        }
    }
    fn swept(bytes: u64) -> Self {
        Self { peer: String::new(), path: String::new(), bytes, idle_days: 0, decision: "swept".into(), why: None }
    }
    fn refused_all(why: &str) -> Self {
        Self { peer: String::new(), path: String::new(), bytes: 0, idle_days: 0, decision: "refused".into(), why: Some(why.into()) }
    }
}

/// Total bytes and newest mtime under a directory, bounded to a depth that cannot walk a
/// whole repo's history: the numbers here decide eviction, so they are read from the tree
/// rather than remembered, but a full recursive walk of 714 workspaces on every tick is
/// exactly the hot-path cost this substrate forbids.
fn dir_bytes_and_newest(dir: &Path) -> (u64, u64) {
    dir_bytes_and_newest_to(dir, 6)
}

/// [`dir_bytes_and_newest`] to a chosen depth. A cargo target needs the whole tree: its
/// freshest files live in `incremental/<crate>/<session>/` and `build/<crate>/out/**`, below
/// the workspace walk's depth (Fable on #4528), and reading them only on a residue decision
/// (under pressure, never per tick) is what makes "stale" true.
fn dir_bytes_and_newest_to(dir: &Path, max_depth: u32) -> (u64, u64) {
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
                if let Ok(at) = continuum_base::time::EpochMs::of(modified) {
                    newest = newest.max(at.millis());
                }
            }
            if meta.is_dir() {
                if depth < max_depth {
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
        let out = continuum_base::shell_portable::tar_on(&tar, "-czf")
            .map_err(PreserveFailed::Archive)?
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
        let verify = continuum_base::shell_portable::tar_on(&tar, "-tzf")
            .map_err(PreserveFailed::Archive)?
            .output()
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
        let out = continuum_base::shell_portable::tar_on(&tar, "-czf")
            .map_err(PreserveFailed::Archive)?
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
        let verify = continuum_base::shell_portable::tar_on(&tar, "-tzf")
            .map_err(PreserveFailed::Archive)?
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
            continuum_base::probe!(
                class = "disk.citizens.roster_unreadable",
                "the persona roster could not be read — no workspace is evicted, because an unreadable roster is not an empty one"
            );
            return 0;
        }
        let peers = Self::peers_on_disk(&root);
        let now_ms = EpochMs::now().millis(); // pre-1970 reads 0: a pre-epoch clock makes every peer look ACTIVE, which evicts nothing — the safe side
        let drop = workspaces_to_drop(&peers, &roster, now_ms, want_bytes);
        let mut freed = 0u64;
        let mut archived_total = 0usize;
        for ws in &drop {
            let Some(_hands) = crate::system_resources::hands_lease::PersonaHandsLease::try_take(ws.peer_id) else {
                continue;
            };
            // Discovery publishes child ownership under these same hands. A
            // whole-workspace deletion cannot bypass the child's Cargo locks,
            // active-image checks, or separately observed broker accounting.
            if !matches!(super::disk_reporters::citizen_has_cargo_owner(ws.peer_id), Ok(false)) {
                continue;
            }
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
                    continuum_base::probe!(
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
            continuum_base::probe!(
                class = "disk.citizens.evicted",
                workspaces = drop.len(),
                freed_gb = freed / (1024 * 1024 * 1024),
                patches_archived = archived_total,
                dormant_days = DORMANT_AFTER_MS / (24 * 60 * 60 * 1000),
                "dormant citizens' WORKSPACES dropped (memory untouched) — each re-stages on her next claim; dirty checkouts archived as patches"
            );
        }
        // A RESIDENT's build output is NOT reclaimed here (Codex on #4528): cargo has no
        // exclusion spanning every entry path, so automatic pressure relief cannot prove no
        // writer exists. It is reclaimed only through `system/reclaim-build-residue`.
        freed
    }
}

#[cfg(test)]
mod tests;
