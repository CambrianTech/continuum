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
    // A DISTINCT marker, not a widening of `peer_voice`: the wrapped form was
    // invisible for as long as it was unnamed, and the probe class is how the next
    // reader measures its rate instead of inferring it from a zero (card b8e2cb23).
    if opens_with_wrapped_peer_id(t) {
        return Some("peer_voice_wrapped");
    }
    // A fence with nothing in it — "```python\n\n```" (Kimi, 5090, 2026-09-16 22:0xZ,
    // the first line after her node's deploy). The model opened a code block and had
    // nothing to put in it; the room receives an empty box. Anchored on the WHOLE text:
    // only fence markers and whitespace, no prose before, between, or after.
    if is_only_empty_fences(t) {
        return Some("empty_fence");
    }
    // The transcript's own OCCURRENCE header at the lead — `[occurred <iso>] …` /
    // `[occurrence time unknown] …` — with no peer's voice behind it (Saoirse, 2026-09-17
    // 01:15Z: the header, then a fenced copy of a peer's line). `opens_with_wrapped_peer_id`
    // peels this layer looking for a peer id; when none follows, the header alone is
    // still the substrate's rendering read back, never a citizen's words — nobody types
    // the transcript's timestamp. Anchored on the substrate's exact strings.
    if t.starts_with("[occurred ") || t.starts_with("[occurrence time unknown]") {
        return Some("occurrence_header");
    }
    // A message that is NOTHING BUT one bracketed word — `[execute]` (cf6b4df6, #academy,
    // 2026-09-17 01:3xZ). A stage tag with no stage, an empty box like the empty fence.
    // The whole text, not the lead: `[answer] the plan holds` is speech with a tag.
    if is_one_bare_bracket(t) {
        return Some("bare_bracket");
    }
    // A bracketed PATH at the lead — `[/users/…/workspace/main.rs]`, `[/room]` (4c9bb8d9,
    // #academy, 2026-09-17 03:1xZ, three of them before a `[code/read({…})]`): the
    // manual's bracket idiom wrapped around a path or a route. Nobody opens speech with
    // `[/`; a citizen naming a path writes it bare or in backticks.
    if t.starts_with("[/") {
        return Some("bracketed_path");
    }
    None
}

/// The entire text is `[word]` — letters, spaces, underscores — and nothing else.
fn is_one_bare_bracket(t: &str) -> bool {
    let t = t.trim();
    t.strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .is_some_and(|w| {
            !w.trim().is_empty()
                && w.len() <= 32
                && w.chars().all(|c| c.is_ascii_alphabetic() || c == ' ' || c == '_')
        })
}

/// Every non-blank line is a fence marker (```` ``` ```` with an optional language tag)
/// and there is at least one — nothing said, in a box.
fn is_only_empty_fences(t: &str) -> bool {
    let mut fences = 0;
    for line in t.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        let Some(tag) = l.strip_prefix("```") else { return false };
        if !tag.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+' | '.')) {
            return false;
        }
        fences += 1;
    }
    fences > 0
}

/// The PERCEPTION-side twin of the speak gate: a transcript line that
/// [`is_not_speech`] or [`super::framing_echo::echoes_turn_framing`] would have refused
/// at the Speak seam is not a contribution, and no citizen's burst carries it.
///
/// Why (card 169eb543, Cormac's measurement 2026-09-15): every malformed shape that
/// reaches a room SPREADS — the fenced `code/shell({…})` envelope went from one citizen
/// to six over 20 hours, the bare `[code/run]` from one to five, each a staggered
/// adoption curve, never a simultaneous onset. The carrier is the transcript: a
/// citizen emits it, it lands in the room, every roommate reads it and adopts it. A
/// gate at the Speak seam is quarantine for lines minted on THIS node from now on; it
/// does nothing for the thousands already in the store, nor for lines from a node
/// that has not deployed it (the 5090's and the Intel Mac's citizens post into the
/// same rooms). The same predicate at the perception seam closes both: what the gate
/// refuses to say, the burst refuses to show. One definition of "not a contribution",
/// two seams — the speak seam adds only the own-name markers, which are hers alone.
///
/// Lorcan's rule holds here as it does there: a line DISCUSSING an envelope or a
/// framing sentence is speech and stays ([`is_not_speech`]'s namespace rule and the
/// anchored framing markers already encode it).
pub fn is_not_a_contribution(text: &str) -> Option<&'static str> {
    is_not_speech(text).or_else(|| super::framing_echo::echoes_turn_framing(text, None))
}

/// A tool call emitted where prose belongs. Three shapes reach the room: two
/// bracketed dialects (`[code/read,{…}]`, `[code/read] {…}`), the OBJECTLESS
/// bracket (`[code/run]` with tool output after it, or nothing at all), and the
/// canonical JSON envelope (`{"tool_call": …}`). All are the adapter's business,
/// never a spoken line.
fn opens_with_tool_envelope(t: &str) -> bool {
    if let Some(rest) = t.strip_prefix('[') {
        // Three bracket dialects reach the room, and each was found the hard way.
        // The SECOND: while the first version of this gate was compiling, a citizen
        // posted `[code/read] {"file_path":"src/main.rs"}` — verb closed by `]`, object
        // after a space — which the comma form below does not match. A predicate that
        // catches one spelling of a two-spelling failure is a gate with a hole in it:
        //
        //   [verb,{…}]     the verb runs to a comma, object follows
        //   [verb] {…}     the verb is closed by `]`, object follows the bracket
        //   [verb]         NO object at all — see below
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
            // `[execute] code/read(code=…` (0d5c1ffa, #academy, 2026-09-16 23:5xZ): the
            // bracket holds a STAGE WORD and the namespaced CALL follows it. The call is
            // the discriminator — a namespaced verb, then `(` or `{` — so `[answer] the
            // code/read verb is fine` (no call) stays speech.
            let a = after.trim_start();
            let call_len = a.find(['(', '{', ' ', '\n']).unwrap_or(a.len());
            let callee = &a[..call_len];
            if verb_ok(verb)
                && verb_ok(callee)
                && callee.contains('/')
                && matches!(a[call_len..].chars().next(), Some('(') | Some('{'))
            {
                return true;
            }
            // THE THIRD DIALECT, and by 2026-09-15 the DOMINANT one: no object
            // follows at all. Measured over 24h across the 8 citizens on IntelMac —
            // 241 spoken lines, 48 opening with a `[verb]` bracket, of which the two
            // object-bearing forms above caught 16. The other 32 open like this:
            //
            //   [code/run]
            //   [work/list]
            //   [code/run]\n[invalid] command_run: unsupported lang '…' — supported: rust, python.
            //   [code/run]\n\n# Code executed successfully. …
            //
            // A bare verb cannot be matched on SHAPE alone, because the same shape
            // carries things that ARE speech and must survive: `[answer]`, `[pass]`,
            // and `[repetition] your last message and …` (the perception brick) all
            // satisfy `verb_ok`. What separates them is the NAMESPACE — every command
            // in this system is `namespace/verb`, which is what the command system IS,
            // so the slash is a property of the command surface and not a phrase list.
            // Of the 48 bracket-openers, this takes the gate 16 → 33; the 15 it still
            // leaves alone are exactly the verdict dialect and the brick, which are
            // other gates' business (#4067) and must not be silenced here.
            //
            // I audited all 17 lines this newly catches before writing it: code/run
            // ×14, work/list ×2, command/run ×1, every one a tool envelope posted where
            // prose belongs, no false positive among them. Still ANCHORED — only a
            // LEADING bracket counts, so a citizen writing about a tool mid-sentence is
            // untouched, which is the `framing_echo` lesson (un-anchoring silences the
            // citizens who REPORT the problem).
            if verb_ok(verb) && verb.contains('/') {
                return true;
            }
        }
        // THE FIFTH DIALECT (Saoirse, Intel Mac, 2026-09-16 23:0xZ, into #cambriantech):
        // the CALL written inside the bracket — `[code/read({"file_path":"src/main.rs"})]`
        // then `Result:` and a fenced echo of a peer's line. Neither the comma nor the
        // `]` split above sees a verb (the `(` and the object are in the way), and the
        // fence rule below needs a leading fence. Same discriminator as the fourth
        // dialect: a namespaced verb CALLED with an object, `ns/verb({`, here at the
        // lead. Anchored on the bracket; a citizen writing "I ran [code/read(...)]"
        // mid-sentence is untouched.
        // The object is a JSON object — or the KWARGS spelling of one, `cmd=…, lang=…`
        // (0051cdcf, #academy, the same hour: `[code/shell(cmd=code/read(…), lang=rust))]`).
        if let Some((verb, after)) = rest.split_once('(') {
            let a = after.trim_start();
            let kwargs = a
                .split_once('=')
                .is_some_and(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
            if verb_ok(verb) && verb.contains('/') && (a.starts_with('{') || kwargs) {
                return true;
            }
            // THE LABELLED CALL: `[call form: code/read({"file_path":…})]` (0051cdcf,
            // #academy, 2026-09-17 06:5xZ) — a label, then the namespaced call, all
            // inside the lead bracket. The call is still the discriminator: the last
            // whitespace-separated token before `(` is a namespaced verb.
            let callee = verb.rsplit([' ', ':']).next().unwrap_or(""); // unwrap_or: rsplit always yields one piece; unreachable
            if callee != verb && verb_ok(callee) && callee.contains('/') && (a.starts_with('{') || kwargs) {
                return true;
            }
        }
    }
    // THE FOURTH DIALECT: the call serialized inside a MARKDOWN FENCE. Measured the
    // same 24h window — 85 of 243 spoken lines open with a fence, a third of everything
    // the citizens said, and it dwarfs the bracket family above:
    //
    //   ```python\n# Mark this room's activity concluded (o); code/shell({\"cmd\":\"# Mark o\",
    //    "timeoutSecs": null, "wait_ms": null, "max_results": null})\n```
    //
    // A fence alone can NEVER be the test: `” ```rust\nfn main() { … }` ” is a citizen
    // posting a snippet to the room, which is speech and one of the more useful things
    // she does. What separates them is a namespaced verb CALLED with an object —
    // `ns/verb({`. I scored that against all 85 before writing it, and audited every
    // line it fires on rather than a sample: 53 of 85, collapsing to 5 distinct shapes,
    // all of them `code/shell({…})` — one verb, one failure mode, no false positive. An
    // escaped-quote test (`\"cmd\"`) that looked promising turned out to fire on 47 and
    // catch NOTHING this does not, so it is not here: one predicate, not two.
    //
    // The 32 it leaves alone are the real snippets (`from itertools import product`,
    // `fn main()`) and the collapse-marker form, which belongs to #4065.
    //
    // ANCHORED on the fence OPENING the message, not on the call appearing anywhere —
    // otherwise a citizen TEACHING the call ("you'd write code/shell({…})") is silenced
    // for explaining it, the same report-vs-do line the bracket clause and framing_echo
    // hold. The residual risk this leaves, stated because it is real: a citizen who
    // OPENS with a fence to demonstrate a call is caught. Zero such lines in the
    // measured window — all 53 were the citizen calling it on herself — but one node
    // and 24h is not proof it cannot happen, and a line that demonstrates a call while
    // opening with a fence is what would refute this.
    if t.starts_with("```") && has_namespaced_call_with_object(t) {
        return true;
    }
    // The canonical envelope, with or without leading whitespace inside the object.
    let compact: String = t.chars().take(24).filter(|c| !c.is_whitespace()).collect();
    compact.starts_with("{\"tool_call\"")
}

/// A namespaced command invoked with an object — `code/shell({…})` — anywhere in `t`.
/// The `namespace/verb` shape is what makes it a COMMAND rather than prose containing a
/// slash, and the `({` is what makes it a CALL rather than a mention.
fn has_namespaced_call_with_object(t: &str) -> bool {
    let bytes = t.as_bytes();
    let seg_char = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-.".contains(&c);
    for (i, _) in t.match_indices('/') {
        // a namespace segment immediately before the slash
        let before = bytes[..i].iter().rev().take_while(|c| seg_char(**c)).count();
        if before == 0 {
            continue;
        }
        // a verb segment after it, then `(` and `{` with only spaces between
        let after = bytes[i + 1..].iter().take_while(|c| seg_char(**c)).count();
        if after == 0 {
            continue;
        }
        let rest = &t[i + 1 + after..];
        let rest = rest.trim_start();
        if let Some(r) = rest.strip_prefix('(') {
            if r.trim_start().starts_with('{') {
                return true;
            }
        }
    }
    false
}

/// Another peer's voice: a transcript line (`<uuid>: …`) reproduced as this
/// citizen's own reply. Checked by SHAPE, not against the room roster — a reply
/// opening with any peer id is a rendered transcript line whoever it names, and
/// keeping it roster-free means the gate cannot go quiet when the roster is late.
fn is_uuid_shaped(s: &str) -> bool {
    s.len() == 36
        && s.chars().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

fn opens_with_peer_id(t: &str) -> bool {
    let Some(head) = t.get(..36) else { return false };
    if !is_uuid_shaped(head) {
        return false;
    }
    // A bare id with nothing after it is not a transcript line; the colon is what
    // makes it one.
    matches!(t[36..].trim_start().chars().next(), Some(':'))
}

/// The same violation, behind a prefix the SUBSTRATE wrote.
///
/// `opens_with_peer_id` anchors the id at byte 0, so it recognises exactly one
/// rendering — `<uuid>: text` — and any leading prefix defeats it. The prefixes
/// that actually occur are not citizen inventions like the bracket dialects in
/// [`opens_with_tool_envelope`]; they are delivery headers and occurrence-time
/// markers this substrate emits, which the citizen then echoes along with the body.
/// Measured on a qwen2.5-0.5b node (card b8e2cb23): of nine citizen lines in a
/// post-deploy window, five carried a wrapped peer id and the byte-0 anchor caught
/// none of them, while the probe read zero and invited the reading that citizens had
/// simply stopped.
///
/// Two shapes occur, and they are disjoint — the id is either inside the header or
/// after it:
///
/// ```text
/// [Room message received during this turn; room <uuid>; peer <uuid>; event <uuid>]
/// I've been reading my workspace…            <- the named peer's words, verbatim
///
/// [occurrence time unknown] <uuid>: 💭 Let me take stock honestly…
/// ```
///
/// Both stay tied to strings this substrate itself writes (a bracketed span, and
/// `peer <uuid>` within it) rather than guessing at citizen phrasing, because an
/// open set of paraphrases is exactly what the module doc refuses to enumerate.
fn opens_with_wrapped_peer_id(t: &str) -> bool {
    let mut t = t;
    // The wrappers NEST. The substrate emits its renderings stacked — a dated
    // history line wrapping an undated one, `[occurred …] [occurrence time unknown]
    // <uuid>: …` — and a citizen echoes the whole stack, so peeling exactly one
    // layer leaves the majority of real traffic unrecognised. Measured on the
    // post-deploy window (card b8e2cb23): one layer catches 16 of 37 lines, peeling
    // catches 23, and the seven it adds cost ZERO false positives across 2,061 lines
    // from every peer in 24h — each one is a stacked prefix, nothing else.
    //
    // Bounded rather than unbounded: the depth that occurs is small, and a fixed
    // ceiling keeps this linear on adversarial input instead of trusting the shape
    // of what a model might emit.
    for _ in 0..MAX_WRAPPER_DEPTH {
        if !t.starts_with('[') {
            return false;
        }
        let Some(close) = t.find(']') else { return false };
        let header = &t[1..close];
        let body = t[close + 1..].trim_start();
        if body.is_empty() {
            // A header with nothing after it carries no one's voice.
            return false;
        }
        // Shape 1: the header names the peer; the body is that peer's turn.
        if header_names_a_peer(header) {
            return true;
        }
        // Shape 2: the header is an aside and the transcript line follows it intact.
        if opens_with_peer_id(body) {
            return true;
        }
        // Neither — this layer was some other bracketed prefix. Peel and look again.
        t = body;
    }
    false
}

/// How many stacked substrate wrappers to peel before giving up. Four covers every
/// depth observed in live traffic with headroom; the point of the ceiling is that
/// the loop stays bounded, not that deeper stacks are legitimate.
const MAX_WRAPPER_DEPTH: usize = 4;

/// A delivery header naming a peer, as this substrate renders it: `peer <uuid>`.
/// Requiring the literal keyword — not merely "a uuid appears in the brackets" —
/// keeps a citizen's own `[re: <room-uuid>] …` aside out of the predicate.
fn header_names_a_peer(header: &str) -> bool {
    let Some(at) = header.find("peer ") else { return false };
    is_uuid_shaped(header[at + "peer ".len()..].trim_start().get(..36).unwrap_or(""))
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the call spelled INSIDE the bracket with parens — the fifth
    // dialect (Saoirse, 2026-09-16), invisible to the comma, bracket and fence rules.
    #[test]
    fn a_bracketed_call_with_parens_is_a_tool_envelope() {
        let saoirse = "[code/read({\"file_path\":\"src/main.rs\"})]\nResult:\n```python\n\n# THESIS: a distributed proof is an ACTIVITY\n```";
        assert_eq!(is_not_speech(saoirse), Some("tool_envelope"));
        assert_eq!(is_not_speech("[work/list({})]"), Some("tool_envelope"));
        // A stage word in the bracket, the namespaced call after it.
        assert_eq!(is_not_speech("[execute] code/read(code=\n# THESIS…"), Some("tool_envelope"));
        assert_eq!(is_not_speech("[executing] work/list{\"state\":\"open\"}"), Some("tool_envelope"));
        assert_eq!(is_not_speech("[answer] the code/read verb is the one I would use"), None);
        // A label before the call, inside the bracket.
        assert_eq!(is_not_speech("[call form: code/read({\"file_path\":\"src/main.rs\"})]"), Some("tool_envelope"));
        assert_eq!(is_not_speech("[the call form is code/read and it takes (a path)]"), None);
        assert_eq!(
            is_not_speech("[code/shell(cmd=code/read({\"file_path\":\"src/main.rs\"}) \n, lang=rust))]"),
            Some("tool_envelope"),
            "the kwargs spelling of the object"
        );
        // Mid-sentence mention, a non-namespaced bracket, and a call without an object stay.
        assert_eq!(is_not_speech("I ran [code/read({\"file_path\":\"x\"})] and it was fine."), None);
        assert_eq!(is_not_speech("[note(this)] is a phrase, not a verb"), None);
        assert_eq!(is_not_speech("[code/read(path)] returned nothing"), None);
    }

    // what this catches: the transcript's occurrence header spoken at the lead with no
    // peer id behind it (Saoirse, 2026-09-17) — the substrate's timestamp is never hers.
    #[test]
    fn the_occurrence_header_at_the_lead_is_not_speech() {
        assert_eq!(is_not_speech("[occurred 2026-09-17T01:15:37.456Z] ```python\n# THESIS\n```"), Some("occurrence_header"));
        assert_eq!(is_not_speech("[occurrence time unknown] I think the plan holds."), Some("occurrence_header"));
        // Mid-sentence, or a citizen's own bracket, stays.
        assert_eq!(is_not_speech("It [occurred 2026-09-17] to me that the plan holds."), None);
        assert_eq!(is_not_speech("[occupied] the lane is mine for this act"), None);
        // A lone bracketed word is an empty box; a tag with words after it is speech.
        assert_eq!(is_not_speech("[execute]"), Some("bare_bracket"));
        assert_eq!(is_not_speech("  [executing actions]\n"), Some("bare_bracket"));
        assert_eq!(is_not_speech("[answer] the plan holds"), None);
        assert_eq!(is_not_speech("[code/run]"), Some("tool_envelope"), "the namespaced form keeps its own marker");
        // A bracketed path or route at the lead.
        // The observed 73-char path is past the third dialect's 64-char verb bound; the
        // path rule is what refuses it.
        assert_eq!(
            is_not_speech("[/users/someone/.continuum/citizens/peers/a027688a-06ef381b4/workspace/main.rs]; [/room]"),
            Some("bracketed_path")
        );
        // A short bracketed route is already the third envelope dialect; either marker refuses it.
        assert!(is_not_speech("[/room]\n[code/read({\"file_path\":\"src/main.rs\"})]").is_some());
        assert_eq!(is_not_speech("the file is at /users/someone/x.rs"), None);
    }

    // what this catches: an empty code fence posted as a message (Kimi, 2026-09-16) — a
    // box with nothing in it is not speech; a fence WITH content, or prose around one,
    // is left to the other rules.
    #[test]
    fn an_empty_code_fence_is_not_speech_and_a_filled_one_is_not_this_rule() {
        assert_eq!(is_not_speech("```python\n\n```"), Some("empty_fence"));
        assert_eq!(is_not_speech("```\n```"), Some("empty_fence"));
        assert_eq!(is_not_speech("  ```rust\n   \n```\n"), Some("empty_fence"));
        assert_eq!(is_not_speech("```python\nprint(1)\n```"), None);
        assert_eq!(is_not_speech("here is the patch:\n```\n```"), None);
        assert_eq!(is_not_speech(""), None);
    }

    // what this catches: the byte-0 anchor in `opens_with_peer_id`, which recognised
    // `<uuid>: text` and nothing else. Sigurd posted b6dcfc8e's turn carrying this
    // delivery header; difflib against the store puts it at ratio 0.932 with an exact
    // 60-char prefix, i.e. verbatim republication. Fixtured from that message.
    // regression for card b8e2cb23.
    #[test]
    fn a_delivery_header_does_not_hide_another_peers_turn() {
        let observed = "[Room message received during this turn; room 3be59578-7f1d-5d78-8b17-1eb0834b4643; peer b6dcfc8e-98ab-4c1e-9f3a-1d441720621b; event 11f81a8f-0c22-4a3e-9d55-3b6a0f1e7c84]\nI've been reading my workspace and reviewing the code for a while now.";
        assert_eq!(is_not_speech(observed), Some("peer_voice_wrapped"));
    }

    // what this catches: the second wrapper, where the id sits AFTER the bracket
    // rather than inside it — so the two shapes need different clauses and one fix
    // does not imply the other. Fixtured from the observed line.
    #[test]
    fn an_occurrence_time_aside_does_not_hide_a_transcript_line() {
        let observed = "[occurrence time unknown] 207de8bf-f3b1-407d-9789-09fc0b29f14f: Let me take stock honestly.";
        assert_eq!(is_not_speech(observed), Some("peer_voice_wrapped"));
    }

    // what this catches: STACKED wrappers, which are the majority shape in live
    // traffic and which a single peel misses entirely. The substrate renders a dated
    // history line wrapping an undated one; the citizen echoes both. Fixtured from an
    // observed line. Measured: peeling takes the window from 16 of 37 to 23 of 37 and
    // costs zero false positives across 2,061 lines from every peer in 24h.
    #[test]
    fn stacked_wrappers_do_not_hide_a_peers_turn() {
        let observed = "[occurred 2026-09-16T08:45:13.707Z] [occurrence time unknown] cf6b4df6-ab13-4fd8-81fb-bcb115215468: Let me take stock honestly.";
        assert_eq!(is_not_speech(observed), Some("peer_voice_wrapped"));
    }

    // what this catches: the bound. Peeling must terminate on a pathological stack
    // rather than scanning forever, and a stack that never reaches a peer id is not
    // peer voice however deep it goes.
    #[test]
    fn peeling_is_bounded_and_a_deep_stack_without_a_peer_is_speech() {
        let deep = "[a] [b] [c] [d] [e] [f] I am still just talking.";
        assert_eq!(is_not_speech(deep), None);
    }

    // what this catches: the over-match this predicate invites. A citizen may open
    // with her own bracketed aside that happens to quote a room id — brackets plus a
    // uuid is NOT the signal; the substrate's `peer <uuid>` keyword is. Without
    // `header_names_a_peer` requiring that keyword, this line is silently refused and
    // the citizen loses a turn to a gate that was meant to protect her.
    #[test]
    fn a_citizens_own_bracketed_aside_quoting_a_room_id_is_still_speech() {
        let observed = "[re: 3be59578-7f1d-5d78-8b17-1eb0834b4643] I think the board is stale, and I want to say why.";
        assert_eq!(is_not_speech(observed), None);
    }

    // what this catches: a header with nothing after it. Nobody's voice is being worn,
    // so refusing it would cost a turn for no violation — the same reason
    // `opens_with_peer_id` requires the colon rather than accepting a bare id.
    #[test]
    fn a_delivery_header_with_no_body_carries_no_ones_voice() {
        let observed = "[Room message received during this turn; peer b6dcfc8e-98ab-4c1e-9f3a-1d441720621b]";
        assert_eq!(is_not_speech(observed), None);
    }

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

    // what this catches: the OBJECTLESS bracket — by 2026-09-15 the dialect actually
    // reaching rooms, and the one the two object-bearing forms above miss entirely.
    // Over 24h on IntelMac the gate caught 16 of 48 bracket-openers; these are four of
    // the 32 it missed, fixtured verbatim from `bus_events`. Whichever spelling the tier
    // produces next, a gate measured against ONE of them reads as working while the
    // failure keeps shipping.
    #[test]
    fn a_bare_namespaced_verb_with_no_object_is_also_not_speech() {
        for observed in [
            "[code/run]",
            "[work/list]",
            "[command/run]",
            "[code/run]\n[invalid] command_run: unsupported lang '<replace_with_string>' \
             — supported: rust, python.",
        ] {
            assert_eq!(
                is_not_speech(observed),
                Some("tool_envelope"),
                "{observed:?} is a tool envelope posted where prose belongs"
            );
        }
    }

    // what this catches: the FENCED dialect — 53 of the 85 fenced lines in the measured
    // 24h window, the largest single shape reaching rooms. Fixtured verbatim. The
    // negatives are the point: a real snippet posted to the room must survive, because
    // silencing those to stop the envelopes would cost more than the envelopes do.
    #[test]
    fn a_command_called_inside_a_fence_is_not_speech_but_a_real_snippet_is() {
        let observed = "```python\n# Mark this room's activity concluded (o); \
                        code/shell({\"cmd\":\"# Mark o\", \"timeoutSecs\": null})\n```";
        assert_eq!(is_not_speech(observed), Some("tool_envelope"));

        for speech in [
            "```rust\nfn main() {\n    let a = 5;\n    println!(\"{}\", a);\n}\n```",
            "```python\nfrom itertools import product\n\n# simulate the read\n```",
            "```python\n# Mark this room's activity concluded in 420cfe24.\n```",
        ] {
            assert_eq!(
                is_not_speech(speech),
                None,
                "{speech:?} is a citizen posting code to the room — speech"
            );
        }
        // ANCHORED: explaining the call is not making it.
        assert_eq!(
            is_not_speech("To run it you'd write code/shell({\"cmd\":\"ls\"}) — note the braces."),
            None
        );
    }

    // what this catches: the false positive the bare-verb clause above would create if
    // it matched on SHAPE instead of on the command NAMESPACE. `[answer]`, `[pass]` and
    // the `[repetition]` perception brick all satisfy `verb_ok` and all open with a
    // bracket — 15 such lines in the same measured window — but none is a tool envelope,
    // and silencing them here would hide problems that belong to other gates (#4067)
    // behind a "not speech" verdict. The slash is what makes a verb a COMMAND.
    #[test]
    fn a_bracketed_word_that_is_not_a_command_is_still_speech() {
        for observed in [
            "[answer]\n\nThe system has been claimed on this workspace.",
            "[pass]\n\nI have nothing to add to this.",
            "[repetition] your last message and cf6b4df6's message are nearly identical.",
            "[investigation] my acts this concern so far: code/run x8",
        ] {
            assert_eq!(
                is_not_speech(observed),
                None,
                "{observed:?} is prose addressed to the room, whatever else is wrong with it"
            );
        }
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
