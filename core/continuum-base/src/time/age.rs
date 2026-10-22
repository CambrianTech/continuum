//! How old something is, in the words a reader can act on.
//!
//! ONE formatter for every surface that tells a citizen an age (recall, the work board).
//! An age, never a timestamp: a model reads a date against a training-era sense of
//! "now" and misjudges it by years (Joel, 2026-10-06); "23h ago" needs no clock.
//!
//! ## Coarse on purpose: an age in a prompt must not tick
//!
//! The prompt cache reuses only a common PREFIX, and a surface that carries an age is
//! re-read every turn. When this formatter wrote the exact minute ("41m"), a board line
//! changed every minute and forfeited every byte behind it: the M5's own
//! `delib.prompt.reuse_split` probe, 2026-10-10, named it twice —
//! `diverged_in="message 3" was="3m — you can take it over…" is="2m — you can take…"`
//! with 7,291 of 14,511 tokens reused, and `message 4`, `was="5m — …"`. On a CPU seat
//! that re-prefill is the whole turn (card 5346e86a, IntelMac: 25k tokens prefilled a
//! turn, 28-minute first tokens). Recall already freezes its rendered bytes for the
//! same reason (`recall_faculty` sticky rendering); the board line never got that, and
//! a second cache is not the fix. The fix is the ONE formatter: ages are log-spaced
//! BUCKETS, labelled by their floor ("5m+", "2h+", "1d+"), so an age renders the same
//! bytes for the whole bucket and the string changes about once per doubling, never
//! per minute. A reader judges "take it over?" from "silent 40m+" exactly as from
//! "silent 41m".

/// How long ago something happened: a span of milliseconds that renders in the coarse
/// buckets above, never as a timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Age(u64);

impl Age {
    pub const fn from_millis(millis: u64) -> Self {
        Age(millis)
    }

    pub const fn millis(self) -> u64 {
        self.0
    }

    /// "moments ago" under two minutes, then the bucket floor: "5m+ ago", "2h+ ago",
    /// "1d+ ago".
    pub fn ago(self) -> String {
        if self.0 < 2 * MIN {
            "moments ago".to_string()
        } else {
            format!("{} ago", self.span())
        }
    }

    /// The same buckets as a span, for "silent 2h+": "under 2m", "5m+", "40m+", "2h+",
    /// "1d+". It is byte-identical across the whole bucket (see the module note).
    pub fn span(self) -> String {
        let floor = BUCKET_FLOORS
            .iter()
            .rev()
            .find(|(floor_ms, _)| self.0 >= *floor_ms)
            .map(|(_, label)| *label);
        match floor {
            None => "under 2m".to_string(),
            Some(label) => format!("{label}+"),
        }
    }
}

const MIN: u64 = 60_000;
const HOUR: u64 = 60 * MIN;
const DAY: u64 = 24 * HOUR;
const WEEK: u64 = 7 * DAY;

/// The bucket floors, ascending, roughly doubling: the label is the floor, so the
/// string is honest ("at least this old") and changes only when an age crosses one.
const BUCKET_FLOORS: [(u64, &str); 17] = [
    (2 * MIN, "2m"),
    (5 * MIN, "5m"),
    (10 * MIN, "10m"),
    (20 * MIN, "20m"),
    (40 * MIN, "40m"),
    (HOUR, "1h"),
    (2 * HOUR, "2h"),
    (4 * HOUR, "4h"),
    (8 * HOUR, "8h"),
    (16 * HOUR, "16h"),
    (DAY, "1d"),
    (2 * DAY, "2d"),
    (4 * DAY, "4d"),
    (WEEK, "1w"),
    (2 * WEEK, "2w"),
    (4 * WEEK, "4w"),
    (8 * WEEK, "8w"),
];

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (the M5's reuse_split probe, 2026-10-10; card 5346e86a): an age
    // that ticks per minute inside a prompt. Every age within a bucket renders the same
    // bytes; the label is the bucket's floor; the sequence is monotone; and the exact
    // strings a reader acts on are what the board and recall now carry.
    #[test]
    fn an_age_renders_the_same_bytes_for_its_whole_bucket_and_never_per_minute() {
        assert_eq!(Age(30_000).span(), "under 2m");
        assert_eq!(Age(2 * MIN).span(), "2m+");
        assert_eq!(Age(2 * MIN + 59_999).span(), "2m+", "a minute later, the same bytes");
        assert_eq!(Age(4 * MIN + 59_999).span(), "2m+");
        assert_eq!(Age(5 * MIN).span(), "5m+");
        assert_eq!(Age(41 * MIN).span(), "40m+", "the board line that forfeited the cache");
        assert_eq!(Age(59 * MIN + 59_999).span(), "40m+");
        assert_eq!(Age(HOUR).span(), "1h+");
        assert_eq!(Age(3 * HOUR).span(), "2h+");
        assert_eq!(Age(23 * HOUR).span(), "16h+");
        assert_eq!(Age(3 * DAY).span(), "2d+");
        assert_eq!(Age(10 * WEEK).span(), "8w+", "the last bucket is open-ended");
        assert_eq!(Age(30_000).ago(), "moments ago");
        assert_eq!(Age(5 * MIN).ago(), "5m+ ago");
        // Monotone: a later age never renders an earlier bucket.
        let mut last = 0;
        for (floor, _) in BUCKET_FLOORS {
            assert!(floor > last, "floors ascend");
            last = floor;
        }
        // Within any bucket, every minute renders identically.
        for w in BUCKET_FLOORS.windows(2) {
            let (lo, label) = w[0];
            let (hi, _) = w[1];
            let mut t = lo;
            while t < hi {
                assert_eq!(Age(t).span(), format!("{label}+"), "at {t} ms");
                t += MIN;
            }
        }
    }
}
