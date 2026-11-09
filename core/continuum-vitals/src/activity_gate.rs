//! The ACTIVITY GATE — boredom as substrate (#2561).
//!
//! Joel, 2026-08-29: dream state is *"simply inactivity vs activity. No
//! heuristic sketchy-inorganics… Think of how the human mind works and apply it
//! efficiently to our computer-architecture-focused approach, like we did
//! caches… at the governor level. Both an RTOS and a mind."*
//!
//! The mind shape: the default-mode network is not *scheduled against* task
//! attention — it FAILS TO ARISE while task-positive systems are engaged, and a
//! salient demand doesn't queue behind a dream, it CANCELS it. The RTOS shape
//! (CONCURRENCY-STYLE-GUIDE): one owning task, `tokio::time::interval`, a
//! `watch::Sender<ActivitySnapshot>` published ON CHANGE, cheap atomic inputs,
//! zero hot-path cost, no polling consumers — waiters park on the watch.
//!
//! Inputs are EXISTING measurements only — nothing invented:
//! - a measured hold is active (`measured_hold::subscribe`) — benchmark/eval work
//! - deliberative generations in flight (`resource_admission::inflight_model_calls`)
//! - a DIRECTED turn ran recently (`note_directed`, stamped at the one seam that
//!   knows directedness — the deliberation faculty)
//!
//! Asymmetric hysteresis (the 718 law wearing a mind's face): EXIT boredom
//! instantly on any activity; ENTER it only after [`BOREDOM_AFTER`] of sustained
//! idle. Slow structure, instant demand-response.
//!
//! Consumers: dreams first (`dream_consolidation`), every background appetite
//! after (curriculum passes, backfills, belief review) — ONE boredom, never
//! per-subsystem timers. A consumer that must abandon work on wakeful demand
//! races its future against [`wait_for_activity`] and treats cancellation as a
//! clean discard — dreams are the cheapest resident in the paging economy:
//! re-dreamable, evicted first, never contended.

use continuum_base::time::EpochMs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use tokio::sync::watch;

/// Sustained-idle threshold before boredom ARISES. A hysteresis cadence (like a
/// monitor tick), not a behavior heuristic: entry is deliberately lazy so a
/// pause between turns never flaps the system into a dream it must instantly
/// abandon; exit needs no threshold at all — any activity ends boredom NOW.
pub const BOREDOM_AFTER: Duration = Duration::from_secs(90);

/// How long a directed turn keeps the system ACTIVE after it completes — covers
/// the think-act-observe residue between generations of one engagement, so a
/// conversation's inter-turn gaps don't read as idleness. One linger, structural.
pub const DIRECTED_LINGER: Duration = Duration::from_secs(30);

/// Cadence of the gate's own reconcile tick — the slow end of the RTOS ladder;
/// consumers never poll this, they park on the watch.
const GATE_TICK: Duration = Duration::from_secs(1);

/// What the organism is doing, as one word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityState {
    /// Task-positive systems engaged: a hold, an in-flight generation, or a
    /// recent directed turn. Background states must not arise.
    Active,
    /// Nothing engaged, but not for long enough to trust — the settling window
    /// where boredom is approaching but a returning demand costs nothing.
    SettlingIdle,
    /// Sustained inactivity: the default-mode window. Dreams (and every other
    /// background appetite) may arise — and abandon instantly when this ends.
    Bored,
}

/// The published snapshot — state plus how long it has held, for probes and
/// pacing decisions ("how bored are we").
#[derive(Debug, Clone, Copy)]
pub struct ActivitySnapshot {
    pub state: ActivityState,
    /// ms the CURRENT state has held, at publish time.
    pub held_ms: u64,
}

/// Last directed-turn completion, ms since epoch. Stamped by [`note_directed`]
/// from the deliberation faculty — the one seam that knows directedness.
static LAST_DIRECTED_MS: AtomicU64 = AtomicU64::new(0);

/// A directed generation just ran — the organism is engaged with someone.
pub fn note_directed() {
    // A pre-1970 clock reads 0 here: reads as never-directed, the safe side.
    LAST_DIRECTED_MS.store(EpochMs::now().millis(), Ordering::Release);
}

/// PURE state computation — the whole decision, testable without a runtime.
/// `idle_since_ms` is the caller-threaded timestamp of when inputs last read
/// active (the hysteresis memory); `None` means "was active this instant".
pub fn compute_state(
    hold_active: bool,
    inflight: usize,
    last_directed_ms: u64,
    now: u64,
    idle_since_ms: Option<u64>,
) -> ActivityState {
    let directed_recent =
        last_directed_ms > 0 && now.saturating_sub(last_directed_ms) < DIRECTED_LINGER.as_millis() as u64;
    if hold_active || inflight > 0 || directed_recent {
        return ActivityState::Active;
    }
    match idle_since_ms {
        Some(since) if now.saturating_sub(since) >= BOREDOM_AFTER.as_millis() as u64 => {
            ActivityState::Bored
        }
        _ => ActivityState::SettlingIdle,
    }
}

struct Gate {
    tx: watch::Sender<ActivitySnapshot>,
}

static GATE: OnceLock<Gate> = OnceLock::new();

/// True once [`spawn_activity_gate`] is running. When the gate task was never
/// spawned (unit tests, gate-less embedders), the boot snapshot would read
/// `Active` FOREVER and every waiter would park for the process's life — so
/// waiters treat an un-spawned gate as permissive and fall back to the seams
/// that guarded before it existed (the hold-defers).
static GATE_LIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn gate() -> &'static Gate {
    GATE.get_or_init(|| {
        let (tx, _rx) = watch::channel(ActivitySnapshot {
            state: ActivityState::Active, // boot = engaged until proven idle
            held_ms: 0,
        });
        Gate { tx }
    })
}

/// Subscribe to the gate — the ONE way background work learns what the
/// organism is doing. Parks cheaply; no consumer ever polls.
/// Publish a snapshot to every subscriber AND store it for late-comers. `send` would
/// return Err and DROP the value when no receiver is alive (the constructor's own
/// receiver is dropped on purpose), which left the stored state at boot's `Active`
/// forever: `prev` read Active every tick, `next` computed Bored, and the "state
/// changed" probe fired once a second for a change that never happened (BigMama,
/// 2026-09-05, 120 identical lines in 2 minutes). `send_replace` stores regardless —
/// the same shape the serving snapshot watch uses.
fn publish(snapshot: ActivitySnapshot) {
    gate().tx.send_replace(snapshot);
}

pub fn subscribe() -> watch::Receiver<ActivitySnapshot> {
    gate().tx.subscribe()
}

/// Current state, borrow-cheap (for probes and non-parking checks).
pub fn current() -> ActivitySnapshot {
    *gate().tx.borrow()
}

/// Spawn the gate's owning task (call once at boot, inside the runtime). The
/// RTOS shape: own task, interval, cheap reads, publish ON CHANGE with a probe.
pub fn spawn_activity_gate() {
    GATE_LIVE.store(true, Ordering::Release);
    let hold_rx = continuum_engine::inference::measured_hold::subscribe();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(GATE_TICK);
        ticker.tick().await;
        let mut idle_since: Option<u64> = None;
        let mut state_since: u64 = EpochMs::now().millis();
        loop {
            ticker.tick().await;
            let now = EpochMs::now().millis();
            let hold_active = hold_rx.borrow().is_some();
            let inflight = continuum_engine::inference::resource_admission::inflight_model_calls();
            let prev = gate().tx.borrow().state;
            // Thread the hysteresis memory: the instant inputs read active,
            // idle memory resets; the first idle instant starts the clock.
            let instant_active = hold_active
                || inflight > 0
                || {
                    let ld = LAST_DIRECTED_MS.load(Ordering::Acquire);
                    ld > 0 && now.saturating_sub(ld) < DIRECTED_LINGER.as_millis() as u64
                };
            if instant_active {
                idle_since = None;
            } else if idle_since.is_none() {
                idle_since = Some(now);
            }
            let next = compute_state(
                hold_active,
                inflight,
                LAST_DIRECTED_MS.load(Ordering::Acquire),
                now,
                idle_since,
            );
            if next != prev {
                state_since = now;
                continuum_base::probe!(
                    class = "activity.gate.state",
                    state = ?next,
                    was = ?prev,
                    hold_active,
                    inflight = inflight as u64,
                    "organism activity state changed — background appetites follow this watch"
                );
            }
            // Publish on change only (held_ms updates ride the next real change;
            // consumers park on transitions, not on a clock).
            if next != prev {
                publish(ActivitySnapshot {
                    state: next,
                    held_ms: now.saturating_sub(state_since),
                });
            }
        }
    });
}


#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn boredom_arises_slowly_and_dies_instantly() {
        let t0 = 1_000_000u64;
        // any engagement → Active, regardless of idle memory
        assert_eq!(compute_state(true, 0, 0, t0, Some(0)), ActivityState::Active);
        assert_eq!(compute_state(false, 2, 0, t0, Some(0)), ActivityState::Active);
        // directed linger: a turn 10s ago is still engagement
        assert_eq!(
            compute_state(false, 0, t0 - 10_000, t0, Some(0)),
            ActivityState::Active
        );
        // idle but not long enough → SettlingIdle
        assert_eq!(
            compute_state(false, 0, 0, t0, Some(t0 - 1_000)),
            ActivityState::SettlingIdle
        );
        // sustained idle → Bored
        let long = BOREDOM_AFTER.as_millis() as u64 + 1;
        assert_eq!(
            compute_state(false, 0, 0, t0, Some(t0 - long)),
            ActivityState::Bored
        );
        // and one in-flight call ends boredom NOW even with old idle memory
        assert_eq!(
            compute_state(false, 1, 0, t0, Some(t0 - long)),
            ActivityState::Active
        );
    }
    // what this catches: a published state that nobody is subscribed to must still be
    // the stored state — otherwise the gate reads Active forever and reports the same
    // transition every tick (2026-09-05, BigMama's 120 identical activity.gate.state lines).
    #[test]
    fn a_publish_with_no_subscriber_is_still_the_current_state() {
        publish(ActivitySnapshot {
            state: ActivityState::Bored,
            held_ms: 7,
        });
        assert_eq!(current().state, ActivityState::Bored);
        assert_eq!(current().held_ms, 7);
        publish(ActivitySnapshot {
            state: ActivityState::Active,
            held_ms: 0,
        });
        assert_eq!(current().state, ActivityState::Active);
    }
}
