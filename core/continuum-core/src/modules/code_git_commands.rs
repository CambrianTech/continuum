//! The persona's GIT hands — the `code/git-*` commands as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s on the ONE registry.
//!
//! ## Why this file exists
//!
//! The git family lived ONLY in [`CodeModule::handle_command`](super::code)'s
//! stringly `match` — dispatchable, but with no [`CommandDescriptor`] in
//! [`command_registry()`](crate::sdk_codegen::command_registry), so a persona was
//! never OFFERED git as a tool. A "coder" with no `git status`, no `commit`. This
//! migrates the family to typed commands: each gets a descriptor (so it appears in
//! the persona tool surface, the grid ACL, codegen, `cu`) AND routes through the
//! O(1) lock-free typed path, winning over the legacy prefix arm (now deleted).
//!
//! ## Authored with the generator
//!
//! Every command here is one [`action_command!`](crate::action_command) block —
//! the §3 generator from [docs/architecture/COMMAND-ORGANIZATION.md]. The author
//! writes a doc comment (⟹ `DESCRIPTION`), the four type fields, and a body. The
//! struct, the trait impl, and descriptor registration are generated. This file is
//! the dep-holding OUTLIER that validates the macro against real module state.
//!
//! ## Identity + concurrency
//!
//! Identity is the AUTHENTICATED caller (`ctx.caller.peer_id`), never a
//! `persona_id` param — the legacy arms read the id from the body, an identity-axis
//! violation any caller could spoof. Each git shell-out runs in `spawn_blocking`
//! with the workspace root resolved and the `DashMap` guard dropped FIRST, so a
//! blocking `git` invocation never holds a shard lock across I/O and never parks a
//! runtime worker — 14–50 personas can run git concurrently without contending on
//! anything but their own per-caller engine.

use std::path::PathBuf;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::code::CodeState;
use super::code_commands::{caller_id, ensure_engine};
use crate::code::git_bridge;
use crate::code::types::GitStatusInfo;
use crate::sdk_codegen::{CommandError, Ctx, DynCommand};

/// Resolve the caller's workspace root, provisioning the engine on first use.
/// Returns an OWNED [`PathBuf`] so the `DashMap` ref guard is dropped before the
/// caller runs blocking git work — never a lock held across a shell-out.
fn workspace_root_for(state: &CodeState, ctx: &Ctx) -> Result<PathBuf, CommandError> {
    let who = caller_id(ctx);
    ensure_engine(state, &who)?;
    let engine = state
        .file_engines
        .get(&who)
        .ok_or_else(|| CommandError::Internal("workspace vanished after provisioning".into()))?;
    Ok(engine.workspace_root())
}

/// Run a blocking git operation off the runtime worker, mapping a join panic to a
/// typed error. `op` produces the git_bridge call against the resolved root.
async fn blocking_git<T, F>(op: F) -> Result<T, CommandError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(op)
        .await
        .map_err(|e| CommandError::Internal(format!("git task panicked: {e}")))
}

// ─────────────────────────── param / result types ───────────────────────────

/// `code/git-status` takes no input — it reports the caller's workspace.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitStatusParams.ts")]
pub struct GitStatusParams {}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitDiffParams.ts")]
pub struct GitDiffParams {
    /// Show STAGED changes (`--cached`) instead of unstaged working-tree changes.
    #[serde(default)]
    pub staged: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitDiffResult.ts")]
pub struct GitDiffResult {
    /// The raw unified diff text.
    pub diff: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitLogParams.ts")]
pub struct GitLogParams {
    /// How many recent commits to return. Omit for the last 10.
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitLogResult.ts")]
pub struct GitLogResult {
    /// The formatted `git log` text.
    pub log: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitAddParams.ts")]
pub struct GitAddParams {
    /// Paths to stage, relative to the workspace root. Empty stages nothing.
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitAddResult.ts")]
pub struct GitAddResult {
    /// Raw `git add` output (usually empty on success).
    pub output: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitCommitParams.ts")]
pub struct GitCommitParams {
    /// The commit message.
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitCommitResult.ts")]
pub struct GitCommitResult {
    /// The full SHA of the new commit.
    pub hash: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitPushParams.ts")]
pub struct GitPushParams {
    /// Remote name (e.g. `origin`). Omit for git's default.
    #[serde(default)]
    pub remote: Option<String>,
    /// Branch to push. Omit for git's default (the current branch).
    #[serde(default)]
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitPushResult.ts")]
pub struct GitPushResult {
    /// Raw `git push` output.
    pub output: String,
}

// ─────────────────────────── the commands ───────────────────────────

crate::action_command! {
    /// Show git status for your workspace: current branch and the modified, added,
    /// deleted, and untracked files. Use before staging or committing to see what
    /// changed.
    pub struct CodeGitStatus { state: Arc<CodeState> }
    name: "code/git-status",
    access: AiSafe,
    params: GitStatusParams,
    output: GitStatusInfo,
    run(this, ctx, _p) => {
        let root = workspace_root_for(&this.state, ctx)?;
        blocking_git(move || git_bridge::git_status(&root)).await
    }
}

crate::action_command! {
    /// Show the git diff for your workspace. By default shows unstaged working-tree
    /// changes; set `staged: true` for the staged (`--cached`) diff.
    pub struct CodeGitDiff { state: Arc<CodeState> }
    name: "code/git-diff",
    access: AiSafe,
    params: GitDiffParams,
    output: GitDiffResult,
    run(this, ctx, p) => {
        let root = workspace_root_for(&this.state, ctx)?;
        let diff = blocking_git(move || git_bridge::git_diff(&root, p.staged))
            .await?
            .map_err(CommandError::Internal)?;
        Ok(GitDiffResult { diff })
    }
}

crate::action_command! {
    /// Show recent commit history (`git log`) for your workspace. Returns the last
    /// `limit` commits (default 10).
    pub struct CodeGitLog { state: Arc<CodeState> }
    name: "code/git-log",
    access: AiSafe,
    params: GitLogParams,
    output: GitLogResult,
    run(this, ctx, p) => {
        let root = workspace_root_for(&this.state, ctx)?;
        let count = p.limit.unwrap_or(10);
        let log = blocking_git(move || git_bridge::git_log(&root, count))
            .await?
            .map_err(CommandError::Internal)?;
        Ok(GitLogResult { log })
    }
}

crate::action_command! {
    /// Stage files for commit (`git add`). Pass workspace-relative paths; stages
    /// nothing if `paths` is empty.
    pub struct CodeGitAdd { state: Arc<CodeState> }
    name: "code/git-add",
    access: AiSafe,
    params: GitAddParams,
    output: GitAddResult,
    run(this, ctx, p) => {
        let root = workspace_root_for(&this.state, ctx)?;
        let output = blocking_git(move || {
            let refs: Vec<&str> = p.paths.iter().map(String::as_str).collect();
            git_bridge::git_add(&root, &refs)
        })
        .await?
        .map_err(CommandError::Internal)?;
        Ok(GitAddResult { output })
    }
}

crate::action_command! {
    /// Commit the staged changes (`git commit -m`). Returns the new commit's full
    /// SHA. Stage files with `code/git-add` first.
    pub struct CodeGitCommit { state: Arc<CodeState> }
    name: "code/git-commit",
    access: AiSafe,
    params: GitCommitParams,
    output: GitCommitResult,
    run(this, ctx, p) => {
        if p.message.trim().is_empty() {
            return Err(CommandError::Invalid(
                "code/git-commit: 'message' is required".into(),
            ));
        }
        let root = workspace_root_for(&this.state, ctx)?;
        let hash = blocking_git(move || git_bridge::git_commit(&root, &p.message))
            .await?
            .map_err(CommandError::Internal)?;
        Ok(GitCommitResult { hash })
    }
}

crate::action_command! {
    /// Push commits to a remote (`git push`). Omit `remote`/`branch` to use git's
    /// defaults. Outward-facing — publishes your commits to the shared remote.
    pub struct CodeGitPush { state: Arc<CodeState> }
    name: "code/git-push",
    access: AiSafe,
    params: GitPushParams,
    output: GitPushResult,
    run(this, ctx, p) => {
        let root = workspace_root_for(&this.state, ctx)?;
        let remote = p.remote.unwrap_or_default();
        let branch = p.branch.unwrap_or_default();
        let output = blocking_git(move || git_bridge::git_push(&root, &remote, &branch))
            .await?
            .map_err(CommandError::Internal)?;
        Ok(GitPushResult { output })
    }
}

/// The dep-holding git command objects [`CodeModule`](super::code::CodeModule)
/// contributes to the kernel's typed object map, so the executor routes each name
/// straight to it — winning over the (now-deleted) legacy prefix → `handle_command`
/// arm.
pub fn command_objects(state: Arc<CodeState>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(CodeGitStatus { state: state.clone() }),
        Arc::new(CodeGitDiff { state: state.clone() }),
        Arc::new(CodeGitLog { state: state.clone() }),
        Arc::new(CodeGitAdd { state: state.clone() }),
        Arc::new(CodeGitCommit { state: state.clone() }),
        Arc::new(CodeGitPush { state }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: the six git commands keep their canonical `code/git-*`
    // wire names — these are the routing keys every caller (cu, the persona tool
    // surface, the grid) binds to; a rename silently breaks them all.
    #[test]
    fn git_command_names_are_canonical() {
        assert_eq!(CodeGitStatus::NAME, "code/git-status");
        assert_eq!(CodeGitDiff::NAME, "code/git-diff");
        assert_eq!(CodeGitLog::NAME, "code/git-log");
        assert_eq!(CodeGitAdd::NAME, "code/git-add");
        assert_eq!(CodeGitCommit::NAME, "code/git-commit");
        assert_eq!(CodeGitPush::NAME, "code/git-push");
    }

    // what this catches: the generator macro fills DESCRIPTION from the doc comment.
    // An empty description means a persona is offered a git tool with no guidance —
    // proves the `///` ⟹ DESCRIPTION wiring stays live.
    #[test]
    fn generator_fills_description_from_doc_comment() {
        assert!(CodeGitStatus::DESCRIPTION.contains("git status"));
        assert!(CodeGitCommit::DESCRIPTION.contains("Commit"));
    }

    // what this catches: a blank commit message is rejected with a typed Invalid
    // BEFORE any git call — never shell out `git commit -m ""`.
    #[tokio::test]
    async fn blank_commit_message_is_rejected() {
        // The guard rejects before any engine/workspace resolution, so empty
        // engine maps are enough — we never touch them.
        let state = Arc::new(CodeState::new(
            Arc::new(dashmap::DashMap::new()),
            Arc::new(dashmap::DashMap::new()),
            tokio::runtime::Handle::current(),
        ));
        let cmd = CodeGitCommit { state };
        let err = cmd
            .run(&Ctx::default(), GitCommitParams { message: "  ".into() })
            .await
            .unwrap_err();
        assert!(matches!(err, CommandError::Invalid(_)));
    }
}
