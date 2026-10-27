//! Interpret the REASON a mind gave for passing a turn.
//!
//! A pass is an accountable decision, not anonymous silence
//! ([[a-citizen-saying-i-have-nothing-to-contribute-is-a-substrate-gap-report]]).
//! `Decision::Pass` carries the mind's own words (captured at the one text→decision
//! seam, `deliberation_parse::decision_from_response`); this module maps those words
//! to a small, deterministic INTENT the substrate can act on at a turn boundary.
//!
//! It is recipe-general on purpose: the held-work settle edge uses it to conclude a
//! card the moment she passes it "done", but the card may belong to ANY recipe — a
//! benchmark solve, a code task, a Slack/AWS/finance pipeline. The classifier reads
//! intent; it never steers ([[no-hardcoded-heuristics-to-steer-cognition]]) — the
//! mind chose the pass and chose the reason, and the conservative default (`Unclear`
//! concludes nothing) means an illegible reason never forces a completion.

/// What a pass MEANS, as far as the substrate can act on it deterministically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassIntent {
    /// She reports the work complete — a held card may conclude (its owning
    /// recipe then scores the outcome; an empty deliverable grades an honest
    /// fail, never a substrate second-guess).
    Done,
    /// She is blocked — the card stays hers and the blocker is a gap signal.
    Blocked,
    /// She has nothing to contribute — the card stays; a substrate-gap signal.
    Nothing,
    /// No stated reason, or intent not legible — honor the pass, conclude
    /// nothing. The safe default: an ambiguous pass never concludes a card.
    Unclear,
}

/// Map a pass reason to its [`PassIntent`]. Deterministic and pure.
///
/// The burst asks for a one-word reason (done / blocked / nothing), so the FIRST
/// word decides; a `blocked: <why>` prefix is honored. Blocked takes precedence
/// over done — a mind that says "done investigating but blocked on the fixture"
/// is blocked, and a blocked card must never conclude. Anything not clearly one
/// of the three is `Unclear`.
pub fn classify_pass(reason: Option<&str>) -> PassIntent {
    let Some(raw) = reason else {
        return PassIntent::Unclear; // a bare PASS token carries no reason
    };
    let lower = raw.to_lowercase();
    // Blocked FIRST — never conclude a card she says she is stuck on, even if the
    // same sentence mentions progress.
    const BLOCKED: &[&str] = &[
        "block", "stuck", "can't", "cannot", "unable", "need help", "waiting on",
    ];
    if BLOCKED.iter().any(|k| lower.contains(k)) {
        return PassIntent::Blocked;
    }
    const NOTHING: &[&str] = &[
        "nothing to", "nothing else", "nothing further", "no more", "no changes",
        "nothing new",
    ];
    if NOTHING.iter().any(|k| lower.contains(k)) {
        return PassIntent::Nothing;
    }
    // Done is the one that CONCLUDES a card, so it is deliberately specific — a
    // clear completion word, not a vague positive.
    const DONE: &[&str] = &[
        "done", "complete", "finished", "ready", "fixed", "resolved", "all set",
    ];
    // Prefer the first token (the burst asks for a one-word reason), then fall
    // back to a contains-scan for a narrated completion ("the patch is ready").
    let first = lower
        .split(|c: char| !c.is_alphanumeric())
        .find(|w| !w.is_empty())
        .unwrap_or("");
    if DONE.contains(&first) || DONE.iter().any(|k| lower.contains(k)) {
        return PassIntent::Done;
    }
    PassIntent::Unclear
}

/// What the held-work settle edge does with a reasoned pass — the DECISION,
/// separated from the side-effecting execution (the card transition, the probes)
/// so it is exhaustively unit-testable. Recipe-general: `Close` names a card to
/// conclude; the recipe that owns it reacts to the transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConcludeAction {
    /// Exactly one held card + a `done` intent → conclude that card.
    Close(uuid::Uuid),
    /// `done` but several held cards → which is done is ambiguous; conclude none.
    Ambiguous,
    /// She is blocked → keep the card(s); a candidate substrate-gap report.
    Blocked,
    /// Nothing to contribute → keep; a substrate-gap signal.
    Nothing,
    /// No legible completion intent (or no held cards) → honor the pass, conclude
    /// nothing.
    Hold,
}

/// Decide what a reasoned pass concludes, given the cards she holds and the
/// intent parsed from her reason. Pure — the edge executes the returned action.
///
/// Only an UNAMBIGUOUS `done` (exactly one held card) concludes a card: the
/// substrate never guesses WHICH of several cards she finished, and never
/// concludes on a blocked/nothing/illegible pass.
pub fn conclude_from_pass(held: &[uuid::Uuid], intent: PassIntent) -> ConcludeAction {
    match intent {
        PassIntent::Done => match held {
            [one] => ConcludeAction::Close(*one),
            [] => ConcludeAction::Hold, // "done" but nothing held — nothing to conclude
            _ => ConcludeAction::Ambiguous,
        },
        PassIntent::Blocked => ConcludeAction::Blocked,
        PassIntent::Nothing => ConcludeAction::Nothing,
        PassIntent::Unclear => ConcludeAction::Hold,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the pass-reason → intent mapping the held-work settle
    // edge acts on. A regression that mis-reads "blocked" as done would conclude
    // (and grade) a card she was stuck on; one that mis-reads a bare pass as done
    // would auto-conclude every silent turn.
    #[test]
    fn classifies_the_four_intents_conservatively() {
        // bare pass → never concludes
        assert_eq!(classify_pass(None), PassIntent::Unclear);
        assert_eq!(classify_pass(Some("")), PassIntent::Unclear);

        // clear done (one-word and narrated)
        assert_eq!(classify_pass(Some("done")), PassIntent::Done);
        assert_eq!(classify_pass(Some("done — patch ready")), PassIntent::Done);
        assert_eq!(classify_pass(Some("the fix is complete")), PassIntent::Done);
        assert_eq!(classify_pass(Some("resolved")), PassIntent::Done);

        // blocked takes precedence over any progress language
        assert_eq!(classify_pass(Some("blocked: fixture missing")), PassIntent::Blocked);
        assert_eq!(
            classify_pass(Some("done investigating but stuck on the import")),
            PassIntent::Blocked
        );
        assert_eq!(classify_pass(Some("cannot reach the repo")), PassIntent::Blocked);

        // nothing-to-contribute → gap signal, not a completion
        assert_eq!(classify_pass(Some("nothing to add")), PassIntent::Nothing);
        assert_eq!(classify_pass(Some("no more changes needed here")), PassIntent::Nothing);

        // illegible → conclude nothing (the safe default)
        assert_eq!(classify_pass(Some("hmm let me think")), PassIntent::Unclear);
    }

    // what this catches: the completion-edge DECISION the held-work turn executes.
    // A regression that concluded a card on a blocked/nothing pass, or guessed
    // WHICH of several cards a lone "done" finished, would grade work she never
    // declared complete — or grade the wrong card.
    #[test]
    fn conclude_only_on_an_unambiguous_done() {
        let a = uuid::Uuid::from_u128(1);
        let b = uuid::Uuid::from_u128(2);

        // exactly one held card + done → conclude THAT card
        assert_eq!(
            conclude_from_pass(&[a], PassIntent::Done),
            ConcludeAction::Close(a)
        );
        // done + several held → never guess which
        assert_eq!(
            conclude_from_pass(&[a, b], PassIntent::Done),
            ConcludeAction::Ambiguous
        );
        // done but nothing held → nothing to conclude
        assert_eq!(conclude_from_pass(&[], PassIntent::Done), ConcludeAction::Hold);
        // blocked / nothing / unclear never conclude a card
        assert_eq!(
            conclude_from_pass(&[a], PassIntent::Blocked),
            ConcludeAction::Blocked
        );
        assert_eq!(
            conclude_from_pass(&[a], PassIntent::Nothing),
            ConcludeAction::Nothing
        );
        assert_eq!(
            conclude_from_pass(&[a], PassIntent::Unclear),
            ConcludeAction::Hold
        );
    }

    // what this catches: the end-to-end reason→intent→action path the edge runs,
    // pinned on the exact strings the burst asks for (PASS: done|blocked|nothing).
    #[test]
    fn the_burst_reason_forms_drive_the_right_action() {
        let card = uuid::Uuid::from_u128(9);
        let act = |r: &str| conclude_from_pass(&[card], classify_pass(Some(r)));
        assert_eq!(act("done"), ConcludeAction::Close(card));
        assert_eq!(act("done — patch ready"), ConcludeAction::Close(card));
        assert_eq!(act("blocked — the fixture is missing"), ConcludeAction::Blocked);
        assert_eq!(act("nothing to add"), ConcludeAction::Nothing);
    }
}
