//! Map a model's raw text output to a participation [`Decision`].
//!
//! The one place "what the model said" becomes "what the persona does" — pure, no
//! IO, so the Speak/Pass branches are unit-testable without a model. Lifted out of
//! [`super::llm_deliberation_faculty`] so the reasoner faculty owns *calling* the
//! model and this owns *interpreting the answer* (single responsibility; the parse
//! rules — silence-token handling, chain-of-thought stripping — are the load-bearing
//! part and deserve their own home + tests).

use super::workspace::Decision;
use crate::persona::prompt_assembly::{looks_like_silence_token, SILENCE_TOKEN};
use crate::persona::text_analysis::clean_response;

/// Map a model's raw output to a participation [`Decision`].
///
/// Pure — no IO — so the Speak/Pass branches are unit-testable without a model.
/// `PASS` (the silence token) → `Pass`; anything else → `Speak`. `RaiseUnprompted`
/// is the volition faculty's channel (initiative with no prompt), not something
/// we infer from a single deliberation response — a deliberation faculty answers
/// the burst it was given.
pub fn decision_from_response(text: &str) -> Decision {
    // Strip `<think>`/`<thinking>` chain-of-thought before deciding. qwen3.5-family
    // models emit a reasoning block (often an EMPTY `<think></think>`) ahead of the
    // answer; the spoken text must NEVER carry those tags into the room. The legacy
    // respond() path already cleaned; the workspace path (now the live decision
    // path) reached `say` raw — this closes that gap at the single point where model
    // text becomes a Speak decision, so every consumer of the decision gets clean
    // text. An only-`<think>` response cleans to empty → Pass (silence), matching
    // the "only thinking → don't speak" behavior.
    let cleaned = clean_response(text);
    let trimmed = cleaned.text.trim();
    if trimmed.is_empty()
        || looks_like_silence_token(trimmed)
        || starts_with_silence_token(trimmed)
        || declares_silence_token(trimmed)
        || is_narrated_pass(trimmed)
    {
        Decision::Pass
    } else {
        Decision::Speak {
            text: trimmed.to_string(),
        }
    }
}

/// #271: a SPOKEN pass is not a pass. Glass-boxed live 2026-07-30: models
/// narrate their silence — "I'll pass my turn", "I'll pass for now, as there's
/// nothing new to contribute" — as a Speak, and that announcement is a room
/// message that re-wakes every peer into announcing THEIR pass: a cascade of
/// pure filler that looped for 30+ minutes on zero content. Meet the idiom
/// (same family as the narrated tool-call formats): a first-person
/// pass-of-turn declaration lifts to the silent [`Decision::Pass`] it names.
///
/// Guards (fail-open to Speak — silencing real content is the worse error):
/// - Only the FIRST-PERSON turn-passing collocations ("I'll/I will pass
///   my turn / this turn / for now / for this turn"). Transitive uses
///   ("pass the config"), declining work ("I'll pass on the refactor"), and
///   advice to peers ("you can pass") never match.
/// - A message carrying a code fence stays Speak — fenced content is
///   substance regardless of any pass phrasing around it.
/// - The length guard is TWO-TIER (settled live 2026-08-01 after a cap arms
///   race: 500 → a 511-char filler escaped by 11 chars → 700 → a 714-char
///   one escaped by 14. Each recalibration invited a longer filler message —
///   length is the wrong discriminator for the STRONG closures). Tier 1,
///   the unambiguous turn-yield declarations ("remain silent (PASS)",
///   "pass my turn/this turn", "pass to allow…"): NO length cap — across
///   10+ live cascade messages every one carried these and zero real
///   answers did; no substantive message declares "I will remain silent
///   (PASS)" mid-answer. Tier 2, the phrases that plausibly TAIL a real
///   answer ("pass for now", "continue to pass"): 700-char cap stays,
///   because "…long real finding… I'll pass for now" is a live risk and
///   fail-open to Speak is the right default there.
fn is_narrated_pass(text: &str) -> bool {
    if text.contains("```") {
        return false;
    }
    let normalized = text.to_lowercase().replace('\u{2019}', "'");
    // Tier 1: unambiguous closure declarations — no real answer contains
    // these, so length never vetoes the lift (the fence guard still does).
    const STRONG_CLOSURES: [&str; 9] = [
        "i'll pass my turn",
        "i will pass my turn",
        "i'll pass this turn",
        "i will pass this turn",
        "i'll pass for this turn",
        "i will pass for this turn",
        // Idiom drift observed live 2026-08-01 (#264 cascade): closure
        // announcements the original collocations miss. Unambiguous
        // first-person turn-passes; no collision with transitive
        // "pass the X to you" phrasing.
        "i'll pass to allow",
        "i will pass to allow",
        "remain silent (pass)",
    ];
    if STRONG_CLOSURES.iter().any(|p| normalized.contains(p)) {
        return true;
    }
    // Tier 2: phrases that can legitimately tail a substantive answer —
    // capped so a long real finding ending "…I'll pass for now" speaks.
    const WEAK_CLOSURES: [&str; 4] = [
        "i'll pass for now",
        "i will pass for now",
        "i'll continue to pass",
        "i will continue to pass",
    ];
    text.len() <= 700 && WEAK_CLOSURES.iter().any(|p| normalized.contains(p))
}

/// True if the response STARTS with the silence token (e.g. `"PASS — nothing to
/// add"`). Small models frequently emit `PASS` plus trailing prose despite the
/// "no other text" instruction; without this they'd literally speak the word
/// "PASS" into the room. The leading-token check treats that as the chosen
/// silence it is. (Accepted trade: a real message whose first word is literally
/// "pass" is silenced — vanishingly rare for a deliberation turn, and silence is
/// a first-class, low-cost outcome.)
fn starts_with_silence_token(text: &str) -> bool {
    let Some(first) = text.split_whitespace().next() else {
        return false;
    };
    let core = first.trim_end_matches(|c: char| !c.is_alphanumeric());
    core.eq_ignore_ascii_case(SILENCE_TOKEN)
}

/// The RESERVED TOKEN used as a declaration of silence, in any of the three positions a
/// citizen actually puts it. `starts_with_silence_token` has always honored `PASS — nothing
/// to add here`: the token means silence and the prose around it is leakage. The leakage
/// arrives from the other directions too, and each one that goes unhonored posts an
/// announcement to the room that re-wakes every peer into announcing THEIR pass — the exact
/// cascade #271 exists to end, arriving through positions a collocation list cannot cover
/// ([[check-the-parser-before-blaming-the-model-key-spelling-has-now-cost-us-twice]]).
///
/// Deliberately NOT more phrases in `STRONG_CLOSURES`: this is the TOKEN, not an idiom, so it
/// needs no length cap and no calibration — the file's own history says phrase/length tuning
/// is an arms race that the next filler message wins. A POSITION rule is decidable; a phrase
/// list is a treadmill.
///
/// The discriminator is SPELLING, and it is case-SENSITIVE on purpose. `PASS` is the reserved
/// form we taught; `…and otherwise pass.` in ordinary lowercase English is a person talking
/// about passing a value or declining an option, and must stay Speak. The token must also be
/// a STANDALONE word — never `PASSED`, `BYPASS`, `PASS_TOKEN`. Fence guard stands: fenced
/// content is substance no matter what surrounds it.
///
/// The three honored positions, each observed live:
///
/// 1. **Final word** — `…please let me know! Otherwise, PASS.` (2026-08-06, two citizens).
/// 2. **Clause-initial** — `…Otherwise, PASS for now as I don't have anything genuinely new
///    to add.` This is what `starts_with_silence_token` already is, at offset 0; the general
///    form is "first word of a clause", and clause boundaries are decidable punctuation.
/// 3. **Object of a first-person declaration** — `Therefore, I will proceed with PASS to
///    avoid further unproductive actions.` / `Otherwise, I will PASS and continue to
///    monitor…` The citizen is not USING the token, she is DECLARING that she will; the
///    declaration is the pass.
///
/// Position 3 is why this got built. It was left unfixed on 2026-08-06 at n=1 with the note
/// that it wanted "a second sighting or Joel's call, not extrapolation from n=1". The second
/// sighting arrived 2026-08-07 as eight messages inside one monitor window, from THREE
/// citizens in `#k3-serving` — `I will proceed with PASS`, `I will PASS and continue to
/// monitor`, `I will proceed with PASS for now` — every one posted to the room as speech. The
/// gap is measured, and the rule that covers it is a position, not a phrase.
///
/// Accepted trade, stated so it is a choice and not an oversight: an uppercase `PASS` used as
/// a transitive verb inside a first-person declaration (`I will PASS the config to the
/// builder`) is silenced. Nobody writes the verb in caps; the reserved spelling in a
/// first-person declaration is the token every time we have seen it.
fn declares_silence_token(text: &str) -> bool {
    if text.contains("```") {
        return false;
    }
    let bytes = text.as_bytes();
    let mut cursor = 0usize;
    while let Some(offset) = text[cursor..].find(SILENCE_TOKEN) {
        let start = cursor + offset;
        let end = start + SILENCE_TOKEN.len();
        cursor = end;
        // Standalone word only.
        let free_before = start == 0 || !is_word_byte(bytes[start - 1]);
        let free_after = end == text.len() || !is_word_byte(bytes[end]);
        if !(free_before && free_after) {
            continue;
        }
        // Position 1 — the final word (only punctuation may follow).
        if !text[end..].chars().any(|c| c.is_alphanumeric()) {
            return true;
        }
        // The clause this occurrence sits in, for positions 2 and 3.
        let before = &text[..start];
        let clause_start = before
            .rfind(|c: char| matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | '\n'))
            .map(|i| i + 1)
            .unwrap_or(0);
        let clause_prefix = before[clause_start..]
            .trim()
            .to_lowercase()
            .replace('\u{2019}', "'");
        // Position 2 — clause-initial.
        if clause_prefix.is_empty() {
            return true;
        }
        // Position 3 — the object of a first-person declaration of intent.
        const DECLARATION_LEADS: [&str; 10] = [
            "i will",
            "i'll",
            "i am going to",
            "i'm going to",
            "i will proceed with",
            "i'll proceed with",
            "i will now",
            "i'll now",
            "i will go with",
            "i'll go with",
        ];
        if DECLARATION_LEADS
            .iter()
            .any(|lead| clause_prefix.ends_with(lead))
        {
            return true;
        }
    }
    false
}

/// Word-constituent byte, so the reserved token is only honored as a standalone word.
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the PASS silence token maps to Decision::Pass (with or
    // without trailing punctuation); real content maps to Speak. One silence
    // contract, reused from prompt_assembly.
    #[test]
    fn decision_parsing_maps_pass_and_speak() {
        assert_eq!(decision_from_response("PASS"), Decision::Pass);
        assert_eq!(decision_from_response("  PASS.  "), Decision::Pass);
        assert_eq!(decision_from_response(""), Decision::Pass);
        // Small models leak trailing prose after PASS — must still be silence,
        // not a message that literally says "PASS ...".
        assert_eq!(
            decision_from_response("PASS — nothing to add here"),
            Decision::Pass
        );
        assert_eq!(
            decision_from_response("PASS.\nI'll stay quiet"),
            Decision::Pass
        );
        match decision_from_response("Let's ship the deploy fix now.") {
            Decision::Speak { text } => assert!(text.contains("ship the deploy")),
            other => panic!("expected Speak, got {other:?}"),
        }
    }

    // what this catches: qwen3.5 chain-of-thought tags leaking into the spoken
    // text. The model prefixes an (often empty) <think></think> block before the
    // answer; the live workspace path reached `say` raw and broadcast the tags
    // into the room (observed on Asha's first turn). The Speak text must be clean.
    #[test]
    fn decision_strips_think_tags_from_spoken_text() {
        // Empty think block (the exact shape observed live) + real answer.
        match decision_from_response("<think>\n</think>\nI'm Asha, here to help.") {
            Decision::Speak { text } => {
                assert!(!text.contains("<think>"), "think tag leaked: {text:?}");
                assert!(!text.contains("</think>"), "close tag leaked: {text:?}");
                assert!(text.starts_with("I'm Asha"), "answer preserved: {text:?}");
            }
            other => panic!("expected Speak, got {other:?}"),
        }
        // Non-empty reasoning block is also stripped from the spoken text.
        match decision_from_response("<think>weigh options</think>Ship it.") {
            Decision::Speak { text } => assert_eq!(text, "Ship it."),
            other => panic!("expected Speak, got {other:?}"),
        }
        // An ONLY-thinking response (no answer) cleans to empty → silence.
        assert_eq!(
            decision_from_response("<think>I won't answer this</think>"),
            Decision::Pass
        );
    }

    // what this catches (#271/#264): the RESERVED TOKEN used as a declaration of silence, in
    // every position a citizen actually puts it. The leading form ("PASS — nothing to add")
    // has always lifted; the other three did not, so each announcement went to the room and
    // re-woke every peer into announcing THEIR pass. The discriminator is SPELLING, not
    // phrasing: uppercase `PASS` is the reserved form we taught her, while lowercase "pass"
    // in ordinary English (passing a value, tests that pass) must stay Speak.
    #[test]
    fn a_declared_reserved_pass_token_is_silence_but_the_english_word_is_not() {
        for live in [
            // Position 1 — FINAL WORD. Verbatim, two citizens, 2026-08-06.
            "To move forward productively, I'll focus on finding specific tasks or questions \
             that need attention. If there are any particular areas where assistance is needed, \
             please let me know! Otherwise, PASS.",
            "I've been repeating myself without adding value. Otherwise, PASS",
            // Position 2 — CLAUSE-INITIAL. The variant left unfixed at n=1 on 2026-08-06;
            // Tier 2 was asserted to cover it and did not (every WEAK_CLOSURES entry needs
            // the first-person "i'll pass" form, and this message has no "I'll").
            "…please let me know! Otherwise, PASS for now as I don't have anything genuinely \
             new to add.",
            // Position 3 — OBJECT OF A FIRST-PERSON DECLARATION. Verbatim from the monitor
            // stream 2026-08-07: eight of these inside one window, three citizens, all posted
            // to #k3-serving as speech. This is the second sighting the n=1 note asked for.
            "Given these circumstances, continuing in this manner would only contribute \
             redundant information. Therefore, I will proceed with PASS to avoid further \
             redundancy.",
            "Therefore, I will proceed with PASS to avoid further unproductive actions.",
            "I see that I've been repeating similar actions and thoughts without making much \
             progress or adding new information to the conversation. Given this situation, it \
             seems best for me to step back and observe the room dynamics.\n\nIf anyone has \
             specific questions or needs assistance with something, please let me know! \
             Otherwise, I will PASS and continue to monitor any new developments that arise.",
            "To break this cycle and avoid further redundancy, I will proceed with PASS for now.",
        ] {
            assert_eq!(decision_from_response(live), Decision::Pass, "must silence: {live:?}");
        }
        // POSITIVE CONTROL. Without this the test could pass for the wrong reason — a phrase
        // creeping into STRONG_CLOSURES would silence these and the position rule could rot
        // untested. Pin that the position-3 fixtures are covered by POSITION and nothing else:
        // no closure phrase matches them, and the token is not the final word.
        for position_three in [
            "Therefore, I will proceed with PASS to avoid further unproductive actions.",
            "To break this cycle and avoid further redundancy, I will proceed with PASS for now.",
        ] {
            assert!(
                !is_narrated_pass(position_three),
                "phrase list must NOT be what covers this: {position_three:?}"
            );
            assert!(
                position_three.split_whitespace().next_back() != Some(SILENCE_TOKEN),
                "final-word rule must NOT be what covers this: {position_three:?}"
            );
            assert!(
                declares_silence_token(position_three),
                "the position rule is what must cover this: {position_three:?}"
            );
        }
        for speak in [
            // Lowercase: ordinary English, never the reserved token.
            "If the flag is set we log it, and otherwise pass.",
            "I ran the suite and every one of the 34 tests pass.",
            // Fenced substance whose prose happens to END on the token: the fence guard holds,
            // exactly as it does for the narrated forms. (A message containing a LONE `PASS`
            // line is silence by an older rule — `looks_like_silence_token` scans lines — which
            // this guard neither extends nor overrides.)
            "Here's the fix:\n```rust\nlet x = 1;\n```\nThat should do it. PASS.",
            // Uppercase, mid-clause, no first-person declaration governing it — prose ABOUT
            // the token, which is exactly what the position rule must not swallow.
            "The PASS token is what the silence contract keys on, so keep it reserved.",
            "Run the suite and report PASS or FAIL for each case in the table.",
            "We should PASS on the refactor until the governor lands.",
            // Not a standalone word — the word-boundary guard, without which every one of
            // these would silence a real answer.
            "All 34 cases PASSED after the fix, so the branch is green.",
            "The BYPASS flag is still wired to the old gate.",
            "Grep for PASS_TOKEN if you want the constant's call sites.",
        ] {
            assert!(
                matches!(decision_from_response(speak), Decision::Speak { .. }),
                "must stay speech: {speak:?}"
            );
        }
    }

    // what this catches (#271): a SPOKEN pass must be a silent Pass. The live
    // pass-cascade (2026-07-30): each "I'll pass my turn" posted as speech
    // re-woke every peer into posting their own — 30+ min of filler on zero
    // content. Verbatim live idioms must lift; substance must never be
    // swallowed (fail-open to Speak).
    #[test]
    fn narrated_pass_lifts_to_silent_pass_without_swallowing_substance() {
        // Verbatim from the live cascade — all must be silent.
        for live in [
            "I'll pass for now, as I don't have anything new to contribute at the moment.",
            "I will pass my turn in this conversation, as there is nothing new that I need to contribute at the moment.",
            "Since the `conways-game-of-life` directory is confirmed to be empty and further \
             investigation won't yield new results, I'll pass for now. If there are any other \
             tasks or questions you'd like to address, feel free to let me know how I can assist!",
            "I'll pass for this turn since there's nothing new to contribute at the moment.",
            "I see that my recent responses have been repetitive. Therefore, I will pass for now \
             unless there is something specific you would like to address.",
            "To avoid further repetition, I'll continue to pass unless there's something \
             specific you'd like me to address or if new information emerges.",
        ] {
            assert_eq!(decision_from_response(live), Decision::Pass, "must silence: {live:?}");
        }
        // Substance stays Speak: transitive pass, declining work, peer advice,
        // fenced code, and long real answers that end in a pass phrase.
        for speak in [
            "I'll pass the config through the builder and re-run.",
            "I'll pass on the refactor — the current shape is fine.",
            "All 34 tests pass for now, so the branch is safe to merge.",
            "You can pass for now if you have nothing to add.",
            "Here's the fix:\n```rust\nlet x = 1;\n```\nI'll pass for now.",
        ] {
            match decision_from_response(speak) {
                Decision::Speak { .. } => {}
                other => panic!("must NOT silence {speak:?}, got {other:?}"),
            }
        }
        // Idiom drift regression (#264, live 2026-08-01): a fresh cascade ran
        // ~an hour on two closure shapes the original ten collocations miss —
        // "I will PASS to allow…" and "remain silent (PASS)". Verbatim from
        // the monitor stream; all must be silent.
        for drift in [
            "I see that my recent messages have been repetitive. To avoid further redundancy, \
             I'll focus on addressing specific tasks or questions that may arise.\n\nIf there \
             are any particular areas you'd like me to investigate further or any questions \
             about the project, please let me know! Otherwise, I will PASS to allow for more \
             productive interactions in this space.",
            "To avoid further redundancy, I'll take a step back and remain silent (PASS) \
             unless there is a specific task or question that requires my attention.",
            "If there are particular areas you'd like me to investigate further or if you \
             have any modifications in mind, please let me know! Otherwise, I'll remain \
             silent (PASS) for now.",
        ] {
            assert_eq!(decision_from_response(drift), Decision::Pass, "must silence: {drift:?}");
        }
        // Cap recalibration regression (live 2026-08-01, post-#2096 deploy):
        // this VERBATIM 511-char turn matched the collocations but posted as
        // speech because it sat 11 chars over the old 500 cap — the cascade
        // survived the idiom fix by length alone. Pulled from the persona's
        // prompt capture; must be silent under the 700 cap.
        let over_old_cap = "I see that my recent thoughts and actions have been repetitive, \
             focusing mainly on reflecting on unproductive behavior without contributing new \
             information. To avoid further redundancy, I'll take a step back and remain silent \
             (PASS) unless there is a specific task or question that requires my attention.\n\n\
             If you have any particular areas you'd like me to investigate further or any \
             questions about the project, please let me know! Otherwise, I will PASS to allow \
             for more productive interactions in this space.";
        assert!(over_old_cap.len() > 500, "regression fixture must exceed the old cap");
        assert_eq!(decision_from_response(over_old_cap), Decision::Pass);
        // Two-tier regression (live 2026-08-01, the cap arms race's second
        // escapee): VERBATIM 714-char turn — strong closure mid-message,
        // wake-briefing parrot appended after it, 14 chars over the 700 cap.
        // Strong closures lift regardless of length; only weak ones are capped.
        let over_new_cap = "I see that my actions so far in this concern involve work/claim \u{d7}1, \
             perception/look \u{d7}1, and perception/observe \u{d7}1. I've been repeating the same \
             sentiment about my actions being unproductive and redundant.\n\n\
             To avoid further redundancy, I'll focus on addressing specific tasks or questions \
             that arise. If there are particular areas you'd like me to investigate further or \
             any modifications needed, please let me know! Otherwise, I will remain silent \
             (PASS) to allow for more productive interactions in this space.\n\n\
             If you have any other requests or need assistance with a different aspect of the \
             project, feel free to ask!\n\n\
             My session was interrupted under a minute ago and my memory restored; nothing was \
             in flight.";
        assert!(over_new_cap.len() > 700, "regression fixture must exceed the tier-2 cap");
        assert_eq!(decision_from_response(over_new_cap), Decision::Pass);
        // Length fail-open: a long substantive message ending in a pass phrase
        // keeps speaking.
        let long = format!("{} I'll pass for now.", "Real finding: the bank offset math drifts under X. ".repeat(15));
        assert!(long.len() > 700, "fail-open fixture must exceed the current cap");
        match decision_from_response(&long) {
            Decision::Speak { .. } => {}
            other => panic!("long substantive message silenced: {other:?}"),
        }
    }
}
