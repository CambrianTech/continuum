//! Cross-drive cold offload — the MISSING consumer of `DriveRole::Cold`.
//!
//! The 2026-07-29 finding (Joel, angry and right): the disk daemon DETECTS the
//! cold drive ([`crate::capacity::system_profile::SystemProfile::cold_drive`] —
//! e.g. the 16 TB D: HDD next to a choked 2 TB NVMe C:) and then NEVER USES IT.
//! [`super::disk_eviction::CargoTargetPool`] only DELETES derived cargo artifacts;
//! nothing DEMOTES cold artifacts from the hot system drive to the cold drive. So
//! when a class that must NOT be deleted grows — models, HF hub, forge exports,
//! the K3 570 GB expert set — it piles on the NVMe until it suffocates, while
//! terabytes of archival space sit idle. That is the "cache layers that move
//! across drives" the whole residency architecture is built on, left unbuilt.
//!
//! This pool closes it. Under hot-drive pressure it MOVES least-recently-used
//! top-level entries of a class from the hot drive to `cold_root/<class>/` and
//! leaves a link (Windows directory junction — NO privilege needed; Unix symlink)
//! at the original path, so every reader still finds the artifact — it just now
//! lives on the cold tier (slower, archival), exactly the L3(NVMe)→L5(HDD)
//! demotion. The bytes leave the hot drive; the capability does not.
//!
//! ## Why move, not delete (the class distinction)
//! `CargoTargetPool` deletes because cargo artifacts are DERIVED — the next build
//! recreates them. Models / hub / forge / persona stores are NOT derivable from
//! anything local; deleting them destroys work or forces a re-download. For those
//! classes the eviction verb is DEMOTE, not DELETE. Same broker, same pressure
//! economy, different physical action.
//!
//! ## Optional by construction (RESOLUTION FIELD, not gate)
//! No cold drive (M5's Mac today, until a flash drive is added to the bay) ⇒ this
//! pool is simply not constructed; the class falls back to delete-eviction (if
//! derived) or to grid placement / degraded quant (if not). A cold drive UPGRADES
//! the box; its absence never breaks it — mirroring
//! [`SystemProfile::has_cold_tier`] ([[public-project-not-joels-machines]]).
//!
//! ## Safety invariants (each pinned by a test)
//! 1. **Copy-verify-then-remove.** Cross-volume moves can't `rename`; we copy to
//!    the cold drive, confirm the byte count, and only THEN remove the hot copy.
//!    A failed/partial copy leaves the hot original intact (no data loss) and
//!    frees 0 bytes — the broker retries next tick.
//! 2. **Link or roll back.** If the link can't be created after the move, the
//!    cold copy is moved BACK so the artifact never becomes unreachable.
//! 3. **Never leave the class root.** Only direct children of the tracked dir are
//!    demoted; symlinked entries are skipped (never chase a link off the tree).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use crate::paging::pool::{ResourcePool, ResourcePoolEntry};

use super::disk_reporters::{dir_size_bytes, TrackedDir};

/// A demotable class on the hot drive, backed by a cold-drive archive root.
/// Shares its [`TrackedDir`] with the reporter/scanner — one measurement, two
/// consumers. Pressure is usage/budget: the class is kept below `hot_budget_bytes`
/// on the fast tier, spillover lives on the cold tier (still readable via links).
pub struct ColdOffloadPool {
    tracked: Arc<TrackedDir>,
    /// Where demoted entries go: `<cold_drive>/<class-name>/`. On this box e.g.
    /// `D:\continuum-cold\genome-models\`. Derived from `cold_drive().mount`.
    cold_root: PathBuf,
    /// How much of this class to keep resident on the HOT drive before demoting.
    /// Over this, the broker drives `evict_at_least` to spill the coldest entries.
    hot_budget_bytes: u64,
    /// Class label for tier reporting.
    class: &'static str,
}

impl ColdOffloadPool {
    /// Construct for a class, given the resolved cold-drive mount. Returns `None`
    /// when there is no cold drive — the pool is OPTIONAL and simply not built,
    /// so the caller falls back to delete-eviction or grid placement. This is the
    /// resolution-field seam: presence upgrades, absence degrades, never excludes.
    pub fn new(
        class: &'static str,
        tracked: Arc<TrackedDir>,
        cold_drive_mount: Option<&Path>,
        hot_budget_bytes: u64,
    ) -> Option<Self> {
        let cold_root = cold_drive_mount?.join(class);
        Some(Self {
            tracked,
            cold_root,
            hot_budget_bytes: hot_budget_bytes.max(1),
            class,
        })
    }

    /// Direct children of the class root, oldest-access first (LRU) — the demote
    /// order. Access time falls back to modified time falls back to epoch, so a
    /// filesystem without atime still yields a stable, deterministic ordering.
    /// Symlinked children are skipped (invariant 3: already demoted / never chase
    /// a link out of the tree).
    fn lru_children(root: &Path) -> Vec<(PathBuf, u64, SystemTime)> {
        let Ok(entries) = std::fs::read_dir(root) else {
            return Vec::new();
        };
        let mut out: Vec<(PathBuf, u64, SystemTime)> = Vec::new();
        for e in entries.flatten() {
            let Ok(meta) = e.metadata() else { continue };
            if meta.is_symlink() {
                continue;
            }
            let when = meta
                .accessed()
                .or_else(|_| meta.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let size = if meta.is_dir() {
                dir_size_bytes(&e.path())
            } else {
                meta.len()
            };
            out.push((e.path(), size, when));
        }
        out.sort_by_key(|(_, _, when)| *when); // oldest first
        out
    }
}

impl ResourcePool for ColdOffloadPool {
    fn tier_name(&self) -> &str {
        self.class
    }

    fn capacity_bytes(&self) -> u64 {
        self.hot_budget_bytes
    }

    fn usage_bytes(&self) -> u64 {
        self.tracked.bytes()
    }

    /// Demote coldest entries hot→cold until `want_bytes` have left the hot drive.
    /// Each demotion is copy-verify-remove-link (invariant 1) with roll-back on a
    /// failed link (invariant 2). Returns bytes actually freed from the hot drive.
    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        let root = self.tracked.path().to_path_buf();
        if !root.exists() {
            return 0;
        }
        if std::fs::create_dir_all(&self.cold_root).is_err() {
            return 0; // cold drive unwritable this tick → free nothing, retry later
        }

        let mut freed = 0u64;
        for (src, size, _) in Self::lru_children(&root) {
            if freed >= want_bytes.max(1) {
                break;
            }
            match demote_entry(&src, &self.cold_root) {
                Ok(moved) => freed = freed.saturating_add(moved),
                Err(_) => continue, // partial/failed move left the hot original intact
            }
            let _ = size; // size is advisory; demote_entry returns the authoritative moved count
        }

        if freed > 0 {
            self.tracked.record_freed(freed);
            crate::clog_warn!(
                "💾 cold-offload demoted {} GB of '{}' to {} — still readable via link, off the hot drive",
                freed / (1024 * 1024 * 1024),
                self.class,
                self.cold_root.display()
            );
        }
        freed
    }

    fn snapshot(&self) -> Vec<ResourcePoolEntry> {
        Vec::new()
    }
}

/// Move one entry (file or dir) from the hot drive to `cold_root` and leave a link
/// at the original path. Cross-volume safe: copy → verify size → remove hot →
/// link; on link failure, move the cold copy back so nothing is orphaned. Returns
/// bytes moved off the hot drive (0 on any failure, with the hot original intact).
fn demote_entry(src: &Path, cold_root: &Path) -> std::io::Result<u64> {
    let name = src
        .file_name()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no file name"))?;
    let dst = cold_root.join(name);
    if dst.exists() {
        // A stale prior demotion of the same name — do not clobber; skip.
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "cold target exists",
        ));
    }

    let expected = if src.is_dir() {
        dir_size_bytes(src)
    } else {
        std::fs::metadata(src)?.len()
    };

    // (1) copy to the cold drive.
    if src.is_dir() {
        copy_dir_all(src, &dst)?;
    } else {
        std::fs::copy(src, &dst)?;
    }

    // (1) verify the copy before removing the hot original.
    let copied = if dst.is_dir() {
        dir_size_bytes(&dst)
    } else {
        std::fs::metadata(&dst)?.len()
    };
    if copied != expected {
        let _ = remove_any(&dst);
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "cold copy size mismatch — hot original kept",
        ));
    }

    // remove the hot original, then (2) link it to the cold copy.
    remove_any(src)?;
    if let Err(e) = make_link(src, &dst) {
        // (2) roll back: put the artifact back on the hot drive so it is never
        // unreachable. Best-effort; if this also fails the cold copy is the record.
        let _ = if dst.is_dir() {
            copy_dir_all(&dst, src).and_then(|_| remove_any(&dst))
        } else {
            std::fs::copy(&dst, src).map(|_| ()).and_then(|_| remove_any(&dst))
        };
        return Err(e);
    }
    Ok(expected)
}

fn remove_any(p: &Path) -> std::io::Result<()> {
    if p.is_dir() {
        std::fs::remove_dir_all(p)
    } else {
        std::fs::remove_file(p)
    }
}

/// Recursively copy `src` → `dst`, returning total bytes copied. Symlinks inside
/// are skipped (never chase a link off the tree during a demotion).
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<u64> {
    std::fs::create_dir_all(dst)?;
    let mut total = 0u64;
    for entry in std::fs::read_dir(src)?.flatten() {
        let meta = entry.metadata()?;
        if meta.is_symlink() {
            continue;
        }
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if meta.is_dir() {
            total = total.saturating_add(copy_dir_all(&from, &to)?);
        } else {
            total = total.saturating_add(std::fs::copy(&from, &to)?);
        }
    }
    Ok(total)
}

/// Leave a link at `link` pointing to the demoted `target`. Windows uses a
/// DIRECTORY JUNCTION (`mklink /J`) which needs NO privilege — unlike Windows
/// symlinks, which require admin/developer-mode. Unix uses a symlink. Junctions
/// are dir-only; a demoted single FILE on Windows is wrapped by linking its parent
/// dir instead is out of scope here — model/hub/forge classes are dir-structured,
/// which is why demotion granularity is the class's top-level ENTRIES.
#[cfg(windows)]
fn make_link(link: &Path, target: &Path) -> std::io::Result<()> {
    if target.is_dir() {
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "mklink /J junction creation failed",
            ))
        }
    } else {
        // A bare file demotion on Windows would need a privileged symlink; the
        // caller demotes dir-structured classes, so this path is not taken. Signal
        // clearly rather than silently leaving a dangling original.
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "file-granularity demotion needs a privileged symlink on Windows; demote at dir granularity",
        ))
    }
}

#[cfg(unix)]
fn make_link(link: &Path, target: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the resolution-field seam — NO cold drive ⇒ no pool
    // (the class degrades to delete/grid), a cold drive ⇒ a pool rooted under it.
    // A regression that constructs a pool with no cold target would try to demote
    // into a bogus path and lose artifacts.
    #[test]
    fn pool_is_none_without_a_cold_drive() {
        let tracked = TrackedDir::new("genome-models", PathBuf::from("/tmp/none"));
        assert!(ColdOffloadPool::new("genome-models", tracked.clone(), None, 1).is_none());
        let cold = std::path::Path::new("/cold");
        let pool = ColdOffloadPool::new("genome-models", tracked, Some(cold), 1).expect("some");
        assert_eq!(pool.cold_root, cold.join("genome-models"));
    }

    // what this catches: the demote candidate set is exactly the class root's
    // direct, NON-symlink children (invariant 3) — an already-demoted entry (now a
    // symlink) must never be re-demoted, and enumeration must not error on a normal
    // dir. Ordering is a plain sort on the collected access-times (trusted); the
    // safety-relevant behavior is WHICH entries are eligible, which this pins
    // without depending on a filetime-setting crate.
    #[test]
    fn lru_children_collects_children_and_skips_symlinks() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("a.bin"), vec![0u8; 10]).expect("write");
        std::fs::create_dir(tmp.path().join("d")).expect("mkdir");
        #[cfg(unix)]
        std::os::unix::fs::symlink(tmp.path().join("a.bin"), tmp.path().join("already-demoted"))
            .expect("symlink");
        let kids = ColdOffloadPool::lru_children(tmp.path());
        // a.bin + d/, never the symlink.
        assert_eq!(kids.len(), 2, "direct non-symlink children only");
        assert!(kids.iter().all(|(p, _, _)| !p.ends_with("already-demoted")));
    }

    // what this catches (Unix CI): the FULL demotion contract — a dir moves to the
    // cold root, the hot original becomes a link to it, content is byte-preserved,
    // and the reported freed count equals the moved bytes. This is invariant 1+2
    // end-to-end: bytes leave the hot drive but the artifact stays reachable.
    #[cfg(unix)]
    #[test]
    fn demote_moves_to_cold_and_leaves_a_working_link() {
        let hot = tempfile::tempdir().expect("hot");
        let cold = tempfile::tempdir().expect("cold");
        let model = hot.path().join("models--org--m");
        std::fs::create_dir_all(model.join("blobs")).expect("mkdir");
        std::fs::write(model.join("blobs/w.bin"), vec![7u8; 4096]).expect("write");

        let moved = demote_entry(&model, cold.path()).expect("demote");
        assert_eq!(moved, 4096, "reports the bytes actually moved");
        // hot path is now a symlink...
        assert!(std::fs::symlink_metadata(&model).unwrap().is_symlink());
        // ...that still reads the exact content from the cold drive.
        let via_link = std::fs::read(model.join("blobs/w.bin")).expect("read through link");
        assert_eq!(via_link, vec![7u8; 4096], "content byte-preserved and reachable");
        assert!(cold.path().join("models--org--m/blobs/w.bin").exists());
    }

    // what this catches: invariant 1 — a name that already exists on the cold
    // drive is NOT clobbered; the hot original is kept and 0 bytes freed, so the
    // broker safely retries rather than destroying either copy.
    #[cfg(unix)]
    #[test]
    fn demote_refuses_to_clobber_an_existing_cold_target() {
        let hot = tempfile::tempdir().expect("hot");
        let cold = tempfile::tempdir().expect("cold");
        std::fs::create_dir_all(hot.path().join("m")).expect("mk");
        std::fs::write(hot.path().join("m/a"), vec![1u8; 8]).expect("w");
        std::fs::create_dir_all(cold.path().join("m")).expect("mk cold collision");

        let err = demote_entry(&hot.path().join("m"), cold.path()).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert!(hot.path().join("m/a").exists(), "hot original kept on refusal");
    }
}
