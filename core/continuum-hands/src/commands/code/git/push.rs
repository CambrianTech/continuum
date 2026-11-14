//! `code/git/push` — push the caller's workspace branch to its remote.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::workspace_root_for;
use crate::code::git_bridge;
use crate::modules::code::CodeState;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/code/GitPushParams.ts"
)]
pub struct GitPushParams {
    /// Remote name (e.g. `origin`). Omit for git's default.
    #[serde(default)]
    pub remote: Option<String>,
    /// Branch to push. Omit for git's default (the current branch).
    #[serde(default)]
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/code/GitPushResult.ts"
)]
pub struct GitPushResult {
    /// Raw `git push` output.
    pub output: String,
    /// The pull request for the pushed branch. A push is not a PR: GitHub's push reply for a
    /// new branch says "Create a pull request ... by visiting <url>", and a resident read that
    /// as done, moved her card to review and said "PR is up" with none open (Aris, the M5,
    /// 2026-11-02). This field says which it is.
    pub pull_request: BranchPullRequest,
}

/// Whether the pushed branch has an open pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/code/BranchPullRequest.ts"
)]
pub enum BranchPullRequest {
    /// An open PR carries this branch.
    Open {
        #[ts(type = "number")]
        number: u64,
        url: String,
    },
    /// No PR carries this branch yet: the push alone is not reviewable.
    NoneYet { open_with: String },
    /// GitHub could not be asked (no `gh`, not authenticated, offline); the push itself landed.
    Unknown { why: String },
}

/// The verb that opens a PR from a pushed branch, named where a push has none: on a card's branch
/// `work/submit`, which opens it AND links it to the card (a PR the card does not link is one the
/// merger never sees); on any other branch `code/github/pr-create`.
fn open_a_pr(branch: &str) -> &'static str {
    if crate::code::card_branch::is_card_branch(branch) {
        "work/submit"
    } else {
        "code/github/pr-create"
    }
}

/// The open PR whose head is `branch`, asked of GitHub once after the push, through the one owner
/// of a branch's PR.
async fn pull_request_for(root: &std::path::Path, branch: &str) -> BranchPullRequest {
    match crate::commands::code::github::branch_pull_request::open_pull_request_for(root, None, branch).await {
        Ok(found) => listed(found, branch),
        Err(err) => BranchPullRequest::Unknown {
            why: err.to_string(),
        },
    }
}

/// The branch a push named, as GitHub knows it: a refspec's destination (`src:dst`), without
/// `refs/heads/`. `None` when the push named the current branch (nothing, or `HEAD`), which the
/// caller resolves from the workspace; `gh --head HEAD` would read as no PR (Cormac on #5719).
fn pushed_branch(branch: &str) -> Option<String> {
    let dst = branch.rsplit_once(':').map_or(branch, |(_, dst)| dst).trim_start_matches('+');
    let name = dst.strip_prefix("refs/heads/").unwrap_or(dst); // unwrap_or: a short name has no refs/heads/ prefix to strip
    (!name.is_empty() && name != "HEAD").then(|| name.to_string())
}

/// GitHub's answer for `branch`: its open PR, or the verb that opens one.
fn listed(found: Option<crate::commands::code::github::branch_pull_request::OpenPullRequest>, branch: &str) -> BranchPullRequest {
    match found {
        Some(pr) => BranchPullRequest::Open { number: pr.number, url: pr.url },
        None => BranchPullRequest::NoneYet { open_with: open_a_pr(branch).to_string() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a pushed branch reported as having a PR when GitHub listed none (the
    // resident who said "PR is up" after a push alone), an open PR read as none, or a card's
    // branch pointed at a verb that opens a PR the card never links (the merger never sees it).
    #[test]
    fn a_push_names_its_pull_request_or_the_verb_that_opens_one() {
        use crate::commands::code::github::branch_pull_request::OpenPullRequest;
        let open = OpenPullRequest {
            number: 5718,
            url: "https://github.com/CambrianTech/continuum/pull/5718".into(),
            head: "fable/x".into(),
            base: "canary".into(),
        };
        assert_eq!(
            listed(Some(open), "fable/x"),
            BranchPullRequest::Open {
                number: 5718,
                url: "https://github.com/CambrianTech/continuum/pull/5718".into()
            }
        );
        assert_eq!(listed(None, "e5c80c7f/her-fix"), BranchPullRequest::NoneYet { open_with: "work/submit".into() }, "a card's branch opens and links through work/submit");
        assert_eq!(listed(None, "fable/x"), BranchPullRequest::NoneYet { open_with: "code/github/pr-create".into() });
    }

    // what this catches: a push naming `HEAD` or a refspec asking GitHub for a PR whose head is
    // literally `HEAD` or `src:dst`, which lists nothing and reads as "no PR yet" (Cormac on #5719).
    #[test]
    fn the_pushed_branch_is_the_name_github_knows() {
        assert_eq!(pushed_branch(""), None, "the current branch, resolved from the workspace");
        assert_eq!(pushed_branch("HEAD"), None, "the current branch, resolved from the workspace");
        assert_eq!(pushed_branch("fable/x"), Some("fable/x".into()));
        assert_eq!(pushed_branch("HEAD:fable/x"), Some("fable/x".into()), "a refspec's destination");
        assert_eq!(pushed_branch("+local:refs/heads/fable/x"), Some("fable/x".into()));
        assert_eq!(pushed_branch("local:HEAD"), None);
    }
}

crate::action_command! {
    /// Push commits to a remote (`git push`). Omit `remote`/`branch` to use git's
    /// defaults. Outward-facing — publishes your commits to the shared remote, so
    /// it's the one git verb that escapes the workspace sandbox. Declared
    /// `Privileged` (the same tier as `code/shell`): a local persona (Trusted) or
    /// the owner runs it; a remote `Provisional` room peer is denied by the grid
    /// ACL — they can't make your citizen push to a shared remote on their say-so.
    pub struct CodeGitPush { state: Arc<CodeState> }
    name: "code/git/push",
    access: Privileged,
    native: true, // a capable citizen's working set (Joel, 2026-09-28: offered, like Claude's)
    aliases: &["git_push"],
    params: GitPushParams,
    output: GitPushResult,
    run(this, ctx, p) => {
        let root = workspace_root_for(&this.state, ctx).await?;
        let remote = p.remote.unwrap_or_default();
        let branch = p.branch.unwrap_or_default();
        let output = git_bridge::git_push(&root, &remote, &branch).await.map_err(CommandError::Internal)?;
        let pushed = match pushed_branch(&branch) {
            Some(name) => Ok(name),
            None => git_bridge::run_git(&root, &["rev-parse", "--abbrev-ref", "HEAD"]).await.map(|b| b.trim().to_string()),
        };
        let pull_request = match pushed {
            Ok(pushed) => pull_request_for(&root, &pushed).await,
            Err(why) => BranchPullRequest::Unknown { why },
        };
        Ok(GitPushResult { output, pull_request })
    }
}
