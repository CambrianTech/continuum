//! WHICH LOCAL CHECKOUT IS `owner/name`: a per-node map from a work card's
//! repo id to the checkout on disk, recorded whenever the CLI runs from inside
//! a repo (start, reboot, any verb) and read by the claim-edge staging so a
//! CITIZEN can pull a repo card — she has no cwd, and the airc CLI resolves the
//! clone from cwd (`work_commands_git.rs`). One small file under the state dir,
//! save-on-write like the round tracker; never a parallel source of git truth.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn registry_path() -> Option<PathBuf> {
    crate::commands::benchmark::continuum_home()
        .ok()
        .map(|h| h.join("state").join("repos.json"))
}

fn load() -> BTreeMap<String, PathBuf> {
    registry_path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default() // unwrap_or: no file yet = nothing recorded, honest empty
}

/// Record that `repo` (e.g. `CambrianTech/continuum`) is checked out at `path`.
pub fn record(repo: &str, path: &Path) {
    let mut map = load();
    if map.get(repo).is_some_and(|p| p == path) {
        return;
    }
    map.insert(repo.to_string(), path.to_path_buf());
    if let Some(file) = registry_path() {
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(&map) {
            let _ = std::fs::write(file, bytes);
        }
    }
}

/// The local checkout for `repo`, if this node ever ran the CLI inside one.
/// "Never recorded" and "recorded, then the checkout vanished" are different
/// facts: the second is said in a probe (the breadcrumb a citizen otherwise
/// never gets — IntelMac's review of #3706), never a silent `None`.
pub fn path_for(repo: &str) -> Option<PathBuf> {
    let recorded = load().get(repo).cloned()?;
    if recorded.join(".git").is_dir() {
        return Some(recorded);
    }
    crate::probe!(
        class = "repo_registry.stale",
        repo = %repo,
        path = %recorded.display(),
        "recorded checkout is gone (or is a linked worktree, not a clone) — re-run the CLI from the clone to re-record"
    );
    None
}

/// The clone root from `git rev-parse --path-format=absolute --git-common-dir`:
/// `<clone>/.git` → `<clone>`, from the main clone AND from a linked worktree.
pub fn clone_root_from_common_dir(common_dir: &str) -> Option<PathBuf> {
    let p = Path::new(common_dir.trim());
    (p.file_name()? == ".git").then(|| p.parent().map(Path::to_path_buf)).flatten()
}

/// `owner/name` from a git remote URL (https or ssh), the key work cards carry.
pub fn repo_id_from_remote(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/').trim_end_matches(".git");
    let tail = trimmed.rsplit(|c| c == '/' || c == ':').take(2).collect::<Vec<_>>();
    match tail.as_slice() {
        [name, owner] if !name.is_empty() && !owner.is_empty() => Some(format!("{owner}/{name}")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the repo id drifting from the card's `owner/name` key for
    // either remote shape — a mismatch here and no citizen ever stages a repo card.
    #[test]
    fn repo_id_reads_https_and_ssh_remotes_as_owner_slash_name() {
        assert_eq!(repo_id_from_remote("https://github.com/CambrianTech/continuum.git").as_deref(), Some("CambrianTech/continuum"));
        assert_eq!(repo_id_from_remote("git@github.com:CambrianTech/airc.git").as_deref(), Some("CambrianTech/airc"));
        assert_eq!(repo_id_from_remote("https://github.com/CambrianTech/continuum/").as_deref(), Some("CambrianTech/continuum"));
        assert!(repo_id_from_remote("nonsense").is_none());
    }

    // what this catches: recording a card WORKTREE as the clone (deleted on merge,
    // so every later citizen claim stages nothing) — the clone root comes from
    // the git common dir, which is `<clone>/.git` from inside a worktree too.
    #[test]
    fn the_clone_root_is_the_parent_of_the_git_common_dir() {
        assert_eq!(clone_root_from_common_dir("/home/u/src/continuum/.git").as_deref(), Some(Path::new("/home/u/src/continuum")));
        assert_eq!(clone_root_from_common_dir("/home/u/src/continuum/.git\n").as_deref(), Some(Path::new("/home/u/src/continuum")));
        assert!(clone_root_from_common_dir("/home/u/.airc/worktrees/44529769").is_none(), "a worktree path is not a common dir");
    }
}
