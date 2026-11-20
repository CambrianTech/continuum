//! The merge gate a careful teammate applies, enforced by the command, never asked of her by a
//! prompt: the pull request is open and ready, every check at its head has passed, GitHub can
//! merge it as it stands, and when a board card links it, the card's latest submission ends at that
//! head and passed an independent review ([`super::card_verdict`]). Each refusal is typed and names
//! her next move.

use uuid::Uuid;

use super::card_verdict::{BoardsUnread, LinkedReview, ReviewGap};
use super::pr_checks::{CodeGithubPrChecks, GithubPrChecksParams};
use super::pr_head::{GithubCheckState, MergeState, Mergeable, PrAtHead};
use super::pr_merge::{CodeGithubPrMerge, GithubPrMergeParams};
use super::pr_view::GithubPrState;
use crate::commands::code::git::merge::{CodeGitMerge, GitMergeParams};
use crate::sdk_codegen::{Category, CommandError, NextCall, NextMoves, NoMove, Refusal, RefusalCause, VerbRef};
use airc_work::GitObjectId;

const PR_CHECKS: VerbRef<CodeGithubPrChecks> = VerbRef::in_hands();
const PR_MERGE: VerbRef<CodeGithubPrMerge> = VerbRef::in_hands();
const GIT_MERGE: VerbRef<CodeGitMerge> = VerbRef::in_hands();

/// Why a pull request does not merge now.
#[derive(Debug, thiserror::Error)]
pub enum MergeRefusal {
    #[error("pull request #{number} is {state:?}, not open, so there is nothing to merge")]
    NotOpen { number: u64, state: GithubPrState },
    #[error("pull request #{number} is a draft; it merges once its author marks it ready for review")]
    Draft { number: u64 },
    #[error("pull request #{number} has failing checks at its head {head}: {}", .failed.join(", "))]
    ChecksFailing { number: u64, head: GitObjectId, failed: Vec<String> },
    #[error("pull request #{number} has checks still running at its head {head}: {}", .pending.join(", "))]
    ChecksPending { number: u64, head: GitObjectId, pending: Vec<String> },
    #[error("pull request #{number} conflicts with {base}: merge {base} into its branch, resolve the conflicts, and push")]
    Conflicts { number: u64, base: String },
    #[error("pull request #{number} is behind {base}, and the repository requires it up to date: merge {base} into its branch and push")]
    Behind { number: u64, base: String },
    #[error("GitHub has not computed yet whether pull request #{number} can merge; it computes it when first asked")]
    MergeabilityUnknown { number: u64 },
    #[error("pull request #{number} links card {card}, and its review does not cover the head: {gap}")]
    Review { number: u64, card: Uuid, gap: ReviewGap },
    #[error("pull request #{number} could not be checked against the board cards that may link it")]
    BoardsUnread {
        number: u64,
        #[source]
        unread: BoardsUnread,
    },
    #[error("pull request #{number}'s URL {url} names no repository to find its cards by")]
    UnnamedRepository { number: u64, url: String },
}

impl RefusalCause for MergeRefusal {
    fn category(&self) -> Category {
        Category::Invalid
    }
}

/// The checks at the head in `state`, by name.
fn named(pr: &PrAtHead, state: GithubCheckState) -> Vec<String> {
    pr.checks.iter().filter(|rolled| rolled.check.state == state).map(|rolled| rolled.check.name.to_owned()).collect()
}

/// Whether `pr`, read at its head, merges now given what the boards say (`review`). Pure: every
/// fact was read once, from the same view the merge will pin its head to.
pub fn judge(pr: &PrAtHead, review: LinkedReview) -> Result<(), MergeRefusal> {
    let number = pr.number;
    if pr.state != GithubPrState::Open {
        return Err(MergeRefusal::NotOpen { number, state: pr.state });
    }
    if pr.merge_state == MergeState::Draft {
        return Err(MergeRefusal::Draft { number });
    }
    let failed = named(pr, GithubCheckState::Failed);
    if !failed.is_empty() {
        return Err(MergeRefusal::ChecksFailing { number, head: pr.head.clone(), failed });
    }
    let pending = named(pr, GithubCheckState::Pending);
    if !pending.is_empty() {
        return Err(MergeRefusal::ChecksPending { number, head: pr.head.clone(), pending });
    }
    let base = || pr.base.clone();
    match (pr.mergeable, pr.merge_state) {
        (Mergeable::Conflicting, _) | (_, MergeState::Dirty) => return Err(MergeRefusal::Conflicts { number, base: base() }),
        (Mergeable::Unknown, _) | (_, MergeState::Unknown) => return Err(MergeRefusal::MergeabilityUnknown { number }),
        (_, MergeState::Behind) => return Err(MergeRefusal::Behind { number, base: base() }),
        _ => {}
    }
    match review {
        LinkedReview::Gap { card, gap } => Err(MergeRefusal::Review { number, card, gap }),
        LinkedReview::NotLinked | LinkedReview::Passed { .. } => Ok(()),
    }
}

/// Merging the base into the PR's branch, when the PR is the workspace's own (no other `repo`).
fn merge_base_in(base: &str, request: &GithubPrMergeParams) -> Result<NextMoves, crate::sdk_codegen::NextCallArgs> {
    let from = continuum_base::non_empty_text::NonEmptyText::try_from(format!("origin/{base}"));
    match (request.repo.as_deref(), from) {
        (None, Ok(from)) => NextCall::new(GIT_MERGE, &GitMergeParams::Start { from }, "on the pull request's branch: brings the base in (fetched first), naming any conflict to resolve; push after").map(NextMoves::call),
        _ => Ok(NextMoves::None(NoMove::AwaitsAnother)),
    }
}

/// `cause` as the refusal she reads, with the move that makes progress.
pub fn refused(cause: MergeRefusal, request: &GithubPrMergeParams) -> CommandError {
    let checks = || {
        NextCall::new(PR_CHECKS, &GithubPrChecksParams { repo: request.repo.clone(), number: request.number }, "each check at the head, failures first with their annotations and failing-step log")
            .map(NextMoves::call)
    };
    let next = match &cause {
        MergeRefusal::NotOpen { .. } => Ok(NextMoves::None(NoMove::NothingMoreExists)),
        MergeRefusal::Draft { .. } | MergeRefusal::Review { .. } | MergeRefusal::BoardsUnread { .. } | MergeRefusal::UnnamedRepository { .. } => {
            Ok(NextMoves::None(NoMove::AwaitsAnother))
        }
        MergeRefusal::ChecksFailing { .. } | MergeRefusal::ChecksPending { .. } => checks(),
        MergeRefusal::Conflicts { base, .. } | MergeRefusal::Behind { base, .. } => merge_base_in(base, request),
        MergeRefusal::MergeabilityUnknown { .. } => NextCall::new(PR_MERGE, request, "GitHub has computed mergeability by the next ask").map(NextMoves::call),
    };
    match next {
        Ok(next) => Refusal::new(cause, next).into(),
        Err(unwritable) => CommandError::Internal(unwritable.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::code::github::pr_head::{GithubCheck, RolledCheck};

    fn head() -> GitObjectId {
        GitObjectId::new("c".repeat(40)).expect("a commit id")
    }

    fn check(name: &str, state: GithubCheckState) -> RolledCheck {
        RolledCheck { check: GithubCheck { name: name.into(), workflow: None, state, conclusion: None, link: None, failure: None }, job: None }
    }

    /// An open, clean, all-green pull request into `main`.
    fn green() -> PrAtHead {
        PrAtHead {
            number: 7,
            url: "https://github.com/acme/widgets/pull/7".into(),
            state: GithubPrState::Open,
            mergeable: Mergeable::Mergeable,
            merge_state: MergeState::Clean,
            base: "main".into(),
            head: head(),
            checks: vec![check("build", GithubCheckState::Passed), check("docs", GithubCheckState::Skipped)],
        }
    }

    fn request(repo: Option<&str>) -> GithubPrMergeParams {
        GithubPrMergeParams { repo: repo.map(str::to_string), number: 7, method: None }
    }

    // what this catches: a merge the careful agent would not make going through: not open, a
    // draft, a red or running check, a conflict, a stale branch, an uncomputed mergeability, or a
    // linked card whose review does not cover the head. A green, clean, reviewed (or unlinked)
    // PR passes; a PR with no CI at all is judged by the rest.
    #[test]
    fn the_gate_refuses_each_unsafe_merge_and_passes_a_green_one() {
        assert!(judge(&green(), LinkedReview::NotLinked).is_ok());
        assert!(judge(&green(), LinkedReview::Passed { card: Uuid::from_u128(1), reviewer: Uuid::from_u128(2) }).is_ok());
        assert!(judge(&PrAtHead { checks: Vec::new(), ..green() }, LinkedReview::NotLinked).is_ok(), "no CI configured is not a red check");

        let refused = |pr: PrAtHead, review: LinkedReview| judge(&pr, review).expect_err("refused");
        assert!(matches!(refused(PrAtHead { state: GithubPrState::Merged, ..green() }, LinkedReview::NotLinked), MergeRefusal::NotOpen { .. }));
        assert!(matches!(refused(PrAtHead { merge_state: MergeState::Draft, ..green() }, LinkedReview::NotLinked), MergeRefusal::Draft { .. }));
        let red = PrAtHead { checks: vec![check("build", GithubCheckState::Passed), check("test", GithubCheckState::Failed), check("lint", GithubCheckState::Pending)], ..green() };
        match refused(red, LinkedReview::NotLinked) {
            MergeRefusal::ChecksFailing { failed, .. } => assert_eq!(failed, ["test"], "a red check is named, ahead of a running one"),
            other => panic!("expected failing checks, got {other:?}"),
        }
        assert!(matches!(refused(PrAtHead { checks: vec![check("lint", GithubCheckState::Pending)], ..green() }, LinkedReview::NotLinked), MergeRefusal::ChecksPending { .. }));
        assert!(matches!(refused(PrAtHead { mergeable: Mergeable::Conflicting, ..green() }, LinkedReview::NotLinked), MergeRefusal::Conflicts { .. }));
        assert!(matches!(refused(PrAtHead { merge_state: MergeState::Behind, ..green() }, LinkedReview::NotLinked), MergeRefusal::Behind { .. }));
        assert!(matches!(refused(PrAtHead { mergeable: Mergeable::Unknown, ..green() }, LinkedReview::NotLinked), MergeRefusal::MergeabilityUnknown { .. }));
        let gap = LinkedReview::Gap { card: Uuid::from_u128(1), gap: ReviewGap::NoIndependentReview };
        assert!(matches!(refused(green(), gap), MergeRefusal::Review { .. }), "CI green is not enough for a linked card");
    }

    // what this catches: a refusal that leaves her to guess (ONE-ACT 6b): red checks name the read
    // of their logs, a conflict names merging the base in, a review gap says the move is
    // another's, and a closed PR says nothing recovers it.
    #[test]
    fn each_refusal_names_her_next_move() {
        let next_of = |cause: MergeRefusal, request: &GithubPrMergeParams| match refused(cause, request) {
            CommandError::Refused(refusal) => refusal.next().clone(),
            other => panic!("expected a typed refusal, got {other:?}"),
        };
        let first = |next: NextMoves| next.calls().next().map(|call| call.verb().canonical());
        let here = request(None);
        let red = MergeRefusal::ChecksFailing { number: 7, head: head(), failed: vec!["test".into()] };
        assert_eq!(first(next_of(red, &here)), Some("code/github/pr-checks"));
        let conflict = || MergeRefusal::Conflicts { number: 7, base: "main".into() };
        let merge_in = next_of(conflict(), &here);
        let call = merge_in.calls().next().expect("a call");
        assert_eq!(call.verb().canonical(), "code/git/merge");
        match serde_json::from_value::<GitMergeParams>(serde_json::Value::Object(call.args().to_owned())) {
            Ok(GitMergeParams::Start { from }) => assert_eq!(from.as_str(), "origin/main"),
            other => panic!("expected a merge of the base, got {other:?}"),
        }
        assert_eq!(next_of(conflict(), &request(Some("acme/widgets"))), NextMoves::None(NoMove::AwaitsAnother), "another repository's branch is not in her workspace");
        let gap = MergeRefusal::Review { number: 7, card: Uuid::from_u128(1), gap: ReviewGap::NoIndependentReview };
        assert_eq!(next_of(gap, &here), NextMoves::None(NoMove::AwaitsAnother));
        assert_eq!(next_of(MergeRefusal::NotOpen { number: 7, state: GithubPrState::Closed }, &here), NextMoves::None(NoMove::NothingMoreExists));
    }
}
