//! A persona must not SPEAK the words the system said TO her.
//!
//! # The turn that produced this file
//!
//! 2026-08-06, ten minutes after a deploy, in Joel's own room. Anwen emitted three
//! near-identical "I'll remain silent unless there are specific questions" turns. The
//! repetition brick fired, correctly. Her next outbound message was:
//!
//! ```text
//! [repetition] 4 of your recent messages were nearly identical — you're circling, and
//! restating what you've already said adds nothing. If you have nothing genuinely new to
//! contribute right now, silence (PASS) is the honest response.
//! ```
//!
//! Verbatim — `deliberation_budget.rs` renders exactly that with `best = 4`. Second person
//! intact. She copied the coaching instead of obeying it, and **the mechanism built to break
//! the loop became the loop's next turn.**
//!
//! # Why it happens, and why phrasing cannot fix it
//!
//! Coaching and conversation arrive through the SAME channel. A perception fact is prose in
//! the burst, sitting beside peer speech. A model cannot tell "this was said TO me" from
//! "this is the kind of thing I say here" — so it imitates rather than obeys. Joel's Sahar
//! datum is the same failure with a longer fuse: told the right verb, used it on the next
//! turn, reverted two turns later. The hint landed and decayed, because a hint in the content
//! channel IS content.
//!
//! Rewording the bricks cannot fix this. Whatever they say is still text in the channel she
//! draws from.
//!
//! # What this gate is NOT
//!
//! **Not a reserved-word ban.** The obvious patch — refuse any draft containing `[repetition]`
//! — is wrong, and it was written and thrown away before this file existed. Citizens discuss
//! their own cognition constantly ("the repetition brick fired on me and I think it misread
//! the turn"); that is some of the most valuable speech in the system, and a token ban would
//! make a persona unable to talk about her own mind. It is also the phrase-list arms race Joel
//! has already ruled out twice.
//!
//! **Not a stripper.** Deleting the echoed span and posting the remainder would make a
//! parroted turn LOOK like a real contribution. A turn whose entire content is a reflection of
//! its own prompt contributed nothing; the honest rendering of that is silence.
//!
//! # What it IS
//!
//! Two structural facts, no vocabulary anywhere:
//!
//! 1. **Which text is the system's own** comes from burst STRUCTURE, not from string matching:
//!    a perception fact carries [`TurnVoice::Perception`]. Every new brick is covered the day
//!    it is written, with no list to maintain. (Contrast #330: ~20 markers as bare literals
//!    across ~20 files, zero symbol uses — a guard keyed on that list would rot immediately.)
//!
//!    `TurnVoice` exists BECAUSE the first version of this gate got it wrong. It keyed on
//!    "unattributed", which is also how `Workspace::new(raw)` builds a peer stimulus, so the
//!    gate silenced a legitimate reply. Authorship answers "whose voice"; it cannot answer
//!    "is this speech at all". The faculty's own test caught that before any room saw it.
//!
//! 2. **Whether she echoed it** is asymmetric containment, not similarity — see
//!    [`crate::cognition::self_repeat::containment`]. "How much of what you were just told
//!    did you reproduce", so padding an echo with her own words cannot launder it.
//!
//! Discussion scores low (a few shared words out of thirty); reproduction scores ~1.0. The
//! separation is wide, which is what makes a single threshold honest here.

use crate::cognition::self_repeat::containment;
use crate::cognition::workspace::{BurstTurn, TurnVoice};

/// How much of one perception fact must reappear in a draft before it is an echo rather
/// than a mention.
///
/// Deliberately equal to [`crate::cognition::self_repeat::SELF_REPEAT_THRESHOLD`]: "you
/// repeated yourself" and "you repeated what you were told" are the same judgement about the
/// same kind of text, and two different numbers would be two different opinions about one
/// question. The measured separation is wide — Anwen's parrot was 1.0, a citizen discussing
/// the brick lands near 0.1 — so this sits in a large empty gap, not on a cliff edge.
pub const PARROT_CONTAINMENT_THRESHOLD: f64 = 0.8;

/// The system's OWN words in this turn's burst — what it said TO her.
///
/// Identified by [`TurnVoice`], not by content and NOT by authorship. Authorship was the
/// first attempt and it was wrong: `Workspace::new(raw)` also produces an unattributed turn,
/// so "no author" covered peer stimuli too and the gate silenced a legitimate reply in
/// `deliberates_through_a_real_adapter`. The test caught it before it ever reached a room.
/// Voice is the honest discriminator — it records that the SYSTEM wrote this, which is
/// exactly the property that makes reproducing it wrong.
pub fn perception_facts(turns: &[BurstTurn]) -> Vec<&str> {
    turns
        .iter()
        .filter(|t| t.voice == TurnVoice::Perception)
        .map(|t| t.content.as_str())
        .filter(|c| !c.trim().is_empty())
        .collect()
}

/// The perception fact this draft is echoing, if any.
///
/// Returns the fact rather than a bool so the caller can say WHICH one in the probe and in
/// her own perception — "you echoed the thing you were told" is only actionable if she can
/// see which thing. Empty drafts and empty fact sets are trivially not echoes.
pub fn parroted_fact<'a>(draft: &str, facts: &[&'a str], threshold: f64) -> Option<&'a str> {
    if draft.trim().is_empty() {
        return None;
    }
    facts
        .iter()
        .copied()
        .find(|fact| containment(draft, fact) >= threshold)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The live brick, verbatim from `deliberation_budget.rs` with `best = 4` — the exact
    /// text Anwen emitted into the room on 2026-08-06.
    const LIVE_BRICK: &str = "[repetition] 4 of your recent messages were nearly identical — \
         you're circling, and restating what you've already said adds nothing. If you have \
         nothing genuinely new to contribute right now, silence (PASS) is the honest response.";

    fn fact(content: &str) -> BurstTurn {
        BurstTurn::perception(content)
    }

    fn peer(author: &str, content: &str) -> BurstTurn {
        BurstTurn::attributed(false, author, content, None)
    }

    // what this catches: THE regression this file exists for. Anwen spoke the anti-loop brick
    // verbatim instead of obeying it (live, 2026-08-06, build 387628ed4). If this ever passes
    // the gate again, the mechanism designed to break loops is feeding them.
    #[test]
    fn the_brick_she_was_handed_is_not_something_she_may_say() {
        let turns = vec![fact(LIVE_BRICK)];
        let facts = perception_facts(&turns);
        assert_eq!(
            facts.len(),
            1,
            "a perception-voiced turn IS a perception fact"
        );
        assert_eq!(
            parroted_fact(LIVE_BRICK, &facts, PARROT_CONTAINMENT_THRESHOLD),
            Some(LIVE_BRICK),
            "reproducing the coaching verbatim must be caught"
        );
    }

    // what this catches: the reason this is not a reserved-word ban. A citizen discussing her
    // own cognition — some of the most valuable speech in the system — must stay speakable.
    // A token-level guard would refuse this sentence, and that would be worse than the bug.
    #[test]
    fn talking_about_the_brick_is_legitimate_speech() {
        let turns = vec![fact(LIVE_BRICK)];
        let facts = perception_facts(&turns);
        let discussion = "The [repetition] fact fired on me just now and I think it misread \
             the turn — those three messages were answering different questions.";
        assert_eq!(
            parroted_fact(discussion, &facts, PARROT_CONTAINMENT_THRESHOLD),
            None,
            "mentioning a fact is not reproducing it: {discussion}"
        );
    }

    // what this catches: verbosity must not launder an echo. This is precisely why the measure
    // is asymmetric containment and not Jaccard — padding the reproduction with her own words
    // collapses a symmetric score while leaving containment at 1.0.
    #[test]
    fn padding_an_echo_with_her_own_words_does_not_launder_it() {
        let turns = vec![fact(LIVE_BRICK)];
        let facts = perception_facts(&turns);
        let padded = format!(
            "Thanks for the note, here is my reply. {LIVE_BRICK} Anyway, that is where I stand \
             on the matter and I will wait for someone else to weigh in before continuing."
        );
        assert_eq!(
            parroted_fact(&padded, &facts, PARROT_CONTAINMENT_THRESHOLD),
            Some(LIVE_BRICK),
            "an echo wrapped in filler is still an echo"
        );
    }

    // what this catches: the gate must never touch PEER speech. Agreeing with a teammate by
    // restating their point is normal conversation, and #303 already treats cross-speaker echo
    // as its own separate concern with its own perception fact. Only text with no author —
    // the system's own — is off limits.
    #[test]
    fn echoing_a_peer_is_a_different_concern_and_not_this_gate() {
        let turns = vec![peer(
            "BigMama",
            "The consolidator has zero production callers.",
        )];
        let facts = perception_facts(&turns);
        assert!(
            facts.is_empty(),
            "a peer turn is speech, never a perception fact"
        );
        assert_eq!(
            parroted_fact("The consolidator has zero production callers.", &facts, 0.8),
            None
        );
    }

    // what this catches: the design flaw the faculty test found before a room ever saw it.
    // `Workspace::new(raw_stimulus)` builds an UNATTRIBUTED turn, so the first version of this
    // gate — keyed on "no author" — treated a peer's message as a system fact and silenced a
    // perfectly good reply to it. Authorship answers "whose voice"; it cannot answer "is this
    // speech at all". Only TurnVoice can, which is why it exists.
    #[test]
    fn an_unattributed_peer_stimulus_is_still_speech_not_a_fact() {
        let turns = vec![BurstTurn::opaque(
            "teammate asks: where did we land on the deploy?",
        )];
        assert!(
            perception_facts(&turns).is_empty(),
            "an unattributed STIMULUS is speech — silencing a reply to it would mute her"
        );
    }

    // what this catches: a turn with nothing handed to her cannot be an echo, and an empty
    // draft is already silence. Both are the no-op path on every single live turn, so a bug
    // here would be a constant cost paid for nothing.
    #[test]
    fn nothing_handed_to_her_and_nothing_said_are_both_no_ops() {
        assert_eq!(parroted_fact("Ship it.", &[], 0.8), None);
        assert_eq!(parroted_fact("   ", &[LIVE_BRICK], 0.8), None);
    }
}
