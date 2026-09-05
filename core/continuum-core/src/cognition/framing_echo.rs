//! Framing echo — a response that reflects the turn's OWN framing back into
//! the room is not speech; it is a PASS.
//!
//! The wake prompt is written to the persona ("[wake] You are Paige, awake on
//! the continuum grid. Nothing has been said in this room since you last
//! looked…"). For a small model the most likely continuation of second-person
//! narration is more of it, and on 2026-09-05 three citizens on two nodes
//! posted the wake prompt back into #academy — "You are ready for the…",
//! "I've been awake for a while, Saoirse…" — then echoed each other's echoes
//! (IntelMac's trace; 96 s for nine tokens on the CPU tier). The room read it
//! as speech; it was the prompt.
//!
//! ONE place for the wake prompt's fixed sentences: the composer
//! (`persona::service_loop`) builds from these constants and this gate matches
//! on them, so the two cannot drift ([[the-compression-principle]]).

/// The wake turn's tag — the composer opens with it, and a response that
/// STARTS with it is the prompt coming back.
pub const WAKE_TAG: &str = "[wake]";
/// "You are {name}, awake on the continuum grid."
pub const WAKE_OPENING: &str = "awake on the continuum grid";
pub const WAKE_QUIET: &str = "Nothing has been said in this room since you last looked";
pub const WAKE_NO_WORK: &str = "No work of yours is on record right now";
pub const WAKE_PRESENT: &str = "Present with you:";
pub const WAKE_MID_WORK: &str = "You are mid-work — cards you hold:";

/// Which framing marker a response reflects, if any. `None` = it reads as speech.
pub fn echoes_turn_framing(text: &str) -> Option<&'static str> {
    let t = text.trim_start();
    if t.starts_with(WAKE_TAG) {
        return Some("wake_tag");
    }
    // Second-person self-narration is the prompt's register, never a citizen's
    // line to a room ("You are ready for the next task" was the prompt talking).
    if t.starts_with("You are ") {
        return Some("second_person_self_narration");
    }
    for s in [WAKE_OPENING, WAKE_QUIET, WAKE_NO_WORK, WAKE_PRESENT, WAKE_MID_WORK] {
        if t.contains(s) {
            return Some("wake_sentence");
        }
    }
    // Bracketed envelope tags belong to the inbound framing ("[room …]",
    // "[Conversation …]"), never to an utterance.
    if t.starts_with("[room ") || t.contains("[Conversation") {
        return Some("bracket_framing");
    }
    // A tool SCHEMA reproduced as a message — "[action #1] work/release({\"description\":…" —
    // is the offered-tools framing coming back, not a call and not speech (IntelMac, Paige).
    if t.starts_with("[action #") {
        return Some("tool_schema_echo");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the three observed echo shapes (the tag, second-person
    // self-narration, a verbatim wake sentence) are recognised, and an ordinary
    // room line — even one that mentions being awake in the first person — is not.
    #[test]
    fn the_observed_echo_shapes_are_caught_and_speech_is_not() {
        assert_eq!(
            echoes_turn_framing("[wake] You are Paige, awake on the continuum grid."),
            Some("wake_tag")
        );
        assert_eq!(
            echoes_turn_framing("You are ready for the next task, Paige."),
            Some("second_person_self_narration")
        );
        assert_eq!(
            echoes_turn_framing("Hello — Nothing has been said in this room since you last looked. Present with you: Kimi."),
            Some("wake_sentence")
        );
        assert_eq!(
            echoes_turn_framing("I've been awake for a while, Saoirse. [Conversation ended]"),
            Some("bracket_framing")
        );
        assert_eq!(
            echoes_turn_framing("[action #1] work/release({\"description\":\"Release a task from the board. U"),
            Some("tool_schema_echo")
        );
        assert_eq!(echoes_turn_framing("I'm awake and reading django-14631 now; the FK ordering is the bug."), None);
        assert_eq!(echoes_turn_framing("I will pass as I have nothing new to contribute."), None);
    }
}
