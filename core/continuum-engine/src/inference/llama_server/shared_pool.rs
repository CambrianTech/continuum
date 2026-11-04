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

/// On the shared pool an idle sequence keeps her cells. llama.cpp's default (`cache_idle_slots`)
/// saves every idle slot to the host prompt cache and, under `--kv-unified`, CLEARS it whenever
/// any other slot starts a task (server-context.cpp [TAG_IDLE_SLOT_CLEAR]). The core owns paging
/// (the page ledger, `/slots/{id}?action=save|restore`), so the clear happened behind it and the
/// ledger read her slot warm while it was empty: on the M5 (2026-11-04, 13:30-13:35Z) every
/// resident turn that followed another resident's launch prefilled in full (Benchy 59,802
/// tokens, Aris 20,962 at 38 tok/s, Aiko 62,026), while back-to-back turns reused 28-38k each.
const KEEP_IDLE_SLOTS: &str = "--no-cache-idle-slots";

/// Launch on the shared pool: every sequence draws on all of `-c`, and an idle one keeps what
/// she holds until the core pages her.
pub(super) fn on_shared_pool(mut invocation: LaneInvocation) -> LaneInvocation {
    invocation.args.push(SHARED_POOL.to_string());
    invocation.args.push(KEEP_IDLE_SLOTS.to_string());
    invocation
}

/// Was this argv launched the way [`on_shared_pool`] launches? An engine on the pool that still
/// clears idle slots is not: the reconcile moves it by its one relaunch, as it moves an engine on
/// fixed slots.
pub(super) fn launched_on_shared_pool(argv: &[String]) -> bool {
    argv.iter().any(|a| a == SHARED_POOL || a == SHARED_POOL_SHORT) && argv.iter().any(|a| a == KEEP_IDLE_SLOTS)
}

#[cfg(test)]
mod tests {
    use super::*;

    // regression for the M5, 2026-11-04: an engine on the pool without --no-cache-idle-slots
    // cleared every idle resident's slot behind the core's page ledger.
    // what this catches: the pool launch dropping the flag, or an engine on the pool that still
    // clears idle slots read back as already moved (so it would never relaunch to get it).
    #[test]
    fn the_shared_pool_launch_keeps_idle_slots_and_the_read_back_requires_it() {
        let argv = |args: &[&str]| args.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        assert!(launched_on_shared_pool(&argv(&["-c", "449755", "--kv-unified", "--no-cache-idle-slots"])));
        assert!(launched_on_shared_pool(&argv(&["-kvu", "--no-cache-idle-slots"])));
        assert!(!launched_on_shared_pool(&argv(&["-c", "449755", "--kv-unified"])), "clears idle slots: moved once");
        assert!(!launched_on_shared_pool(&argv(&["-c", "449755", "--no-cache-idle-slots"])), "fixed slots: moved once");
    }
}
