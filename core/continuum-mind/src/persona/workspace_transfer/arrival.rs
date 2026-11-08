//! Arrival: bringing a card checkout to its branch on this node (card 73eefbbb), never off its
//! creation anchor (card f76005e8). Split from `workspace_transfer` whole; the act-end sync and
//! the git facts stay there.

use super::*;

/// What an arrival did with the node's checkout of the card's branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrivalOutcome {
    /// `origin` has no such branch yet — the first claim anywhere; the local checkout stands.
    NoRemoteBranch { branch: String },
    /// The fetch could not reach the remote; the local checkout stands, said loudly.
    Unreachable { branch: String, error: String },
    /// Already at the fetched commit with a clean tree: nothing moved.
    Current { branch: String, sha: String },
    /// Same branch already contains the fetched history; keep unpublished commits
    /// and edits in place. This is local continuity, not a successful push/transfer.
    LocalWorkPreserved { branch: String, sha: String, remote_sha: String, local_commits: u64, dirty: bool },
    /// The branch is checked out at the fetched commit. `stranded` names the ref that
    /// holds what this node had (its own commits and any dirty edit) when it diverged.
    Transferred { branch: String, sha: String, from_node: Option<String>, stranded: Option<String> },
    /// A git step failed after the fetch; `stage` names it. The checkout is whatever
    /// that step left — the caller's probe carries it.
    Failed { branch: String, stage: &'static str, error: String },
    /// This checkout's `origin` is not a remote another node writes to (a `--shared`
    /// clone of a local mirror), so no fetch here can deliver another node's work.
    NotTransferable { branch: String, reason: &'static str },
    /// `origin`'s branch does not contain this checkout's creation anchor: it predates the
    /// allocation (a branch left by an earlier one), so adopting it would put her HEAD off the
    /// base her work is measured from. The checkout stands untouched; `remote_sha` is what
    /// origin holds, `anchor` the allocation's base.
    RemotePredatesAllocation { branch: String, remote_sha: String, anchor: String },
    /// The checkout's HEAD had left its creation anchor (an earlier arrival adopted a branch
    /// that predates the allocation). HEAD and any edits are kept whole under `stranded`, and
    /// the branch is back at `anchor`. `own_commits` counts the stranded commits no remote
    /// carries: hers to bring back, never replayed for her.
    Realigned { branch: String, anchor: String, stranded: String, own_commits: u64 },
    /// Whether origin's branch contains the checkout's creation anchor could not be proven (a
    /// shallow clone, or a commit missing locally), so nothing was adopted: the checkout stands.
    AncestryUnknown { branch: String, reason: String },
}

/// What she reads when arrival put her checkout of `branch` back on its base `anchor`, keeping what
/// it held at `stranded`.
pub fn realigned_notice(branch: &str, anchor: &str, stranded: &str, own_commits: u64) -> String {
    format!(
        "Your checkout of {branch} had left its base {anchor} (an earlier arrival adopted an older branch), \
         so it is back on that base. Everything it held is kept at {stranded}; {own_commits} commit(s) \
         there exist only in your checkout. Bring back what you meant with `git cherry-pick` or \
         `git merge {stranded}`."
    )
}

/// What she reads when her checkout of `branch` could not be checked against its base.
pub fn ancestry_unknown_notice(branch: &str, reason: &str) -> String {
    format!(
        "Your checkout of {branch} could not be checked against the base its work is measured from \
         ({reason}), so nothing in it was moved or adopted. Submitting from it would measure your patch \
         from a base this checkout may not contain; report this as a substrate fault (card f76005e8) \
         rather than resetting it yourself."
    )
}

impl ArrivalOutcome {
    /// This outcome when it changed something she must be told, else `None`.
    pub fn notable(self) -> Option<Self> {
        self.notice().is_some().then_some(self)
    }

    /// What she reads about this arrival, in her own terms; `None` when it is routine.
    pub fn notice(&self) -> Option<String> {
        match self {
            ArrivalOutcome::Realigned { branch, anchor, stranded, own_commits } => {
                Some(realigned_notice(branch, anchor, stranded, *own_commits))
            }
            ArrivalOutcome::RemotePredatesAllocation { branch, remote_sha, anchor } => Some(format!(
                "origin's {branch} is at {remote_sha}, which does not contain your checkout's base {anchor}, so it \
                 was not adopted. Publishing your branch will replace that older one (a non-fast-forward push): \
                 that is your explicit choice, never automatic."
            )),
            ArrivalOutcome::AncestryUnknown { branch, reason } => Some(ancestry_unknown_notice(branch, reason)),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ArrivalOutcome::NoRemoteBranch { .. } => "no_remote_branch",
            ArrivalOutcome::Unreachable { .. } => "unreachable",
            ArrivalOutcome::Current { .. } => "current",
            ArrivalOutcome::LocalWorkPreserved { .. } => "local_work_preserved",
            ArrivalOutcome::Transferred { .. } => "transferred",
            ArrivalOutcome::Failed { .. } => "failed",
            ArrivalOutcome::NotTransferable { .. } => "not_transferable",
            ArrivalOutcome::RemotePredatesAllocation { .. } => "remote_predates_allocation",
            ArrivalOutcome::Realigned { .. } => "realigned",
            ArrivalOutcome::AncestryUnknown { .. } => "ancestry_unknown",
        }
    }
}

/// Bring `root` to `origin/<branch>`'s tip before her first turn on this node: fetch,
/// then retain same-branch local work if it already contains the fetched history.
/// Otherwise check `branch` out AT the fetched commit. Diverged local commits and
/// dirty edits are committed whole (no hooks) and kept under [`STRANDED_REF_PREFIX`]`<branch>-<now_ms>`; the remote wins the
/// branch and the tree. Synchronous git; [`arrive_for`] is the bounded, probed form.
pub async fn arrive(root: &Path, branch: &str, now_ms: u64) -> ArrivalOutcome {
    if let Some(realigned) = realign_to_anchor(root, branch, now_ms).await {
        return realigned;
    }
    // The mirror of the push guard (and the same policy/mechanism split): fetching from a
    // target no other node can push to cannot deliver another node's work, so say so
    // rather than report a transfer that only ever re-read this machine's own cache.
    if let Ok(url) = run_git(root, &["remote", "get-url", "origin"]).await {
        if !is_transfer_target(&url) {
            return ArrivalOutcome::NotTransferable { branch: branch.to_string(), reason: LOCAL_ORIGIN_REASON };
        }
    }
    arrive_over(root, branch, now_ms).await
}

/// A checkout whose HEAD no longer descends from its creation anchor is put back on it, keeping
/// everything it had: the recovery for a checkout an earlier arrival moved onto a branch that
/// predates its allocation (Kimi on the Windows node, 2026-11-08, Codex 52754e2a). `None` when
/// there is no anchor or HEAD descends from it (nothing to do).
pub(super) async fn realign_to_anchor(root: &Path, branch: &str, now_ms: u64) -> Option<ArrivalOutcome> {
    let branch_s = branch.to_string();
    let anchor = run_git(root, &["rev-parse", "--verify", "-q", airc_lib::work_worktree::CREATION_BASE_REF]).await.ok()?;
    let anchor = anchor.trim().to_string();
    // Only a proven departure is realigned: a shallow clone or a missing commit can make a valid
    // descendant look like one, and realigning it would strand good work (Codex on #5873 review).
    match proven_ancestry(root, &anchor, "HEAD").await {
        Ok(true) => return None,
        Ok(false) => {}
        // Unproven is never realigned: the checkout stands and the reason is said.
        Err(reason) => return Some(ArrivalOutcome::AncestryUnknown { branch: branch_s, reason }),
    }
    let dirty = match is_dirty(root).await {
        Ok(d) => d,
        Err(error) => return Some(ArrivalOutcome::Failed { branch: branch_s, stage: "status", error }),
    };
    let keep = if dirty {
        commit_tree_no_hooks(root, &format!("stranded: {branch} off its creation anchor"), false, None).await
    } else {
        run_git(root, &["rev-parse", "HEAD"]).await.map(|s| s.trim().to_string())
    };
    let keep = match keep {
        Ok(keep) => keep,
        Err(error) => return Some(ArrivalOutcome::Failed { branch: branch_s, stage: "strand_commit", error }),
    };
    let stranded = format!("{STRANDED_REF_PREFIX}{branch}-{now_ms}");
    if let Err(error) = run_git(root, &["update-ref", &stranded, &keep]).await {
        return Some(ArrivalOutcome::Failed { branch: branch_s, stage: "strand_ref", error });
    }
    let own_commits = match run_git(root, &["rev-list", "--count", &keep, "--not", "--remotes"]).await
        .and_then(|n| n.trim().parse::<u64>().map_err(|e| format!("rev-list --count printed {n:?}: {e}")))
    {
        Ok(n) => n,
        Err(error) => return Some(ArrivalOutcome::Failed { branch: branch_s, stage: "rev_list", error }),
    };
    if let Err(error) = run_git(root, &["checkout", "-q", "-B", branch, &anchor]).await {
        return Some(ArrivalOutcome::Failed { branch: branch_s, stage: "checkout", error });
    }
    if let Err(error) = run_git(root, &["reset", "-q", "--hard", &anchor]).await {
        return Some(ArrivalOutcome::Failed { branch: branch_s, stage: "reset", error });
    }
    Some(ArrivalOutcome::Realigned { branch: branch_s, anchor, stranded, own_commits })
}

/// git's own answer to "is `ancestor` an ancestor of `of`": exit 0 yes, exit 1 no, anything else
/// (a missing object, a deadline, the runner) is no answer, said.
pub(super) async fn is_ancestor(root: &Path, ancestor: &str, of: &str) -> Result<bool, String> {
    use crate::code::git_bridge::{run_git_input, GitEnd};
    match run_git_input(root, &["merge-base", "--is-ancestor", ancestor, of], &[], None).await {
        Ok(_) => Ok(true),
        Err(error) if error.end == GitEnd::Refused && error.code == Some(1) => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

/// Whether `ancestor` is an ancestor of `of`, only on proof. A yes is proof in any clone; a no is
/// proof only with complete history, since in a shallow clone it may mean the history is missing
/// ([`complete_history_if_departed`] completes it first, when that matters). `Err` says why it
/// could not be proven.
pub(super) async fn proven_ancestry(root: &Path, ancestor: &str, of: &str) -> Result<bool, String> {
    if is_ancestor(root, ancestor, of).await? {
        return Ok(true);
    }
    match run_git(root, &["rev-parse", "--is-shallow-repository"]).await?.trim() {
        "false" => Ok(false),
        _ => Err("the checkout's history is shallow, so its ancestry cannot be proven".into()),
    }
}

/// When a shallow checkout's HEAD answers "not a descendant" of its creation anchor, complete its
/// history (airc's owner, under airc's measured network bound) so the answer can be proven, before
/// any arrival acts on it (Codex e302c1d0). Nothing else in a shallow clone pays for a full
/// history. A failure is left for the arrival to say as the unknown it is.
/// What completes a shallow checkout's history: airc's owner in production, a stand-in in tests.
pub(super) type HistoryCompleter = fn(&Path) -> Result<(), String>;

/// The checkout lock as held: shared by the arrival's scope and any blocking work it starts.
type HeldCheckout = std::sync::Arc<tokio::sync::OwnedMutexGuard<()>>;

pub(super) async fn complete_history_if_departed(root: &Path, held: HeldCheckout, complete: HistoryCompleter) {
    let Ok(anchor) = run_git(root, &["rev-parse", "--verify", "-q", airc_lib::work_worktree::CREATION_BASE_REF]).await else {
        return;
    };
    let shallow = run_git(root, &["rev-parse", "--is-shallow-repository"]).await.is_ok_and(|s| s.trim() == "true");
    if !shallow || is_ancestor(root, anchor.trim(), "HEAD").await != Ok(false) {
        return;
    }
    let path = root.to_path_buf();
    // The lock goes INTO the blocking task: an arrival dropped mid-fetch keeps the checkout locked
    // until airc's git has ended, so no sync starts beside it (Codex b7194c2c). The scope's
    // `holding` covers only the owned continuum-process gits, not this blocking one.
    let completed = tokio::task::spawn_blocking(move || {
        let _held = held;
        complete(&path)
    })
    .await;
    continuum_base::probe!(
        class = "workspace.history_completed",
        root = %root.display(),
        outcome = ?completed,
        "a shallow checkout suspected off its creation anchor had its history completed to prove it"
    );
}

/// [`arrive`] past the transfer-target policy — the fetch / checkout / strand mechanism.
pub(super) async fn arrive_over(root: &Path, branch: &str, now_ms: u64) -> ArrivalOutcome {
    let branch_s = branch.to_string();
    if !root.join(".git").exists() {
        return ArrivalOutcome::Failed { branch: branch_s, stage: "checkout", error: NOT_A_REPO_REASON.into() };
    }
    if let Err(error) = run_git(root, &["fetch", "--no-tags", "origin", branch]).await {
        return if is_missing_remote_ref(&error) {
            ArrivalOutcome::NoRemoteBranch { branch: branch_s }
        } else {
            ArrivalOutcome::Unreachable { branch: branch_s, error }
        };
    }
    let fetched = match run_git(root, &["rev-parse", "FETCH_HEAD"]).await {
        Ok(s) => s.trim().to_string(),
        Err(error) => return ArrivalOutcome::Failed { branch: branch_s, stage: "fetch_head", error },
    };
    // Kimi on the Windows node, 2026-11-08: a fresh allocation at canary bcbd4aae fetched her old
    // card branch (e5fe473d, off another line), the remote won, and her anchor was no longer an
    // ancestor of HEAD, so work/submit refused. A remote that does not contain the anchor is never
    // adopted. A checkout with no anchor (before airc recorded one) keeps the rule below.
    if let Ok(anchor) = run_git(root, &["rev-parse", "--verify", "-q", airc_lib::work_worktree::CREATION_BASE_REF]).await {
        let anchor = anchor.trim().to_string();
        match proven_ancestry(root, &anchor, &fetched).await {
            Ok(true) => {}
            Ok(false) => return ArrivalOutcome::RemotePredatesAllocation { branch: branch_s, remote_sha: fetched, anchor },
            // Unproven is never adopted (adopting on a guess is how Kimi's anchor left HEAD): the
            // checkout stands, which loses nothing, and the reason is said.
            Err(reason) => return ArrivalOutcome::AncestryUnknown { branch: branch_s, reason },
        }
    }
    let head = head_sha(root).await;
    let dirty = match is_dirty(root).await {
        Ok(d) => d,
        Err(error) => return ArrivalOutcome::Failed { branch: branch_s, stage: "status", error },
    };
    if head.as_deref() == Some(fetched.as_str()) && !dirty && current_branch(root).await.as_deref() == Some(branch) {
        return ArrivalOutcome::Current { branch: branch_s, sha: fetched };
    }
    // Count both sides against immutable tips. A local successor is not a
    // divergence: re-claiming must not reset reviewed work to an older origin.
    let (remote_only, local_only) = match head.as_deref() {
        Some(sha) => {
            let counts = history_counts(root, &fetched, sha).await;
            match counts {
                Ok(counts) => counts,
                Err(error) => return ArrivalOutcome::Failed { branch: branch_s, stage: "rev_list", error },
            }
        }
        None => (0, 0),
    };
    if remote_only == 0 && (local_only > 0 || dirty) && current_branch(root).await.as_deref() == Some(branch) {
        if let Some(sha) = &head {
            return ArrivalOutcome::LocalWorkPreserved {
                branch: branch_s, sha: sha.clone(), remote_sha: fetched, local_commits: local_only, dirty,
            };
        }
    }
    let mut stranded = None;
    if dirty || local_only > 0 {
        let reference = format!("{STRANDED_REF_PREFIX}{branch}-{now_ms}");
        let keep = if dirty {
            match commit_tree_no_hooks(root, &format!("stranded: local state of {branch} before transfer"), false, None).await {
                Ok(sha) => sha,
                Err(error) => return ArrivalOutcome::Failed { branch: branch_s, stage: "strand_commit", error },
            }
        } else {
            head.clone().unwrap_or_default() // unwrap_or_default: local_only > 0 implies a HEAD
        };
        if let Err(error) = run_git(root, &["update-ref", &reference, &keep]).await {
            return ArrivalOutcome::Failed { branch: branch_s, stage: "strand_ref", error };
        }
        stranded = Some(reference);
    }
    // The remote wins: the branch at the fetched tip, the tree exactly that commit.
    if let Err(error) = run_git(root, &["checkout", "-q", "-B", branch]).await {
        return ArrivalOutcome::Failed { branch: branch_s, stage: "checkout", error };
    }
    if let Err(error) = run_git(root, &["reset", "-q", "--hard", &fetched]).await {
        return ArrivalOutcome::Failed { branch: branch_s, stage: "reset", error };
    }
    // Best effort: name the upstream so `@{upstream}` reads on this node too.
    let _ = run_git(root, &["branch", "--set-upstream-to", &format!("origin/{branch}")]).await;
    let from_node = run_git(root, &["log", "-1", "--format=%s", &fetched]).await
        .ok()
        .and_then(|s| node_of_wip_subject(&s));
    ArrivalOutcome::Transferred { branch: branch_s, sha: fetched, from_node, stranded }
}

/// [`arrive`] under the checkout's lock ([`CHECKOUT_SYNC`], the one an act-end sync holds), since
/// an arrival commits, moves refs and resets the tree (Codex on f76005e8): first any history a
/// shallow checkout needs to prove where it stands, then the arrival within [`ARRIVE_BOUND`].
/// Every git it starts keeps the lock until that git has ended.
pub(super) async fn arrive_locked(root: &Path, branch: &str, now_ms: u64) -> ArrivalOutcome {
    arrive_locked_with(root, branch, now_ms, airc_lib::work_worktree::complete_card_history).await
}

/// [`arrive_locked`] with the history completer named (a test's stand-in).
pub(super) async fn arrive_locked_with(root: &Path, branch: &str, now_ms: u64, complete: HistoryCompleter) -> ArrivalOutcome {
    let lock = CHECKOUT_SYNC.entry(root.to_path_buf()).or_default().clone();
    let Ok(one_at_a_time) = tokio::time::timeout(ARRIVE_BOUND, lock.lock_owned()).await else {
        return ArrivalOutcome::Failed {
            branch: branch.to_string(),
            stage: "lock",
            error: format!("another sync held this checkout for {} s", ARRIVE_BOUND.as_secs()),
        };
    };
    let held: HeldCheckout = std::sync::Arc::new(one_at_a_time);
    continuum_process::holding(held.clone(), async {
        complete_history_if_departed(root, held, complete).await;
        crate::code::git_bridge::within(tokio::time::Instant::now() + ARRIVE_BOUND, arrive(root, branch, now_ms)).await
    })
    .await
}

/// [`arrive_locked`] under an admission to the claim's hands: the lock's holding scope needs one
/// (`continuum_process::holding`), as the act-end sync's does, so every git the arrival starts
/// keeps the checkout until its tree has ended. Hands that are closing, or a card with no claim to
/// admit to, are a named failure; nothing runs.
async fn admitted_arrival(hands: Option<crate::identity::hands_context::HandsContext>, root: &Path, branch: &str, now_ms: u64) -> ArrivalOutcome {
    let Some(hands) = hands else {
        return ArrivalOutcome::Failed { branch: branch.to_string(), stage: "admission", error: "the card has no claim, so there are no hands to arrive in".into() };
    };
    match crate::identity::hands_gate::admit(hands) {
        Ok(lease) => crate::identity::hands_gate::admitted(Some(std::sync::Arc::new(lease)), arrive_locked(root, branch, now_ms)).await,
        Err(error) => ArrivalOutcome::Failed { branch: branch.to_string(), stage: "admission", error },
    }
}

/// The staging-path form of [`arrive`]: within
/// [`ARRIVE_BOUND`], receipted as `workspace.transfer` with card, branch, sha, from and
/// to node. The claim is authoritative and the checkout is convenience: no outcome here
/// undoes a claim, every outcome is said.
pub async fn arrive_for(root: PathBuf, branch: String, card: Uuid, hands: Option<crate::identity::hands_context::HandsContext>) -> ArrivalOutcome {
    let now_ms = continuum_base::time::EpochMs::now().millis(); // pre-1970 reads 0: a pre-epoch clock stamps 0; the ref is still unique per branch per arrival in practice
    let started = std::time::Instant::now();
    let outcome = match admitted_arrival(hands, &root, &branch, now_ms).await {
        // a step the deadline ended or never started: said as the timeout it was
        ArrivalOutcome::Unreachable { error, .. } | ArrivalOutcome::Failed { error, .. } if crate::code::git_bridge::out_of_time(&error) => ArrivalOutcome::Failed {
            branch: branch.clone(),
            stage: "timeout",
            error: format!("fetch + checkout did not finish within {} s: {error}", ARRIVE_BOUND.as_secs()),
        },
        other => other,
    };
    let (sha, from_node, stranded, error) = match &outcome {
        ArrivalOutcome::Transferred { sha, from_node, stranded, .. } => {
            (sha.as_str(), from_node.clone().unwrap_or_default(), stranded.clone().unwrap_or_default(), String::new()) // unwrap_or_default: probe labels only
        }
        ArrivalOutcome::Current { sha, .. } | ArrivalOutcome::LocalWorkPreserved { sha, .. } =>
            (sha.as_str(), String::new(), String::new(), String::new()),
        ArrivalOutcome::Unreachable { error, .. } | ArrivalOutcome::Failed { error, .. } => ("", String::new(), String::new(), error.clone()),
        ArrivalOutcome::NotTransferable { reason, .. } => ("", String::new(), String::new(), (*reason).to_string()),
        ArrivalOutcome::NoRemoteBranch { .. } | ArrivalOutcome::RemotePredatesAllocation { .. } => ("", String::new(), String::new(), String::new()),
        ArrivalOutcome::AncestryUnknown { reason, .. } => ("", String::new(), String::new(), reason.clone()),
        ArrivalOutcome::Realigned { anchor, stranded, .. } => (anchor.as_str(), String::new(), stranded.clone(), String::new()),
    };
    let (remote_sha, local_commits, dirty) = match &outcome {
        ArrivalOutcome::LocalWorkPreserved { remote_sha, local_commits, dirty, .. } =>
            (remote_sha.as_str(), *local_commits, *dirty),
        ArrivalOutcome::RemotePredatesAllocation { remote_sha, .. } => (remote_sha.as_str(), 0, false),
        ArrivalOutcome::Realigned { own_commits, .. } => ("", *own_commits, false),
        _ => ("", 0, false), // Other outcomes do not carry a local-preservation receipt.
    };
    let to_node = crate::capacity::gossip::this_process_origin().to_string();
    continuum_base::probe!(
        class = "workspace.transfer",
        card = %card,
        branch = %branch,
        root = %root.display(),
        sha = %sha,
        from_node = %from_node,
        to_node = %to_node,
        stranded = %stranded,
        remote_sha = %remote_sha,
        local_commits = local_commits,
        dirty = dirty,
        outcome = %outcome.label(),
        error = %error,
        ms = started.elapsed().as_millis() as u64,
        "card workspace arrival reconciled; outcome distinguishes transferred and preserved local work"
    );
    outcome
}
