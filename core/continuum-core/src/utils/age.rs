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

/// Coarse human age — a memory's rough distance in time, not a timestamp. "moments
/// ago" under two minutes, then the bucket floor: "5m+ ago", "2h+ ago", "1d+ ago".
pub fn humanize_age(delta_ms: u64) -> String {
    if delta_ms < 2 * MIN {
        "moments ago".to_string()
    } else {
        format!("{} ago", humanize_span(delta_ms))
    }
}

/// The same buckets as a span — "under 2m", "5m+", "40m+", "2h+", "1d+" — for
/// "silent 2h+". Byte-identical across the whole bucket (see the module note).
pub fn humanize_span(delta_ms: u64) -> String {
    let floor = BUCKET_FLOORS
        .iter()
        .rev()
        .find(|(floor_ms, _)| delta_ms >= *floor_ms)
        .map(|(_, label)| *label);
    match floor {
        None => "under 2m".to_string(),
        Some(label) => format!("{label}+"),
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
        assert_eq!(humanize_span(30_000), "under 2m");
        assert_eq!(humanize_span(2 * MIN), "2m+");
        assert_eq!(humanize_span(2 * MIN + 59_999), "2m+", "a minute later, the same bytes");
        assert_eq!(humanize_span(4 * MIN + 59_999), "2m+");
        assert_eq!(humanize_span(5 * MIN), "5m+");
        assert_eq!(humanize_span(41 * MIN), "40m+", "the board line that forfeited the cache");
        assert_eq!(humanize_span(59 * MIN + 59_999), "40m+");
        assert_eq!(humanize_span(HOUR), "1h+");
        assert_eq!(humanize_span(3 * HOUR), "2h+");
        assert_eq!(humanize_span(23 * HOUR), "16h+");
        assert_eq!(humanize_span(3 * DAY), "2d+");
        assert_eq!(humanize_span(10 * WEEK), "8w+", "the last bucket is open-ended");
        assert_eq!(humanize_age(30_000), "moments ago");
        assert_eq!(humanize_age(5 * MIN), "5m+ ago");
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
                assert_eq!(humanize_span(t), format!("{label}+"), "at {t} ms");
                t += MIN;
            }
        }
    }
}
