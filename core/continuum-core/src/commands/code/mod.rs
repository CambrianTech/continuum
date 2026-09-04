//! `code/*` — the persona's workspace hands, grouped under the one domain a coding
//! citizen reaches into: execute code, version-control it, and (slice 2) read/write
//! files. The namespace is **conventional and predictable** — `code/run`,
//! `code/git/<verb>`, `code/fs/<verb>` — so a persona reaches for a tool by the name
//! it would already guess. Predictability is the point: a discoverable, expected
//! namespace lowers the mistake rate when the mind picks a tool, exactly as the
//! persona tool surface intends (offer == authorized).
//!
//! Subtree map:
//! - [`run`] — `code/run`: execute a snippet, observe stdout/stderr/exit (stateless).
//! - [`cargo`] — `code/cargo/<verb>`: the persona's Rust hands — `cargo check`/`test`
//!   over the caller's workspace, returning structured diagnostics (hand + grader).
//! - [`git`] — `code/git/<verb>`: version control over the caller's own workspace.
//! - `fs/` — `code/fs/<verb>`: file operations (slice 2; today in `modules::code_commands`).

pub mod cargo;
pub mod git;
pub mod github;
pub mod run;
