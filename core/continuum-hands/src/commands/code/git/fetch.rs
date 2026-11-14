//! `code/git/fetch` — bring a remote's commits into the caller's workspace (`git fetch`), so she can
//! see where the base moved and rebase onto it.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::checkout_root_for;
use crate::code::git_bridge;
use crate::modules::code::CodeState;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitFetchParams.ts")]
pub struct GitFetchParams {
    /// Remote name. Omit for `origin`.
    #[serde(default)]
    #[ts(optional)]
    pub remote: Option<String>,
    /// What to fetch (a branch such as `canary`, or a refspec). Omit for every branch of the remote.
    #[serde(default)]
    #[ts(optional)]
    pub refspec: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitFetchResult.ts")]
pub struct GitFetchResult {
    /// The remote fetched from.
    pub remote: String,
    /// Raw `git fetch` output (git reports progress on stderr; empty when nothing moved).
    pub output: String,
}

crate::action_command! {
    /// Fetch a remote's commits (`git fetch`), e.g. `refspec: "canary"` before rebasing your branch
    /// onto `origin/canary` with `code/git/rebase`.
    pub struct CodeGitFetch { state: Arc<CodeState> }
    name: "code/git/fetch",
    access: AiSafe,
    native: false, // found through help when her work needs it; the always-offered surface stays within its measured bound (goal 10)
    aliases: &["git_fetch"],
    disclosure: OnDemand,
    params: GitFetchParams,
    output: GitFetchResult,
    run(this, ctx, p) => {
        let root = checkout_root_for(&this.state, ctx).await?;
        let remote = p.remote.filter(|r| !r.trim().is_empty()).unwrap_or_else(|| "origin".to_string()); // unwrap_or_else: no remote named is git's own default remote
        let mut args = vec!["fetch", remote.as_str()];
        if let Some(refspec) = p.refspec.as_deref().filter(|r| !r.trim().is_empty()) {
            args.push(refspec);
        }
        let output = git_bridge::run_git(&root, &args).await.map_err(CommandError::Internal)?;
        Ok(GitFetchResult { remote, output })
    }
}
