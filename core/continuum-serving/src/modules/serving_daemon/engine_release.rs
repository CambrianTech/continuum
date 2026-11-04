//! Whether a budget reading may commit the engine's one KV pool: only once the resource authority
//! has seen the engine before this one gone (out of the lane registry, its attribution off the
//! board, and a physical scan taken after both), or this daemon has verified it serves that engine
//! (spawned or adopted, ready, the incarnation the registry names) with its bytes credited.
//!
//! Neither a time window nor stable memory proves that (Codex on card 4a20255d): an old engine can
//! hold the same bytes for minutes, and a budget that spends its credit before it leaves, or before
//! it is ours, launches a second engine into memory the first still holds. The M5, 2026-11-04
//! 20:15:16-18Z (87eccb3a7): two seconds into a boot, before serving's attribution landed
//! (`credited_bytes=0`), the plan read `usable_bytes=22,785,477,836` on a 64 GB M5 Pro and the
//! engine launched `-c 125640 --parallel 5`, kept for its life: three residents starved.

use super::*;
use crate::inference::engine_residency::EngineIncarnation;

/// Whether a budget reading may size the engine's one KV pool (`--kv-unified`, `-c`, kept for the
/// engine's life).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BudgetReading {
    /// The authority has observed every engine of ours either gone or verified ours: the reading
    /// (the replace-myself bytes of the shared physical pool at the drive mode's fill) is what a
    /// launch may lay weights + KV + staging out on.
    Settled(u64),
    /// An engine of ours is still holding memory, is not yet verified ours, or its release has not
    /// been scanned: no pool is committed from this reading. A running engine of the target model
    /// keeps its own pool; nothing launches.
    Transient,
}

/// The engines this reading is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EngineFacts {
    /// The live engine of ours the lane registry names (pidfile, record, a live llama-server).
    pub(super) registered: Option<EngineIncarnation>,
    /// The engine this daemon serves and has verified ready (spawned or adopted).
    pub(super) verified: Option<EngineIncarnation>,
}

/// What the authority has seen of an engine of ours this daemon does not serve.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Prior {
    /// None known to be holding or leaving memory.
    #[default]
    Clear,
    /// One is registered and holds memory.
    Holding,
    /// It left the registry when the authority's physical scan was `at_scan`.
    Exited { at_scan: u64 },
}

/// The budget owner's record of the engine before this one, across readings.
#[derive(Debug, Default)]
pub(super) struct EngineRelease {
    prior: Prior,
}

impl EngineRelease {
    /// The launch reading for `bytes`, read with `credit` and `scan` from one authority read.
    pub(super) fn read(&mut self, credit: OwnCredit, engines: EngineFacts, scan: u64, bytes: u64) -> BudgetReading {
        match engines.registered {
            // Verified ours: settled once its bytes are credited in the reading (adoption verified).
            Some(engine) if engines.verified == Some(engine) => {
                self.prior = Prior::Clear;
                match credit {
                    OwnCredit::Included { .. } => BudgetReading::Settled(bytes),
                    OwnCredit::ToApply => BudgetReading::Transient,
                }
            }
            // Ours but not verified: an old engine still holding memory, however stable it reads.
            Some(_) => {
                self.prior = Prior::Holding;
                BudgetReading::Transient
            }
            None => {
                if self.prior == Prior::Holding {
                    self.prior = Prior::Exited { at_scan: scan };
                }
                match (self.prior, credit) {
                    // Bytes still attributed to an engine the registry no longer names: not released.
                    (_, OwnCredit::Included { .. }) => BudgetReading::Transient,
                    // Its exit has not been scanned yet.
                    (Prior::Exited { at_scan }, OwnCredit::ToApply) if scan <= at_scan => BudgetReading::Transient,
                    (_, OwnCredit::ToApply) => {
                        self.prior = Prior::Clear;
                        BudgetReading::Settled(bytes)
                    }
                }
            }
        }
    }
}

impl ServingDaemonModule {
    /// The engines a budget reading is about: the registry's live engine of ours, and the one this
    /// daemon serves verified ready.
    pub(super) fn engine_facts(&self) -> EngineFacts {
        let registered = (self.inherited_lane)().map(|lane| lane.incarnation());
        let ready = self.serving_tx.borrow().ready;
        let verified = ready.then(|| self.server.served_incarnation()).flatten();
        EngineFacts { registered, verified }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(pid: u32) -> EngineIncarnation {
        EngineIncarnation { pid, started_s: 1_762_284_000, port: 8_181 }
    }

    const BYTES: u64 = 36_480_000_000;

    // what this catches (Codex on card 4a20255d, the M5 20:15Z boot): an old engine still
    // registered and attributed reads Transient on every reading, however long memory holds
    // still; a stable reading is not a release.
    #[test]
    fn an_old_engine_still_held_is_transient_however_long_memory_is_stable() {
        let mut release = EngineRelease::default();
        let old = EngineFacts { registered: Some(engine(41)), verified: None };
        for scan in 1..=600 {
            let credit = OwnCredit::Included { own_bytes: 21_000_000_000 };
            assert_eq!(release.read(credit, old, scan, BYTES), BudgetReading::Transient, "scan {scan}");
        }
        assert_eq!(release.read(OwnCredit::ToApply, old, 601, BYTES), BudgetReading::Transient, "unattributed yet registered");
    }

    // what this catches: the old engine's exit read as its release. Gone from the registry, its
    // attribution off the board, the reading of the same scan is still Transient; the next scan's
    // reading is Settled.
    #[test]
    fn an_old_engine_exited_and_released_settles_on_the_next_scan() {
        let mut release = EngineRelease::default();
        let old = EngineFacts { registered: Some(engine(41)), verified: None };
        let gone = EngineFacts { registered: None, verified: None };
        assert_eq!(release.read(OwnCredit::Included { own_bytes: 21_000_000_000 }, old, 7, BYTES), BudgetReading::Transient);
        assert_eq!(
            release.read(OwnCredit::Included { own_bytes: 21_000_000_000 }, gone, 8, BYTES),
            BudgetReading::Transient,
            "exited, but its attribution is still on the board"
        );
        assert_eq!(release.read(OwnCredit::ToApply, gone, 8, BYTES), BudgetReading::Transient, "the exit's own scan");
        assert_eq!(release.read(OwnCredit::ToApply, gone, 9, BYTES), BudgetReading::Settled(BYTES), "a scan after the release");
    }

    // what this catches: adoption spent before it is verified. The registered engine is settled
    // only once this daemon serves it verified ready and its bytes are credited; a cold boot with
    // no engine of ours is settled at once.
    #[test]
    fn a_verified_adoption_settles_with_its_credit_and_a_clean_boot_settles_at_once() {
        let mut release = EngineRelease::default();
        let adopted = EngineFacts { registered: Some(engine(41)), verified: Some(engine(41)) };
        assert_eq!(release.read(OwnCredit::ToApply, adopted, 3, BYTES), BudgetReading::Transient, "verified, credit not yet on the board");
        let credit = OwnCredit::Included { own_bytes: 21_000_000_000 };
        assert_eq!(release.read(credit, adopted, 4, BYTES), BudgetReading::Settled(BYTES));
        let other = EngineFacts { registered: Some(engine(41)), verified: Some(engine(57)) };
        assert_eq!(release.read(credit, other, 5, BYTES), BudgetReading::Transient, "a different incarnation is not the one adopted");
        let clean = EngineFacts { registered: None, verified: None };
        assert_eq!(EngineRelease::default().read(OwnCredit::ToApply, clean, 1, BYTES), BudgetReading::Settled(BYTES));
    }
}
