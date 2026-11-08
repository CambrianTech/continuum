//! `code/github/pr-view` — read a pull request as a teammate does: its state (open, closed,
//! merged), its conversation (the closing comment is the newest), its reviews and the inline review
//! threads. A PR an agent closed with a comment was a silent wall to its author (Aris's #5797 and
//! Benchy's #5791, closed 2026-11-07, both still "in Review" on her board); she reads it here.

use std::path::Path;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{run_gh_owned, workspace_root_for, GhRunError};
use crate::modules::code::CodeState;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubPrViewParams.ts")]
pub struct GithubPrViewParams {
    /// The pull request number. Omit for the newest PR of your current branch (open or not).
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub number: Option<u64>,
}

/// Where a pull request stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubPrState.ts")]
pub enum GithubPrState {
    Open,
    Closed,
    Merged,
}

/// One comment on the PR's conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubPrComment.ts")]
pub struct GithubPrComment {
    pub author: String,
    pub body: String,
    pub at: String,
}

/// One review: its verdict (APPROVED, CHANGES_REQUESTED, COMMENTED, ...) and its summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubPrReview.ts")]
pub struct GithubPrReview {
    pub author: String,
    pub state: String,
    pub body: String,
}

/// One inline review comment, on a line of a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubPrReviewComment.ts")]
pub struct GithubPrReviewComment {
    pub author: String,
    pub path: String,
    #[ts(type = "number | null")]
    pub line: Option<u64>,
    pub body: String,
    /// The comment this one answers, when it is a reply in a thread.
    #[ts(type = "number | null")]
    pub in_reply_to: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubPrViewResult.ts")]
pub struct GithubPrViewResult {
    #[ts(type = "number")]
    pub number: u64,
    pub url: String,
    pub title: String,
    pub state: GithubPrState,
    pub head: String,
    pub base: String,
    /// The conversation, oldest first: on a closed PR the newest is usually why it closed.
    pub comments: Vec<GithubPrComment>,
    pub reviews: Vec<GithubPrReview>,
    pub review_comments: Vec<GithubPrReviewComment>,
}

#[derive(Deserialize)]
struct Login {
    login: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ViewedComment {
    author: Option<Login>,
    body: String,
    created_at: String,
}

#[derive(Deserialize)]
struct ViewedReview {
    author: Option<Login>,
    state: String,
    body: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Viewed {
    number: u64,
    url: String,
    title: String,
    state: GithubPrState,
    head_ref_name: String,
    base_ref_name: String,
    comments: Vec<ViewedComment>,
    reviews: Vec<ViewedReview>,
}

#[derive(Deserialize)]
struct ViewedReviewComment {
    user: Option<Login>,
    path: String,
    line: Option<u64>,
    body: String,
    in_reply_to_id: Option<u64>,
}

/// A deleted account reads as GitHub shows it.
fn login(author: Option<Login>) -> String {
    author.map(|a| a.login).unwrap_or_else(|| "ghost".to_string()) // unwrap_or_else: GitHub's own name for a deleted account
}

/// `gh pr view --json` and the inline comments `gh api` lists, read into one view.
fn read_view(viewed: &str, inline: &str) -> Result<GithubPrViewResult, String> {
    let v: Viewed = serde_json::from_str(viewed.trim()).map_err(|e| format!("gh pr view did not answer a PR: {e}"))?;
    let inline: Vec<ViewedReviewComment> = serde_json::from_str(inline.trim()).map_err(|e| format!("gh api did not answer review comments: {e}"))?;
    Ok(GithubPrViewResult {
        number: v.number,
        url: v.url,
        title: v.title,
        state: v.state,
        head: v.head_ref_name,
        base: v.base_ref_name,
        comments: v.comments.into_iter().map(|c| GithubPrComment { author: login(c.author), body: c.body, at: c.created_at }).collect(),
        reviews: v.reviews.into_iter().map(|r| GithubPrReview { author: login(r.author), state: r.state, body: r.body }).collect(),
        review_comments: inline
            .into_iter()
            .map(|c| GithubPrReviewComment { author: login(c.user), path: c.path, line: c.line, body: c.body, in_reply_to: c.in_reply_to_id })
            .collect(),
    })
}

/// Read pull request `number` (or the current branch's) from `root`.
pub async fn view_pull_request(root: &Path, number: Option<u64>) -> Result<GithubPrViewResult, GhRunError> {
    let mut args = vec!["pr".to_string(), "view".to_string()];
    if let Some(number) = number {
        args.push(number.to_string());
    }
    args.extend(["--json".to_string(), "number,url,title,state,headRefName,baseRefName,comments,reviews".to_string()]);
    let viewed = run_gh_owned(root, &args, None).await?;
    let pr: u64 = serde_json::from_str::<serde_json::Value>(&viewed)
        .ok()
        .and_then(|v| v.get("number").and_then(serde_json::Value::as_u64))
        .ok_or_else(|| GhRunError::Incomplete { args: args.join(" "), code: Some(0), note: "no PR number in gh pr view's answer".into() })?;
    let inline_args = vec!["api".to_string(), "--paginate".to_string(), format!("repos/{{owner}}/{{repo}}/pulls/{pr}/comments")];
    let inline = run_gh_owned(root, &inline_args, None).await?;
    read_view(&viewed, &inline).map_err(|note| GhRunError::Incomplete { args: args.join(" "), code: Some(0), note })
}

crate::action_command! {
    /// Read a pull request (`gh pr view`): open, closed or merged, its conversation (on a closed PR
    /// the newest comment usually says why), its reviews and the inline review threads. Omit
    /// `number` for your current branch's PR.
    pub struct CodeGithubPrView { state: Arc<CodeState> }
    name: "code/github/pr-view",
    access: AiSafe,
    native: true,
    disclosure: OnDemand,
    params: GithubPrViewParams,
    output: GithubPrViewResult,
    run(this, ctx, p) => {
        let root = workspace_root_for(&this.state, ctx).await?;
        Ok(view_pull_request(&root, p.number).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a closed PR read as a wall: its state, the comment that closed it and the
    // inline threads must reach her whole, and a deleted reviewer must not drop the comment.
    #[test]
    fn a_closed_pull_request_reads_with_its_closing_comment_and_threads() {
        let viewed = r#"{"number":5797,"url":"https://github.com/CambrianTech/continuum/pull/5797","title":"ladder","state":"CLOSED",
            "headRefName":"e5c80c7f/ladder","baseRefName":"canary",
            "comments":[{"author":{"login":"joelteply"},"body":"SUPERSEDED by #5814","createdAt":"2026-11-07T15:27:00Z"}],
            "reviews":[{"author":null,"state":"CHANGES_REQUESTED","body":"diff does not do what the title says"}]}"#;
        let inline = r#"[{"user":{"login":"codex"},"path":"core/x.rs","line":12,"body":"why a constant?","in_reply_to_id":null},
            {"user":{"login":"aris"},"path":"core/x.rs","line":12,"body":"measured: see receipt","in_reply_to_id":1}]"#;
        let pr = read_view(viewed, inline).expect("a PR");
        assert_eq!((pr.number, pr.state, pr.head.as_str(), pr.base.as_str()), (5797, GithubPrState::Closed, "e5c80c7f/ladder", "canary"));
        assert_eq!(pr.comments.last().map(|c| c.body.as_str()), Some("SUPERSEDED by #5814"), "the closing comment");
        assert_eq!(pr.reviews[0].author, "ghost");
        assert_eq!(pr.review_comments.len(), 2);
        assert_eq!(pr.review_comments[1].in_reply_to, Some(1), "a reply keeps its thread");
        assert!(read_view("{}", "[]").is_err(), "an answer that is not a PR is said, never an empty view");
    }
}
