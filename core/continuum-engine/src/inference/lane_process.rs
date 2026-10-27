//! lane_process.rs — the shared unix-process primitives the two lane-tracking
//! modules ([`crate::inference::lane_pidfile`] canonical-port reclaim and
//! [`crate::inference::lane_registry`] orphan accounting) both need.
//!
//! ## Why this is its own module
//!
//! Both modules answer a "is this recorded pid still one of MY llama-servers, and
//! if so may I reap it" question, and both must obey the SAME never-blind-kill
//! safety: verify the pid is actually a `llama-server` (via `ps`) before sending a
//! signal, because a recorded pid can be STALE (the process died and the OS reused
//! its number for something unrelated). Duplicating `is_alive` / `kill9` /
//! `command_name` across the two callers would be the exact compression violation
//! we forbid — one decision ("is this pid a live llama-server we own"), one place.
//!
//! No new dependency: `libc` is already in the tree; identity via `ps -p <pid> -o
//! comm=` works identically on macOS and Linux.

/// True if `pid` names a live process — the ONE liveness check (`deploy_claim::owner_alive`
/// and the grid's job table ask it too). The process table itself via `sysinfo`, on every
/// platform. The Windows arm used to scrape `tasklist` output for the pid as a SUBSTRING, so pid
/// 42 read alive whenever any pid containing "42" existed (the bug deploy_claim had already
/// fixed with exactly this lookup).

mod kill9;
pub use kill9::*;
mod parse_tasklist_image_name;
pub use parse_tasklist_image_name::*;

#[cfg(test)]
mod tests;
