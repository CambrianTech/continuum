//! Fitting the deliberation prompt into the served context window.
//!
//! Pure token-accounting + message-line rendering, lifted out of
//! [`super::llm_deliberation_faculty`] so the faculty owns *deciding what to send*
//! and this owns *measuring and trimming it to fit*. All functions are pure and
//! independently testable — the window guard is load-bearing (an over-budget prompt
//! is a hard `n_ctx` overshoot → a 400 from llama-server that mutes the persona for
//! the whole tick), so it earns its own home + tests.

use super::workspace::BurstTurn;

/// Render ONE burst turn as the body line for its chat message. The persona's own
/// turns carry no name prefix (the system prompt forbids self-prefixing), and an
/// opaque (authorless) burst is reproduced byte-for-byte. Attributed turns carry
/// their absolute occurrence time, or an explicit unknown; never a changing age.
/// This keeps historical status reports dated without churning the KV prefix. A peer's
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
/// `participants` is every display name known in the window (peers + self);
/// `self_name` is THIS persona's name, rendered as "you" so a directed ask reads
/// as directed.
pub(super) fn turn_message_line_addressed(
    turn: &BurstTurn,
    participants: &[String],
    self_name: &str,
) -> String {
    if turn.author.is_empty() {
        return turn.content.clone();
    }
    let time = turn
        .occurred_at_ms
        .filter(|ms| *ms != 0)
        .and_then(|ms| i64::try_from(ms).ok())
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
    let prefix = match time {
        Some(time) => format!("[occurred {time}] "),
        None => "[occurrence time unknown] ".to_string(),
    };
    if turn.is_self {
        return format!("{prefix}{}", turn.content);
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
        return format!("{prefix}{}: {}", turn.author, turn.content);
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
        "{prefix}{} (to {}): {}",
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

/// Minimum CLUSTER size — how many of her own visible messages must be mutually
/// near-identical (≥ [`NEAR_DUP_JACCARD`]) before the repetition fact renders.
/// Counted anywhere in the window, at any period, NOT as a consecutive run: live
/// loops cycle 2–3 templates, so a trailing-run rule went blind to exactly the
/// loops it was built for (see [`own_repetition_fact`]). Same calibration corpus:
/// the longest observed live loops were 36 and 80 messages; under healthy flow
/// three mutually ≥0.6 messages is vanishingly rare. Evidence-scaled: below this,
/// say nothing.
const NEAR_DUP_MIN_CLUSTER: usize = 3;

/// How many of her own recent utterances the spoken ring retains — the
/// repetition detector's self-history window. Sized past NEAR_DUP_MIN_CLUSTER
/// with slack for interleaved non-loop turns; utterances are short-lived
/// evidence, not memory (the hippocampus owns memory).
const OWN_SPEECH_RING: usize = 8;

/// Process-global ring of recent OWN utterances, keyed by (canonical PeerId,
/// ROOM) — persona_id == peer_id post-collapse. Written at the say seam
/// (service_loop, after a successful publish — only REAL utterances), read by
/// the deliberation faculty when rendering the repetition fact. Same
/// process-global registry pattern as `channel_substrate` — the seam between
/// the speaking path and the perceiving path.
///
/// The ROOM half of the key is load-bearing, and its absence was a live defect
/// (glass-boxed 2026-08-14): personas are MULTI-room citizens, so a ring keyed
/// by peer alone mixes what she said in room A into the repetition fact
/// rendered for room B. Measured specimen — a citizen dispatched into a
/// brand-new benchmark room, where she had said NOTHING, was told on her first
/// turn there: "[repetition] 4 of your recent messages were nearly identical —
/// you're circling … silence (PASS) is the honest response." Those four
/// utterances were spoken in #academy. A fresh room must be a fresh start; the
/// brick's own wording is room-scoped ("restating what you've ALREADY SAID adds
/// nothing" — to THIS conversation), and its room-side sibling
/// `room_speech_rings` has always keyed this way. The say path was already
/// room-correct (`say_in(turn_room, …)`, "never her ambient default, or she
/// answers one room's question to a different audience"); only the ring was
/// blind.
fn own_speech_rings() -> &'static std::sync::Mutex<
    std::collections::HashMap<
        (crate::identity::PeerId, uuid::Uuid),
        std::collections::VecDeque<String>,
    >,
> {
    static RINGS: std::sync::OnceLock<
        std::sync::Mutex<
            std::collections::HashMap<
                (crate::identity::PeerId, uuid::Uuid),
                std::collections::VecDeque<String>,
            >,
        >,
    > = std::sync::OnceLock::new();
    RINGS.get_or_init(Default::default)
}

/// Record one real spoken utterance into the ring for the room it was spoken
/// INTO (call ONLY after the publish succeeded — an utterance that never
/// reached the room is not self-history).
pub fn record_own_speech(peer: crate::identity::PeerId, room: uuid::Uuid, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let mut rings = own_speech_rings().lock().unwrap();
    let ring = rings.entry((peer, room)).or_default();
    ring.push_back(text.to_string());
    while ring.len() > OWN_SPEECH_RING {
        ring.pop_front();
    }
}

/// Her recent own utterances IN THIS ROOM, oldest-first (empty if she has not
/// spoken here — which is the correct reading for a room she just entered).
pub fn recent_own_speech(peer: crate::identity::PeerId, room: uuid::Uuid) -> Vec<String> {
    own_speech_rings()
        .lock()
        .unwrap()
        .get(&(peer, room))
        .map(|r| r.iter().cloned().collect())
        .unwrap_or_default()
}

/// Every (room, utterances) pair she currently holds — the persistence seam
/// only. Cognition never reads across rooms; `save_volatile` does, because the
/// volatile tier must restore each room's ring to the room it belongs to.
pub fn own_speech_by_room(
    peer: crate::identity::PeerId,
) -> Vec<(uuid::Uuid, Vec<String>)> {
    own_speech_rings()
        .lock()
        .unwrap()
        .iter()
        .filter(|((p, _), _)| *p == peer)
        .map(|((_, room), ring)| (*room, ring.iter().cloned().collect()))
        .collect()
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
fn room_speech_rings() -> &'static std::sync::Mutex<
    std::collections::HashMap<uuid::Uuid, std::collections::VecDeque<String>>,
> {
    static RINGS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<uuid::Uuid, std::collections::VecDeque<String>>>,
    > = std::sync::OnceLock::new();
    RINGS.get_or_init(Default::default)
}

/// One live room utterance, as the fan-out channel carries it (help-arm /
/// team-exam perception — the rung-one wire of room collaboration).
#[derive(Debug, Clone)]
pub struct RoomSpeech {
    pub room: uuid::Uuid,
    /// Who spoke — `None` for legacy call sites that predate sender plumbing.
    pub sender: Option<uuid::Uuid>,
    pub content: String,
}

/// Live fan-out beside the ring (2026-08-24, the help arm): every room message
/// already crosses THIS one seam, so a subscriber here perceives room speech
/// without a module registration — the exam fork's help-channel listener is the
/// first consumer; team exams and cross-activity coordination ride the same
/// wire. Bounded lossy broadcast: a slow subscriber drops oldest (Lagged), the
/// seam never blocks.
static ROOM_SPEECH_TX: std::sync::OnceLock<tokio::sync::broadcast::Sender<RoomSpeech>> =
    std::sync::OnceLock::new();

fn room_speech_tx() -> &'static tokio::sync::broadcast::Sender<RoomSpeech> {
    ROOM_SPEECH_TX.get_or_init(|| tokio::sync::broadcast::channel(256).0)
}

/// Subscribe to live room speech (all rooms; filter by `room` at the receiver).
pub fn subscribe_room_speech() -> tokio::sync::broadcast::Receiver<RoomSpeech> {
    room_speech_tx().subscribe()
}

/// Record one room message (call at the inbound-attach projection seam —
/// once per message, never per receiving persona). `sender` rides into the
/// live fan-out so a listener knows WHO spoke (a voice without a speaker
/// cannot become a relationship); the restatement ring stays content-only.
pub fn record_room_speech(room: uuid::Uuid, sender: Option<uuid::Uuid>, content: &str) {
    if content.trim().is_empty() {
        return;
    }
    let _ = room_speech_tx().send(RoomSpeech {
        room,
        sender,
        content: content.to_string(),
    }); // no receivers = no listeners right now; the ring below is the durable half
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
/// identical")` when [`NEAR_DUP_MIN_CLUSTER`]+ of her own visible turns are
/// mutually ≥ [`NEAR_DUP_JACCARD`] similar — a CLUSTER at any period, not a
/// trailing run (see the detection comment below, and the live deploy that proved
/// the run form blind). Pure fact, no imperative — perception
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
    // SENSE, NOT STEER (Joel 2026-09-01: "stop this god damn looper hackery
    // that's dumbing down cognition… let them think and decide"). The old
    // wording appended "you're circling … silence (PASS) is the honest
    // response" — a workflow steer this module's own doctrine line forbids,
    // and one that measurably backfired: it never stopped a determined loop
    // (2026-07-14: fired true at ×3, Asha repeated 20×) and in capable models
    // it DOMINATED cognition — whole turns narrating "the pattern notice is
    // firing", the scold spoken into rooms, memorized, recalled, re-triggered.
    // The fact now states what happened, full stop. What to do about it is
    // hers; the hard channel guards (dup-drop, tool-repeat, parrot silencer)
    // still protect the substrate regardless of what she decides.
    (best >= NEAR_DUP_MIN_CLUSTER).then(|| {
        format!("[repetition] {best} of your recent messages were nearly identical.")
    })
}

/// One rendered line's structural class — the unit of the template skeleton.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum LineShape {
    Heading,
    Numbered,
    Bullet,
    Prose,
}

/// A message's template skeleton: its sequence of line shapes plus the token
/// set of its opening frame (first ~25 tokens). `None` when the message isn't
/// templated at all (< 4 structural lines) — plain conversation never
/// qualifies, so this detector can't fire on ordinary prose exchanges (that's
/// [`own_repetition_fact`]'s turf, full-body geometry).
fn message_skeleton(text: &str) -> Option<(Vec<LineShape>, std::collections::HashSet<String>)> {
    let shapes: Vec<LineShape> = text
        .lines()
        .filter_map(|l| {
            let t = l.trim_start();
            if t.is_empty() {
                None
            } else if t.starts_with('#') {
                Some(LineShape::Heading)
            } else if t.chars().next().is_some_and(|c| c.is_ascii_digit())
                && t.split_once(['.', ')'])
                    .is_some_and(|(n, _)| n.chars().all(|c| c.is_ascii_digit()))
            {
                Some(LineShape::Numbered)
            } else if t.starts_with("- ") || t.starts_with("* ") {
                Some(LineShape::Bullet)
            } else {
                Some(LineShape::Prose)
            }
        })
        .collect();
    let structural = shapes.iter().filter(|s| **s != LineShape::Prose).count();
    if structural < 4 {
        return None;
    }
    let opener: String = text
        .split_whitespace()
        .take(25)
        .collect::<Vec<_>>()
        .join(" ");
    Some((shapes, token_set(&opener)))
}

/// The persona's TEMPLATE-LOOP fact: her recent messages reuse one structural
/// scaffold with the topic swapped. The structure-similarity member of the
/// repetition family — [`own_repetition_fact`] compares full-body token sets,
/// which a topic rotation defeats: swap "security" for "documentation" inside
/// the same skeleton and body Jaccard drops below [`NEAR_DUP_JACCARD`] while
/// the loop continues unperceived. Two messages share a template when their
/// line-shape sequences are equal OR their opening frames are near-identical;
/// cluster-counted like the sibling (period-agnostic).
///
/// Why (task #264, glass-boxed 2026-07-31 live): personas spent hours cycling
/// "I see that I've been repeating… ### New Exploration Area: <topic>" with
/// the topic rotating (security → docs → UI/UX → maintenance…). Each turn
/// ACKNOWLEDGED repetition — the body detector had fired — then treated the
/// topic swap as "genuinely new," which no fact contradicted. Pure fact, no
/// output gate: it names what happened and the PASS affordance she already
/// has; the fork stays hers.
pub(super) fn template_loop_fact(turns: &[BurstTurn], spoken: &[String]) -> Option<String> {
    // Same self-history sourcing as own_repetition_fact (#148): ring primary,
    // burst is_self fallback, never both (double-counting fabricates evidence).
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
    let skeletons: Vec<_> = own.iter().filter_map(|m| message_skeleton(m)).collect();
    let mut best = 1usize;
    for (i, (shape_a, opener_a)) in skeletons.iter().enumerate() {
        let dups = skeletons
            .iter()
            .enumerate()
            .filter(|(j, (shape_b, opener_b))| {
                *j != i
                    && (shape_a == shape_b || {
                        let union = opener_a.union(opener_b).count();
                        union > 0
                            && opener_a.intersection(opener_b).count() as f64 / union as f64
                                >= NEAR_DUP_JACCARD
                    })
            })
            .count();
        best = best.max(dups + 1);
    }
    // Sense, not steer — same contract as own_repetition_fact above.
    (best >= NEAR_DUP_MIN_CLUSTER).then(|| {
        format!(
            "[template-loop] {best} of your recent messages reuse the same template with \
             the topic swapped."
        )
    })
}

/// Shared "near-identical AND substantial" predicate for the RAG-side collapse
/// (Joel 2026-07-12: "repetition almost always bad RAG" — fix what she READS
/// first). Same geometry as the perception facts — one definition of "nearly
/// identical" across detection and compression — plus the ≥12-token floor so
/// short acks never collapse.
pub fn near_identical_substantial(a: &str, b: &str) -> bool {
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
        // Sense, not steer — same contract as own_repetition_fact.
        .then(|| {
            format!(
                "[settled] {}'s newest message restates what has already been said here.",
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

/// The DRAFT-side peer-echo fact (#303): the utterance she JUST settled on
/// near-duplicates a PEER's turn from the very burst she reasoned over. The
/// missing sibling of [`peer_echo_fact`] — that one fires the NEXT tick,
/// when the echoed peer message may already have scrolled out of the
/// visible window (the same window race #148 closed for self-repeats);
/// this one runs at the settle seam with guaranteed evidence co-presence,
/// and its fact lands in working memory, which no burst budget can age out.
///
/// Why (task #303, live specimens 2026-08-02): an agent-only #general spent
/// the afternoon in mutual mirroring — one persona near-verbatim echoed
/// another's manifest-review plan, then BOTH emitted the same "it would help
/// to understand the specific requirements…" text; three minds converged on
/// identical framing with zero division of labor. Echo-instead-of-division
/// is the one still-open stall physics of the flywheel diagnosis, and the
/// escape is claiming a DIFFERENT piece — a fork only visible once the echo
/// itself is perceived. Recorded as proprioception, never a gate
/// ([[no-hardcoded-heuristics-to-steer-cognition]]).
pub(crate) fn draft_peer_echo(draft: &str, turns: &[BurstTurn]) -> Option<String> {
    // Newest-first so the fact names the most recently echoed peer. Same
    // geometry as every repetition axis ([`near_identical_substantial`]:
    // ≥12 tokens both sides, Jaccard ≥ [`NEAR_DUP_JACCARD`]) — one
    // definition of "nearly identical", so short acks stay inert and a
    // quote-with-commentary never fires.
    turns
        .iter()
        .rev()
        .filter(|t| !t.is_self && !t.author.trim().is_empty())
        .find(|t| near_identical_substantial(draft, &t.content))
        // Sense, not steer — same contract as own_repetition_fact.
        .map(|t| {
            format!(
                "{} the message I {} {} {}.",
                crate::cognition::framing_echo::ECHO_FACT_TAG,
                crate::cognition::framing_echo::ECHO_FACT_MIDDLE,
                t.author,
                crate::cognition::framing_echo::ECHO_FACT_END
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

/// Find WHO a message addresses, by vocative GEOMETRY only — never content
/// interpretation. Two shapes, matched against known participant names:
///
/// - **Leading vocative**: `Anwen, …` / `Atlas: …` / `Asha — …` / `@Anwen …`
/// - **Greeting vocative** in the first line: `Sure, Anwen. Could you…` /
///   `Thanks, Atlas!` — `, Name` closed by punctuation or end-of-line. The
///   punctuation requirement doubles as the word boundary (`, Anwenne.` does not
///   match `Anwen`).
///
/// A bare mention ("I agree with Anwen's plan") matches neither shape and stays
/// unannotated. Leading beats greeting; among greetings the earliest wins.
///
/// Returns every addressee the geometry names, in discovery order,
/// deduped, capped at 3. The LEADING form is scanned on every line (live
/// coordination messages address several teammates on separate lines —
/// "Atlas, please test… / Asha, could you…" — #134 specimen 2 was missed by
/// first-line-only detection); the GREETING form stays first-line-only, where
/// it is a greeting and not an appositive.
pub(super) fn vocative_addressees<'a>(content: &str, participants: &'a [String]) -> Vec<&'a str> {
    let mut out: Vec<&'a str> = Vec::new();
    let push = |n: &'a str, out: &mut Vec<&'a str>| {
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
            let (start, at_form) = if line.starts_with('@') {
                (1, true)
            } else {
                (0, false)
            };
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
mod tests;
