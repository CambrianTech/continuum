//! WHICH LOCAL CHECKOUT IS `owner/name`: a per-node map from a work card's
//! repo id to the checkout on disk, recorded whenever the CLI runs from inside
//! a repo (start, reboot, any verb) and read by the claim-edge staging so a
//! CITIZEN can pull a repo card — she has no cwd, and the airc CLI resolves the
//! clone from cwd (`work_commands_git.rs`). One small file under the state dir,
//! save-on-write like the round tracker; never a parallel source of git truth.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn registry_path() -> Option<PathBuf> {
    crate::paths::continuum_home()
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

/// The checkout of the repository this core is built from, as this node recorded it: the
/// shared base a citizen layer clones and refreshes from, and the operator's own root. Its
/// repository is Cargo's declared `repository`, never a hardcoded name; its path is the one
/// the CLI recorded from inside the clone. NEVER the process cwd: a launchd-installed core
/// runs in its data dir (`WorkingDirectory` = ~/.continuum), which is not a checkout, and
/// reading it there left every citizen layer unrefreshed, 735 commits behind canary on
/// IntelMac (card 2a2cd6f1).
///
/// The recorded path is verified, not trusted: its `origin` must name the declared repository
/// (read through the owned git runner). A path replaced by another clone, or a repointed origin,
/// is refused by name, because the operator writes there and a citizen layer's refresh merges
/// from it with unrelated histories allowed (Codex reviewer on #5041).
pub async fn own_checkout() -> Result<PathBuf, String> {
    let repo = repo_id_from_remote(env!("CARGO_PKG_REPOSITORY"))
        .ok_or("continuum-core's Cargo.toml declares no repository")?;
    let path = path_for(&repo).ok_or_else(|| {
        format!(
            "this node has no recorded checkout of {repo}: run any `continuum` verb from inside \
             the clone once to record it"
        )
    })?;
    let origin = crate::code::git_bridge::run_git(&path, &["remote", "get-url", "origin"])
        .await
        .map_err(|e| format!("the recorded checkout {} of {repo} has no readable origin: {e}", path.display()))?;
    match repo_id_from_remote(origin.trim()) {
        Some(found) if found.eq_ignore_ascii_case(&repo) => Ok(path),
        found => Err(format!(
            "the recorded checkout {} is {}, not {repo}: run any `continuum` verb from inside {repo}'s \
             clone to re-record it",
            path.display(),
            found.as_deref().unwrap_or("an unidentifiable repository")
        )),
    }
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

/// Record the clone this test binary was built from as this node's own checkout, in the
/// CURRENT home's registry (callers hold `test_env::HomeGuard`): the fixture for code that
/// resolves `own_checkout`. The clone root comes from git's common dir, so a linked worktree
/// records its clone, as the CLI does.
#[cfg(test)]
pub(crate) fn record_own_checkout_for_test() -> PathBuf {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("git runs in the test environment");
    let root = clone_root_from_common_dir(&String::from_utf8_lossy(&out.stdout)).expect("the test tree is inside a clone");
    let repo = repo_id_from_remote(env!("CARGO_PKG_REPOSITORY")).expect("continuum-core declares its repository");
    record(&repo, &root);
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card 2a2cd6f1): "our checkout" read from the process cwd, which a
    // launchd-installed core sets to its data dir. Every citizen layer on IntelMac stopped
    // refreshing, 735 commits behind canary. `own_checkout` reads only the recorded clone of
    // the repo Cargo declares: with none recorded it fails by name even though this test's
    // cwd IS inside a checkout, and once recorded it returns that clone.
    #[tokio::test]
    async fn own_checkout_is_the_recorded_clone_never_the_cwd() {
        let home = tempfile::tempdir().expect("home");
        let _home = crate::test_env::HomeGuard::set(home.path()).await;
        let err = own_checkout().await.expect_err("nothing recorded under this home");
        assert!(err.contains("CambrianTech/continuum") && err.contains("no recorded checkout"), "{err}");

        // what this catches (Codex reviewer on #5041): a valid clone of ANOTHER repository at
        // the recorded path (replaced, or its origin repointed) was trusted. The operator would
        // write there and a layer refresh would merge it with unrelated histories allowed.
        let impostor = tempfile::tempdir().expect("impostor");
        let git = |args: &[&str]| {
            let ok = std::process::Command::new("git").args(args).current_dir(impostor.path()).status().expect("git").success();
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q"]);
        record("CambrianTech/continuum", impostor.path());
        let err = own_checkout().await.expect_err("a clone with no origin is not our checkout");
        assert!(err.contains("no readable origin"), "{err}");
        git(&["remote", "add", "origin", "https://github.com/SomeOrg/other-repo.git"]);
        let err = own_checkout().await.expect_err("another repository is not our checkout");
        assert!(err.contains("SomeOrg/other-repo") && err.contains("re-record"), "{err}");

        let recorded = record_own_checkout_for_test();
        assert_eq!(own_checkout().await.expect("recorded"), recorded);
    }

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
