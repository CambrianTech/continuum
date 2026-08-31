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

pub mod adapter;
pub mod agent;
pub mod ai;
pub mod airc;
pub mod auth;
pub mod avatar;
pub mod benchmark;
pub mod benchmark_pause;
pub mod benchmark_round;
pub mod benchmark_round_report;
pub mod benchmark_verify;
pub mod genome_recall;
pub mod genome_share;
pub mod capacity;
pub mod catalog;
pub mod chat;
pub mod code;
pub mod cognition;
pub mod command;
pub mod data;
pub mod dataset;
pub mod desktop;
pub mod embedding;
pub mod events;
pub mod focus;
pub mod generator;
pub mod genome;
pub mod gpu;
pub mod gym;
pub mod health;
pub mod help;
pub mod hf;
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
pub mod presence_directory;
pub mod recipe_run;
pub mod plasticity;
pub mod rag;
pub mod resources;
pub mod runtime;
pub mod search;
pub mod serving;
pub mod system;
pub mod tool;
pub mod tool_parsing;
pub mod training_trigger;
pub mod vdd;
pub mod vector;
pub mod vision;
pub mod web;
