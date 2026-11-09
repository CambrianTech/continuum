//! What each native git and GitHub verb's own result says it did (ONE-ACT 6b). These verbs are
//! declared `native: true` through `action_command!`, which submits each one's projection, so
//! without these impls they do not compile.

use super::git::add::{CodeGitAdd, GitAddResult};
use super::git::apply::{CodeGitApply, GitApplyResult};
use super::git::commit::{CodeGitCommit, GitCommitResult};
use super::git::diff::{CodeGitDiff, GitDiffResult};
use super::git::push::{CodeGitPush, GitPushResult};
use super::git::status::CodeGitStatus;
use super::github::pr_comment::{CodeGithubPrComment, GithubPrCommentResult};
use super::github::pr_create::{CodeGithubPrCreate, GithubPrCreateResult};
use crate::code::types::GitStatusInfo;
use crate::sdk_codegen::{ProjectsOutcome, ToolVerdict};

// Their failures are all refusals (`Err`): a result each returns is the work done.
continuum_kernel::projects_success_on_return! {
    CodeGitAdd => GitAddResult,
    CodeGitApply => GitApplyResult,
    CodeGitCommit => GitCommitResult,
    CodeGitDiff => GitDiffResult,
    CodeGitPush => GitPushResult,
    CodeGithubPrComment => GithubPrCommentResult,
    CodeGithubPrCreate => GithubPrCreateResult,
}

impl ProjectsOutcome for CodeGitStatus {
    fn outcome(output: &GitStatusInfo) -> ToolVerdict {
        ToolVerdict::of_success(output.success)
    }
}
