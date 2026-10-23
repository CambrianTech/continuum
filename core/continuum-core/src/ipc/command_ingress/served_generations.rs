//! Generations this seat serves concurrently for other nodes (card b3e5b582, slice 2a).
//!
//! The command pump used to await each served `ai/generate` inline, so one generation
//! holding a lane for minutes stopped the seat from reading anything else: a 2-lane
//! seat answered remote requesters one at a time (IntelMac -> M5, 2026-10-23). The
//! handler now admits a generation here, runs it as its own task, and returns.
//!
//! Custody is explicit and bounded: a generation is admitted only while fewer than the
//! seat's live served-lane count are outstanding (each waiting generation retains its
//! whole request), a refused one is answered at once with a typed overload error, and
//! dropping the registry (the pump stops) aborts every task still running. The registry
//! is keyed by correlation and records who asked, so the requester that abandons a
//! generation can cancel it, and nobody else can (card 83f774fb, slice 2b).

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use airc_core::PeerId;
use parking_lot::Mutex;
use tokio::task::AbortHandle;
use uuid::Uuid;

/// Outstanding served generations, keyed by request correlation.
#[derive(Default)]
pub(super) struct ServedGenerations {
    outstanding: Mutex<HashMap<Uuid, Served>>,
    /// Cancels that arrived before their generation was admitted, per authenticated
    /// requester: (generation, arrival). A cancel travels on its own connection and can
    /// overtake the request it cancels; a remembered cancel refuses THAT requester's
    /// request when it arrives. Keyed by requester first, so one peer's cancel can neither
    /// overwrite nor evict another's. Each requester keeps at most the admission bound; past
    /// it the seat FAILS CLOSED: it forgets the individual cancels and fences that requester
    /// (refuses all its new generations) for one window, so no abandoned request can start
    /// because its cancel was dropped. Always locked after `outstanding`, so a cancel and an
    /// admission never interleave.
    early_cancels: Mutex<HashMap<PeerId, EarlyCancels>>,
}

/// One requester's cancels that arrived ahead of their requests.
#[derive(Default)]
struct EarlyCancels {
    /// (generation, arrival), at most the admission bound.
    remembered: std::collections::VecDeque<(Uuid, tokio::time::Instant)>,
    /// Set when `remembered` overflowed: until then every new generation from this
    /// requester is refused (fail closed, O(1) memory).
    fenced_until: Option<tokio::time::Instant>,
}

/// How long an early cancel is remembered: the command round-trip budget both ends already
/// agree on. A request and its own cancel arrive within one such round trip of each other.
// derived-or-floor: the protocol's shared round-trip deadline, not a new number.
const EARLY_CANCEL_WINDOW: std::time::Duration = continuum_airc_protocol::DEFAULT_COMMAND_DEADLINE;

/// One outstanding generation.
struct Served {
    /// The authenticated peer that asked for it: the only peer that may cancel it.
    caller: PeerId,
    /// `None` between admission and spawn; the task's abort handle after.
    task: Option<AbortHandle>,
    /// Cancelled before its task was attached: [`ServedGenerations::attach`] aborts it.
    cancelled: bool,
}

/// What a requester's cancel did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CancelOutcome {
    /// The generation was aborted (or will be, the instant its task is attached).
    Cancelled,
    /// This seat serves no generations (a registry-less copy); nothing to cancel.
    NotServing,
    /// Nothing outstanding has that generation yet: the cancel is remembered for
    /// [`EARLY_CANCEL_WINDOW`] and refuses the request if it arrives after its cancel.
    Remembered,
    /// Past the per-requester bound: the requester is fenced for one window instead of a
    /// live cancel being dropped.
    Fenced,
    /// It is outstanding but was asked for by another peer: left running.
    NotYours,
}

impl CancelOutcome {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::NotServing => "not_serving",
            Self::Remembered => "remembered",
            Self::Fenced => "fenced",
            Self::NotYours => "not_yours",
        }
    }
}

/// Why a generation was not admitted. Rendered into the requester's error reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Refusal {
    /// Every admissible slot is held by a generation still being served.
    Overloaded { outstanding: usize, bound: usize },
    /// A generation with this correlation is already being served.
    Duplicate { correlation: Uuid },
    /// Its requester cancelled it before it arrived here.
    CancelledByRequester { correlation: Uuid },
    /// Its requester sent more early cancels than this seat remembers, so all of that
    /// requester's new generations are refused for one cancel window (fail closed).
    Fenced { correlation: Uuid },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overloaded { outstanding, bound } => write!(
                f,
                "seat overloaded: {outstanding} generations outstanding for {bound} served lanes; \
                 retry elsewhere or later"
            ),
            Self::Duplicate { correlation } => {
                write!(f, "generation {correlation} is already being served here")
            }
            Self::Fenced { correlation } => write!(
                f,
                "generation {correlation} refused: its requester overflowed this seat's early-cancel memory; retry after the cancel window"
            ),
            Self::CancelledByRequester { correlation } => write!(
                f,
                "generation {correlation} was cancelled by its requester before it arrived"
            ),
        }
    }
}

/// One admitted generation's hold on its slot. Dropping it (the task ends, by any path)
/// releases the slot. It holds the registry weakly, so it never keeps a stopped pump's
/// registry alive.
pub(super) struct Admission {
    registry: Weak<ServedGenerations>,
    correlation: Uuid,
}

impl Drop for Admission {
    fn drop(&mut self) {
        if let Some(registry) = self.registry.upgrade() {
            registry.outstanding.lock().remove(&self.correlation);
        }
    }
}

impl ServedGenerations {
    /// Admit `correlation` if fewer than `bound` generations are outstanding.
    pub(super) fn admit(
        self: &Arc<Self>,
        correlation: Uuid,
        caller: PeerId,
        bound: usize,
    ) -> Result<Admission, Refusal> {
        let mut outstanding = self.outstanding.lock();
        if outstanding.contains_key(&correlation) {
            return Err(Refusal::Duplicate { correlation });
        }
        {
            let mut early = self.early_cancels.lock();
            forget_expired(&mut early);
            if let Some(cancels) = early.get_mut(&caller) {
                if cancels.fenced_until.is_some() {
                    return Err(Refusal::Fenced { correlation });
                }
                if let Some(at) = cancels
                    .remembered
                    .iter()
                    .position(|(id, _)| *id == correlation)
                {
                    cancels.remembered.remove(at);
                    if cancels.remembered.is_empty() {
                        early.remove(&caller);
                    }
                    return Err(Refusal::CancelledByRequester { correlation });
                }
            }
        }
        if outstanding.len() >= bound {
            return Err(Refusal::Overloaded {
                outstanding: outstanding.len(),
                bound,
            });
        }
        outstanding.insert(
            correlation,
            Served {
                caller,
                task: None,
                cancelled: false,
            },
        );
        Ok(Admission {
            registry: Arc::downgrade(self),
            correlation,
        })
    }

    /// Record the spawned task for an admitted generation, so a stop or a cancel can abort
    /// it. A task that already finished (its admission released the entry) is not
    /// re-inserted; one cancelled before it was attached is aborted now.
    pub(super) fn attach(&self, correlation: Uuid, task: AbortHandle) {
        if let Some(served) = self.outstanding.lock().get_mut(&correlation) {
            if served.cancelled {
                task.abort();
            }
            served.task = Some(task);
        }
    }

    /// The requester abandoned `correlation`: abort it, but only for the peer that asked
    /// for it. The aborted task's admission then releases its slot.
    pub(super) fn cancel(
        &self,
        correlation: Uuid,
        requester: PeerId,
        bound: usize,
    ) -> CancelOutcome {
        let mut outstanding = self.outstanding.lock();
        let Some(served) = outstanding.get_mut(&correlation) else {
            let mut early = self.early_cancels.lock();
            forget_expired(&mut early);
            let now = tokio::time::Instant::now();
            let cancels = early.entry(requester).or_default();
            if cancels.fenced_until.is_some() {
                // Already fenced: a further early cancel extends the fence to cover it.
                cancels.fenced_until = Some(now + EARLY_CANCEL_WINDOW);
                return CancelOutcome::Fenced;
            }
            cancels.remembered.retain(|(id, _)| *id != correlation);
            cancels.remembered.push_back((correlation, now));
            if cancels.remembered.len() > bound.max(1) {
                // Never drop a live cancel: fence the requester for one window instead.
                cancels.remembered.clear();
                cancels.fenced_until = Some(now + EARLY_CANCEL_WINDOW);
                return CancelOutcome::Fenced;
            }
            return CancelOutcome::Remembered;
        };
        if served.caller != requester {
            return CancelOutcome::NotYours;
        }
        match &served.task {
            Some(task) => task.abort(),
            None => served.cancelled = true,
        }
        CancelOutcome::Cancelled
    }

    pub(super) fn outstanding(&self) -> usize {
        self.outstanding.lock().len()
    }
}

/// Drop early cancels older than [`EARLY_CANCEL_WINDOW`]: bounded by time, so a cancel
/// for a request that never comes (or already finished) does not live on.
fn forget_expired(early: &mut HashMap<PeerId, EarlyCancels>) {
    let now = tokio::time::Instant::now();
    early.retain(|_, cancels| {
        cancels
            .remembered
            .retain(|(_, at)| at.elapsed() <= EARLY_CANCEL_WINDOW);
        if cancels.fenced_until.is_some_and(|until| now >= until) {
            cancels.fenced_until = None;
        }
        !cancels.remembered.is_empty() || cancels.fenced_until.is_some()
    });
}

impl Drop for ServedGenerations {
    /// The pump stopped: every generation it was serving is aborted, not orphaned.
    fn drop(&mut self) {
        for task in self
            .outstanding
            .get_mut()
            .values()
            .filter_map(|s| s.task.as_ref())
        {
            task.abort();
        }
    }
}

/// The seat's admission bound for served generations: its live served-lane count, the
/// same number the serving gate is sized to. Before the first serving plan that count is
/// 0; one outstanding generation is what the inline pump always allowed, so it is the
/// floor, never less.
// derived-or-floor: the live lane count, floored at the inline pump's prior capacity of 1.
pub(super) fn admission_bound() -> usize {
    crate::inference::resource_admission::served_lane_count().max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(n: u128) -> PeerId {
        PeerId(Uuid::from_u128(n))
    }

    // what this catches: custody is bounded. A burst while every slot is held is refused
    // with the counts (not queued in unbounded tasks), a duplicate correlation is refused,
    // and a finished generation frees its slot.
    #[test]
    fn admission_is_bounded_and_a_finished_generation_frees_its_slot() {
        let registry = Arc::new(ServedGenerations::default());
        let first = registry
            .admit(Uuid::from_u128(1), peer(100), 2)
            .expect("first admitted");
        let _second = registry
            .admit(Uuid::from_u128(2), peer(100), 2)
            .expect("second admitted");
        assert_eq!(
            registry.admit(Uuid::from_u128(3), peer(100), 2).err(),
            Some(Refusal::Overloaded {
                outstanding: 2,
                bound: 2
            })
        );
        assert_eq!(
            registry.admit(Uuid::from_u128(2), peer(100), 3).err(),
            Some(Refusal::Duplicate {
                correlation: Uuid::from_u128(2)
            })
        );
        drop(first);
        assert_eq!(registry.outstanding(), 1);
        assert!(
            registry.admit(Uuid::from_u128(3), peer(100), 2).is_ok(),
            "the freed slot is admissible"
        );
    }

    // what this catches: a stopped pump does not orphan the generations it was serving.
    // Dropping the registry aborts every attached task; the tasks hold it only weakly.
    #[tokio::test]
    async fn dropping_the_registry_aborts_every_outstanding_generation() {
        let registry = Arc::new(ServedGenerations::default());
        let correlation = Uuid::from_u128(7);
        let admission = registry.admit(correlation, peer(100), 1).expect("admitted");
        let task = tokio::spawn(async move {
            let _admission = admission;
            std::future::pending::<()>().await;
        });
        registry.attach(correlation, task.abort_handle());
        drop(registry);
        let joined = tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .expect("the aborted task ends promptly");
        assert!(
            joined.is_err_and(|e| e.is_cancelled()),
            "the task was cancelled, not completed"
        );
    }

    // what this catches (card 83f774fb): an abandoned generation is cancelled only by the
    // peer that asked for it. Another peer's cancel and a cancel for an unknown
    // correlation abort nothing; a cancel that lands before the task is attached aborts
    // it the moment it is attached, never letting it run for no one.
    #[tokio::test]
    async fn only_the_requester_cancels_and_an_early_cancel_still_lands() {
        let registry = Arc::new(ServedGenerations::default());
        let (asker, stranger) = (peer(1), peer(2));
        let held = Uuid::from_u128(10);
        let admission = registry.admit(held, asker, 2).expect("admitted");
        let task = tokio::spawn(async move {
            let _admission = admission;
            std::future::pending::<()>().await;
        });
        registry.attach(held, task.abort_handle());
        assert_eq!(registry.cancel(held, stranger, 4), CancelOutcome::NotYours);
        assert_eq!(
            registry.cancel(Uuid::from_u128(99), asker, 4),
            CancelOutcome::Remembered
        );
        assert_eq!(
            registry.outstanding(),
            1,
            "another peer's cancel left it running"
        );
        assert_eq!(registry.cancel(held, asker, 4), CancelOutcome::Cancelled);
        let joined = tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .expect("the cancelled task ends promptly");
        assert!(joined.is_err_and(|e| e.is_cancelled()));
        assert_eq!(
            registry.outstanding(),
            0,
            "the aborted task released its slot"
        );

        let early = Uuid::from_u128(11);
        let admission = registry.admit(early, asker, 2).expect("admitted");
        assert_eq!(registry.cancel(early, asker, 4), CancelOutcome::Cancelled);
        let task = tokio::spawn(async move {
            let _admission = admission;
            std::future::pending::<()>().await;
        });
        registry.attach(early, task.abort_handle());
        let joined = tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .expect("a cancel that landed before attach aborts at attach");
        assert!(joined.is_err_and(|e| e.is_cancelled()));
    }

    // what this catches (Codex review of cf08ff017): a cancel travels on its own
    // connection and can arrive BEFORE the request it cancels. It is remembered for the
    // round-trip window and refuses that requester's request when it arrives; another
    // caller's request with the same id is not blocked; after the window it is forgotten.
    #[tokio::test(start_paused = true)]
    async fn a_cancel_that_overtakes_its_request_refuses_it_within_the_window() {
        let registry = Arc::new(ServedGenerations::default());
        let (asker, stranger) = (peer(1), peer(2));
        let early = Uuid::from_u128(20);
        assert_eq!(registry.cancel(early, asker, 4), CancelOutcome::Remembered);
        assert!(
            registry.admit(early, stranger, 4).is_ok(),
            "another caller is not blocked"
        );
        let late = Uuid::from_u128(21);
        assert_eq!(registry.cancel(late, asker, 4), CancelOutcome::Remembered);
        assert_eq!(
            registry.admit(late, asker, 4).err(),
            Some(Refusal::CancelledByRequester { correlation: late }),
            "the overtaken request is refused, never started"
        );
        let stale = Uuid::from_u128(22);
        assert_eq!(registry.cancel(stale, asker, 4), CancelOutcome::Remembered);
        tokio::time::advance(EARLY_CANCEL_WINDOW + std::time::Duration::from_secs(1)).await;
        assert!(
            registry.admit(stale, asker, 4).is_ok(),
            "a cancel past the window is forgotten"
        );
    }

    // what this catches (Codex re-review of 165b35a5b): early cancels are scoped to the
    // authenticated requester. B's cancel for the same id neither overwrites nor removes
    // A's, so A's overtaken request is still refused; and a requester that overflows the
    // bound is fenced for one window (fail closed), never has a live cancel dropped.
    #[tokio::test(start_paused = true)]
    async fn early_cancels_are_per_requester_and_bounded() {
        let registry = Arc::new(ServedGenerations::default());
        let (a, b) = (peer(1), peer(2));
        let id = Uuid::from_u128(30);
        assert_eq!(registry.cancel(id, a, 4), CancelOutcome::Remembered);
        assert_eq!(registry.cancel(id, b, 4), CancelOutcome::Remembered);
        assert_eq!(
            registry.admit(id, a, 4).err(),
            Some(Refusal::CancelledByRequester { correlation: id }),
            "B's cancel did not overwrite A's"
        );
        assert_eq!(
            registry.admit(id, b, 4).err(),
            Some(Refusal::CancelledByRequester { correlation: id }),
            "and B's own cancel still applies to B"
        );

        // Codex re-review of 9b12c8b75: with bound 1, cancel(r1), cancel(r2), then a
        // delayed admit(r1) must NOT start r1. Overflow fences the requester (fail closed)
        // instead of evicting r1's still-live cancel.
        let bound = 1;
        let (r1, r2, r3) = (
            Uuid::from_u128(40),
            Uuid::from_u128(41),
            Uuid::from_u128(42),
        );
        assert_eq!(registry.cancel(r1, b, bound), CancelOutcome::Remembered);
        assert_eq!(registry.cancel(r2, b, bound), CancelOutcome::Fenced);
        assert_eq!(
            registry.admit(r1, b, 4).err(),
            Some(Refusal::Fenced { correlation: r1 }),
            "the delayed r1 is refused: its cancel was never silently dropped"
        );
        assert_eq!(
            registry.admit(r3, b, 4).err(),
            Some(Refusal::Fenced { correlation: r3 }),
            "every new generation from the fenced requester is refused within the window"
        );
        let early = Uuid::from_u128(31);
        assert_eq!(registry.cancel(early, a, bound), CancelOutcome::Remembered);
        assert_eq!(
            registry.admit(early, a, bound).err(),
            Some(Refusal::CancelledByRequester { correlation: early }),
            "B's fence never touched A"
        );
        tokio::time::advance(EARLY_CANCEL_WINDOW + std::time::Duration::from_secs(1)).await;
        assert!(
            registry.admit(r3, b, 4).is_ok(),
            "the fence lifts after one window"
        );
    }
}
