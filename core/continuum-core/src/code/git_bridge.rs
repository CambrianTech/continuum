//! Git Bridge — git status, diff, and branch operations.
//!
//! Shells out to `git` CLI for operations. This keeps the implementation
//! simple and avoids pulling in libgit2 as a dependency.
//!
//! Every git child runs OWNED (card bfa87dd5): through `lane_process::run_owned_with_input`, so a
//! cancelled command or an expired deadline ends the git and everything it started, its output
//! is read whole or named as not whole, and it holds the hands' admission until it has ended.
//! A caller with a deadline runs its git calls [`within`] it: ONE absolute deadline across all
//! of them, never reset per call (Codex root on bfa87dd5).

use std::path::Path;

use super::types::GitStatusInfo;

/// Get git status for a workspace directory.
///
/// Returns branch name, modified/added/deleted/untracked files.
pub async fn git_status(workspace_root: &Path) -> GitStatusInfo {
    // Get current branch
    let branch = run_git(workspace_root, &["rev-parse", "--abbrev-ref", "HEAD"]).await
        .ok()
        .map(|s| s.trim().to_string());

    // Get porcelain status (machine-readable)
    let status_output = match run_git(workspace_root, &["status", "--porcelain=v1"]).await {
        Ok(output) => output,
        Err(e) => {
            return GitStatusInfo {
                success: false,
                branch,
                modified: Vec::new(),
                added: Vec::new(),
                deleted: Vec::new(),
                untracked: Vec::new(),
                error: Some(format!("git status failed: {}", e)),
            };
        }
    };

    let mut modified = Vec::new();
    let mut added = Vec::new();
    let mut deleted = Vec::new();
    let mut untracked = Vec::new();

    for line in status_output.lines() {
        if line.len() < 3 {
            continue;
        }

        let status_code = &line[..2];
        let file_path = line[3..].trim().to_string();

        // Parse porcelain v1 status codes
        // First char = index status, second char = work tree status
        match status_code {
            "??" => untracked.push(file_path),
            s if s.starts_with('A') || s.ends_with('A') => added.push(file_path),
            s if s.starts_with('D') || s.ends_with('D') => deleted.push(file_path),
            s if s.starts_with('M')
                || s.ends_with('M')
                || s.starts_with('R')
                || s.ends_with('R')
                || s.starts_with('C')
                || s.ends_with('C') =>
            {
                modified.push(file_path)
            }
            _ => {
                // Catch-all: treat as modified
                if !file_path.is_empty() {
                    modified.push(file_path);
                }
            }
        }
    }

    GitStatusInfo {
        success: true,
        branch,
        modified,
        added,
        deleted,
        untracked,
        error: None,
    }
}

/// Get git diff output for the workspace.
///
/// If `staged` is true, shows staged changes (--cached).
/// Otherwise shows unstaged working tree changes.
pub async fn git_diff(workspace_root: &Path, staged: bool) -> Result<String, String> {
    let mut args = vec!["diff"];
    if staged {
        args.push("--cached");
    }
    run_git(workspace_root, &args).await
}

/// Get git diff against a specific ref (branch, commit, etc.).
pub async fn git_diff_ref(workspace_root: &Path, reference: &str) -> Result<String, String> {
    run_git(workspace_root, &["diff", reference]).await
}

/// The `owner/name` board key this checkout belongs to, read from `origin`.
///
/// The work board keys cards by repo, and that key is a FACT about the checkout —
/// not a string a caller should have to retype. `benchmark/dispatch` used to infer it
/// from whatever cards already sat on the board, which worked only while every run
/// shared one permanent board; once a run gets its own fresh room there are no cards
/// to infer from, and the inference had to come from the real thing instead.
///
/// Returns `None` rather than erroring: not every workspace has an `origin` (a fresh
/// `git init`, a local-only clone), and a caller that can still pass `repo` explicitly
/// should get that chance before anything fails.
pub async fn origin_repo_slug(workspace_root: &Path) -> Option<String> {
    let url = run_git(workspace_root, &["remote", "get-url", "origin"]).await.ok()?;
    repo_slug_from_remote_url(url.trim())
}

/// Parse `owner/name` out of a git remote URL — the part that actually varies and
/// therefore the part worth testing.
///
/// Handles the three forms a clone can carry:
/// - `https://github.com/owner/name.git`
/// - `git@github.com:owner/name.git` (scp-style, no scheme, `:` separator)
/// - `ssh://git@github.com/owner/name`
///
/// Host-agnostic on purpose: the board key is `owner/name`, and a GitLab or
/// self-hosted remote produces exactly as valid a key as a GitHub one. Takes the LAST
/// two path segments so deeper hosting paths (GitLab subgroups) still yield the
/// project's own `group/name`.
pub fn repo_slug_from_remote_url(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    // Strip scheme, then any `user@host` prefix, leaving a host-relative path. The
    // scp-style form uses `:` where the others use `/`, so normalize that first.
    let after_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    let after_host = match after_scheme.split_once(':') {
        // scp-style `git@host:owner/name` — everything after the colon is the path.
        Some((_, path)) if !path.starts_with("//") => path,
        _ => after_scheme.split_once('/').map_or("", |(_, path)| path),
    };
    let path = after_host.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);

    let mut segments = path.rsplit('/').filter(|s| !s.is_empty());
    let name = segments.next()?;
    let owner = segments.next()?;
    if name.is_empty() || owner.is_empty() {
        return None;
    }
    Some(format!("{owner}/{name}"))
}

/// Get git log (last N commits, one-line format).
pub async fn git_log(workspace_root: &Path, count: u32) -> Result<String, String> {
    run_git(
        workspace_root,
        &["log", &format!("-{}", count), "--oneline", "--no-decorate"],
    ).await
}

/// Stage files for commit.
///
/// Pass specific file paths, or `&["--all"]` / `&["."]` to stage everything.
pub async fn git_add(workspace_root: &Path, paths: &[&str]) -> Result<String, String> {
    let mut args = vec!["add"];
    args.extend_from_slice(paths);
    run_git(workspace_root, &args).await
}

/// Create a commit with the given message.
///
/// Returns the full commit hash on success.
pub async fn git_commit(workspace_root: &Path, message: &str) -> Result<String, String> {
    // Commit through the repository's normal hook path. AI-authored commits
    // must fail loudly when validation fails; callers surface the git stderr.
    run_git(workspace_root, &["commit", "-m", message]).await?;

    // Return the commit hash
    run_git(workspace_root, &["rev-parse", "HEAD"]).await.map(|s| s.trim().to_string())
}

/// Apply a unified diff to the working tree (`git apply`). `check_only` runs
/// `git apply --check` — validate without touching files. The patch arrives on
/// stdin so peer-shared diffs of any size apply without temp files. This is the
/// RECEIVING end of diffs-over-the-room: one citizen `code/git/diff`s, posts the
/// patch, another applies it — the consolidation verb the Conway team asked for
/// (2026-07-11) before the full branch/merge rails exist.
pub async fn git_apply(workspace_root: &Path, patch: &str, check_only: bool) -> Result<String, String> {
    let mut args = vec!["apply", "--whitespace=nowarn"];
    if check_only {
        args.push("--check");
    }
    // the patch goes in on stdin; a patch git didn't take whole is not an applied patch
    run_git_input(workspace_root, &args, &[], Some(patch.as_bytes().to_vec())).await?;
    Ok(if check_only {
        "patch applies cleanly (checked, not applied)".to_string()
    } else {
        "patch applied".to_string()
    })
}

/// Ensure `workspace_root` is a git repository: no-op when `.git` exists;
/// otherwise `git init` + an initial commit of whatever is present, so every
/// citizen workspace is diff-able/apply-able from birth
/// ([[workspace-is-a-cow-diff-from-shared-always-git]]). Loud Err if git
/// itself is unavailable — a workspace that silently can't version work is
/// the kind of quiet defect the jobs-ledger taught us to refuse.
pub async fn git_init_if_needed(workspace_root: &Path) -> Result<bool, String> {
    if workspace_root.join(".git").exists() {
        return Ok(false);
    }
    run_git(workspace_root, &["init"]).await.map_err(|e| format!("git init failed: {e}"))?;
    // Identity for the initial commit: repo-local, never touching global config.
    let _ = run_git(
        workspace_root,
        &["config", "user.email", "citizen@continuum.local"],
    ).await;
    let _ = run_git(
        workspace_root,
        &["config", "user.name", "continuum-citizen"],
    ).await;
    let _ = run_git(workspace_root, &["add", "-A"]).await;
    // An empty dir still gets a root commit so diffs have a base.
    let _ = run_git(
        workspace_root,
        &["commit", "--allow-empty", "-m", "workspace: initial state"],
    ).await;
    Ok(true)
}

/// Push the current branch to a remote. Defaults to `origin` if remote is empty.
pub async fn git_push(workspace_root: &Path, remote: &str, branch: &str) -> Result<String, String> {
    let remote = if remote.is_empty() { "origin" } else { remote };
    let mut args = vec!["push", remote];
    if !branch.is_empty() {
        args.push(branch);
    }
    run_git(workspace_root, &args).await
}

/// Outcome of a [`git_sync_from_shared`] refresh — drives the "notify" the caller
/// surfaces to the persona (Joel: "preserve and notify… eventually just learn this").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSyncReport {
    /// True when new shared commits were actually merged in (workspace changed).
    pub synced: bool,
    /// Short human/persona-readable summary of what happened.
    pub summary: String,
}

/// Bring a citizen workspace CURRENT with the shared checkout WITHOUT destroying
/// the persona's own work — the self-heal for
/// [[citizen-workspaces-are-stale-one-time-clones]]. Citizen workspaces are
/// `cp -cR` clones of the shared checkout (including its `.git`), so they share
/// history with shared; but `ensure_citizen_layer` never refreshed them, so they
/// drifted stale (one froze before the `workers/→core/` restructure). Because the
/// histories are related, a fetch + merge brings the framework current while
/// KEEPING the persona's commits.
///
/// Preserve-first (Joel: "preserve and notify"):
///  1. autocommit any uncommitted persona work, so a merge can't lose it and it
///     survives on the persona's own history.
///  2. `git fetch <shared> HEAD` — shared's current HEAD into FETCH_HEAD.
///  3. `git merge -X theirs --no-edit --allow-unrelated-histories FETCH_HEAD` —
///     shared wins framework-file conflicts (the persona shouldn't diverge the
///     framework); the persona's OWN new files are untouched; their commits stay
///     in history. `-X theirs` is deterministic — never drops to an interactive
///     conflict. On a merge that truly can't complete we `--abort` so the
///     workspace is never left half-merged, and surface the cause LOUD.
///
/// Returns whether anything changed (+ a summary) so the caller can drop the
/// teaching note only when there was actually a refresh.
pub async fn git_sync_from_shared(
    workspace_root: &Path,
    shared_checkout: &Path,
) -> Result<GitSyncReport, String> {
    // 0. A card branch is the citizen's own work in flight for review, and it gets pushed.
    // The autosave below would commit her whole workspace onto it, and the shared-wins merge
    // could overwrite her edits (Kimi's card 63855189: five autosaves swept 3,026 files into
    // the branch and both its PRs were closed). A card branch reconciles with canary through
    // card staging, never here, so the sync leaves it untouched and says why.
    let branch = run_git(workspace_root, &["symbolic-ref", "--short", "-q", "HEAD"]).await
        .unwrap_or_default()
        .trim()
        .to_string();
    if crate::modules::card_staging::is_card_branch(&branch) {
        crate::probe!(
            class = "workspace.sync.skipped_card_branch",
            root = %workspace_root.display(),
            branch = %branch,
            "workspace is on a card branch; shared sync left it untouched"
        );
        return Ok(GitSyncReport {
            synced: false,
            summary: format!(
                "on card branch {branch}: not synced from shared (a card branch reconciles through card staging)"
            ),
        });
    }

    // 1. Preserve: commit in-flight persona work before merging.
    let porcelain = run_git(workspace_root, &["status", "--porcelain"]).await.unwrap_or_default();
    if !porcelain.trim().is_empty() {
        let _ = run_git(workspace_root, &["add", "-A"]).await;
        let _ = run_git(
            workspace_root,
            &[
                "commit",
                "-m",
                "workspace: autosave persona work before shared sync",
            ],
        ).await;
    }
    let before = run_git(workspace_root, &["rev-parse", "HEAD"]).await
        .unwrap_or_default()
        .trim()
        .to_string();

    // 2. Fetch shared's current HEAD (a filesystem path is a valid git remote).
    let shared = shared_checkout.to_string_lossy();
    run_git(
        workspace_root,
        &["fetch", "--no-tags", shared.as_ref(), "HEAD"],
    ).await
    .map_err(|e| format!("fetch from shared checkout '{shared}' failed: {e}"))?;

    // 3. Merge shared in — shared wins framework conflicts, persona files kept.
    if let Err(e) = run_git(
        workspace_root,
        &[
            "merge",
            "--no-edit",
            "--allow-unrelated-histories",
            "-X",
            "theirs",
            "FETCH_HEAD",
        ],
    ).await {
        // Two conflict shapes live outside `-X theirs` and stranded every citizen
        // workspace stale (glass-boxed 2026-08-03, rounds 1+2): diverged SUBMODULE
        // pointers (git refuses non-trivial gitlink merges outright) and
        // modify/delete (shared deleted debris the persona's autosave had touched
        // — the #2135 sweep made this the steady state). Resolve every staged
        // conflict SHARED-WINS — the sync's one documented rule — and conclude the
        // merge; the persona's versions live on in her autosave history
        // (preserve-first is the commit, not the tree). A merge that failed
        // without staging anything still aborts loud, workspace intact.
        match resolve_conflicts_shared_wins(workspace_root).await {
            Ok(true) => {
                crate::probe!(
                    class = "workspace.sync.conflicts_resolved",
                    root = %workspace_root.display(),
                    "merge blocked on gitlink/modify-delete conflicts — resolved shared-wins and concluded"
                );
            }
            _ => {
                // Never leave a half-merge — abort so the workspace stays usable, fail loud.
                let _ = run_git(workspace_root, &["merge", "--abort"]).await;
                return Err(format!(
                    "shared merge failed (aborted; workspace left intact): {e}"
                ));
            }
        }
    }

    let after = run_git(workspace_root, &["rev-parse", "HEAD"]).await
        .unwrap_or_default()
        .trim()
        .to_string();
    let synced = !before.is_empty() && before != after;
    let summary = if synced {
        let count = run_git(
            workspace_root,
            &["rev-list", "--count", &format!("{before}..{after}")],
        ).await
        .unwrap_or_default()
        .trim()
        .to_string();
        format!("synced {count} shared commit(s) in; your work was preserved")
    } else {
        "already current with the shared checkout".to_string()
    };
    Ok(GitSyncReport { synced, summary })
}

/// After a failed `merge -X theirs`, salvage the shapes `-X theirs` cannot
/// touch, resolving each SHARED-WINS — the sync's one documented rule, extended
/// uniformly to what git refuses to auto-resolve:
///
///  - SUBMODULE pointers (mode 160000): both sides moved the gitlink (the
///    persona's autosave commits her stale vendored pointer while shared
///    advances it) — git aborts with "only trivial cases". Resolve to
///    FETCH_HEAD's recorded sha.
///  - modify/delete: shared DELETED a path the persona modified (glass-boxed
///    2026-08-03 round 2: the #2135 debris sweep deleted conway/wordstats from
///    shared while every citizen's autosave had touched them — the sync then
///    failed forever on the very files it was supposed to clear). Shared's
///    deletion wins the WORKING TREE; the persona's version is NOT lost — her
///    autosave commit holds it in her own history (preserve-first is the
///    commit, not the tree).
///  - any other unmerged entry: theirs-stage exists → take theirs; theirs-stage
///    absent → the path is gone in shared → remove.
///
/// Returns Ok(true) on a concluded merge, Ok(false) when there was nothing
/// staged to salvage (caller aborts loud).
async fn resolve_conflicts_shared_wins(workspace_root: &Path) -> Result<bool, String> {
    let unmerged = run_git(workspace_root, &["ls-files", "-u"]).await?;
    if unmerged.trim().is_empty() {
        return Ok(false); // merge failed before staging conflicts — nothing to salvage
    }
    // path → has a stage-3 (theirs) entry? Stage layout: 1=base, 2=ours, 3=theirs.
    let mut theirs_present: std::collections::BTreeMap<String, bool> =
        std::collections::BTreeMap::new();
    for line in unmerged.lines() {
        // "<mode> <sha> <stage>\t<path>"
        let Some((meta, path)) = line.split_once('\t') else {
            return Ok(false);
        };
        let stage = meta.split_whitespace().nth(2).unwrap_or("");
        let entry = theirs_present.entry(path.to_string()).or_insert(false);
        if stage == "3" {
            *entry = true;
        }
    }
    for (p, has_theirs) in &theirs_present {
        if *has_theirs {
            // Content/gitlink present on shared's side — adopt it. `checkout
            // FETCH_HEAD --` covers both regular files and gitlinks (updates the
            // index entry; for gitlinks the pointer, for files the blob).
            run_git(workspace_root, &["checkout", "FETCH_HEAD", "--", p]).await
                .map_err(|e| format!("shared-wins resolve of '{p}' failed: {e}"))?;
        } else {
            // Deleted in shared (modify/delete) — the deletion wins the tree.
            run_git(workspace_root, &["rm", "-f", "--", p]).await
                .map_err(|e| format!("shared-wins delete of '{p}' failed: {e}"))?;
        }
    }
    run_git(workspace_root, &["commit", "--no-edit"]).await
        .map_err(|e| format!("concluding merge after shared-wins resolve failed: {e}"))?;
    Ok(true)
}

tokio::task_local! {
    /// The absolute deadline every git call on this task runs within, when its caller set one.
    static DEADLINE: tokio::time::Instant;
}

/// Run `work` with every git call in it bounded by ONE absolute `deadline`: carried across them
/// all, never reset per call, so a slow first call leaves the later ones less time, not more.
pub(crate) async fn within<F: std::future::Future>(deadline: tokio::time::Instant, work: F) -> F::Output {
    DEADLINE.scope(deadline, work).await
}

/// What a git error says when its deadline ended the git mid-run: its effect is UNKNOWN (a
/// push may have reached the remote). One phrase, defined here and read by [`interrupted`].
const INTERRUPTED: &str = "interrupted at its deadline: its effect is unknown";
/// What a git error says when its deadline had already passed: it never started.
const NOT_STARTED: &str = "not started: its deadline had passed";

/// Did the deadline end this git mid-run (its effect unknown)?
pub(crate) fn interrupted(err: &str) -> bool {
    err.contains(INTERRUPTED)
}

/// Did the deadline stop this git before it started, or end it mid-run?
pub(crate) fn out_of_time(err: &str) -> bool {
    err.contains(INTERRUPTED) || err.contains(NOT_STARTED)
}

/// The deadline in force on this task, if its caller set one.
pub(crate) fn deadline() -> Option<tokio::time::Instant> {
    DEADLINE.try_with(|at| *at).ok()
}

/// Run a git command in the workspace directory. `pub(crate)` so the one context-clean
/// runner (env stripped, ceiling set, no terminal prompt) is shared by every crate seam
/// that shells to git — `persona::workspace_transfer` carries a card's branch between
/// nodes through it — rather than each seam re-deriving the hygiene below.
pub(crate) async fn run_git(workspace_root: &Path, args: &[&str]) -> Result<String, String> {
    Ok(run_git_input(workspace_root, args, &[], None).await?)
}

/// [`run_git`] with extra environment for this one invocation (e.g. `GIT_AUTHOR_*` so a
/// citizen's act commit is authored by her). Same hygiene; the env is applied last.
pub(crate) async fn run_git_with_env(workspace_root: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<String, String> {
    Ok(run_git_input(workspace_root, args, env, None).await?)
}

/// How a git that gave no answer ended, for a caller whose next step depends on whether it had
/// an effect (a push: did the remote take it?).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GitEnd {
    /// It never ran: its deadline had passed before it started. It had no effect.
    NeverRan,
    /// It exited non-zero: git's own verdict that it didn't do what was asked.
    Refused,
    /// Its effect is unknown: its deadline ended it mid-run, it exited 0 but its input or
    /// output wasn't whole or readable, or the runner failed around it (Codex reviewer on #5037).
    Unknown,
}

/// A git that gave no answer: how it ended, and the one rendering of why.
#[derive(Debug)]
pub(crate) struct GitError {
    pub(crate) end: GitEnd,
    message: String,
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<GitError> for String {
    fn from(error: GitError) -> Self {
        error.message
    }
}

/// The ONE git runner: [`run_git`] with optional stdin `input` (a patch for `git apply`). Owned
/// through `lane_process::run_owned_with_input`, within the task's [`deadline`] when one is set.
/// An exit that isn't success is an `Err` with git's stderr; so is an output that wasn't read
/// whole or an input that wasn't delivered; a deadline that ended it says its effect is UNKNOWN
/// (a push may have landed), never that nothing happened.
pub(crate) async fn run_git_input(workspace_root: &Path, args: &[&str], env: &[(&str, &str)], input: Option<Vec<u8>>) -> Result<String, GitError> {
    use crate::inference::lane_process::{run_owned_with_input, Answer, Why};
    let bound = match deadline() {
        Some(at) => {
            let left = at.saturating_duration_since(tokio::time::Instant::now());
            if left.is_zero() {
                return Err(GitError { end: GitEnd::NeverRan, message: format!("git {} {NOT_STARTED}", args.join(" ")) });
            }
            Some(left)
        }
        None => None,
    };
    let mut cmd = tokio::process::Command::new("git");
    cmd.args(args)
        .current_dir(workspace_root)
        // Strip git-context env vars that would otherwise pin git to
        // the parent repo regardless of cwd. Without this, when
        // run_git is invoked from a process that itself was launched
        // by git (the most common case: pre-push / pre-commit hooks
        // invoking `cargo test`), git sets GIT_DIR/GIT_PREFIX/etc and
        // those propagate to every child. Concrete failure:
        // git_bridge::tests' tempdir `git commit` inherited GIT_DIR
        // pointing at the parent worktree's .git, then ran the
        // worktree's pre-commit hook (whose paths don't exist in the
        // tempdir context) and panicked. Caught 2026-05-02 wedging the
        // whole git_bridge::tests cluster every time the pre-push hook
        // ran them. Stripping these makes run_git context-clean — git
        // discovers from current_dir(workspace_root) only, no parent
        // contamination.
        // GIT_CEILING_DIRECTORIES caps any residual upward discovery
        // at workspace_root (defense in depth — env_remove handles the
        // documented vars; ceiling handles anything new git might add
        // in future versions).
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_PREFIX")
        .env("GIT_CEILING_DIRECTORIES", workspace_root)
        // A headless core has no terminal: a fetch or push that would ask for
        // credentials must FAIL (a named outcome) rather than hang the turn on a prompt
        // nobody will answer.
        .env("GIT_TERMINAL_PROMPT", "0")
        // The caller's env LAST, so a call that means to differ from the hygiene says so
        // by name (the citizen layer's `rev-parse --show-toplevel` lifts the ceiling: it
        // IS the upward discovery).
        .envs(env.iter().copied());
    let out = run_owned_with_input(cmd, input, bound, "git", std::sync::Arc::default())
        .await
        // the runner's errors aren't all before the spawn (a failed wait, an end that couldn't
        // run): what the git did is unknown, never "it never ran" (Codex reviewer on #5037)
        .map_err(|e| GitError { end: GitEnd::Unknown, message: format!("Failed to run git: {e}") })?;
    // the shared projection (lane_process::OwnedOutput::answer): a non-zero exit is git's own
    // failure, with its stderr, and any delivery note beside it; a deadline that ended it says
    // its effect is unknown
    if let Answer::Not(unanswered) = out.answer() {
        let (end, at_deadline) = match unanswered.why {
            Why::Exited(_) => (GitEnd::Refused, String::new()),
            Why::NotWhole => (GitEnd::Unknown, String::new()),
            Why::TimedOut { .. } => (GitEnd::Unknown, format!(" {INTERRUPTED}")),
        };
        return Err(GitError { end, message: format!("git {}{at_deadline}: {unanswered}", args.join(" ")) });
    }
    // it exited 0 with everything read whole, but what it said isn't text: what it did is unknown
    String::from_utf8(out.stdout.bytes).map_err(|e| GitError { end: GitEnd::Unknown, message: format!("git {}: invalid UTF-8 in its output: {e}", args.join(" ")) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // what this catches: a remote URL form that yields the WRONG board key, or none.
    // `benchmark/dispatch` keys every card it posts by this slug, so a bad parse
    // silently files a run's cards under a repo nobody is looking at — and the scp-style
    // `git@host:owner/name` form is the one that breaks naive `split('/')` parsing,
    // because it has no scheme and uses `:` where the others use `/`.
    #[tokio::test]
    async fn a_remote_url_yields_the_owner_slash_name_board_key() {
        for (url, want) in [
            ("https://github.com/CambrianTech/continuum.git", "CambrianTech/continuum"),
            ("https://github.com/CambrianTech/continuum", "CambrianTech/continuum"),
            ("git@github.com:CambrianTech/continuum.git", "CambrianTech/continuum"),
            ("ssh://git@github.com/CambrianTech/continuum.git", "CambrianTech/continuum"),
            // Host-agnostic: a non-GitHub remote is just as valid a board key.
            ("https://gitlab.com/acme/widget.git", "acme/widget"),
            // GitLab subgroups nest deeper — take the project's own group/name.
            ("https://gitlab.com/acme/team/widget.git", "team/widget"),
        ] {
            assert_eq!(
                repo_slug_from_remote_url(url).as_deref(),
                Some(want),
                "remote {url:?}"
            );
        }
        // Nothing to key on → None, so the caller is ASKED for `repo` rather than
        // handed a fabricated key.
        for junk in ["", "   ", "https://github.com/", "not-a-url"] {
            assert_eq!(repo_slug_from_remote_url(junk), None, "junk {junk:?}");
        }
    }

    async fn setup_git_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();

        // Initialize a git repo
        run_git(dir.path(), &["init"]).await.expect("git init should work");
        run_git(dir.path(), &["config", "user.email", "test@test.com"]).await.expect("git config email");
        run_git(dir.path(), &["config", "user.name", "Test"]).await.expect("git config name");

        // Create an initial commit
        fs::write(dir.path().join("initial.txt"), "hello\n").unwrap();
        run_git(dir.path(), &["add", "."]).await.expect("git add");
        run_git(dir.path(), &["commit", "-m", "Initial"]).await.expect("git commit");

        dir
    }

    // what this catches: the self-heal for stale citizen clones must bring the
    // workspace CURRENT with shared AND preserve the persona's own work. A
    // regression here silently either strands a persona on stale code (Casper's
    // pre-restructure clone) or clobbers her work on refresh.
    #[tokio::test]
    async fn sync_from_shared_brings_current_and_preserves_persona_work() {
        let shared = setup_git_repo().await;

        // citizen: a cp-clone of shared incl. .git — mirrors the `cp -cR` CoW clone
        // (related history, the whole reason a fetch+merge works).
        let citizen = tempfile::tempdir().unwrap();
        let out = std::process::Command::new("cp")
            .arg("-R")
            .arg(format!("{}/.", shared.path().display()))
            .arg(citizen.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "cp clone failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        run_git(citizen.path(), &["config", "user.email", "citizen@test"]).await.unwrap();
        run_git(citizen.path(), &["config", "user.name", "Citizen"]).await.unwrap();

        // shared advances (a new framework file), citizen has its OWN uncommitted work.
        fs::write(shared.path().join("framework.rs"), "// shared code\n").unwrap();
        run_git(shared.path(), &["add", "."]).await.unwrap();
        run_git(shared.path(), &["commit", "-m", "shared: add framework.rs"]).await.unwrap();
        fs::write(citizen.path().join("my_work.rs"), "// persona code\n").unwrap();

        let report = git_sync_from_shared(citizen.path(), shared.path()).await.expect("sync ok");
        assert!(report.synced, "should report a sync: {}", report.summary);

        // BOTH survive: shared's new file arrives, the persona's work is preserved.
        assert!(
            citizen.path().join("framework.rs").exists(),
            "shared file must arrive"
        );
        assert!(
            citizen.path().join("my_work.rs").exists(),
            "persona work must survive"
        );
        assert!(
            citizen.path().join("initial.txt").exists(),
            "base file still present"
        );

        // Idempotent: a second sync is a clean no-op.
        let again = git_sync_from_shared(citizen.path(), shared.path()).await.expect("2nd sync ok");
        assert!(!again.synced, "already current: {}", again.summary);
    }

    // what this catches (card 8fecb762): the sync autosaving a citizen's workspace onto
    // her card branch and merging shared into it. Kimi's 63855189 branch carried five such
    // autosaves (3,026 files) and both its PRs were closed unreviewable. On a card branch
    // the sync must commit nothing, merge nothing, and leave her uncommitted work in place.
    #[tokio::test]
    async fn sync_from_shared_leaves_a_card_branch_untouched() {
        let shared = setup_git_repo().await;
        let citizen = tempfile::tempdir().unwrap();
        let out = std::process::Command::new("cp")
            .arg("-R")
            .arg(format!("{}/.", shared.path().display()))
            .arg(citizen.path())
            .output()
            .unwrap();
        assert!(out.status.success(), "cp clone failed: {}", String::from_utf8_lossy(&out.stderr));
        run_git(citizen.path(), &["checkout", "-b", "63855189/code-github-pr-create-checks-the"]).await.unwrap();
        let head = run_git(citizen.path(), &["rev-parse", "HEAD"]).await.unwrap();

        fs::write(shared.path().join("framework.rs"), "// shared code\n").unwrap();
        run_git(shared.path(), &["add", "."]).await.unwrap();
        run_git(shared.path(), &["commit", "-m", "shared: add framework.rs"]).await.unwrap();
        fs::write(citizen.path().join("repro_scratch.py"), "# workspace debris\n").unwrap();

        let report = git_sync_from_shared(citizen.path(), shared.path()).await.expect("sync ok");
        assert!(!report.synced, "a card branch is not synced: {}", report.summary);
        assert_eq!(run_git(citizen.path(), &["rev-parse", "HEAD"]).await.unwrap(), head, "no autosave or merge commit");
        assert!(!citizen.path().join("framework.rs").exists(), "shared was not merged in");
        assert!(
            run_git(citizen.path(), &["status", "--porcelain"]).await.unwrap().contains("repro_scratch.py"),
            "her uncommitted file stays uncommitted"
        );
    }

    // what this catches: the citizen-sync modify/delete strand (glass-boxed
    // 2026-08-03 round 2): shared DELETED debris files (the #2135 sweep) that the
    // persona's autosave had modified — `-X theirs` never auto-resolves
    // modify/delete, so the sync failed forever on the very files it was meant to
    // clear. Shared's deletion must win the working tree; the persona's version
    // survives in her autosave commit (history, not tree).
    #[tokio::test]
    async fn sync_resolves_modify_delete_shared_deletion_wins() {
        let shared = setup_git_repo().await;
        // Debris file exists in the common base.
        fs::write(shared.path().join("conway.rs"), "// v1\n").unwrap();
        run_git(shared.path(), &["add", "."]).await.unwrap();
        run_git(shared.path(), &["commit", "-m", "base: debris present"]).await.unwrap();

        // Citizen cp-clone (related history).
        let citizen = tempfile::tempdir().unwrap();
        let out = std::process::Command::new("cp")
            .arg("-R")
            .arg(format!("{}/.", shared.path().display()))
            .arg(citizen.path())
            .output()
            .unwrap();
        assert!(out.status.success());
        run_git(citizen.path(), &["config", "user.email", "citizen@test"]).await.unwrap();
        run_git(citizen.path(), &["config", "user.name", "Citizen"]).await.unwrap();

        // Shared SWEEPS the debris + ships a framework file; citizen MODIFIES it
        // (uncommitted — the autosave inside the sync commits it, the live shape).
        run_git(shared.path(), &["rm", "-q", "conway.rs"]).await.unwrap();
        fs::write(shared.path().join("framework3.rs"), "// newer\n").unwrap();
        run_git(shared.path(), &["add", "framework3.rs"]).await.unwrap();
        run_git(shared.path(), &["commit", "-m", "shared: sweep debris"]).await.unwrap();
        fs::write(citizen.path().join("conway.rs"), "// persona edit\n").unwrap();

        let report = git_sync_from_shared(citizen.path(), shared.path()).await
            .expect("sync must survive modify/delete");
        assert!(report.synced, "should sync: {}", report.summary);
        assert!(
            !citizen.path().join("conway.rs").exists(),
            "shared's deletion wins the tree"
        );
        assert!(
            citizen.path().join("framework3.rs").exists(),
            "shared file arrives"
        );
        // Preserve-first: her edit is one commit back in HER history.
        let show = run_git(
            citizen.path(),
            &["log", "--all", "-S", "persona edit", "--oneline"],
        ).await
        .unwrap_or_default();
        assert!(
            !show.trim().is_empty(),
            "the persona's modified version survives in history"
        );
    }

    // what this catches: the citizen-sync submodule strand (glass-boxed 2026-08-03,
    // all four residents, every ensure): the persona's autosave commits her stale
    // vendored-submodule pointer while shared advances it, and git's recursive
    // merge refuses non-trivial gitlink divergence regardless of -X theirs
    // ("Recursive merging with submodules currently only supports trivial cases")
    // — so the self-heal ABORTED forever and every workspace stayed stale. The
    // sync must resolve gitlink conflicts shared-wins and conclude the merge.
    #[tokio::test]
    async fn sync_resolves_diverged_submodule_pointer_shared_wins() {
        fn raw_git(dir: &Path, args: &[&str]) -> std::process::Output {
            let mut c = std::process::Command::new("git");
            c.args(args).current_dir(dir);
            for v in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_PREFIX"] {
                c.env_remove(v);
            }
            c.output().expect("git spawn")
        }

        // Upstream submodule repo with two commits: P0 (the pointer both clones
        // start at) and S2 (what shared advances to).
        let sub = setup_git_repo().await;
        let _p0 = run_git(sub.path(), &["rev-parse", "HEAD"]).await
            .unwrap()
            .trim()
            .to_string();
        fs::write(sub.path().join("v2.txt"), "v2\n").unwrap();
        run_git(sub.path(), &["add", "."]).await.unwrap();
        run_git(sub.path(), &["commit", "-m", "sub: v2"]).await.unwrap();
        let s2 = run_git(sub.path(), &["rev-parse", "HEAD"]).await
            .unwrap()
            .trim()
            .to_string();

        // Shared checkout carrying the submodule at P0.
        let shared = setup_git_repo().await;
        let out = raw_git(
            shared.path(),
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub.path().display().to_string(),
                "vendor/sub",
            ],
        );
        assert!(
            out.status.success(),
            "submodule add: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        // Pin the submodule at P0 (submodule add checks out its current HEAD = S2 tip;
        // rewind the gitlink so both sides start from a common base pointer).
        run_git(shared.path(), &["commit", "-m", "shared: add submodule"]).await.unwrap();

        // Citizen: cp-clone of shared (related history), like the CoW layer.
        let citizen = tempfile::tempdir().unwrap();
        let out = std::process::Command::new("cp")
            .arg("-R")
            .arg(format!("{}/.", shared.path().display()))
            .arg(citizen.path())
            .output()
            .unwrap();
        assert!(out.status.success());
        run_git(citizen.path(), &["config", "user.email", "citizen@test"]).await.unwrap();
        run_git(citizen.path(), &["config", "user.name", "Citizen"]).await.unwrap();

        // Shared moves the pointer to S2 + ships a framework file.
        run_git(
            shared.path(),
            &[
                "update-index",
                "--cacheinfo",
                &format!("160000,{s2},vendor/sub"),
            ],
        ).await
        .unwrap();
        fs::write(shared.path().join("framework2.rs"), "// newer shared\n").unwrap();
        run_git(shared.path(), &["add", "framework2.rs"]).await.unwrap();
        run_git(
            shared.path(),
            &["commit", "-m", "shared: bump sub + framework2"],
        ).await
        .unwrap();

        // Citizen diverges the pointer to a THIRD sha (what autosave does with a
        // drifted vendored tree) plus her own uncommitted work.
        let stray = run_git(citizen.path(), &["rev-parse", "HEAD"]).await
            .unwrap()
            .trim()
            .to_string();
        run_git(
            citizen.path(),
            &[
                "update-index",
                "--cacheinfo",
                &format!("160000,{stray},vendor/sub"),
            ],
        ).await
        .unwrap();
        run_git(
            citizen.path(),
            &["commit", "-m", "citizen: drifted pointer"],
        ).await
        .unwrap();
        fs::write(citizen.path().join("my_work.rs"), "// persona code\n").unwrap();

        let report = git_sync_from_shared(citizen.path(), shared.path()).await
            .expect("sync must survive a diverged submodule pointer");
        assert!(report.synced, "should sync: {}", report.summary);

        // Shared's pointer wins; shared's file arrives; persona work survives.
        let tree = run_git(citizen.path(), &["ls-tree", "HEAD", "vendor/sub"]).await.unwrap();
        assert!(
            tree.contains(&s2),
            "gitlink must resolve to shared's sha: {tree}"
        );
        assert!(citizen.path().join("framework2.rs").exists());
        assert!(citizen.path().join("my_work.rs").exists());
    }

    #[tokio::test]
    async fn test_git_status_clean() {
        let dir = setup_git_repo().await;
        let status = git_status(dir.path()).await;
        assert!(status.success);
        assert!(status.branch.is_some());
        assert!(status.modified.is_empty());
        assert!(status.untracked.is_empty());
    }

    #[tokio::test]
    async fn test_git_status_modified() {
        let dir = setup_git_repo().await;
        fs::write(dir.path().join("initial.txt"), "modified\n").unwrap();

        let status = git_status(dir.path()).await;
        assert!(status.success);
        assert!(status.modified.contains(&"initial.txt".to_string()));
    }

    #[tokio::test]
    async fn test_git_status_untracked() {
        let dir = setup_git_repo().await;
        fs::write(dir.path().join("new_file.txt"), "new\n").unwrap();

        let status = git_status(dir.path()).await;
        assert!(status.success);
        assert!(status.untracked.contains(&"new_file.txt".to_string()));
    }

    #[tokio::test]
    async fn test_git_diff() {
        let dir = setup_git_repo().await;
        fs::write(dir.path().join("initial.txt"), "modified\n").unwrap();

        let diff = git_diff(dir.path(), false).await.unwrap();
        assert!(diff.contains("-hello"));
        assert!(diff.contains("+modified"));
    }

    #[tokio::test]
    async fn test_git_log() {
        let dir = setup_git_repo().await;
        let log = git_log(dir.path(), 5).await.unwrap();
        assert!(log.contains("Initial"));
    }

    #[tokio::test]
    async fn test_git_status_not_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        let status = git_status(dir.path()).await;
        // Should still return a result (possibly with error)
        // git status in non-repo returns error
        assert!(!status.success || status.branch.is_none());
    }

    #[tokio::test]
    async fn test_git_add_and_commit() {
        let dir = setup_git_repo().await;

        // Create a new file
        fs::write(dir.path().join("feature.txt"), "new feature\n").unwrap();

        // Stage it
        git_add(dir.path(), &["feature.txt"]).await.expect("git add should work");

        // Status should show it as added
        let status = git_status(dir.path()).await;
        assert!(status.added.contains(&"feature.txt".to_string()));

        // Commit it
        let hash = git_commit(dir.path(), "Add feature").await.expect("git commit should work");
        assert!(!hash.is_empty());
        assert!(hash.len() >= 7); // At least a short hash

        // Status should be clean now
        let status_after = git_status(dir.path()).await;
        assert!(status_after.modified.is_empty());
        assert!(status_after.added.is_empty());
        assert!(status_after.untracked.is_empty());
    }

    #[tokio::test]
    async fn test_git_commit_empty_fails() {
        let dir = setup_git_repo().await;
        // Nothing staged — commit should fail
        let result = git_commit(dir.path(), "Empty commit").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_git_add_all() {
        let dir = setup_git_repo().await;

        fs::write(dir.path().join("a.txt"), "aaa\n").unwrap();
        fs::write(dir.path().join("b.txt"), "bbb\n").unwrap();

        git_add(dir.path(), &["."]).await.expect("git add . should work");

        let status = git_status(dir.path()).await;
        // Both files should be staged (added)
        assert!(status.added.contains(&"a.txt".to_string()));
        assert!(status.added.contains(&"b.txt".to_string()));
    }

    // what this catches (card bfa87dd5): a git the core waited on ran unbounded on a blocking
    // thread, so a cancelled command or a passed deadline abandoned it, and the git and what it
    // started kept running under hands a release then removed. A git now ends with its waiter and
    // says how: at the deadline it is INTERRUPTED (its effect unknown, never "failed"), with no
    // time left it never starts, cancelled it ends before the hands' release completes, and stdin
    // it never took is an error, never a result. That the TREE ends is lane_process's contract,
    // tested once there; this pins that every git goes through it and how its ends read.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_git_ends_with_its_waiter_and_says_how_it_ended() {
        use std::os::unix::fs::PermissionsExt;
        let dir = setup_git_repo().await;
        let hooks = tempfile::tempdir().unwrap();
        let hook_pid = hooks.path().join("hook.pid");
        // a pre-commit hook is a process git starts: it records itself and outlives every bound here
        let hook = hooks.path().join("pre-commit");
        fs::write(&hook, format!("#!/bin/sh\necho $$ > {}\nexec sleep 600\n", hook_pid.display())).unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        let hooks_path = format!("core.hooksPath={}", hooks.path().display());
        let commit = ["-c", hooks_path.as_str(), "commit", "--allow-empty", "-q", "-m", "held"];

        // the deadline only has to land while the hook sleeps (600 s); 2 s is that margin, not a tuning
        let started = tokio::time::Instant::now();
        let err = within(started + std::time::Duration::from_secs(2), run_git(dir.path(), &commit)).await.unwrap_err();
        assert!(interrupted(&err) && out_of_time(&err), "a git the deadline ended is interrupted: {err}");
        assert!(!err.contains("not confirmed gone"), "its tree's end is confirmed: {err}");
        assert!(started.elapsed() < std::time::Duration::from_secs(60), "the deadline ended it, not the hook");

        let err = within(tokio::time::Instant::now(), run_git(dir.path(), &["status"])).await.unwrap_err();
        assert!(out_of_time(&err) && !interrupted(&err), "with no time left a git never starts: {err}");

        let _ = fs::remove_file(&hook_pid);
        let context = crate::code::hands::HandsContext::fresh();
        let lease = std::sync::Arc::new(crate::code::hands::admit(context).expect("open hands"));
        let run = crate::code::hands::admitted(Some(lease), run_git(dir.path(), &commit));
        tokio::select! {
            out = run => panic!("the git ended on its own: {out:?}"),
            () = async {
                while !hook_pid.exists() {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            } => {}, // the command is cancelled while its git's hook runs: the waiter is dropped
        }
        let lingering = tokio::time::timeout(std::time::Duration::from_secs(10), crate::code::hands::close(context))
            .await
            .expect("the release completes once the cancelled git's tree has ended");
        assert!(lingering.is_empty(), "nothing it started is left unconfirmed: {lingering:?}");

        // a pipe holds 64 KiB; 1 MiB to a git that never reads stdin can't be delivered whole
        let err = run_git_input(dir.path(), &["--version"], &[], Some(vec![b'x'; 1 << 20])).await.unwrap_err();
        assert_eq!(err.end, GitEnd::Unknown, "it exited 0 without taking its input: what it did is unknown");
        assert!(err.to_string().contains("--version"), "input the git never took is an error naming it, never its output: {err}");
    }
}
