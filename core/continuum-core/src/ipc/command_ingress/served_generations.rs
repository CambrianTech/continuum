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
    /// requester. A cancel travels on its own connection and can overtake the request it
    /// cancels; a remembered cancel refuses THAT requester's request when it arrives. It is
    /// remembered exactly as long as the original request can still be admitted: until the
    /// request's own validity (HEADER_GENERATE_VALID_UNTIL_MS, which admission enforces),
    /// uncapped, so a delayed original is refused either by its cancel or as expired, never
    /// started. The lifetime is the requester's own claim and only ever refuses that
    /// requester's own ids; memory stays bounded by count.
    /// Keyed by requester first, so one peer's cancel can neither overwrite nor evict
    /// another's. Each requester keeps at most the admission bound; past it the seat FAILS
    /// CLOSED and fences that requester until the latest validity it was holding. Always
    /// locked after `outstanding`, so a cancel and an admission never interleave.
    early_cancels: Mutex<HashMap<PeerId, EarlyCancels>>,
}

/// One requester's cancels that arrived ahead of their requests.
#[derive(Default)]
struct EarlyCancels {
    /// (generation, the original request's validity in epoch ms), at most the bound.
    remembered: std::collections::VecDeque<(Uuid, u64)>,
    /// Set when `remembered` overflowed: until this epoch ms every new generation from this
    /// requester is refused (fail closed, O(1) memory).
    fenced_until_ms: Option<u64>,
}

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
    /// Nothing outstanding has that generation yet: the cancel is remembered until the
    /// original request's own validity and refuses it if it arrives after its cancel.
    Remembered,
    /// Past the per-requester bound: the requester is fenced until the latest validity it
    /// held, instead of a live cancel being dropped.
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
    /// requester's new generations are refused until the fence lifts (fail closed).
    Fenced { correlation: Uuid },
    /// It arrived after its own stated validity: a request this late is never started.
    Expired { correlation: Uuid },
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
                "generation {correlation} refused: its requester overflowed this seat's early-cancel memory; retry later"
            ),
            Self::Expired { correlation } => write!(
                f,
                "generation {correlation} arrived after its own validity; not started"
            ),
            Self::CancelledByRequester { correlation } => write!(
                f,
                "generation {correlation} was cancelled by its requester before it arrived"
            ),
        }
    }
}

impl Refusal {
    /// The typed reason the requester receives beside the text (card 03660a77).
    pub(super) fn wire(&self) -> continuum_airc_protocol::GenerateRefusal {
        use continuum_airc_protocol::GenerateRefusal as Wire;
        match self {
            Self::Overloaded { .. } => Wire::Overloaded,
            Self::Duplicate { .. } => Wire::Duplicate,
            Self::CancelledByRequester { .. } => Wire::CancelledByRequester,
            Self::Fenced { .. } => Wire::Fenced,
            Self::Expired { .. } => Wire::Expired,
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
    /// Admit `correlation` if it is still valid (`valid_until_ms`, the requester's stated
    /// validity), not cancelled ahead of time,
    /// and fewer than `bound` generations are outstanding.
    pub(super) fn admit(
        self: &Arc<Self>,
        correlation: Uuid,
        caller: PeerId,
        bound: usize,
        valid_until_ms: u64,
        now_ms: u64,
    ) -> Result<Admission, Refusal> {
        let mut outstanding = self.outstanding.lock();
        if outstanding.contains_key(&correlation) {
            return Err(Refusal::Duplicate { correlation });
        }
        {
            let mut early = self.early_cancels.lock();
            forget_expired(&mut early, now_ms);
            if now_ms > valid_until_ms {
                return Err(Refusal::Expired { correlation });
            }
            if let Some(cancels) = early.get_mut(&caller) {
                if cancels.fenced_until_ms.is_some() {
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
        valid_until_ms: u64,
        now_ms: u64,
    ) -> CancelOutcome {
        let mut outstanding = self.outstanding.lock();
        let Some(served) = outstanding.get_mut(&correlation) else {
            let mut early = self.early_cancels.lock();
            forget_expired(&mut early, now_ms);
            let until = valid_until_ms;
            let cancels = early.entry(requester).or_default();
            if let Some(fence) = cancels.fenced_until_ms.as_mut() {
                // Already fenced: the fence must also cover this cancel's request.
                *fence = (*fence).max(until);
                return CancelOutcome::Fenced;
            }
            cancels.remembered.retain(|(id, _)| *id != correlation);
            cancels.remembered.push_back((correlation, until));
            if cancels.remembered.len() > bound.max(1) {
                // Never drop a live cancel: fence the requester until the latest validity
                // among the cancels it was holding, then forget them (O(1) memory).
                let latest = cancels.remembered.iter().map(|(_, until)| *until).max();
                cancels.remembered.clear();
                cancels.fenced_until_ms = latest;
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

/// Drop early cancels whose original request can no longer be admitted (past its validity,
/// which admission refuses anyway), and lift fences that have run out.
fn forget_expired(early: &mut HashMap<PeerId, EarlyCancels>, now_ms: u64) {
    early.retain(|_, cancels| {
        cancels.remembered.retain(|(_, until)| now_ms <= *until);
        if cancels.fenced_until_ms.is_some_and(|until| now_ms > until) {
            cancels.fenced_until_ms = None;
        }
        !cancels.remembered.is_empty() || cancels.fenced_until_ms.is_some()
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

    /// A wall clock for the tests (epoch ms) and a request validity one minute past it.
    const NOW: u64 = 1_000_000;
    const VALID: u64 = NOW + 60_000;

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
            .admit(Uuid::from_u128(1), peer(100), 2, VALID, NOW)
            .expect("first admitted");
        let _second = registry
            .admit(Uuid::from_u128(2), peer(100), 2, VALID, NOW)
            .expect("second admitted");
        assert_eq!(
            registry
                .admit(Uuid::from_u128(3), peer(100), 2, VALID, NOW)
                .err(),
            Some(Refusal::Overloaded {
                outstanding: 2,
                bound: 2
            })
        );
        assert_eq!(
            registry
                .admit(Uuid::from_u128(2), peer(100), 3, VALID, NOW)
                .err(),
            Some(Refusal::Duplicate {
                correlation: Uuid::from_u128(2)
            })
        );
        drop(first);
        assert_eq!(registry.outstanding(), 1);
        assert!(
            registry
                .admit(Uuid::from_u128(3), peer(100), 2, VALID, NOW)
                .is_ok(),
            "the freed slot is admissible"
        );
    }

    // what this catches: a stopped pump does not orphan the generations it was serving.
    // Dropping the registry aborts every attached task; the tasks hold it only weakly.
    #[tokio::test]
    async fn dropping_the_registry_aborts_every_outstanding_generation() {
        let registry = Arc::new(ServedGenerations::default());
        let correlation = Uuid::from_u128(7);
        let admission = registry
            .admit(correlation, peer(100), 1, VALID, NOW)
            .expect("admitted");
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
        let admission = registry
            .admit(held, asker, 2, VALID, NOW)
            .expect("admitted");
        let task = tokio::spawn(async move {
            let _admission = admission;
            std::future::pending::<()>().await;
        });
        registry.attach(held, task.abort_handle());
        assert_eq!(
            registry.cancel(held, stranger, 4, VALID, NOW),
            CancelOutcome::NotYours
        );
        assert_eq!(
            registry.cancel(Uuid::from_u128(99), asker, 4, VALID, NOW),
            CancelOutcome::Remembered
        );
        assert_eq!(
            registry.outstanding(),
            1,
            "another peer's cancel left it running"
        );
        assert_eq!(
            registry.cancel(held, asker, 4, VALID, NOW),
            CancelOutcome::Cancelled
        );
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
        let admission = registry
            .admit(early, asker, 2, VALID, NOW)
            .expect("admitted");
        assert_eq!(
            registry.cancel(early, asker, 4, VALID, NOW),
            CancelOutcome::Cancelled
        );
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
    // original request's own validity and refuses that requester's request when it arrives;
    // another caller's request with the same id is not blocked.
    #[test]
    fn a_cancel_that_overtakes_its_request_refuses_it_within_its_validity() {
        let registry = Arc::new(ServedGenerations::default());
        let (asker, stranger) = (peer(1), peer(2));
        let early = Uuid::from_u128(20);
        assert_eq!(
            registry.cancel(early, asker, 4, VALID, NOW),
            CancelOutcome::Remembered
        );
        assert!(
            registry.admit(early, stranger, 4, VALID, NOW).is_ok(),
            "another caller is not blocked"
        );
        let late = Uuid::from_u128(21);
        assert_eq!(
            registry.cancel(late, asker, 4, VALID, NOW),
            CancelOutcome::Remembered
        );
        assert_eq!(
            registry.admit(late, asker, 4, VALID, NOW).err(),
            Some(Refusal::CancelledByRequester { correlation: late }),
            "the overtaken request is refused, never started"
        );
    }

    // what this catches (Codex review of 7b05b3981): the early-cancel memory is bounded by
    // the ENFORCED validity of the request, not a nominal round trip. A delayed original
    // arriving long after its cancel (far past any round-trip budget) but still inside its
    // validity is refused by the remembered cancel; once past its validity it is refused as
    // expired, so forgetting the cancel there can never let it start. The memory is not
    // capped below the validity, so no gap opens between them.
    #[test]
    fn a_delayed_original_is_refused_until_and_after_its_validity() {
        let registry = Arc::new(ServedGenerations::default());
        let asker = peer(1);
        let valid_until = NOW + 20 * 60_000;
        let delayed = Uuid::from_u128(22);
        assert_eq!(
            registry.cancel(delayed, asker, 4, valid_until, NOW),
            CancelOutcome::Remembered
        );
        let much_later = NOW + 15 * 60_000;
        assert_eq!(
            registry
                .admit(delayed, asker, 4, valid_until, much_later)
                .err(),
            Some(Refusal::CancelledByRequester {
                correlation: delayed
            }),
            "fifteen minutes on, still inside its validity: refused by its cancel"
        );
        let expired = Uuid::from_u128(23);
        assert_eq!(
            registry.cancel(expired, asker, 4, valid_until, NOW),
            CancelOutcome::Remembered
        );
        assert_eq!(
            registry
                .admit(expired, asker, 4, valid_until, valid_until + 1)
                .err(),
            Some(Refusal::Expired {
                correlation: expired
            }),
            "past its validity: refused as expired, the cancel no longer needed"
        );
        assert!(
            registry.early_cancels.lock().is_empty(),
            "the expired cancel was forgotten"
        );
    }

    // what this catches (Codex re-review of 165b35a5b): early cancels are scoped to the
    // authenticated requester. B's cancel for the same id neither overwrites nor removes
    // A's, so A's overtaken request is still refused; and a requester that overflows the
    // bound is fenced until the latest validity it held (fail closed), never has a live
    // cancel dropped.
    #[test]
    fn early_cancels_are_per_requester_and_bounded() {
        let registry = Arc::new(ServedGenerations::default());
        let (a, b) = (peer(1), peer(2));
        let id = Uuid::from_u128(30);
        assert_eq!(
            registry.cancel(id, a, 4, VALID, NOW),
            CancelOutcome::Remembered
        );
        assert_eq!(
            registry.cancel(id, b, 4, VALID, NOW),
            CancelOutcome::Remembered
        );
        assert_eq!(
            registry.admit(id, a, 4, VALID, NOW).err(),
            Some(Refusal::CancelledByRequester { correlation: id }),
            "B's cancel did not overwrite A's"
        );
        assert_eq!(
            registry.admit(id, b, 4, VALID, NOW).err(),
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
        assert_eq!(
            registry.cancel(r1, b, bound, VALID, NOW),
            CancelOutcome::Remembered
        );
        assert_eq!(
            registry.cancel(r2, b, bound, VALID, NOW),
            CancelOutcome::Fenced
        );
        assert_eq!(
            registry.admit(r1, b, 4, VALID, NOW).err(),
            Some(Refusal::Fenced { correlation: r1 }),
            "the delayed r1 is refused: its cancel was never silently dropped"
        );
        assert_eq!(
            registry.admit(r3, b, 4, VALID, NOW).err(),
            Some(Refusal::Fenced { correlation: r3 }),
            "every new generation from the fenced requester is refused while fenced"
        );
        let early = Uuid::from_u128(31);
        assert_eq!(
            registry.cancel(early, a, bound, VALID, NOW),
            CancelOutcome::Remembered
        );
        assert_eq!(
            registry.admit(early, a, bound, VALID, NOW).err(),
            Some(Refusal::CancelledByRequester { correlation: early }),
            "B's fence never touched A"
        );
        let after = VALID + 1;
        assert!(
            registry.admit(r3, b, 4, after + 60_000, after).is_ok(),
            "the fence lifts once every request it covered has expired"
        );
    }
}
