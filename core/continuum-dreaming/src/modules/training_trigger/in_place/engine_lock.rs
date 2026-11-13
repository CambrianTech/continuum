//! One engine's learning lock, naming its holder (card b82899f3). Every learner on an engine waits
//! behind whoever holds it; a hold that never ends stalls all of them silently (BigMama 2026-11-11:
//! 19 silent operations at shutdown, one engine busy, nothing naming the holder). So the lock
//! records who holds it (her bucket's persona, the phase) and since when, and a waiter that finds
//! it held says so once: the measurement that narrows the lock to the engine's own operations.

use super::*;

/// What the holder is doing with the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    /// Seating her learner and covering her next example.
    Prepare,
    /// One learning or restoring step.
    Step,
    /// Publishing her generation and paging her out.
    Settle,
    /// Holding the engine until a lost answer's stop is confirmed.
    Fence,
    /// Shutdown: publishing and stopping every learner.
    Quiesce,
}

struct Holder {
    /// Whose bucket holds it; none for shutdown, which holds it for every bucket.
    persona: Option<Uuid>,
    phase: Phase,
    since: std::time::Instant,
}

/// One engine's learning and who holds it.
#[derive(Default)]
pub(super) struct EngineSlot {
    learning: Arc<tokio::sync::Mutex<EngineLearning>>,
    holder: Arc<parking_lot::Mutex<Option<Holder>>>,
}

impl EngineSlot {
    /// Take the engine for `persona`'s `phase`, waiting behind the holder (and saying who it is).
    pub(super) async fn acquire(&self, persona: Option<Uuid>, phase: Phase) -> EngineGuard {
        let guard = match Arc::clone(&self.learning).try_lock_owned() {
            Ok(guard) => guard,
            Err(_) => {
                self.report_held(persona, phase);
                Arc::clone(&self.learning).lock_owned().await
            }
        };
        self.hold(guard, persona, phase)
    }

    /// Take the engine only if it is free; a held engine is reported and left to its holder.
    pub(super) fn try_acquire(&self, persona: Option<Uuid>, phase: Phase) -> Option<EngineGuard> {
        match Arc::clone(&self.learning).try_lock_owned() {
            Ok(guard) => Some(self.hold(guard, persona, phase)),
            Err(_) => {
                self.report_held(persona, phase);
                None
            }
        }
    }

    fn hold(&self, guard: tokio::sync::OwnedMutexGuard<EngineLearning>, persona: Option<Uuid>, phase: Phase) -> EngineGuard {
        *self.holder.lock() = Some(Holder { persona, phase, since: std::time::Instant::now() });
        EngineGuard { guard, holder: Arc::clone(&self.holder) }
    }

    /// One probe per waiter: who holds the engine, doing what, for how long.
    fn report_held(&self, waiter: Option<Uuid>, waiting: Phase) {
        let held = self.holder.lock().as_ref().map(|h| (h.persona, h.phase, h.since.elapsed()));
        match held {
            Some((holder, phase, held_for)) => continuum_base::probe!(class = "training.in_place.engine_held", waiter = ?waiter,
                waiting = ?waiting, holder = ?holder, holder_phase = ?phase, held_ms = held_for.as_secs_f64() * 1000.0,
                "the engine's learning lock is held: this waiter queues behind the holder"),
            // between a release and the next holder's record: held, but by no one named yet
            None => continuum_base::probe!(class = "training.in_place.engine_held", waiter = ?waiter, waiting = ?waiting,
                "the engine's learning lock is held by an acquirer not yet recorded"),
        }
    }
}

/// The engine, held: clears its holder record when released.
pub(super) struct EngineGuard {
    guard: tokio::sync::OwnedMutexGuard<EngineLearning>,
    holder: Arc<parking_lot::Mutex<Option<Holder>>>,
}

impl EngineGuard {
    /// The holder moves to another phase without releasing the engine (a step's guard into its fence).
    pub(super) fn now(&self, phase: Phase) {
        if let Some(holder) = self.holder.lock().as_mut() {
            holder.phase = phase;
        }
    }
}

impl std::ops::Deref for EngineGuard {
    type Target = EngineLearning;
    fn deref(&self) -> &EngineLearning {
        &self.guard
    }
}

impl std::ops::DerefMut for EngineGuard {
    fn deref_mut(&mut self) -> &mut EngineLearning {
        &mut self.guard
    }
}

impl Drop for EngineGuard {
    fn drop(&mut self) {
        *self.holder.lock() = None; // before the mutex itself releases (fields drop after this body)
    }
}

#[cfg(test)]
impl EngineSlot {
    /// Who holds the engine now, as a waiter would be told.
    fn held_by(&self) -> Option<(Option<Uuid>, Phase)> {
        self.holder.lock().as_ref().map(|h| (h.persona, h.phase))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (BigMama 2026-11-11, 19 silent operations behind one unnamed hold): an
    // engine held with nobody named, or a record that outlives the hold so a waiter blames the
    // wrong mind. The record names her and her phase while she holds it, follows her into a fence,
    // and is gone the moment she lets go; a second taker finds it held.
    #[tokio::test]
    async fn the_engine_lock_names_its_holder_only_while_held() {
        let slot = EngineSlot::default();
        let her = Uuid::new_v4();
        let held = slot.acquire(Some(her), Phase::Step).await;
        assert_eq!(slot.held_by(), Some((Some(her), Phase::Step)), "she holds it, stepping");
        assert!(slot.try_acquire(Some(Uuid::new_v4()), Phase::Prepare).is_none(), "a second taker finds it held");
        held.now(Phase::Fence);
        assert_eq!(slot.held_by(), Some((Some(her), Phase::Fence)), "her hold followed her into the fence");
        drop(held);
        assert_eq!(slot.held_by(), None, "released: nobody is named");
        assert!(slot.try_acquire(None, Phase::Quiesce).is_some(), "and the engine is free");
    }
}
