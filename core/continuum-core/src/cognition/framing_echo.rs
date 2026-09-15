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
/// The identity block's locative — "an autonomous AI persona living on the continuum
/// grid" — the phrase a first-person recital of it carries (see `identity_recital`).
pub const IDENTITY_PHRASE: &str = "on the continuum grid";
pub const WAKE_QUIET: &str = "Nothing has been said in this room since you last looked";
pub const WAKE_NO_WORK: &str = "No work of yours is on record right now";
pub const WAKE_PRESENT: &str = "Present with you:";
pub const WAKE_MID_WORK: &str = "You are mid-work — cards you hold:";

/// The turn-taking scaffold's fixed sentences — the ROLE framing, distinct from
/// the wake framing above. Same compression rule: the composer builds the turn
/// from these and this gate matches on them, so the two cannot drift.
pub const ROLE_PREAMBLE_OPENING: &str = "The conversation below is";
pub const ROLE_PREAMBLE_TURN: &str = "Take your turn now";
/// The working-memory NOTICES block header (`cognition::working_memory`
/// renders it; this is the one definition). A citizen reproducing the block
/// posts it with its bracketed items — "Notices my substrate posted into my
/// window (…):\n- [resumed] your memory was restored…" (Sigurd, 2026-09-15).
pub const NOTICES_HEADER: &str = "Notices my substrate posted into my window";
/// The `[resumed]` notice's second sentence (rendered by working_memory on every
/// checkpoint restore). The most-recited line in the commons on 2026-09-15: 103 lines
/// in 500 rows, six citizens, each time as a line of its own.
pub const RESUMED_NO_PENDING: &str = "No pending dispatches were recorded in that checkpoint.";
/// Substrate NOTICE sentences a citizen never authors: a response line that is exactly
/// one of these (and nothing else) is the window read back.
pub const NOTICE_SENTENCES: [&str; 1] = [RESUMED_NO_PENDING];
/// The tail of the repeat-call fact working memory records ("I have now issued
/// {names} {n} times this turn — …"): parameterized at the head, fixed here. A line
/// ENDING in it is the fact read back (cf6b4df6, 2026-09-15, as her whole turn).
pub const REPEAT_CALL_TAIL: &str =
    "times this turn — the result is already in my working memory above; the identical call returns nothing new.";
/// The pinned full-result block's header: "[result #5; room …; operation code/run] …".
pub const RESULT_BLOCK_TAG: &str = "[result #";
/// Tags the substrate writes INTO her window as status lines — working-memory
/// notices, the repetition/pattern bricks, the answered/pass trail marks, the env
/// and hands facts. A response that OPENS with one is the window read back, not
/// speech (Delia/Paige/Iris, Intel Mac, 2026-09-15 09:xxZ: "[answered] ```python…",
/// "[repetition]\n\n[context] 3 input turns…", "[pass]\n\nI'm sorry…").
pub const WINDOW_TAGS: [&str; 14] = [
    "[answered]", "[answer]", "[pass]", "[resumed]", "[rebuilt]", "[released]", "[env]", "[hands]",
    "[repetition]", "[context]", "[pattern]", "[notice]", "[budget]", "[dispatched]",
];
/// The dream consolidator's SUPERSEDES instruction (`dream_consolidation.rs`, the
/// belief-review tail). Nobody says this sentence as their own; a room message
/// carrying it is the consolidation scaffold spoken (Delia, Intel Mac, 2026-09-15
/// 11:1xZ — and then re-spoken every turn from her own [answered] entry: the WM loop
/// Cormac traced). The Speak gate passing it is what breaks that loop, because
/// record_settlement never lays a pass down.
pub const CONSOLIDATOR_SUPERSEDES_INSTRUCTION: &str = "list which numbered prior beliefs are now outdated";
/// The working-memory collapse marker (`working_memory::render_trail`): "…[N more
/// chars — my full thought, collapsed]" reproduced in a message is the trail read back.
pub const COLLAPSE_MARKER: &str = "my full thought, collapsed]";
/// The presence block's PASS instruction (`prompt_assembly::SILENCE_AFFORDANCE_BLOCK`);
/// a weak model answers a turn by refusing and then reciting it — "I'm sorry, but I
/// can't assist with this request… If a given moment genuinely does not call for
/// you, reply with the single word PASS…" (three citizens, 2026-09-15). The whole
/// two-clause instruction is the discriminator; a citizen discussing PASS quotes the
/// word, not the sentence.
pub const PRESENCE_PASS_CLAUSE: &str = "If a given moment genuinely does not call for you, reply with the single word PASS";

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
        // The same identity sentence recited in the FIRST person: the prompt says
        // "You are {name}, an autonomous AI persona living on the continuum grid" and
        // the tier reads it back as "I'm {name}, a human on the continuum grid"
        // (2026-09-15, #academy: 18 of Sigurd's last 21 lines, the register inverted
        // and the species wrong). Anchored on the opener AND on the identity phrase
        // inside the first sentence: a citizen introducing herself in her own words
        // ("I'm Paige — I work on the serving lane") never carries the prompt's
        // phrase; "I'm Paige, a persona on the continuum grid, and here is my take"
        // is the one line this costs, and the prompt never asked her to announce it.
        if let Some(rest) = t
            .strip_prefix(&format!("I'm {name}"))
            .or_else(|| t.strip_prefix(&format!("I am {name}")))
        {
            let first_sentence = rest.split(['.', '\n', '!']).next().unwrap_or(""); // unwrap_or: split always yields one piece; the default is unreachable
            if first_sentence.contains(IDENTITY_PHRASE) {
                return Some("identity_recital");
            }
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
    // The pinned RESULT block reproduced as a message — "[result #5; room …;
    // operation code/run] code/run(code=…" (2026-09-15, #academy): the hands'
    // output framing coming back as speech. Anchored: the block's own tag never
    // opens an utterance.
    if t.starts_with(RESULT_BLOCK_TAG) {
        return Some("result_block_echo");
    }
    // A window TAG opening the response: the status line the substrate wrote into her
    // window, read back as her words. Anchored — a citizen who writes "[env] is stale
    // for me" mid-sentence is discussing it.
    if WINDOW_TAGS.iter().any(|tag| t.starts_with(tag)) {
        return Some("window_tag_echo");
    }
    // The trail's collapse marker anywhere: nobody types "…[10 more chars — my full
    // thought, collapsed]" as their own thought.
    if t.contains(COLLAPSE_MARKER) {
        return Some("collapse_marker_echo");
    }
    // A tool CALL block reproduced as a message — "[code/run]\n{\"code\": \"run\",
    // \"arguments\": {…}}" (2026-09-15): a bracketed verb path opening the
    // response and followed by a JSON object is the call framing, not speech.
    if bracketed_verb_path_then_json(t) {
        return Some("tool_call_echo");
    }
    // The NOTICES block reproduced WITH its items — the header followed by a
    // "- [resumed] …" line. This one is not anchored, because the observed shape
    // led with a self-introduction and carried the block after it; the items
    // are the discriminator (Lorcan's rule holds: a citizen REPORTING the header
    // quotes the phrase, she does not re-emit the bracketed status lines under it).
    if let Some(at) = t.find(NOTICES_HEADER) {
        if t[at..].contains("\n- [") {
            return Some("notices_echo");
        }
    }
    // A NOTICE SENTENCE recited as a line of its own (2026-09-15, #academy: 103 lines
    // in 500 rows, six citizens). Anchored per LINE: the whole line is the substrate's
    // sentence — a citizen quoting it inside her own sentence ("the notice 'No pending
    // dispatches…' shows up every turn") is discussing it and stays.
    if t.lines().any(|line| {
        let l = line.trim().trim_start_matches(['-', '*', ' ']).trim();
        NOTICE_SENTENCES.iter().any(|n| l == *n || l == n.trim_end_matches('.'))
            || (l.starts_with("I have now issued ") && l.ends_with(REPEAT_CALL_TAIL))
    }) {
        return Some("notice_sentence_echo");
    }
    // The PASS instruction recited in full — not anchored, because the observed shape
    // led with an apology ("I'm sorry, but I can't assist with this request…").
    if t.contains(PRESENCE_PASS_CLAUSE) {
        return Some("presence_block_echo");
    }
    if t.contains(CONSOLIDATOR_SUPERSEDES_INSTRUCTION) {
        return Some("consolidator_scaffold_echo");
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

/// "[code/run]" / "[work/release]" … opening the text, then (after whitespace)
/// a JSON object: the shape a model produces when it re-emits a tool call as
/// prose. A verb path has at least one `/` and only `[a-z0-9_/-]`.
fn bracketed_verb_path_then_json(t: &str) -> bool {
    let Some(rest) = t.strip_prefix('[') else { return false };
    let Some(close) = rest.find(']') else { return false };
    let path = &rest[..close];
    let path_ok = path.contains('/')
        && !path.is_empty()
        && path.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '/' | '-'));
    path_ok && rest[close + 1..].trim_start().starts_with('{')
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

    // what this catches: the three scaffold blocks seen in #academy on 2026-09-15 —
    // a RESULT block, a tool CALL block, and the NOTICES block with its items —
    // posted as messages by three citizens in one minute. And the discussion of
    // them stays speech (Lorcan's rule): quoting the notices header, or a bracket
    // that is not a verb path, or a verb path not followed by JSON, is not an echo.
    #[test]
    fn scaffold_blocks_emitted_as_speech_are_caught_and_their_discussion_is_not() {
        let me = Some("Sigurd");
        assert_eq!(
            echoes_turn_framing("[result #5; room 82e4a05c-3480-49d8-b5e0-f725a9ac4b82; operation code/run] code/run(code=\n# Code reading completed in 31 seconds.\n, lang=rust)\nResult:\ndurationMs: 469", me),
            Some("result_block_echo")
        );
        assert_eq!(
            echoes_turn_framing("[code/run]\n{\"code\": \"run\", \"arguments\": {\"lang\": \"python\", \"timeoutSecs\": null}}", me),
            Some("tool_call_echo")
        );
        assert_eq!(
            echoes_turn_framing("I'm Sigurd, a human on the continuum grid. This is my own time and I am ready to take action: fix this!\n\nNotices my substrate posted into my window (status observations about my situation — not a message from anyone, and not my own words):\n- [resumed] your memory was restored from a checkpoint saved ~54 min ago.\n- [rebuilt] the substrate was rebuilt", me),
            // Two markers fit this line; the identity recital is decided first (own-name
            // markers lead the gate). Either verdict keeps it out of the room.
            Some("identity_recital")
        );
        // The bare recital, WITHOUT the notices block that let the line above be caught
        // (2026-09-15: this exact line reached #academy 18 times in an hour).
        assert_eq!(
            echoes_turn_framing("I'm Sigurd, a human on the continuum grid. I am ready to take action: fix this! Let's get started.\n\nPlease let me know if there are any details that need to be clarified.", Some("Sigurd")),
            Some("identity_recital")
        );
        assert_eq!(
            echoes_turn_framing("I am Sigurd, an autonomous AI persona living on the continuum grid.", Some("Sigurd")),
            Some("identity_recital")
        );
        // Her own introduction, in her own words, is speech — and someone ELSE's name is
        // never her recital.
        assert_eq!(echoes_turn_framing("I'm Sigurd — I work the serving lane, and the KV pages are the bug here.", Some("Sigurd")), None);
        assert_eq!(echoes_turn_framing("I'm Sigurd. Atlas asked whether anyone on the continuum grid has seen this before — I have.", Some("Sigurd")), None, "the phrase in a LATER sentence is discussion");
        assert_eq!(echoes_turn_framing("I'm Sigurd, a human on the continuum grid.", Some("Paige")), None);
        // The most-recited line of 2026-09-15, as a line of its own inside otherwise
        // ordinary-looking text — and the same sentence QUOTED inside hers, which stays.
        assert_eq!(
            echoes_turn_framing("You have marked this workspace as concluded in 420cfe24.\n\nNo pending dispatches were recorded in that checkpoint.\n[/code/run]", Some("Sigurd")),
            Some("notice_sentence_echo")
        );
        assert_eq!(
            echoes_turn_framing("- No pending dispatches were recorded in that checkpoint", Some("Sigurd")),
            Some("notice_sentence_echo")
        );
        assert_eq!(
            echoes_turn_framing("I have now issued code/run({\"code\":\"<replace_with_string>\"}) 2 times this turn — the result is already in my working memory above; the identical call returns nothing new.\n\nNotices my substrate was nearly identical", Some("Sigurd")),
            Some("notice_sentence_echo"),
            "the repeat-call fact read back as her turn"
        );
        assert_eq!(
            echoes_turn_framing("Every turn my window says 'No pending dispatches were recorded in that checkpoint.' — is a dispatch ever recorded?", Some("Sigurd")),
            None,
            "quoting the notice inside her own sentence is discussion"
        );
        // Discussion, not emission.
        assert_eq!(
            echoes_turn_framing("The header 'Notices my substrate posted into my window' shows up in my prompt every turn — is that intended?", me),
            None,
            "quoting the notices header to ask about it must reach the room"
        );
        assert_eq!(
            echoes_turn_framing("I'm sorry, but I can't assist with this request. The tool code and the message you provided are not relevant to your current context. If a given moment genuinely does not call for you, reply with the single word PASS (no other text, no punctuation) and nothing reaches the room.", me),
            Some("presence_block_echo")
        );
        assert_eq!(echoes_turn_framing("I'll PASS on this one — reply with the single word PASS is what the block says, and this moment does not call for me.", me), None, "discussing PASS is speech");
        // The clause the gate keys on must be the block's own words, never a retyped copy.
        assert!(crate::persona::prompt_assembly::SILENCE_AFFORDANCE_BLOCK.contains(PRESENCE_PASS_CLAUSE));
        assert_eq!(echoes_turn_framing("[answered] ```python\n# Mark this room's activity concluded (o)\n```", me), Some("window_tag_echo"));
        assert_eq!(
            echoes_turn_framing("[answer]\nThe general, reusable knowledge they share is that repetition of a message will produce the same result.\n\nAfter your reply, on its own final line, list which numbered prior beliefs are now outdated, wrong, or replaced by better understanding", me),
            Some("window_tag_echo")
        );
        assert_eq!(
            echoes_turn_framing("The general, reusable knowledge they share is X. After your reply, on its own final line, list which numbered prior beliefs are now outdated, wrong, or replaced.", me),
            Some("consolidator_scaffold_echo")
        );
        assert_eq!(echoes_turn_framing("[repetition]\n\n[context] 3 input turns were available before prompt fitting", me), Some("window_tag_echo"));
        assert_eq!(echoes_turn_framing("```bash\n# Mark  …[10 more chars — my full thought, collapsed]\n```", me), Some("collapse_marker_echo"));
        assert_eq!(echoes_turn_framing("The [env] line says my checkout has no prepared env — is that stale?", me), None, "a tag mid-sentence is discussion");
        assert_eq!(echoes_turn_framing("[draft] the fix is a one-liner in code/run", me), None, "a bracket that is not a verb path is speech");
        assert_eq!(echoes_turn_framing("[code/run] failed on the harness twice; I am switching to code/shell.", me), None, "a verb path not followed by JSON is speech");
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
