//! `commands/` — the compartmentalized command tree.
//!
//! Per [docs/architecture/COMMAND-ORGANIZATION.md], a command is a self-contained
//! unit (spec + handler + tests) in its own file, self-registering via
//! `register_command!` / `register_stateless_command!`. There is NO central list:
//! adding a file here makes the command appear in the registry, the persona tool
//! surface, the ACL, codegen, and `uu` — with no edit anywhere else.
//!
//! Stateless commands (no deps) live here and self-route with zero host-module
//! ceremony. Dep-holding commands stay with the module that owns their state and
//! are exposed through that module's `commands()`.

pub mod access;
mod tool_aliases;
pub mod adapter;
pub mod agent;
pub mod ai;
pub mod airc;
pub mod auth;
pub mod avatar;
pub mod benchmark;
pub mod benchmark_import;
pub mod benchmark_verify;
pub use continuum_dreaming::commands::genome_recall;
pub mod genome_push;
pub use continuum_dreaming::commands::genome_share;
pub mod capacity;
pub mod catalog;
pub mod chat;
pub mod code;
pub mod cognition;
pub mod command;
pub mod data;
pub use continuum_dreaming::commands::dataset;
pub mod desktop;
pub mod embedding;
pub mod events;
pub mod focus;
pub mod generator;
pub use continuum_dreaming::commands::genome;
pub mod gpu;
pub mod gym;
pub mod health;
pub mod help;
pub use continuum_academy::hf;
pub mod inference;
pub mod interface;
pub mod keys;
pub mod log;
pub mod mcp;
pub mod memory;
pub mod migration;
pub mod models;
pub mod persona;
pub mod persona_roster;
pub mod identity_whoami;
pub mod live_token;
pub mod presence_directory;
pub mod recipe_run;
pub mod plasticity;
pub mod ports;
pub mod rag;
pub mod resources;
pub mod runtime;
pub use continuum_perception::search;
pub use crate::modules::serving_daemon::commands as serving;
pub mod system;
pub mod tool;
pub use continuum_dreaming::commands::training_trigger;
pub use continuum_mind::vdd::commands as vdd;
pub mod vector;
pub mod vision;
pub mod web;
