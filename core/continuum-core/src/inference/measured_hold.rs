//! The measured-work hold — Law 3 of the restore economy: **measured work holds
//! the core**, and every model-touching path respects it at ONE seam.
//!
//! ## The defect this closes (measured 2026-08-28)
//!
//! `agent/solve` quiesces the other citizens' service loops
//! (`quiesce_others`, verified `quiesced_peers=3`) — and the GPU still spent
//! ~half its generations on `dream-belief-review` (52 of 109
//! `inference.prefill.complete` rows) DURING a measured solve. Dreams don't run
//! in the service loop the lease gates; they spawn their own tasks and walked
//! straight past the quiesce onto the ONE serving slot, evicting the solve's
//! warm KV every time. On this geometry an eviction is a ~32.9s re-prefill vs a
//! ~0.1s restore (~330×, measured), so the bypass didn't slow the solve — it
//! halved the machine.
//!
//! Gating at each background producer would be whack-a-mole (the next
//! `tokio::spawn` forgets). The adapter is the one seam every generation
//! already crosses with its `purpose` in hand, so the hold is consulted there
//! and nowhere else. Producers never need to know it exists.
//!
//! ## What defers, what rides through
//!
//! [`SlotClass`] is the decision vocabulary (one purpose→class map,
//! `slots::class_for`):
//! - **Turn** — never deferred. A real citizen turn outranks a measurement
//!   (a human talking to a citizen must never dead-air), and the existing
//!   quiesce lease already keeps idle citizens' turns from firing. Blocking
//!   Turn here could also deadlock the measured drive itself.
//! - **Sidecar** — never deferred: sidecars ride a live turn, and on quiesced
//!   geometry the only live turn is the measured one.
//! - **Background / Probe** — deferred while someone ELSE holds. Dreams are
//!   idle-time work by definition ([[idle-is-self-directed-free-time]]); a
//!   measured solve means the core is not idle. The holder's OWN background
//!   traffic passes (`caller == holder`).
//!
//! ## Bounded, never a silent park
//!
//! Deferral waits on a notify with a hard ceiling. A leaked lease (process
//! killed mid-drop, a bug) must not park dreaming forever — at the ceiling the
//! waiter PROCEEDS and says so loudly (`inference.hold.defer_ceiling`), which
//! is a lease-leak detector, not a policy.
//!
//! ## Probes (VDD receipt)
//!
//! `inference.hold.acquired` / `.released` / `.deferred` / `.resumed` /
//! `.defer_ceiling`. The phase receipt reads `inference.prefill.complete`
//! during a hold: ZERO Background-class rows is the pass condition.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use uuid::Uuid;

use super::slots::SlotClass;

/// A leaked lease must not park background cognition forever: at this ceiling a
/// deferred waiter proceeds LOUDLY. Solves run 1–3h, so 4h defers real dreams
/// behind real work while still unsticking a leak within one operator shift.
/// A refusal-to-hang, not a tuning knob.
pub const DEFER_CEILING: Duration = Duration::from_secs(4 * 60 * 60);

/// Re-check cadence while deferred. Wakeups are notify-driven (release wakes
/// waiters instantly); this tick only bounds how stale a missed notify can be.
const DEFER_TICK: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldInfo {
    pub holder: Uuid,
    pub run_id: String,
    /// Generation counter — lets a stale lease's Drop recognise that a NEWER
    /// hold replaced it and leave that newer hold untouched.
    generation: u64,
}

/// The hold's state + wakeup machinery. Public-in-crate only for the
/// injected-core test pattern (`spill.rs` precedent): production uses the ONE
/// process-global cell; tests construct their own so parallel tests cannot race
/// each other through a shared singleton (that race was observed immediately —
/// the first parallel run of this module's own tests).
pub struct HoldCell {
    state: Mutex<Option<HoldInfo>>,
    notify: tokio::sync::Notify,
    generations: AtomicU64,
}

impl HoldCell {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(None),
            notify: tokio::sync::Notify::new(),
            generations: AtomicU64::new(0),
        }
    }
}

impl Default for HoldCell {
    fn default() -> Self {
        Self::new()
    }
}

fn cell() -> &'static HoldCell {
    static CELL: OnceLock<HoldCell> = OnceLock::new();
    CELL.get_or_init(HoldCell::new)
}

/// RAII lease: dropping releases the hold (if it is still ours) and wakes every
/// deferred waiter.
pub struct HoldLease {
    generation: u64,
    cell: &'static HoldCell,
}

impl Drop for HoldLease {
    fn drop(&mut self) {
        let c = self.cell;
        let mut guard = c.state.lock().unwrap_or_else(|p| p.into_inner());
        // Only clear OUR hold. A newer acquire (last-wins) must survive a stale
        // lease's late drop — without the generation check, a leaked old lease
        // dropping late would silently release the live measurement's hold.
        if guard.as_ref().is_some_and(|h| h.generation == self.generation) {
            let released = guard.take();
            drop(guard);
            if let Some(h) = released {
                crate::probe!(
                    class = "inference.hold.released",
                    holder = %h.holder,
                    run_id = %h.run_id,
                    "measured-work hold released — deferred background cognition wakes now"
                );
            }
            c.notify.notify_waiters();
        }
    }
}

/// Take the measured-work hold. Last-wins: a second acquire replaces the first
/// (and says so) rather than deadlocking two measurements against each other —
/// the solve driver serializes measured work anyway, so a replacement here is a
/// bug being made VISIBLE, not a supported mode.
pub fn acquire(holder: Uuid, run_id: &str) -> HoldLease {
    acquire_in(cell(), holder, run_id)
}

/// Injected-core acquire — tests pass their own leaked cell.
pub fn acquire_in(c: &'static HoldCell, holder: Uuid, run_id: &str) -> HoldLease {
    let generation = c.generations.fetch_add(1, Ordering::Relaxed) + 1;
    let mut guard = c.state.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(prev) = guard.as_ref() {
        crate::probe!(
            class = "inference.hold.replaced",
            prev_holder = %prev.holder,
            prev_run = %prev.run_id,
            new_holder = %holder,
            new_run = %run_id,
            "a second measured hold replaced a live one — measured work should be \
             serial; if both runs are real this is the bug to chase"
        );
    }
    *guard = Some(HoldInfo {
        holder,
        run_id: run_id.to_string(),
        generation,
    });
    drop(guard);
    crate::probe!(
        class = "inference.hold.acquired",
        holder = %holder,
        run_id = %run_id,
        "measured-work hold acquired — Background/Probe generations defer until release"
    );
    HoldLease { generation, cell: c }
}

/// The live hold, if any.
pub fn current() -> Option<HoldInfo> {
    current_in(cell())
}

fn current_in(c: &HoldCell) -> Option<HoldInfo> {
    c.state.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// The PURE decision — table-tested, no clock, no locks. `true` = this
/// generation must wait for the hold to release.
pub fn should_defer(class: SlotClass, hold: Option<&HoldInfo>, caller: Option<Uuid>) -> bool {
    let Some(hold) = hold else {
        return false; // no measurement in flight — everything passes
    };
    match class {
        // A real turn or its sidecars never wait (see module doc: outranking +
        // deadlock-freedom; idle citizens' turns are already quiesced upstream).
        SlotClass::Turn | SlotClass::Sidecar => false,
        // Background/Probe defer unless they belong to the measured run itself.
        SlotClass::Background | SlotClass::Probe => caller != Some(hold.holder),
    }
}

/// How a deferred generation eventually proceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeferOutcome {
    /// No hold, or the class rides through: no waiting happened.
    Passed,
    /// Waited and the hold released.
    Resumed,
    /// Waited to the ceiling and proceeded anyway — a lease-leak detector
    /// firing, not a policy success.
    CeilingProceed,
}

/// Await the hold at the adapter seam. See [`should_defer`] for the decision;
/// this adds the bounded wait + probes around it.
pub async fn defer_while_held(
    class: SlotClass,
    caller: Option<Uuid>,
    purpose: Option<&str>,
) -> DeferOutcome {
    defer_while_held_with_ceiling(class, caller, purpose, DEFER_CEILING).await
}

/// Ceiling-injected core so the timeout path is testable in milliseconds.
pub async fn defer_while_held_with_ceiling(
    class: SlotClass,
    caller: Option<Uuid>,
    purpose: Option<&str>,
    ceiling: Duration,
) -> DeferOutcome {
    defer_in(cell(), class, caller, purpose, ceiling).await
}

/// Injected-core defer — the loop the public entry points share.
async fn defer_in(
    c: &'static HoldCell,
    class: SlotClass,
    caller: Option<Uuid>,
    purpose: Option<&str>,
    ceiling: Duration,
) -> DeferOutcome {
    if !should_defer(class, current_in(c).as_ref(), caller) {
        return DeferOutcome::Passed;
    }
    let started = std::time::Instant::now();
    let held_by = current_in(c).map(|h| h.run_id).unwrap_or_default();
    crate::probe!(
        class = "inference.hold.deferred",
        traffic = %class.as_str(),
        purpose = %purpose.unwrap_or("-"),
        held_by = %held_by,
        "background generation deferring — measured work holds the core"
    );
    loop {
        if !should_defer(class, current_in(c).as_ref(), caller) {
            crate::probe!(
                class = "inference.hold.resumed",
                traffic = %class.as_str(),
                purpose = %purpose.unwrap_or("-"),
                waited_ms = started.elapsed().as_millis() as u64,
                "hold released — deferred generation proceeding"
            );
            return DeferOutcome::Resumed;
        }
        if started.elapsed() >= ceiling {
            crate::probe!(
                class = "inference.hold.defer_ceiling",
                traffic = %class.as_str(),
                purpose = %purpose.unwrap_or("-"),
                waited_ms = started.elapsed().as_millis() as u64,
                "defer ceiling reached with the hold still set — proceeding LOUDLY; \
                 if the measured run is not actually alive this is a leaked lease"
            );
            return DeferOutcome::CeilingProceed;
        }
        let remaining = ceiling.saturating_sub(started.elapsed()).min(DEFER_TICK);
        let _ = tokio::time::timeout(remaining, c.notify.notified()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hold_of(holder: Uuid) -> HoldInfo {
        HoldInfo {
            holder,
            run_id: "claim-test".into(),
            generation: 1,
        }
    }

    // what this catches: the defer table drifting. Turn/Sidecar must NEVER wait
    // (a human's turn outranks a measurement, and blocking the measured drive's
    // own traffic would deadlock it); Background/Probe must wait exactly when
    // someone ELSE holds. The 2026-08-28 defect was the absence of this table:
    // dream-belief-review (Background) took 52 of 109 generations DURING a
    // measured solve because nothing at the adapter seam said no.
    #[test]
    fn the_defer_table_holds() {
        let holder = Uuid::new_v4();
        let other = Uuid::new_v4();
        let h = hold_of(holder);

        for class in [SlotClass::Turn, SlotClass::Sidecar] {
            assert!(!should_defer(class, Some(&h), Some(other)), "{class:?} never waits");
            assert!(!should_defer(class, None, Some(other)));
        }
        for class in [SlotClass::Background, SlotClass::Probe] {
            assert!(
                should_defer(class, Some(&h), Some(other)),
                "{class:?} from another mind defers during a hold"
            );
            assert!(
                should_defer(class, Some(&h), None),
                "anonymous {class:?} defers too — no caller is not the holder"
            );
            assert!(
                !should_defer(class, Some(&h), Some(holder)),
                "the holder's own {class:?} rides through"
            );
            assert!(!should_defer(class, None, Some(other)), "no hold, no wait");
        }
    }

    // what this catches: the RAII lifecycle — and specifically the stale-drop
    // hazard. Without the generation check, a leaked OLD lease dropping late
    // would silently release the LIVE measurement's hold, re-opening the exact
    // dream-clobber this module exists to close.
    #[tokio::test]
    async fn a_stale_lease_cannot_release_a_newer_hold() {
        // Own cell (leaked — 'static is the lease contract), so parallel tests
        // can never race through the process-global singleton.
        let c: &'static HoldCell = Box::leak(Box::new(HoldCell::new()));
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        let lease_a = acquire_in(c, a, "claim-a");
        assert_eq!(current_in(c).map(|h| h.holder), Some(a));

        let lease_b = acquire_in(c, b, "claim-b"); // last-wins replacement
        assert_eq!(current_in(c).map(|h| h.holder), Some(b));

        drop(lease_a); // stale — must NOT clear b's hold
        assert_eq!(
            current_in(c).map(|h| h.holder),
            Some(b),
            "a stale lease's late drop must not release the live hold"
        );

        drop(lease_b);
        assert!(current_in(c).is_none(), "the live lease's drop releases");
    }

    // what this catches: a waiter that never wakes, and a ceiling that never
    // fires. Both ends of the bounded-wait contract, in milliseconds.
    #[tokio::test]
    async fn deferred_waiters_wake_on_release_and_proceed_at_the_ceiling() {
        let holder = Uuid::new_v4();
        let other = Uuid::new_v4();

        let c: &'static HoldCell = Box::leak(Box::new(HoldCell::new()));

        // Release wakes the waiter promptly.
        let lease = acquire_in(c, holder, "claim-wake");
        let waiter = tokio::spawn(defer_in(
            c,
            SlotClass::Background,
            Some(other),
            Some("dream-belief-review"),
            Duration::from_secs(30),
        ));
        tokio::time::sleep(Duration::from_millis(50)).await;
        drop(lease);
        let outcome = tokio::time::timeout(Duration::from_secs(5), waiter)
            .await
            .expect("waiter must wake promptly on release")
            .unwrap();
        assert_eq!(outcome, DeferOutcome::Resumed);

        // A hold that never releases: the ceiling proceeds loudly instead of
        // parking forever (the lease-leak detector).
        let _leak = std::mem::ManuallyDrop::new(acquire_in(c, holder, "claim-leak"));
        let outcome = defer_in(
            c,
            SlotClass::Background,
            Some(other),
            Some("dream-belief-review"),
            Duration::from_millis(80),
        )
        .await;
        assert_eq!(outcome, DeferOutcome::CeilingProceed);
    }
}
