//! Salience: how loud an activity's delta is to HER, as a pure function.
//!
//! The event mind (docs/architecture/EVENT-MIND.md §1b, §6): her perception renders
//! the one truth per activity; what changed above her cursor is the delta; this
//! module turns a delta into a [`Salience`] — a level and the typed reasons for it.
//! The maximum salience across her activities, measured against her attention
//! dial, is what wakes her. Nothing here decides for her; it only says how loud.
//!
//! One signal, three jobs (Joel, 2026-10-04): a `Surprise` reason — perception
//! breaking with the expectation her continuation states — wakes her, picks the
//! curriculum (the prediction error says what to learn, and transcends into the
//! genome), and raises capture resolution at that moment. So `Surprise` carries the
//! expectation and the observation as data, never as prose.
//!
//! Pure: no clock, no I/O, no room, no persona runtime. `now_ms` is an input.

use airc_core::{MentionTarget, PeerId};
use uuid::Uuid;

use crate::cognition::channel_element::ChannelElement;

/// How loud a delta is, lowest to highest. `Ord` so the maximum across activities
/// and the comparison against the dial are plain comparisons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum SalienceLevel {
    /// Nothing above her cursor, or only her own words.
    Quiet,
    /// Others spoke or things moved, none of it to her.
    Notable,
    /// Addressed to her: a mention, a human speaking in her activity, a verdict on
    /// her work, a teammate waiting on her.
    Addressed,
    /// Passes every dial: a blocker on her held work, or the world contradicting
    /// what she expected.
    Urgent,
}

/// What she expected next, as her continuation states it (EVENT-MIND §6). The
/// typed half is what `Surprise` is judged against; `text` is for her own eyes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Expectation {
    pub text: String,
    /// When she expects it by. Past this with nothing observed is `Silent`.
    pub by_ms: Option<u64>,
    /// The verdict she expects on her held card, if any. A verdict that differs is
    /// a `Surprise`; one that matches is merely `Addressed`.
    pub verdict: Option<ExpectedVerdict>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExpectedVerdict {
    Passed,
    Failed,
}

/// Why the level is what it is. Typed so the strip, the recorder and the
/// curriculum lifter read the same reasons the wake policy read.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SalienceReason {
    MentionedMe { by: Uuid },
    HumanSpoke { who: Uuid },
    VerdictOnMyWork { card_id: Uuid, outcome: ObservedVerdict },
    TeammateWaitingOnMe { who: Uuid },
    MyCardMoved { card_id: Uuid, by: Uuid },
    Blocker { card_id: Uuid, what: String },
    /// The world broke with her expectation. The three-job signal.
    Surprise { expected: Expectation, observed: Observed },
    /// Past the time she expected something by, nothing came.
    Silent { expected_by_ms: u64, now_ms: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ObservedVerdict {
    Passed,
    Failed,
    Unknown,
}

/// What was observed when a `Surprise` fired, typed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Observed {
    Verdict { card_id: Uuid, outcome: ObservedVerdict },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Salience {
    pub level: SalienceLevel,
    pub reasons: Vec<SalienceReason>,
}

impl Salience {
    pub const QUIET: Salience = Salience { level: SalienceLevel::Quiet, reasons: Vec::new() };

    fn raise(&mut self, level: SalienceLevel, reason: SalienceReason) {
        if level > self.level {
            self.level = level;
        }
        self.reasons.push(reason);
    }
}

/// A typed board change above her cursor (the kanban view's delta). Kept minimal
/// here; the kanban `PerceivedView` produces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardChange {
    /// `review` is the review's own id: the identity a duplicate delivery is recognised by.
    Reviewed { card_id: Uuid, outcome: ObservedVerdict, reviewer: Uuid, review: Uuid },
    /// A card changed column (claimed, review, done...). Notable; Addressed on her own card.
    Moved { card_id: Uuid, by: Uuid },
    Blocked { card_id: Uuid, what: String },
    WaitingOnMe { card_id: Uuid, who: Uuid },
}

/// Everything about her the function needs, so it stays pure.
#[derive(Debug, Clone)]
pub struct Me<'a> {
    pub peer_id: PeerId,
    /// Peers who are humans (operator / web identities); a human speaking in her
    /// activity is addressed to the room's citizens in a way a peer's line is not.
    pub humans: &'a [Uuid],
    /// Cards she holds, so a verdict or a blocker on one is about her work.
    pub held_cards: &'a [Uuid],
}

/// One spoken line above her cursor, reduced to what salience needs. Built once
/// at the core's single inbound seam (`perception_feed`), never from a digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeechLine {
    pub sender: Uuid,
    pub addressed_to_me: bool,
    /// 0 = untimed (never "now").
    pub occurred_at_ms: u64,
}

impl SpeechLine {
    pub fn from_element(element: &ChannelElement, me: PeerId) -> Self {
        let event = element.event();
        Self {
            sender: element.sender_id(),
            addressed_to_me: matches!(event.target, MentionTarget::Peer(p) if p == me),
            occurred_at_ms: event.occurred_at_ms,
        }
    }

    pub fn from_event(event: &airc_core::TranscriptEvent, me: PeerId) -> Self {
        Self {
            sender: event.peer_id.as_uuid(),
            addressed_to_me: matches!(event.target, MentionTarget::Peer(p) if p == me),
            occurred_at_ms: event.occurred_at_ms,
        }
    }

    /// Addressed when the typed target names her OR the text mentions her by name
    /// (the ONE detector the loop uses, `PersonaIdentity::mentions`: word-boundary,
    /// identity-aware). Run 2 of the live acceptance (2026-10-04 19:24Z): an
    /// `@Kimi` typed by a peer arrives as `MentionTarget::All` with the mention in
    /// the text, and read as Notable, so she was not woken by her own name.
    pub fn from_event_for(
        event: &airc_core::TranscriptEvent,
        me: &super::persona_identity::PersonaIdentity,
    ) -> Self {
        let typed = matches!(event.target, MentionTarget::Peer(p) if p.as_uuid() == me.id);
        let named = event.body.as_ref().and_then(|b| b.as_text()).is_some_and(|t| me.mentions(t));
        Self {
            sender: event.peer_id.as_uuid(),
            addressed_to_me: typed || named,
            occurred_at_ms: event.occurred_at_ms,
        }
    }
}

/// The delta of one activity above her cursor, typed (EVENT-MIND §6).
#[derive(Debug, Clone, Default)]
pub struct ActivityDelta<'a> {
    pub speech: &'a [SpeechLine],
    pub board: &'a [BoardChange],
}

/// The salience of `delta` to `me`, against `expectation` (hers, from her
/// continuation), at `now_ms`.
pub fn salience(
    delta: &ActivityDelta<'_>,
    me: &Me<'_>,
    expectation: Option<&Expectation>,
    now_ms: u64,
) -> Salience {
    let mut s = Salience::QUIET;
    let mine = me.peer_id.as_uuid();

    for line in delta.speech {
        let sender = line.sender;
        if sender == mine {
            continue; // her own words never wake her
        }
        if s.level < SalienceLevel::Notable {
            s.level = SalienceLevel::Notable;
        }
        if line.addressed_to_me {
            s.raise(SalienceLevel::Addressed, SalienceReason::MentionedMe { by: sender });
        }
        if me.humans.contains(&sender) {
            s.raise(SalienceLevel::Addressed, SalienceReason::HumanSpoke { who: sender });
        }
    }

    for change in delta.board {
        match change {
            BoardChange::Reviewed { card_id, outcome, .. } if me.held_cards.contains(card_id) => {
                s.raise(
                    SalienceLevel::Addressed,
                    SalienceReason::VerdictOnMyWork { card_id: *card_id, outcome: *outcome },
                );
                if let Some(exp) = expectation {
                    let contradicted = match (exp.verdict, outcome) {
                        (Some(ExpectedVerdict::Passed), ObservedVerdict::Failed)
                        | (Some(ExpectedVerdict::Failed), ObservedVerdict::Passed) => true,
                        _ => false,
                    };
                    if contradicted {
                        s.raise(
                            SalienceLevel::Urgent,
                            SalienceReason::Surprise {
                                expected: exp.clone(),
                                observed: Observed::Verdict { card_id: *card_id, outcome: *outcome },
                            },
                        );
                    }
                }
            }
            BoardChange::Blocked { card_id, what } if me.held_cards.contains(card_id) => {
                s.raise(
                    SalienceLevel::Urgent,
                    SalienceReason::Blocker { card_id: *card_id, what: what.clone() },
                );
            }
            BoardChange::WaitingOnMe { who, .. } => {
                s.raise(SalienceLevel::Addressed, SalienceReason::TeammateWaitingOnMe { who: *who });
            }
            BoardChange::Moved { card_id, by } if *by != mine && me.held_cards.contains(card_id) => {
                s.raise(SalienceLevel::Addressed, SalienceReason::MyCardMoved { card_id: *card_id, by: *by });
            }
            BoardChange::Moved { by, .. } if *by == mine => {}
            BoardChange::Reviewed { .. } | BoardChange::Blocked { .. } | BoardChange::Moved { .. } => {
                if s.level < SalienceLevel::Notable {
                    s.level = SalienceLevel::Notable;
                }
            }
        }
    }

    if let Some(exp) = expectation {
        if let Some(by) = exp.by_ms {
            let nothing_came = delta.speech.is_empty() && delta.board.is_empty();
            if nothing_came && now_ms > by {
                s.raise(SalienceLevel::Addressed, SalienceReason::Silent { expected_by_ms: by, now_ms });
            }
        }
    }

    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn element(from: Uuid, target: MentionTarget, _text: &str) -> SpeechLine {
        SpeechLine { sender: from, addressed_to_me: matches!(target, MentionTarget::Peer(p) if p == PeerId::from_uuid(ME)), occurred_at_ms: 1_000 }
    }

    const ME: Uuid = Uuid::from_u128(0x1);
    const PEER: Uuid = Uuid::from_u128(0x2);
    const JOEL: Uuid = Uuid::from_u128(0x3);
    const CARD: Uuid = Uuid::from_u128(0x9);

    fn me<'a>(humans: &'a [Uuid], held: &'a [Uuid]) -> Me<'a> {
        Me { peer_id: PeerId::from_uuid(ME), humans, held_cards: held }
    }

    // what this catches: the wake policy's inputs. Her own words are Quiet; a peer's
    // line is Notable; a mention or a human is Addressed. If a mention ever reads as
    // Notable, a Deep dial would sleep through Joel calling her by name.
    #[test]
    fn mention_and_human_are_addressed_peer_is_notable_self_is_quiet() {
        let own = [element(ME, MentionTarget::All, "thinking")];
        assert_eq!(salience(&ActivityDelta { speech: &own, board: &[] }, &me(&[], &[]), None, 0).level, SalienceLevel::Quiet);

        let peer = [element(PEER, MentionTarget::All, "hi all")];
        assert_eq!(salience(&ActivityDelta { speech: &peer, board: &[] }, &me(&[], &[]), None, 0).level, SalienceLevel::Notable);

        let mention = [element(PEER, MentionTarget::Peer(PeerId::from_uuid(ME)), "@her")];
        let s = salience(&ActivityDelta { speech: &mention, board: &[] }, &me(&[], &[]), None, 0);
        assert_eq!(s.level, SalienceLevel::Addressed);
        assert!(matches!(s.reasons[0], SalienceReason::MentionedMe { by } if by == PEER));

        let human = [element(JOEL, MentionTarget::All, "how is it going")];
        let s = salience(&ActivityDelta { speech: &human, board: &[] }, &me(&[JOEL], &[]), None, 0);
        assert_eq!(s.level, SalienceLevel::Addressed);
        assert!(matches!(s.reasons[0], SalienceReason::HumanSpoke { who } if who == JOEL));
    }

    // what this catches: the three-job signal. A verdict on her card is Addressed;
    // the SAME verdict contradicting her stated expectation is Urgent with a typed
    // Surprise carrying both sides, which is what the recorder and the curriculum
    // lifter key on. A verdict on someone else's card is only Notable.
    #[test]
    fn a_verdict_against_her_expectation_is_a_surprise() {
        let reviewed = [BoardChange::Reviewed { card_id: CARD, outcome: ObservedVerdict::Failed, reviewer: PEER, review: Uuid::from_u128(0x5e) }];
        let held = [CARD];
        let s = salience(&ActivityDelta { speech: &[], board: &reviewed }, &me(&[], &held), None, 0);
        assert_eq!(s.level, SalienceLevel::Addressed);

        let expected_pass = Expectation { text: "review passes; then deploy".into(), by_ms: None, verdict: Some(ExpectedVerdict::Passed) };
        let s = salience(&ActivityDelta { speech: &[], board: &reviewed }, &me(&[], &held), Some(&expected_pass), 0);
        assert_eq!(s.level, SalienceLevel::Urgent);
        assert!(s.reasons.iter().any(|r| matches!(r,
            SalienceReason::Surprise { expected, observed: Observed::Verdict { card_id, outcome: ObservedVerdict::Failed } }
            if *card_id == CARD && expected.verdict == Some(ExpectedVerdict::Passed))));

        let expected_fail = Expectation { verdict: Some(ExpectedVerdict::Failed), ..expected_pass.clone() };
        let s = salience(&ActivityDelta { speech: &[], board: &reviewed }, &me(&[], &held), Some(&expected_fail), 0);
        assert_eq!(s.level, SalienceLevel::Addressed, "a confirmed expectation is not a surprise");

        let s = salience(&ActivityDelta { speech: &[], board: &reviewed }, &me(&[], &[]), Some(&expected_pass), 0);
        assert_eq!(s.level, SalienceLevel::Notable, "someone else's verdict: {s:?}");
    }

    // what this catches: silence as a signal. Past the time she expected a reply
    // with nothing in the delta, the activity is Addressed (she should look), not
    // Quiet; and a blocker on her held card is Urgent regardless of her dial.
    #[test]
    fn silence_past_her_deadline_and_a_blocker_on_her_card() {
        let exp = Expectation { text: "Cormac replies".into(), by_ms: Some(10_000), verdict: None };
        let s = salience(&ActivityDelta::default(), &me(&[], &[]), Some(&exp), 20_000);
        assert_eq!(s.level, SalienceLevel::Addressed);
        assert!(matches!(s.reasons[0], SalienceReason::Silent { expected_by_ms: 10_000, now_ms: 20_000 }));
        assert_eq!(salience(&ActivityDelta::default(), &me(&[], &[]), Some(&exp), 5_000).level, SalienceLevel::Quiet);

        let blocked = [BoardChange::Blocked { card_id: CARD, what: "CI red".into() }];
        let held = [CARD];
        assert_eq!(salience(&ActivityDelta { speech: &[], board: &blocked }, &me(&[], &held), None, 0).level, SalienceLevel::Urgent);
        assert_eq!(salience(&ActivityDelta { speech: &[], board: &blocked }, &me(&[], &[]), None, 0).level, SalienceLevel::Notable);
    }
}
