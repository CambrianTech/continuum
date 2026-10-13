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

/// A `gh` process a command owns, through its exit and reap (Astra on #5026). If the
/// command's waiter is dropped (cancelled, or its own timeout) before `gh` exits, its whole
/// tree is killed and a task keeps the command's admission until `gh` is reaped, so a release
/// never completes under a live `gh` or the `git` it runs.
struct OwnedGh {
    child: Option<tokio::process::Child>,
    held: Option<std::sync::Arc<crate::code::hands::HandsLease>>,
}

impl Drop for OwnedGh {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else { return }; // exited and reaped: nothing to own
        let _ = child.id().map(kill_gh_tree); // the tree first: Windows' walk starts from the live parent
        let _ = child.start_kill();
        let held = self.held.take();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _held = held; // the admission lasts until gh is reaped
                let _ = child.wait().await;
            });
        }
    }
}

/// Run one `gh` invocation in `root` as an owned process, with an optional bound: the ONE
/// execution both [`run_gh`] and [`run_gh_within`] use. A `gh` that outlives `bound` is killed
/// with its whole tree (`gh repo clone` runs `git` as a descendant) BEFORE `gh` itself is
/// reaped, because Windows' tree walk starts from the live parent. A timeout is
/// [`GhRunError::TimedOut`] carrying what the platform said about the tree kill, and never
/// claims the tree exited.
async fn run_gh_owned(root: &Path, args: &[String], bound: Option<std::time::Duration>) -> Result<String, GhRunError> {
    let mut cmd = gh_command(root, args);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(cmd.as_std_mut(), 0);
    let child = cmd.spawn().map_err(|e| GhRunError::Spawn(e.to_string()))?;
    let mut owned = OwnedGh { child: Some(child), held: crate::code::hands::current() };
    let Some(child) = owned.child.as_mut() else {
        return Err(GhRunError::Spawn("the spawned gh vanished before it was owned".into()));
    };
    let stdout = tokio::spawn(read_all(child.stdout.take()));
    let stderr = tokio::spawn(read_all(child.stderr.take()));
    let joined = args.join(" ");
    let waited = match bound {
        Some(bound) => tokio::time::timeout(bound, child.wait()).await,
        None => Ok(child.wait().await),
    };
    match waited {
        Ok(Ok(status)) => {
            owned.child = None; // exited and reaped by wait()
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
            // reap gh so it is not left a zombie; this says nothing about its descendants. If it
            // isn't reaped within the bound, the owner's drop keeps the admission until it is.
            if let Ok(Ok(_)) = tokio::time::timeout(GH_REAP_BOUND, child.wait()).await {
                owned.child = None;
            }
            Err(GhRunError::TimedOut { args: joined, secs: bound.map_or(0, |b| b.as_secs()), tree_kill })
        }
    }
}

/// [`run_gh`] with a bound; see [`run_gh_owned`].
pub(crate) async fn run_gh_within(root: &Path, args: &[String], bound: std::time::Duration) -> Result<String, GhRunError> {
    run_gh_owned(root, args, Some(bound)).await
}

/// Run one `gh` invocation in `root` as an owned process. Returns trimmed stdout on success; a
/// non-zero exit or a missing/unauthenticated `gh` FAILS LOUD with the fix. `gh` reads the
/// repo + auth from the workspace + the host's gh config — never a param.
pub(crate) async fn run_gh(root: PathBuf, args: Vec<String>) -> Result<String, CommandError> {
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
    cmd.args(args)
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    cmd
}

/// Run one local `git` invocation in `root`, off the runtime worker — the preflight twin of
/// `run_gh`. This is the module's single git executor: it preserves the full Output (status and
/// both streams) and owns the execution error policy. When we cannot run git at all (missing
/// binary, spawn failure), it returns None = unknown; a check that cannot decide defers to the
/// real command instead of inventing a new failure mode. Callers layer their own interpretation.
pub(crate) async fn git_output(root: PathBuf, args: Vec<String>) -> Option<std::process::Output> {
    crate::code::hands::spawn_blocking(move || {
        std::process::Command::new("git")
            .args(&args)
            .current_dir(&root)
            .output() // no git on this host → unknown, defer to `gh` (which will say so itself)
            .ok()
    })
    .await
    .unwrap_or(None) // the worker panicked — treat as undecidable
}

/// [`git_output`] with the "did it succeed?" interpretation: a non-zero exit returns None (unknown).
pub(crate) async fn git_quiet(root: PathBuf, args: Vec<String>) -> Option<String> {
    let out = git_output(root, args).await?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
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

    // what this catches (Astra on #5026): a `gh` whose command waiter was cancelled kept running
    // (run_gh had no owner at all) while a release removed the workspace under it. The owner,
    // dropped mid-run, kills the process and keeps the command's admission until it is REAPED.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_dropped_gh_owner_holds_the_admission_until_the_process_is_reaped() {
        let context = crate::code::hands::HandsContext::fresh();
        let lease = std::sync::Arc::new(crate::code::hands::admit(context).expect("open hands"));
        let child = tokio::process::Command::new("sleep").arg("30").spawn().expect("a long process");
        let pid = child.id().expect("a live pid");
        drop(super::OwnedGh { child: Some(child), held: Some(lease) }); // the command's waiter is gone
        tokio::time::timeout(std::time::Duration::from_secs(10), crate::code::hands::close(context))
            .await
            .expect("the release completes once the killed process is reaped, not before and not never");
        let still = std::process::Command::new("kill").args(["-0", &pid.to_string()]).status().expect("kill -0");
        assert!(!still.success(), "the process is gone (reaped) when the release completes");
    }

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
