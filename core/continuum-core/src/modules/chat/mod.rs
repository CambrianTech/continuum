//! ChatModule — first proof-of-pattern module migration.
//!
//! Per Joel's directive:
//! > "Chat is gonna be airc man. So that's extracted period. Chat is of
//! > course a bonafide command though. Do not cheapen it. So the
//! > commands need to be or at least some to start, entirely rust."
//!
//! The split:
//! - **Substrate** (delivery, pub/sub, peers, signing) → airc.
//! - **Commands** (`chat/send`, `chat/poll`, `chat/analyze`, `chat/export`)
//!   → Continuum kernel-level ServiceModule, this module.
//!
//! This is the FIRST real module migration from a TS command to a
//! Rust `ServiceModule`, following every pattern the substrate floor
//! established in the recent PRs:
//! - `ServiceModule` trait (PR #1471)
//! - `CommandResult` cell shapes (PR #1485)
//! - `CommandRequest<P>` / `CommandResponse<T>` envelopes (PR #1486)
//! - Architecture from `docs/architecture/MODULE-ARCHITECTURE.md` (PR #1482)
//! - Scaffold shape from `GeneratorModule` (PR #1487)
//!
//! # Scope of this PR
//!
//! Only `chat/poll` ships in Rust today. The other three commands
//! (`chat/send`, `chat/analyze`, `chat/export`) are wired into the
//! dispatch table as fail-loud stubs that name follow-up PRs. The
//! TS implementations stay live on canary so consumers see no
//! regression; the kernel will start owning each command as its
//! follow-up PR lands.
//!
//! The reason for the staged migration: `chat/poll` is the cleanest
//! outlier (pure read, no airc, no media side-effects) which lets us
//! validate the cross-module call pattern (chat → data via the kernel
//! executor) without dragging substrate + media into the first
//! migration. Subsequent commands fold in real behavior incrementally.
//!
//! # Cross-module call pattern
//!
//! `chat/poll` doesn't open a database connection itself — it calls
//! `data/query` via the kernel executor (the same global executor any
//! other module reaches for at call time). Chat is blind to which
//! adapter implements the storage; the data module routes the query
//! per its own resolution rules. This is exactly the composition
//! pattern from `MODULE-ARCHITECTURE.md` §5: commands call commands;
//! modules don't know about each other beyond the command surface.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::runtime::{command_executor::CommandExecutor, CommandResult, LateBound, ModuleConfig, ModulePriority, ServiceModule};
use crate::sdk_codegen::DynCommand;

pub mod types;

use types::{ChatPollParams, ChatPollResult, ChatSendParams, ChatSendResult, CHAT_MESSAGES_COLLECTION, DEFAULT_POLL_LIMIT};

mod chat_data_handle;
pub use chat_data_handle::*;
mod chat_module;
pub use chat_module::*;

#[cfg(test)]
mod tests;
