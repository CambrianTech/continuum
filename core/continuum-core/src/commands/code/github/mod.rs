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
        Some(c) if matches!(c.source, CallerSource::Airc) => format!(
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
/// `gh` itself reported. A timeout never asserts that `gh`'s descendants exited: a caller
/// must treat anything they were writing as possibly still being written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum GhRunError {
    #[error("could not run `gh` — is the GitHub CLI installed and authenticated? ({0})")]
    Spawn(String),
    #[error("`gh {args}` failed (exit {code:?}): {stderr}")]
    Failed { args: String, code: Option<i32>, stderr: String },
    #[error("`gh {args}` did not finish within {secs} s; a tree kill was {tree_kill} and gh itself was stopped, but its descendants' exit is not asserted")]
    TimedOut { args: String, secs: u64, tree_kill: TreeKill },
}

/// What the platform said about the tree kill, reported as said, never upgraded to "exited".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TreeKill {
    Delivered,
    Refused,
    NoPid,
}

impl std::fmt::Display for TreeKill {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            TreeKill::Delivered => "delivered",
            TreeKill::Refused => "refused by the platform",
            TreeKill::NoPid => "impossible (no pid)",
        })
    }
}

impl From<GhRunError> for CommandError {
    fn from(e: GhRunError) -> Self {
        CommandError::Internal(format!("code/github: {e}"))
    }
}

/// How long a killed `gh` gets to be reaped.
const GH_REAP_BOUND: std::time::Duration = std::time::Duration::from_secs(15);

/// Kill `pid` and every descendant while `pid` is still alive, so the walk can find them:
/// Unix, the process group it leads (spawned with `process_group(0)`); Windows,
/// `taskkill /F /T`, the same tree kill `lane_process::kill9` uses, with its outcome kept.
fn kill_gh_tree(pid: u32) -> TreeKill {
    #[cfg(unix)]
    {
        // SAFETY: killpg only sends a signal to the group gh leads; no memory is touched
        let rc = unsafe { libc::killpg(pid as i32, libc::SIGKILL) };
        if rc == 0 { TreeKill::Delivered } else { TreeKill::Refused }
    }
    #[cfg(windows)]
    {
        let ok = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .output()
            .is_ok_and(|o| o.status.success());
        if ok { TreeKill::Delivered } else { TreeKill::Refused }
    }
}

async fn read_all(pipe: Option<impl tokio::io::AsyncRead + Unpin>) -> Vec<u8> {
    use tokio::io::AsyncReadExt;
    let mut buf = Vec::new();
    if let Some(mut pipe) = pipe {
        let _ = pipe.read_to_end(&mut buf).await;
    }
    buf
}

/// [`run_gh`] with a bound. A `gh` that outlives `bound` is killed with its whole tree
/// (`gh repo clone` runs `git` as a descendant) BEFORE `gh` itself is reaped, because
/// Windows' tree walk starts from the live parent; then `gh` is reaped. A timeout is
/// [`GhRunError::TimedOut`] carrying what the platform said about the tree kill, and never
/// claims the tree exited.
pub(crate) async fn run_gh_within(root: &Path, args: &[String], bound: std::time::Duration) -> Result<String, GhRunError> {
    let mut cmd = tokio::process::Command::new("gh");
    cmd.args(args)
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(cmd.as_std_mut(), 0);
    let mut child = cmd.spawn().map_err(|e| GhRunError::Spawn(e.to_string()))?;
    let stdout = tokio::spawn(read_all(child.stdout.take()));
    let stderr = tokio::spawn(read_all(child.stderr.take()));
    let joined = args.join(" ");
    match tokio::time::timeout(bound, child.wait()).await {
        Ok(Ok(status)) => {
            let out = stdout.await.unwrap_or_default(); // unwrap_or_default: a reader task that panicked reads as empty output
            let err = stderr.await.unwrap_or_default(); // unwrap_or_default: as above
            if status.success() {
                Ok(String::from_utf8_lossy(&out).trim().to_string())
            } else {
                Err(GhRunError::Failed { args: joined, code: status.code(), stderr: String::from_utf8_lossy(&err).trim().to_string() })
            }
        }
        Ok(Err(e)) => Err(GhRunError::Spawn(e.to_string())),
        Err(_elapsed) => {
            let tree_kill = child.id().map_or(TreeKill::NoPid, kill_gh_tree);
            let _ = child.start_kill();
            // reap gh so it is not left a zombie; this says nothing about its descendants
            let _ = tokio::time::timeout(GH_REAP_BOUND, child.wait()).await;
            Err(GhRunError::TimedOut { args: joined, secs: bound.as_secs(), tree_kill })
        }
    }
}

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
        ctx.caller = Some(CallerIdentity::local(crate::identity::PeerId::new()));
        assert_eq!(attributed("body".into(), &ctx), "body");
    }
}
