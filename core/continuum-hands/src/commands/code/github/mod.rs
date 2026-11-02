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

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::modules::code::CodeState;
use crate::sdk_codegen::{CommandError, DynCommand};

pub mod issue_create;
pub mod pr_comment;
pub mod pr_create;

use issue_create::CodeGithubIssueCreate;

/// Attribution for GitHub text a CITIZEN writes. Every push and API call goes out under the
/// operator's `gh` account (GitHub has no notion of an agent acting inside one account), so a
/// citizen-authored PR, comment or issue carries a footer naming her by peer id. Citizens
/// hold these verbs exactly as Claude or Codex do (Joel, 2026-09-28: the point is a system
/// that replaces them); attribution is what keeps authorship legible, never a gate.
pub(crate) fn attributed(body: String, ctx: &crate::sdk_codegen::Ctx) -> String {
    use crate::routing::auth_policy::CallerSource;
    match ctx.caller.as_ref() {
        Some(c) if matches!(c.source, CallerSource::Airc | CallerSource::LocalPersona) => format!(
            "{body}\n\n---\nAuthored by Continuum citizen `{}` (posted through the operator's GitHub account).",
            c.peer_id
        ),
        _ => body,
    }
}

use pr_comment::CodeGithubPrComment;
use pr_create::CodeGithubPrCreate;

/// Reuse the git family's workspace-root resolution — a `gh` command operates on the
/// SAME per-caller repo checkout `code/git/*` does.
pub(crate) use super::git::workspace_root_for;

/// How a bounded `gh` invocation failed. Typed so a caller can tell a timeout from a refusal
/// `gh` itself reported. A timeout names the processes it couldn't confirm gone: a caller
/// must treat anything those were writing as possibly still being written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GhRunError {
    #[error("could not run `gh` — is the GitHub CLI installed and authenticated? ({0})")]
    Spawn(String),
    #[error("`gh {args}` failed (exit {code:?}): {stderr}")]
    Failed { args: String, code: Option<i32>, stderr: String },
    #[error("`gh {args}` did not finish within {secs} s and was ended with everything it started; not confirmed gone: {unconfirmed:?}")]
    TimedOut { args: String, secs: u64, unconfirmed: Vec<u32> },
    /// `gh` exited, but its output wasn't read whole: a process it started held the pipes past
    /// the bound (and may still be writing what it was writing), or the read failed.
    #[error("`gh {args}` exited (exit {code:?}) but its output is not whole: {note}")]
    Incomplete { args: String, code: Option<i32>, note: String },
}

impl From<GhRunError> for CommandError {
    fn from(e: GhRunError) -> Self {
        CommandError::Internal(format!("code/github: {e}"))
    }
}

/// Run one `gh` invocation in `root` as an owned process ([`continuum_process::run_owned`]), with an
/// optional bound: the ONE execution both [`run_gh`] and [`run_gh_within`] use. A `gh` that
/// outlives `bound` is ended with everything it started (`gh repo clone` runs `git` as a
/// descendant), and the timeout names what isn't confirmed gone.
///
/// [`continuum_process::run_owned`]: continuum_process::run_owned
async fn run_gh_owned(root: &Path, args: &[String], bound: Option<std::time::Duration>) -> Result<String, GhRunError> {
    use continuum_process::{run_owned, Ended};
    let joined = args.join(" ");
    let out = run_owned(gh_command(root, args), bound, "gh", std::sync::Arc::default()).await.map_err(|e| GhRunError::Spawn(e.to_string()))?;
    match out.ended {
        Ended::Exited(status) => match out.capture_note() {
            Some(note) => Err(GhRunError::Incomplete { args: joined, code: status.code(), note }),
            None if status.success() => Ok(String::from_utf8_lossy(&out.stdout.bytes).trim().to_string()),
            None => Err(GhRunError::Failed { args: joined, code: status.code(), stderr: String::from_utf8_lossy(&out.stderr.bytes).trim().to_string() }),
        },
        Ended::TimedOut { unconfirmed } => Err(GhRunError::TimedOut { args: joined, secs: bound.map_or(0, |b| b.as_secs()), unconfirmed }),
    }
}

/// [`run_gh`] with a bound; see [`run_gh_owned`].
pub async fn run_gh_within(root: &Path, args: &[String], bound: std::time::Duration) -> Result<String, GhRunError> {
    run_gh_owned(root, args, Some(bound)).await
}

/// Run one `gh` invocation in `root` as an owned process. Returns trimmed stdout on success; a
/// non-zero exit or a missing/unauthenticated `gh` FAILS LOUD with the fix. `gh` reads the
/// repo + auth from the workspace + the host's gh config — never a param.
pub async fn run_gh(root: PathBuf, args: Vec<String>) -> Result<String, CommandError> {
    run_gh_owned(&root, &args, None).await.map_err(|e| match e {
        GhRunError::Spawn(e) => CommandError::Internal(format!(
            "code/github: could not run `gh` — is the GitHub CLI installed and \
             authenticated? Install it, then `gh auth login`. ({e})"
        )),
        other => CommandError::Internal(format!("code/github: {other}")),
    })
}

/// THE one place a `gh` process is built (the source-hygiene rule: GitHub goes through this
/// module's client). Output is piped and stdin closed; callers add only what they need.
fn gh_command(root: &Path, args: &[String]) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("gh");
    cmd.args(args).current_dir(root).stdin(std::process::Stdio::null());
    cmd
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
        // pr-create and pr-comment are a capable citizen's working set, offered natively
        // (Joel, 2026-09-28); issue-create stays one call away (the 9/03 spam was issues)
        assert!(pr_create::CodeGithubPrCreate::NATIVE);
        assert!(pr_comment::CodeGithubPrComment::NATIVE);
        assert!(!issue_create::CodeGithubIssueCreate::NATIVE);
    }

    // what this catches (Joel, 2026-09-28): citizens once lost GitHub hands entirely because
    // their authorship could not be told apart from the operator's. They hold the verbs now;
    // what they write names them, and the operator's own text is untouched.
    #[test]
    fn a_citizens_github_text_names_her_and_the_operators_does_not() {
        use super::*;
        use crate::routing::CallerIdentity;
        let mut ctx = crate::sdk_codegen::Ctx::default();
        assert_eq!(attributed("body".into(), &ctx), "body");
        let citizen = crate::identity::PeerId::new();
        ctx.caller = Some(CallerIdentity::airc(citizen));
        let text = attributed("body".into(), &ctx);
        assert!(text.starts_with("body") && text.contains(&citizen.to_string()), "{text}");
        // Regression: in-process residents use LocalPersona, not the AIRC transport.
        ctx.caller = Some(CallerIdentity::local_persona(citizen));
        assert_eq!(attributed("body".into(), &ctx), text);
        ctx.caller = Some(CallerIdentity::local(crate::identity::PeerId::new()));
        assert_eq!(attributed("body".into(), &ctx), "body");
    }
}
