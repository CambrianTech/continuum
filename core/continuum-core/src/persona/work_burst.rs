//! What a work turn is handed to begin with: the held-card burst, her own last
//! thoughts on the work (resume, never re-orient), and the board anchor the
//! self-cycle speaks from. Pure text builders — no I/O, fully testable — carved
//! out of `service_loop` 2026-09-05.

use uuid::Uuid;

/// Acts on a held card without a file change before the work turn stops narrating
/// and GATES: edit now, or release with a reason. Six is two turns of "let me read
/// one more thing" — the shape every glass box found tonight (Atlas: 850+ acts, no
/// deliverable; the substrate said "no act of mine has changed a file" and nothing
/// followed from it).
// context-budget-exempt: an act count, not a window or token budget
pub(crate) const WRITE_OR_RELEASE_AFTER_ACTS: usize = 6;
/// The substrate's own exit from "write or release": at twice the gate, the card
/// is released FOR her, with a receipt. Measured 2026-09-07 11:30–12:20Z on
/// 66ab948fc: the gate fired eight times across five holders, 78 acts, 0 writes,
/// 0 releases — the sentence was read and not acted on. A governor acts.
pub(crate) const GOVERNOR_RELEASE_AFTER_ACTS: usize = 2 * WRITE_OR_RELEASE_AFTER_ACTS;

/// Whether the substrate releases the card this turn. Pure.
pub(crate) fn governor_releases(acts_without_write: usize) -> bool {
    acts_without_write >= GOVERNOR_RELEASE_AFTER_ACTS
}

/// Her acts since her last file change, counted from her own ⚙ receipts in the
/// room (oldest → newest). A `code/edit` / `git_apply` / `edit_file` receipt resets
/// the count; a card with no edit ever counts every act. Pure.
pub(crate) fn acts_since_last_write(rows: &[crate::persona::durable_history::RoomRow], me: Uuid) -> usize {
    let mut mine: Vec<&crate::persona::durable_history::RoomRow> =
        rows.iter().filter(|r| r.sender == me).collect();
    mine.sort_by_key(|r| r.occurred_at_ms);
    let mut n = 0usize;
    for r in mine {
        for act in r.text.split(crate::persona::presence_glyph::ACT).skip(1) {
            let verb = act.split_whitespace().next().unwrap_or(""); // unwrap_or: a bare glyph names no verb
                        // Every write-capable hand resets the count (review on #3790: `code/write`
            // is what the one card completion tonight used — a gate that reads a
            // write as an act misfires on the behaviour it exists to reward).
            // COUPLING: this parses the rendered ⚙ receipt (`presence_glyph::act_line`);
            // the acceptance verb reads `persona.act.observed wrote=true`. If the
            // receipt shape moves, this counter reads zero and the gate silently
            // stops — read the act probe stream here when it is queryable per card.
            if is_write_verb(verb) {
                n = 0;
            } else {
                n += 1;
            }
        }
    }
    n
}

pub(crate) fn held_work_burst(held: &[&airc_lib::WorkCard], last_state: &[String]) -> String {
    held_work_burst_gated(held, last_state, 0, &CardProgress::default())
}

/// What she has already done on this card, derived from her own ⚙ receipts in
/// the room page (card 516738b4): the files she read, the commands she ran, the
/// act/write tally. No new store — the transcript already holds every act; this
/// is the projection the next turn resumes from so it stops re-reading what it
/// read. Measured 2026-09-06 08:0xZ: every work turn opened with "let me parse
/// where I am", re-read the same file region, and ended; 48 acts / 0 writes in a
/// warm 80-minute window across ten holders.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct CardProgress {
    pub acts: usize,
    pub writes: usize,
    /// Distinct objects of her read-shaped acts (code/read, code/list) — paths —
    /// newest last, capped.
    pub read: Vec<String>,
    /// Distinct code/search terms, newest last, capped (review on #3793: a search
    /// term is not something she "already read").
    pub searched: Vec<String>,
    /// Her newest run/shell commands with their outcome glyph, newest last, capped.
    pub ran: Vec<String>,
}

const PROGRESS_READ_KEEP: usize = 6;
const PROGRESS_RAN_KEEP: usize = 3;

/// Fold her ⚙ receipts (oldest first) into a [`CardProgress`]. A receipt row is
/// `⚙ verb object ✓` segments joined by the act glyph; the object is whatever the
/// writer put there (a path, a command, a card id), clipped for the block.
pub(crate) fn card_progress(rows: &[crate::persona::durable_history::RoomRow], me: Uuid) -> CardProgress {
    use crate::persona::presence_glyph::{ACT, FAIL, OK};
    let mut mine: Vec<&crate::persona::durable_history::RoomRow> =
        rows.iter().filter(|r| r.sender == me && r.text.contains(ACT)).collect();
    mine.sort_by_key(|r| r.occurred_at_ms);
    let mut p = CardProgress::default();
    for row in mine {
        for seg in row.text.split(ACT).skip(1) {
            let seg = seg.trim();
            if seg.is_empty() {
                continue;
            }
            let ok = seg.ends_with(OK);
            let seg = seg.trim_end_matches(OK).trim_end_matches(FAIL).trim();
            let (verb, object) = match seg.split_once(' ') {
                Some((v, o)) => (v, o.trim()),
                None => (seg, ""),
            };
            p.acts += 1;
            if is_write_verb(verb) {
                p.writes += 1;
            }
            let obj: String = object.chars().take(72).collect();
            if verb.starts_with("code/read") || verb.starts_with("code/list") {
                push_distinct(&mut p.read, obj);
            } else if verb.starts_with("code/search") {
                push_distinct(&mut p.searched, obj);
            } else if verb.starts_with("code/run") || verb.starts_with("code/shell") {
                let shown = if obj.is_empty() { verb.to_string() } else { obj };
                p.ran.push(format!("{shown} {}", if ok { OK } else { FAIL }));
                if p.ran.len() > PROGRESS_RAN_KEEP {
                    p.ran.remove(0);
                }
            }
        }
    }
    p
}

/// Keep `v` a distinct, newest-last list capped at [`PROGRESS_READ_KEEP`].
fn push_distinct(v: &mut Vec<String>, obj: String) {
    if obj.is_empty() {
        return;
    }
    v.retain(|o| o != &obj);
    v.push(obj);
    if v.len() > PROGRESS_READ_KEEP {
        v.remove(0);
    }
}

/// One `[progress]` line for the head of the held-work block; empty when she has
/// no acts yet (a fresh card carries no note).
pub(crate) fn progress_line(p: &CardProgress) -> String {
    if p.acts == 0 {
        return String::new();
    }
    let mut s = format!("[progress] {} acts so far ({} writes).", p.acts, p.writes);
    if !p.read.is_empty() {
        s.push_str(" Already read: ");
        s.push_str(&p.read.join(", "));
        s.push('.');
    }
    if !p.searched.is_empty() {
        s.push_str(" Already searched: ");
        s.push_str(&p.searched.join(", "));
        s.push('.');
    }
    if !p.ran.is_empty() {
        s.push_str(" Last ran: ");
        s.push_str(&p.ran.join("; "));
        s.push('.');
    }
    s.push_str(" Do not re-read those; go on from your last thought.");
    s
}

/// The one list of write-capable hands, shared by the write-or-release count and
/// the progress note (review on #3790: `code/write` finished the only card
/// completion that night and was not on the list).
fn is_write_verb(verb: &str) -> bool {
    verb.starts_with("code/edit")
        || verb.starts_with("code/write")
        || verb.starts_with("code/create-workspace")
        || verb.starts_with("git_apply")
        || verb.starts_with("edit_file")
        || verb.starts_with("code/git/apply")
}

/// [`held_work_burst`] with the write-or-release gate: past
/// [`WRITE_OR_RELEASE_AFTER_ACTS`] acts without a file change, the turn is told the
/// investigation is finished and given exactly two ways out.
pub(crate) fn held_work_burst_gated(
    held: &[&airc_lib::WorkCard],
    last_state: &[String],
    acts_without_write: usize,
    progress: &CardProgress,
) -> String {
    use std::fmt::Write as _;
    let mut s = String::from(
        "[work turn] The room is quiet and your speak-turn is settled. This \
         turn is for your claimed work:\n",
    );
    for card in held {
        let id8: String = card.card_id.as_uuid().to_string().chars().take(8).collect();
        let _ = writeln!(s, "- card {id8} \"{}\"", card.title);
    }
    // HER LAST STATE LEADS (2026-09-04, measured on Freya: at 11:01 she had the
    // bug located — "lines 107-152, I can see it clearly" — and at 11:16 the
    // next work turn opened with "let me recall what I know"; twelve checkouts,
    // zero diffs after fourteen hours). The turn resumes from her own newest
    // thoughts on this work, oldest first, instead of re-orienting from the
    // room. Her words, unedited: state, not steering.
    // WHAT SHE ALREADY DID LEADS HER THOUGHTS (card 516738b4): the files read and
    // commands run are facts the transcript holds; without them every turn re-read
    // the same region before thinking.
    let note = progress_line(progress);
    if !note.is_empty() {
        let _ = writeln!(s, "{note}");
    }
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
         [active-work]). Continue the work with your tools — read, run, edit, \
         test. When this card is finished, or you can go no further, conclude by \
         passing with a reason on ONE line: 'PASS: done' (the work is complete \
         and in the workspace), 'PASS: blocked — <one line why>', or \
         'PASS: nothing' (nothing to contribute). 'PASS: done' concludes the \
         card, so use it only when the deliverable is really written. Speak only \
         to report a result or blocker to the room.",
    );
    if acts_without_write >= WRITE_OR_RELEASE_AFTER_ACTS {
        let _ = write!(
            s,
            "\n[write or release] You have made {acts_without_write} acts on this card \
             without changing a file. The investigation is finished. This turn does ONE \
             of two things: make the edit now (code/edit or git_apply — the fix you have \
             already named in your last thoughts), or conclude 'PASS: blocked — <one \
             line why>' and release the card so a peer can take it. No more reading, \
             running, or status checks before one of those."
        );
    }
    s
}

/// Her own newest thoughts in the room (💭 lines), oldest first, each clipped —
/// the raw material of the resume block in [`held_work_burst`]. Pure: the
/// caller pages the durable store.
pub(crate) fn own_recent_thoughts(
    rows: &[crate::persona::durable_history::RoomRow],
    me: Uuid,
    keep: usize,
    max_chars: usize,
) -> Vec<String> {
    own_recent_thoughts_about(rows, me, keep, max_chars, &[])
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
        .filter(|r| r.sender == me && r.text.starts_with(crate::persona::presence_glyph::THOUGHT))
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

pub(crate) fn work_board_anchor(deliveries: &[crate::persona::rag_budget::RagDelivery]) -> String {
    // Did the board source SPEAK this turn? "The board is empty" and "I never read the
    // board" are different facts about the world, and only one of them is knowable from an
    // absent delivery. Glass-boxed 2026-08-06 from Benchy's live capture: `room-kanban`
    // delivered NOTHING (grounding is last in the budget queue), the anchor rendered that
    // as "No open cards are visible", and she then said exactly that in-room for six turns
    // — while `work/list()` in her OWN working memory listed a full board in the same
    // prompt. She trusted the authoritative-sounding anchor over her own receipt.
    //
    // Never assert a fact about the world on behalf of a source that did not speak.
    // [[grounding-is-last-in-the-budget-queue-so-she-goes-blind-one-turn-in-ten]]
    let board_spoke = deliveries.iter().any(|d| d.source_id == "room-kanban");
    if !board_spoke {
        // Say nothing rather than something false. A silent anchor leaves her own
        // `work/list` receipt as the only board claim in the prompt — which is the truthful
        // one. An anchor that invents emptiness actively overrides it.
        return String::new();
    }
    let cards: Vec<&crate::persona::rag_budget::RagItem> = deliveries
        .iter()
        .filter(|d| d.source_id == "room-kanban")
        .flat_map(|d| d.items.iter())
        .filter(|i| i.metadata.get("card_id").is_some())
        .collect();
    /// The card's state as the TYPE, never as a string to be spelled correctly.
    ///
    /// `None` for an item whose metadata carries no parseable state — which is a real
    /// possibility (a future variant this build doesn't know) and must read as "unknown",
    /// never as a silent mismatch against a hardcoded spelling.
    fn state(i: &crate::persona::rag_budget::RagItem) -> Option<airc_work::CardState> {
        i.metadata
            .get("state")
            .and_then(|s| serde_json::from_value(s.clone()).ok())
    }
    /// Is this card's hold still good? Read as the structural fact the board source
    /// carries, never re-derived here — `claim_is_live` is the ONE definition and
    /// `room_board_source` already applied it. Absent (an older projection) reads as
    /// LIVE, so a missing field can never invent availability that isn't there.
    fn claim_live(i: &crate::persona::rag_budget::RagItem) -> bool {
        i.metadata
            .get("claim_live")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)  // unwrap_or: no card named = keep every thought
    }
    // AVAILABLE work is not just `Open` — it is anything nobody currently holds. A card
    // stuck in `Claimed` with a LAPSED lease is free to take, and treating it as taken is
    // what emptied this anchor while 19 takeable cards sat on the board (2026-08-06: every
    // resident read "nothing available" off their own expired claims and passed, for hours).
    // `state == Open` and "unheld" are different questions; ask the second one.
    use airc_work::CardState;
    let unclaimed: Vec<&str> = cards
        .iter()
        .filter(|i| {
            let unowned_open = state(i) == Some(CardState::Open)
                && i.metadata.get("owner").is_none_or(|o| o.is_null());
            // A lapsed hold on ANY non-terminal card is available work, whoever held it.
            let lapsed = !claim_live(i)
                && matches!(
                    state(i),
                    Some(CardState::Claimed | CardState::InProgress | CardState::Review)
                );
            unowned_open || lapsed
        })
        .map(|i| i.content.trim())
        .take(2)
        .collect();
    // Exhaustive over the enum, so ADDING a variant to `CardState` forces a decision here
    // instead of silently falling through as "not in flight". That is the whole point of
    // matching the type rather than a string.
    let in_flight: Vec<&str> = cards
        .iter()
        // Genuinely in flight = claimed AND the hold is still live. Without the liveness
        // term a lapsed card counts as both available and in-flight, and the anchor would
        // tell her the same card is free and busy in one breath.
        .filter(|i| claim_live(i))
        .filter(|i| match state(i) {
            Some(CardState::Claimed | CardState::InProgress | CardState::Review) => true,
            Some(CardState::Open | CardState::Blocked | CardState::Merged | CardState::Closed) => {
                false
            }
            None => false,
        })
        .map(|i| i.content.trim())
        .take(1)
        .collect();
    if unclaimed.is_empty() && in_flight.is_empty() {
        // Honest empty: no cards visible (empty board, unreadable board, or a
        // context whose board source abstained). Never invent work.
        "[anchor] No open cards are visible on this room's board right now — \
         proposing one (work/create) would add something new; restating prior \
         messages adds nothing."
            .to_string()
    } else {
        let facts: Vec<&str> = unclaimed.into_iter().chain(in_flight).collect();
        format!(
            "[anchor] Open work exists on this room's board right now: {}. \
             Restating prior messages adds nothing; acting on a card would.",
            facts.join("; ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // what this catches: the governor firing under the gate (a nag becoming a release
    // at six acts) or never (the sentence read and ignored forever). It releases at
    // exactly twice the gate.
    #[test]
    fn the_governor_releases_at_twice_the_gate_and_not_before() {
        assert!(!governor_releases(WRITE_OR_RELEASE_AFTER_ACTS));
        assert!(!governor_releases(GOVERNOR_RELEASE_AFTER_ACTS - 1));
        assert!(governor_releases(GOVERNOR_RELEASE_AFTER_ACTS));
        assert!(governor_releases(GOVERNOR_RELEASE_AFTER_ACTS + 30));
    }
}
