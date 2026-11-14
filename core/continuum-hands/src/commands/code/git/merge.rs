//! `code/git/merge` — bring another branch into hers (`git merge`), never rewriting her history:
//! the way a teammate takes the base in when a pull request is behind or conflicts ("merge canary
//! IN, never rebase"). A remote branch (`origin/canary`) is fetched first, so she merges what the
//! remote has now. When it stops, the conflicted files are named; she edits each, `code/git/add`s
//! it, and continues (or aborts). The merge commit is hers ([`GitActor`]).

use std::path::Path;
use std::sync::Arc;

use continuum_base::non_empty_text::NonEmptyText;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{conflicted, head, workspace_root_for};
use crate::code::git_actor::GitActor;
use crate::code::git_bridge;
use crate::modules::code::CodeState;
use crate::sdk_codegen::{CommandError, ProjectsOutcome, ToolVerdict};

/// What to do with the merge.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/code/GitMergeParams.ts")]
pub enum GitMergeParams {
    /// Merge `from` (a branch or ref, e.g. `origin/canary`) into your current branch.
    Start {
        #[schemars(with = "String")]
        #[ts(type = "string")]
        from: NonEmptyText,
    },
    /// After resolving the named files and `code/git/add`-ing them: make the merge commit.
    Continue,
    /// Put the branch back where it was before the merge started.
    Abort,
}

/// Where the merge stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/code/GitMergeResult.ts")]
pub enum GitMergeResult {
    /// Done: the branch now ends at `head`. Push it; no force is needed, nothing was rewritten.
    Merged { head: String },
    /// Already contains `from`: nothing moved.
    UpToDate { head: String },
    /// Stopped: these files conflict. Edit each, `code/git/add` it, then `continue`.
    Conflicts { files: Vec<String> },
    /// Back where it started.
    Aborted { head: String },
}

impl ProjectsOutcome for CodeGitMerge {
    fn outcome(output: &GitMergeResult) -> ToolVerdict {
        ToolVerdict::of_success(!matches!(output, GitMergeResult::Conflicts { .. }))
    }
}

crate::action_command! {
    /// Merge another branch into yours (`git merge`): `start` with `from` such as `origin/canary`
    /// (a remote branch is fetched first); when it stops, the conflicting files are listed: fix
    /// each, `code/git/add` it, then `continue` (or `abort`). Your history is kept, never rewritten.
    pub struct CodeGitMerge { state: Arc<CodeState> }
    name: "code/git/merge",
    access: AiSafe,
    native: true, // named by the merge gate's refusals when a PR is behind or conflicts; one help away (OnDemand)
    aliases: &["git_merge"],
    disclosure: OnDemand,
    params: GitMergeParams,
    output: GitMergeResult,
    run(this, ctx, p) => {
        let actor = crate::modules::code_commands::engine_err::acting_git_actor(&this.state, ctx)?;
        let root = workspace_root_for(&this.state, ctx).await?;
        merge(&root, &actor, &p).await
    }
}

/// When `from` names a configured remote's branch (`origin/canary`), fetch that branch, so the merge
/// takes what the remote holds now rather than a stale copy of it. Any other ref is local already.
async fn fetch_remote_branch(root: &Path, from: &str) -> Result<(), CommandError> {
    let Some((remote, branch)) = from.split_once('/') else { return Ok(()) };
    let remotes = git_bridge::run_git(root, &["remote"]).await.map_err(CommandError::Internal)?;
    if remotes.lines().any(|known| known.trim() == remote) {
        git_bridge::run_git(root, &["fetch", remote, branch]).await.map_err(CommandError::Internal)?;
    }
    Ok(())
}

/// The merge step `p` asks for in `root`, made by `actor`.
pub async fn merge(root: &Path, actor: &GitActor, p: &GitMergeParams) -> Result<GitMergeResult, CommandError> {
    let args: Vec<&str> = match p {
        GitMergeParams::Start { from } => vec!["merge", "--no-edit", from.as_str()],
        GitMergeParams::Continue => vec!["merge", "--continue"],
        GitMergeParams::Abort => vec!["merge", "--abort"],
    };
    if let GitMergeParams::Start { from } = p {
        fetch_remote_branch(root, from.as_str()).await?;
    }
    let before = head(root).await?;
    // No editor in a headless core: the merge commit keeps git's own message.
    match git_bridge::run_git_input(root, Some(actor), &args, &[("GIT_EDITOR", "true")], None).await {
        Ok(_) if matches!(p, GitMergeParams::Abort) => Ok(GitMergeResult::Aborted { head: head(root).await? }),
        Ok(_) => {
            let after = head(root).await?;
            Ok(match p {
                GitMergeParams::Start { .. } if after == before => GitMergeResult::UpToDate { head: after },
                _ => GitMergeResult::Merged { head: after },
            })
        }
        Err(stopped) => {
            let files = conflicted(root).await?;
            match files.is_empty() {
                true => Err(CommandError::Internal(stopped.to_string())),
                false => Ok(GitMergeResult::Conflicts { files }),
            }
        }
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

    fn start(from: &str) -> GitMergeParams {
        GitMergeParams::Start { from: NonEmptyText::try_from(from.to_string()).expect("a ref") }
    }

    /// A base `main`, her branch cut from it with `hers`, then `main` moving on with `upstream`.
    async fn diverged(upstream: &str, hers: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        git(root, &["init", "-q", "-b", "main"]).await;
        std::fs::write(root.join("f.txt"), "base\n").expect("seed");
        git(root, &["add", "f.txt"]).await;
        git(root, &["commit", "-q", "-m", "base"]).await;
        git(root, &["checkout", "-q", "-b", "her/work"]).await;
        std::fs::write(root.join("f.txt"), hers).expect("hers");
        git(root, &["commit", "-q", "-am", "her change"]).await;
        git(root, &["checkout", "-q", "main"]).await;
        std::fs::write(root.join("f.txt"), upstream).expect("upstream");
        git(root, &["commit", "-q", "-am", "upstream moved"]).await;
        git(root, &["checkout", "-q", "her/work"]).await;
        dir
    }

    // what this catches: a conflict reported as a bare failure (no files to resolve), a resolved
    // conflict she could not carry on from, a merge that rewrote her history, or a merge commit
    // signed by the node instead of her. Start stops naming the file; she resolves, adds and
    // continues; her commit and the base's are both parents of the merge, committed by her.
    #[tokio::test]
    async fn a_conflicting_base_merges_in_file_by_file_and_keeps_her_history() {
        let dir = diverged("upstream\n", "hers\n").await;
        let root = dir.path();
        let her = tester();
        let mine = git(root, &["rev-parse", "HEAD"]).await;
        assert_eq!(merge(root, &her, &start("main")).await.expect("stops"), GitMergeResult::Conflicts { files: vec!["f.txt".into()] });
        std::fs::write(root.join("f.txt"), "upstream and hers\n").expect("resolve");
        git(root, &["add", "f.txt"]).await;
        let GitMergeResult::Merged { head } = merge(root, &her, &GitMergeParams::Continue).await.expect("continues") else { panic!("expected a merge commit") };
        let parents = git(root, &["log", "-1", "--format=%P", &head]).await;
        let main = git(root, &["rev-parse", "main"]).await;
        assert_eq!(parents.split_whitespace().collect::<Vec<_>>(), [mine.trim(), main.trim()], "her commit kept, the base merged in");
        let who = git(root, &["log", "-1", "--format=%cn|%ce"]).await;
        assert_eq!(who.trim(), format!("{}|{}", her.name(), her.email()), "the merge commit is hers");
        assert_eq!(merge(root, &her, &start("main")).await.expect("merges"), GitMergeResult::UpToDate { head }, "contains the base already: nothing moves");
    }

    // what this catches: an abort that leaves her mid-merge, or a remote branch merged from a stale
    // local copy: `origin/main` is fetched first, so a commit pushed to the remote after her last
    // fetch is merged in.
    #[tokio::test]
    async fn an_abort_puts_the_branch_back_and_a_remote_branch_is_fetched_first() {
        let dir = diverged("upstream\n", "hers\n").await;
        let root = dir.path();
        let her = tester();
        let before = git(root, &["rev-parse", "HEAD"]).await;
        assert!(matches!(merge(root, &her, &start("main")).await.expect("stops"), GitMergeResult::Conflicts { .. }));
        assert_eq!(merge(root, &her, &GitMergeParams::Abort).await.expect("aborts"), GitMergeResult::Aborted { head: before.trim().to_string() });

        let remote = tempfile::tempdir().expect("remote");
        git(remote.path(), &["init", "-q", "--bare", "-b", "main"]).await;
        git(root, &["remote", "add", "origin", &remote.path().to_string_lossy()]).await;
        git(root, &["push", "-q", "origin", "main"]).await;
        git(root, &["fetch", "-q", "origin"]).await;
        // The remote moves on after her fetch: a teammate's new file on main.
        let teammate = tempfile::tempdir().expect("teammate");
        git(teammate.path(), &["clone", "-q", &remote.path().to_string_lossy(), "."]).await;
        std::fs::write(teammate.path().join("g.txt"), "new\n").expect("teammate's file");
        git(teammate.path(), &["add", "g.txt"]).await;
        git(teammate.path(), &["commit", "-q", "-m", "teammate"]).await;
        git(teammate.path(), &["push", "-q", "origin", "main"]).await;
        assert!(matches!(merge(root, &her, &start("origin/main")).await.expect("stops on f.txt"), GitMergeResult::Conflicts { .. }));
        assert!(root.join("g.txt").exists(), "the commit pushed after her fetch was merged in");
    }
}
