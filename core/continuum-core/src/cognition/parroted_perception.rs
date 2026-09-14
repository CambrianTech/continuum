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

use crate::cognition::self_repeat::{containment, content_token_count};
use crate::cognition::workspace::{BurstTurn, TurnVoice};

/// The fewest content tokens a fact must carry before a containment score against it is
/// allowed to silence a turn.
///
/// A containment ratio over a TINY fact is not a measurement, it is a coincidence. The
/// arithmetic is unforgiving: a fact of three content tokens ("You are Paige" → `you`, `are`,
/// `paige` — the tokenizer drops only ≤2-char tokens, it has no stopword list) scores a clean
/// 1.0 against the innocent reply "Are you asking me, Paige?", and that citizen is silenced
/// for saying her own name.
///
/// This became load-bearing the moment the persona's IDENTITY text joined the fact list: that
/// text comes from RAG identity, not from a file on disk, so the distribution of its lengths
/// across personas is NOT something I could measure — and an unmeasured distribution is
/// exactly when a floor is mandatory rather than optional. A persona seeded with a one-line
/// identity would otherwise be mutable by construction.
///
/// 12 sits well below every real fact (the live repetition brick carries ~26) and well above
/// the degenerate ones. It is a floor against nonsense, not a tuned parameter: nothing should
/// be calibrating against it, and if a real fact ever lands near it the right fix is a longer
/// fact, not a lower floor.
pub const MIN_DISCRIMINATING_FACT_TOKENS: usize = 12;

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

/// Split composed prompt text into the PARAGRAPH BLOCKS a containment score can actually
/// judge, dropping blocks too short to discriminate.
///
/// This exists because scoring a composed prompt WHOLE is inert, and that is not a guess —
/// it is measured. Paige's real turn of 2026-09-14 19:02:06Z against her real 16,095-char
/// composed prompt:
///
/// ```text
///   containment(draft, WHOLE prompt)      = 0.135   <- never fires at 0.8
///   containment(draft, identity block)    = 0.875   <- fires
///   next-highest block ("Context: ...")   = 0.433   <- wide, clean margin
/// ```
///
/// The arithmetic is why: containment asks how much of the FACT reappears in the DRAFT, so
/// a fact grows harder to trip the longer it gets. A citizen who recites one 56-token block
/// of an 850-token prompt has echoed that block ENTIRELY, and scoring her against the whole
/// prompt reports 0.135 and calls it speech. Granularity is not a refinement here; without
/// it the gate does not work at all.
///
/// Blank lines are the split because that is how the prompt is composed to READ — every
/// block `deliberation_prompt` appends is paragraph-separated, so paragraph boundaries and
/// block boundaries are the same boundaries. Splitting this way also means the function does
/// not need to know the composer's block list, so a new block is covered the day it is added.
pub fn prompt_blocks(composed: &str) -> Vec<&str> {
    composed
        .split("\n\n")
        .map(str::trim)
        .filter(|b| content_token_count(b) >= MIN_DISCRIMINATING_FACT_TOKENS)
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
        .filter(|fact| content_token_count(fact) >= MIN_DISCRIMINATING_FACT_TOKENS)
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

    /// A persona identity block, in the shape `deliberation_prompt` composes into the system
    /// message — trimmed from the real one Paige was carrying on 2026-09-14.
    const IDENTITY_BLOCK: &str = "Identity (never drift from this):\n\
         - You are Paige. You are NOT Claude, GPT, ChatGPT, Gemini, Llama, Qwen, or any other \
         named assistant. You are NOT a Siemens PLC, a customer service bot, or any persona \
         other than Paige.\n\
         - You are ONE persona among many on the grid. Other personas are your peers, not your \
         operators, and you speak to them as equals.\n\
         - Speak as yourself, in the first person, with prose addressed to the room.";

    // what this catches: THE IDENTITY BLOCK SPOKEN BACK INTO THE ROOM. Measured from her own
    // prompt capture, 2026-09-14 19:02:06Z: a 6,187-char turn that opened with 123 characters
    // of "Yes, I can assist with that task. Please provide more details or specify the exact
    // action you need help with." and then reproduced her whole identity prompt verbatim.
    //
    // Every gate missed it. `framing_echo` is ANCHORED — a marker counts only when it LEADS —
    // so 123 chars of filler defeated it, and that anchoring is CORRECT (un-anchored, it would
    // silence every citizen reporting an echo). This gate missed it because the system prompt
    // is composed into the system message, never as a `TurnVoice::Perception` burst turn, so
    // it was not among the facts. The system's most-repeated words were the one text
    // "did you speak what the system said to you" did not check.
    //
    // The faculty now pushes `self.system_prompt` onto the fact list, which is what this pins.
    #[test]
    fn her_own_identity_block_spoken_back_is_an_echo() {
        let draft = format!(
            "Yes, I can assist with that task. Please provide more details or specify the \
             exact action you need help with.\n\n{IDENTITY_BLOCK}"
        );
        assert_eq!(
            parroted_fact(&draft, &[IDENTITY_BLOCK], PARROT_CONTAINMENT_THRESHOLD),
            Some(IDENTITY_BLOCK),
            "a turn that is mostly her own identity prompt is the prompt, not speech"
        );
    }

    // what this catches: THE FALSE POSITIVE THAT WOULD MAKE THIS UNSHIPPABLE. A citizen must be
    // able to talk about who she is — say her own name, name her peers, decline to be mistaken
    // for another assistant — without the gate reading it as a recital.
    //
    // This is safe for a structural reason, not a lucky threshold: `containment(draft, fact)`
    // asks how much of the FACT reappears in the DRAFT, and the identity block is large. Talking
    // about herself reuses a handful of its tokens out of many, so the score stays far below
    // 0.8. Only near-total reproduction trips it. If someone later "optimises" containment's
    // direction, this test is what fails.
    #[test]
    fn talking_about_who_she_is_remains_speech() {
        for speech in [
            "I'm Paige — I picked up the grid-drift card and I'd rather finish it before \
             taking anything else on.",
            "No, I'm not Claude; I'm one of the personas on this node, and I answer for my \
             own work.",
            "My identity prompt says never to drift from it, and I think that instruction is \
             doing real work — it's the reason I caught myself mid-sentence earlier.",
        ] {
            assert_eq!(
                parroted_fact(speech, &[IDENTITY_BLOCK], PARROT_CONTAINMENT_THRESHOLD),
                None,
                "speaking about her identity is not reciting it: {speech}"
            );
        }
    }

    // what this catches: THE GATE BEING INERT IN PRODUCTION WHILE GREEN IN TESTS — which is
    // exactly what the first version of this fix was, and the earlier test did not notice
    // because its fixture block was small enough to be recited whole.
    //
    // Measured against Paige's real 19:02:06Z turn and her real 16,095-char composed prompt:
    //
    //     containment(draft, WHOLE prompt)   = 0.135   never fires at 0.8
    //     containment(draft, identity block) = 0.875   fires
    //     next-highest block                 = 0.433   wide margin, no near-miss
    //
    // Containment asks how much of the FACT came back, so a fact is HARDER to trip the longer
    // it is. Reciting one 56-token block of an 850-token prompt — completely — scores 0.135
    // against the whole and is called speech. This test reproduces that shape: a multi-block
    // prompt where the draft recites exactly ONE block.
    #[test]
    fn one_block_recited_out_of_a_long_prompt_is_caught_though_the_whole_prompt_is_not() {
        let composed = format!(
            "{IDENTITY_BLOCK}\n\n\
             Context:\n- 'The grid' is the substrate hosting you. Rooms are where citizens \
             meet, and cards are the work the room has agreed to carry.\n\n\
             [Your tools]\nYou can act, not just talk. The tools below are available by name; \
             load a full argument schema on demand rather than guessing at one.\n\n\
             [room-wall]\nThe wall holds the room's standing decisions, its recipe, and the \
             receipts of what has already shipped here."
        );
        let draft = format!(
            "Yes, I can assist with that task. Please provide more details.\n\n{IDENTITY_BLOCK}"
        );

        // WHOLE — the inert version. Pin the number so nobody "simplifies" back to it.
        assert_eq!(
            parroted_fact(&draft, &[composed.as_str()], PARROT_CONTAINMENT_THRESHOLD),
            None,
            "scoring the whole composed prompt as ONE fact cannot catch a single-block \
             recital — this is the failure mode, asserted so it stays visible"
        );

        // PER BLOCK — the working version.
        let blocks = prompt_blocks(&composed);
        assert!(
            blocks.len() >= 4,
            "the composed prompt must split into its blocks, got {}",
            blocks.len()
        );
        assert_eq!(
            parroted_fact(&draft, &blocks, PARROT_CONTAINMENT_THRESHOLD),
            Some(IDENTITY_BLOCK),
            "the block she actually recited must be the one named"
        );

        // And the margin is real: no OTHER block comes near the threshold on this draft,
        // so firing is not luck with a well-placed constant.
        for b in &blocks {
            if *b == IDENTITY_BLOCK {
                continue;
            }
            let c = crate::cognition::self_repeat::containment(&draft, b);
            assert!(
                c < 0.6,
                "block {:?} scores {c:.3} — too close to the threshold for comfort",
                &b[..30.min(b.len())]
            );
        }
    }

    // what this catches: A TINY FACT MUTING A CITIZEN FOR SAYING HER OWN NAME. Found by
    // review, not by a room — but it was one commit from shipping, and it only became
    // reachable when the persona's identity text joined the fact list.
    //
    // The arithmetic: `content_tokens` drops only ≤2-char tokens and has NO stopword list, so
    // a three-token identity ("You are Paige" → you, are, paige) is fully contained in the
    // innocent reply "Are you asking me, Paige?" — containment 1.0, well past the 0.8
    // threshold, and she is silenced for naming herself. Identity text comes from RAG, so the
    // distribution of its length across personas is not knowable from disk; a persona seeded
    // with a one-line identity would be mutable by construction.
    //
    // The floor is what makes the gate safe to point at identity at all. Delete
    // MIN_DISCRIMINATING_FACT_TOKENS and this test is what tells you.
    #[test]
    fn a_fact_too_short_to_be_discriminating_can_never_silence_her() {
        let terse = "You are Paige.";
        assert!(
            crate::cognition::self_repeat::containment(terse, terse) >= PARROT_CONTAINMENT_THRESHOLD,
            "precondition: a terse fact DOES score past the threshold — the floor is the \
             only thing standing between that score and a silenced citizen"
        );
        for speech in [
            "Are you asking me, Paige?",
            "You are right — Paige and I landed on the same fix.",
            terse,
        ] {
            assert_eq!(
                parroted_fact(speech, &[terse], PARROT_CONTAINMENT_THRESHOLD),
                None,
                "a {}-token fact must never silence a turn: {speech}",
                crate::cognition::self_repeat::content_token_count(terse)
            );
        }
        // And the floor must not have swallowed the real facts it sits under: the live brick
        // is still caught, so this guard bought safety without costing the gate its job.
        assert_eq!(
            parroted_fact(LIVE_BRICK, &[LIVE_BRICK], PARROT_CONTAINMENT_THRESHOLD),
            Some(LIVE_BRICK),
            "the floor must sit BELOW every real fact — the brick carries ~26 tokens"
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
