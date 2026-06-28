//! The deferred lane — taking a SLOW faculty off the cognition hot path.
//!
//! **The cbar lesson (slice 2 of the decoupling work; slice 1 = the `CycleId`
//! stamp in `workspace.rs`).** A deep analyzer — segmentation, a full-LLM
//! faculty — is too slow to run inside the per-tick loop. cbar never let one
//! block the 45fps pipeline: it ran on its own thread, and the loop READ its
//! last-good result (reprojected forward via feature-history). This is the
//! cognition analog, at the faculty layer.
//!
//! A [`DeferredFaculty`] wraps an inner [`Faculty`] so that the inner's
//! expensive work runs on its **own** `tokio` task, while the hot-path
//! `contribute()` is non-blocking:
//!
//! 1. `contribute()` publishes the current world at the background task through a
//!    `watch` channel (always-latest, never blocks, never drops the newest), and
//! 2. returns the inner faculty's **most recent finding** from another `watch`
//!    snapshot — already stamped with the (possibly older) [`CycleId`] it was
//!    computed against — or `None` until the first compute lands.
//!
//! So a slow faculty sits in the SAME `faculties` Vec as the fast ones and the
//! per-tick `join_all` barrier in [`WorkspaceCycle::run_in_room`] never waits on
//! it: the barrier stays, but nothing slow is ON it. The late finding lands a
//! tick or three later carrying its own cycle — ready for slice-3
//! reconcile-forward (fold a turn-N finding onto turn-N+3's world via
//! turn-history, the cbar reprojection).
//!
//! This is the "scary-fast reflexes in a slow brain": the immediate lane answers
//! every tick; the slow lane lands late, honestly stamped, and is merged in
//! rather than blocking the spoken turn.
//!
//! Conforms to `docs/architecture/CONCURRENCY-STYLE-GUIDE.md`: own `tokio::spawn`
//! task, `catch_unwind` around the loop body, `watch` for state in BOTH
//! directions (no `Arc<Mutex>` across `await`), event-triggered (not a
//! sleep-loop), and a quarantine after repeated inner panics.

use std::sync::Arc;

use async_trait::async_trait;
use futures_util::FutureExt;
use tokio::sync::watch;
use uuid::Uuid;

use crate::cognition::workspace::{Contribution, CycleId, Faculty, FacultyId, Workspace};

/// The slice of world-state the background task recomputes against. Carried
/// through a `watch` so the hot path always overwrites with the latest (no
/// backlog, no dropped-newest) and the worker chews the freshest burst it can.
#[derive(Debug, Clone)]
struct DeferredInput {
    world_state: String,
    room_id: Uuid,
    /// The cycle this input belongs to — the worker stamps its finding with it,
    /// so a finding always carries the moment it reasoned about, not the moment
    /// it finished.
    cycle: CycleId,
}

impl DeferredInput {
    /// The initial `watch` value: a sentinel the worker skips (UNSTAMPED cycle =
    /// "no real burst yet"). `watch::Receiver::changed()` doesn't fire on the
    /// initial value, so this is never actually computed against — it just gives
    /// the channel something to hold before the first real tick.
    fn sentinel() -> Self {
        Self {
            world_state: String::new(),
            room_id: Uuid::nil(),
            cycle: CycleId::UNSTAMPED,
        }
    }
}

/// A finding the worker has computed, tagged with the room it reasoned about so
/// the hot path can refuse to serve it into a DIFFERENT room. This is the
/// "assuming it didn't context-switch to where it's irrelevant" guard: a recall
/// computed against room A's burst must not be injected into room B's turn just
/// because it happens to be the last thing the worker finished.
#[derive(Debug, Clone)]
struct StampedFinding {
    /// The room the inner faculty reasoned about.
    room_id: Uuid,
    /// The finding itself (already carrying the cycle it was computed against).
    contribution: Contribution,
}

/// A faculty whose inner work is taken off the cognition hot path (see module
/// docs). Drops the background task when dropped.
pub struct DeferredFaculty {
    id: FacultyId,
    /// Hot path reads the inner faculty's last-good finding here, lock-free.
    latest: watch::Receiver<Option<StampedFinding>>,
    /// Hot path pushes the current world here for the worker to recompute against.
    world_tx: watch::Sender<DeferredInput>,
    /// Owns the worker task so it's aborted when this faculty is dropped — no
    /// orphaned compute outliving the mind it served.
    _worker: DropGuard,
}

/// Aborts the held task on drop. Keeps the worker's lifetime tied to the faculty.
struct DropGuard(tokio::task::JoinHandle<()>);
impl Drop for DropGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// After this many consecutive inner panics the worker stops recomputing and the
/// faculty just serves its last-good (or `None`) — a bad backend degrades the
/// lane to stale, it never crashes the mind. Matches the guide's quarantine
/// discipline (the monitor's 3-strike rule).
const QUARANTINE_AFTER: u32 = 3;

impl DeferredFaculty {
    /// Wrap `inner` and spawn its background worker. The inner faculty must be a
    /// **perception-tier** faculty (`reacts_to_broadcast() == false`): the worker
    /// reconstructs a minimal [`Workspace`] from the raw world-state, with an
    /// empty broadcast, so a deliberation-tier inner (which reads the assembled
    /// broadcast) would run blind. We force `reacts_to_broadcast() == false` on
    /// the wrapper to keep it in the perception phase regardless.
    pub fn spawn(inner: Arc<dyn Faculty>) -> Self {
        let id = inner.id();
        let (world_tx, mut world_rx) = watch::channel(DeferredInput::sentinel());
        let (latest_tx, latest) = watch::channel::<Option<StampedFinding>>(None);

        let worker = tokio::spawn(async move {
            let mut consecutive_panics: u32 = 0;
            // Event-triggered, not a sleep-loop: wake only when the hot path
            // publishes a new world. `changed()` skips the initial sentinel.
            while world_rx.changed().await.is_ok() {
                if consecutive_panics >= QUARANTINE_AFTER {
                    // Quarantined: stop recomputing, keep the last-good. Draining
                    // the receiver here would spin; just stop reacting.
                    break;
                }
                // Take the freshest burst (watch already coalesced to latest).
                let input = world_rx.borrow_and_update().clone();
                if input.cycle == CycleId::UNSTAMPED {
                    continue; // sentinel / not a real burst
                }
                let room_id = input.room_id;
                let ws = Workspace::in_room(input.world_state, room_id)
                    .with_cycle(input.cycle);

                // The inner faculty's contribute is async (real inference/IPC).
                // Catch a panic so a flawed backend degrades the lane to stale,
                // never kills the runtime (guide: catch_unwind around the body).
                let outcome = AssertUnwindSafeFut(inner.contribute(&ws))
                    .catch_unwind()
                    .await;

                match outcome {
                    Ok(finding) => {
                        consecutive_panics = 0;
                        // Stamp with the cycle it reasoned against AND the room it
                        // reasoned about — the late finding carries its own moment
                        // (cycle) and its own place (room), so the hot path can
                        // refuse to serve it into a different context.
                        let stamped = finding.map(|mut c| {
                            c.cycle = input.cycle;
                            StampedFinding {
                                room_id,
                                contribution: c,
                            }
                        });
                        // Ignore send error: no receivers left = faculty dropped.
                        let _ = latest_tx.send(stamped);
                    }
                    Err(_) => {
                        consecutive_panics += 1;
                    }
                }
            }
        });

        Self {
            id,
            latest,
            world_tx,
            _worker: DropGuard(worker),
        }
    }
}

#[async_trait]
impl Faculty for DeferredFaculty {
    fn id(&self) -> FacultyId {
        self.id.clone()
    }

    /// Non-blocking: kick the worker with the current world and return the
    /// last-good finding. NEVER awaits the inner faculty — that's the whole point
    /// of the lane.
    async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
        // Publish the current world for the worker (always-latest, never blocks).
        // Ignore send error: worker gone = serve whatever last-good we have.
        let _ = self.world_tx.send(DeferredInput {
            world_state: ws.world_state.clone(),
            room_id: ws.room_id,
            cycle: ws.cycle,
        });
        // Context guard: serve the last-good ONLY if it was computed for the room
        // the mind is in NOW. A finding from another room is "context-switched to
        // where it's irrelevant" — withhold it rather than inject a cross-context
        // memory into this turn. (Slice 3 will REPROJECT a same-room-but-stale
        // finding forward; a different-room finding is simply not ours to serve.)
        let guard = self.latest.borrow();
        match guard.as_ref() {
            Some(found) if found.room_id == ws.room_id => Some(found.contribution.clone()),
            _ => None,
        }
    }

    /// A deferred faculty is perception-tier (see [`DeferredFaculty::spawn`]): it
    /// reconstructs from raw world-state, so it must run in phase 1, never read
    /// the assembled broadcast.
    fn reacts_to_broadcast(&self) -> bool {
        false
    }
}

/// `Future` adapter that asserts unwind-safety so an async body can be
/// `catch_unwind`'d. The captured `&Workspace` and `Arc<dyn Faculty>` are only
/// read across the await, so this is sound for our use.
struct AssertUnwindSafeFut<F>(F);
impl<F: std::future::Future> std::future::Future for AssertUnwindSafeFut<F> {
    type Output = F::Output;
    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // SAFETY: structural pin projection of the single field; we never move it.
        let inner = unsafe { self.map_unchecked_mut(|s| &mut s.0) };
        inner.poll(cx)
    }
}
impl<F> std::panic::UnwindSafe for AssertUnwindSafeFut<F> {}
impl<F> std::panic::RefUnwindSafe for AssertUnwindSafeFut<F> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::workspace::Contribution;

    /// A deliberately slow perception faculty: it waits, then surfaces a finding.
    /// Models a full-LLM / deep-analyzer faculty whose latency would blow the
    /// per-tick budget if run inline.
    struct SlowRecall;
    #[async_trait]
    impl Faculty for SlowRecall {
        fn id(&self) -> FacultyId {
            FacultyId::Recall
        }
        async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
            Some(Contribution::context(
                FacultyId::Recall,
                "a slow, late recall finding",
                0.8,
                "deep analyzer, off the hot path",
            ))
        }
    }

    // what this catches: a slow faculty wrapped as Deferred never blocks the hot
    // path — contribute() returns immediately with the last-good (None until the
    // first compute lands), and when the late finding arrives it carries the
    // OLDER cycle it was computed against, not the current tick. This is the
    // immediate-vs-deferred split: the per-tick join_all barrier stays harmless
    // because nothing slow is ON it, and slice-3 reconcile can fold the late
    // finding forward precisely because it knows its own moment.
    #[tokio::test]
    async fn deferred_faculty_never_blocks_and_lands_late_with_its_own_cycle() {
        let deferred = DeferredFaculty::spawn(Arc::new(SlowRecall));

        // Tick 1 (cycle 1): nothing computed yet → None, and it must return FAST
        // (the inner sleeps 40ms; the hot path must not wait that long).
        let ws1 = Workspace::in_room("burst one", Uuid::nil()).with_cycle(CycleId(1));
        let t = tokio::time::Instant::now();
        let r1 = deferred.contribute(&ws1).await;
        assert!(
            t.elapsed() < std::time::Duration::from_millis(15),
            "hot path waited on the slow inner ({:?}) — the lane isn't deferred",
            t.elapsed()
        );
        assert!(r1.is_none(), "no last-good finding on the very first tick");

        // Give the worker time to compute against cycle 1 and publish.
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;

        // A later tick (cycle 5): now the last-good is available, and it's stamped
        // with cycle 1 (when it was computed), NOT 5 (now).
        let ws5 = Workspace::in_room("burst five", Uuid::nil()).with_cycle(CycleId(5));
        let r5 = deferred.contribute(&ws5).await;
        let late = r5.expect("the slow finding has landed by now");
        assert_eq!(
            late.cycle,
            CycleId(1),
            "the late finding must carry the cycle it reasoned against, not the current tick"
        );
        assert_eq!(late.faculty, FacultyId::Recall);
    }

    // what this catches: the context guard — a finding computed against room A
    // must NOT be served into room B's turn. This is Joel's "assuming it didn't
    // context-switch to something where it was irrelevant": a deferred recall
    // that lands a minute late is only useful if the mind is still in the room it
    // was reasoned for; in a different room it's withheld, never injected as a
    // cross-context memory. (Same-room-but-stale is slice-3 reproject; this test
    // covers different-room = simply not ours to serve.)
    #[tokio::test]
    async fn deferred_finding_is_withheld_when_the_mind_changed_rooms() {
        let deferred = DeferredFaculty::spawn(Arc::new(SlowRecall));
        let room_a = Uuid::new_v4();
        let room_b = Uuid::new_v4();

        // Tick against room A — kicks the worker to compute for room A.
        let ws_a = Workspace::in_room("burst in A", room_a).with_cycle(CycleId(1));
        let _ = deferred.contribute(&ws_a).await;

        // Let the worker finish computing against room A.
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;

        // Now the mind is in room B. The room-A finding must be withheld.
        let ws_b = Workspace::in_room("burst in B", room_b).with_cycle(CycleId(2));
        let in_b = deferred.contribute(&ws_b).await;
        assert!(
            in_b.is_none(),
            "a room-A finding must not be injected into room B's turn"
        );

        // Back in room A: the finding is relevant again and IS served.
        let ws_a2 = Workspace::in_room("back in A", room_a).with_cycle(CycleId(3));
        let in_a = deferred.contribute(&ws_a2).await;
        let found = in_a.expect("the room-A finding is ours to serve back in room A");
        assert_eq!(found.cycle, CycleId(1), "still stamped with its original cycle");
    }
}
