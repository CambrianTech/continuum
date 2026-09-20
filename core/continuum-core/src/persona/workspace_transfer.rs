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
//! ## A push is a TRANSFER only when the other node can read its target
//!
//! Cormac's condition on #4278, and the defect it names: a staged SWE-bench copy's
//! `origin` is THIS NODE'S OWN LOCAL MIRROR (`swe_bench::clone_at` clones `--shared`
//! from `<cache>/swe/mirrors/<repo>`). A push there would report `ok` into a repository
//! no other node has ever heard of — the blocker would clear, she would move, and the
//! arriving node's fetch would read `no_remote_branch`: her acts stranded, under a
//! receipt that says carried. A receipt must never say carried when nothing crossed.
//!
//! So every carry is classified FIRST ([`is_transfer_target`], pure): an `https`/`ssh`/
//! scp-style origin is a remote both nodes read; a filesystem path — which is what every
//! `--shared`/`--mirror` clone of a local cache has — is not, and neither is `file://`
//! or a Windows drive. A checkout that cannot carry is never pushed and never committed
//! into: it reports `not_pushable`, and [`has_unpushed_work`] stays TRUE while real work
//! sits there, so the move DEFERS honestly (she is pinned) instead of stranding her.
//!
//! Nor is the benchmark upstream an answer: a SWE copy's mirror comes from
//! `github.com/<the task's repo>`, which we do not own and must never push to. Such work
//! is graded on the node that staged it (`staged_workspace::owners_of`) and has no
//! branch to carry — it is `detached HEAD`, and pins nobody.

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

/// Why a checkout carries nothing anywhere. `NOT_A_REPO`/`DETACHED` pin nobody — there
/// is no branch of hers to lose; `LOCAL_ORIGIN`/`NO_ORIGIN` DO, because real work can sit
/// there and no push can move it.
pub const NOT_A_REPO_REASON: &str = "not a git checkout";
pub const DETACHED_REASON: &str =
    "detached HEAD: no branch to carry (a benchmark copy is graded where it was staged)";
pub const LOCAL_ORIGIN_REASON: &str =
    "origin is a local path: a --shared/--mirror clone of this node's own cache is not a transfer target";
pub const NO_ORIGIN_REASON: &str = "no origin remote";

/// PURE: can a push to this origin URL be READ by another node?
///
/// The question the whole transfer rests on. `https://`, `ssh://`, `git://` and the
/// scp-style `git@host:owner/name` are remotes a second machine resolves the same way.
/// Everything else is this machine's own filesystem — an absolute or relative path, a
/// `~` path, `file://`, a Windows drive (`C:\repo`, whose single-letter "host" is what
/// separates it from `git@host:path`) — and a push there crosses nothing.
pub fn is_transfer_target(origin: &str) -> bool {
    let url = origin.trim();
    if url.is_empty() {
        return false;
    }
    if let Some((scheme, rest)) = url.split_once("://") {
        return !scheme.eq_ignore_ascii_case("file") && !rest.trim().is_empty();
    }
    if url.starts_with('/') || url.starts_with('.') || url.starts_with('~') || url.starts_with('\\') {
        return false;
    }
    // scp-style `user@host:path`. A Windows drive letter is one char before the colon;
    // a bare relative path has no colon at all.
    match url.split_once(':') {
        Some((host, path)) => host.len() > 1 && !path.trim().is_empty(),
        None => false,
    }
}

/// How work in a checkout can leave this node — asked before anything is committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Carry {
    /// `origin` is a remote another node reads: a push carries her work there.
    Via(String),
    /// A git checkout on a branch whose `origin` is a local path: work here is real and
    /// CANNOT be carried by a push. She is pinned while it sits there.
    LocalOrigin,
    /// A git checkout on a branch with no `origin` at all: same pin, different absence.
    NoOrigin,
    /// Nothing here can hold a branch's work: not a repo, or a detached HEAD.
    Nothing(&'static str),
}

/// Classify what `root` can do with work — the one place the four shapes are told apart,
/// so the push seam and the "may she move" seam can never disagree about a checkout.
pub fn carry_of(root: &Path) -> Carry {
    if !root.join(".git").exists() {
        return Carry::Nothing(NOT_A_REPO_REASON);
    }
    // Also the UNBORN branch of a just-`git init`ed workspace: `rev-parse --abbrev-ref
    // HEAD` fails there, and with no commit there is nothing to carry yet either.
    if current_branch(root).is_none() {
        return Carry::Nothing(DETACHED_REASON);
    }
    match run_git(root, &["remote", "get-url", "origin"]) {
        Ok(url) if is_transfer_target(&url) => Carry::Via(url.trim().to_string()),
        Ok(_) => Carry::LocalOrigin,
        Err(_) => Carry::NoOrigin,
    }
}

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

    /// Does this outcome leave work on THIS node that a seat change would strand? Every
    /// failure does, and so does a checkout that cannot carry but can hold work (a local
    /// origin, no origin). A clean `Ok` and a checkout with no branch of hers pin nobody.
    pub fn pins_the_mind(&self) -> bool {
        match self {
            PushOutcome::Ok { .. } => false,
            PushOutcome::NotPushable { reason } => {
                *reason == LOCAL_ORIGIN_REASON || *reason == NO_ORIGIN_REASON
            }
            _ => true,
        }
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
    /// This checkout's `origin` is not a remote another node writes to (a `--shared`
    /// clone of a local mirror), so no fetch here can deliver another node's work.
    NotTransferable { branch: String, reason: &'static str },
}

impl ArrivalOutcome {
    pub fn label(&self) -> &'static str {
        match self {
            ArrivalOutcome::NoRemoteBranch { .. } => "no_remote_branch",
            ArrivalOutcome::Unreachable { .. } => "unreachable",
            ArrivalOutcome::Current { .. } => "current",
            ArrivalOutcome::Transferred { .. } => "transferred",
            ArrivalOutcome::Failed { .. } => "failed",
            ArrivalOutcome::NotTransferable { .. } => "not_transferable",
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

/// Is there work here that `origin` does not hold — so a move would strand it?
///
/// A carryable checkout ([`Carry::Via`]) answers on the push's own terms: a dirty tree,
/// or commits no `origin/*` ref reaches. A checkout that CANNOT carry but can hold work
/// (a local-path origin — every `--shared` clone of this node's cache — or no origin)
/// answers on what sits there, and a yes PINS her: moving would leave it behind with
/// nothing able to fetch it. Only a checkout with no branch of hers at all answers `false`
/// outright. A checkout git cannot read answers `true` (fail closed).
pub fn has_unpushed_work(root: &Path) -> bool {
    match carry_of(root) {
        Carry::Nothing(_) => false,
        // No origin refs exist to count commits against, so the dirty tree IS the work:
        // nothing ever commits into a checkout that cannot carry (see `sync_after_act`).
        Carry::NoOrigin => is_dirty(root).unwrap_or(true), // unwrap_or: unreadable = assume work, never move on a guess
        Carry::Via(_) | Carry::LocalOrigin => match (is_dirty(root), unpushed_commit_count(root)) {
            (Ok(dirty), Ok(ahead)) => dirty || ahead > 0,
            _ => true,
        },
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
    sync_after_act_over(root, card, node, carry_of(root))
}

/// [`sync_after_act`] with the carry decision supplied — the POLICY (can this checkout
/// reach another node at all) separated from the MECHANISM (commit, then push). Split
/// because the two need different proofs: the policy is a pure function over origin URLs,
/// tested against the real shapes including `clone_at`'s mirror path; the mechanism needs
/// two real clones and a real remote, and an offline test's remote is necessarily a local
/// path — the very thing the policy refuses. Injecting the decision lets each be tested as
/// what it is, instead of a fixture quietly proving neither.
fn sync_after_act_over(root: &Path, card: Uuid, node: &str, carry: Carry) -> PushOutcome {
    // Asked BEFORE anything is written: a checkout that cannot carry is left exactly as
    // her act left it. No WIP commit either — a commit whose push can never happen only
    // makes the work harder to see (and a benchmark copy's commits are its grade).
    match carry {
        Carry::Via(_) => {}
        Carry::Nothing(reason) => return PushOutcome::NotPushable { reason },
        Carry::LocalOrigin => return PushOutcome::NotPushable { reason: LOCAL_ORIGIN_REASON },
        Carry::NoOrigin => return PushOutcome::NotPushable { reason: NO_ORIGIN_REASON },
    }
    let Some(branch) = current_branch(root) else {
        return PushOutcome::NotPushable { reason: DETACHED_REASON };
    };
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
    // Recorded whenever this checkout could hold work a move would strand — INCLUDING a
    // local-path origin, which is exactly the case that must pin her. Only a checkout
    // with no branch of hers is forgotten.
    if outcome.is_ok() || outcome.pins_the_mind() {
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
    // The mirror of the push guard (and the same policy/mechanism split): fetching from a
    // target no other node can push to cannot deliver another node's work, so say so
    // rather than report a transfer that only ever re-read this machine's own cache.
    if let Ok(url) = run_git(root, &["remote", "get-url", "origin"]) {
        if !is_transfer_target(&url) {
            return ArrivalOutcome::NotTransferable { branch: branch.to_string(), reason: LOCAL_ORIGIN_REASON };
        }
    }
    arrive_over(root, branch, now_ms)
}

/// [`arrive`] past the transfer-target policy — the fetch / checkout / strand mechanism.
fn arrive_over(root: &Path, branch: &str, now_ms: u64) -> ArrivalOutcome {
    let branch_s = branch.to_string();
    if !root.join(".git").exists() {
        return ArrivalOutcome::Failed { branch: branch_s, stage: "checkout", error: NOT_A_REPO_REASON.into() };
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
        ArrivalOutcome::NotTransferable { reason, .. } => ("", String::new(), String::new(), (*reason).to_string()),
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

    /// The act-end sync's MECHANISM, with the carry decision granted. An offline fixture's
    /// remote is a local bare repo — exactly what the POLICY refuses — so the two are
    /// tested apart: the policy in
    /// `a_push_only_transfers_when_the_other_node_can_read_its_target`, the mechanism here.
    fn sync(root: &Path, card: Uuid, node: &str) -> PushOutcome {
        sync_after_act_over(root, card, node, Carry::Via("https://origin.test/repo.git".into()))
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
        let out = sync(&m.a, card, "node-a");
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
            matches!(sync(&m.a, card, "node-a"), PushOutcome::Ok { committed: false, pushed: false, .. }),
            "nothing new = nothing committed, nothing pushed"
        );

        git(&m.b, &["checkout", "-q", "-B", &m.branch]);
        let arrived = arrive_over(&m.b, &m.branch, 7);
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
        assert!(matches!(arrive_over(&m.b, &m.branch, 8), ArrivalOutcome::Current { .. }), "a second arrival moves nothing");
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
        let out = sync(&m.a, card, "node-a");
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
        let out = sync(&m.a, card, "node-a");
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
        let PushOutcome::Ok { sha: sha_a, .. } = sync(&m.a, card, "node-a") else { panic!("push") };
        // B: the same branch cut from main, its own commit Y, and an uncommitted edit.
        git(&m.b, &["checkout", "-q", "-B", &m.branch]);
        std::fs::write(m.b.join("y.txt"), "B's commit\n").unwrap();
        git(&m.b, &["add", "y.txt"]);
        git(&m.b, &["commit", "-q", "-m", "B's own"]);
        let y = head(&m.b);
        std::fs::write(m.b.join("z.txt"), "B's dirty edit\n").unwrap();
        let arrived = arrive_over(&m.b, &m.branch, 42);
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
        let PushOutcome::Ok { sha: sha_a2, .. } = sync(&m.a, card, "node-a") else { panic!("push") };
        match arrive_over(&m.b, &m.branch, 43) {
            ArrivalOutcome::Transferred { sha, stranded: None, .. } => assert_eq!(sha, sha_a2),
            other => panic!("behind, clean: a plain fast-forward, got {other:?}"),
        }
        // Rejected: B commits on the tip, A pushes first, B's push is a non-fast-forward.
        std::fs::write(m.a.join("src.txt"), "A wins the race\n").unwrap();
        assert!(sync(&m.a, card, "node-a").is_ok());
        std::fs::write(m.b.join("late.txt"), "late\n").unwrap();
        let out = sync(&m.b, card, "node-b");
        assert!(matches!(out, PushOutcome::Rejected { .. }), "{out:?}");
        // The next arrival on B strands the rejected commit and takes A's tip.
        assert!(matches!(arrive_over(&m.b, &m.branch, 44), ArrivalOutcome::Transferred { stranded: Some(_), .. }));
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
        assert_eq!(sync(&m.a, card, "n"), PushOutcome::NotPushable { reason: DETACHED_REASON });
        assert!(!has_unpushed_work(&m.a), "a detached tree has no branch of hers, so it never defers");
        assert!(matches!(arrive_over(&m.b, "no/such-branch", 1), ArrivalOutcome::NoRemoteBranch { .. }));
    }

    // what this catches (Cormac's condition on #4278 — the defect that would have stranded
    // her acts under a receipt saying "carried"): a staged SWE-bench copy's `origin` is
    // THIS NODE'S OWN local mirror (`clone_at`: `git clone --shared <cache>/swe/mirrors/…`).
    // A push there reports success into a repository no other node has ever heard of — the
    // blocker clears, she moves, and the arriving node's fetch finds nothing. So a
    // local-path origin is NOT a transfer target: never pushed, never committed into, and
    // the work still sitting there keeps `has_unpushed_work` TRUE so the move defers and
    // she is pinned. The https/ssh case (a continuum card's worktree) must stay pushable,
    // or this test would also pass with the transfer simply switched off.
    #[test]
    fn a_push_only_transfers_when_the_other_node_can_read_its_target() {
        for shared in [
            "https://github.com/CambrianTech/continuum.git",
            "http://gitlab.example.com/acme/widget",
            "ssh://git@github.com/CambrianTech/continuum.git",
            "git@github.com:CambrianTech/continuum.git",
            "git://example.com/repo.git",
        ] {
            assert!(is_transfer_target(shared), "{shared} is readable from another node");
        }
        for local in [
            "/Users/someone/.continuum/cache/swe/mirrors/django__django", // what clone_at clones from
            "/srv/repos/continuum.git",
            "../sibling-clone",
            "./mirror.git",
            "~/.continuum/cache/swe/mirrors/astropy__astropy",
            "file:///Users/someone/.continuum/cache/swe/mirrors/sympy__sympy",
            "C:\\repos\\continuum",
            "C:/repos/continuum",
            "mirrors/psf__requests",
            "",
            "   ",
        ] {
            assert!(!is_transfer_target(local), "{local:?} never leaves this machine");
        }

        // The live shape, built the way `swe_bench::clone_at` builds it: a bare local
        // mirror, then a `--shared` clone of it. Her act leaves an edit there.
        let m = two_machines();
        let mirror = m.tmp.path().join("mirror.git");
        let staged = m.tmp.path().join("staged-copy");
        git(m.tmp.path(), &["clone", "-q", "--bare", &m.a.to_string_lossy(), &mirror.to_string_lossy()]);
        git(m.tmp.path(), &["clone", "-q", "--shared", &mirror.to_string_lossy(), &staged.to_string_lossy()]);
        identity(&staged);
        // `-B`, not `-b`: `clone --bare` copies the source's HEAD symref, so a `--shared`
        // clone of the mirror already SITS on the card branch and `-b` would fail here.
        git(&staged, &["checkout", "-q", "-B", &m.branch]);
        assert_eq!(carry_of(&staged), Carry::LocalOrigin, "a --shared clone of a local mirror carries nothing");
        std::fs::write(staged.join("work.txt"), "her act\n").unwrap();

        let (card, persona) = (Uuid::new_v4(), Uuid::new_v4());
        let out = sync_after_act(&staged, card, "node-a");
        assert_eq!(out, PushOutcome::NotPushable { reason: LOCAL_ORIGIN_REASON }, "never a push into this node's own cache");
        assert_eq!(out.label(), "not_pushable");
        assert!(out.pins_the_mind(), "work that cannot be carried must pin her");
        assert_eq!(git(&staged, &["status", "--porcelain"]).trim(), "?? work.txt", "left exactly as her act left it");
        assert!(has_unpushed_work(&staged), "the work is real and no push can move it");
        note_acted_root(persona, staged.clone());
        let why = move_blocker_over(persona, None).expect("a move would strand her work");
        assert!(why.contains(&staged.display().to_string()), "{why}");
        // …and a fetch there cannot deliver another node's work either.
        assert!(matches!(arrive(&staged, &m.branch, 1), ArrivalOutcome::NotTransferable { .. }));

        // The positive control on the POLICY: a checkout whose origin is the grid-owned
        // remote (a continuum card's worktree — Cormac: "right as written") carries. Without
        // this the test would also pass with the transfer simply switched off.
        let card_worktree = m.tmp.path().join("card-worktree");
        git(m.tmp.path(), &["init", "-q", "-b", "main", &card_worktree.to_string_lossy()]);
        git(&card_worktree, &["remote", "add", "origin", "https://github.com/CambrianTech/continuum.git"]);
        assert_eq!(
            carry_of(&card_worktree),
            Carry::Via("https://github.com/CambrianTech/continuum.git".into()),
            "the continuum-card case must stay pushable"
        );
        // …and the MECHANISM, granted that carry, pushes and stops pinning her.
        std::fs::write(m.a.join("work.txt"), "her act\n").unwrap();
        let out = sync(&m.a, card, "node-a");
        assert!(out.is_ok() && !out.pins_the_mind(), "{out:?}");
        assert!(!has_unpushed_work(&m.a), "nothing left here to strand");
        // A checkout with no origin at all pins her the same way, for the other reason.
        let orphan = m.tmp.path().join("orphan");
        git(m.tmp.path(), &["init", "-q", "-b", "main", &orphan.to_string_lossy()]);
        identity(&orphan);
        // A root commit first: `git_init_if_needed` gives every citizen workspace one, and
        // an UNBORN branch has no branch at all (`rev-parse --abbrev-ref HEAD` fails), which
        // is the `Nothing` shape, not this one.
        std::fs::write(orphan.join("seed.txt"), "seed\n").unwrap();
        git(&orphan, &["add", "seed.txt"]);
        git(&orphan, &["commit", "-q", "-m", "workspace: initial state"]);
        std::fs::write(orphan.join("a.txt"), "x\n").unwrap();
        assert_eq!(carry_of(&orphan), Carry::NoOrigin);
        assert_eq!(sync_after_act(&orphan, card, "n"), PushOutcome::NotPushable { reason: NO_ORIGIN_REASON });
        assert!(has_unpushed_work(&orphan), "dirty work with nowhere to go pins her");
    }
}
