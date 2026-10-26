//! `code/git/<verb>` — the persona's GIT hands as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s, one command per file.
//!
//! ## Why these are typed commands (not a `match` arm)
//!
//! The git family once lived ONLY in [`CodeModule::handle_command`](crate::modules::code)'s
//! stringly `match` — dispatchable, but with no descriptor in the registry, so a
//! persona was never OFFERED git as a tool. A "coder" with no `git status`, no
//! `commit`. As typed commands each gets a descriptor (so it appears in the persona
//! tool surface, the grid ACL, codegen, `uu`) AND routes through the O(1) lock-free
//! typed path. The wire name mirrors the file path — `commands/code/git/status.rs`
//! ⟺ `code/git/status` — so the source tree IS the namespace a persona reads.
//!
//! ## Identity + concurrency
//!
//! Identity is the AUTHENTICATED caller (`ctx.caller.peer_id`), never a `persona_id`
//! param — the legacy arms read the id from the body, an identity-axis violation any
//! caller could spoof. Each git runs as an owned child (`git_bridge::run_git`) awaited
//! with the workspace root resolved and the `DashMap` guard dropped FIRST, so a git never
//! holds a shard lock across I/O, and a cancelled command ends its git and what it started.

use std::path::PathBuf;
use std::sync::Arc;

use crate::modules::code::CodeState;
use crate::modules::code_commands::ensure_engine;
use crate::sdk_codegen::{CommandError, Ctx, DynCommand};

pub mod add;
pub mod apply;
pub mod commit;
pub mod diff;
pub mod log;
pub mod push;
pub mod status;

use add::CodeGitAdd;
use apply::CodeGitApply;
use commit::CodeGitCommit;
use diff::CodeGitDiff;
use log::CodeGitLog;
use push::CodeGitPush;
use status::CodeGitStatus;

/// Resolve the caller's workspace root, provisioning the engine on first use.
/// Returns an OWNED [`PathBuf`] so the `DashMap` ref guard is dropped before the
/// caller awaits its git — never a lock held across a shell-out.
pub(crate) async fn workspace_root_for(state: &CodeState, ctx: &Ctx) -> Result<PathBuf, CommandError> {
    let key = crate::modules::code_commands::hands_of(state, ctx);
    ensure_engine(state, &key).await?;
    let engine = state
        .file_engines
        .get(&key.map_key())
        .ok_or_else(|| CommandError::Internal("workspace vanished after provisioning".into()))?;
    Ok(engine.workspace_root())
}

/// The dep-holding git command objects [`CodeModule`](crate::modules::code::CodeModule)
/// contributes to the kernel's typed object map, so the executor routes each name
/// straight to it — winning over the (now-deleted) legacy prefix → `handle_command`
/// arm. Each struct is defined in its own verb file; this aggregates them with the
/// shared `Arc<CodeState>`.
pub fn command_objects(state: Arc<CodeState>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(CodeGitStatus {
            state: state.clone(),
        }),
        Arc::new(CodeGitDiff {
            state: state.clone(),
        }),
        Arc::new(CodeGitLog {
            state: state.clone(),
        }),
        Arc::new(CodeGitAdd {
            state: state.clone(),
        }),
        Arc::new(CodeGitCommit {
            state: state.clone(),
        }),
        Arc::new(CodeGitPush {
            state: state.clone(),
        }),
        Arc::new(CodeGitApply { state }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: the six git commands carry their namespaced `code/git/<verb>`
    // wire names — the routing keys every caller (cu, the persona tool surface, the
    // grid) binds to. The name mirrors the file path; a drift between them silently
    // breaks the "file tree IS the namespace" contract.
    #[test]
    fn git_command_names_mirror_their_path() {
        assert_eq!(CodeGitStatus::NAME, "code/git/status");
        assert_eq!(CodeGitDiff::NAME, "code/git/diff");
        assert_eq!(CodeGitLog::NAME, "code/git/log");
        assert_eq!(CodeGitAdd::NAME, "code/git/add");
        assert_eq!(CodeGitCommit::NAME, "code/git/commit");
        assert_eq!(CodeGitPush::NAME, "code/git/push");
    }

    // what this catches: the generator macro fills DESCRIPTION from the doc comment.
    // An empty description means a persona is offered a git tool with no guidance —
    // proves the `///` ⟹ DESCRIPTION wiring stays live.
    #[test]
    fn generator_fills_description_from_doc_comment() {
        assert!(CodeGitStatus::DESCRIPTION.contains("git status"));
        assert!(CodeGitCommit::DESCRIPTION.contains("Commit"));
    }
}
