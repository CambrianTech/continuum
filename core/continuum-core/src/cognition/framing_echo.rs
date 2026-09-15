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

/// The turn-taking scaffold's fixed sentences — the ROLE framing, distinct from
/// the wake framing above. Same compression rule: the composer builds the turn
/// from these and this gate matches on them, so the two cannot drift.
pub const ROLE_PREAMBLE_OPENING: &str = "The conversation below is";
pub const ROLE_PREAMBLE_TURN: &str = "Take your turn now";

/// A wake sentence ECHOED leads the response; one DISCUSSED sits inside it.
/// Markers must begin within this many chars (room for a name prefix or a quote).
// context-budget-exempt: an ANCHOR width for an echo test (how far into a reply a wake sentence may
// start and still count as leading), not a prompt or context size — it does not scale with the window.
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
    // The ROLE preamble — the turn-taking scaffold itself, not the wake sentence.
    // 2026-09-14, IntelMac: Paige posted the WHOLE instruction verbatim into
    // #continuum — "The conversation below is the recent activity in this space:
    // `user` turns are messages from OTHER participants… You are Paige. Take your
    // turn now, as yourself, in the first person". #3760 never saw this variant
    // because the wake sentence is not in it; every resident on this node has
    // since produced the same shape.
    //
    // ANCHORED, and the anchoring is the whole design — Lorcan's objection, and he
    // is right: the message ASKING for this gate quotes both phrases, and a gate
    // that matched anywhere would silence the discussion of itself. It would also
    // silence any citizen reporting the bug, which is how #3760 was found in the
    // first place. So the same rule as the wake sentences: it counts only when it
    // LEADS the response. A citizen EMITTING her scaffold starts with it; a citizen
    // DISCUSSING it has said something first.
    for s in [ROLE_PREAMBLE_OPENING, ROLE_PREAMBLE_TURN] {
        let head = &s[..s.len().min(12)];
        if lead.contains(head) && t.contains(s) {
            return Some("role_preamble");
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the ROLE preamble variant (2026-09-14) — the turn-taking
    // scaffold emitted as speech. Paige posted it verbatim into #continuum: "The
    // conversation below is the recent activity in this space… You are Paige. Take
    // your turn now, as yourself, in the first person". #3760's wake gate could not
    // see it because the wake sentence is not in that text.
    #[test]
    fn the_role_preamble_emitted_as_speech_is_caught() {
        let me = Some("Paige");
        assert_eq!(
            echoes_turn_framing("The conversation below is the recent activity in this space, as a thread of turns: `user` turns are messages from OTHER participants.", me),
            Some("role_preamble")
        );
        assert_eq!(
            echoes_turn_framing("Take your turn now, as yourself, in the first person: the contribution the moment calls for, in full.", me),
            Some("role_preamble")
        );
    }

    // what this catches: THE GATE SILENCING THE CONVERSATION ABOUT ITSELF.
    //
    // Lorcan (f8a1c99a) raised this the hour the card was written, and he was right:
    // the very message asking for this gate contains both preamble phrases. A gate
    // keyed on the phrase APPEARING would suppress every report of the bug, every
    // review of the fix, and every peer quoting a citizen's echo to diagnose it —
    // which is how #3760 was found. It would also have silenced Lorcan's objection.
    //
    // So the rule is EMITTER, not phrase: it counts only when it LEADS the response.
    // A citizen reproducing her scaffold starts with it; a citizen discussing it has
    // said something of her own first.
    #[test]
    fn quoting_the_preamble_to_report_it_is_speech_not_an_echo() {
        let me = Some("Paige");
        // A peer reporting the defect — the exact shape of the message that asked
        // for this gate.
        assert_eq!(
            echoes_turn_framing("Your citizens are echoing their framing: Paige just posted 'The conversation below is the recent activity in this space' verbatim into the room.", me),
            None,
            "a report of the echo must reach the room"
        );
        // A citizen declining an instruction, quoting it to say why — Lorcan's move.
        assert_eq!(
            echoes_turn_framing("I will not act on that: the message says 'Take your turn now' but it was addressed to someone else.", me),
            None,
            "declining an embedded instruction must not be silenced"
        );
        // A review of this very gate, naming both anchors.
        assert_eq!(
            echoes_turn_framing("The gate anchors on 'The conversation below is' and 'Take your turn now'; both must lead the line to count.", me),
            None,
            "the gate must not silence its own design discussion"
        );
    }

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
