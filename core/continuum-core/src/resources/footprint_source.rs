//! ONE adapter per holder answers "how many bytes are you holding right now".
//!
//! # Why this is a seam and not a function call
//!
//! Every consumer's residency is a different KIND of question. Serving's is derivable
//! from the live catalog row plus the served shape. The vision sidecar's is the same
//! derivation against a different row. The core process's is its own RSS. A training
//! job's is whatever the trainer reports. If each call site assembles its own answer,
//! the same holder gets two sizes depending on who asked — which is exactly how VRAM
//! and RAM came to disagree by 23 GB about one physical pool (#56).
//!
//! So: one adapter per holder, and it is the ONLY thing that knows how that holder is
//! sized. It is explicitly ALLOWED to be crude inside — a hardcoded figure, a catalog
//! estimate, last-measured-with-age — and to keep whatever state that needs. What it
//! may not do is let its crudeness be invisible: every reading carries its
//! [`Provenance`], so a caller can tell a real measurement from a guess.
//!
//! # The acyclicity rule (Joel, 2026-08-19: "don't craft some recursive loop")
//!
//! **A footprint adapter is a LEAF.** It may read hardware, the model catalog, a
//! process table, its own cached state. It may NOT read the governor, a budget, a
//! lease board, or another consumer's footprint.
//!
//! This is not stylistic. The governor computes budgets FROM footprints
//! (`budget_for_replacing` = available + own residency). If a footprint could consult a
//! budget, the definition would be circular and the value would depend on evaluation
//! order — the same shape as the self-eviction ratchet, but non-terminating instead of
//! merely wrong. The trait is deliberately given no governor handle, so the cycle is
//! unrepresentable rather than merely discouraged.
//!
//! # Unknown is not zero
//!
//! The pre-existing `serving_footprint_fn` ends in `.unwrap_or(0)`: a live lane whose
//! catalog row will not resolve reports ZERO bytes held. As a board display that is
//! cosmetic. As a term in a budget it is the defect class that cost this project a
//! full day — `sysinfo::available_memory()` returning 0 while the machine was 54 GB
//! used and swapping, and every consumer believing the box was empty. A reading that
//! cannot be made must SAY so, and the caller must decide; it must never silently
//! become a number that happens to parse.

use std::sync::atomic::{AtomicU64, Ordering};

use super::lease::ResourceKind;

/// How a reading was obtained — the difference between "I measured this" and "I am
/// guessing", carried with the number so it can never be lost in transit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    /// Read from the thing itself right now (process RSS, a device query).
    Measured,
    /// Derived from metadata — a catalog row's weights + the served shape. Accurate
    /// when the metadata is, and it moves when the model or shape moves, which is the
    /// property that matters most.
    Estimated,
    /// The adapter could not read anything this tick and is reporting the last value it
    /// did obtain, `age_ms` ago. Stale beats fabricated: an old real number keeps a
    /// governor conservative; a zero invites over-grant.
    LastKnown { age_ms: u64 },
    /// Nothing has ever been obtained. NOT zero bytes — no answer at all.
    Unknown,
}

impl Provenance {
    /// Whether a budget may safely be computed from this reading. `Unknown` must not
    /// silently contribute a zero, and the caller is expected to fail loud instead.
    pub fn is_usable(&self) -> bool {
        !matches!(self, Provenance::Unknown)
    }
}

/// One holder's residency for one axis, with the provenance of the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FootprintReading {
    pub kind: ResourceKind,
    /// Bytes held. Meaningless unless `provenance.is_usable()`.
    pub bytes: u64,
    pub provenance: Provenance,
    /// WHY this number, in the adapter's own words — carried WITH the reading so a
    /// consumer, a board row and a probe all render the same explanation instead of
    /// three call sites re-deriving it.
    ///
    /// Measured live 2026-08-19: the board showed `vision 0.00 GB` and nothing on the
    /// row could distinguish "no vision provider is running, personas are blind" from
    /// "the main lane's own model sees, so vision is free". Same number, opposite
    /// meanings, opposite responses. A zero without its reason is unactionable.
    pub note: &'static str,
}

impl FootprintReading {
    pub fn measured(kind: ResourceKind, bytes: u64) -> Self {
        Self {
            kind,
            bytes,
            provenance: Provenance::Measured,
            note: "",
        }
    }

    pub fn estimated(kind: ResourceKind, bytes: u64) -> Self {
        Self {
            kind,
            bytes,
            provenance: Provenance::Estimated,
            note: "",
        }
    }

    pub fn unknown(kind: ResourceKind) -> Self {
        Self {
            kind,
            bytes: 0,
            provenance: Provenance::Unknown,
            note: "",
        }
    }

    /// Attach the explanation. Chainable so an adapter states the reason at the exact
    /// branch that decided it, never reconstructed later from the number.
    pub fn because(mut self, note: &'static str) -> Self {
        self.note = note;
        self
    }

    /// Demote a reading to [`Provenance::LastKnown`] with its age — the value was real
    /// once and is being re-reported now. Kept as a transition on the constructor so a
    /// stale reading can never be built by hand as a fresh `Measured` one.
    pub fn aged(mut self, age_ms: u64) -> Self {
        self.provenance = Provenance::LastKnown { age_ms };
        self
    }

    /// Bytes if the reading can be used in arithmetic, else `None` — the call shape
    /// that makes "unknown" impossible to accidentally treat as zero.
    pub fn usable_bytes(&self) -> Option<u64> {
        self.provenance.is_usable().then_some(self.bytes)
    }
}

/// THE question "how big is this holder", asked of the one thing entitled to answer.
///
/// Implementations are leaves — see the module's acyclicity rule. The trait takes no
/// governor, no board, and no other source, so an implementation physically cannot
/// build a cycle without importing something the signature never gives it.
pub trait FootprintSource: Send + Sync {
    /// The holder this answers for. Must match its row in
    /// [`standard_memory_holders`](super::holders::standard_memory_holders) and, if the
    /// holder registers as a consumer, its `consumer_id`.
    fn holder_id(&self) -> &str;

    /// Residency right now, one reading per axis this holder draws on. Called on the
    /// daemon's poll, so it must not block on I/O it does not control.
    fn read(&self) -> Vec<FootprintReading>;
}

/// Last-known-value memory for adapters whose live read can transiently fail.
///
/// Provided so every adapter does not hand-roll the same ladder (live → last → never).
/// Holds only its own bytes + timestamp; it consults nothing, which keeps adapters
/// leaves by construction.
#[derive(Debug, Default)]
pub struct LastKnown {
    bytes: AtomicU64,
    /// Epoch ms of the last successful read. 0 = never.
    at_ms: AtomicU64,
}

impl LastKnown {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a successful live read.
    pub fn record(&self, bytes: u64, now_ms: u64) {
        self.bytes.store(bytes, Ordering::Relaxed);
        self.at_ms.store(now_ms, Ordering::Relaxed);
    }

    /// Fall back to the last good value, aged. `Unknown` when there has never been one
    /// — deliberately NOT zero bytes.
    pub fn reading(&self, kind: ResourceKind, now_ms: u64) -> FootprintReading {
        let at = self.at_ms.load(Ordering::Relaxed);
        if at == 0 {
            return FootprintReading::unknown(kind).because("no live read has ever succeeded");
        }
        // Built through the constructor, never a struct literal: a literal must be
        // updated at every construction site each time the type grows a field, which is
        // how `note` came to be missing here in the first place. One builder, one place.
        FootprintReading::measured(kind, self.bytes.load(Ordering::Relaxed))
            .aged(now_ms.saturating_sub(at))
            .because("live read failed; reporting the last value actually obtained")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: an unknown reading being usable as a number. This is the
    // `.unwrap_or(0)` defect expressed as a type: a live holder whose size cannot be
    // determined must not contribute a zero to anyone's budget. The same silent zero
    // (from `sysinfo::available_memory()`) had the governor believing a 54 GB-used,
    // swapping machine was empty.
    #[test]
    fn unknown_is_not_zero_bytes() {
        let r = FootprintReading::unknown(ResourceKind::Vram);
        assert_eq!(r.usable_bytes(), None, "unknown must not read as a quantity");
        assert!(!r.provenance.is_usable());
        // A real zero IS usable — a holder can legitimately hold nothing.
        let empty = FootprintReading::measured(ResourceKind::Vram, 0);
        assert_eq!(empty.usable_bytes(), Some(0));
    }

    // what this catches: provenance being dropped on the way to a caller, so an
    // estimate gets treated as a measurement. The distinction only helps if it survives.
    #[test]
    fn provenance_survives_and_distinguishes_estimate_from_measurement() {
        assert_eq!(
            FootprintReading::estimated(ResourceKind::Ram, 5).provenance,
            Provenance::Estimated
        );
        assert_eq!(
            FootprintReading::measured(ResourceKind::Ram, 5).provenance,
            Provenance::Measured
        );
        assert!(Provenance::Estimated.is_usable(), "an estimate is still an answer");
    }

    // what this catches: the fallback ladder fabricating a value before any real read.
    // "Never read" and "read 0 bytes" are different facts and must stay different.
    #[test]
    fn last_known_reports_unknown_until_something_is_actually_recorded() {
        let lk = LastKnown::new();
        assert_eq!(
            lk.reading(ResourceKind::Vram, 1_000).provenance,
            Provenance::Unknown
        );
        lk.record(9_400_000_000, 1_000);
        let r = lk.reading(ResourceKind::Vram, 3_500);
        assert_eq!(r.bytes, 9_400_000_000);
        assert_eq!(r.provenance, Provenance::LastKnown { age_ms: 2_500 });
        assert_eq!(r.usable_bytes(), Some(9_400_000_000));
    }

    // what this catches: a clock that goes backwards producing a wrapped, enormous age
    // that would make a stale reading look fresh.
    #[test]
    fn a_backwards_clock_cannot_forge_a_fresh_age() {
        let lk = LastKnown::new();
        lk.record(100, 5_000);
        assert_eq!(
            lk.reading(ResourceKind::Ram, 1_000).provenance,
            Provenance::LastKnown { age_ms: 0 },
            "saturating, never wrapping into a bogus age"
        );
    }
}
