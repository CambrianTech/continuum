//! `code/github/<verb>` — the persona's GITHUB COLLABORATION hands (PRs, issues,
//! comments) as typed [`ActionCommand`](crate::sdk_codegen::ActionCommand)s, one per file.
//!
//! ## Why this exists (the executor → teammate line)
//!
//! `code/git/*` gave her LOCAL git (commit, push, diff). But a teammate does not just
//! write code — they open a PR, respond to a review, file and triage issues. That
//! collaboration layer was the gap between a code EXECUTOR and a friendly TEAMMATE
//! (Joel 2026-08-25: "friendly in how code and GitHub work are managed"). These verbs
//! wrap the `gh` CLI — the same hand a human collaborator uses — run in the caller's
//! workspace (which already has the repo + remote), so a PR opens against the right repo.
//!
//! ## Identity + concurrency (same contract as `code/git`)
//!
//! Identity is the AUTHENTICATED caller (`ctx.caller.peer_id`), never a body param. The
//! workspace root is resolved and the `DashMap` guard dropped BEFORE the blocking `gh`
//! shell-out, so a slow `gh` call never holds a shard lock across network I/O.
//!
//! ## Auth
//!
//! `gh` must be installed + authenticated (`gh auth login`) on the host. A missing/uauthed
//! `gh` FAILS LOUD naming the fix — never a silent no-op ([[fallbacks-are-illegal-fail-loud]]).

use std::path::PathBuf;
use std::sync::Arc;

use crate::modules::code::CodeState;
use crate::sdk_codegen::{CommandError, DynCommand};

pub mod issue_create;
pub mod pr_comment;
pub mod pr_create;

use issue_create::CodeGithubIssueCreate;
use pr_comment::CodeGithubPrComment;
use pr_create::CodeGithubPrCreate;

/// Reuse the git family's workspace-root resolution — a `gh` command operates on the
/// SAME per-caller repo checkout `code/git/*` does.
pub(crate) use super::git::workspace_root_for;

/// Run one `gh` invocation in `root`, off the runtime worker. Returns trimmed stdout on
/// success; a non-zero exit or a missing/unauthenticated `gh` FAILS LOUD with the fix.
/// `gh` reads the repo + auth from the workspace + the host's gh config — never a param.
pub(crate) async fn run_gh(root: PathBuf, args: Vec<String>) -> Result<String, CommandError> {
    tokio::task::spawn_blocking(move || {
        let out = std::process::Command::new("gh")
            .args(&args)
            .current_dir(&root)
            .output()
            .map_err(|e| {
                CommandError::Internal(format!(
                    "code/github: could not run `gh` — is the GitHub CLI installed and \
                     authenticated? Install it, then `gh auth login`. ({e})"
                ))
            })?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
        } else {
            Err(CommandError::Internal(format!(
                "code/github: `gh {}` failed (exit {:?}): {}",
                args.join(" "),
                out.status.code(),
                String::from_utf8_lossy(&out.stderr).trim()
            )))
        }
    })
    .await
    .map_err(|e| CommandError::Internal(format!("gh task panicked: {e}")))?
}

/// The GitHub-collaboration command objects the code module contributes to the kernel's
/// typed object map, aggregated with the shared `Arc<CodeState>` (mirrors
/// [`super::git::command_objects`]).
pub fn command_objects(state: Arc<CodeState>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(CodeGithubPrCreate {
            state: state.clone(),
        }),
        Arc::new(CodeGithubPrComment {
            state: state.clone(),
        }),
        Arc::new(CodeGithubIssueCreate { state }),
    ]
}

#[cfg(test)]
mod tests {
    use crate::sdk_codegen::ActionCommand;

    // what this catches: the wire names mirror the file paths (the routing keys), and each
    // is offered natively (a native-call model can only emit calls for OFFERED tools, so a
    // non-native collaboration verb would be unusable by her — the whole point is she can
    // participate in PR/issue workflow).
    #[test]
    fn github_command_names_mirror_path_and_are_native() {
        use super::*;
        assert_eq!(pr_create::CodeGithubPrCreate::NAME, "code/github/pr-create");
        assert_eq!(pr_comment::CodeGithubPrComment::NAME, "code/github/pr-comment");
        assert_eq!(issue_create::CodeGithubIssueCreate::NAME, "code/github/issue-create");
        assert!(pr_create::CodeGithubPrCreate::NATIVE, "PR create must be offered to be usable");
    }
}
