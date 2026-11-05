//! What a work turn is handed to begin with: its fixed instructions and the advisory progress
//! checkpoint. Everything else she needs is in her view, each a keyed item her one allocation
//! sizes: which cards she holds and what she can do on each (`context_streams::held_work`), what
//! she already did (her actions and open views), and her own thoughts (`context_streams::thoughts`,
//! read from her thought log, every thought across turns and restarts, her newest whole). So the
//! stimulus repeats none of them: it once carried her cards, a `[progress]` receipt list and her
//! last thoughts paged off the room store, each with its own count or cut. Pure text builders,
//! carved out of `service_loop` 2026-09-05.

use continuum_base::time::EpochMs;

/// Diagnostic interval for an advisory progress checkpoint. This never grants an
/// action allowance or requires an edit or release. The count stays in telemetry;
/// the citizen sees concrete receipts and a reminder to preserve useful context.
// context-budget-exempt: an act count, not a window or token budget
pub(crate) const WRITE_OR_RELEASE_AFTER_ACTS: usize = 6;

/// Her acts since her last write, over the commands she called since her hold boundary, oldest
/// first (her action log, `WorkingMemory::operations_since`): THIS hold's acts. 2026-09-12 21:30Z:
/// a citizen re-pulled a card after a reboot and was released 28 s later at "22 acts without a
/// write", every one of them from her previous hold. Every write-capable hand resets the count
/// (review on #3790: `code/write` is what the one card completion that night used), and so does a
/// claim or a release, a HOLD BOUNDARY (measured 2026-09-11 23:55Z: a count that spanned her
/// previous hold released a fresh pull at once). Pure.
pub(crate) fn acts_since_last_write<'a>(operations: impl IntoIterator<Item = &'a str>) -> usize {
    operations.into_iter().fold(0, |n, verb| if is_write_verb(verb) || is_hold_boundary(verb) { 0 } else { n + 1 })
}

/// A claim or a release starts or ends a hold; the write-or-release count is a per-hold number,
/// so either verb restarts it. The pull rides `work/claim` (one claim path), so a governor
/// release followed by a pull reads as a fresh hold.
fn is_hold_boundary(verb: &str) -> bool {
    verb.starts_with("work/claim") || verb.starts_with("work/release")
}

/// The one list of write-capable hands the write-or-release count reads.
fn is_write_verb(verb: &str) -> bool {
    // The ledger is the mind's deliverable: writing it is progress, not orientation.
    verb.starts_with("work/note")
        || verb.starts_with("code/edit")
        || verb.starts_with("code/write")
        || verb.starts_with("code/create-workspace")
        || verb.starts_with("git_apply")
        || verb.starts_with("edit_file")
        || verb.starts_with("code/git/apply")
}

/// The work turn's stimulus, with an advisory progress checkpoint. Receipt counts do not
/// establish that investigation is finished or that responsibility should transfer (card
/// 3bd860ba: multi-day project work includes reading and review). Credit and gene selection read
/// this same text.
pub(crate) fn held_work_burst_gated(acts_without_write: usize) -> String {
    let mut s = String::from(
        "[work turn] The room is quiet and your speak-turn is settled. This turn is for your \
         claimed work (see [my work]); resume from your own thoughts on it, do not re-orient.\n\
         Your workspace holds the staged checkout (see [workspace-map] and \
         [my work]). Continue the work with your tools — read, run, edit, \
         test. When this card is finished, or you can go no further, conclude by \
         passing with a reason on ONE line: 'PASS: done' (the work is complete \
         and in the workspace), 'PASS: blocked — <one line why>', or \
         'PASS: nothing' (nothing to contribute). 'PASS: done' concludes the \
         card, so use it only when the deliverable is complete and verified. You may \
         ask collaborators for help, discuss a finding, or report a result or blocker \
         to the room. Being blocked is not a request to hand off responsibility.",
    );
    if acts_without_write >= WRITE_OR_RELEASE_AFTER_ACTS {
        s.push_str(
            "\n[progress checkpoint] Reading, planning, verification and review can be \
             useful work. When useful for resuming or collaborating, use work/note to record what changed in your \
             understanding, the next step, or a specific blocker and who can help. \
             Continue the appropriate investigation or action. If you choose a handoff, identify the remaining \
             work and any uncommitted changes explicitly."
        );
    }
    s
}

/// When each citizen last crossed a HOLD BOUNDARY (ms): a claim or a release, by her own
/// verb. The write-or-release count reads acts from here. A card claimed inside a turn
/// (Mathis, 2026-09-13 13:50Z) used to keep the window at an older boundary, and forty
/// old acts released a four-minute-old claim.
static HOLD_BOUNDARY_MS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<uuid::Uuid, u64>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Stamp a hold boundary for `peer`: she claimed or released a card.
pub fn note_hold_boundary(peer: uuid::Uuid) {
    HOLD_BOUNDARY_MS
        .lock()
        .unwrap_or_else(|e| e.into_inner()) // poisoned lock = read the last state, same policy as every lock in this crate
        .insert(peer, EpochMs::now().millis());
}

/// Her last hold boundary (ms), 0 if none this boot (every act counts).
pub(crate) fn last_hold_boundary_ms(peer: uuid::Uuid) -> u64 {
    HOLD_BOUNDARY_MS
        .lock()
        .unwrap_or_else(|e| e.into_inner()) // unwrap_or_else: a poisoned clock still answers — the boundary must exist
        .get(&peer)
        .copied()
        .unwrap_or(0) // unwrap_or: no boundary this boot = 0 (every act counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    // what this catches: card 3bd860ba's prompt coercing release or unnecessary
    // edits when legitimate project investigation reaches the gate or twice it (the
    // old governor threshold, which no longer releases anything).
    #[test]
    fn progress_checkpoint_preserves_investigation_and_explicit_handoff() {
        for acts in [WRITE_OR_RELEASE_AFTER_ACTS, 2 * WRITE_OR_RELEASE_AFTER_ACTS] {
            let burst = held_work_burst_gated(acts);
            assert!(burst.contains("[progress checkpoint]"));
            assert!(burst.contains("specific blocker and who can help"));
            assert!(burst.contains("uncommitted changes explicitly"));
            assert!(!burst.contains("release the card"));
            assert!(!burst.contains("No more reading"));
            assert!(!burst.contains("substrate releases"));
            assert!(!burst.contains("acts since"));
            assert!(!burst.contains("counter"));
        }
    }
}
