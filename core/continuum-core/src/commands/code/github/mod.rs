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

/// GitHub verbs act on an EXTERNAL service under the OPERATOR's `gh` identity.
/// A citizen (an airc caller) may not file issues, open PRs, or comment as the
/// operator: 114 "placeholder" issues landed on the repo on 2026-09-03 because
/// the verbs were offered to every turn and looping citizens used them to
/// "avoid an unused tool". Until a room recipe can GRANT a citizen GitHub
/// authorship in her own name, the verbs refuse airc callers loudly — the
/// refusal names the rule, so a citizen learns the boundary instead of a 500.
pub(crate) fn require_operator(ctx: &crate::sdk_codegen::Ctx, verb: &str) -> Result<(), CommandError> {
    use crate::routing::auth_policy::CallerSource;
    match ctx.caller.as_ref().map(|c| &c.source) {
        Some(CallerSource::Airc) => Err(CommandError::Invalid(format!(
            "{verb}: GitHub verbs act under the operator's identity and are not available to \
             citizens in this room. Do the work in your checkout (code/read, code/edit, \
             code/shell, git) and report in the room; an offered tool is never a must-use."
        ))),
        _ => Ok(()),
    }
}
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

    // what this catches: the wire names mirror the file paths (the routing keys), and
    // NONE is pushed into every turn's native offer — a tool-call model treats an offered
    // tool as a must-use (114 placeholder issues on 2026-09-03). They stay reachable BY
    // NAME through commands/list + commands/help for a caller allowed to use them.
    #[test]
    fn github_command_names_mirror_path_and_are_not_native() {
        use super::*;
        assert_eq!(pr_create::CodeGithubPrCreate::NAME, "code/github/pr-create");
        assert_eq!(pr_comment::CodeGithubPrComment::NAME, "code/github/pr-comment");
        assert_eq!(issue_create::CodeGithubIssueCreate::NAME, "code/github/issue-create");
        assert!(!pr_create::CodeGithubPrCreate::NATIVE);
        assert!(!issue_create::CodeGithubIssueCreate::NATIVE);
        assert!(!pr_comment::CodeGithubPrComment::NATIVE);
    }

    // what this catches: a citizen (airc caller) is refused with the rule named; the
    // operator (local caller, or no caller — the CLI) passes.
    #[test]
    fn github_verbs_refuse_citizens_and_admit_the_operator() {
        use super::*;
        use crate::routing::CallerIdentity;
        let mut ctx = crate::sdk_codegen::Ctx::default();
        assert!(require_operator(&ctx, "code/github/issue-create").is_ok());
        ctx.caller = Some(CallerIdentity::local(crate::identity::PeerId::new()));
        assert!(require_operator(&ctx, "code/github/issue-create").is_ok());
        ctx.caller = Some(CallerIdentity::airc(crate::identity::PeerId::new()));
        let err = require_operator(&ctx, "code/github/issue-create").unwrap_err();
        assert!(err.to_string().contains("not available to citizens"), "{err}");
    }
}
