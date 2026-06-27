//! `commands/` — the compartmentalized command tree.
//!
//! Per [docs/architecture/COMMAND-ORGANIZATION.md], a command is a self-contained
//! unit (spec + handler + tests) in its own file, self-registering via
//! `register_command!` / `register_stateless_command!`. There is NO central list:
//! adding a file here makes the command appear in the registry, the persona tool
//! surface, the ACL, codegen, and `cu` — with no edit anywhere else.
//!
//! Stateless commands (no deps) live here and self-route with zero host-module
//! ceremony. Dep-holding commands stay with the module that owns their state and
//! are exposed through that module's `commands()`.

pub mod adapter;
pub mod agent;
pub mod airc;
pub mod auth;
pub mod avatar;
pub mod ai;
pub mod catalog;
pub mod code;
pub mod command;
pub mod data;
pub mod dataset;
pub mod events;
pub mod generator;
pub mod genome;
pub mod gpu;
pub mod help;
pub mod inference;
pub mod log;
pub mod memory;
pub mod migration;
pub mod models;
pub mod persona;
pub mod plasticity;
pub mod rag;
pub mod runtime;
pub mod search;
pub mod serving;
pub mod system;
pub mod tool_parsing;
pub mod vdd;
pub mod vector;
pub mod vision;
