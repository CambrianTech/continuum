//! ONE UPWARD RELAUNCH PER ENGINE, until the engine resizes its pool live (S2 DEBT, card e9724a7d).
//!
//! The engine launches once on a shared pool and is never relaunched for a window or lane change,
//! so whatever the launch reads is kept for the engine's life. Every launch reads a transient: on
//! the M5 on 2026-11-03 the cold boot plan gave 5 x 2,048 (b4be7bc59), the host's physical total
//! gave 5 x 262,144 at 91% memory pressure (590df4bc3), and the live budget sampled while the old
//! core's memory came back gave 16,640 (81aef1844). All three were rolled back. A launch cannot be
//! right by construction, so the plan's rule for every downward adaptation applies ("control both
//! ways"): the launched pool gets ONE scheduled upward test. When the first steady plan proves a
//! strictly larger pool, the serving daemon asks for it, and the next ensure relaunches into it,
//! once. Downward never relaunches. This bends the one-launch rule and goes when S2 lands.
//!
//! The daemon decides (it holds the plan and its budget); the engine's owner acts, as it does for
//! the shared-pool move and the engine convergence: a request keyed by the engine process it names.

/// The engine process the serving daemon has asked to grow, once. A request for any other process
/// is stale and never acts.
static REQUESTED: parking_lot::Mutex<Option<u32>> = parking_lot::Mutex::new(None);

/// Admission held for the relaunch, with the engine process it was taken for (card 2540194e):
/// the engine is idle by construction rather than by a sample. It is held across the relaunch
/// until the new engine answers, because a request released into a relaunching engine just fails.
static HELD: parking_lot::Mutex<Option<(u32, crate::inference::resource_admission::Quiesced)>> = parking_lot::Mutex::new(None);

/// Ask the engine owner to relaunch engine `pid` once into the larger pool its next target carries,
/// with admission already quiesced (`held`) so nothing is generating when it does.
pub fn request_pool_growth(pid: u32, held: crate::inference::resource_admission::Quiesced) {
    *HELD.lock() = Some((pid, held));
    request(pid);
}

fn request(pid: u32) {
    *REQUESTED.lock() = Some(pid);
}

/// Whether engine `pid` has a growth relaunch requested and not yet decided.
pub fn growth_requested(pid: u32) -> bool {
    *REQUESTED.lock() == Some(pid)
}

/// After an ensure: end the hold once its relaunch was decided (the request is spent, so the
/// relaunch ran to a serving engine or failed). A hold whose request is still pending stays.
pub fn end_hold_if_decided() {
    let mut held = HELD.lock();
    if held.as_ref().is_some_and(|(pid, _)| !growth_requested(*pid)) {
        end(held.take(), "relaunch_ended");
    }
}

/// End a hold taken for an engine process other than `running`, the one now serving: a new engine
/// answers, whatever replaced the old one. Never while no engine serves (mid-relaunch).
pub fn end_stale_hold(running: u32) {
    let mut held = HELD.lock();
    if held.as_ref().is_some_and(|(pid, _)| *pid != running) {
        end(held.take(), "engine_replaced");
    }
}

fn end(held: Option<(u32, crate::inference::resource_admission::Quiesced)>, reason: &'static str) {
    if let Some((pid, quiesced)) = held {
        continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.pool.grow_held",
            pid = u64::from(pid),
            held_ms = quiesced.held_ms(),
            reason = reason,
            "admission reopens: the pool's one relaunch held it this long (drain + relaunch to ready)"
        );
    }
}

/// Whether the running engine is due its one growth relaunch: the daemon asked for this process,
/// and the target's pool is strictly larger than what the process reports holding. Unknown (no
/// process, no reported pool) is never a growth. A known non-growing target settles
/// the request so end_hold_if_decided reopens admission after ensure returns.
pub(super) fn growth_due(running_pid: Option<u32>, running_pool: Option<u32>, target_pool: u64) -> bool {
    let (Some(pid), Some(running)) = (running_pid, running_pool) else { return false };
    let mut requested = REQUESTED.lock();
    if *requested != Some(pid) {
        return false;
    }
    if target_pool <= u64::from(running) {
        // A known refusal is terminal too. Otherwise the drained admission hold
        // waits forever for a relaunch that this owner has decided not to perform.
        *requested = None;
        return false;
    }
    true
}

/// The growth was decided for `pid`: the request is spent, so a failed or partial relaunch never
/// loops. The next engine process starts with no request.
pub(super) fn spend(pid: u32) {
    let mut requested = REQUESTED.lock();
    if *requested == Some(pid) {
        *requested = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Both tests drive the same process-global request and hold owner.
    static OWNER: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    // what this catches: the bridge relaunching more than once, downward, or for a process it
    // was not asked about. One request names one process; it acts only while the target is larger
    // and only until it is spent.
    #[tokio::test]
    async fn an_engine_grows_once_upward_and_only_when_asked() {
        let _owner = OWNER.lock().await;
        // A pid no other test in this binary uses.
        let pid = 4_000_000_001;
        assert!(!growth_due(Some(pid), Some(16_640), 257_536), "never asked: no growth");
        request(pid);
        assert!(!growth_due(Some(pid + 1), Some(16_640), 257_536), "asked for another process: stale");
        assert!(!growth_due(Some(pid), Some(257_536), 257_536), "not strictly larger: no relaunch");
        assert!(!growth_requested(pid), "equal target settles the request");
        request(pid);
        assert!(!growth_due(Some(pid), Some(300_000), 257_536), "downward never relaunches");
        assert!(!growth_requested(pid), "smaller target settles the request");
        request(pid);
        assert!(!growth_due(Some(pid), None, 257_536), "an unknown running pool is never a growth");
        assert!(growth_requested(pid), "unknown readback is not a refusal");
        assert!(growth_due(Some(pid), Some(16_640), 257_536), "asked, and the target is larger: grow");
        spend(pid);
        assert!(!growth_due(Some(pid), Some(16_640), 257_536), "spent: once per engine");
    }

    // what this catches (the M5, 2026-11-04 18:00:46-18:16Z: admission held 15 min 14 s, zero
    // generations): a hold whose release depends on an action its owner refuses. The daemon
    // quiesced admission and asked engine `pid` to grow; the ensure's launch planner then carried
    // a target no larger than the running pool, so `growth_due` was false, the request was never
    // spent, and `end_hold_if_decided` kept the hold for good. Driven through the reconcile's own
    // calls (llama_server/reconcile.rs: growth_due -> spend; serving launch.rs: end_hold_if_decided).
    #[tokio::test]
    async fn a_hold_is_released_when_the_relaunch_it_waits_for_is_refused() {
        let _owner = OWNER.lock().await;
        let hold = crate::capacity::dyno::recorded::Recorded::load()
            .expect("test: the recorded 2026-11-04 data parses")
            .unreleased_hold
            .value;
        let pid = 4_000_000_006; // a pid no other test in this binary uses
        let held = crate::inference::resource_admission::quiesced_fixture()
            .await
            .expect("test: a fresh gate quiesces");
        request_pool_growth(pid, held);
        // The ensure: the planner's target is not larger than the running pool, so no relaunch.
        let relaunch = growth_due(Some(pid), Some(hold.running_pool), hold.target_pool);
        if relaunch {
            spend(pid);
        }
        end_hold_if_decided();
        let still_held = HELD.lock().as_ref().is_some_and(|(held_for, _)| *held_for == pid);
        let still_requested = growth_requested(pid);
        // Leave nothing behind for another test, whatever the verdict.
        spend(pid);
        end_hold_if_decided();
        assert!(!relaunch, "test: the recorded ensure refused the relaunch");
        assert!(
            !still_held && !still_requested,
            "admission stays held (held {still_held}, requested {still_requested}) for a relaunch the planner refused \
             (the M5 held it {} ms with zero generations)",
            hold.held_ms
        );
    }
}
