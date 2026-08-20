//! THE SUBSTITUTE-VALUE LADDER for a capacity ceiling that has not been measured yet.
//!
//! # The bug this exists to make unrepresentable (#438, measured 2026-08-19)
//!
//! `governed_vram_ceiling` honestly returns `Option<u64>` — `None` means "the governor
//! has not reported a VRAM row yet", which at boot is simply true for a second or two.
//! Every call site then wrote `.unwrap_or(0)`, and the serving planner ran with
//! `usable_gb = 0, fits_on_gpu = false` and picked the smallest model in the catalog: a
//! 0.5B, spawned on a 64 GB machine, serving template-token garbage into the room until
//! the board caught up ~90 seconds later.
//!
//! **No monitor ever measured zero.** The zero was manufactured at a type boundary,
//! where "no answer yet" was forced into a slot that only accepts a number.
//!
//! # Why `0` looked like the safe default (and why that reasoning was half right)
//!
//! Zero IS the correct conservative answer — for a GRANTER. `acquire` asking "may I hand
//! out these bytes?" against a ceiling of 0 refuses, and refusing is safe.
//!
//! Zero is a catastrophic answer for a PLANNER. "How big a model should live here?"
//! against a ceiling of 0 does not decline — it picks the tiniest thing that fits in
//! nothing, and then actuates.
//!
//! One scalar, two consumers, **opposite safety polarity**. That is the actual defect:
//! not the constant, but a single value serving two questions whose safe direction is
//! reversed. So the granter keeps refusing on unknown, and the planner gets a *prior*.
//!
//! # The ladder (this is standard practice, not an invention)
//!
//! Automotive ECUs under ISO 26262 do exactly this: when a sensor fails its plausibility
//! check, the controller substitutes a DEFINED default held in memory and falls back on
//! analytic redundancy — infer the missing signal from other sensors — then runs
//! degraded but operational. It does not feed a raw zero into the control law. The same
//! shape appears wherever a controller has to act before its state estimate converges:
//! TCP slow-start opens at a nonzero initial window, DVFS governors boot at a default
//! P-state, a Kalman filter is initialised with a prior and wide covariance rather than
//! a zeroed state.
//!
//! Our ladder, best evidence first:
//!
//! 1. **Measured** — the governed board has a VRAM row. Use it.
//! 2. **LastKnown** — the board reported earlier this process and has gone quiet. An old
//!    real number keeps the planner honest; the age rides along so a caller can judge it.
//! 3. **Estimated** — no board reading ever, but the GPU monitor knows the DEVICE'S
//!    PHYSICAL VRAM, which is a static hardware fact available before any pressure
//!    sample. Discounted by the operator headroom, because a prior should be
//!    conservative — but conservative means *smaller than the truth*, never zero.
//! 4. **Unknown** — genuinely nothing: no row, no history, no device. Only here does the
//!    planner have nothing to stand on, and only here should it decline.
//!
//! On any machine with a GPU, rungs 1–3 always produce a real number, so rung 4 is
//! unreachable in practice. That is the point: **there is no path where 0 is the best
//! available answer.** It was never the most conservative value — only the emptiest.
//!
//! # Why this reuses `FootprintReading`
//!
//! A ceiling and a footprint are the same kind of thing: a byte quantity that may be
//! measured, estimated, stale, or absent. Giving capacity its own parallel type would be
//! a second vocabulary for one concept — and two vocabularies for one concept is how the
//! gate and the ledger came to disagree about the sidecar's size. One carrier, one
//! provenance ladder, both axes.

use super::footprint_source::{FootprintReading, Provenance};
use super::lease::ResourceKind;

/// Everything known about the ceiling right now, gathered by the caller so the decision
/// itself is pure — every rung is unit-testable without a board, a GPU, or a clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct CeilingEvidence {
    /// The governed board's VRAM row, if it exists this tick.
    pub board_bytes: Option<u64>,
    /// The last board reading this process saw, and how long ago.
    pub last_good: Option<(u64, u64)>,
    /// The device's PHYSICAL VRAM from the GPU monitor — a static hardware fact, known
    /// before any pressure sample. `None` only when there is no GPU at all.
    pub device_total_bytes: Option<u64>,
}

/// How much of the device's physical VRAM a cold prior may claim. Not a new magic number:
/// it is the operator's existing VRAM headroom, the same fraction the pin fit-gate uses
/// to answer "can this model physically fit". A prior should sit UNDER the truth.
fn prior_fraction() -> f64 {
    crate::config_env::vram_headroom() as f64
}

/// Pick the best available ceiling, and say which rung it came from.
///
/// NEVER returns a fabricated zero. A `Measured(0)` can only come from the board
/// genuinely reporting zero free bytes, which is a real fact about a full GPU; the
/// absence of evidence returns [`Provenance::Unknown`], which callers must handle as
/// "I do not know" rather than as a quantity.
pub fn decide(evidence: CeilingEvidence) -> FootprintReading {
    if let Some(bytes) = evidence.board_bytes {
        return FootprintReading::measured(ResourceKind::Vram, bytes)
            .because("governed board VRAM row");
    }
    if let Some((bytes, age_ms)) = evidence.last_good {
        return FootprintReading::measured(ResourceKind::Vram, bytes)
            .aged(age_ms)
            .because("board silent — planning on the last ceiling it actually reported");
    }
    if let Some(total) = evidence.device_total_bytes {
        // Analytic redundancy: the board is silent, but the DEVICE still knows its own
        // size. Discounted, so a cold plan is conservative without being empty.
        let prior = (total as f64 * prior_fraction()) as u64;
        if prior > 0 {
            return FootprintReading::estimated(ResourceKind::Vram, prior).because(
                "no board reading yet — planning on the device's physical VRAM, discounted",
            );
        }
    }
    FootprintReading::unknown(ResourceKind::Vram)
        .because("no board row, no prior reading, and no GPU device reports a size")
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICE: u64 = 64 * 1024 * 1024 * 1024;

    // what this catches: THE #438 DEFECT ITSELF. Before this, an unreported board meant
    // `usable_gb = 0` and the planner picked a 0.5B on a 64 GB machine. A cold boot must
    // yield a real, usable, conservative number — never an empty one.
    #[test]
    fn a_cold_boot_plans_on_the_device_not_on_zero() {
        let r = decide(CeilingEvidence {
            board_bytes: None,
            last_good: None,
            device_total_bytes: Some(DEVICE),
        });
        let bytes = r.usable_bytes().expect("a cold ceiling must still be usable");
        assert!(bytes > 0, "the whole bug: a cold ceiling must never be zero");
        assert!(
            bytes <= DEVICE,
            "a prior must sit under the device's real size, never over it"
        );
        assert_eq!(r.provenance, Provenance::Estimated, "and it must SAY it is a guess");
    }

    // what this catches: a prior outranking a real measurement. Evidence order is the
    // whole contract — a live board reading must always win, however good the prior.
    #[test]
    fn a_real_board_reading_outranks_every_prior() {
        let r = decide(CeilingEvidence {
            board_bytes: Some(55_662_788_608),
            last_good: Some((1, 5)),
            device_total_bytes: Some(DEVICE),
        });
        assert_eq!(r.bytes, 55_662_788_608);
        assert_eq!(r.provenance, Provenance::Measured);
    }

    // what this catches: a stale reading being dressed up as fresh, or being discarded in
    // favour of a weaker device estimate. Last-known beats a cold guess AND admits its age.
    #[test]
    fn a_stale_reading_beats_a_cold_guess_and_admits_its_age() {
        let r = decide(CeilingEvidence {
            board_bytes: None,
            last_good: Some((40 * 1024 * 1024 * 1024, 2_500)),
            device_total_bytes: Some(DEVICE),
        });
        assert_eq!(r.bytes, 40 * 1024 * 1024 * 1024);
        assert_eq!(r.provenance, Provenance::LastKnown { age_ms: 2_500 });
    }

    // what this catches: the ladder inventing a number when it truly has none. A machine
    // with no GPU must read UNKNOWN — which the granter refuses on — and must NOT read as
    // a zero-byte quantity, because those are different facts (the day's whole lesson).
    #[test]
    fn genuinely_no_evidence_is_unknown_and_not_a_quantity() {
        let r = decide(CeilingEvidence::default());
        assert_eq!(r.usable_bytes(), None);
        assert_eq!(r.provenance, Provenance::Unknown);
        assert!(!r.note.is_empty(), "an unknown must still explain itself");
    }

    // what this catches: a board that HONESTLY reports zero free bytes being confused with
    // a missing board. A full GPU is a real measurement and must stay usable — the planner
    // should decline because there is no room, not because it has no information.
    #[test]
    fn a_genuinely_full_gpu_still_reads_as_measured_zero() {
        let r = decide(CeilingEvidence {
            board_bytes: Some(0),
            ..Default::default()
        });
        assert_eq!(r.usable_bytes(), Some(0), "a measured zero IS an answer");
        assert_eq!(r.provenance, Provenance::Measured);
    }
}
