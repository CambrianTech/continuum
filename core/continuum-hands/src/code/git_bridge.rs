//! Git Bridge — git status, diff, and branch operations.
//!
//! Shells out to `git` CLI for operations. This keeps the implementation
//! simple and avoids pulling in libgit2 as a dependency.
//!
//! Every git child runs OWNED (card bfa87dd5): through `continuum_process::run_owned_with_input`, so a
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
    // Only the repository path may become a board key or model-facing fact.
    // URL parsing separates credentials, ports, query and fragment from that path.
    // Local paths are not hosted repository identities, even if they have two parts.
    let parsed;
    let path = if url.contains("://") {
        parsed = reqwest::Url::parse(url).ok()?;
        if parsed.scheme() == "file" || parsed.host_str().is_none() {
            return None;
        }
        parsed.path()
    } else {
        let (host, path) = url.split_once(':')?;
        if host.len() == 1 || host.contains(['/', '\\']) || host.is_empty() {
            return None;
        }
        // SCP syntax has no query/fragment; refuse ambiguous suffixes rather than
        // displaying a credential-looking parameter as part of the repository.
        if path.contains(['?', '#', '@', ':', '\\']) {
            return None;
        }
        path
    };
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);

    let mut segments = path.rsplit('/').filter(|s| !s.is_empty());
    let name = segments.next()?;
    let owner = segments.next()?;
    if [name, owner].iter().any(|part| {
        part.is_empty() || matches!(*part, "." | "..") || part.chars().any(char::is_whitespace)
    }) {
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
    if crate::code::card_branch::is_card_branch(&branch) {
        continuum_base::probe!(
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
                continuum_base::probe!(
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
pub async fn within<F: std::future::Future>(deadline: tokio::time::Instant, work: F) -> F::Output {
    DEADLINE.scope(deadline, work).await
}

/// What a git error says when its deadline ended the git mid-run: its effect is UNKNOWN (a
/// push may have reached the remote). One phrase, defined here and read by [`interrupted`].
const INTERRUPTED: &str = "interrupted at its deadline: its effect is unknown";
/// What a git error says when its deadline had already passed: it never started.
const NOT_STARTED: &str = "not started: its deadline had passed";

/// Did the deadline end this git mid-run (its effect unknown)?
#[cfg(test)]
pub(crate) fn interrupted(err: &str) -> bool {
    err.contains(INTERRUPTED)
}

/// Did the deadline stop this git before it started, or end it mid-run?
pub fn out_of_time(err: &str) -> bool {
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
pub async fn run_git(workspace_root: &Path, args: &[&str]) -> Result<String, String> {
    Ok(run_git_input(workspace_root, args, &[], None).await?)
}

/// [`run_git`] with extra environment for this one invocation (e.g. `GIT_AUTHOR_*` so a
/// citizen's act commit is authored by her). Same hygiene; the env is applied last.
pub async fn run_git_with_env(workspace_root: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<String, String> {
    Ok(run_git_input(workspace_root, args, env, None).await?)
}

/// How a git that gave no answer ended, for a caller whose next step depends on whether it had
/// an effect (a push: did the remote take it?).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitEnd {
    /// It never ran: its deadline had passed before it started. It had no effect.
    NeverRan,
    /// It exited non-zero: git's own verdict that it failed. For a push that doesn't prove the
    /// remote is unchanged (it can take the ref and the connection fail before git hears so).
    Refused,
    /// Its effect is unknown: its deadline ended it mid-run, it exited 0 but its input or
    /// output wasn't whole or readable, or the runner failed around it (Codex reviewer on #5037).
    Unknown,
}

/// A git that gave no answer: how it ended, and the one rendering of why.
#[derive(Debug)]
pub struct GitError {
    pub end: GitEnd,
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
/// through `continuum_process::run_owned_with_input`, within the task's [`deadline`] when one is set.
/// An exit that isn't success is an `Err` with git's stderr; so is an output that wasn't read
/// whole or an input that wasn't delivered; a deadline that ended it says its effect is UNKNOWN
/// (a push may have landed), never that nothing happened.
pub async fn run_git_input(workspace_root: &Path, args: &[&str], env: &[(&str, &str)], input: Option<Vec<u8>>) -> Result<String, GitError> {
    use continuum_process::{run_owned_with_input, Answer, Why};
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
    // A dead git's index.lock blocks every write in this tree until it is removed (card
    // d7765b90: 6 residents on enrico could not commit for ~10 days). One stat on a clean tree.
    settle_index_lock(workspace_root, continuum_git::index_lock::STALE_INDEX_LOCK).await;
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
        // GIT_CEILING_DIRECTORIES caps upward discovery AT workspace_root:
        // the ceiling is its PARENT, because git never chdirs up INTO a
        // ceiling but does check the ceiling's children. A ceiling of the
        // root itself let a root with an invalid or empty `.git` discover
        // the repository above it (measured on macOS and Windows git,
        // Codex reviewer on #5041). A root with no parent ("/") has
        // nothing above it.
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_PREFIX")
        .env("GIT_CEILING_DIRECTORIES", workspace_root.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(workspace_root))
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
    // the shared projection (continuum_process::OwnedOutput::answer): a non-zero exit is git's own
    // failure, with its stderr, and any delivery note beside it; a deadline that ended it says
    // its effect is unknown
    if let Answer::Not(unanswered) = out.answer() {
        let (end, at_deadline) = match unanswered.why {
            Why::Exited(_) => (GitEnd::Refused, String::new()),
            Why::NotWhole => (GitEnd::Unknown, String::new()),
            Why::TimedOut { .. } => {
                // this process just ended that git: its lock is that git's, unless another git
                // now works in the tree, so it goes now rather than in ten minutes
                settle_index_lock(workspace_root, std::time::Duration::ZERO).await;
                (GitEnd::Unknown, format!(" {INTERRUPTED}"))
            }
        };
        return Err(GitError { end, message: format!("git {}{at_deadline}: {unanswered}", args.join(" ")) });
    }
    // it exited 0 with everything read whole, but what it said isn't text: what it did is unknown
    String::from_utf8(out.stdout.bytes).map_err(|e| GitError { end: GitEnd::Unknown, message: format!("git {}: invalid UTF-8 in its output: {e}", args.join(" ")) })
}

/// [`continuum_git::index_lock::settle`] for a citizen's workspace, off the async threads (its
/// process scan blocks), with the outcome probed: a lock left in place or removed is named with
/// its path and age, never silent.
async fn settle_index_lock(workspace_root: &Path, stale_after: std::time::Duration) {
    use continuum_git::index_lock::{settle, Settled};
    let root = workspace_root.to_path_buf();
    let settled = match tokio::task::spawn_blocking(move || settle(&root, stale_after)).await {
        Ok(settled) => settled,
        Err(error) => {
            continuum_base::probe!(class = "hands.git.index_lock_unjudged", root = %workspace_root.display(),
                error = %error, "the index-lock check did not finish; this git runs without it");
            return;
        }
    };
    match settled {
        Settled::Absent => {}
        Settled::Held { lock, age } => continuum_base::probe!(
            class = "hands.git.index_lock_held", lock = %lock.display(), age_s = age.as_secs(),
            stale_after_s = stale_after.as_secs(),
            "a git index.lock a live git may hold — left in place; this git may fail on it"
        ),
        Settled::Cleared { lock, age } => continuum_base::probe!(
            class = "hands.git.index_lock_cleared", lock = %lock.display(), age_s = age.as_secs(),
            stale_after_s = stale_after.as_secs(),
            "removed a dead git's index.lock (no git running in the tree)"
        ),
        Settled::Failed { lock, age, error } => continuum_base::probe!(
            class = "hands.git.index_lock_unremovable", lock = %lock.display(), age_s = age.as_secs(),
            error = %error, "a dead git's index.lock could not be removed; this git will fail on it"
        ),
    }
}

#[cfg(test)]
mod tests;
