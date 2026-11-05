//! What a work turn is handed to begin with: its fixed instructions and the advisory progress
//! checkpoint. Everything else she needs is in her view, each a keyed item her one allocation
//! sizes: which cards she holds and what she can do on each (`context_streams::held_work`), what
//! she already did (her actions and open views), and her own thoughts (`context_streams::thoughts`,
//! read from her thought log, every thought across turns and restarts, her newest whole). So the
//! stimulus repeats none of them: it once carried her cards, a `[progress]` receipt list and her
//! last thoughts paged off the room store, each with its own count or cut. Pure text builders,
//! carved out of `service_loop` 2026-09-05.

/// Diagnostic interval for an advisory progress checkpoint. This never grants an
/// action allowance or requires an edit or release. The count stays in telemetry;
/// the citizen sees concrete receipts and a reminder to preserve useful context.
// context-budget-exempt: an act count, not a window or token budget
pub(crate) const WRITE_OR_RELEASE_AFTER_ACTS: usize = 6;

/// The work turn's stimulus, with an advisory progress checkpoint. Receipt counts do not
/// establish that investigation is finished or that responsibility should transfer (card
/// 3bd860ba: multi-day project work includes reading and review). Credit and gene selection read
/// this same text.
/// `acts_without_write` is her write-less stretch on this hold (`WorkingMemory::acts_since_change`),
/// `None` when the hold's start is unknown: then no checkpoint, never a count of zero.
pub(crate) fn held_work_burst_gated(acts_without_write: Option<usize>) -> String {
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
    if acts_without_write.is_some_and(|acts| acts >= WRITE_OR_RELEASE_AFTER_ACTS) {
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

#[cfg(test)]
mod tests {
    use super::*;
    // what this catches: card 3bd860ba's prompt coercing release or unnecessary
    // edits when legitimate project investigation reaches the gate or twice it (the
    // old governor threshold, which no longer releases anything).
    #[test]
    fn progress_checkpoint_preserves_investigation_and_explicit_handoff() {
        for acts in [WRITE_OR_RELEASE_AFTER_ACTS, 2 * WRITE_OR_RELEASE_AFTER_ACTS] {
            let burst = held_work_burst_gated(Some(acts));
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
