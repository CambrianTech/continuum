//! Where this CLI stands: the home dir, its own image, the tracked checkout the core
//! deploys from, and every git read made on it — HEAD, the build key, the index lock,
//! a tip's age, and whether a running git may be working in the tree.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use super::deploy_log::deploy_note;

/// The user's home dir — `HOME` (unix) or `USERPROFILE` (Windows). Loud when absent: the
/// resolution order depends on it, and guessing would defeat the shared contract.
pub(super) fn home_dir() -> Result<String, String> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| {
            "HOME/USERPROFILE unset — cannot resolve installed continuum-core-server locations \
             (set CONTINUUM_CORE_BIN explicitly)"
                .to_string()
        })
}

/// The image this CLI runs from, for `CONTINUUM_SKIP_SELF_BUILD`: the build script skips
/// the CLI only when this IS the file it would write (a locked running image). "1" when the
/// OS cannot say — the script's unconditional skip, never a build over a running image.
pub(super) fn running_cli_image() -> std::ffi::OsString {
    std::env::current_exe().map_or_else(|_| "1".into(), PathBuf::into_os_string)
}

/// Short git HEAD SHA of the current checkout (matches what `build.rs` embeds), or `None`
/// when not in a git tree.
/// Record the cwd's repo checkout for the core (`modules::repo_registry`): the
/// claim-edge staging of a REPO card needs `owner/name → path`, and the core has
/// no cwd. Idempotent, silent outside a repo.
pub(super) fn record_repo_checkout() {
    let out = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    };
    // `--show-toplevel` is the WORKTREE when run inside one (a card worktree,
    // deleted on merge — IntelMac's review of #3706); `--git-common-dir` is the
    // clone's .git in both cases, and absolute so the main clone does not
    // answer a relative `.git`.
    let (Some(url), Some(common)) = (
        out(&["remote", "get-url", "origin"]),
        out(&["rev-parse", "--path-format=absolute", "--git-common-dir"]),
    ) else {
        return;
    };
    let Some(root) = continuum_core::modules::repo_registry::clone_root_from_common_dir(&common)
    else {
        return;
    };
    if let Some(repo) = continuum_core::modules::repo_registry::repo_id_from_remote(&url) {
        continuum_core::modules::repo_registry::record(&repo, &root);
    }
}

/// The checkout the tracker decides over — THE resolver the tracker itself uses
/// (`runtime::tracked_checkout`, card 790c6bcb): the recorded `CONTINUUM_TRACK_REPO_DIR`
/// else the build-time source root, resolved to the DURABLE main checkout of its repo, so
/// a binary built in a lease worktree deploys from the clone the lease came from.
pub(super) fn tracked_repo_dir() -> Result<PathBuf, String> {
    use continuum_core::runtime::tracked_checkout::{tracked_checkout, TRACK_REPO_DIR_KEY};
    match tracked_checkout() {
        Ok(Some(dir)) => Ok(dir),
        Ok(None) => Err(format!("deploy-consume: no checkout to deploy from (set {TRACK_REPO_DIR_KEY})")),
        Err(e) => Err(format!("deploy-consume: {e}")),
    }
}

/// `tip`'s build key in `repo` (`prebuilt_artifact::build_key_log_args`), or `None` if git
/// cannot say; `None` falls back to the tip itself.
pub(super) fn build_key_in(repo: &Path, tip: &str) -> Option<String> {
    let args = continuum_cli_lifecycle::prebuilt_artifact::build_key_log_args(tip);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    git_in(repo, &args).ok().map(|key| key.trim().to_string()).filter(|key| !key.is_empty())
}

pub(super) fn git_in(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Whether any running `git` might be working on `repo`, judged conservatively: a git whose
/// cwd is inside the tree, whose command line names the tree (`--git-dir` / `--work-tree`
/// given from elsewhere), or whose cwd cannot be read at all. Only a git positively seen
/// working elsewhere is ruled out: an unreadable cwd (permissions, a platform without cwd
/// inspection) is not evidence of absence (Codex on #4477).
fn git_running_in(repo: &Path) -> bool {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_cwd(UpdateKind::Always)
            .with_cmd(UpdateKind::Always),
    );
    let canonical = repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf()); // unwrap_or_else: an uncanonicalizable root still compares as given
    let spellings = [repo.to_string_lossy().into_owned(), canonical.to_string_lossy().into_owned()];
    sys.processes().values().any(|p| {
        let name = p.name().to_string_lossy();
        if name != "git" && name != "git.exe" {
            return false;
        }
        let names_repo = p
            .cmd()
            .iter()
            .any(|a| spellings.iter().any(|r| a.to_string_lossy().contains(r.as_str())));
        match p.cwd() {
            Some(cwd) => names_repo || cwd.starts_with(&canonical) || cwd.starts_with(repo),
            None => true, // unreadable: it may be working here
        }
    })
}

/// The deploy tree's `index.lock`, judged and (when stale) removed. `Ok(true)` = the tree
/// may be deployed; `Ok(false)` = a lock that may be live stands, named in the deploy log,
/// retried next tick without spending one of the tip's attempts.
pub(super) fn settle_index_lock(repo: &Path) -> Result<bool, String> {
    use continuum_cli_lifecycle::deploy_tracker::{index_lock_verdict, IndexLock, STALE_INDEX_LOCK};
    let rel = git_in(repo, &["rev-parse", "--git-path", "index.lock"])?;
    let lock = repo.join(rel);
    let age = std::fs::metadata(&lock)
        .ok()
        .and_then(|m| m.modified().ok())
        .map(|t| t.elapsed().unwrap_or_default()); // unwrap_or_default: an mtime in the future reads as brand new, never as stale
    match index_lock_verdict(age, age.is_some() && git_running_in(repo)) {
        IndexLock::Absent => Ok(true),
        IndexLock::Held => {
            deploy_note(&format!(
                "deploy-consume: {} is locked ({} s old, a git operation may hold it) — not deploying this tick; \
                 it is removed once it is {} s old with no git running in the tree",
                lock.display(),
                age.unwrap_or_default().as_secs(), // unwrap_or_default: Held always carries an age
                STALE_INDEX_LOCK.as_secs()
            ));
            Ok(false)
        }
        IndexLock::Stale => {
            std::fs::remove_file(&lock)
                .map_err(|e| format!("deploy-consume: cannot remove stale {}: {e}", lock.display()))?;
            deploy_note(&format!(
                "deploy.tree.stale_lock_cleared: removed {} ({} s old, no git running in the tree) — a git process died mid-write",
                lock.display(),
                age.unwrap_or_default().as_secs() // unwrap_or_default: Stale always carries an age
            ));
            Ok(true)
        }
    }
}

/// Seconds since `tip` was committed (a merge's commit time is the time it landed).
pub(super) fn tip_age_secs(repo: &Path, tip: &str) -> u64 {
    let committed = git_in(repo, &["log", "-1", "--format=%ct", tip])
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok());
    // A clock before 1970 reads as the epoch, so every tip is "new" and the consumer waits.
    let now = continuum_core::time::EpochMs::now().secs();
    committed.map(|c| now.saturating_sub(c)).unwrap_or(0) // unwrap_or: an unreadable commit time waits, it does not compile early
}

pub(super) fn git_head_short_sha() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Unlike the legacy best-effort HEAD lookup, a prebuilt deploy may self-anchor
/// only outside a repository. A broken/inaccessible checkout is not an absence.
pub(super) async fn prebuilt_checkout_sha(
    cwd: &Path,
    git_dir: Option<&std::ffi::OsStr>,
) -> Result<Option<String>, String> {
    let cwd = cwd
        .canonicalize()
        .map_err(|e| format!("cannot inspect prebuilt checkout {}: {e}", cwd.display()))?;
    if git_dir.is_none() && !has_git_checkout(&cwd)? {
        return Ok(None);
    }

    let mut cmd = std::process::Command::new("git");
    cmd.current_dir(&cwd)
        .args(["rev-parse", "--short", "HEAD"])
        .env_remove("GIT_DIR")
        .stdin(Stdio::null());
    if let Some(git_dir) = git_dir {
        cmd.env("GIT_DIR", git_dir);
    }
    // owned (lane_process::run_owned): a hung git is ended with anything it started
    let output = continuum_core::inference::lane_process::run_owned(
        tokio::process::Command::from(cmd),
        Some(Duration::from_secs(30)),
        "prebuilt-head-lookup",
        std::sync::Arc::default(),
    )
    .await
    .map_err(|e| format!("cannot read prebuilt checkout HEAD in {}: {e}", cwd.display()))?;
    let status = match output.ended {
        continuum_core::inference::lane_process::Ended::Exited(status) => status,
        continuum_core::inference::lane_process::Ended::TimedOut { .. } => {
            return Err(format!("prebuilt checkout HEAD lookup timed out in {}", cwd.display()))
        }
    };
    if let Some(note) = output.capture_note() {
        return Err(format!("cannot verify prebuilt checkout HEAD in {}: {note}", cwd.display()));
    }
    if !status.success() {
        return Err(format!(
            "cannot verify prebuilt checkout HEAD in {}: git exited {}: {}",
            cwd.display(),
            status,
            String::from_utf8_lossy(&output.stderr.bytes).trim(),
        ));
    }
    let head = String::from_utf8(output.stdout.bytes)
        .map_err(|e| format!("prebuilt checkout HEAD is not UTF-8: {e}"))?;
    let head = head.trim();
    if head.is_empty() {
        return Err("prebuilt checkout HEAD lookup returned no SHA".into());
    }
    Ok(Some(head.to_owned()))
}

/// The checkout's BUILD KEY: the newest commit at or before `head` that touched a build
/// input (`prebuilt_artifact::BUILD_INPUTS`), which is the commit CI built a core for. Falls
/// back to `head` itself, named as such, when git cannot say.
pub(super) fn prebuilt_checkout_build_key(cwd: &Path, git_dir: Option<&std::ffi::OsStr>, head: &str) -> (String, &'static str) {
    let args = continuum_cli_lifecycle::prebuilt_artifact::build_key_log_args(head);
    let mut cmd = std::process::Command::new("git");
    cmd.current_dir(cwd).args(&args).env_remove("GIT_DIR").stdin(Stdio::null());
    if let Some(git_dir) = git_dir {
        cmd.env("GIT_DIR", git_dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    match cmd.output() {
        Ok(out) if out.status.success() => {
            let key = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if key.is_empty() {
                (head.to_string(), "git HEAD of this checkout (no build key found)")
            } else {
                (key, "build key of this checkout (newest commit touching a build input)")
            }
        }
        _ => (head.to_string(), "git HEAD of this checkout (build key unreadable)"),
    }
}

/// Detect repository metadata without asking Git to read HEAD. Both a normal
/// `.git` directory and a linked worktree/submodule's `.git` file count, even if
/// broken; Git must then verify the selected checkout. Bare repositories count
/// too. Metadata permission errors must not downgrade verification to standalone.
fn has_git_checkout(cwd: &Path) -> Result<bool, String> {
    fn present(path: &Path) -> Result<bool, String> {
        match std::fs::symlink_metadata(path) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(format!(
                "cannot inspect Git metadata {}: {e}",
                path.display()
            )),
        }
    }
    for ancestor in cwd.ancestors() {
        if present(&ancestor.join(".git"))?
            || (present(&ancestor.join("HEAD"))? && present(&ancestor.join("objects"))?)
        {
            return Ok(true);
        }
    }
    Ok(false)
}
