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
//! 1. [`sync_after_act`] — at the end of every turn bound to a card claim's hands
//!    (`persona_workspace::end_card_turn`): a work-in-progress commit of whatever the act left, then
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
//!    her first turn there. Same-branch local successors and dirty work are preserved
//!    when the fetched tip adds no commits. A genuinely diverged local checkout
//!    is saved whole under `refs/continuum/stranded/<branch>-<ts>` and the remote
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
use crate::persona::git_actor::GitActor;

/// Where a diverged local checkout is kept when the remote wins an arrival.
pub const STRANDED_REF_PREFIX: &str = "refs/continuum/stranded/";
/// Bound on the git subprocesses one act-end sync may take (a push to a slow remote): ONE
/// absolute deadline across its lock and every git call, and the git it ends is ended, not
/// abandoned. UNMEASURED: it arrived with #4278 with no receipt; the one push timing on record
/// (IntelMac, 2026-09-22T14:55:43Z) took 116 ms. Card c1bdc29b derives it from measured syncs.
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
const DIRTY_CARRY_REASON: &str = "dirty checkout: only the resident's turn may commit it";

/// The turn may save its work; a background carry may only publish existing commits.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CommitPolicy {
    TurnEnd,
    Never,
}

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
pub async fn carry_of(root: &Path) -> Carry {
    if !root.join(".git").exists() {
        return Carry::Nothing(NOT_A_REPO_REASON);
    }
    // Also the UNBORN branch of a just-`git init`ed workspace: `rev-parse --abbrev-ref
    // HEAD` fails there, and with no commit there is nothing to carry yet either.
    if current_branch(root).await.is_none() {
        return Carry::Nothing(DETACHED_REASON);
    }
    match run_git(root, &["remote", "get-url", "origin"]).await {
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
    /// Whether the remote took the push is UNKNOWN: the deadline ended it mid-run, or it exited
    /// 0 with output that wasn't whole or readable. Never read as pushed or as not pushed; the
    /// next sync reconciles with the remote before it pushes again (Codex root on bfa87dd5,
    /// Codex reviewer on #5037).
    PushInterrupted { branch: String, error: String },
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
            PushOutcome::PushInterrupted { .. } => "push_interrupted",
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
            | PushOutcome::CommitFailed { error, .. }
            | PushOutcome::PushInterrupted { error, .. } => error.as_str(),
            PushOutcome::NotPushable { reason } => reason,
            PushOutcome::Ok { .. } | PushOutcome::TimedOut => "",
        }
    }
}

// ── the git facts, each one question ────────────────────────────────────────────────

/// The branch HEAD is on, or `None` for a detached HEAD / not a repo.
pub async fn current_branch(root: &Path) -> Option<String> {
    let name = run_git(root, &["rev-parse", "--abbrev-ref", "HEAD"]).await.ok()?;
    let name = name.trim();
    (!name.is_empty() && name != "HEAD").then(|| name.to_string())
}

async fn head_sha(root: &Path) -> Option<String> {
    run_git(root, &["rev-parse", "--verify", "--quiet", "HEAD"]).await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Does the working tree differ from HEAD (tracked edits or untracked files)?
pub(crate) async fn is_dirty(root: &Path) -> Result<bool, String> {
    run_git(root, &["status", "--porcelain", "--untracked-files=all"]).await.map(|s| !s.trim().is_empty())
}

/// Count each side of two immutable tips. Arrival and branch publication use the
/// same ancestry question; reachability from an unrelated branch is not delivery.
async fn history_counts(root: &Path, remote: &str, local: &str) -> Result<(u64, u64), String> {
    let output = run_git(root, &["rev-list", "--left-right", "--count", &format!("{remote}...{local}")]).await?;
    let counts = output.split_whitespace().map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("rev-list counts unreadable ({error}): {output:?}"))?;
    match counts.as_slice() {
        [remote, local] => Ok((*remote, *local)),
        _ => Err(format!("rev-list expected two counts: {output:?}")),
    }
}

/// The receiving node fetches THIS branch, not whichever origin ref contains HEAD.
/// A missing branch needs publication even when its initial commit exists on main.
async fn needs_branch_publication(root: &Path) -> Result<bool, String> {
    let branch = current_branch(root).await.ok_or_else(|| "cannot read current branch".to_string())?;
    let local = run_git(root, &["rev-parse", "--verify", "HEAD"]).await?;
    let remote_ref = format!("refs/remotes/origin/{branch}");
    // A successful empty listing means absent. Do not turn a failed git read into
    // absence; placement must retain its fail-closed error behavior.
    let refs = run_git(root, &["for-each-ref", "--format=%(objectname) %(refname)", &remote_ref]).await?;
    let remote = refs.lines().filter_map(|line| line.split_once(' '))
        .find_map(|(sha, name)| (name == remote_ref).then_some(sha));
    match remote {
        Some(remote) => history_counts(root, remote, local.trim()).await.map(|(_, local_only)| local_only > 0),
        None => Ok(true),
    }
}

/// Is there work here that `origin` does not hold — so a move would strand it?
///
/// A carryable checkout ([`Carry::Via`]) answers on the push's own terms: a dirty tree,
/// or work not published to its exact origin branch. A checkout that CANNOT carry but can hold work
/// (a local-path origin — every `--shared` clone of this node's cache — or no origin)
/// answers on what sits there, and a yes PINS her: moving would leave it behind with
/// nothing able to fetch it. Only a checkout with no branch of hers at all answers `false`
/// outright. A checkout git cannot read answers `true` (fail closed).
pub async fn has_unpushed_work(root: &Path) -> bool {
    match carry_of(root).await {
        Carry::Nothing(_) => false,
        // No origin refs exist to count commits against, so the dirty tree IS the work:
        // nothing ever commits into a checkout that cannot carry (see `sync_after_act`).
        Carry::NoOrigin => is_dirty(root).await.unwrap_or(true), // unwrap_or: unreadable = assume work, never move on a guess
        Carry::Via(_) | Carry::LocalOrigin => match (is_dirty(root).await, needs_branch_publication(root).await) {
            (Ok(dirty), Ok(unpublished)) => dirty || unpublished,
            _ => true,
        },
    }
}

/// The message a work-in-progress commit carries: which card, and which node made it —
/// the "from" half of the transfer receipt on arrival, read back by
/// [`node_of_wip_subject`]. Plain text in the commit, so plain git carries it.
///
/// The acting citizen is the commit's author AND committer ([`GitActor`], applied by the git
/// runner), so her name on the commits she made needs no trailer (card e5e72485 had to add it
/// by hand when she was neither).
pub fn wip_message(card: Uuid, node: &str) -> String {
    format!("wip({}): continuum act on node {node}", airc_core::shown_id::shown_form(card))
}

/// PURE: the node a [`wip_message`] names, or `None` for any other subject.
pub fn node_of_wip_subject(subject: &str) -> Option<String> {
    subject
        .lines()
        .next()
        .unwrap_or_default() // unwrap_or_default: an empty message names no node
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
async fn commit_tree_no_hooks(
    root: &Path,
    message: &str,
    advance_branch: bool,
    actor: &GitActor,
) -> Result<String, String> {
    run_git(root, &["add", "-A"]).await?;
    let tree = run_git(root, &["write-tree"]).await?.trim().to_string();
    let parent = head_sha(root).await;
    // The acting citizen is author AND committer: the runner applies `actor` (e5e72485).
    use crate::code::git_bridge::run_git_input;
    let commit = match &parent {
        Some(p) => run_git_input(root, Some(actor), &["commit-tree", &tree, "-p", p, "-m", message], &[], None).await?,
        None => run_git_input(root, Some(actor), &["commit-tree", &tree, "-m", message], &[], None).await?,
    }
    .trim()
    .to_string();
    if advance_branch {
        let branch = current_branch(root).await.ok_or("cannot advance a detached HEAD")?;
        let reference = format!("refs/heads/{branch}");
        match &parent {
            Some(p) => run_git(root, &["update-ref", "-m", message, &reference, &commit, p]).await?,
            None => run_git(root, &["update-ref", "-m", message, &reference, &commit]).await?,
        };
    }
    Ok(commit)
}

/// PURE: did the remote refuse a push, read from `git push --porcelain`'s ref lines (git's machine
/// format, one per ref, flag first): a `!` ref was rejected by the remote that answered (a
/// non-fast-forward, a declined hook, a protected branch). A push with no rejected ref line never
/// had the remote's verdict, so it was not delivered: the conservative reading, and both are
/// receipted verbatim.
pub fn classify_push_porcelain(porcelain: &str) -> PushFailure {
    if porcelain.lines().any(|line| line.starts_with('!')) {
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

/// `git ls-remote --exit-code` answers "no such ref" with exit 2.
const LS_REMOTE_NO_MATCH: i32 = 2;

/// Is `branch` absent from `origin`? Asked after a fetch of it failed, so the fetch's failure is
/// told apart by git's exit code, never by its stderr: `Ok(true)` only when the remote answered
/// and holds no such branch.
async fn remote_branch_absent(root: &Path, branch: &str) -> Result<bool, crate::code::git_bridge::GitError> {
    let reference = format!("refs/heads/{branch}");
    match crate::code::git_bridge::run_git_input(root, None, &["ls-remote", "--exit-code", "origin", &reference], &[], None).await {
        Ok(_) => Ok(false),
        Err(error) if error.code == Some(LS_REMOTE_NO_MATCH) => Ok(true),
        Err(error) => Err(error),
    }
}

// ── seam 1: the act-end sync ────────────────────────────────────────────────────────

/// Ensure a work-in-progress commit of whatever the act left in `root` and push the
/// current branch to `origin`, each git owned and within any deadline its caller set; the bounded, probed form the
/// turn path calls is [`sync_after_act_for`]. `node` is stamped into the WIP commit's
/// subject so the receiving node can say where the work came from.
pub async fn sync_after_act(root: &Path, card: Uuid, node: &str, actor: &GitActor) -> PushOutcome {
    sync_after_act_over(root, card, node, carry_of(root).await, actor, CommitPolicy::TurnEnd).await
}

/// [`sync_after_act`] with the carry decision supplied — the POLICY (can this checkout
/// reach another node at all) separated from the MECHANISM (commit, then push). Split
/// because the two need different proofs: the policy is a pure function over origin URLs,
/// tested against the real shapes including `clone_at`'s mirror path; the mechanism needs
/// two real clones and a real remote, and an offline test's remote is necessarily a local
/// path — the very thing the policy refuses. Injecting the decision lets each be tested as
/// what it is, instead of a fixture quietly proving neither.
async fn sync_after_act_over(root: &Path, card: Uuid, node: &str, carry: Carry, actor: &GitActor, commit: CommitPolicy) -> PushOutcome {
    // Asked BEFORE anything is written: a checkout that cannot carry is left exactly as
    // her act left it. No WIP commit either — a commit whose push can never happen only
    // makes the work harder to see (and a benchmark copy's commits are its grade).
    match carry {
        Carry::Via(_) => {}
        Carry::Nothing(reason) => return PushOutcome::NotPushable { reason },
        Carry::LocalOrigin => return PushOutcome::NotPushable { reason: LOCAL_ORIGIN_REASON },
        Carry::NoOrigin => return PushOutcome::NotPushable { reason: NO_ORIGIN_REASON },
    }
    let Some(branch) = current_branch(root).await else {
        return PushOutcome::NotPushable { reason: DETACHED_REASON };
    };
    let mut committed = false;
    match is_dirty(root).await {
        Ok(true) if commit == CommitPolicy::Never => {
            return PushOutcome::NotPushable { reason: DIRTY_CARRY_REASON };
        }
        Ok(true) => {
            if let Err(error) = commit_tree_no_hooks(root, &wip_message(card, node), true, actor).await {
                return PushOutcome::CommitFailed { branch, error };
            }
            committed = true;
        }
        Ok(false) => {}
        Err(error) => return PushOutcome::CommitFailed { branch, error },
    }
    let sha = head_sha(root).await.unwrap_or_default(); // unwrap_or_default: an unborn branch has no sha; the push below says so
    if REMOTE_UNKNOWN.contains(root) {
        // the last push was ended mid-run, so the remote may already hold it: read the
        // remote's branch first, so publication is decided against what it holds, and a
        // push that landed isn't repeated as if it hadn't
        let spec = format!("+refs/heads/{branch}:refs/remotes/origin/{branch}");
        if let Err(error) = crate::code::git_bridge::run_git_input(root, None, &["fetch", "--no-tags", "origin", &spec], &[], None).await {
            match remote_branch_absent(root, &branch).await {
                Ok(true) => {} // it never landed: the branch isn't there
                Ok(false) => return push_failure(branch, error),
                Err(asked) => return push_failure(branch, asked),
            }
        }
        REMOTE_UNKNOWN.remove(root);
    }
    let unpublished = match needs_branch_publication(root).await {
        Ok(n) => n,
        Err(error) => return PushOutcome::CommitFailed { branch, error },
    };
    if !unpublished {
        return PushOutcome::Ok { branch, sha, committed, pushed: false };
    }
    // unknown from BEFORE the push: a push cut off by its deadline or by a cancel (which
    // returns nothing here) leaves the remote unknown, and only a push that completed says what
    // the remote holds (Codex reviewer on #5037)
    REMOTE_UNKNOWN.insert(root.to_path_buf());
    let pushed = crate::code::git_bridge::run_git_input(root, None, &["push", "--porcelain", "-u", "origin", &branch], &[], None).await;
    // only a push that answered, or one that never ran, settles what the remote holds. One git
    // refused may still have landed (the remote can take the ref and the connection fail before
    // git hears so), and one cut off or unreadable may too: the remote stays unknown, and the
    // next sync's one fetch reads it (Codex reviewer on #5037)
    match pushed {
        Ok(_) => {
            REMOTE_UNKNOWN.remove(root);
            PushOutcome::Ok { branch, sha, committed, pushed: true }
        }
        Err(error) => match error.end {
            crate::code::git_bridge::GitEnd::Unknown | crate::code::git_bridge::GitEnd::Interrupted => PushOutcome::PushInterrupted { branch, error: error.into() },
            crate::code::git_bridge::GitEnd::NeverRan => {
                REMOTE_UNKNOWN.remove(root);
                push_failure(branch, error)
            }
            crate::code::git_bridge::GitEnd::Refused => push_failure(branch, error),
        },
    }
}

/// A push-step failure, named: out of time, refused by the remote, or never delivered.
fn push_failure(branch: String, error: crate::code::git_bridge::GitError) -> PushOutcome {
    if error.out_of_time() {
        return PushOutcome::TimedOut;
    }
    let failure = classify_push_porcelain(&error.stdout);
    let error = error.to_string();
    match failure {
        PushFailure::Rejected => PushOutcome::Rejected { branch, error },
        PushFailure::Unreachable => PushOutcome::Unreachable { branch, error },
    }
}

/// Checkouts whose last push was ended mid-run, so the remote's state is unknown until the
/// next sync reads it. A reboot forgets the set; the next sync then decides publication
/// against the tracking ref as before (a re-push of the same tip is a no-op at the remote).
static REMOTE_UNKNOWN: LazyLock<dashmap::DashSet<PathBuf>> = LazyLock::new(dashmap::DashSet::new);

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
pub async fn persona_has_unpushed_work(persona: Uuid) -> bool {
    move_blocker_over(persona, None).await.is_some()
}

/// Why `persona` must not change seat right now, or `None`: a turn in flight with her
/// hands rooted at a card (a move is between turns), or unpushed work in a checkout she
/// acted in (a move is after the last push landed). Names the root so the probe says
/// where the work sits. `acting_root` is passed in so the rule is testable without the
/// process-wide hands registry.
pub async fn move_blocker(persona: Uuid) -> Option<String> {
    move_blocker_over(persona, crate::cognition::persona_workspace::acting_root_of(persona)).await
}

async fn move_blocker_over(persona: Uuid, acting_root: Option<PathBuf>) -> Option<String> {
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
        if has_unpushed_work(&root).await {
            return Some(format!("unpushed work at {}", root.display()));
        }
    }
    None
}

/// The per-checkout lock [`sync_after_act_for`] holds around its git work: one entry per
/// checkout with open claim hands, dropped when those hands close ([`forget_checkout`]).
static CHECKOUT_SYNC: std::sync::LazyLock<dashmap::DashMap<PathBuf, std::sync::Arc<tokio::sync::Mutex<()>>>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);

/// Drop `root`'s sync lock once the claim hands standing in it are released, but ONLY when
/// no sync holds it: a sync holding the lock keeps its clone, and removing the entry under it
/// would let a new claim's sync run beside it (Astra on #5026).
pub fn forget_checkout(root: &std::path::Path) {
    CHECKOUT_SYNC.remove_if(root, |_, lock| std::sync::Arc::strong_count(lock) == 1);
}

/// The turn-path form of [`sync_after_act`]: within [`SYNC_BOUND`], probed as
/// `workspace.push`, and recorded so a later move can ask. Never fails the turn — every shape
/// is an outcome. It runs ONLY under an admission to the hands that acted (`lease`, a
/// parameter, so an unleased sync can't be written): each git it runs keeps that admission
/// until the git's tree has ended, so a release never removes the checkout under it (Astra
/// and Codex root on #5026), and the deadline ends the git instead of abandoning it.
pub async fn sync_after_act_for(lease: std::sync::Arc<crate::identity::hands_gate::HandsLease>, persona: Uuid, persona_name: &str, root: PathBuf, card: Uuid) -> PushOutcome {
    crate::identity::hands_gate::admitted(Some(lease), sync_admitted(persona, persona_name, root, card, CommitPolicy::TurnEnd)).await
}

async fn sync_admitted(persona: Uuid, persona_name: &str, root: PathBuf, card: Uuid, commit: CommitPolicy) -> PushOutcome {
    let node = crate::capacity::gossip::this_process_origin().to_string();
    let started = std::time::Instant::now();
    let outcome = sync_by(tokio::time::Instant::now() + SYNC_BOUND, &root, async {
        sync_after_act_over(&root, card, &node, carry_of(&root).await, &GitActor::citizen(persona, Some(persona_name)), commit).await
    }).await;
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
        | PushOutcome::CommitFailed { branch, .. }
        | PushOutcome::PushInterrupted { branch, .. } => (branch.as_str(), "", false, false),
        PushOutcome::NotPushable { .. } | PushOutcome::TimedOut => ("", "", false, false),
    };
    continuum_base::probe!(
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

/// `sync` (a [`sync_after_act`] of `root`) by ONE absolute `deadline`: waiting for the
/// checkout's lock spends it too, and every git in it gets only what is left (Codex root on
/// bfa87dd5).
async fn sync_by(deadline: tokio::time::Instant, root: &Path, sync: impl std::future::Future<Output = PushOutcome>) -> PushOutcome {
    let lock = CHECKOUT_SYNC.entry(root.to_path_buf()).or_default().clone();
    let outcome = crate::code::git_bridge::within(deadline, async {
        // ONE sync per checkout at a time: two turns of one card claim can end together (card
        // fc9e9fd7). Every git the sync starts keeps the lock until ITS tree has ended, so a
        // cancelled sync can't let the next one in beside a git still being ended (Codex
        // reviewer on #5037).
        let Ok(one_at_a_time) = tokio::time::timeout_at(deadline, lock.lock_owned()).await else {
            return PushOutcome::TimedOut;
        };
        continuum_process::holding(std::sync::Arc::new(one_at_a_time), sync).await
    })
    .await;
    // past the deadline, a step's failure was the deadline's (a git it ended or never ran), so
    // it is said as the timeout it was; only a landed push and an interrupted one keep their
    // own outcome, since those say something about the remote
    match outcome {
        o @ (PushOutcome::Ok { .. } | PushOutcome::PushInterrupted { .. }) => o,
        _ if tokio::time::Instant::now() >= deadline => PushOutcome::TimedOut,
        o => o,
    }
}

/// Carry COMMITTED work in `root` to origin, under `lease` (an admission to the claim's
/// hands), by the same bounded, per-checkout-locked sync the turn uses. An initially
/// dirty tree is left alone (`None`): uncommitted work belongs to the turn writing it, whose own end
/// commits and carries it. The early checks only avoid unnecessary syncs; Never is
/// enforced inside the locked sync, so becoming dirty while waiting returns a typed
/// NotPushable outcome and cannot commit.
pub async fn carry_committed(lease: std::sync::Arc<crate::identity::hands_gate::HandsLease>, persona: Uuid, persona_name: &str, root: PathBuf, card: Uuid) -> Option<PushOutcome> {
    match is_dirty(&root).await {
        Ok(false) => {}
        _ => return None,
    }
    if !has_unpushed_work(&root).await {
        return None;
    }
    Some(crate::identity::hands_gate::admitted(Some(lease), sync_admitted(persona, persona_name, root, card, CommitPolicy::Never)).await)
}

/// [`move_blocker`] within [`BLOCKER_BOUND`]; unanswered in time = a blocker (fail
/// closed).
pub async fn move_blocker_bounded(persona: Uuid) -> Option<String> {
    let deadline = tokio::time::Instant::now() + BLOCKER_BOUND;
    let blocker = crate::code::git_bridge::within(deadline, move_blocker(persona)).await;
    if tokio::time::Instant::now() >= deadline {
        // a git question the deadline ended reads as work (fail closed); say why instead
        return Some(format!("workspace question unanswered within {} s", BLOCKER_BOUND.as_secs()));
    }
    blocker
}

// ── seam 3: arrival ─────────────────────────────────────────────────────────────────

mod arrival;
pub use arrival::*;
#[cfg(test)]
use arrival::{arrive_locked_with, arrive_over, realign_to_anchor};

#[cfg(test)]
mod tests;
