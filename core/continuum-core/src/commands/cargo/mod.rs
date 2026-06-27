//! `cargo/*` — Rust toolchain commands with structured output.
//!
//! Per [PERSONA-AS-DEVELOPER-GAP.md](../../../../../../docs/planning/PERSONA-AS-DEVELOPER-GAP.md)
//! Priority 2: cargo wrappers with structured envelopes, closing the
//! iteration-loop seam so a persona can build/test its own scaffolded
//! modules with the same feedback density a human gets from
//! `cargo build` / `cargo test`.
//!
//! # Migrated from `modules/cargo`
//!
//! These were `ServiceModule::handle_command` arms on `CargoModule`.
//! They are now stateless `ActionCommand`s on the single command
//! registry (`command_registry()`), which means they reach the
//! persona tool surface, the ACL, codegen, and `cu` by construction —
//! the legacy module saw none of those. Both are `Privileged`: they
//! spawn heavy, side-effecting cargo subprocesses, so they're offered
//! only where the caller's trust authorizes it, not on the
//! unconditional `AiSafe` surface.
//!
//! # Composability with the grid
//!
//! Both result types serialize to flat camelCase JSON envelopes. A
//! persona on machine A can call `cargo/test` against a module a
//! persona on machine B just authored — the result envelope routes
//! back over airc's grid without any cargo-specific protocol. See
//! [[alignment-via-substrate-economics]].
//!
//! # What these commands do NOT do
//!
//! - **Do NOT manage per-persona workspaces.** Take optional
//!   `working_dir` (default: process cwd). Worktree isolation is an
//!   orthogonal layer (continuum task #49).
//! - **Do NOT stream output line-by-line.** Return a single envelope
//!   at the end. Streaming is PERSONA-AS-DEVELOPER-GAP.md priority 3+4.
//! - **Do NOT cap cargo's own concurrency.** cargo manages its own
//!   target-dir lock; concurrent invocations against the same target
//!   dir serialize at cargo's level.

pub mod build;
pub mod exec;
pub mod test;
pub mod types;
