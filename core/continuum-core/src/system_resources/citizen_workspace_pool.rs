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

/// Archive a dirty checkout as a patch before its tree goes. Best effort by design: a
/// workspace we cannot read a diff from is still evictable, and saying so is the probe's
/// job, not a reason to keep 8 GB.
fn archive_dirty_checkouts(workspace: &Path, dropped_dir: &Path) -> usize {
    let swe = workspace.join("swe");
    let Ok(entries) = std::fs::read_dir(&swe) else {
        return 0;
    };
    let mut archived = 0usize;
    for entry in entries.flatten() {
        let repo = entry.path();
        if !repo.join(".git").exists() {
            continue;
        }
        let Ok(out) = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["diff", "HEAD"])
            .output()
        else {
            continue;
        };
        if out.stdout.is_empty() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if std::fs::create_dir_all(dropped_dir).is_err() {
            continue;
        }
        if std::fs::write(dropped_dir.join(format!("{name}.patch")), &out.stdout).is_ok() {
            archived += 1;
        }
    }
    archived
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
                .unwrap_or_else(|| root.join("dropped")); // unwrap_or: a workspace with no parent cannot exist on disk; the root keeps the patch findable
            archived_total += archive_dirty_checkouts(&ws.workspace, &dropped_dir);
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
}
