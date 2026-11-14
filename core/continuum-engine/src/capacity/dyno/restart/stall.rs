//! Dyno 11b: what a KV page switch costs the OTHER minds' decode. The pinned fork runs a whole
//! save or restore (the KV copy and the file IO) as one task on its single task loop, so every
//! slot's decode waits for it and switches issued together wait for each other (the M5
//! 2026-11-14 00:18:13Z: Benchy's restore waited 1,024 ms, Aris's 1,012 ms in the engine).
//! Streamed, the loop only copies the sequence's state out (`llama_state_seq_get_data`) or in
//! (`set_data`), a memcpy proportional to the bytes; the file write or read runs on a worker per
//! switch and its completion is an event back to the loop. Rates are the restart day's
//! receipts ([`RestartDay`]); the copy rate is the M5's host memcpy until the deployed engine's
//! `copy_ms` receipt replaces it.

use super::{completions, RestartDay};

/// Where the engine runs a page switch's work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageIo {
    /// The pinned fork: the whole switch is one task on the loop.
    OnTheLoop,
    /// The loop copies the state in or out; the file IO runs on a worker per switch.
    Streamed,
}

/// Which way a page moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Way {
    Save,
    Restore,
}

/// One switch the core issues: a mind's page of `tokens`, moving `way`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Switch {
    pub(crate) way: Way,
    pub(crate) tokens: u64,
}

/// What a set of switches issued at once costs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stall {
    /// How long every slot not being switched cannot decode: the loop's time inside switches.
    pub(crate) decode_stall_ms: u64,
    /// When each switch's receipt is sent, from the issue, in the order issued.
    pub(crate) done_ms: Vec<u64>,
}

impl RestartDay {
    /// The file IO of one switch at the engine's measured rate. The measured save rate includes
    /// the copy the loop now does, so a streamed save's IO is over-counted by at most its copy.
    fn io_ms(&self, s: Switch) -> u64 {
        let rate = match s.way {
            Way::Save => self.save_bytes_per_ms.value,
            Way::Restore => self.restore_bytes_per_ms.value,
        };
        self.page_bytes(s.tokens) / rate
    }

    /// The loop's share of a streamed switch: its bytes at host memcpy, rounded up. The page's
    /// bytes include its checkpoint sidecar; a restore only moves the sidecar's checkpoints into
    /// the slot, so the restore's copy is an upper bound.
    fn copy_ms(&self, s: Switch) -> u64 {
        self.page_bytes(s.tokens).div_ceil(self.host_copy_bytes_per_ms.value)
    }

    /// `switches` issued at once onto an engine that runs them `io`. Streamed, each switch has its
    /// own worker (the fork sizes the pool from its slots); the loop takes the copy steps one at a
    /// time: a save's at its issue, a restore's when its read completes.
    pub(crate) fn switches_at_once(&self, switches: &[Switch], io: PageIo) -> Stall {
        match io {
            PageIo::OnTheLoop => {
                let jobs: Vec<u64> = switches.iter().map(|s| self.io_ms(*s)).collect();
                Stall { decode_stall_ms: jobs.iter().sum(), done_ms: completions(&jobs, 1) }
            }
            PageIo::Streamed => {
                let mut done_ms = vec![0u64; switches.len()];
                let mut loop_free = 0u64;
                for (i, s) in switches.iter().enumerate().filter(|(_, s)| s.way == Way::Save) {
                    loop_free += self.copy_ms(*s);
                    done_ms[i] = loop_free + self.io_ms(*s);
                }
                let mut reads: Vec<(u64, usize)> = switches
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.way == Way::Restore)
                    .map(|(i, s)| (self.io_ms(*s), i))
                    .collect();
                reads.sort_unstable();
                for (read_done, i) in reads {
                    loop_free = loop_free.max(read_done) + self.copy_ms(switches[i]);
                    done_ms[i] = loop_free;
                }
                let decode_stall_ms = switches.iter().map(|s| self.copy_ms(*s)).sum();
                Stall { decode_stall_ms, done_ms }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day() -> RestartDay {
        RestartDay::load().expect("test: the recorded 2026-11-13 restart data parses")
    }

    fn saves(day: &RestartDay) -> Vec<Switch> {
        day.contexts_tokens.value.iter().map(|&tokens| Switch { way: Way::Save, tokens }).collect()
    }

    // what this catches: a page save that holds the engine's loop for its disk write. The stop's
    // checkpoint saves the four recorded residents at once: on the loop every other slot cannot
    // decode for the ~13.5 s sum; streamed, the loop holds only the four copies (~0.2 s) and the
    // last receipt arrives after the largest save plus the copies ahead of it (~5.0 s).
    #[test]
    fn four_saves_at_once_stall_decode_for_their_copies_not_their_writes() {
        let day = day();
        let before = day.switches_at_once(&saves(&day), PageIo::OnTheLoop);
        let after = day.switches_at_once(&saves(&day), PageIo::Streamed);
        assert!((13_000..=14_000).contains(&before.decode_stall_ms), "{before:?}");
        assert!((150..=250).contains(&after.decode_stall_ms), "{after:?}");
        assert!(after.decode_stall_ms * 50 < before.decode_stall_ms, "streamed stalls decode under a fiftieth: {after:?} vs {before:?}");
        let last = after.done_ms.iter().max().copied().unwrap_or_default(); // unwrap_or_default: four saves were issued
        assert!((4_900..=5_100).contains(&last), "the last receipt is the largest save plus the copies before it: {after:?}");
        assert!(last < before.done_ms.iter().max().copied().unwrap_or_default() / 2); // unwrap_or_default: as above
    }

    // what this catches (the M5 2026-11-14 00:18:13Z): two restores issued together, the second
    // waiting out the first in the engine (Benchy 1,024 ms behind Aris's 1,012 ms). Streamed, each
    // mind waits for her own read and one set step (~1.0 s each), and the other slots' decode
    // stalls ~70 ms instead of ~1.9 s.
    #[test]
    fn two_restores_issued_together_each_wait_only_for_their_own_read() {
        let day = day();
        let pair = [Switch { way: Way::Restore, tokens: 43_389 }, Switch { way: Way::Restore, tokens: 43_637 }];
        let before = day.switches_at_once(&pair, PageIo::OnTheLoop);
        let after = day.switches_at_once(&pair, PageIo::Streamed);
        assert!((1_850..=1_950).contains(&before.done_ms[1]), "the second waits the first out: {before:?}");
        assert!(after.done_ms.iter().all(|ms| (950..=1_050).contains(ms)), "{after:?}");
        assert!((60..=80).contains(&after.decode_stall_ms), "{after:?}");
        assert!(after.decode_stall_ms * 20 < before.decode_stall_ms, "{after:?} vs {before:?}");
    }

    // what this catches: a streamed save's receipt counted before its file exists. A switch's
    // receipt is never earlier than its own IO, whatever else is in flight.
    #[test]
    fn no_receipt_precedes_its_own_io() {
        let day = day();
        let mut mixed = saves(&day);
        mixed.push(Switch { way: Way::Restore, tokens: 43_389 });
        let after = day.switches_at_once(&mixed, PageIo::Streamed);
        for (s, done) in mixed.iter().zip(&after.done_ms) {
            assert!(*done >= day.io_ms(*s) + day.copy_ms(*s), "{s:?} done at {done} ms");
        }
    }
}
