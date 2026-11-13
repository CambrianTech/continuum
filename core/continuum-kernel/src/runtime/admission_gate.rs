//! ONE ADMISSION GATE, two users.
//!
//! A stopping node has the same question in more than one place: *may new work be
//! accepted, and how much accepted work is still running?* Both halves must be read and
//! written together — a flag beside a counter is a race, because a close can land between
//! the check and the increment, and a drain then reads zero while a producer is about to
//! make it one.
//!
//! This is that pair in a single word: high bit CLOSED, low 63 bits the in-flight count,
//! admission by `compare_exchange`. It exists as a TYPE rather than a pattern because it
//! was written twice — once for persona turns, once for the log queue — and the second
//! copy reintroduced the exact race the first had just removed. Two implementations of an
//! invariant are two chances to get it wrong; the review that caught the second copy said
//! "use one-word CAS like the turn gate", which is this.
//!
//! Instance methods, not free functions over a global, so a test can drive the REAL
//! admission and the REAL close on its own gate. The previous shape was a process-wide
//! one-way static, which meant any test exercising a real close poisoned every later test
//! in the binary — so the tests reimplemented the logic instead of calling it, and could
//! not fail when the production copy regressed.

use std::sync::atomic::{AtomicU64, Ordering};

/// High bit. Leaves 63 bits of count, which is not a limit anyone reaches: the count is
/// concurrent units of work, bounded by citizens or queue capacity.
const CLOSED: u64 = 1 << 63;

/// Admission state for one resource: open/closed plus the in-flight count, in one word.
pub struct AdmissionGate {
    word: AtomicU64,
}

impl AdmissionGate {
    /// A new, OPEN gate with nothing in flight. `const` so it can be a `static` without
    /// a lazy initialiser.
    pub const fn new() -> Self {
        Self {
            word: AtomicU64::new(0),
        }
    }

    /// Is the gate still accepting? Prefer [`admit`](Self::admit), which answers this AND
    /// takes ownership in the same atomic step; a bare read is only safe for display.
    pub fn is_open(&self) -> bool {
        self.word.load(Ordering::Acquire) & CLOSED == 0
    }

    /// Units of work admitted and not yet released.
    pub fn in_flight(&self) -> u64 {
        self.word.load(Ordering::Acquire) & !CLOSED
    }

    /// ADMIT one unit of work, or refuse because the gate is closed.
    ///
    /// The returned permit owns it: counted on creation, uncounted on drop, so the count
    /// is right on every exit path including `?`, `continue`, `break` and unwind. A manual
    /// decrement has to be repeated at each of those and will be missed at the next one
    /// added, and a leaked count is a drain that waits its whole budget for work that
    /// finished long ago.
    ///
    /// The CAS is what makes this safe: an admission either lands before a close — and is
    /// therefore in the count that close carries, so a drain waits for it — or observes
    /// the close and refuses. There is no interleaving in which work starts unseen.
    pub fn admit(&self) -> Option<Permit<'_>> {
        // A no-op hook, monomorphised away: production runs the identical loop.
        self.admit_hooked(|| {})
    }

    /// `admit`, with a hook run between the LOAD and the compare-exchange.
    ///
    /// The hook exists so a test can make the CAS genuinely fail. A test that simply
    /// admits twice in sequence never contends, so the retry branch is never taken and a
    /// test named for it proves nothing — which is what the first version of the retry
    /// regression did. Mutating the word from inside the hook guarantees the first
    /// exchange sees a stale value, deterministically, with no threads and no timing.
    ///
    /// Private: the hook is a test seam, not an API. Production reaches this only through
    /// [`admit`](Self::admit) with an empty closure.
    fn admit_hooked(&self, mut between_load_and_cas: impl FnMut()) -> Option<Permit<'_>> {
        let mut cur = self.word.load(Ordering::Acquire);
        loop {
            if cur & CLOSED != 0 {
                return None;
            }
            between_load_and_cas();
            // The count is the low bits, so `cur + 1` increments it and leaves the (here
            // necessarily clear) closed bit alone.
            match self
                .word
                .compare_exchange(cur, cur + 1, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return Some(Permit { gate: self }),
                // Someone else moved the word: re-read and retry against what is there
                // now, rather than against the stale value we loaded. Re-reading is the
                // whole point — retrying with `cur` unchanged would either spin forever or
                // clobber the other writer's increment.
                Err(actual) => cur = actual,
            }
        }
    }

    /// Release one unit of work whose permit was FORGOTTEN because ownership crossed a
    /// thread boundary.
    ///
    /// The RAII permit is the right tool whenever admission and completion happen in one
    /// scope, and it should be used wherever it can be. It cannot span the log queue: the
    /// producer admits an entry, hands it to a writer THREAD, and the writer is what knows
    /// when the entry has actually been written. A permit dropped at the send would uncount
    /// an entry still sitting in the channel, and a drain would then call the module quiet
    /// while the writer still had work.
    ///
    /// So the contract for this method is narrow and must stay narrow: **it is only
    /// correct when a permit was `mem::forget`ed at a hand-off, and it must be called
    /// exactly once for each one.** Anywhere both ends are in the same scope, use the
    /// permit and let it drop — a manual decrement has to be repeated at every exit path
    /// and will be missed at the next one added.
    ///
    /// Saturating, so a double release cannot wrap the count into the closed bit and
    /// reopen a shut gate. A double release is still a bug; this only bounds the damage.
    pub fn release_one(&self) {
        let _ = self
            .word
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |cur| {
                let count = cur & !CLOSED;
                if count == 0 {
                    // Nothing to release. Refuse rather than wrap: `cur - 1` here would
                    // borrow from the closed bit and silently REOPEN the gate.
                    None
                } else {
                    Some((cur & CLOSED) | (count - 1))
                }
            });
    }

    /// Shut the gate. Returns whether THIS call closed it, so a second caller — a signal
    /// racing a stop verb, a retried request — can tell it is re-entering rather than
    /// report a fresh close.
    ///
    /// One-way by design: the only caller is a stop, and a reopen would exist to be called
    /// by mistake. A gate is re-opened by constructing a new one.
    pub fn close(&self) -> bool {
        self.word.fetch_or(CLOSED, Ordering::AcqRel) & CLOSED == 0
    }
}

impl Default for AdmissionGate {
    fn default() -> Self {
        Self::new()
    }
}

/// Ownership of one admitted unit of work. See [`AdmissionGate::admit`].
#[must_use = "the permit IS the admission; dropping it immediately releases the work"]
pub struct Permit<'a> {
    gate: &'a AdmissionGate,
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        // Subtracting 1 from the whole word decrements the low-bit count and cannot reach
        // the closed bit, because a permit only exists when the count was at least 1.
        self.gate.word.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: THE RACE, driven through the REAL admit/close rather than a
    // reimplementation of them. With an open flag and a count as separate atomics this
    // order was reachable: admit reads OPEN, close lands, drain reads 0 and saves, admit
    // then increments and starts work behind the drain's back — the torn save the drain
    // exists to prevent, caused by the drain.
    #[test]
    fn work_can_never_be_admitted_after_the_gate_reads_closed() {
        let gate = AdmissionGate::new();
        assert!(gate.close(), "first close is the closer");
        assert!(
            gate.admit().is_none(),
            "an admission after close must be refused, not deferred"
        );
        assert_eq!(
            gate.in_flight(),
            0,
            "and the drain's zero is permanently true, because the refusal cannot become work"
        );
    }

    // what this catches: an admission that WON the race being invisible to the close that
    // follows it — the drain would then wait for nothing and save over live work.
    #[test]
    fn an_admission_that_wins_is_carried_by_the_close_that_follows() {
        let gate = AdmissionGate::new();
        let permit = gate.admit().expect("open gate admits");
        assert!(gate.close());
        assert_eq!(
            gate.in_flight(),
            1,
            "the close must carry the work admitted just before it"
        );
        assert!(gate.admit().is_none());
        drop(permit);
        assert_eq!(gate.in_flight(), 0, "the drain's wait ends when the work does");
        assert!(!gate.is_open(), "releasing the last permit must not reopen the gate");
    }

    // what this catches: the closed bit and the count disturbing each other. They share
    // one word, so an arithmetic slip either way reopens a shut gate or corrupts the
    // number a drain waits on.
    #[test]
    fn the_count_and_the_closed_bit_survive_each_other_in_any_order() {
        let gate = AdmissionGate::new();
        let a = gate.admit().expect("open");
        let b = gate.admit().expect("open");
        let c = gate.admit().expect("open");
        assert_eq!(gate.in_flight(), 3);
        drop(b); // one finishes BEFORE the close
        gate.close();
        assert_eq!(gate.in_flight(), 2, "the close carries what was still running");
        drop(a);
        drop(c); // and the rest finish after it, out of admission order
        assert_eq!(gate.in_flight(), 0);
        assert!(!gate.is_open());
    }

    // what this catches: `close` reporting a fresh close every time. Both a signal handler
    // and a stop verb reach the same broadcast, so close is called twice on an ordinary
    // stop; the second must say it was already shut rather than look like a new event.
    #[test]
    fn a_second_close_reports_that_it_was_already_shut() {
        let gate = AdmissionGate::new();
        let _held = gate.admit().expect("open");
        assert!(gate.close(), "first close closed it");
        assert!(!gate.close(), "second close must report it was already shut");
        assert_eq!(gate.in_flight(), 1, "and must not disturb the count");
    }

    // what this catches: the CAS retry path dropping or clobbering an admission.
    //
    // The FIRST version of this test admitted several times in sequence and asserted the
    // count — which never contends, so the retry branch it was named for was never taken.
    // A test named for an invariant that never drives the invariant is worse than a
    // missing one, because it is counted. Astra caught it.
    //
    // This one FORCES a failed exchange: the hook fires between the load and the CAS and
    // moves the word, so the first attempt is guaranteed to see a stale value. No threads,
    // no timing, no flake — the interleaving is driven, not hoped for.
    #[test]
    fn a_failed_exchange_retries_against_the_current_word_not_the_stale_one() {
        let gate = AdmissionGate::new();
        let interferences = std::cell::Cell::new(0u32);

        // Steal the word exactly once, on the first attempt only. A hook that fired every
        // time would spin forever, which is itself worth knowing: the retry must make
        // progress against a word that STOPS moving.
        let permit = gate
            .admit_hooked(|| {
                if interferences.get() == 0 {
                    interferences.set(1);
                    // Another admission lands between our load and our exchange.
                    gate.word.fetch_add(1, Ordering::AcqRel);
                }
            })
            .expect("an open gate admits even when it has to retry");

        assert_eq!(interferences.get(), 1, "the hook must have contended once");
        assert_eq!(
            gate.in_flight(),
            2,
            "the retry must ADD to the interfering write, not replace it — a retry against              the stale word would have stored 1 and erased the other admission"
        );
        drop(permit);
        assert_eq!(gate.in_flight(), 1, "and only OUR admission is released");
    }

    // what this catches: a close that lands during the retry being ignored. The loop
    // re-reads on failure, so it must re-check CLOSED on the new value too — retrying
    // blindly would admit work into a gate that shut while we were contending.
    #[test]
    fn a_close_that_lands_during_a_retry_refuses_the_admission() {
        let gate = AdmissionGate::new();
        let fired = std::cell::Cell::new(false);
        let outcome = gate.admit_hooked(|| {
            if !fired.get() {
                fired.set(true);
                gate.close(); // shuts between our load and our exchange
            }
        });
        assert!(fired.get(), "the hook must have run");
        assert!(
            outcome.is_none(),
            "an admission whose CAS lost to a CLOSE must be refused, not retried into a              closed gate"
        );
        assert_eq!(gate.in_flight(), 0);
        assert!(!gate.is_open());
    }

    // what this catches: a double release wrapping the count and REOPENING a closed gate.
    // The count and the closed bit share a word, so `cur - 1` at zero would borrow from
    // the high bit — turning a stopping node back into an accepting one, which is the
    // worst possible direction for this bug.
    #[test]
    fn releasing_more_than_was_admitted_cannot_reopen_the_gate() {
        let gate = AdmissionGate::new();
        gate.close();
        assert_eq!(gate.in_flight(), 0);
        gate.release_one(); // unpaired: a bug, but it must not be a catastrophic one
        gate.release_one();
        assert_eq!(gate.in_flight(), 0, "the count must not wrap");
        assert!(!gate.is_open(), "a closed gate must stay closed");
        assert!(gate.admit().is_none());
    }

    // what this catches: the hand-off path miscounting. The log queue forgets its permit
    // at the send and the writer releases after the write, so an entry being written is
    // still in flight — a drain that stopped at the hand-off would race the write.
    #[test]
    fn a_forgotten_permit_stays_counted_until_the_holder_releases_it() {
        let gate = AdmissionGate::new();
        let permit = gate.admit().expect("open");
        std::mem::forget(permit); // handed to another thread
        assert_eq!(gate.in_flight(), 1, "the work is still in flight after the hand-off");
        gate.release_one(); // the holder finished it
        assert_eq!(gate.in_flight(), 0);
    }

    // what this catches: a gate defaulting CLOSED, which would make every producer in a
    // normally-booted core refuse — a total, silent outage that no test of the shutdown
    // path would exercise.
    #[test]
    fn a_new_gate_is_open_and_empty() {
        let gate = AdmissionGate::new();
        assert!(gate.is_open());
        assert_eq!(gate.in_flight(), 0);
        assert!(gate.admit().is_some());
    }
}
