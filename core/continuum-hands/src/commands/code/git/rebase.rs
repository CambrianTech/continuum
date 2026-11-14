//! `code/git/rebase` — move her branch onto the current base, and carry it through conflicts file
//! by file. A stale branch (66-94 commits behind canary on the M5, 2026-11-07) could only be
//! refreshed by raw shell; this is the typed path: start onto a base, and when it stops, the
//! conflicted files are named, she edits them, `code/git/add`s them and continues (or aborts).
//! Each rewritten commit keeps its author and is committed by her ([`GitActor`]).

use std::path::Path;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{conflicted, head, checkout_root_for};
use crate::code::git_actor::GitActor;
use crate::code::git_bridge;
use crate::modules::code::CodeState;
use crate::sdk_codegen::{CommandError, ProjectsOutcome, ToolVerdict};

/// What to do with the rebase.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/code/GitRebaseParams.ts")]
pub enum GitRebaseParams {
    /// Replay your branch onto `onto` (e.g. `origin/canary`; `code/git/fetch` it first).
    Start { onto: String },
    /// After resolving the named files and `code/git/add`-ing them: carry on.
    Continue,
    /// Drop the commit it stopped on and carry on.
    Skip,
    /// Put the branch back where it was before the rebase started.
    Abort,
}

/// Where the rebase stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/code/GitRebaseResult.ts")]
pub enum GitRebaseResult {
    /// Done: the branch now ends at `head`. Push it (`code/git/push`, force needed for a rewritten
    /// branch: `branch: "+<branch>"`) and `work/submit` again.
    Rebased { head: String },
    /// Already on top of the base: nothing moved.
    UpToDate { head: String },
    /// Stopped: these files conflict. Edit each, `code/git/add` it, then `continue`.
    Conflicts { files: Vec<String> },
    /// Back where it started.
    Aborted { head: String },
}

impl ProjectsOutcome for CodeGitRebase {
    fn outcome(output: &GitRebaseResult) -> ToolVerdict {
        ToolVerdict::of_success(!matches!(output, GitRebaseResult::Conflicts { .. }))
    }
}

crate::action_command! {
    /// Rebase your branch (`git rebase`): `start` onto a base such as `origin/canary`; when it stops,
    /// the conflicting files are listed: fix each, `code/git/add` it, then `continue` (or `skip`,
    /// or `abort`). Commits are rewritten as yours.
    pub struct CodeGitRebase { state: Arc<CodeState> }
    name: "code/git/rebase",
    access: AiSafe,
    native: false, // found through help when her work needs it; the always-offered surface stays within its measured bound (goal 10)
    aliases: &["git_rebase"],
    disclosure: OnDemand,
    params: GitRebaseParams,
    output: GitRebaseResult,
    run(this, ctx, p) => {
        let actor = crate::modules::code_commands::engine_err::acting_git_actor(&this.state, ctx)?;
        let root = checkout_root_for(&this.state, ctx).await?;
        rebase(&root, &actor, &p).await
    }
}

/// The rebase step `p` asks for in `root`, made by `actor`.
pub async fn rebase(root: &Path, actor: &GitActor, p: &GitRebaseParams) -> Result<GitRebaseResult, CommandError> {
    let args: Vec<&str> = match p {
        GitRebaseParams::Start { onto } if onto.trim().is_empty() => {
            return Err(CommandError::Invalid("`onto` names the base to rebase onto, e.g. origin/canary".into()))
        }
        GitRebaseParams::Start { onto } => vec!["rebase", onto.as_str()],
        GitRebaseParams::Continue => vec!["rebase", "--continue"],
        GitRebaseParams::Skip => vec!["rebase", "--skip"],
        GitRebaseParams::Abort => vec!["rebase", "--abort"],
    };
    let before = head(root).await?;
    // No editor in a headless core: a continue keeps each commit's message as it was.
    let ran = git_bridge::run_git_input(root, Some(actor), &args, &[("GIT_EDITOR", "true")], None).await;
    let conflicts = conflicted(root).await?;
    match ran {
        Ok(_) if matches!(p, GitRebaseParams::Abort) => Ok(GitRebaseResult::Aborted { head: head(root).await? }),
        Ok(_) => {
            let after = head(root).await?;
            Ok(if after == before && matches!(p, GitRebaseParams::Start { .. }) {
                GitRebaseResult::UpToDate { head: after }
            } else {
                GitRebaseResult::Rebased { head: after }
            })
        }
        Err(_) if !conflicts.is_empty() => Ok(GitRebaseResult::Conflicts { files: conflicts }),
        Err(error) => Err(CommandError::Internal(error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn git(root: &Path, args: &[&str]) -> String {
        git_bridge::run_git_as(root, &tester(), args).await.unwrap_or_else(|e| panic!("git {args:?}: {e}"))
    }

    fn tester() -> GitActor {
        GitActor::citizen(uuid::Uuid::from_u128(0xa115), Some("Aris"))
    }

    /// A base with `main`, and her branch cut from it, then `main` moving on by `upstream`.
    async fn stale_branch(upstream: &str, hers: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init", "-q", "-b", "main"]).await;
        std::fs::write(root.join("f.txt"), "base\n").unwrap();
        git(root, &["add", "f.txt"]).await;
        git(root, &["commit", "-q", "-m", "base"]).await;
        git(root, &["checkout", "-q", "-b", "her/work"]).await;
        std::fs::write(root.join("f.txt"), hers).unwrap();
        git(root, &["commit", "-q", "-am", "her change"]).await;
        git(root, &["checkout", "-q", "main"]).await;
        std::fs::write(root.join("f.txt"), upstream).unwrap();
        git(root, &["commit", "-q", "-am", "upstream moved"]).await;
        git(root, &["checkout", "-q", "her/work"]).await;
        dir
    }

    // what this catches: a stale branch she could only refresh by raw shell, a conflict reported as
    // a bare failure (no files to resolve), or a resolved conflict she could not carry on from.
    // Start stops naming the conflicted file; she resolves it, adds it, continues, and her branch
    // ends on top of the moved base with her change committed by her.
    #[tokio::test]
    async fn a_stale_branch_rebases_through_its_conflict_file_by_file() {
        let dir = stale_branch("upstream\n", "hers\n").await;
        let root = dir.path();
        let her = tester();
        let stopped = rebase(root, &her, &GitRebaseParams::Start { onto: "main".into() }).await.unwrap();
        assert_eq!(stopped, GitRebaseResult::Conflicts { files: vec!["f.txt".into()] });
        std::fs::write(root.join("f.txt"), "upstream and hers\n").unwrap();
        git(root, &["add", "f.txt"]).await;
        let done = rebase(root, &her, &GitRebaseParams::Continue).await.unwrap();
        let GitRebaseResult::Rebased { head } = done else { panic!("expected a rebased branch, got {done:?}") };
        let main = git(root, &["rev-parse", "main"]).await;
        assert_eq!(git(root, &["rev-parse", &format!("{head}~1")]).await, main, "her change sits on the moved base");
        let who = git(root, &["log", "-1", "--format=%cn|%ce"]).await;
        assert_eq!(who.trim(), format!("{}|{}", her.name(), her.email()), "the rewritten commit is committed by her");
        assert_eq!(
            rebase(root, &her, &GitRebaseParams::Start { onto: "main".into() }).await.unwrap(),
            GitRebaseResult::UpToDate { head },
            "on top already: nothing moves"
        );
    }

    // what this catches: an abort that leaves her mid-rebase, or a start with no base silently
    // rebasing onto git's guess.
    #[tokio::test]
    async fn an_abort_puts_the_branch_back_and_a_start_needs_a_base() {
        let dir = stale_branch("upstream\n", "hers\n").await;
        let root = dir.path();
        let her = tester();
        let before = git(root, &["rev-parse", "HEAD"]).await;
        assert!(matches!(rebase(root, &her, &GitRebaseParams::Start { onto: "main".into() }).await.unwrap(), GitRebaseResult::Conflicts { .. }));
        assert_eq!(rebase(root, &her, &GitRebaseParams::Abort).await.unwrap(), GitRebaseResult::Aborted { head: before.trim().to_string() });
        assert!(matches!(rebase(root, &her, &GitRebaseParams::Start { onto: " ".into() }).await, Err(CommandError::Invalid(_))));
    }
}
