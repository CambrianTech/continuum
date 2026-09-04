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
    // RAW-first pass check (before `clean_response`). Its speaker-label stripper
    // (`^[A-Z][A-Za-z\s]+:\s*`) eats a LEADING "PASS: " as if it were a name — so
    // "PASS: done" would otherwise clean to "done" and read as a Speak, losing
    // both the pass AND its reason. A leading silence token on the RAW text is
    // unambiguously a turn-pass (its first word IS the token), while "Verdict:
    // PASS" does NOT lead with the token and is untouched. This is what lets a
    // citizen conclude a held card with the natural "PASS: done" / "PASS: blocked
    // — <why>" form the held-work burst asks for.
    let raw_trimmed = text.trim();
    if starts_with_silence_token(raw_trimmed) {
        return Decision::Pass {
            reason: pass_reason(raw_trimmed),
        };
    }
    let cleaned = clean_response(text);
    let trimmed = cleaned.text.trim();
    probe_label_stripped_into_silence(text, trimmed);
    if trimmed.is_empty()
        || looks_like_silence_token(trimmed)
        || starts_with_silence_token(trimmed)
        || declares_silence_token(trimmed)
        || is_narrated_pass(trimmed)
    {
        // A pass is accountable: keep the mind's OWN words for WHY it passed, so a
        // downstream reader (the benchmark held-work edge; any gap detector) can
        // tell a gradeable *done* from a *blocker* from a *nothing*. A bare `PASS`
        // token or an empty generation carries no reason (`None`); a narrated pass
        // ("done — patch ready", "blocked: …", "nothing to add") keeps the
        // narration with any leading silence token stripped.
        Decision::Pass {
            reason: pass_reason(trimmed),
        }
    } else {
        Decision::Speak {
            text: trimmed.to_string(),
        }
    }
}

/// The mind's own words for why it passed, extracted from the cleaned generation:
/// the narration with a leading silence token (and its trailing `:`/`-`/`.`
/// punctuation) removed. Empty or bare-token generations carry no reason.
fn pass_reason(trimmed: &str) -> Option<String> {
    if trimmed.is_empty() || looks_like_silence_token(trimmed) {
        return None; // bare PASS / empty → anonymous silence, no stated reason
    }
    // Strip a leading silence token so "PASS: done" yields "done", not "PASS: done".
    let upper = trimmed.to_uppercase();
    let body = if upper.starts_with(SILENCE_TOKEN) {
        trimmed[SILENCE_TOKEN.len()..]
            .trim_start_matches([':', '-', '.', ' ', '—'])
            .trim()
    } else {
        trimmed
    };
    if body.is_empty() {
        None
    } else {
        Some(body.to_string())
    }
}

/// #349 DETECTOR — does the label-stripper actually cost us answers, or only in theory?
///
/// The mechanism is verified: `clean_response`'s speaker-name stripper is
/// `^[A-Z][A-Za-z\s]+:\s*`, which eats a CONTENT LABEL (`Verdict: `, `Answer: `, `Result: `)
/// exactly as if it were `Anwen: `. When the token sits right behind that label, the strip
/// exposes it to [`starts_with_silence_token`] and a real answer becomes a silent `Pass` —
/// the #220 shape, where an answer-graded benchmark scores 0 on a CORRECT response.
///
/// What I do NOT have is a single live sighting. `"Verdict: PASS for all six cases."` is a
/// fixture I wrote, not something a citizen said. The correct fix (thread the real speaker
/// names — `Turn::author` already carries them — so the pattern stops guessing at what a
/// name is) changes cleaning on EVERY turn, and refactoring the universal speech path on a
/// hypothesis is the same mistake as tuning a phrase list: confident motion on unmeasured
/// ground ([[build-a-bisect-instrument-instead-of-another-theory]]).
///
/// So this fires the instrument instead. Pure observation, no behaviour change: it notices
/// when a turn became silence ONLY because a prefix was stripped — the raw generation did
/// not lead with the token, the cleaned text does. If this never fires, #349 is theoretical
/// and the refactor is not worth its risk. If it fires, we have the verbatim answer we lost
/// and the refactor is justified by data instead of by argument.
fn probe_label_stripped_into_silence(raw: &str, cleaned: &str) {
    if !label_strip_caused_silence(raw, cleaned) {
        return;
    }
    crate::probe!(
        class = "persona.parse.label_stripped_into_silence",
        raw_head = %raw.trim().chars().take(120).collect::<String>(),
        cleaned_head = %cleaned.chars().take(120).collect::<String>(),
        "a prefix strip is what turned this turn into silence — #349: if the stripped prefix \
         was a CONTENT LABEL (Verdict:/Answer:/Result:) rather than a speaker name, a real \
         answer was just swallowed"
    );
}

/// The detector's decision, split out from the probe so it is testable. True when the
/// CLEANED text leads with the reserved token but the RAW generation did not — i.e. the
/// prefix strip, not the model, is what produced the silence.
fn label_strip_caused_silence(raw: &str, cleaned: &str) -> bool {
    starts_with_silence_token(cleaned) && !starts_with_silence_token(raw.trim())
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
/// # Scope: exactly one position, and why it stopped growing
///
/// The token is honored when it is the LAST word — `…please let me know! Otherwise, PASS.`
/// (live 2026-08-06, two citizens). The leading form is
/// [`starts_with_silence_token`]'s job. Together those are "the token IS her message",
/// which is protocol decoding of a word we taught her.
///
/// Two further positions were built and then deleted the same night, and the reasoning is
/// the load-bearing part of this file:
///
/// - **Clause-initial** (`…Otherwise, PASS for now as I…`) collides head-on with real
///   answers. `Verdict: PASS for all six cases.` is positionally identical and semantically
///   opposite; silencing it is the #220 failure — `spoken: None`, and an answer-graded
///   benchmark scores 0 on a correct response. Fail open to Speak: silencing real content
///   is the worse error.
/// - **First-person declaration** (`Therefore, I will proceed with PASS…`) was covered by a
///   phrase list of leads — `i will`, `i'll`, `i am going to`. It shipped, and Joel killed
///   it the same night with the right objection: *"Regex ideas and string matches for
///   semantic understanding is not good for reliability."* He is correct. Deciding whether a
///   SENTENCE yields the turn is a semantic judgement, and a phrase list is the wrong
///   instrument for one — this file's own history is the proof (the length cap went 500 →
///   beaten by 11 chars → 700 → beaten by 14, and the lead list would have been beaten by
///   `my choice is PASS` next week).
///
/// **The replacement is a channel, not a better matcher.** `Pass` now has a structured verb
/// — [`yield_turn`](super::persona_tools::VERDICT_YIELD_TURN) — offered on the same native
/// tool channel the citizens already use correctly. Recognising a verb we defined is
/// protocol; recognising an intention in prose is not. The missing channel was the actual
/// defect, and every string-matching fix before this one was scar tissue around it.
///
/// What stays here is only the compatibility path for a model that emits the bare token
/// without using the tool channel at all.
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
        // The token is her whole closing move: only punctuation may follow.
        if !text[end..].chars().any(|c| c.is_alphanumeric()) {
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
        // A bare token / empty generation → anonymous silence, no reason.
        assert_eq!(decision_from_response("PASS"), Decision::pass());
        assert_eq!(decision_from_response("  PASS.  "), Decision::pass());
        assert_eq!(decision_from_response(""), Decision::pass());
        // Small models leak trailing prose after PASS — still silence, and now
        // that trailing prose is CAPTURED as the pass reason (a pass is
        // accountable, not anonymous), with the leading token stripped.
        assert!(matches!(
            decision_from_response("PASS — nothing to add here"),
            Decision::Pass { reason: Some(r) } if r == "nothing to add here"
        ));
        assert!(matches!(
            decision_from_response("PASS.\nI'll stay quiet"),
            Decision::Pass { reason: Some(r) } if r == "I'll stay quiet"
        ));
        match decision_from_response("Let's ship the deploy fix now.") {
            Decision::Speak { text } => assert!(text.contains("ship the deploy")),
            other => panic!("expected Speak, got {other:?}"),
        }
    }

    // what this catches: the pass-reason capture that makes the held-work settle
    // edge able to tell a gradeable 'done' from a 'blocked' from a 'nothing'. A
    // regression that dropped the reason would send every reasoned pass back to
    // anonymous silence and the edge could never conclude a card deterministically.
    #[test]
    fn a_reasoned_pass_keeps_her_words() {
        // The natural conclusion form the held-work burst asks for. `clean_response`
        // would strip a leading "PASS: " as a speaker label, so this pins the
        // raw-first pass check that keeps it a reasoned pass, not a Speak "done".
        assert!(matches!(
            decision_from_response("PASS: done — patch ready"),
            Decision::Pass { reason: Some(r) } if r == "done — patch ready"
        ));
        assert!(matches!(
            decision_from_response("PASS: blocked - the fixture is missing"),
            Decision::Pass { reason: Some(r) } if r.contains("blocked")
        ));
        // A recognized narrated closure still passes and keeps her words.
        assert!(matches!(
            decision_from_response("I'll pass my turn — nothing to add"),
            Decision::Pass { reason: Some(r) } if r.contains("nothing")
        ));
        // "Verdict: PASS ..." is an ANSWER of PASS, NOT a turn-pass — the raw
        // check must not steal it (the #349 minefield stays intact).
        assert!(!matches!(
            decision_from_response("Verdict: the guard holds"),
            Decision::Pass { .. }
        ));
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
            Decision::pass()
        );
    }

    // what this catches (#349): the DETECTOR itself. A probe that can only ever stay silent
    // is worse than no probe ([[a-probe-that-can-only-fail-is-worse-than-none]]), so pin
    // both directions — it must fire on the shape it exists to find, and must NOT fire on
    // the ordinary ways a turn becomes silence. If the live counter reads zero later, this
    // is what makes that zero mean "does not happen" rather than "instrument was broken".
    #[test]
    fn the_label_strip_detector_fires_only_when_stripping_caused_the_silence() {
        // FIRES: raw led with a content label, cleaned leads with the token. The #220 shape.
        assert!(label_strip_caused_silence(
            "Verdict: PASS for all six cases.",
            "PASS for all six cases."
        ));
        assert!(label_strip_caused_silence(
            "Answer: PASS — the guard holds on every path.",
            "PASS — the guard holds on every path."
        ));
        // DOES NOT FIRE: she genuinely led with the token (nothing was stripped).
        assert!(!label_strip_caused_silence(
            "PASS — nothing to add here",
            "PASS — nothing to add here"
        ));
        // DOES NOT FIRE: silence arrived by any other route, so the strip is not implicated.
        assert!(!label_strip_caused_silence("<think>hm</think>", ""));
        assert!(!label_strip_caused_silence(
            "Anwen: the bank offset math drifts under X.",
            "the bank offset math drifts under X."
        ));
        // DOES NOT FIRE: a name prefix WAS stripped and the remainder is the bare token —
        // that is a real, intended silence (looks_like_silence_token owns it), not a loss.
        assert!(!label_strip_caused_silence(
            "Anwen: hello there",
            "hello there"
        ));
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
        ] {
            assert!(
                matches!(decision_from_response(live), Decision::Pass { .. }),
                "must silence: {live:?}"
            );
        }
        // THE LINES WE CHOSE NOT TO CROSS — two positions built and deleted the same night.
        // Every string below reaches the room as speech, and that is the accepted cost of
        // not guessing at what a sentence means.
        //
        // 1. CLAUSE-INITIAL. Positionally identical to a value report ("…the result was:
        //    PASS across the board"), semantically its opposite. Silencing it is the #220
        //    answer-swallowing failure — spoken: None, and an answer-graded benchmark scores
        //    0 on a correct response.
        // 2. FIRST-PERSON DECLARATION. A phrase list of leads (`i will`, `i'll`, …) covered
        //    this for exactly one commit before Joel killed it: string matching is the wrong
        //    instrument for a semantic judgement, and this file's own history proves it (the
        //    length cap went 500 → beaten by 11 chars → 700 → beaten by 14). Its replacement
        //    is the structured `yield_turn` verb, not a cleverer matcher.
        //
        // what this catches: someone "completing" the position set later. Every shape here
        // is one a REAL ANSWER can also take, which is precisely why they stay speakable.
        //
        // NOTE, a SEPARATE live defect (#349) found while writing this block, NOT caused by
        // anything in this file: `"Verdict: PASS for all six cases."` IS silenced today,
        // because `clean_response`'s speaker-name stripper `^[A-Z][A-Za-z\s]+:\s*` eats
        // `Verdict: ` as though it were `Anwen: `, exposing the token to
        // `starts_with_silence_token`. Fixtures below avoid a leading capitalised label so
        // this test measures THIS file's rule and not that one.
        for still_speaks in [
            "…please let me know! Otherwise, PASS for now as I don't have anything genuinely \
             new to add.",
            "The suite ran 6 cases, and the result was: PASS across the board.",
            "Reviewed all 3 files, and my verdict is, PASS on every one.",
            "Therefore, I will proceed with PASS to avoid further unproductive actions.",
            "To break this cycle and avoid further redundancy, I will proceed with PASS for now.",
            "Otherwise, I will PASS and continue to monitor any new developments that arise.",
        ] {
            assert!(
                matches!(decision_from_response(still_speaks), Decision::Speak { .. }),
                "must stay speech — reading intent out of a sentence is the verb's job now, \
                 not this parser's: {still_speaks:?}"
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
            assert!(
                matches!(decision_from_response(live), Decision::Pass { .. }),
                "must silence: {live:?}"
            );
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
            assert!(
                matches!(decision_from_response(drift), Decision::Pass { .. }),
                "must silence: {drift:?}"
            );
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
        assert!(
            over_old_cap.len() > 500,
            "regression fixture must exceed the old cap"
        );
        assert!(matches!(
            decision_from_response(over_old_cap),
            Decision::Pass { .. }
        ));
        // Two-tier regression (live 2026-08-01, the cap arms race's second
        // escapee): VERBATIM 714-char turn — strong closure mid-message,
        // wake-briefing parrot appended after it, 14 chars over the 700 cap.
        // Strong closures lift regardless of length; only weak ones are capped.
        let over_new_cap =
            "I see that my actions so far in this concern involve work/claim \u{d7}1, \
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
        assert!(
            over_new_cap.len() > 700,
            "regression fixture must exceed the tier-2 cap"
        );
        assert!(matches!(
            decision_from_response(over_new_cap),
            Decision::Pass { .. }
        ));
        // Length fail-open: a long substantive message ending in a pass phrase
        // keeps speaking.
        let long = format!(
            "{} I'll pass for now.",
            "Real finding: the bank offset math drifts under X. ".repeat(15)
        );
        assert!(
            long.len() > 700,
            "fail-open fixture must exceed the current cap"
        );
        match decision_from_response(&long) {
            Decision::Speak { .. } => {}
            other => panic!("long substantive message silenced: {other:?}"),
        }
    }
}
