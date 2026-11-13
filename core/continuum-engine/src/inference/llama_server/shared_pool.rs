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

/// How a running engine was launched, read off its own argv. Two questions ask it, and each asks
/// its own (Cormac on #5764): the grants and the served window ask whether the KV is ONE pool
/// ([`PoolLaunch::kv_shared`]); the reconcile asks whether the launch is the one
/// [`on_shared_pool`] makes now ([`PoolLaunch::current`]), and moves it by its one relaunch if not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoolLaunch {
    /// Fixed slots: `-c` divided among `--parallel` (every node before 2026-11-03).
    FixedSlots,
    /// One shared pool whose engine still clears an idle slot when another launches.
    SharedClearingIdle,
    /// One shared pool whose idle slots keep their KV: the launch [`on_shared_pool`] makes.
    Shared,
}

impl PoolLaunch {
    /// Every sequence draws on one pool, so a grant is a share of it.
    pub fn kv_shared(self) -> bool {
        !matches!(self, PoolLaunch::FixedSlots)
    }

    /// The launch [`on_shared_pool`] makes now; anything else is moved by one relaunch.
    pub fn current(self) -> bool {
        self == PoolLaunch::Shared
    }
}

/// How this argv was launched.
pub(super) fn pool_launch(argv: &[String]) -> PoolLaunch {
    let shared = argv.iter().any(|a| a == SHARED_POOL || a == SHARED_POOL_SHORT);
    let keeps_idle = argv.iter().any(|a| a == KEEP_IDLE_SLOTS);
    match (shared, keeps_idle) {
        (false, _) => PoolLaunch::FixedSlots,
        (true, false) => PoolLaunch::SharedClearingIdle,
        (true, true) => PoolLaunch::Shared,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // regression for the M5, 2026-11-04: an engine on the pool without --no-cache-idle-slots
    // cleared every idle resident's slot behind the core's page ledger.
    // what this catches: the pool launch dropping the flag; an engine on the pool that still
    // clears idle slots read as current (it would never relaunch to get the flag) or as fixed
    // slots (its grants would be a slot's, not a share of the pool, until a relaunch landed).
    #[test]
    fn the_pool_launch_keeps_idle_slots_and_each_reader_asks_its_own_question() {
        let argv = |args: &[&str]| args.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        let current = pool_launch(&argv(&["-c", "449755", "--kv-unified", "--no-cache-idle-slots"]));
        assert_eq!(current, PoolLaunch::Shared);
        assert_eq!(pool_launch(&argv(&["-kvu", "--no-cache-idle-slots"])), PoolLaunch::Shared);
        let clearing = pool_launch(&argv(&["-c", "449755", "--kv-unified"]));
        assert_eq!(clearing, PoolLaunch::SharedClearingIdle);
        assert!(clearing.kv_shared() && !clearing.current(), "its KV is shared; its launch is moved once");
        let fixed = pool_launch(&argv(&["-c", "449755", "--no-cache-idle-slots"]));
        assert!(!fixed.kv_shared() && !fixed.current());
        assert!(current.kv_shared() && current.current());
    }
}
