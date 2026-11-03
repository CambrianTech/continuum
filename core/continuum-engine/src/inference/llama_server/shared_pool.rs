//! ONE SHARED POOL (2026-11-03, Joel: "Slot counts should never require shutdown"). The
//! engine starts with `--kv-unified`: `-c` is one KV pool and `--parallel` the most sequences
//! that may draw on it; each sequence can use the whole pool and an idle one holds none. Which
//! minds hold how much is decided at runtime, never by a relaunch: the M5 loaded its model 40
//! times as the plan's slot count moved 1 -> 2 -> 1, every reload wiping every mind's cache.
//!
//! The one owner of that decision: it puts the flag on a launch, and reads it back off a
//! running engine's argv so the reconcile can move an engine on fixed slots to the pool once.

use crate::inference::lane_args::LaneInvocation;

/// llama-server's switch for one KV pool shared by every sequence (`-kvu` is its short form).
const SHARED_POOL: &str = "--kv-unified";
const SHARED_POOL_SHORT: &str = "-kvu";

/// Launch on the shared pool: every sequence draws on all of `-c`.
pub(super) fn on_shared_pool(mut invocation: LaneInvocation) -> LaneInvocation {
    invocation.args.push(SHARED_POOL.to_string());
    invocation
}

/// Was this argv launched on the shared pool?
pub(super) fn launched_on_shared_pool(argv: &[String]) -> bool {
    argv.iter().any(|a| a == SHARED_POOL || a == SHARED_POOL_SHORT)
}
