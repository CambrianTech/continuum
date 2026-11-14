#![cfg(test)]
//! Dyno 11: a restart with resident minds. What each mind pays to act again after serving comes
//! down and up, from what the M5 recorded on 2026-11-13/14 (`fixtures/dyno/2026-11-13-restart.json`):
//! a mind whose page is on disk restores it (bytes / the engine's measured restore rate), a mind
//! without one re-prefills her whole context, sharing the engine's measured prefill rate with the
//! others doing the same. Whether a mind that was out of a slot still has her page is the page
//! store's REAL decision ([`ColdPages::under`]) at the recorded pressure.

use crate::inference::slots::{ColdPages, Ratio, StorePressure};

use super::recorded::Receipted;

mod stall;

/// The restart's data file, compiled in so a run needs no file system.
const RESTART_JSON: &str = include_str!("../../../fixtures/dyno/2026-11-13-restart.json");

/// The recorded store and volume, in bytes.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub(crate) struct RecordedPressure {
    pub(crate) store_bytes: u64,
    pub(crate) store_budget_bytes: u64,
    pub(crate) volume_used_bytes: u64,
    pub(crate) volume_total_bytes: u64,
}

/// Everything the restart scenario replays.
#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct RestartDay {
    pub(crate) contexts_tokens: Receipted<Vec<u64>>,
    pub(crate) cold_prefill_tok_per_s: Receipted<u64>,
    pub(crate) page_bytes_per_token: Receipted<u64>,
    pub(crate) restore_bytes_per_ms: Receipted<u64>,
    pub(crate) save_bytes_per_ms: Receipted<u64>,
    pub(crate) store_pressure: Receipted<RecordedPressure>,
    /// How many page saves or restores the engine runs at once (the pinned fork: one).
    pub(crate) engine_page_lanes: Receipted<usize>,
    /// What copying a page's bytes in host memory costs (dyno 11b, [`stall`]).
    pub(crate) host_copy_bytes_per_ms: Receipted<u64>,
}

/// When each job completes, issued all at once in order onto `lanes` engine lanes, each
/// taking the next free lane: with one lane the sum, with a lane per job each its own time.
fn completions(jobs_ms: &[u64], lanes: usize) -> Vec<u64> {
    let mut free_at = vec![0u64; lanes.max(1)]; // max(1): an engine runs at least the job it is given
    jobs_ms
        .iter()
        .map(|job| {
            let lane = free_at.iter_mut().min().expect("test: at least one lane");
            *lane += job;
            *lane
        })
        .collect()
}

/// Where a mind's KV was when serving came down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AtStop {
    /// In a slot: the stop's checkpoint saves her page.
    Resident,
    /// Paged out earlier: her page is on disk unless the page store gave it back since.
    PagedOut,
    /// No page, whatever the stop does (the 23:45Z deploy: the ledger restored nothing).
    NoPage,
}

/// How a mind came back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Return {
    Restored,
    Reprefilled,
}

/// One mind's return after the restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MindReturn {
    pub(crate) how: Return,
    /// Tokens the engine prefilled again before her first act could start.
    pub(crate) reprefilled_tokens: u64,
    /// From the engine's ready to her first act's first new token.
    pub(crate) first_act_ms: u64,
}

/// What one restart cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Restart {
    /// What the stop's checkpoint added to the dark window.
    pub(crate) stop_save_ms: u64,
    pub(crate) minds: Vec<MindReturn>,
}

impl Restart {
    pub(crate) fn reprefilled_tokens(&self) -> u64 {
        self.minds.iter().map(|m| m.reprefilled_tokens).sum()
    }

    /// When the last mind is acting again.
    pub(crate) fn all_acting_ms(&self) -> u64 {
        self.minds.iter().map(|m| m.first_act_ms).max().unwrap_or_default() // unwrap_or_default: no minds act at once
    }
}

impl RestartDay {
    pub(crate) fn load() -> Result<Self, serde_json::Error> {
        serde_json::from_str(RESTART_JSON)
    }

    fn page_bytes(&self, tokens: u64) -> u64 {
        tokens.saturating_mul(self.page_bytes_per_token.value)
    }

    /// The page store's decision at the recorded pressure.
    pub(crate) fn recorded_cold_pages(&self) -> ColdPages {
        let p = self.store_pressure.value;
        ColdPages::under(StorePressure {
            own: Ratio::of(p.store_bytes, p.store_budget_bytes),
            volume: Ratio::of(p.volume_used_bytes, p.volume_total_bytes),
        })
    }

    /// A restart where each recorded mind was `at_stop[i]`, with the page store deciding by
    /// `cold_pages` whether a paged-out mind's page survived until the stop. The core issues
    /// every save at the stop and every restore at the boot at once (each mind's first act
    /// waits on her own restore only); the engine runs them on `page_lanes` lanes, so the stop
    /// costs the sum on one lane and the largest save on a lane each. Re-prefills share the
    /// engine's aggregate prefill rate equally while they run (processor sharing), so the
    /// smallest context is warm first.
    pub(crate) fn restart(&self, at_stop: &[AtStop], cold_pages: ColdPages, page_lanes: usize) -> Restart {
        let contexts = &self.contexts_tokens.value;
        let has_page: Vec<bool> = at_stop
            .iter()
            .map(|at| match at {
                AtStop::Resident => true,
                AtStop::PagedOut => cold_pages == ColdPages::Keep,
                AtStop::NoPage => false,
            })
            .collect();
        let saves_ms: Vec<u64> = contexts
            .iter()
            .zip(at_stop)
            .filter(|(_, at)| **at == AtStop::Resident)
            .map(|(tokens, _)| self.page_bytes(*tokens) / self.save_bytes_per_ms.value)
            .collect();
        let stop_save_ms = completions(&saves_ms, page_lanes).into_iter().max().unwrap_or_default(); // unwrap_or_default: no resident, no save

        let mut minds: Vec<Option<MindReturn>> = vec![None; contexts.len()];
        let mut restoring: Vec<(usize, u64)> = Vec::new();
        let mut cold: Vec<(usize, u64)> = Vec::new();
        for (i, (tokens, page)) in contexts.iter().copied().zip(has_page).enumerate() {
            if page {
                restoring.push((i, self.page_bytes(tokens) / self.restore_bytes_per_ms.value));
            } else {
                cold.push((i, tokens));
            }
        }
        let restores_ms: Vec<u64> = restoring.iter().map(|(_, ms)| *ms).collect();
        for ((i, _), done_ms) in restoring.iter().zip(completions(&restores_ms, page_lanes)) {
            minds[*i] = Some(MindReturn { how: Return::Restored, reprefilled_tokens: 0, first_act_ms: done_ms });
        }
        cold.sort_by_key(|(_, tokens)| *tokens);
        let (mut done_tokens, mut previous) = (0u64, 0u64);
        let rate = self.cold_prefill_tok_per_s.value;
        for (rank, (i, tokens)) in cold.iter().enumerate() {
            let sharing = (cold.len() - rank) as u64;
            done_tokens += sharing * (tokens - previous);
            previous = *tokens;
            minds[*i] = Some(MindReturn {
                how: Return::Reprefilled,
                reprefilled_tokens: *tokens,
                first_act_ms: done_tokens * 1000 / rate,
            });
        }
        Restart { stop_save_ms, minds: minds.into_iter().flatten().collect() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day() -> RestartDay {
        RestartDay::load().expect("test: the recorded 2026-11-13 restart data parses")
    }

    // what this catches (the M5, 2026-11-13 23:45Z deploy: the ledger restored nothing and every
    // mind's first act was a full prefill, `cold_turn cause="no_page"`): a restart that re-prefills
    // what a page on disk holds. Four recorded residents: cold, the last is acting again after
    // ~401 s and 253,321 tokens are prefilled again; restored on today's one-lane engine, ~5.6 s
    // of restores and 0 tokens, for ~13.5 s of saves in the stop.
    #[test]
    fn a_restart_with_four_resident_minds_restores_instead_of_re_prefilling() {
        let day = day();
        let lanes = day.engine_page_lanes.value;
        assert_eq!(lanes, 1, "the pinned fork runs one page task at a time");
        let cold = day.restart(&[AtStop::NoPage; 4], ColdPages::Keep, lanes);
        let warm = day.restart(&[AtStop::Resident; 4], ColdPages::Keep, lanes);

        assert_eq!(cold.reprefilled_tokens(), 253_321, "every mind re-prefills her whole context");
        assert!((400_000..=401_000).contains(&cold.all_acting_ms()), "cold: {} ms", cold.all_acting_ms());
        assert_eq!(cold.stop_save_ms, 0);

        assert_eq!(warm.reprefilled_tokens(), 0, "a restored prefix is never prefilled again");
        assert!(warm.minds.iter().all(|m| m.how == Return::Restored));
        assert!((5_000..=6_000).contains(&warm.all_acting_ms()), "warm: {} ms", warm.all_acting_ms());
        assert!((13_000..=14_000).contains(&warm.stop_save_ms), "the stop pays {} ms to save", warm.stop_save_ms);
        let first_cold = cold.minds.iter().map(|m| m.first_act_ms).min().expect("test: four minds");
        assert!(warm.stop_save_ms + warm.all_acting_ms() < first_cold / 10, "saving and restoring all costs under a tenth of the first cold wait");
    }

    // what this catches (Joel, 2026-11-14: "independent work runs concurrently ... never serial
    // minutes"): the core serializing independent saves or restores. Issued at once, on an engine
    // that overlaps page tasks the stop costs its LARGEST save (Atlas's 90,853 tokens, ~4.9 s,
    // not the ~13.5 s sum) and each mind acts after HER OWN restore (Aiko's 36,893 tokens ~0.8 s,
    // not behind the others). The pinned fork is one lane, so this is the gain left in the fork.
    #[test]
    fn concurrent_page_io_costs_the_largest_save_and_each_mind_only_her_own_restore() {
        let day = day();
        let serial = day.restart(&[AtStop::Resident; 4], ColdPages::Keep, 1);
        let overlapped = day.restart(&[AtStop::Resident; 4], ColdPages::Keep, 4);
        let largest_save = day.contexts_tokens.value.iter().map(|t| t * day.page_bytes_per_token.value / day.save_bytes_per_ms.value).max();
        assert_eq!(Some(overlapped.stop_save_ms), largest_save, "the stop is its largest save, not the sum");
        assert!((4_800..=4_900).contains(&overlapped.stop_save_ms), "{} ms", overlapped.stop_save_ms);
        for (mind, tokens) in overlapped.minds.iter().zip(&day.contexts_tokens.value) {
            assert_eq!(mind.first_act_ms, tokens * day.page_bytes_per_token.value / day.restore_bytes_per_ms.value, "her own restore only");
        }
        assert!(overlapped.all_acting_ms() < serial.all_acting_ms() / 2, "{overlapped:?} vs {serial:?}");
    }

    // what this catches (the same night: 29 paged-out minds' pages taken by the store at volume
    // 0.85, store 3% of its budget): a mind out of a slot at the stop returning cold because the
    // store gave her page to a volume other classes filled. Under the recorded pressure the real
    // decision keeps it; the old behaviour (yield under any pressure the broker acted on) costs her
    // her whole context.
    #[test]
    fn a_paged_out_mind_keeps_her_page_under_the_recorded_pressure() {
        let day = day();
        let lanes = day.engine_page_lanes.value;
        assert_eq!(day.recorded_cold_pages(), ColdPages::Keep, "the recorded pressure is the volume's, not the store's");
        // SHAPE, not a receipt: the second mind out of a slot at the stop (the M5 ran 5 personas on
        // 4 warm slots, `serving.plan decision="warm-slot-oversubscribed"` at 23:45:23Z).
        let at_stop = [AtStop::Resident, AtStop::PagedOut, AtStop::Resident, AtStop::Resident];

        let kept = day.restart(&at_stop, day.recorded_cold_pages(), lanes);
        assert_eq!(kept.reprefilled_tokens(), 0, "she restores like the residents");

        let yielded = day.restart(&at_stop, ColdPages::Yield, lanes);
        assert_eq!(yielded.reprefilled_tokens(), 36_893, "her page was taken: her whole context again");
        assert!(yielded.all_acting_ms() > 10 * kept.all_acting_ms(), "{yielded:?} vs {kept:?}");
    }
}
