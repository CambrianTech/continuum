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
pub(crate) enum GhRunError {
    #[error("could not run `gh` — is the GitHub CLI installed and authenticated? ({0})")]
    Spawn(String),
    #[error("`gh {args}` failed (exit {code:?}): {stderr}")]
    Failed { args: String, code: Option<i32>, stderr: String },
    #[error("`gh {args}` did not finish within {secs} s and was ended with everything it started; not confirmed gone: {unconfirmed:?}")]
    TimedOut { args: String, secs: u64, unconfirmed: Vec<u32> },
}

impl From<GhRunError> for CommandError {
    fn from(e: GhRunError) -> Self {
        CommandError::Internal(format!("code/github: {e}"))
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

/// A `gh` process a command owns until it and everything it started have ended (Astra on
/// #5026). Ending it is ONE path, [`end_owned`], for a timeout and for a dropped waiter alike.
struct OwnedGh {
    child: Option<tokio::process::Child>,
    held: Option<std::sync::Arc<crate::code::hands::HandsLease>>,
}

impl OwnedGh {
    /// Hand the process to its end: a spawned task, so the caller's cancellation can't cut it
    /// short. `None` once it exited and was reaped, or with no runtime to run the end on.
    fn end(&mut self) -> Option<tokio::task::JoinHandle<crate::inference::lane_process::TreeEnd>> {
        let child = self.child.take()?;
        let runtime = tokio::runtime::Handle::try_current().ok()?;
        Some(runtime.spawn(end_owned(child, self.held.take())))
    }
}

impl Drop for OwnedGh {
    fn drop(&mut self) {
        let _ = self.end(); // the command's waiter is gone: the end still runs, holding its admission
    }
}

/// End `gh` and its tree (`lane_process::end_tree`: descendants deepest first, then `gh`, then
/// its reap) while holding the command's admission. What can't be confirmed gone is kept ON
/// the admission before it drops, so the release that waits it out re-checks those processes
/// and keeps the hands while any is present (Codex root on #5026).
async fn end_owned(mut child: tokio::process::Child, held: Option<std::sync::Arc<crate::code::hands::HandsLease>>) -> crate::inference::lane_process::TreeEnd {
    let end = crate::inference::lane_process::end_tree(&mut child).await;
    if !end.confirmed() {
        crate::probe!(
            class = "code.github.unconfirmed_end",
            survivors = ?end.survivors.iter().map(|who| who.pid).collect::<Vec<_>>(),
            reaped = ?end.reaped,
            unwalked = ?end.unwalked,
            "a gh process was ended but not everything is confirmed ended; its hands' release re-checks or holds"
        );
    }
    if let Some(lease) = &held {
        lease.keep_unconfirmed(&end);
    }
    end
}

/// Run one `gh` invocation in `root` as an owned process, with an optional bound: the ONE
/// execution both [`run_gh`] and [`run_gh_within`] use. A `gh` that outlives `bound` is ended
/// with everything it started (`gh repo clone` runs `git` as a descendant), and the timeout
/// names what isn't confirmed gone, never claiming the tree exited.
async fn run_gh_owned(root: &Path, args: &[String], bound: Option<std::time::Duration>) -> Result<String, GhRunError> {
    let mut cmd = gh_command(root, args);
    // stays in the core's process group, so a stop ends it; its end reaches what it started by
    // the process table's parent links (end_tree), not by a group of its own
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
            let gh_pid = child.id();
            let unconfirmed = match owned.end() {
                // gh itself counts as unconfirmed when its reap or its tree couldn't be confirmed
                Some(end) => match end.await {
                    Ok(end) => end.survivors.iter().map(|who| who.pid).chain(end.unknown().next().and(gh_pid)).collect(),
                    Err(join) => return Err(GhRunError::Spawn(format!("ending the timed-out gh failed to run: {join}"))),
                },
                None => Vec::new(),
            };
            Err(GhRunError::TimedOut { args: joined, secs: bound.map_or(0, |b| b.as_secs()), unconfirmed })
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

    // what this catches (Astra and Codex root on #5026): a `gh` whose command waiter was
    // cancelled kept running, and so did the `git` it started, while a release removed the
    // workspace under them. The owner, dropped mid-run, ends the process AND what it started,
    // and keeps the command's admission until that end is done: the release completes only
    // after, and finds nothing unconfirmed.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_dropped_gh_owner_ends_its_tree_and_holds_the_admission_until_it_has() {
        let dir = tempfile::tempdir().unwrap();
        let grandchild = dir.path().join("grandchild.pid");
        let context = crate::code::hands::HandsContext::fresh();
        let lease = std::sync::Arc::new(crate::code::hands::admit(context).expect("open hands"));
        let child = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(format!("sleep 600 & echo $! > {}; wait", grandchild.display()))
            .spawn()
            .expect("a long process with a descendant");
        let pid = child.id().expect("a live pid");
        let mut started = None;
        for _ in 0..200 {
            if let Some(gc) = std::fs::read_to_string(&grandchild).ok().and_then(|t| t.trim().parse::<u32>().ok()) {
                started = Some(gc);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let gc = started.expect("the descendant started");
        let tree = crate::inference::lane_process::descendants(pid);
        assert!(tree.iter().any(|who| who.pid == gc), "the descendant is in gh's tree: {tree:?}");
        drop(super::OwnedGh { child: Some(child), held: Some(lease) }); // the command's waiter is gone
        let unconfirmed = tokio::time::timeout(std::time::Duration::from_secs(10), crate::code::hands::close(context))
            .await
            .expect("the release completes once the tree is ended, not before and not never");
        assert!(unconfirmed.is_empty(), "nothing is left unconfirmed: {unconfirmed:?}");
        let alive = std::process::Command::new("kill").args(["-0", &pid.to_string()]).status().expect("kill -0");
        assert!(!alive.success(), "gh (pid {pid}) is reaped when the release completes");
        // a killed descendant may wait for init to reap it; a zombie has exited, so it counts as gone
        assert!(crate::inference::lane_process::still_present(&tree).is_empty(), "what gh started is gone when the release completes");
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
