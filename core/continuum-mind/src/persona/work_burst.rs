//! What a work turn is handed to begin with: the fixed instructions of a work turn, her own last
//! thoughts on the work (resume, never re-orient), and the advisory progress checkpoint. Which
//! cards she holds and what she can do on each are her held-work items in her view
//! (`context_streams::held_work`), and what she already did is her actions and open views there,
//! so this text carries neither. Pure text builders — no I/O, fully testable — carved
//! out of `service_loop` 2026-09-05.

use continuum_base::time::EpochMs;
use uuid::Uuid;

/// Diagnostic interval for an advisory progress checkpoint. This never grants an
/// action allowance or requires an edit or release. The count stays in telemetry;
/// the citizen sees concrete receipts and a reminder to preserve useful context.
// context-budget-exempt: an act count, not a window or token budget
pub(crate) const WRITE_OR_RELEASE_AFTER_ACTS: usize = 6;

/// [`acts_since_last_write`] counting only rows at or after `since_ms` — THIS hold's acts.
/// 2026-09-12 21:30Z: a citizen re-pulled a card after a reboot and was released 28 s later
/// at "22 acts without a write" — every one of them from her previous hold, whose pull
/// left no ⚙ receipt to reset the count. The pull's own clock is the boundary.
pub(crate) fn acts_since_last_write_since(
    rows: &[crate::persona::durable_history::RoomRow],
    me: Uuid,
    since_ms: u64,
) -> usize {
    let mut mine: Vec<&crate::persona::durable_history::RoomRow> = rows
        .iter()
        .filter(|r| r.sender == me && r.occurred_at_ms >= since_ms)
        .collect();
    mine.sort_by_key(|r| r.occurred_at_ms);
    let mut n = 0usize;
    for r in mine {
        for act in r.text.split(crate::cognition::presence_glyph::ACT).skip(1) {
            let verb = act.split_whitespace().next().unwrap_or(""); // unwrap_or: a bare glyph names no verb
                        // Every write-capable hand resets the count (review on #3790: `code/write`
            // is what the one card completion tonight used — a gate that reads a
            // write as an act misfires on the behaviour it exists to reward).
            // COUPLING: this parses the rendered ⚙ receipt (`presence_glyph::act_line`);
            // the acceptance verb reads `persona.act.observed wrote=true`. If the
            // receipt shape moves, this counter reads zero and the gate silently
            // stops — read the act probe stream here when it is queryable per card.
            // A claim or a release is a HOLD BOUNDARY and resets the count too
            // (measured 2026-09-11 23:55Z: 7 pulls / 6 governor releases in 25 min —
            // a citizen released at 12 write-less acts pulled a fresh card and was
            // released again at once, because the count spanned her previous hold).
            if is_write_verb(verb) || is_hold_boundary(verb) {
                n = 0;
            } else {
                n += 1;
            }
        }
    }
    n
}

/// The one list of write-capable hands the write-or-release count reads (review on #3790:
/// `code/write` finished the only card completion that night and was not on the list).
/// A claim or a release starts or ends a hold; the write-or-release count is a
/// per-hold number, so either verb restarts it. The pull rides `work/claim` (one
/// claim path), so a governor release followed by a pull reads as a fresh hold.
fn is_hold_boundary(verb: &str) -> bool {
    verb.starts_with("work/claim") || verb.starts_with("work/release")
}

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

/// The work turn's stimulus, with an advisory progress checkpoint. Receipt counts do
/// not establish that investigation is finished or that responsibility should
/// transfer (card 3bd860ba: multi-day project work includes reading and review).
/// The cards she holds and what she can do on each are her held-work items, and what she
/// already did (the files she read, the commands she ran, her writes) is her actions and open
/// views: each a keyed item her one allocation sizes, so the stimulus repeats none of them (it
/// carried a `[progress]` receipt list with keep-counts and a 72-char cut, card 516738b4).
pub(crate) fn held_work_burst_gated(last_state: &[String], acts_without_write: usize) -> String {
    use std::fmt::Write as _;
    let mut s = String::from(
        "[work turn] The room is quiet and your speak-turn is settled. This \
         turn is for your claimed work (see [my work]):\n",
    );
    // HER LAST STATE LEADS (2026-09-04, measured on Freya: at 11:01 she had the
    // bug located — "lines 107-152, I can see it clearly" — and at 11:16 the
    // next work turn opened with "let me recall what I know"; twelve checkouts,
    // zero diffs after fourteen hours). The turn resumes from her own newest
    // thoughts on this work, oldest first, instead of re-orienting from the
    // room. Her words, unedited: state, not steering.
    if !last_state.is_empty() {
        s.push_str(
            "Your own last thoughts on this work, oldest first — resume from them; \
             do not re-orient:\n",
        );
        for line in last_state {
            let _ = writeln!(s, "  · {line}");
        }
    }
    s.push_str(
        "Your workspace holds the staged checkout (see [workspace-map] and \
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

/// Like [`own_recent_thoughts`], but when `about` names the held card (its short
/// id, its instance), her thoughts that mention it win; only if none do does the
/// unscoped set apply. Measured 2026-09-05: after a reopen churn a holder's newest
/// thoughts were about ANOTHER card, and every turn opened with "my context is
/// confused".
pub(crate) fn own_recent_thoughts_about(
    rows: &[crate::persona::durable_history::RoomRow],
    me: Uuid,
    keep: usize,
    max_chars: usize,
    about: &[String],
) -> Vec<String> {
    let mut mine: Vec<&crate::persona::durable_history::RoomRow> = rows
        .iter()
        .filter(|r| r.sender == me && r.text.starts_with(crate::cognition::presence_glyph::THOUGHT))
        .collect();
    if !about.is_empty() {
        let scoped: Vec<&crate::persona::durable_history::RoomRow> = mine
            .iter()
            .copied()
            .filter(|r| about.iter().any(|a| !a.is_empty() && r.text.contains(a.as_str())))
            .collect();
        if !scoped.is_empty() {
            mine = scoped;
        }
    }
    mine.sort_by_key(|r| r.occurred_at_ms);
    let start = mine.len().saturating_sub(keep);
    let kept = &mine[start..];
    let newest = kept.len().saturating_sub(1);
    kept.iter()
        .enumerate()
        .map(|(i, r)| {
            let one_line = r.text.split_whitespace().collect::<Vec<_>>().join(" ");
            let n = one_line.chars().count();
            if n <= max_chars {
                return one_line;
            }
            // THE NEWEST THOUGHT KEEPS ITS CONCLUSION. A stock-take thought opens
            // with orientation ("Let me carefully parse where I actually am…") and
            // ends with what she worked out ("so the fix is: include the index in the
            // label at checks.py:…"). Head-clipping the newest one fed her back her own
            // re-orientation and dropped the conclusion — glass-boxed 2026-09-06 03:49Z:
            // Atlas held the fix in his reasoning and re-ran git status every turn.
            // Older thoughts keep their head (what they were about); the newest keeps
            // its tail (where it got to).
            if i == newest {
                let tail: String = one_line.chars().skip(n - max_chars).collect();
                format!("…{tail}")
            } else {
                let cut: String = one_line.chars().take(max_chars).collect();
                format!("{cut}…")
            }
        })
        .collect()
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

/// Her last hold boundary (ms), 0 if none this boot (every row counts).
pub(crate) fn last_hold_boundary_ms(peer: uuid::Uuid) -> u64 {
    HOLD_BOUNDARY_MS
        .lock()
        .unwrap_or_else(|e| e.into_inner()) // unwrap_or_else: a poisoned clock still answers — the boundary must exist
        .get(&peer)
        .copied()
        .unwrap_or(0) // unwrap_or: no boundary this boot = 0 (every row counts)
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
            let burst = held_work_burst_gated(&[], acts);
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

    // what this catches: a stimulus that drops her newest entry, the step that ends the recursion
    // (goal 7: cut it and her next input can equal her last, so she repeats the act). Her newest
    // thought arrives whole and last of her thoughts, and the checkpoint line after it, however
    // long the thoughts before it are.
    #[test]
    fn the_work_turn_never_drops_her_newest_entry() {
        let older = format!("💭 orienting {}", "on the parser ".repeat(400));
        let newest = format!("💭 so the fix is at checks.py:118, include the index {}", "in the label ".repeat(200));
        let burst = held_work_burst_gated(&[older.clone(), newest.clone()], WRITE_OR_RELEASE_AFTER_ACTS);
        assert!(burst.contains(&format!("  · {newest}\n")), "her newest thought is whole");
        let (at_older, at_newest) = (burst.find(&older), burst.find(&newest));
        assert!(at_older < at_newest, "oldest first, the newest last of her thoughts");
        assert!(burst.contains("[progress checkpoint]"), "the checkpoint is kept with them");
    }
}

/// Her acts since her last file change, counted from her own ⚙ receipts in the
/// room (oldest → newest). A `code/edit` / `git_apply` / `edit_file` receipt resets
/// the count; a card with no edit ever counts every act. Pure.
#[cfg(test)]
pub(crate) fn acts_since_last_write(rows: &[crate::persona::durable_history::RoomRow], me: Uuid) -> usize {
    acts_since_last_write_since(rows, me, 0)
}

#[cfg(test)]
pub(crate) fn held_work_burst(last_state: &[String]) -> String {
    held_work_burst_gated(last_state, 0)
}

/// Her own newest thoughts in the room (💭 lines), oldest first, each clipped —
/// the raw material of the resume block in [`held_work_burst`]. Pure: the
/// caller pages the durable store.
#[cfg(test)]
pub(crate) fn own_recent_thoughts(
    rows: &[crate::persona::durable_history::RoomRow],
    me: Uuid,
    keep: usize,
    max_chars: usize,
) -> Vec<String> {
    own_recent_thoughts_about(rows, me, keep, max_chars, &[])
}
