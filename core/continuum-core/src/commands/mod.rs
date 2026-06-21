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

pub mod catalog;
