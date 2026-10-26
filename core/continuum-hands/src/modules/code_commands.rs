//! The persona's HANDS as typed commands on the ONE registry.
//!
//! These are the file-operation tools (`code/read`, `code/search`, `code/edit`,
//! …) reimplemented as [`ActionCommand`]s so they live on the SINGLE command
//! surface every interface reads — the persona tool surface
//! ([`authorized_tool_specs`](crate::cognition::persona_tools::authorized_tool_specs)),
//! the grid ACL ([`ai_safe_commands`](crate::routing::command_acl)), codegen, and
//! runtime dispatch — all from ONE `CommandSpec` declaration per command. No
//! second list, no parallel allow-table. A command becomes a persona tool simply
//! by being an `AiSafe` `ActionCommand`; the descriptor flows everywhere.
//!
//! ## Why this replaces the legacy `code/*` match arms
//!
//! The old surface dispatched these through `CodeModule::handle_command`'s
//! stringly `match` — invisible to `command_registry()`, so a persona was never
//! OFFERED them (it saw the ~3 commands that had migrated). That is the disjoint
//! two-registry tragedy: the real commands existed but the persona couldn't see
//! them. Declaring each as an `ActionCommand` + `register_command!` puts its typed
//! descriptor (name + schema + access + description) into the one registry, and
//! [`CodeModule::commands`](super::code::CodeModule) returns the dep-holding
//! object so the executor routes the name straight to it (winning over the legacy
//! prefix arm, per [`route_object`](crate::runtime::registry)). The match arms for
//! the migrated commands are then deleted — one path, one source of truth.
//!
//! ## Identity is the AUTHENTICATED caller, never a param
//!
//! The legacy arms read `persona_id` from the request body — an identity-axis
//! violation (any caller could name any persona) and an ergonomic tax (the model
//! had to pass its own id). Here the workspace is scoped to `ctx.caller.peer_id`
//! — the airc-verified identity the executor already gated on. A persona acts as
//! ITSELF by construction; there is nothing to spoof. (`None` caller = the
//! substrate-local owner.) This is the encapsulate-airc-identity principle applied
//! at the tool boundary: the peer_id is airc's, not a continuum-minted param.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::code::CodeState;
use crate::code::shell_types::{ShellExecuteResponse, ShellExecutionStatus};
use crate::code::types::{
    DirEntry, ExistsResult, FsEntryKind, GlobResult, ListResult, ReadResult, SearchMatch,
    SearchResult, TreeResult, WriteResult,
};
use crate::code::hands::HandsKey;
use crate::code::{search, tree, EditMode, FileEngine, PathSecurity, ShellSession};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx, DynCommand, ToolVerdict};

/// Resolve the caller's engine, provisioning it on first use. Returned as a
/// `DashMap` ref guard the caller borrows the engine through.
macro_rules! engine {
    ($self:ident, $ctx:ident) => {{
        let key = hands_of(&$self.state, $ctx);
        ensure_engine(&$self.state, &key).await?;
        $self
            .state
            .file_engines
            .get(&key.map_key())
            .ok_or_else(|| CommandError::Internal("workspace vanished after provisioning".into()))?
    }};
}

use crate::code::citizen_layer::*;
use crate::code::hands::ReleaseHandsResult;

mod engine_err;
pub use engine_err::*;
mod normalize_edit_mode;
pub use normalize_edit_mode::*;
mod code_shell_kill_result;
pub use code_shell_kill_result::*;

#[cfg(test)]
mod tests;
