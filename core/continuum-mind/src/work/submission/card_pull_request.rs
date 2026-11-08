//! A submitted card's pull request: pushed at the submitted head, found or opened, and linked to the
//! card. The ONE path a resident's work reaches the merger (the finishing chain's break 1,
//! 2026-11-08): before it, `work/submit` published a submission and stopped, `code/github/pr-create`
//! opened PRs the card never linked, and the merger, which reads only linked PRs, never saw them.
//! The merge gate compares the reviewed submission's head with the PR's (`HeadMoved`), so the branch
//! is pushed to exactly the submitted head, never past it.

use std::path::Path;

use airc_work::{BranchName, GitObjectId, PullRequestRef, WorkCard};
use continuum_hands::commands::code::github::branch_pull_request::{NewPullRequest, PullRequestHost};
use continuum_hands::commands::code::github::GhRunError;

/// Why the card's pull request was not ensured. Each names what fixes it.
#[derive(Debug, thiserror::Error)]
pub enum CardPullRequestError {
    #[error("the checkout is on no branch (detached HEAD); check out the card's branch, then work/submit again")]
    Detached,
    #[error("the branch name `{0}` is not one a pull request can carry")]
    Branch(String),
    #[error("pushing the submitted head {head} to {branch} failed (the branch on origin must end at the submitted head; fetch and rebase onto it, then work/submit again): {why}")]
    Push { branch: String, head: String, why: String },
    #[error("GitHub: {0}")]
    GitHub(#[from] GhRunError),
    #[error("GitHub opened {url} for {branch}, but lists no open pull request for it; retry work/submit")]
    NotListed { branch: String, url: String },
    #[error("pull request #{number} is open, but linking it to the card failed: {why}")]
    Link { number: u64, why: String },
    #[error("{0}")]
    Base(String),
    #[error("this node holds no checkout of the card to push from; work/submit without the typed fields derives the submission from your checkout and opens its pull request")]
    NoCheckout,
    #[error("the submission records no head commit, so no pull request can carry exactly it; submit again from your checkout")]
    HeadUnrecorded,
}

/// The card's pull request, as `work/submit` left it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnsuredPullRequest {
    pub pull_request: PullRequestRef,
    pub url: String,
    /// Opened by this submit (false: it was already open for the branch).
    pub opened: bool,
    /// Linked to the card by this submit (false: the card already linked it).
    pub linked: bool,
}

/// What the PR carries: her card, her submission, the patch it is.
pub struct PullRequestText<'a> {
    pub title: &'a str,
    pub body: String,
}

/// Ensure `card`'s pull request for the work at `head` in `checkout`: push the branch to exactly
/// `head`, find its open PR on `host` (open one when there is none, into `base` when the card's
/// repository names an integration branch), and link it to the card in `room`. Idempotent: a retry
/// finds the same PR and links nothing twice.
pub async fn ensure_card_pull_request(
    airc: &airc_lib::Airc,
    room: &airc_lib::Room,
    card: &WorkCard,
    checkout: &Path,
    head: &GitObjectId,
    host: &dyn PullRequestHost,
    text: PullRequestText<'_>,
) -> Result<EnsuredPullRequest, CardPullRequestError> {
    let git = |args: Vec<String>| async move {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        crate::code::git_bridge::run_git(checkout, &args).await
    };
    let branch = git(vec!["symbolic-ref".into(), "--short".into(), "-q".into(), "HEAD".into()])
        .await
        .map(|b| b.trim().to_string())
        .ok()
        .filter(|b| !b.is_empty())
        .ok_or(CardPullRequestError::Detached)?;
    let head_branch = BranchName::new(branch.clone()).map_err(|_| CardPullRequestError::Branch(branch.clone()))?;
    // Exactly the submitted head, on her own branch. A rebase rewrites her branch, so the push
    // replaces it, but only with a lease: it is refused if origin's branch moved past what her
    // checkout last saw (someone else pushed), never a blind force.
    git(vec!["push".into(), "--force-with-lease".into(), "origin".into(), format!("{head}:refs/heads/{branch}")])
        .await
        .map_err(|why| CardPullRequestError::Push { branch: branch.clone(), head: head.to_string(), why })?;
    let base = super::latest_submission::submission_integration_branch(checkout, Some(&card.repo), &branch)
        .map_err(|e| CardPullRequestError::Base(e.to_string()))?;
    let (found, opened) = match host.open_for(&branch).await? {
        Some(open) => (open, false),
        None => {
            let url = host
                .create(NewPullRequest { title: text.title, body: &text.body, base: base.as_deref(), head: Some(&branch), draft: false, repo: None })
                .await?;
            let open = host.open_for(&branch).await?.ok_or_else(|| CardPullRequestError::NotListed { branch: branch.clone(), url })?;
            (open, true)
        }
    };
    let pull_request = PullRequestRef {
        repo: card.repo.clone(),
        number: found.number,
        head: head_branch,
        base: BranchName::new(found.base.clone()).map_err(|_| CardPullRequestError::Branch(found.base.clone()))?,
    };
    let linked = card.pull_request.as_ref() != Some(&pull_request);
    if linked {
        airc.link_card_pull_request_in(room, airc_lib::LinkCardPullRequest { card_id: card.card_id, pull_request: pull_request.clone() })
            .await
            .map_err(|e| CardPullRequestError::Link { number: found.number, why: e.to_string() })?;
    }
    continuum_base::probe!(
        class = "work.submit.pull_request",
        card = %card.card_id,
        number = found.number,
        head = %head,
        opened,
        linked,
        "the submission's pull request: pushed at the submitted head, found or opened, linked to the card"
    );
    Ok(EnsuredPullRequest { pull_request, url: found.url, opened, linked })
}

/// A submission as its pull request carries it: where it was made (her checkout on this node, when
/// there is one), the head it is, its id and patch.
pub struct Submitted<'a> {
    pub checkout: Option<&'a Path>,
    pub head: &'a airc_work::SubmissionHead,
    pub submission: uuid::Uuid,
    pub patch_hash: String,
}

/// Land a submission `work/submit` just published (or found already published) through its pull
/// request: `None` for a card its oracle grades (`graded`), whose grade is its finish; otherwise
/// [`ensure_card_pull_request`] through the brokered `gh`, the PR's text naming the card, the
/// submission and its patch, attributed to her.
pub async fn land_submission(
    airc: &airc_lib::Airc,
    room: &airc_lib::Room,
    card: &WorkCard,
    submitted: Submitted<'_>,
    graded: bool,
    ctx: &crate::sdk_codegen::Ctx,
) -> Option<Result<EnsuredPullRequest, CardPullRequestError>> {
    if graded {
        return None;
    }
    let Submitted { checkout, head, submission, patch_hash } = submitted;
    let Some(checkout) = checkout else {
        return Some(Err(CardPullRequestError::NoCheckout));
    };
    let airc_work::SubmissionHead::At(head) = head else {
        return Some(Err(CardPullRequestError::HeadUnrecorded));
    };
    let host = continuum_hands::commands::code::github::branch_pull_request::GhHost { root: checkout, repo: card.repo.as_str() };
    let detail = card.body.as_deref().map(|b| format!("\n\n{b}")).unwrap_or_default(); // unwrap_or_default: a card with no body adds nothing
    let body = continuum_hands::commands::code::github::attributed(
        format!(
            "Card {}: {}\n\nSubmission {submission} (patch {patch_hash}) at {head}, on the room's board for review. \
             The merger lands it after a passing review of this exact head.{detail}",
            card.card_id, card.title
        ),
        ctx,
    );
    Some(ensure_card_pull_request(airc, room, card, checkout, head, &host, PullRequestText { title: &card.title, body }).await)
}

/// Her guidance, with what became of the pull request said after it: its URL, or why it is not up
/// and that a retry of the same patch ensures it.
pub fn with_pull_request(next: String, landing: Option<&Result<EnsuredPullRequest, CardPullRequestError>>) -> String {
    match landing {
        None => next,
        Some(Ok(ensured)) => format!(
            "{next} Pull request {} ({}) is {} and linked to the card.",
            ensured.pull_request.number,
            ensured.url,
            if ensured.opened { "opened" } else { "open" }
        ),
        Some(Err(error)) => format!(
            "{next} Its pull request is NOT up: {error}. Once that is fixed, work/submit the same patch again: it finds or opens the pull request and links it."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_hands::commands::code::github::branch_pull_request::InMemoryPullRequestHost;

    async fn git(root: &Path, args: &[&str]) -> String {
        let her = crate::code::git_actor::GitActor::citizen(uuid::Uuid::from_u128(0xa115), Some("Aris"));
        crate::code::git_bridge::run_git_as(root, &her, args).await.unwrap_or_else(|e| panic!("git {args:?}: {e}"))
    }

    // what this catches (the finishing chain's break 1): a submission that reached no merger. Her
    // branch had no PR; ensuring it pushes the branch to exactly the submitted head, opens ONE
    // PR, links it to the card (the card enters Review with it, the merger's input), and says so;
    // a retry of the same submission opens nothing new and links nothing twice.
    #[tokio::test]
    async fn a_submit_on_a_branch_with_no_pr_opens_and_links_exactly_one() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        let _home = crate::paths::test_home::HomeGuard::set(tmp.path()).await;
        let origin = tmp.path().join("origin.git");
        let checkout = tmp.path().join("checkout");
        git(tmp.path(), &["init", "-q", "--bare", "-b", "main", &origin.to_string_lossy()]).await;
        git(tmp.path(), &["init", "-q", "-b", "main", &checkout.to_string_lossy()]).await;
        std::fs::write(checkout.join("f.txt"), "base\n")?;
        git(&checkout, &["add", "f.txt"]).await;
        git(&checkout, &["commit", "-q", "-m", "base"]).await;
        git(&checkout, &["remote", "add", "origin", &origin.to_string_lossy()]).await;
        git(&checkout, &["push", "-q", "origin", "main"]).await;
        git(&checkout, &["checkout", "-q", "-b", "a1150000/her-fix"]).await;
        std::fs::write(checkout.join("f.txt"), "fixed\n")?;
        git(&checkout, &["commit", "-q", "-am", "her fix"]).await;
        let head = GitObjectId::new(git(&checkout, &["rev-parse", "HEAD"]).await.trim())?;

        let airc = airc_lib::Airc::open_with_wire_root_for_test(tmp.path(), tmp.path()).await?;
        let room = airc.join("card-pull-request").await?;
        let repo = airc_work::RepoId::new("acme/project")?;
        let card_id = airc.create_work_card(airc_lib::CreateWorkCard::new(repo, "her fix", airc_lib::Priority::P1)).await?;
        airc.claim_work_card(airc_lib::ClaimWorkCard { card_id, ttl_ms: 600_000 }).await?;
        let card = airc.work_board_in(&room).await?.card(card_id).cloned().ok_or("card")?;
        let hub = InMemoryPullRequestHost::default();
        let text = || PullRequestText { title: "her fix", body: "card, submission, patch".into() };

        let ensured = ensure_card_pull_request(&airc, &room, &card, &checkout, &head, &hub, text()).await?;
        assert!(ensured.opened && ensured.linked, "{ensured:?}");
        assert_eq!(ensured.pull_request.number, 101);
        assert_eq!(ensured.pull_request.head.as_str(), "a1150000/her-fix");
        let pushed = git(&checkout, &["ls-remote", &origin.to_string_lossy(), "refs/heads/a1150000/her-fix"]).await;
        assert!(pushed.starts_with(head.as_str()), "origin's branch is the submitted head: {pushed}");
        let board = airc.work_board_in(&room).await?;
        let linked = board.card(card_id).ok_or("card")?;
        assert_eq!(linked.pull_request.as_ref(), Some(&ensured.pull_request), "the card links it: the merger's input");
        assert_eq!(linked.state, airc_work::CardState::Review);
        let said = with_pull_request("Submitted.".into(), Some(&Ok(ensured.clone())));
        assert!(said.contains("Pull request 101") && said.contains(&ensured.url), "her guidance reports it: {said}");

        let again = ensure_card_pull_request(&airc, &room, linked, &checkout, &head, &hub, text()).await?;
        assert!(!again.opened && !again.linked, "a retry finds the same PR: {again:?}");
        assert_eq!(hub.opened(), 1, "exactly one PR for the branch");
        Ok(())
    }
}
