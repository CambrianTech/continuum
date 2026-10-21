//! Cargo's own build output inside a citizen's workspace: how a tree is proven to be
//! cargo's, how its build locks are held across a reclaim, and how it is taken.

use std::path::{Path, PathBuf};

/// The cachedir-spec signature cargo writes as the first line of `CACHEDIR.TAG`. Generic to
/// the spec, so [`is_cargo_target`] pairs it with a cargo-only file.
pub(super) const CARGO_CACHEDIR_SIGNATURE: &str = "Signature: 8a477f597d28d172789f06886806bc55";

/// Is `dir` a tree cargo wrote? The cachedir signature alone is a GENERIC cache marker any
/// tool may write (Codex on #4528), so it must sit beside `.rustc_info.json`, the file cargo
/// writes at the root of every target dir it owns.
pub(crate) fn is_cargo_target(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("CACHEDIR.TAG")).is_ok_and(|t| t.starts_with(CARGO_CACHEDIR_SIGNATURE))
        && dir.join(".rustc_info.json").is_file()
}

/// Why a residue tree was kept. Every one of them is a refusal: a tree this pass cannot
/// prove idle and take whole is never touched (Codex on #4528: fail closed).
#[derive(Debug)]
pub(super) enum ResidueKept {
    /// Cargo holds this build lock: a build is live in the tree.
    BuildLive(PathBuf),
    /// The tree or one of its locks could not be read, so idleness is unproven.
    Unreadable(String),
    /// The tree was taken (parked, no longer `target`) but its delete stopped part way; the
    /// remainder is swept by the next pass. Not a keep: named so a partial is never silent.
    DeleteIncomplete(String),
    /// This platform cannot move a directory while holding a lock inside it, so the
    /// exclusion cannot span the take; the reclaim is deferred rather than raced.
    #[cfg(not(unix))]
    Unsupported,
}

impl ResidueKept {
    pub(super) fn why(&self) -> &'static str {
        match self {
            Self::BuildLive(_) => "build_live",
            Self::Unreadable(_) => "unreadable",
            Self::DeleteIncomplete(_) => "delete_incomplete",
            #[cfg(not(unix))]
            Self::Unsupported => "unsupported_platform",
        }
    }
    pub(super) fn detail(&self) -> String {
        match self {
            Self::BuildLive(lock) => lock.display().to_string(),
            Self::Unreadable(e) | Self::DeleteIncomplete(e) => e.clone(),
            #[cfg(not(unix))]
            Self::Unsupported => "no atomic move under a held lock on this platform".into(),
        }
    }
}

/// Every `.cargo-lock` in a target dir (the artifact-dir lock at its root and each profile's
/// build-dir lock, `target/[<triple>/]<profile>/.cargo-lock`). Any read error is an `Err`:
/// a lock this walk could not see is a build it cannot rule out.
fn cargo_locks_in(target: &Path) -> Result<Vec<PathBuf>, ResidueKept> {
    let unreadable = |p: &Path, e: std::io::Error| ResidueKept::Unreadable(format!("{}: {e}", p.display()));
    let mut out = Vec::new();
    let mut stack = vec![(target.to_path_buf(), 0u32)];
    while let Some((dir, depth)) = stack.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|e| unreadable(&dir, e))? {
            let entry = entry.map_err(|e| unreadable(&dir, e))?;
            let path = entry.path();
            if entry.file_name() == ".cargo-lock" {
                out.push(path);
            } else if depth < 2 && entry.file_type().map_err(|e| unreadable(&path, e))?.is_dir() {
                stack.push((path, depth + 1));
            }
        }
    }
    Ok(out)
}

/// Cargo's build locks on one target tree, held by this pass, released EXPLICITLY on drop.
/// A flock belongs to the open file description, not to our descriptor: a process forked
/// while we hold it (the core spawns git, engines and builds; tests fork in parallel)
/// inherits that description until its exec, so a plain close leaves the lock held for that
/// window and the next pass reads an idle tree as a live build. Unlocking releases it for
/// every descriptor sharing the description.
#[derive(Debug)]
pub(super) struct HeldCargoLocks(pub(super) Vec<std::fs::File>);

impl Drop for HeldCargoLocks {
    fn drop(&mut self) {
        for lock in &self.0 {
            if let Err(error) = lock.unlock() {
                crate::probe!(
                    class = "disk.citizens.residue",
                    error = %error,
                    "a held cargo build lock did not unlock; it is released when the last descriptor closes"
                );
            }
        }
    }
}

/// Take cargo's own locks on every build lock in the tree and HOLD them: the returned
/// [`HeldCargoLocks`] keep the exclusion until they drop. A lock held elsewhere means a live
/// build; a lock that cannot be opened or locked for any other reason is unproven, and both
/// refuse.
pub(super) fn hold_cargo_locks(target: &Path) -> Result<HeldCargoLocks, ResidueKept> {
    let mut held = Vec::new();
    for lock in cargo_locks_in(target)? {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock)
            .map_err(|e| ResidueKept::Unreadable(format!("{}: {e}", lock.display())))?;
        match file.try_lock() {
            Ok(()) => held.push(file),
            Err(std::fs::TryLockError::WouldBlock) => return Err(ResidueKept::BuildLive(lock)),
            Err(std::fs::TryLockError::Error(e)) => {
                return Err(ResidueKept::Unreadable(format!("{}: {e}", lock.display())))
            }
        }
    }
    Ok(HeldCargoLocks(held))
}

/// The prefix a reclaimed tree is moved to before it is deleted. A leftover one (a delete
/// interrupted by a crash) is swept by the next pass: nothing builds into it, because cargo
/// only ever addresses `target`.
pub(super) const PARKED_PREFIX: &str = ".target-reclaim-";

/// THE TAKE, with cargo's exclusion spanning it (Codex on #4528: releasing the locks before
/// the delete let a build start inside a tree being removed). While every build lock is held
/// the tree is renamed aside, which is atomic on one filesystem, so from that instant any new
/// build creates a fresh `target`; only then are the locks released and the parked tree
/// deleted. Windows cannot rename a directory holding an open handle inside it, so there the
/// exclusion cannot span the take and the reclaim is deferred.
///
/// The one residual shape, named rather than hidden: a cargo that began waiting on the lock
/// inside the take's own microseconds is released holding the PARKED file's lock, and it then
/// builds into a fresh `target`, where a second cargo starting at that moment would take a new
/// lock. Two builds queued inside one rename is the whole exposure, and it costs a rebuild,
/// never work.
#[cfg(unix)]
pub(super) fn take_target(target: &Path, locks: HeldCargoLocks) -> Result<(), ResidueKept> {
    let parked = target.with_file_name(format!("{PARKED_PREFIX}{}", uuid::Uuid::new_v4()));
    std::fs::rename(target, &parked)
        .map_err(|e| ResidueKept::Unreadable(format!("park {}: {e}", target.display())))?;
    drop(locks);
    std::fs::remove_dir_all(&parked).map_err(|e| ResidueKept::DeleteIncomplete(format!("{}: {e}", parked.display())))
}

#[cfg(not(unix))]
pub(super) fn take_target(_target: &Path, _locks: HeldCargoLocks) -> Result<(), ResidueKept> {
    Err(ResidueKept::Unsupported)
}
