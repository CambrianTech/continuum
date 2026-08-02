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
/// - Long messages stay Speak: every live narrated pass observed was under
///   ~400 chars; past 500 the message almost certainly carries substance the
///   room should hear. (Conservative cap, not a policy knob — the failure
///   mode it prevents is swallowing a real answer that happens to end
///   "...I'll pass for now".)
fn is_narrated_pass(text: &str) -> bool {
    if text.len() > 500 || text.contains("```") {
        return false;
    }
    let normalized = text.to_lowercase().replace('\u{2019}', "'");
    const PASS_COLLOCATIONS: [&str; 13] = [
        "i'll pass my turn",
        "i will pass my turn",
        "i'll pass this turn",
        "i will pass this turn",
        "i'll pass for now",
        "i will pass for now",
        "i'll pass for this turn",
        "i will pass for this turn",
        "i'll continue to pass",
        "i will continue to pass",
        // Idiom drift observed live 2026-08-01 (#264 cascade): three residents
        // looped ~an hour on closure announcements the first ten collocations
        // miss. Both shapes are unambiguous first-person turn-passes; neither
        // collides with transitive "pass the X to you" phrasing.
        "i'll pass to allow",
        "i will pass to allow",
        "remain silent (pass)",
    ];
    PASS_COLLOCATIONS
        .iter()
        .any(|phrase| normalized.contains(phrase))
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
        // Length fail-open: a long substantive message ending in a pass phrase
        // keeps speaking.
        let long = format!("{} I'll pass for now.", "Real finding: the bank offset math drifts under X. ".repeat(12));
        match decision_from_response(&long) {
            Decision::Speak { .. } => {}
            other => panic!("long substantive message silenced: {other:?}"),
        }
    }
}
