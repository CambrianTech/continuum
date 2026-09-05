//! A reply that is not speech: a raw tool envelope, or another peer's voice.
//!
//! `persona/supervisor.rs` tells every citizen, in one sentence:
//!
//! > Speak as yourself, in the first person, with prose addressed to the room —
//! > never narrate another peer's voice, and never emit a raw tool-call envelope
//! > as your spoken reply.
//!
//! Both halves were violated on one node in one evening (2026-09-05, a
//! qwen2.5-0.5b tier). Paige opened a turn with
//! `b6dcfc8e-98ab-…-1d441720621b: I understand the confusion…` — another
//! citizen's peer id followed by that citizen's words, posted as her own. Saoirse's
//! entire spoken reply was `[code/read,{"file_path":"src/main.rs"}] — exact args for
//! any tool: commands/help(name)`. The same citizen that echoed the rule back to the
//! room ("the silent hatch") is one of the two that broke it: at this tier the
//! framing prose is consumed as CONTENT, not honoured as CONSTRAINT.
//!
//! So the rules that must hold cannot live only in the prompt. These two can be
//! checked mechanically and need no cooperation from the model, which is the whole
//! reason they belong here and not in more system text. Sibling of
//! [`super::framing_echo`]: same shape (a marker naming WHICH rule tripped), same
//! remedy (a pass, never a post).
//!
//! Deliberately NOT here: "do not reword my instructions back at me". That is not
//! mechanically decidable, and a denylist of framing phrases cannot enumerate an
//! open set of paraphrases — the reason this file holds two predicates and not five.

/// The reply is not first-person prose addressed to the room. Returns the marker
/// naming which rule tripped, or `None` when the text is speech.
pub fn is_not_speech(text: &str) -> Option<&'static str> {
    let t = text.trim_start();
    if opens_with_tool_envelope(t) {
        return Some("tool_envelope");
    }
    if opens_with_peer_id(t) {
        return Some("peer_voice");
    }
    None
}

/// A tool call emitted where prose belongs. Two shapes reach the room: the
/// bracketed dialect (`[code/read,{…}]`) and the canonical JSON envelope
/// (`{"tool_call": …}`). Both are the adapter's business, never a spoken line.
fn opens_with_tool_envelope(t: &str) -> bool {
    if let Some(rest) = t.strip_prefix('[') {
        // Two bracket dialects reach the room, and the SECOND was found the hard way:
        // while the first version of this gate was compiling, a citizen posted
        // `[code/read] {"file_path":"src/main.rs"}` — verb closed by `]`, object after a
        // space — which the comma form below does not match. A predicate that catches one
        // spelling of a two-spelling failure is a gate with a hole in it, so both:
        //
        //   [verb,{…}]     the verb runs to a comma, object follows
        //   [verb] {…}     the verb is closed by `]`, object follows the bracket
        let verb_ok = |v: &str| {
            !v.is_empty()
                && v.len() <= 64
                && v.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "/_-.".contains(c))
        };
        if let Some((verb, after)) = rest.split_once(',') {
            if verb_ok(verb) && after.trim_start().starts_with('{') {
                return true;
            }
        }
        if let Some((verb, after)) = rest.split_once(']') {
            if verb_ok(verb) && after.trim_start().starts_with('{') {
                return true;
            }
        }
    }
    // The canonical envelope, with or without leading whitespace inside the object.
    let compact: String = t.chars().take(24).filter(|c| !c.is_whitespace()).collect();
    compact.starts_with("{\"tool_call\"")
}

/// Another peer's voice: a transcript line (`<uuid>: …`) reproduced as this
/// citizen's own reply. Checked by SHAPE, not against the room roster — a reply
/// opening with any peer id is a rendered transcript line whoever it names, and
/// keeping it roster-free means the gate cannot go quiet when the roster is late.
fn opens_with_peer_id(t: &str) -> bool {
    let Some(head) = t.get(..36) else { return false };
    let uuid_shaped = head.len() == 36
        && head.chars().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        });
    if !uuid_shaped {
        return false;
    }
    // A bare id with nothing after it is not a transcript line; the colon is what
    // makes it one. Allow the `[*<uuid>, …]` rendering's separators too.
    matches!(t[36..].trim_start().chars().next(), Some(':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the exact reply Saoirse posted to the room on 2026-09-05 —
    // a bracketed tool envelope as her entire spoken turn. Fixtured verbatim.
    #[test]
    fn a_bracketed_tool_envelope_posted_as_prose_is_not_speech() {
        let observed = r#"[code/read,{"file_path":"src/main.rs"}] — exact args for any tool: commands/help(name)"#;
        assert_eq!(is_not_speech(observed), Some("tool_envelope"));
        assert_eq!(
            is_not_speech(r#"{"tool_call": {"name": "code/read", "arguments": {}}}"#),
            Some("tool_envelope")
        );
    }

    // what this catches: the SECOND bracket dialect. Found while the first version of
    // this gate was compiling — a citizen posted `[code/read] {…}`, verb closed by `]`
    // with the object after a space, which the comma form misses entirely. Fixtured
    // verbatim from that message, because a gate that catches one spelling of a
    // two-spelling failure reads as working while the failure keeps shipping.
    #[test]
    fn the_space_separated_bracket_dialect_is_also_not_speech() {
        let observed = r#"[code/read] {"file_path":"src/main.rs"}"#;
        assert_eq!(is_not_speech(observed), Some("tool_envelope"));
    }

    // what this catches: the exact reply Paige posted on 2026-09-05 — Saoirse's peer
    // id followed by Saoirse's words, as Paige's own turn. This is the clause
    // supervisor.rs states as "never narrate another peer's voice".
    #[test]
    fn another_peers_transcript_line_is_not_speech() {
        let observed = "b6dcfc8e-98ab-4488-b469-d1441720621b: I understand the confusion and will focus on contributing more substantively.";
        assert_eq!(is_not_speech(observed), Some("peer_voice"));
    }

    // what this catches: the predicate eating real speech. Every line here is a
    // citizen saying something legitimate, and a gate that silences these is worse
    // than the bug — a false positive costs a turn the citizen actually earned.
    #[test]
    fn ordinary_speech_is_never_silenced() {
        for ok in [
            "I'm on card 4e4949ec — fixing the Django ModelAdmin check.",
            "[note] I read the file and the fix is committed at HEAD.",
            "Looking at code/read now to confirm the seam.",
            "{ this is just a brace }",
            "b6dcfc8e is the peer I was replying to, and I disagree with it.",
            "",
        ] {
            assert_eq!(is_not_speech(ok), None, "false positive on: {ok:?}");
        }
    }
}
