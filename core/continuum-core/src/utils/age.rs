//! How old something is, in the words a reader can act on.
//!
//! ONE formatter for every surface that tells a citizen an age (recall, the work board).
//! An age, never a timestamp: a model reads a date against a training-era sense of
//! "now" and misjudges it by years (Joel, 2026-10-06); "23h ago" needs no clock.

/// Coarse human age buckets — a memory's rough distance in time, not a
/// timestamp. Coarseness is deliberate: "2h ago" orients; "7,243,118ms"
/// is noise the model would parrot.
pub fn humanize_age(delta_ms: u64) -> String {
    if delta_ms < 2 * MIN {
        "moments ago".to_string()
    } else {
        format!("{} ago", humanize_span(delta_ms))
    }
}

/// The same buckets as a span — "41m", "23h", "3d", "under 2m" — for "silent 23h".
pub fn humanize_span(delta_ms: u64) -> String {
    match delta_ms {
        d if d < 2 * MIN => "under 2m".to_string(),
        d if d < 2 * HOUR => format!("{}m", d / MIN),
        d if d < 2 * DAY => format!("{}h", d / HOUR),
        d => format!("{}d", d / DAY),
    }
}

const MIN: u64 = 60_000;
const HOUR: u64 = 60 * MIN;
const DAY: u64 = 24 * HOUR;
