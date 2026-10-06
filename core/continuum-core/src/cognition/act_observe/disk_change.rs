//! Did an act batch change her checkout? Measured, never read off a command's NAME.
//!
//! `wrote` used to be a substring match on the verb name (write/edit/apply/commit), so a
//! write that reached disk through `code/shell` (`sed -i`, a heredoc, `git apply`,
//! `git commit`) counted as reading, and `writes 0` read as a statement about the citizen
//! when it was the counter that could not see (Joel, 2026-10-06: "0 writes always a
//! serious plumbing bug"). The checkout is fingerprinted before and after the batch; a
//! different fingerprint is a write, by whatever verb.

use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::Duration;

/// Bound on each git read; a checkout on a cold disk answers well inside it, and a hang
/// is an unreadable fingerprint, never waited out.
const READ_BOUND: Duration = Duration::from_secs(5);

/// PURE: the fingerprint of a checkout from what git and the filesystem said: HEAD, the
/// porcelain status, and each listed path's size and modification time. The stats matter
/// because a file that is ALREADY dirty and is edited again leaves the porcelain unchanged.
pub(crate) fn fingerprint(head: &str, porcelain: &str, stats: &[(String, u64, u128)]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    head.trim().hash(&mut h);
    porcelain.hash(&mut h);
    stats.hash(&mut h);
    h.finish()
}

/// The paths a `git status --porcelain=v1 -z` listing names (renames carry two; both
/// are taken, which only makes the fingerprint more sensitive, never less).
fn porcelain_paths(porcelain: &str) -> Vec<String> {
    porcelain
        .split('\0')
        .filter(|e| !e.is_empty())
        .map(|e| if e.len() > 3 { e[3..].to_string() } else { e.to_string() })
        .collect()
}

/// The checkout's fingerprint, or `None` when it could not be read (no checkout, git
/// refused, a read timed out). Blocking: call through `spawn_blocking`.
pub(crate) fn checkout_fingerprint(root: &Path) -> Option<u64> {
    use crate::system_resources::bounded_command::probe;
    let root_s = root.to_string_lossy().to_string();
    let head = probe("git", &["-C", &root_s, "rev-parse", "HEAD"], READ_BOUND);
    let head = head.stdout_if_ok()?.to_string();
    let status = probe("git", &["-C", &root_s, "status", "--porcelain=v1", "-z", "-uall"], READ_BOUND);
    let porcelain = status.stdout_if_ok()?.to_string();
    let stats: Vec<(String, u64, u128)> = porcelain_paths(&porcelain)
        .into_iter()
        .map(|p| {
            let (len, mtime) = std::fs::metadata(root.join(&p))
                .map(|m| {
                    let mtime = m
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_nanos()); // map_or: no mtime folds as 0, the size and the listing still count
                    (m.len(), mtime)
                })
                .unwrap_or((0, 0)); // unwrap_or: a deleted path has no stats; the listing records the deletion
            (p, len, mtime)
        })
        .collect();
    Some(fingerprint(&head, &porcelain, &stats))
}

/// A checkout's fingerprint, off the async thread. The ROOT is read once by the caller
/// and used for both sides: a batch that moves her (work/claim staging a new checkout)
/// must not read as a write because two different checkouts differ.
pub(crate) async fn fingerprint_at(root: Option<std::path::PathBuf>) -> Option<u64> {
    let root = root?;
    tokio::task::spawn_blocking(move || checkout_fingerprint(&root)).await.ok().flatten()
}

/// What the batch did to her checkout, as a typed fact (never a guess either way).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiskChange {
    Changed,
    Unchanged,
    /// No checkout, or one side could not be read: not counted as a write, and said so.
    Unknown,
}

impl DiskChange {
    pub(crate) fn between(before: Option<u64>, after: Option<u64>) -> Self {
        match (before, after) {
            (Some(a), Some(b)) if a != b => DiskChange::Changed,
            (Some(_), Some(_)) => DiskChange::Unchanged,
            _ => DiskChange::Unknown,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            DiskChange::Changed => "changed",
            DiskChange::Unchanged => "unchanged",
            DiskChange::Unknown => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(root: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .expect("test: git runs")
            .status
            .success();
        assert!(ok, "test: git {args:?}");
    }

    // what this catches (Joel, 2026-10-06: "0 writes always a serious plumbing bug"): a
    // write that reached disk through code/shell read as no write, because `wrote` read the
    // verb's NAME. The checkout's fingerprint sees it, by any verb: an append (sed -i /
    // heredoc shape), a second edit to a file that is ALREADY dirty (the porcelain alone
    // would not change), and a commit; a batch that touches nothing reads unchanged.
    #[test]
    fn a_shell_write_changes_the_checkouts_fingerprint_and_a_read_does_not() {
        let dir = tempfile::tempdir().expect("test: dir");
        let root = dir.path();
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "t@t"]);
        git(root, &["config", "user.name", "t"]);
        std::fs::write(root.join("a.txt"), "one\n").expect("test: write");
        git(root, &["add", "a.txt"]);
        git(root, &["commit", "-q", "-m", "base"]);

        let clean = checkout_fingerprint(root).expect("readable");
        assert_eq!(checkout_fingerprint(root), Some(clean), "a read changes nothing");

        std::fs::write(root.join("a.txt"), "one\ntwo\n").expect("test: shell-style write");
        let dirty = checkout_fingerprint(root).expect("readable");
        assert_eq!(DiskChange::between(Some(clean), Some(dirty)), DiskChange::Changed);

        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(root.join("a.txt"), "one\ntwo\nthree\n").expect("test: second edit");
        let dirtier = checkout_fingerprint(root).expect("readable");
        assert_eq!(
            DiskChange::between(Some(dirty), Some(dirtier)),
            DiskChange::Changed,
            "an edit to an already-dirty file is still a write"
        );

        git(root, &["commit", "-qam", "work"]);
        let committed = checkout_fingerprint(root).expect("readable");
        assert_eq!(DiskChange::between(Some(dirtier), Some(committed)), DiskChange::Changed);

        std::fs::write(root.join("new.txt"), "x").expect("test: untracked file");
        assert_eq!(
            DiskChange::between(Some(committed), checkout_fingerprint(root)),
            DiskChange::Changed,
            "a new file is a write"
        );
    }

    // what this catches: an unreadable side (no checkout, git refused) counted either way;
    // it is Unknown, never a write and never a confirmed non-write
    #[test]
    fn an_unreadable_side_is_unknown_never_a_write() {
        assert_eq!(DiskChange::between(None, Some(1)), DiskChange::Unknown);
        assert_eq!(DiskChange::between(Some(1), None), DiskChange::Unknown);
        assert_eq!(checkout_fingerprint(tempfile::tempdir().expect("test: dir").path()), None, "not a checkout");
    }
}
