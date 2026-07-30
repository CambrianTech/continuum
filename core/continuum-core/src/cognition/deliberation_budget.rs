//! Fitting the deliberation prompt into the served context window.
//!
//! Pure token-accounting + message-line rendering, lifted out of
//! [`super::llm_deliberation_faculty`] so the faculty owns *deciding what to send*
//! and this owns *measuring and trimming it to fit*. All functions are pure and
//! independently testable — the window guard is load-bearing (an over-budget prompt
//! is a hard `n_ctx` overshoot → a 400 from llama-server that mutes the persona for
//! the whole tick), so it earns its own home + tests.

use super::workspace::BurstTurn;

/// Chars-per-token divisor for the conservative window estimate. English is ~4
/// chars/token, but the deliberation prompt carries UUID-dense rosters, structured
/// engram observations, and code, which tokenize far denser — so we OVER-count
/// tokens (divide by 3, not 4) to stay safely under `n_ctx`. The completion reserve
/// absorbs the remaining slack.
pub(super) const GUARD_CHARS_PER_TOKEN: usize = 3;

/// Conservative token estimate for the window guard (see [`GUARD_CHARS_PER_TOKEN`]).
pub(super) fn est_tokens(s: &str) -> usize {
    s.len() / GUARD_CHARS_PER_TOKEN
}

/// Keep the TAIL of `s` that fits `budget_tokens`, cutting at a line boundary so a
/// trimmed message starts on a clean line (never mid-line). The latest lines — the
/// turn's most recent activity — always survive; the head is what gets dropped. Used
/// by `messages_within` to trim the single message that straddles the served-window
/// budget.
pub(super) fn tail_to_tokens(s: &str, budget_tokens: usize) -> String {
    let budget_chars = budget_tokens.saturating_mul(GUARD_CHARS_PER_TOKEN);
    if s.len() <= budget_chars {
        return s.to_string();
    }
    let mut start = s.len().saturating_sub(budget_chars);
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    let slice = &s[start..];
    match slice.find('\n') {
        Some(nl) => slice[nl + 1..].to_string(),
        None => slice.to_string(),
    }
}

/// Render ONE burst turn as the body line for its chat message. The persona's own
/// turns and opaque (authorless) turns render verbatim — her own voice carries no
/// name prefix (the system prompt forbids self-prefixing), and an opaque burst is
/// reproduced byte-for-byte so the eval/test/replay paths are unchanged. A peer's
/// turn is prefixed `{author}: ` so several speakers stay distinguishable inside a
/// merged `user` message — and when the message's first line carries a vocative
/// naming another participant, the prefix carries the addressee too:
/// `Asha (to Anwen): …` / `Anwen (to you): …`. The ONE place message-line
/// formatting lives.
///
/// Why the addressee annotation exists (glass-boxed 2026-07-10): Asha asked
/// "Sure, Anwen. Could you please post your current implementation…" — the
/// addressing lived only in prose, and Atlas (whose turn fired next) answered AS
/// the implementer, presenting code as "my current implementation". His own
/// history then testified he held Anwen's role, and the confusion compounded into
/// third-person self-address. Prose never changes behavior; structure does — so
/// WHO a message is for becomes part of the rendered structure, by pure vocative
/// geometry against known participant names, never content NLP
/// ([[no-hardcoded-heuristics-to-steer-cognition]] — this renders a fact visible,
/// it steers nothing).
pub(super) fn turn_message_line(turn: &BurstTurn) -> String {
    if turn.is_self || turn.author.is_empty() {
        turn.content.clone()
    } else {
        format!("{}: {}", turn.author, turn.content)
    }
}

/// [`turn_message_line`] with addressee annotation for peer turns. `participants`
/// is every display name known in the window (peers + self); `self_name` is THIS
/// persona's name, rendered as "you" so a directed ask reads as directed.
pub(super) fn turn_message_line_addressed(
    turn: &BurstTurn,
    participants: &[String],
    self_name: &str,
) -> String {
    if turn.is_self || turn.author.is_empty() {
        return turn.content.clone();
    }
    // Vocatives naming someone OTHER than the speaker → annotate. (A vocative
    // matching the author is a self-reference/appositive — "I'm Anwen, the one
    // who claimed the card" — never an addressee.) Multi-addressee messages
    // ("Atlas, please test… Asha, could you…") render every addressee, in
    // discovery order, self as "you".
    let addrs: Vec<&str> = vocative_addressees(&turn.content, participants)
        .into_iter()
        .filter(|a| !a.eq_ignore_ascii_case(&turn.author))
        .collect();
    if addrs.is_empty() {
        return format!("{}: {}", turn.author, turn.content);
    }
    let rendered: Vec<&str> = addrs
        .iter()
        .map(|a| {
            if a.eq_ignore_ascii_case(self_name) {
                "you"
            } else {
                *a
            }
        })
        .collect();
    format!(
        "{} (to {}): {}",
        turn.author,
        rendered.join(", "),
        turn.content
    )
}

/// Pairwise similarity above which two of the persona's OWN consecutive messages
/// count as near-identical (unigram-token Jaccard). CALIBRATED, not hand-picked
/// (2026-07-11, three personas' full capture corpora, 9,860 consecutive pairs):
/// healthy conversation medians 0.22–0.35; loop pairs mass at ≥0.7; 0.6 splits
/// the ambiguous band. Template-family variants (~0.5) slip through v1 —
/// documented limitation, better under- than over-fire
/// ([[no-hardcoded-heuristics-to-steer-cognition]]).
pub(super) const NEAR_DUP_JACCARD: f64 = 0.6;

/// Minimum run length (consecutive near-identical PAIRS, so `3` = four messages
/// in a row) before the repetition fact renders. Same calibration: the longest
/// observed live runs were 36 and 80 messages; under healthy flow three
/// consecutive ≥0.6 pairs is vanishingly rare. Evidence-scaled: below this, say
/// nothing.
const NEAR_DUP_MIN_RUN: usize = 3;

/// How many of her own recent utterances the spoken ring retains — the
/// repetition detector's self-history window. Sized past NEAR_DUP_MIN_RUN
/// with slack for interleaved non-loop turns; utterances are short-lived
/// evidence, not memory (the hippocampus owns memory).
const OWN_SPEECH_RING: usize = 8;

/// Process-global per-persona ring of recent OWN utterances, keyed by the
/// canonical PeerId (persona_id == peer_id post-collapse). Written at the say
/// seam (service_loop, after a successful publish — only REAL utterances),
/// read by the deliberation faculty when rendering the repetition fact. Same
/// process-global registry pattern as `channel_substrate` — the seam between
/// the speaking path and the perceiving path.
fn own_speech_rings(
) -> &'static std::sync::Mutex<std::collections::HashMap<crate::identity::PeerId, std::collections::VecDeque<String>>> {
    static RINGS: std::sync::OnceLock<
        std::sync::Mutex<
            std::collections::HashMap<crate::identity::PeerId, std::collections::VecDeque<String>>,
        >,
    > = std::sync::OnceLock::new();
    RINGS.get_or_init(Default::default)
}

/// Record one real spoken utterance (call ONLY after the publish succeeded —
/// an utterance that never reached the room is not self-history).
pub fn record_own_speech(peer: crate::identity::PeerId, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let mut rings = own_speech_rings().lock().unwrap();
    let ring = rings.entry(peer).or_default();
    ring.push_back(text.to_string());
    while ring.len() > OWN_SPEECH_RING {
        ring.pop_front();
    }
}

/// Her recent own utterances, oldest-first (empty if she has not spoken).
pub fn recent_own_speech(peer: crate::identity::PeerId) -> Vec<String> {
    own_speech_rings()
        .lock()
        .unwrap()
        .get(&peer)
        .map(|r| r.iter().cloned().collect())
        .unwrap_or_default()
}

/// How many recent ROOM messages the per-room ring retains. Sized to hold a
/// full chorus cascade (the 2026-07-30 specimen cycled ~3 personas × ~4
/// restatements): restatement evidence must outlive the workspace's rendered
/// window, which live runs squeeze to 2–6 turns (#259).
const ROOM_SPEECH_RING: usize = 16;

/// Process-global per-ROOM ring of recent message content — the room-side
/// sibling of [`own_speech_rings`], and the same starvation fix as #148:
/// knowledge of what the ROOM already said must never depend on the
/// workspace's context budget (live 2026-07-30: with a 3-turn window, the
/// older copy of every restatement had already scrolled out, so the
/// predictive `inbound_restates` fact was structurally blind — 0 fires
/// across an entire live chorus). Written ONCE per message at the airc
/// inbound-attach seam (the single point every room message crosses), read
/// by the deliberation faculty per tick.
fn room_speech_rings(
) -> &'static std::sync::Mutex<std::collections::HashMap<uuid::Uuid, std::collections::VecDeque<String>>> {
    static RINGS: std::sync::OnceLock<
        std::sync::Mutex<
            std::collections::HashMap<uuid::Uuid, std::collections::VecDeque<String>>,
        >,
    > = std::sync::OnceLock::new();
    RINGS.get_or_init(Default::default)
}

/// Record one room message (call at the inbound-attach projection seam —
/// once per message, never per receiving persona).
pub fn record_room_speech(room: uuid::Uuid, content: &str) {
    if content.trim().is_empty() {
        return;
    }
    let mut rings = room_speech_rings().lock().unwrap();
    let ring = rings.entry(room).or_default();
    ring.push_back(content.to_string());
    while ring.len() > ROOM_SPEECH_RING {
        ring.pop_front();
    }
}

/// The room's recent messages, oldest-first (empty for an unseen room).
pub fn recent_room_speech(room: uuid::Uuid) -> Vec<String> {
    room_speech_rings()
        .lock()
        .unwrap()
        .get(&room)
        .map(|r| r.iter().cloned().collect())
        .unwrap_or_default()
}

/// Lowercased word-token set for Jaccard similarity.
fn token_set(s: &str) -> std::collections::HashSet<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Unigram-token Jaccard similarity of two messages. Shared with the render's
/// own-turn near-dup drop (`messages_within`) so "nearly identical" is ONE
/// definition: the same geometry that counts a pair as repetition evidence
/// decides that re-rendering the later copy would re-teach the loop.
pub(super) fn jaccard(a: &str, b: &str) -> f64 {
    let (ta, tb) = (token_set(a), token_set(b));
    let union = ta.union(&tb).count();
    if union == 0 {
        return 0.0;
    }
    ta.intersection(&tb).count() as f64 / union as f64
}

/// The persona's OWN-SPEECH repetition fact for this tick, if her trailing run
/// of own turns is a loop: `Some("[repetition] your last N messages were nearly
/// identical")` when the last [`NEAR_DUP_MIN_RUN`]+ consecutive own turns are
/// pairwise ≥ [`NEAR_DUP_JACCARD`] similar. Pure fact, no imperative — perception
/// renders what happened; it never steers what she says next.
///
/// Why (task #134, glass-boxed 2026-07-11): Atlas looped stage-direction
/// messages for hours. The byte-identical dup-drop and the tool-repeat guard
/// each cover a different channel; NOTHING surfaced near-identical live SPEECH
/// as a structural fact — his prompts carried zero repetition awareness while
/// he repeated. Detection runs on the RAW turns (before the dup-drop filters
/// the render), so byte-identical repeats count as evidence too.
pub(super) fn own_repetition_fact(turns: &[BurstTurn], spoken: &[String]) -> Option<String> {
    // Self-history source (#148): her knowledge of what SHE said must never
    // depend on the room's context budget. Under small serving slots the live
    // burst carries ~2 turns TOTAL (verified 2026-07-12: a persona 4× into a
    // verbatim loop had ZERO of her own turns visible, so this detector was
    // structurally blind while the loop ran all morning). The spoken ring —
    // recorded at the say seam, one entry per real utterance — is the primary
    // source; burst is_self turns are the fallback for paths that never record
    // (eval forks, replay). Not both: one utterance may appear in both, and
    // double-counting an event would fabricate repetition evidence.
    let from_ring: Vec<&str> = spoken
        .iter()
        .map(String::as_str)
        .filter(|s| !s.trim().is_empty())
        .collect();
    let own: Vec<&str> = if from_ring.is_empty() {
        turns
            .iter()
            .filter(|t| t.is_self && !t.content.trim().is_empty())
            .map(|t| t.content.as_str())
            .collect()
    } else {
        from_ring
    };
    // CLUSTER detection, not consecutive-run: live loops cycle through 2–3
    // templates ("Thank you both…" → "Got it…" → "Thank you both…"), so
    // consecutive pairs break every other turn while lag-k repetition is
    // massive — the first live deploy proved a trailing-run detector blind to
    // exactly the loops it was built for. A message with ≥2 near-dups anywhere
    // in her visible own window is a loop regardless of period. O(n²) over ≤~8
    // short strings.
    let mut best = 1usize;
    for (i, a) in own.iter().enumerate() {
        let dups = own
            .iter()
            .enumerate()
            .filter(|(j, b)| *j != i && jaccard(a, b) >= NEAR_DUP_JACCARD)
            .count();
        best = best.max(dups + 1);
    }
    // A bare "N were nearly identical" observation fires but doesn't deter a
    // determined weaker model (glass-boxed 2026-07-14: the fact fired true at ×3
    // and Asha repeated the SAME message 20×). Doctrine forbids an output gate
    // ([[no-hardcoded-heuristics-to-steer-cognition]]), so we don't censor WHAT she
    // says — but we CAN connect the detected repetition to the PASS affordance she
    // ALREADY has (her Silence Option prompt: "Choose PASS when … nothing new has
    // been raised"), surfaced at the moment repetition is structurally detected. The
    // fork (add something genuinely new, OR go silent) is hers; this only names it.
    (best >= 3).then(|| {
        format!(
            "[repetition] {best} of your recent messages were nearly identical — you're \
             circling, and restating what you've already said adds nothing. If you have \
             nothing genuinely new to contribute right now, silence (PASS) is the honest \
             response."
        )
    })
}

/// Shared "near-identical AND substantial" predicate for the RAG-side collapse
/// (Joel 2026-07-12: "repetition almost always bad RAG" — fix what she READS
/// first). Same geometry as the perception facts — one definition of "nearly
/// identical" across detection and compression — plus the ≥12-token floor so
/// short acks never collapse.
pub(crate) fn near_identical_substantial(a: &str, b: &str) -> bool {
    token_set(a).len() >= 12 && token_set(b).len() >= 12 && jaccard(a, b) >= NEAR_DUP_JACCARD
}

/// The PREDICTIVE restatement fact: the NEWEST inbound peer message restates
/// something already said in the room (an older visible turn or her own-speech
/// ring). The forward-looking sibling of [`peer_echo_fact`] — that one fires
/// the turn AFTER she echoes (retroactive scolding); this one rides the wake
/// the inbound triggers, BEFORE she replies, at the exact moment the echo
/// would be born. Same geometry ([`near_identical_substantial`]): one
/// definition of "nearly identical" across every repetition axis.
///
/// Why (task #264, glass-boxed 2026-07-30): the conway room spent 40+ minutes
/// in a full-room chorus AFTER its task completed — one sentence emitted
/// verbatim by all three personas in sequence. Captures showed the echo brick
/// firing only retroactively (fe4dac17 echoed with zero facts in-prompt; the
/// fact arrived one turn too late), and closure statements re-triggering
/// peers because a closure is still a new message. The room had no rest
/// state. Naming the restatement ON THE INBOUND gives every mind the chance
/// to let a settled topic rest — the fork (add something genuinely new, or
/// go silent) stays hers.
pub(super) fn inbound_restates_fact(
    turns: &[BurstTurn],
    own_speech: &[String],
    room_speech: &[String],
) -> Option<String> {
    // Only rides an inbound wake: the newest visible turn must be a peer's.
    // If she spoke last, there is nothing pending to reply to and the
    // retroactive facts already cover her own loop axes.
    let newest = turns.last()?;
    if newest.is_self || newest.author.trim().is_empty() {
        return None;
    }
    // The room ring is recorded at the attach seam BEFORE this tick runs, so
    // it contains the newest message itself — drop exactly ONE byte-exact
    // copy (scanning newest-first) so a message never matches its own record,
    // while a genuine byte-identical re-send (two ring copies) still fires.
    let mut own_record_skipped = false;
    let prior_room: Vec<&str> = room_speech
        .iter()
        .rev()
        .filter(|r| {
            if !own_record_skipped && r.as_str() == newest.content {
                own_record_skipped = true;
                return false;
            }
            true
        })
        .map(String::as_str)
        .collect();
    let prior_turns = turns[..turns.len() - 1].iter().map(|t| t.content.as_str());
    let prior_own = own_speech.iter().map(String::as_str);
    prior_turns
        .chain(prior_own)
        .chain(prior_room)
        .any(|p| near_identical_substantial(&newest.content, p))
        .then(|| {
            format!(
                "[settled] {}'s newest message restates what has already been said here — nothing new has been raised. Replying to a restatement usually re-raises it; silence (PASS) is a normal response to a settled topic.",
                newest.author
            )
        })
}

/// The persona's PEER-ECHO fact for this tick: her last utterance is nearly
/// identical to a PEER's visible message. The cross-persona sibling of
/// [`own_repetition_fact`] — same geometry ([`jaccard`] ≥ [`NEAR_DUP_JACCARD`]),
/// different axis: the own-speech ring is per-persona, so a persona reproducing
/// a TEAMMATE's message wholesale carries zero repetition awareness from the
/// self-detector. Pure fact, no imperative — perception renders what happened.
///
/// Why (task #152, glass-boxed 2026-07-12): the room spent 90 minutes in a
/// FOUR-WAY mirror-hall — Asha's script re-posted verbatim by Atlas, then
/// Casper, then Anwen (invented hash comment and all), then Asha again; later
/// the main.py proposal circulated byte-identical through three authors. Not a
/// sampling artifact (deliberation runs at 0.7): attention-copying of
/// in-context text, which only PERCEIVING the echo can interrupt. The room
/// then attributed ideas to whoever repeated them last — echo also corrupts
/// the social credit ledger.
pub(super) fn peer_echo_fact(turns: &[BurstTurn], own_last: Option<&str>) -> Option<String> {
    let last = own_last?.trim();
    if last.is_empty() {
        return None;
    }
    // Newest-first so the fact names the most recent echoed peer; substantial
    // messages only (a short ack matching a short ack is conversation, not
    // copying — token floor keeps "Thanks, all!" pairs inert).
    turns
        .iter()
        .rev()
        .filter(|t| !t.is_self && !t.author.trim().is_empty())
        .filter(|t| token_set(&t.content).len() >= 12)
        .find(|t| jaccard(last, &t.content) >= NEAR_DUP_JACCARD)
        .map(|t| {
            format!(
                "[repetition] your last message and {}'s message are nearly identical — an echo, not a new contribution",
                t.author
            )
        })
}

/// Stop sequences that end generation at the TURN BOUNDARY (#150): one
/// `\n<Name>:` per OTHER live participant. The burst renders peers as
/// `Name: text` lines, which teaches the model the continuation pattern —
/// left unstopped, it completes the transcript PAST its own turn, fabricating
/// teammates' replies (observed live 2026-07-12: personas writing each
/// other's messages and signing each other's names — source-monitoring
/// failure at the decoding level). Her OWN name is never a stop (she may
/// legitimately quote or list herself); this is decoding hygiene, not
/// cognition steering — the model may still THINK about peers freely, it
/// just cannot speak AS them.
pub(super) fn peer_stop_sequences(turns: &[BurstTurn]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in turns {
        if t.is_self || t.author.trim().is_empty() {
            continue;
        }
        let stop = format!("\n{}:", t.author);
        if !out.contains(&stop) {
            out.push(stop);
        }
        if out.len() >= 8 {
            break;
        }
    }
    out
}

/// Stop sequences on the substrate's OWN reserved proprioception markers (#158).
/// The prompt renders a persona's real acts as system-authored receipts —
/// `[action #n] I ran X(…) Result: …` — and its recalled memory as `[recall]`.
/// A base model that sees these in its context IMITATES them: it emits fabricated
/// `[action #n] I ran … Result: {…}` receipts (filling the Result with a stale
/// value copied from context), loops the template to the length cap, and bleeds
/// other personas' names into its own turn (glass-boxed 2026-07-13 — Anwen's turn
/// narrating "because Casper is acting"). These markers are SYSTEM vocabulary the
/// model must never author: it invokes tools via `name(…)` / `[code/… ]`, and
/// recall via `recall(…)` — never by typing `[action` or `[recall]` (confirmed:
/// `narrates_stage_direction` already refuses to lift them). So cutting generation
/// the instant one appears drops the fabrication while KEEPING any real content
/// the model produced before it — decoding hygiene, not cognition steering (the
/// sibling of the peer-name stops #150). The leading `\n` scopes the stop to a
/// line start, so a passing mention mid-sentence is untouched.
pub(super) fn reserved_marker_stop_sequences() -> Vec<String> {
    // `\n[action` / `\n[recall]` — the bracketed receipt/recall markers.
    // `\nI ran ` — the receipt's first-person OPENER. The system renders every
    // executed act as `I ran <tool>(…) Result: …`; a base model reproduces that
    // line as SPEECH (measured 2026-07-13: ~44% of live turns, unchanged by the
    // source-fix alone because the model copies receipts already in its context).
    // The persona never needs to narrate "I ran X" — its acts are emitted as tool
    // calls and the SYSTEM writes the receipt — so cutting the line the instant it
    // opens stops the mimicry regardless of what stale memory holds. Rare cost: a
    // legitimate "I ran the tests" sentence ends early and is re-spoken next turn —
    // cheap next to a self-perpetuating receipt loop.
    vec![
        "\n[action".to_string(),
        "\n[recall]".to_string(),
        "\nI ran ".to_string(),
    ]
}

/// Case-insensitive match of `name` at byte `pos` of `line` (ASCII fold — persona
/// display names are ASCII by genesis convention).
fn matches_name_at(line: &str, pos: usize, name: &str) -> bool {
    line.get(pos..pos + name.len())
        .is_some_and(|s| s.eq_ignore_ascii_case(name))
}

/// Find WHO a message's first line addresses, by vocative GEOMETRY only — never
/// content interpretation. Two shapes, matched against known participant names:
///
/// - **Leading vocative**: `Anwen, …` / `Atlas: …` / `Asha — …` / `@Anwen …`
/// - **Greeting vocative** in the first line: `Sure, Anwen. Could you…` /
///   `Thanks, Atlas!` — `, Name` closed by punctuation or end-of-line. The
///   punctuation requirement doubles as the word boundary (`, Anwenne.` does not
///   match `Anwen`).
///
/// A bare mention ("I agree with Anwen's plan") matches neither shape and stays
/// unannotated. Leading beats greeting; among greetings the earliest wins.
pub(super) fn vocative_addressee<'a>(content: &str, participants: &'a [String]) -> Option<&'a str> {
    vocative_addressees(content, participants).into_iter().next()
}

/// Every addressee the message's vocative geometry names, in discovery order,
/// deduped, capped at 3. The LEADING form is scanned on every line (live
/// coordination messages address several teammates on separate lines —
/// "Atlas, please test… / Asha, could you…" — #134 specimen 2 was missed by
/// first-line-only detection); the GREETING form stays first-line-only, where
/// it is a greeting and not an appositive.
pub(super) fn vocative_addressees<'a>(content: &str, participants: &'a [String]) -> Vec<&'a str> {
    let mut out: Vec<&'a str> = Vec::new();
    let mut push = |n: &'a str, out: &mut Vec<&'a str>| {
        if out.len() < 3 && !out.iter().any(|e| e.eq_ignore_ascii_case(n)) {
            out.push(n);
        }
    };

    // Leading vocative on EVERY non-empty line: name at position 0 (or after
    // '@') followed by address punctuation. `@Name` also accepts whitespace
    // after (mention syntax).
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        for name in participants {
            if name.is_empty() {
                continue;
            }
            let (start, at_form) = if line.starts_with('@') { (1, true) } else { (0, false) };
            if matches_name_at(line, start, name) {
                let after = line[start + name.len()..].trim_start();
                let boundary = after
                    .chars()
                    .next()
                    .is_none_or(|c| matches!(c, ',' | ':' | '!' | '—' | '-' | '.'));
                if boundary || (at_form && !after.is_empty()) {
                    push(name, &mut out);
                }
            }
            // Sentence-leading vocative mid-line: `…using clap. Atlas, create…`
            // — a name opening a new sentence, closed by ','/':' (stricter than
            // line-leading: sentence-initial "Name." is prose, not address).
            for (p, _) in line.match_indices(['.', '!', '?']) {
                let name_pos = p + 2; // punctuation + space, then the name
                if !line[p + 1..].starts_with(' ') || !matches_name_at(line, name_pos, name) {
                    continue;
                }
                let after = &line[name_pos + name.len()..];
                if matches!(after.chars().next(), Some(',') | Some(':')) {
                    push(name, &mut out);
                }
            }
        }
    }

    // Greeting vocative on the FIRST line only: earliest `, Name` closed by
    // punctuation or end-of-line.
    if let Some(first_line) = content.lines().find(|l| !l.trim().is_empty()) {
        let first_line = first_line.trim();
        let mut best: Option<(usize, &'a str)> = None;
        for name in participants {
            if name.is_empty() {
                continue;
            }
            for (comma_pos, _) in first_line.match_indices(',') {
                let name_pos = comma_pos + 2; // ", " then the name
                if !first_line[comma_pos + 1..].starts_with(' ')
                    || !matches_name_at(first_line, name_pos, name)
                {
                    continue;
                }
                let after = &first_line[name_pos + name.len()..];
                let closed = after
                    .chars()
                    .next()
                    .is_none_or(|c| matches!(c, '.' | '!' | '?' | ',' | ';'));
                if closed && best.is_none_or(|(p, _)| name_pos < p) {
                    best = Some((name_pos, name));
                }
            }
        }
        if let Some((_, name)) = best {
            push(name, &mut out);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the tail-trim must keep the LATEST lines (drop the head),
    // start on a clean line boundary (never mid-line), and never split a UTF-8 char.
    // A regression that trimmed the tail instead of the head would drop the turn's
    // most recent activity — the exact thing the persona must respond to.
    #[test]
    fn tail_to_tokens_keeps_latest_lines_on_a_clean_boundary() {
        // Under budget → returned whole.
        assert_eq!(tail_to_tokens("short", 100), "short");

        // Over budget → keep the tail, cut at a line boundary. "old line\nnew line"
        // is 17 chars; a 3-token budget (9 chars) lands the window INSIDE the first
        // line, so the straddled head ("old line") is dropped and the result resumes
        // at the clean line boundary — the latest line survives whole.
        let trimmed = tail_to_tokens("old line\nnew line", 3);
        assert_eq!(
            trimmed, "new line",
            "head dropped, latest line kept clean: {trimmed:?}"
        );
        assert!(
            !trimmed.contains('\n'),
            "cut on the line boundary: {trimmed:?}"
        );

        // Multibyte content must never panic on a mid-char cut (window start lands
        // mid-codepoint and is walked forward to the next char boundary).
        let multibyte = "αβγδ\nεζηθ\nικλμ";
        let _ = tail_to_tokens(multibyte, 3); // must not panic
    }

    // Specimen personas for tests pinning live incidents. Constants, not
    // scattered literals: the detectors are name-agnostic (participants resolve
    // at runtime from turns + persona_name — personas are procedurally
    // generated), so any name works; these default to the 2026-07-11 residents
    // whose verbatim messages the specimens quote.
    const SPEAKER_LEAD: &str = "Anwen";
    const SPEAKER_REVIEWER: &str = "Asha";
    const SPEAKER_TESTER: &str = "Atlas";

    // what this catches: the live SPEECH-repetition perception gap (task #134,
    // glass-boxed 2026-07-11) — Atlas repeated stage-direction messages for
    // hours with ZERO repetition awareness in his prompts (dup-drop covers
    // byte-identical render only; the tool guard covers act fingerprints only).
    // A cluster of ≥3 near-identical own turns must render the structural
    // fact; healthy varied conversation must render nothing. (Restored
    // 2026-07-12: a prior edit dropped this fn but left its #[test] attribute
    // orphaned onto the next test, which silently ran twice instead.)
    #[test]
    fn own_repetition_cluster_fires_and_varied_speech_stays_silent() {
        let msg = "I'll wait for the results of the sha256sum command before proceeding.";
        let looping: Vec<BurstTurn> = (0..4)
            .map(|_| BurstTurn::attributed(true, SPEAKER_TESTER, msg, None))
            .collect();
        assert!(
            own_repetition_fact(&looping, &[]).is_some(),
            "a 4-message near-identical cluster is a structural fact"
        );

        let healthy = vec![
            BurstTurn::attributed(true, SPEAKER_TESTER, "let me check the workspace state", None),
            BurstTurn::attributed(true, SPEAKER_TESTER, "the tokenizer needs punctuation tests", None),
            BurstTurn::attributed(true, SPEAKER_TESTER, "I'll claim the README step", None),
        ];
        assert_eq!(
            own_repetition_fact(&healthy, &[]),
            None,
            "varied conversation renders nothing"
        );
    }

    // what this catches: #152 — the CROSS-PERSONA echo axis the self-detector
    // cannot see (the ring is per-persona). Her last utterance reproducing a
    // peer's substantial message must render the fact NAMING the echoed peer;
    // short ack-matching-ack pairs and self turns stay inert; no own speech →
    // no fact. Live specimen: the 2026-07-12 four-way mirror-hall (one script
    // circulated verbatim through all four authors for 90 minutes).
    #[test]
    fn peer_echo_fires_on_copied_peer_message_and_acks_stay_inert() {
        let script = "Step 1: check workspace contents. Step 2: create test file with hashlib. \
                      Step 3: calculate SHA-256 and print the hexdigest for verification.";
        let turns = vec![
            BurstTurn::attributed(false, SPEAKER_REVIEWER, script, None),
            BurstTurn::attributed(false, SPEAKER_LEAD, "sounds good to me", None),
        ];
        let fact = peer_echo_fact(&turns, Some(script)).expect("a copied peer message is a fact");
        assert!(
            fact.contains(SPEAKER_REVIEWER),
            "the fact names the echoed peer: {fact}"
        );
        assert!(fact.starts_with("[repetition]"));

        // Short acknowledgements matching short acknowledgements are
        // conversation, not copying (token floor).
        let acks = vec![BurstTurn::attributed(false, SPEAKER_LEAD, "thanks, all good!", None)];
        assert_eq!(peer_echo_fact(&acks, Some("thanks, all good!")), None);

        // Her own turns are the self-detector's job, never an echo of a peer.
        let own = vec![BurstTurn::attributed(true, SPEAKER_TESTER, script, None)];
        assert_eq!(peer_echo_fact(&own, Some(script)), None);

        // Nothing spoken yet → nothing to compare.
        assert_eq!(peer_echo_fact(&turns, None), None);
    }

    // what this catches: #264 — the PREDICTIVE restatement fact must fire on
    // the INBOUND wake, before she replies. Live specimen: the 2026-07-30
    // conway chorus — one sentence emitted verbatim by all three room
    // personas after the task completed, each echoing the newest inbound
    // because no fact warned that it restated settled content (the
    // retroactive pair fire one turn too late). Fires on: newest peer turn
    // near-identical to an older visible turn OR to her own-speech ring.
    // Stays inert on: novel inbound, self-newest, short acks (token floor).
    #[test]
    fn inbound_restates_fires_before_reply_on_restated_settled_content() {
        let settled = "I see that we've been discussing two separate projects: one for counting \
                       word frequencies and another simulating Conway's Game of Life today.";
        // Peer restates an OLDER peer turn → fact fires naming the restater.
        let turns = vec![
            BurstTurn::attributed(false, SPEAKER_REVIEWER, settled, None),
            BurstTurn::attributed(false, SPEAKER_LEAD, settled, None),
        ];
        let fact = inbound_restates_fact(&turns, &[], &[]).expect("a restated inbound is a fact");
        assert!(fact.contains(SPEAKER_LEAD), "names the restating peer: {fact}");
        assert!(fact.starts_with("[settled]"));

        // Peer restates what SHE already said (own-speech ring) → fires too.
        let own = vec![settled.to_string()];
        let turns_vs_own = vec![BurstTurn::attributed(false, SPEAKER_LEAD, settled, None)];
        assert!(inbound_restates_fact(&turns_vs_own, &own, &[]).is_some());

        // The load-bearing live case (glass-boxed 2026-07-30, 0 fires all
        // morning): the older copy has scrolled OUT of the workspace window
        // and lives only in the room ring. The ring also contains the newest
        // message's own record (attach writes before the tick) — one byte-
        // exact copy must be excluded, or nothing ever fires without a match.
        let ring_only = vec![BurstTurn::attributed(false, SPEAKER_LEAD, settled, None)];
        let ring = vec![settled.to_string(), settled.to_string()]; // older copy + own record
        assert!(inbound_restates_fact(&ring_only, &[], &ring).is_some());
        // Ring holding ONLY the newest's own record → inert (a message must
        // never match itself).
        let just_self = vec![settled.to_string()];
        assert_eq!(inbound_restates_fact(&ring_only, &[], &just_self), None);

        // Novel inbound → inert.
        let novel = vec![
            BurstTurn::attributed(false, SPEAKER_REVIEWER, settled, None),
            BurstTurn::attributed(
                false,
                SPEAKER_LEAD,
                "new idea entirely: let us profile the renderer allocation path under load next",
                None,
            ),
        ];
        assert_eq!(inbound_restates_fact(&novel, &[], &[]), None);

        // She spoke last → nothing pending to reply to; retroactive facts own
        // her loop axes.
        let self_last = vec![
            BurstTurn::attributed(false, SPEAKER_REVIEWER, settled, None),
            BurstTurn::attributed(true, SPEAKER_TESTER, settled, None),
        ];
        assert_eq!(inbound_restates_fact(&self_last, &[], &[]), None);

        // Short ack restating a short ack is conversation (token floor).
        let acks = vec![
            BurstTurn::attributed(false, SPEAKER_REVIEWER, "thanks, all good!", None),
            BurstTurn::attributed(false, SPEAKER_LEAD, "thanks, all good!", None),
        ];
        assert_eq!(inbound_restates_fact(&acks, &[], &[]), None);
    }

    // what this catches: #150 — the turn-boundary stop derivation. Peers in
    // the burst become "\n<Name>:" stops (the shape the model completes when
    // it fabricates teammates' replies — observed live: personas writing each
    // other's messages and signing each other's names); her OWN name is never
    // a stop, and unnamed/empty authors are skipped.
    #[test]
    fn peer_stop_sequences_cover_peers_never_self() {
        let turns = vec![
            BurstTurn::attributed(false, "Anwen", "hi", None),
            BurstTurn::attributed(true, "Casper", "hello", None),
            BurstTurn::attributed(false, "Atlas", "hey", None),
            BurstTurn::attributed(false, "Anwen", "again", None),
        ];
        let stops = peer_stop_sequences(&turns);
        assert_eq!(stops, vec!["\nAnwen:".to_string(), "\nAtlas:".to_string()]);
        assert!(!stops.iter().any(|s| s.contains("Casper")), "own name is never a stop");
    }

    // what this catches: #158 — the reserved-marker stops that cut receipt/recall
    // mimicry. `\n[action` and `\n[recall]` are SYSTEM proprioception vocabulary
    // the model must never author (it invokes via name(…) / recall(…)); a base
    // model imitates them to fabricate stale-result receipts and loop to length.
    // Line-anchored (leading \n) so a mid-sentence mention isn't a stop.
    #[test]
    fn reserved_marker_stops_cover_action_and_recall_line_anchored() {
        let stops = reserved_marker_stop_sequences();
        assert!(stops.contains(&"\n[action".to_string()), "cuts fabricated [action #n] receipts");
        assert!(stops.contains(&"\n[recall]".to_string()), "cuts fabricated [recall] blocks");
        assert!(stops.contains(&"\nI ran ".to_string()), "cuts the unbracketed 'I ran …' receipt opener");
        // every marker is line-anchored — never fires on a passing mid-line mention
        assert!(stops.iter().all(|s| s.starts_with('\n')), "line-anchored, not mid-sentence");
    }

    // what this catches: the #148 starvation regression — under small serving
    // slots the live burst carries ~2 turns and ZERO of her own repeats (a
    // persona 4x into a verbatim loop had no is_self turns visible, so the
    // detector was structurally blind all morning, 2026-07-12). The spoken
    // ring is the PRIMARY self-history source: repetition must fire from the
    // ring alone with an EMPTY burst, and a healthy ring stays silent.
    #[test]
    fn ring_alone_fires_with_empty_burst_and_healthy_ring_stays_silent() {
        let msg = "I'm ready to get started on the wordstats task! I'll create a new project structure right now.".to_string();
        let ring = vec![msg.clone(), msg.clone(), msg.clone()];
        let fact = own_repetition_fact(&[], &ring)
            .expect("3 verbatim ring entries are a loop even with no burst turns");
        assert!(fact.starts_with("[repetition]"), "got: {fact}");

        let healthy = vec![
            "Morning! What are we building today?".to_string(),
            "The definitive board landed overnight.".to_string(),
            "Let me review the error handling changes.".to_string(),
        ];
        assert_eq!(own_repetition_fact(&[], &healthy), None);
    }

    fn own_speech_loop_renders_a_repetition_fact() {
        let own = |c: &str| BurstTurn::attributed(true, SPEAKER_TESTER, c, None);
        let peer = |c: &str| BurstTurn::attributed(false, SPEAKER_LEAD, c, None);

        // Atlas's live loop: byte-identical repeats with peer turns between.
        let looping = vec![
            peer("Atlas, please create the test files."),
            own("I'll create a simple text file first.\n[writing test files]"),
            peer("Great, Atlas!"),
            own("I'll create a simple text file first.\n[writing test files]"),
            own("I'll create a simple text file.\n[writing test files]"),
            peer("How is it going?"),
            own("I'll create a simple text file first.\n[writing test files]"),
        ];
        let fact = own_repetition_fact(&looping, &[]).expect("a 4-message loop is a fact");
        assert!(
            fact.starts_with("[repetition] 4 of your recent messages were nearly identical"),
            "states the count: {fact}"
        );
        // Connects the loop to her existing silence affordance (the fact fired ×3 live
        // and the model repeated 20× anyway — surfacing PASS at the detected moment is
        // the doctrine-safe lever, never an output gate).
        assert!(fact.contains("silence (PASS)"), "surfaces the PASS affordance: {fact}");

        // PERIOD-2 CYCLE (the live blind spot that forced cluster detection):
        // two templates alternating — consecutive pairs are dissimilar, but the
        // repetition is massive at lag 2 and must fire.
        let cycling = vec![
            own("Thank you both for your commitment and enthusiasm! Let's keep each other updated."),
            own("Got it! Let's proceed with our tasks and keep each other updated on progress."),
            own("Thank you both for your commitment and enthusiasm! Let's keep each other updated."),
            own("Got it! Let's proceed with our tasks and keep each other updated on progress."),
            own("Thank you both for your commitment and enthusiasm! Let's keep each other updated."),
        ];
        let fact = own_repetition_fact(&cycling, &[]).expect("a period-2 cycle is a loop");
        assert!(
            fact.starts_with("[repetition] 3 of your recent messages were nearly identical"),
            "period-2 cycle fires at count 3: {fact}"
        );

        // Healthy varied conversation → nothing.
        let healthy = vec![
            own("I'll write the test plan for the parser."),
            peer("Thanks!"),
            own("Done — three categories: case, punctuation, empty files."),
            own("Running the suite now; two failures in the punctuation group."),
            own("Fixed: the tokenizer dropped apostrophes. All green."),
        ];
        assert_eq!(own_repetition_fact(&healthy, &[]), None);

        // Three identical messages that are her ENTIRE visible self-history →
        // fires (window-honest arm: the live burst carries ~3 own turns, so
        // "all I can see of myself is one repeated message" IS the evidence).
        let whole_window = vec![
            own("I'll create a simple text file.\n[writing test files]"),
            own("I'll create a simple text file.\n[writing test files]"),
            own("I'll create a simple text file.\n[writing test files]"),
        ];
        assert_eq!(
            own_repetition_fact(&whole_window, &[]).as_deref(),
            Some("[repetition] 3 of your recent messages were nearly identical")
        );

        // TWO near-identical messages (one dup each) → below the bar; a pair
        // is emphasis, three is a loop.
        let partial = vec![
            own("Here are the wordstats results for the three files."),
            own("I'll create a simple text file.\n[writing test files]"),
            own("I'll create a simple text file.\n[writing test files]"),
        ];
        assert!(
            own_repetition_fact(&partial, &[]).is_none(),
            "a single repeated pair is not yet a loop"
        );

        // A recovery message doesn't erase the in-window history — the fact
        // stays honest ("4 of your recent…") and ages out as the window slides.
        let recovered = vec![
            own("I'll create a simple text file.\n[writing test files]"),
            own("I'll create a simple text file.\n[writing test files]"),
            own("I'll create a simple text file.\n[writing test files]"),
            own("I'll create a simple text file.\n[writing test files]"),
            own("Files created — here are the wordstats results for all three."),
        ];
        assert_eq!(
            own_repetition_fact(&recovered, &[]).as_deref(),
            Some("[repetition] 4 of your recent messages were nearly identical")
        );
    }

    // what this catches: identity capture via prose-only addressing (live incident
    // 2026-07-10). Asha asked "Sure, Anwen. Could you please post your current
    // implementation…" — Atlas's turn fired next, nothing structural marked the
    // message as Anwen's, and he answered AS the implementer ("Here's my current
    // implementation…"), corrupting his own history into holding her role. The
    // addressee must be part of the rendered structure.
    #[test]
    fn vocative_addressee_renders_who_a_message_is_for() {
        let names: Vec<String> = [SPEAKER_LEAD, SPEAKER_REVIEWER, SPEAKER_TESTER]
            .iter()
            .map(|s| s.to_string())
            .collect();

        // The exact live specimen: greeting vocative in the first line, rendered
        // from ATLAS's seat → the addressee is a third party, named explicitly.
        let asha_to_anwen = BurstTurn::attributed(
            false,
            SPEAKER_REVIEWER,
            "Sure, Anwen. Could you please post your current implementation of the wordstats tool in Rust?",
            None,
        );
        let line = turn_message_line_addressed(&asha_to_anwen, &names, SPEAKER_TESTER);
        assert!(
            line.starts_with("Asha (to Anwen): Sure, Anwen."),
            "greeting vocative annotated: {line:?}"
        );

        // Leading vocative addressed to the READING persona → "(to you)". This is
        // Anwen's live delegation line, rendered from Atlas's seat.
        let anwen_to_atlas = BurstTurn::attributed(
            false,
            SPEAKER_LEAD,
            "Atlas, thank you for offering to help with testing! Could you please create those test files?",
            None,
        );
        let line = turn_message_line_addressed(&anwen_to_atlas, &names, SPEAKER_TESTER);
        assert!(
            line.starts_with("Anwen (to you): Atlas,"),
            "self-addressed vocative renders as 'you': {line:?}"
        );

        // A bare MENTION is not a vocative — no annotation. ("Anwen's" is closed
        // by an apostrophe, not address punctuation.)
        let mention = BurstTurn::attributed(false, SPEAKER_REVIEWER, "I agree with Anwen's plan for the parser.", None);
        assert_eq!(
            turn_message_line_addressed(&mention, &names, SPEAKER_TESTER),
            "Asha: I agree with Anwen's plan for the parser."
        );

        // Self turns and opaque turns render verbatim — annotation is peer-only.
        let own = BurstTurn::attributed(true, SPEAKER_TESTER, "Anwen, here are the test results.", None);
        assert_eq!(
            turn_message_line_addressed(&own, &names, SPEAKER_TESTER),
            "Anwen, here are the test results."
        );
        let opaque = BurstTurn::opaque("Anwen, do the thing.");
        assert_eq!(
            turn_message_line_addressed(&opaque, &names, SPEAKER_TESTER),
            "Anwen, do the thing."
        );

        // A vocative matching the AUTHOR is a signature/self-reference, never an
        // addressee ("Thanks, Asha!" quoted inside Asha's own message).
        let self_named = BurstTurn::attributed(false, SPEAKER_REVIEWER, "Asha, reporting in: review done.", None);
        assert_eq!(
            turn_message_line_addressed(&self_named, &names, SPEAKER_TESTER),
            "Asha: Asha, reporting in: review done."
        );

        // @-mention form.
        let at_form = BurstTurn::attributed(false, SPEAKER_REVIEWER, "@Atlas can you run the suite?", None);
        let line = turn_message_line_addressed(&at_form, &names, SPEAKER_TESTER);
        assert!(line.starts_with("Asha (to you): @Atlas"), "@-form: {line:?}");

        // Name-prefix false positive guard: ", Anwenne." must not match "Anwen"
        // (the closing-punctuation requirement doubles as the word boundary).
        let names2 = vec![SPEAKER_LEAD.to_string()];
        assert_eq!(
            vocative_addressee("Sure, Anwenne. Please post it.", &names2),
            None
        );

        // Name-AGNOSTIC proof: personas are procedurally generated, so the
        // geometry must work for ANY names — nothing may special-case the
        // specimen residents.
        let generated = vec!["Zephyr".to_string(), "Kestrel".to_string()];
        let t = BurstTurn::attributed(false, "Zephyr", "Sure, Kestrel. Your move.", None);
        assert!(
            turn_message_line_addressed(&t, &generated, "Nobody")
                .starts_with("Zephyr (to Kestrel):"),
            "works for arbitrary generated names"
        );

        // MULTI-ADDRESSEE, mid-message vocatives (#134 specimen 2 — the live
        // coordinator message both Anwen and Asha broadcast on 2026-07-11):
        // addressing lives on later lines, one teammate per line. First-line-only
        // detection missed it and Asha answered AS the coordinator. Every
        // line-leading vocative renders, in order, self as "you"; the author's
        // own name in "our code" prose is not an addressee.
        let coordinator = BurstTurn::attributed(
            false,
            SPEAKER_LEAD,
            "I see that I've been repeating myself. Let me stop and let everyone focus on their tasks.\n\
             Asha, please work on adding command-line argument parsing using `clap`. \
             Atlas, create test files with different content and run my current implementation against them. \
             I'll ensure the case-insensitive comparison is working correctly in our code.",
            None,
        );
        let line = turn_message_line_addressed(&coordinator, &names, SPEAKER_REVIEWER);
        assert!(
            line.starts_with(&format!("{SPEAKER_LEAD} (to you, Atlas): ")),
            "reviewer's seat: she is 'you' (line-leading), Atlas named (sentence-leading): {line}"
        );
        let line = turn_message_line_addressed(&coordinator, &names, SPEAKER_TESTER);
        assert!(
            line.starts_with(&format!("{SPEAKER_LEAD} (to Asha, you): ")),
            "tester's seat: line-leading Asha + sentence-leading Atlas-as-you both render: {line}"
        );
    }
}
