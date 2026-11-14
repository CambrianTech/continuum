//! `code/github/pr-merge` — land a pull request as a careful teammate does, through the operator's
//! brokered `gh` (usable by a citizen, never readable). The command applies the gate itself
//! ([`super::merge_gate`]): open and ready, every check green at the head, mergeable, and for a PR a
//! board card links, a recorded independent pass at that head. The merge pins the head it judged
//! (`--match-head-commit`), so a push after the read never lands unjudged. Residents could open PRs
//! and never land one (2026-11-14: Joel, "Persona can merge and review too").

use std::path::Path;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::card_verdict::{linked_cards, verdict_at_head, BoardsUnread, LinkedReview};
use super::merge_gate::{judge, refused, MergeRefusal};
use super::pr_head::{read_at_head, PrAddress, PrAtHead};
use super::{run_gh_owned, checkout_root_for};
use crate::modules::code::CodeState;
use crate::sdk_codegen::Ctx;

/// How the pull request's commits land on its base.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubMergeMethod.ts")]
pub enum GithubMergeMethod {
    #[default]
    Squash,
    Merge,
    Rebase,
}

impl GithubMergeMethod {
    fn flag(self) -> &'static str {
        match self {
            GithubMergeMethod::Squash => "--squash",
            GithubMergeMethod::Merge => "--merge",
            GithubMergeMethod::Rebase => "--rebase",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubPrMergeParams.ts")]
pub struct GithubPrMergeParams {
    /// `owner/name`; omit for your workspace's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repo: Option<String>,
    /// PR number.
    #[ts(type = "number")]
    pub number: u64,
    /// Default squash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub method: Option<GithubMergeMethod>,
}

impl GithubPrMergeParams {
    pub fn address(&self) -> PrAddress<'_> {
        PrAddress { repo: self.repo.as_deref(), number: self.number }
    }
}

/// The board card's passing review that let a linked pull request merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubMergeReview.ts")]
pub struct GithubMergeReview {
    #[ts(type = "string")]
    pub card: Uuid,
    #[ts(type = "string")]
    pub reviewer: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubPrMergeResult.ts")]
pub struct GithubPrMergeResult {
    #[ts(type = "number")]
    pub number: u64,
    pub url: String,
    pub method: GithubMergeMethod,
    /// The head commit that landed: the one its checks and its review covered.
    pub head: String,
    /// The linked card's passing review, when a card links the pull request.
    #[ts(optional)]
    pub review: Option<GithubMergeReview>,
}

/// What the caller's boards say about merging `pr` at its head.
async fn linked_review(state: &CodeState, ctx: &Ctx, pr: &PrAtHead) -> Result<LinkedReview, MergeRefusal> {
    let number = pr.number;
    let repo = pr.repo().ok_or_else(|| MergeRefusal::UnnamedRepository { number, url: pr.url.to_owned() })?;
    let reader = state.card_boards.get().ok_or(MergeRefusal::BoardsUnread { number, unread: BoardsUnread::NotBound })?;
    let boards = reader.boards(ctx).await.map_err(|unread| MergeRefusal::BoardsUnread { number, unread })?;
    Ok(verdict_at_head(&linked_cards(&boards, &repo, number), &pr.head))
}

/// Merge `pr`, judged at its head, from `root`: `gh pr merge`, pinned to that head.
async fn land(root: &Path, request: &GithubPrMergeParams, pr: &PrAtHead, method: GithubMergeMethod) -> Result<(), super::GhRunError> {
    let mut args = request.address().gh_args(&["pr", "merge"]);
    args.extend([method.flag().to_string(), "--match-head-commit".to_string(), pr.head.to_string()]);
    run_gh_owned(root, &args, None).await.map(drop)
}

crate::action_command! {
    /// Merge a PR: only when open, every check at its head green, mergeable, and a linked card's
    /// head passed another's review.
    pub struct CodeGithubPrMerge { state: Arc<CodeState> }
    name: "code/github/pr-merge",
    access: AiSafe,
    native: true, // her finishing verb, as Claude's; one help away (OnDemand)
    disclosure: OnDemand,
    params: GithubPrMergeParams,
    output: GithubPrMergeResult,
    run(this, ctx, p) => {
        let root = checkout_root_for(&this.state, ctx).await?;
        let pr = read_at_head(&root, &p.address()).await?;
        let review = linked_review(&this.state, ctx, &pr).await.map_err(|cause| refused(cause, &p))?;
        let merged_review = match &review {
            LinkedReview::Passed { card, reviewer } => Some(GithubMergeReview { card: *card, reviewer: *reviewer }),
            LinkedReview::NotLinked | LinkedReview::Gap { .. } => None,
        };
        judge(&pr, review).map_err(|cause| refused(cause, &p))?;
        let method = p.method.unwrap_or_default(); // unwrap_or_default: no method named is a squash, the declared default
        land(&root, &p, &pr, method).await?;
        Ok(GithubPrMergeResult { number: pr.number, url: pr.url, method, head: pr.head.to_string(), review: merged_review })
    }
}
