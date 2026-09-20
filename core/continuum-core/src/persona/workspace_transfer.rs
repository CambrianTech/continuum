//! The workspace moves with the mind — by git, the tool built for it.
//!
//! Joel (2026-09-20, card 73eefbbb): "the workspace somehow has to transfer. Should work:
//! just commit to git and push. That is what git is for. It is built for this."
//!
//! A citizen's hands act in a card's checkout on the machine that hosts her — airc's
//! per-card worktree on the card's branch (`card_staging::stage_for_card`). Until this
//! module nothing pushed and nothing fetched: when the card was next staged on another
//! node, that node cut the branch from ITS clone's HEAD and her edits stayed behind on
//! the old machine. Three seams, plain git, no bundles, no new transport:
//!
//! 1. [`sync_after_act`] — at the end of every turn her hands were rooted at a card
//!    (`persona_workspace::restore_acting_workspace`, the one restore every acting path
//!    goes through): a work-in-progress commit of whatever the act left, then
//!    `git push -u origin <branch>`. The commit is made with plumbing
//!    (`write-tree` / `commit-tree` / `update-ref`) so an autosave can never run the
//!    repo's hooks — hooks belong to the reviewed commit she makes with `code/git/commit`.
//!    A push that fails is a NAMED outcome on the `workspace.push` probe
//!    (`ok` / `unreachable` / `rejected` / `commit_failed` / `not_pushable`), never a
//!    panic and never the turn's failure.
//! 2. [`move_blocker`] — a seat change happens only between turns and only after the last
//!    push succeeded. The placement pass asks before it moves her and defers the move
//!    while she has unpushed work or a turn in flight (`placement.move.deferred_unpushed`).
//! 3. [`arrive`] — when the card is staged on a node (`card_staging::stage_for_card`),
//!    `git fetch origin <branch>` and check the branch out AT the fetched commit before
//!    her first turn there. A local checkout that diverged (its own commits, or a dirty
//!    tree) is saved whole under `refs/continuum/stranded/<branch>-<ts>` and the remote
//!    wins — receipted on `workspace.transfer`, never silently discarded.
//!
//! What is deliberately NOT here: a detached checkout (a SWE-bench instance at its base
//! commit, cloned `--shared` from a local mirror) is `not_pushable` — it is graded on the
//! node that staged it and has no branch to carry; and a checkout with no `origin` has
//! nowhere to go. Both are said, neither is guessed.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use dashmap::DashMap;
use uuid::Uuid;

use crate::code::git_bridge::run_git;

/// Where a diverged local checkout is kept when the remote wins an arrival.
pub const STRANDED_REF_PREFIX: &str = "refs/continuum/stranded/";
/// Bound on the git subprocesses one act-end sync may take (a push to a slow remote).
/// Past it the turn proceeds and the outcome is `timed_out`; the next act tries again.
pub const SYNC_BOUND: std::time::Duration = std::time::Duration::from_secs(60);
/// Bound on the workspace question the placement pass asks before a move. Unanswered
/// = deferred (fail closed): a move that cannot prove the push landed does not happen.
pub const BLOCKER_BOUND: std::time::Duration = std::time::Duration::from_secs(5);
/// Bound on an arrival's fetch + checkout inside a claim's staging.
pub const ARRIVE_BOUND: std::time::Duration = std::time::Duration::from_secs(120);

/// What the act-end sync did with her checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushOutcome {
    /// The branch is at `origin` at `sha`: pushed now (`pushed`), or already there.
    Ok { branch: String, sha: String, committed: bool, pushed: bool },
    /// The remote could not be reached (network down, path gone, host unresolved).
    Unreachable { branch: String, error: String },
    /// The remote answered and refused (non-fast-forward, a hook, a protected branch).
    Rejected { branch: String, error: String },
    /// The work-in-progress commit itself failed (no identity, a locked index).
    CommitFailed { branch: String, error: String },
    /// Not a checkout that can carry work anywhere: no `.git`, a detached HEAD, no `origin`.
    NotPushable { reason: &'static str },
    /// The sync did not finish inside [`SYNC_BOUND`]; the turn went on without it.
    TimedOut,
}

impl PushOutcome {
    pub fn label(&self) -> &'static str {
        match self {
            PushOutcome::Ok { .. } => "ok",
            PushOutcome::Unreachable { .. } => "unreachable",
            PushOutcome::Rejected { .. } => "rejected",
            PushOutcome::CommitFailed { .. } => "commit_failed",
            PushOutcome::NotPushable { .. } => "not_pushable",
            PushOutcome::TimedOut => "timed_out",
        }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, PushOutcome::Ok { .. })
    }

    fn error(&self) -> &str {
        match self {
            PushOutcome::Unreachable { error, .. }
            | PushOutcome::Rejected { error, .. }
            | PushOutcome::CommitFailed { error, .. } => error.as_str(),
            PushOutcome::NotPushable { reason } => reason,
            PushOutcome::Ok { .. } | PushOutcome::TimedOut => "",
        }
    }
}

/// What an arrival did with the node's checkout of the card's branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrivalOutcome {
    /// `origin` has no such branch yet — the first claim anywhere; the local checkout stands.
    NoRemoteBranch { branch: String },
    /// The fetch could not reach the remote; the local checkout stands, said loudly.
    Unreachable { branch: String, error: String },
    /// Already at the fetched commit with a clean tree: nothing moved.
    Current { branch: String, sha: String },
    /// The branch is checked out at the fetched commit. `stranded` names the ref that
    /// holds what this node had (its own commits and any dirty edit) when it diverged.
    Transferred { branch: String, sha: String, from_node: Option<String>, stranded: Option<String> },
    /// A git step failed after the fetch; `stage` names it. The checkout is whatever
    /// that step left — the caller's probe carries it.
    Failed { branch: String, stage: &'static str, error: String },
}

impl ArrivalOutcome {
    pub fn label(&self) -> &'static str {
        match self {
            ArrivalOutcome::NoRemoteBranch { .. } => "no_remote_branch",
            ArrivalOutcome::Unreachable { .. } => "unreachable",
            ArrivalOutcome::Current { .. } => "current",
            ArrivalOutcome::Transferred { .. } => "transferred",
            ArrivalOutcome::Failed { .. } => "failed",
        }
    }
}

// ── the git facts, each one question ────────────────────────────────────────────────

/// The branch HEAD is on, or `None` for a detached HEAD / not a repo.
pub fn current_branch(root: &Path) -> Option<String> {
    let name = run_git(root, &["rev-parse", "--abbrev-ref", "HEAD"]).ok()?;
    let name = name.trim();
    (!name.is_empty() && name != "HEAD").then(|| name.to_string())
}

fn head_sha(root: &Path) -> Option<String> {
    run_git(root, &["rev-parse", "--verify", "--quiet", "HEAD"])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Does the working tree differ from HEAD (tracked edits or untracked files)?
fn is_dirty(root: &Path) -> Result<bool, String> {
    run_git(root, &["status", "--porcelain", "--untracked-files=all"]).map(|s| !s.trim().is_empty())
}

/// Commits reachable from HEAD that no `origin/*` ref holds — the exact set a push
/// would carry. A branch cut from the clone's HEAD and never edited counts 0: nothing
/// is lost by leaving it, so it never defers a move.
fn unpushed_commit_count(root: &Path) -> Result<u64, String> {
    let out = run_git(root, &["rev-list", "--count", "HEAD", "--not", "--remotes=origin"])?;
    out.trim().parse::<u64>().map_err(|e| format!("rev-list count unreadable ({e}): {out:?}"))
}

/// Is there work in this checkout that `origin` does not hold — a dirty tree, or
/// commits on a named branch no remote ref reaches? A detached checkout or one with no
/// `origin` carries nothing anywhere and answers `false` (it cannot be pushed, so a move
/// cannot wait on it); a checkout git cannot read answers `true` (fail closed).
pub fn has_unpushed_work(root: &Path) -> bool {
    if !root.join(".git").exists() {
        return false;
    }
    if current_branch(root).is_none() || run_git(root, &["remote", "get-url", "origin"]).is_err() {
        return false;
    }
    match (is_dirty(root), unpushed_commit_count(root)) {
        (Ok(dirty), Ok(ahead)) => dirty || ahead > 0,
        _ => true,
    }
}

/// The message a work-in-progress commit carries: which card, and which node made it —
/// the "from" half of the transfer receipt on arrival, read back by
/// [`node_of_wip_subject`]. Plain text in the commit, so plain git carries it.
pub fn wip_message(card: Uuid, node: &str) -> String {
    format!("wip({}): continuum act on node {node}", &card.to_string()[..8])
}

/// PURE: the node a [`wip_message`] names, or `None` for any other subject.
pub fn node_of_wip_subject(subject: &str) -> Option<String> {
    subject
        .trim()
        .split_once(" on node ")
        .map(|(_, node)| node.trim().to_string())
        .filter(|n| !n.is_empty())
}

/// Commit the whole working tree (tracked edits and untracked files, minus what the
/// repo's excludes hide) as one commit on `HEAD`'s parent chain, WITHOUT running any
/// hook: `add -A`, `write-tree`, `commit-tree`, and — when `advance_branch` — an
/// `update-ref` of the current branch so HEAD moves to it. With `advance_branch` false
/// the commit is dangling for the caller to name (the stranded ref). Returns the sha.
fn commit_tree_no_hooks(root: &Path, message: &str, advance_branch: bool) -> Result<String, String> {
    run_git(root, &["add", "-A"])?;
    let tree = run_git(root, &["write-tree"])?.trim().to_string();
    let parent = head_sha(root);
    let commit = match &parent {
        Some(p) => run_git(root, &["commit-tree", &tree, "-p", p, "-m", message])?,
        None => run_git(root, &["commit-tree", &tree, "-m", message])?,
    }
    .trim()
    .to_string();
    if advance_branch {
        let branch = current_branch(root).ok_or("cannot advance a detached HEAD")?;
        let reference = format!("refs/heads/{branch}");
        match &parent {
            Some(p) => run_git(root, &["update-ref", "-m", message, &reference, &commit, p])?,
            None => run_git(root, &["update-ref", "-m", message, &reference, &commit])?,
        };
    }
    Ok(commit)
}

/// PURE: was a failed push refused by the remote, or never delivered to it? The remote
/// answering "no" (a non-fast-forward, a declined hook, a protected branch, a denied
/// key) is `Rejected`; everything that reads as "could not get there" — and anything
/// unrecognised — is `Unreachable`: the conservative reading, and both are receipted
/// verbatim.
pub fn classify_push_error(err: &str) -> PushFailure {
    let e = err.to_ascii_lowercase();
    const REJECTED: [&str; 7] = [
        "[rejected]",
        "[remote rejected]",
        "non-fast-forward",
        "pre-receive hook declined",
        "protected branch",
        "permission denied",
        "authentication failed",
    ];
    if REJECTED.iter().any(|m| e.contains(m)) {
        PushFailure::Rejected
    } else {
        PushFailure::Unreachable
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushFailure {
    Rejected,
    Unreachable,
}

fn is_missing_remote_ref(err: &str) -> bool {
    let e = err.to_ascii_lowercase();
    e.contains("couldn't find remote ref") || e.contains("could not find remote ref")
}

// ── seam 1: the act-end sync ────────────────────────────────────────────────────────

/// Ensure a work-in-progress commit of whatever the act left in `root` and push the
/// current branch to `origin`. Synchronous git; the async, bounded, probed form the
/// turn path calls is [`sync_after_act_for`]. `node` is stamped into the WIP commit's
/// subject so the receiving node can say where the work came from.
pub fn sync_after_act(root: &Path, card: Uuid, node: &str) -> PushOutcome {
    if !root.join(".git").exists() {
        return PushOutcome::NotPushable { reason: "not a git checkout" };
    }
    let Some(branch) = current_branch(root) else {
        return PushOutcome::NotPushable { reason: "detached HEAD" };
    };
    if run_git(root, &["remote", "get-url", "origin"]).is_err() {
        return PushOutcome::NotPushable { reason: "no origin remote" };
    }
    let mut committed = false;
    match is_dirty(root) {
        Ok(true) => {
            if let Err(error) = commit_tree_no_hooks(root, &wip_message(card, node), true) {
                return PushOutcome::CommitFailed { branch, error };
            }
            committed = true;
        }
        Ok(false) => {}
        Err(error) => return PushOutcome::CommitFailed { branch, error },
    }
    let sha = head_sha(root).unwrap_or_default(); // unwrap_or_default: an unborn branch has no sha; the push below says so
    let ahead = match unpushed_commit_count(root) {
        Ok(n) => n,
        Err(error) => return PushOutcome::CommitFailed { branch, error },
    };
    if ahead == 0 {
        return PushOutcome::Ok { branch, sha, committed, pushed: false };
    }
    match run_git(root, &["push", "-u", "origin", &branch]) {
        Ok(_) => PushOutcome::Ok { branch, sha, committed, pushed: true },
        Err(error) => match classify_push_error(&error) {
            PushFailure::Rejected => PushOutcome::Rejected { branch, error },
            PushFailure::Unreachable => PushOutcome::Unreachable { branch, error },
        },
    }
}

/// Every checkout a persona's hands have acted in this boot — the roots
/// [`persona_has_unpushed_work`] re-reads live before a move. A root that vanished is
/// pruned on read. (A reboot forgets the set; her next act records the root again and
/// pushes what it finds — the git state on disk is the truth, this is only where to look.)
static ACTED_ROOTS: LazyLock<DashMap<Uuid, BTreeSet<PathBuf>>> = LazyLock::new(DashMap::new);

/// Record that `persona`'s hands acted in `root` (a checkout worth asking before a move).
pub fn note_acted_root(persona: Uuid, root: PathBuf) {
    ACTED_ROOTS.entry(persona).or_default().insert(root);
}

fn acted_roots_of(persona: Uuid) -> Vec<PathBuf> {
    ACTED_ROOTS.get(&persona).map(|s| s.iter().cloned().collect()).unwrap_or_default() // unwrap_or_default: never acted = no roots
}

/// Does any checkout this persona acted in hold work `origin` does not?
pub fn persona_has_unpushed_work(persona: Uuid) -> bool {
    move_blocker_over(persona, None).is_some()
}

/// Why `persona` must not change seat right now, or `None`: a turn in flight with her
/// hands rooted at a card (a move is between turns), or unpushed work in a checkout she
/// acted in (a move is after the last push landed). Names the root so the probe says
/// where the work sits. `acting_root` is passed in so the rule is testable without the
/// process-wide hands registry.
pub fn move_blocker(persona: Uuid) -> Option<String> {
    move_blocker_over(persona, crate::cognition::persona_workspace::acting_root_of(persona))
}

fn move_blocker_over(persona: Uuid, acting_root: Option<PathBuf>) -> Option<String> {
    if let Some(root) = acting_root {
        return Some(format!("turn in flight: her hands stand at {}", root.display()));
    }
    for root in acted_roots_of(persona) {
        if !root.exists() {
            if let Some(mut set) = ACTED_ROOTS.get_mut(&persona) {
                set.remove(&root);
            }
            continue;
        }
        if has_unpushed_work(&root) {
            return Some(format!("unpushed work at {}", root.display()));
        }
    }
    None
}

/// The turn-path form of [`sync_after_act`]: off the async thread, bounded by
/// [`SYNC_BOUND`], probed as `workspace.push`, and recorded so a later move can ask.
/// Never fails the turn — every shape is an outcome.
pub async fn sync_after_act_for(persona: Uuid, persona_name: &str, root: PathBuf, card: Uuid) -> PushOutcome {
    let node = crate::capacity::gossip::this_process_origin().to_string();
    let started = std::time::Instant::now();
    let root_for_git = root.clone();
    let outcome = match tokio::time::timeout(
        SYNC_BOUND,
        tokio::task::spawn_blocking(move || sync_after_act(&root_for_git, card, &node)),
    )
    .await
    {
        Ok(Ok(o)) => o,
        Ok(Err(join)) => PushOutcome::CommitFailed { branch: String::new(), error: format!("sync task panicked: {join}") },
        Err(_) => PushOutcome::TimedOut,
    };
    if !matches!(outcome, PushOutcome::NotPushable { .. }) {
        note_acted_root(persona, root.clone());
    }
    let (branch, sha, committed, pushed) = match &outcome {
        PushOutcome::Ok { branch, sha, committed, pushed } => (branch.as_str(), sha.as_str(), *committed, *pushed),
        PushOutcome::Unreachable { branch, .. }
        | PushOutcome::Rejected { branch, .. }
        | PushOutcome::CommitFailed { branch, .. } => (branch.as_str(), "", false, false),
        PushOutcome::NotPushable { .. } | PushOutcome::TimedOut => ("", "", false, false),
    };
    crate::probe!(
        class = "workspace.push",
        persona = %persona_name,
        card = %card,
        root = %root.display(),
        branch = %branch,
        sha = %sha,
        committed = committed,
        pushed = pushed,
        outcome = %outcome.label(),
        error = %outcome.error(),
        ms = started.elapsed().as_millis() as u64,
        "act over — her card branch carried to origin (or why not); a move waits on this"
    );
    outcome
}

/// [`move_blocker`] off the async thread, bounded by [`BLOCKER_BOUND`]; unanswered
/// in time = a blocker (fail closed).
pub async fn move_blocker_bounded(persona: Uuid) -> Option<String> {
    match tokio::time::timeout(BLOCKER_BOUND, tokio::task::spawn_blocking(move || move_blocker(persona))).await {
        Ok(Ok(b)) => b,
        Ok(Err(join)) => Some(format!("workspace question panicked: {join}")),
        Err(_) => Some(format!("workspace question unanswered within {} s", BLOCKER_BOUND.as_secs())),
    }
}

// ── seam 3: arrival ─────────────────────────────────────────────────────────────────

/// Bring `root` to `origin/<branch>`'s tip before her first turn on this node: fetch,
/// then check `branch` out AT the fetched commit. What this node had that the remote
/// does not — its own commits on the branch, a dirty tree — is committed whole (no
/// hooks) and kept under [`STRANDED_REF_PREFIX`]`<branch>-<now_ms>`; the remote wins the
/// branch and the tree. Synchronous git; [`arrive_for`] is the bounded, probed form.
pub fn arrive(root: &Path, branch: &str, now_ms: u64) -> ArrivalOutcome {
    let branch_s = branch.to_string();
    if !root.join(".git").exists() {
        return ArrivalOutcome::Failed { branch: branch_s, stage: "checkout", error: "not a git checkout".into() };
    }
    if let Err(error) = run_git(root, &["fetch", "--no-tags", "origin", branch]) {
        return if is_missing_remote_ref(&error) {
            ArrivalOutcome::NoRemoteBranch { branch: branch_s }
        } else {
            ArrivalOutcome::Unreachable { branch: branch_s, error }
        };
    }
    let fetched = match run_git(root, &["rev-parse", "FETCH_HEAD"]) {
        Ok(s) => s.trim().to_string(),
        Err(error) => return ArrivalOutcome::Failed { branch: branch_s, stage: "fetch_head", error },
    };
    let head = head_sha(root);
    let dirty = match is_dirty(root) {
        Ok(d) => d,
        Err(error) => return ArrivalOutcome::Failed { branch: branch_s, stage: "status", error },
    };
    if head.as_deref() == Some(fetched.as_str()) && !dirty && current_branch(root).as_deref() == Some(branch) {
        return ArrivalOutcome::Current { branch: branch_s, sha: fetched };
    }
    // Local-only commits: what HEAD reaches that the fetched tip does not.
    let local_only = match head {
        Some(_) => match run_git(root, &["rev-list", "--count", &format!("{fetched}..HEAD")]) {
            Ok(s) => s.trim().parse::<u64>().unwrap_or(0), // unwrap_or: unreadable count reads as none; the dirty check still strands edits
            Err(error) => return ArrivalOutcome::Failed { branch: branch_s, stage: "rev_list", error },
        },
        None => 0,
    };
    let mut stranded = None;
    if dirty || local_only > 0 {
        let reference = format!("{STRANDED_REF_PREFIX}{branch}-{now_ms}");
        let keep = if dirty {
            match commit_tree_no_hooks(root, &format!("stranded: local state of {branch} before transfer"), false) {
                Ok(sha) => sha,
                Err(error) => return ArrivalOutcome::Failed { branch: branch_s, stage: "strand_commit", error },
            }
        } else {
            head.clone().unwrap_or_default() // unwrap_or_default: local_only > 0 implies a HEAD
        };
        if let Err(error) = run_git(root, &["update-ref", &reference, &keep]) {
            return ArrivalOutcome::Failed { branch: branch_s, stage: "strand_ref", error };
        }
        stranded = Some(reference);
    }
    // The remote wins: the branch at the fetched tip, the tree exactly that commit.
    if let Err(error) = run_git(root, &["checkout", "-q", "-B", branch]) {
        return ArrivalOutcome::Failed { branch: branch_s, stage: "checkout", error };
    }
    if let Err(error) = run_git(root, &["reset", "-q", "--hard", &fetched]) {
        return ArrivalOutcome::Failed { branch: branch_s, stage: "reset", error };
    }
    // Best effort: name the upstream so `@{upstream}` reads on this node too.
    let _ = run_git(root, &["branch", "--set-upstream-to", &format!("origin/{branch}")]);
    let from_node = run_git(root, &["log", "-1", "--format=%s", &fetched])
        .ok()
        .and_then(|s| node_of_wip_subject(&s));
    ArrivalOutcome::Transferred { branch: branch_s, sha: fetched, from_node, stranded }
}

/// The staging-path form of [`arrive`]: off the async thread, bounded by
/// [`ARRIVE_BOUND`], receipted as `workspace.transfer` with card, branch, sha, from and
/// to node. The claim is authoritative and the checkout is convenience: no outcome here
/// undoes a claim, every outcome is said.
pub async fn arrive_for(root: PathBuf, branch: String, card: Uuid) -> ArrivalOutcome {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0); // unwrap_or: a pre-epoch clock stamps 0; the ref is still unique per branch per arrival in practice
    let started = std::time::Instant::now();
    let (r, b) = (root.clone(), branch.clone());
    let outcome = match tokio::time::timeout(ARRIVE_BOUND, tokio::task::spawn_blocking(move || arrive(&r, &b, now_ms))).await {
        Ok(Ok(o)) => o,
        Ok(Err(join)) => ArrivalOutcome::Failed { branch: branch.clone(), stage: "task", error: format!("arrival task panicked: {join}") },
        Err(_) => ArrivalOutcome::Failed {
            branch: branch.clone(),
            stage: "timeout",
            error: format!("fetch + checkout did not finish within {} s", ARRIVE_BOUND.as_secs()),
        },
    };
    let (sha, from_node, stranded, error) = match &outcome {
        ArrivalOutcome::Transferred { sha, from_node, stranded, .. } => {
            (sha.as_str(), from_node.clone().unwrap_or_default(), stranded.clone().unwrap_or_default(), String::new()) // unwrap_or_default: probe labels only
        }
        ArrivalOutcome::Current { sha, .. } => (sha.as_str(), String::new(), String::new(), String::new()),
        ArrivalOutcome::Unreachable { error, .. } | ArrivalOutcome::Failed { error, .. } => ("", String::new(), String::new(), error.clone()),
        ArrivalOutcome::NoRemoteBranch { .. } => ("", String::new(), String::new(), String::new()),
    };
    let to_node = crate::capacity::gossip::this_process_origin().to_string();
    crate::probe!(
        class = "workspace.transfer",
        card = %card,
        branch = %branch,
        root = %root.display(),
        sha = %sha,
        from_node = %from_node,
        to_node = %to_node,
        stranded = %stranded,
        outcome = %outcome.label(),
        error = %error,
        ms = started.elapsed().as_millis() as u64,
        "the card's branch arrived on this node before her first turn here"
    );
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) -> String {
        run_git(dir, args).unwrap_or_else(|e| panic!("git {args:?} in {}: {e}", dir.display()))
    }
    fn head(dir: &Path) -> String {
        git(dir, &["rev-parse", "HEAD"]).trim().to_string()
    }
    fn identity(dir: &Path) {
        git(dir, &["config", "user.email", "t@t"]);
        git(dir, &["config", "user.name", "t"]);
    }

    /// A bare `origin` and two clones — machine A and machine B — sharing one `main`
    /// commit; A stands on the card's branch, cut from main, the way `ensure_worktree`
    /// cuts it from the clone's HEAD.
    struct TwoMachines {
        tmp: tempfile::TempDir,
        origin: PathBuf,
        a: PathBuf,
        b: PathBuf,
        branch: String,
    }

    fn two_machines() -> TwoMachines {
        let tmp = tempfile::tempdir().expect("tempdir");
        let origin = tmp.path().join("origin.git");
        let a = tmp.path().join("machine-a");
        let b = tmp.path().join("machine-b");
        git(tmp.path(), &["init", "-q", "--bare", "-b", "main", &origin.to_string_lossy()]);
        git(tmp.path(), &["init", "-q", "-b", "main", &a.to_string_lossy()]);
        identity(&a);
        std::fs::write(a.join("README"), "shared\n").unwrap();
        git(&a, &["add", "README"]);
        git(&a, &["commit", "-q", "-m", "base"]);
        git(&a, &["remote", "add", "origin", &origin.to_string_lossy()]);
        git(&a, &["push", "-q", "-u", "origin", "main"]);
        git(tmp.path(), &["clone", "-q", &origin.to_string_lossy(), &b.to_string_lossy()]);
        identity(&b);
        let branch = "73eefbbb/workspace-moves-with-the-mind".to_string();
        git(&a, &["checkout", "-q", "-b", &branch]);
        TwoMachines { tmp, origin, a, b, branch }
    }

    // what this catches (card 73eefbbb): the transfer itself. Machine A's act leaves an
    // edit; the act-end sync WIP-commits and pushes the card branch; machine B stages
    // the card (a branch cut from ITS clone's HEAD, the pre-fix state) and arrives:
    // fetched, checked out at A's sha, tree equal to A's, the from-node read off the
    // commit. Before this nothing pushed and B's branch stayed at the shared base.
    #[test]
    fn machine_b_arrives_at_the_sha_machine_a_pushed_with_the_same_tree() {
        let m = two_machines();
        let card = Uuid::new_v4();
        std::fs::write(m.a.join("src.txt"), "edit from A\n").unwrap();
        assert!(has_unpushed_work(&m.a), "a dirty tree is unpushed work");
        let out = sync_after_act(&m.a, card, "node-a");
        let sha = match &out {
            PushOutcome::Ok { sha, committed: true, pushed: true, branch } => {
                assert_eq!(*branch, m.branch);
                sha.clone()
            }
            other => panic!("expected a committed push, got {other:?}"),
        };
        assert!(!has_unpushed_work(&m.a), "pushed = nothing unpushed");
        assert_eq!(git(&m.origin, &["rev-parse", &format!("refs/heads/{}", m.branch)]).trim(), sha, "origin holds the branch at her sha");
        assert!(
            matches!(sync_after_act(&m.a, card, "node-a"), PushOutcome::Ok { committed: false, pushed: false, .. }),
            "nothing new = nothing committed, nothing pushed"
        );

        git(&m.b, &["checkout", "-q", "-b", &m.branch]);
        let arrived = arrive(&m.b, &m.branch, 7);
        match &arrived {
            ArrivalOutcome::Transferred { sha: s, stranded: None, from_node: Some(n), .. } => {
                assert_eq!(*s, sha);
                assert_eq!(n, "node-a", "the WIP commit names the node it came from");
            }
            other => panic!("expected a clean transfer, got {other:?}"),
        }
        assert_eq!(head(&m.b), sha);
        assert_eq!(std::fs::read_to_string(m.b.join("src.txt")).unwrap(), "edit from A\n", "B's tree is A's");
        assert_eq!(git(&m.b, &["status", "--porcelain"]).trim(), "");
        assert!(matches!(arrive(&m.b, &m.branch, 8), ArrivalOutcome::Current { .. }), "a second arrival moves nothing");
        assert!(!has_unpushed_work(&m.b), "at origin's tip: nothing to carry");
    }

    // what this catches: a push that cannot reach origin is a NAMED outcome, the work is
    // committed locally (nothing lost), and the placement question answers "blocked"
    // until a later act's sync lands the push — never a panic, never a silent move.
    #[test]
    fn an_unreachable_remote_is_named_and_defers_the_move_until_the_push_lands() {
        let m = two_machines();
        let (card, persona) = (Uuid::new_v4(), Uuid::new_v4());
        std::fs::write(m.a.join("x.txt"), "x\n").unwrap();
        let gone = m.tmp.path().join("origin.gone");
        std::fs::rename(&m.origin, &gone).unwrap();
        let out = sync_after_act(&m.a, card, "node-a");
        assert!(matches!(out, PushOutcome::Unreachable { .. }), "{out:?}");
        assert_eq!(out.label(), "unreachable");
        assert_eq!(git(&m.a, &["status", "--porcelain"]).trim(), "", "the WIP commit landed locally");
        assert!(has_unpushed_work(&m.a), "a local-only commit is unpushed work");
        note_acted_root(persona, m.a.clone());
        assert!(persona_has_unpushed_work(persona));
        let why = move_blocker_over(persona, None).expect("blocked");
        assert!(why.contains("unpushed work at"), "{why}");
        assert!(
            move_blocker_over(persona, Some(m.a.clone())).expect("blocked").starts_with("turn in flight"),
            "a turn in flight blocks before the git question is even asked"
        );
        std::fs::rename(&gone, &m.origin).unwrap();
        let out = sync_after_act(&m.a, card, "node-a");
        assert!(matches!(out, PushOutcome::Ok { committed: false, pushed: true, .. }), "{out:?}");
        assert!(move_blocker_over(persona, None).is_none(), "the push landed: free to move");
        // A vanished root is pruned, not a permanent blocker.
        note_acted_root(persona, m.tmp.path().join("never-existed"));
        assert!(move_blocker_over(persona, None).is_none());
    }

    // what this catches: a local checkout that DIVERGED from what origin holds (its own
    // commit and a dirty edit) is kept whole under the stranded ref and the remote wins
    // the branch and the tree — never a silent discard, never a guess between the two.
    // Also: a push origin refuses (non-fast-forward) reads `rejected`, not `unreachable`.
    #[test]
    fn a_diverged_local_checkout_is_stranded_under_a_ref_and_the_remote_wins() {
        let m = two_machines();
        let card = Uuid::new_v4();
        std::fs::write(m.a.join("src.txt"), "from A\n").unwrap();
        let PushOutcome::Ok { sha: sha_a, .. } = sync_after_act(&m.a, card, "node-a") else { panic!("push") };
        // B: the same branch cut from main, its own commit Y, and an uncommitted edit.
        git(&m.b, &["checkout", "-q", "-b", &m.branch]);
        std::fs::write(m.b.join("y.txt"), "B's commit\n").unwrap();
        git(&m.b, &["add", "y.txt"]);
        git(&m.b, &["commit", "-q", "-m", "B's own"]);
        let y = head(&m.b);
        std::fs::write(m.b.join("z.txt"), "B's dirty edit\n").unwrap();
        let arrived = arrive(&m.b, &m.branch, 42);
        let ArrivalOutcome::Transferred { sha, stranded: Some(reference), .. } = &arrived else {
            panic!("expected a transfer with a stranded ref, got {arrived:?}");
        };
        assert_eq!(*sha, sha_a);
        assert_eq!(*reference, format!("{STRANDED_REF_PREFIX}{}-42", m.branch));
        let kept = git(&m.b, &["rev-parse", reference]).trim().to_string();
        assert_eq!(git(&m.b, &["rev-parse", &format!("{reference}^")]).trim(), y, "the stranded commit sits on B's own commit");
        assert_eq!(git(&m.b, &["show", &format!("{kept}:z.txt")]), "B's dirty edit\n", "the dirty edit is inside it");
        assert_eq!(head(&m.b), sha_a, "the remote won the branch");
        assert!(!m.b.join("y.txt").exists() && !m.b.join("z.txt").exists(), "…and the tree");
        assert_eq!(std::fs::read_to_string(m.b.join("src.txt")).unwrap(), "from A\n");
        assert_eq!(git(&m.b, &["status", "--porcelain"]).trim(), "");
        // A clean checkout that is merely BEHIND strands nothing.
        std::fs::write(m.a.join("src.txt"), "from A again\n").unwrap();
        let PushOutcome::Ok { sha: sha_a2, .. } = sync_after_act(&m.a, card, "node-a") else { panic!("push") };
        match arrive(&m.b, &m.branch, 43) {
            ArrivalOutcome::Transferred { sha, stranded: None, .. } => assert_eq!(sha, sha_a2),
            other => panic!("behind, clean: a plain fast-forward, got {other:?}"),
        }
        // Rejected: B commits on the tip, A pushes first, B's push is a non-fast-forward.
        std::fs::write(m.a.join("src.txt"), "A wins the race\n").unwrap();
        assert!(sync_after_act(&m.a, card, "node-a").is_ok());
        std::fs::write(m.b.join("late.txt"), "late\n").unwrap();
        let out = sync_after_act(&m.b, card, "node-b");
        assert!(matches!(out, PushOutcome::Rejected { .. }), "{out:?}");
        // The next arrival on B strands the rejected commit and takes A's tip.
        assert!(matches!(arrive(&m.b, &m.branch, 44), ArrivalOutcome::Transferred { stranded: Some(_), .. }));
    }

    // what this catches: the two pure readers. The push-error classifier must not call a
    // refused push "unreachable" (a move would wait on a network that is fine) nor a dead
    // network "rejected"; the WIP subject parser is the only source of the from-node.
    #[test]
    fn push_errors_classify_and_the_wip_subject_names_its_node() {
        assert_eq!(classify_push_error("! [rejected] x -> x (fetch first)"), PushFailure::Rejected);
        assert_eq!(classify_push_error("remote: error: GH006: Protected branch update failed"), PushFailure::Rejected);
        assert_eq!(classify_push_error("fatal: Could not read from remote repository."), PushFailure::Unreachable);
        assert_eq!(classify_push_error("ssh: Could not resolve hostname github.com"), PushFailure::Unreachable);
        assert_eq!(classify_push_error("something new"), PushFailure::Unreachable, "unknown reads as not delivered");
        let card = Uuid::new_v4();
        assert_eq!(node_of_wip_subject(&wip_message(card, "node-a")).as_deref(), Some("node-a"));
        assert!(wip_message(card, "n").starts_with(&format!("wip({})", &card.to_string()[..8])));
        assert_eq!(node_of_wip_subject("fix: the printer"), None);
        // Not pushable is said, not guessed.
        let m = two_machines();
        git(&m.a, &["checkout", "-q", "--detach"]);
        assert_eq!(sync_after_act(&m.a, card, "n"), PushOutcome::NotPushable { reason: "detached HEAD" });
        assert!(!has_unpushed_work(&m.a), "a detached tree cannot be carried, so it never defers");
        assert!(matches!(arrive(&m.b, "no/such-branch", 1), ArrivalOutcome::NoRemoteBranch { .. }));
    }
}
