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
//! # What it serves
//!
//! `chat/poll`, `chat/history` and `chat/send` run here in Rust: reads page by a
//! `(timestamp, id)` keyset (`keyset_page`), never by offset. `chat/analyze` and
//! `chat/export` are still unmigrated and refuse loudly, naming that (`chat_module.rs`).
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

/// Bus event carrying a posted message. The wire name is the chat module's (its persist
/// listener consumes it); the EMITTER (`airc::inbound_attach`) and the positron projection
/// import it, so one string names the event (compression principle).
pub const CHAT_POSTED: &str = "chat:posted";

use types::{ChatPollParams, ChatPollResult, ChatSendParams, ChatSendResult, CHAT_MESSAGES_COLLECTION, DEFAULT_POLL_LIMIT};

pub mod arrival;
pub mod line_store;
pub mod drain;
pub mod import_watermark;
pub mod orm_entities;
mod chat_data_handle;
pub use chat_data_handle::*;
mod chat_module;
use chat_module::*;
mod keyset_page;
pub mod commands;
use keyset_page::{chronological, PageDirection};

#[cfg(test)]
mod tests;
