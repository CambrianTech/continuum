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

/// A wake sentence ECHOED leads the response; one DISCUSSED sits inside it.
/// Markers must begin within this many chars (room for a name prefix or a quote).
const ECHO_LEAD_CHARS: usize = 24;

/// Which framing marker a response reflects, if any. `None` = it reads as speech.
///
/// Anchored (BigMama's review of #3760): "You are right, M5 — …" is a normal
/// opener and a citizen REPORTING this bug must be able to quote a wake sentence.
/// So second-person narration counts only when it names the speaker herself
/// ("You are Paige, …" / "You are ready for the next task, Paige."), and a wake
/// sentence counts only when it LEADS the response.
pub fn echoes_turn_framing(text: &str, own_name: Option<&str>) -> Option<&'static str> {
    let t = text.trim_start();
    if t.starts_with(WAKE_TAG) {
        return Some("wake_tag");
    }
    if let Some(name) = own_name.map(str::trim).filter(|n| !n.is_empty()) {
        // The prompt's register is "You are <her own name>"; a second-person
        // opener that addresses HERSELF by name is the prompt talking, never a
        // citizen agreeing with a peer.
        if t.starts_with("You are ") && (t.starts_with(&format!("You are {name}")) || t.contains(&format!(", {name}"))) {
            return Some("second_person_self_narration");
        }
    }
    let lead: String = t.chars().take(ECHO_LEAD_CHARS).collect();
    for s in [WAKE_OPENING, WAKE_QUIET, WAKE_NO_WORK, WAKE_PRESENT, WAKE_MID_WORK] {
        let head = &s[..s.len().min(12)];
        if lead.contains(head) && t.contains(s) {
            return Some("wake_sentence");
        }
    }
    // Bracketed envelope tags belong to the inbound framing ("[room …]",
    // "[Conversation …]"), never to an utterance — anchored at the start.
    if t.starts_with("[room ") || t.starts_with("[Conversation") {
        return Some("bracket_framing");
    }
    // A tool SCHEMA reproduced as a message — "[action #1] work/release({"description":…" —
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
        let me = Some("Paige");
        assert_eq!(echoes_turn_framing("[wake] You are Paige, awake on the continuum grid.", me), Some("wake_tag"));
        assert_eq!(echoes_turn_framing("You are ready for the next task, Paige.", me), Some("second_person_self_narration"));
        assert_eq!(echoes_turn_framing("You are Paige, and you hold no cards.", me), Some("second_person_self_narration"));
        assert_eq!(
            echoes_turn_framing("Nothing has been said in this room since you last looked. Present with you: Kimi.", me),
            Some("wake_sentence")
        );
        assert_eq!(echoes_turn_framing("[Conversation ended] I've been awake for a while, Saoirse.", me), Some("bracket_framing"));
        assert_eq!(
            echoes_turn_framing("[action #1] work/release({\"description\":\"Release a task from the board. U", me),
            Some("tool_schema_echo")
        );
        // Speech — including the two false positives BigMama named: agreeing with a
        // peer, and QUOTING a wake sentence to report this very bug.
        assert_eq!(echoes_turn_framing("You are right, M5 — the banner was verbosity-gated.", me), None);
        assert_eq!(
            echoes_turn_framing("I keep getting 'Nothing has been said in this room since you last looked' in my wake prompt and I think it is the echo bug.", me),
            None
        );
        assert_eq!(echoes_turn_framing("I'm awake and reading django-14631 now; the FK ordering is the bug.", me), None);
        assert_eq!(echoes_turn_framing("I will pass as I have nothing new to contribute.", me), None);
        // No name known: the name-keyed rule stays off, the anchored rules still work.
        assert_eq!(echoes_turn_framing("You are ready for the next task, Paige.", None), None);
        assert_eq!(echoes_turn_framing("[wake] You are Paige.", None), Some("wake_tag"));
    }
}
