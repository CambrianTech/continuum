//! One feed: the core's single inbound seam feeds every resident persona's
//! perception region. No persona subscribes to rooms for perception.
//!
//! EVENT-MIND.md §1b and the 2026-10-04 one-feed decision (Cormac, BigMama,
//! Fable): the core already holds one attach per room for the positron
//! projector; that feed IS the one truth, and her perception renders it. The
//! per-persona channel-set attach (#1523) had a persona subscription that never
//! received other peers' durable pushes on the 5090 while the projector on the
//! same daemon did; instead of debugging a second feed, there is one.
//!
//! `feed` is called from `airc::inbound_attach::publish_transcript_event`, the
//! one place every room event crosses once per core. For each resident persona
//! who is a member of the event's room it classifies the event once (speech or a
//! typed work fact), hands the region the line or the board change, and if the
//! region says the persona should wake, sends the `Wake` on her channel: the
//! single call site the loop consumes (phase 2, BigMama).
//!
//! A registry, not a bus subscriber: the seam is synchronous per event and must
//! stay cheap. Regions are behind a `Mutex` because `observe_*` mutates; the lock
//! is held for one classification, never across an await.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use airc_core::{PeerId, TranscriptEvent};
use tokio::sync::mpsc;
use uuid::Uuid;

use super::perception_region::{PerceptionRegion, Wake};
use super::salience::{BoardChange, ObservedVerdict, SpeechLine};
use crate::airc::realtime_wire::{room_work_from_event, RoomWork};

struct Resident {
    peer: PeerId,
    region: Arc<Mutex<PerceptionRegion>>,
    wake_tx: mpsc::Sender<Wake>,
}

fn registry() -> &'static Mutex<HashMap<Uuid, Resident>> {
    static G: OnceLock<Mutex<HashMap<Uuid, Resident>>> = OnceLock::new();
    G.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A persona became resident on this core: her region now receives the feed.
/// `wake_tx` is where her wakes go; a full channel drops the wake (the region's
/// salience persists, so the next event or her idle clip re-raises it).
pub fn register(persona: Uuid, peer: PeerId, region: Arc<Mutex<PerceptionRegion>>, wake_tx: mpsc::Sender<Wake>) {
    registry().lock().unwrap_or_else(|p| p.into_inner()).insert(persona, Resident { peer, region, wake_tx });
    crate::probe!(class = "mind.feed.registered", persona = %persona, "her perception region is on the one feed");
}

pub fn unregister(persona: Uuid) {
    registry().lock().unwrap_or_else(|p| p.into_inner()).remove(&persona);
}

/// What one event is, classified once for every resident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fed {
    Speech,
    Board(BoardChange),
    /// A work fact the mind does not yet render as a board change.
    OtherWork(String),
    NotPerception(&'static str),
}

/// Classify once: a typed work fact first, else speech if the event is a room
/// line, else not perception. Pure.
pub fn classify(event: &TranscriptEvent) -> Fed {
    match room_work_from_event(event) {
        Ok(Some(RoomWork::Reviewed { card_id, outcome, reviewer })) => {
            Fed::Board(BoardChange::Reviewed { card_id, outcome, reviewer })
        }
        Ok(Some(RoomWork::StateChanged { card_id, by, .. })) => Fed::Board(BoardChange::Moved { card_id, by }),
        Ok(Some(RoomWork::Claimed { card_id, owner })) => Fed::Board(BoardChange::Moved { card_id, by: owner }),
        Ok(Some(RoomWork::Submitted { card_id, publisher })) => Fed::Board(BoardChange::Moved { card_id, by: publisher }),
        Ok(Some(RoomWork::Other { kind })) => Fed::OtherWork(kind),
        Err(reason) => Fed::NotPerception(reason),
        Ok(None) => match crate::airc::realtime_wire::room_content_from_event(event) {
            Ok(_) => Fed::Speech,
            Err(reason) => Fed::NotPerception(reason),
        },
    }
}

/// Feed one room event to every resident persona who is in that room. Returns
/// how many regions were fed (for the seam's probe).
pub fn feed(event: &TranscriptEvent, now_ms: u64) -> usize {
    let fed = classify(event);
    let room = event.room_id.as_uuid();
    let mut count = 0;
    let residents = registry().lock().unwrap_or_else(|p| p.into_inner());
    for (persona, resident) in residents.iter() {
        let wake = {
            let mut region = resident.region.lock().unwrap_or_else(|p| p.into_inner());
            if !region.is_in(room) {
                continue;
            }
            count += 1;
            match &fed {
                Fed::Speech => region.observe_speech(room, SpeechLine::from_event(event, resident.peer), now_ms),
                Fed::Board(change) => {
                    // Her own act's echo is hers, not news (the same rule as her words).
                    let actor = match change {
                        BoardChange::Reviewed { reviewer, .. } => *reviewer,
                        BoardChange::Moved { by, .. } => *by,
                        BoardChange::Blocked { .. } | BoardChange::WaitingOnMe { .. } => Uuid::nil(),
                    };
                    if actor == resident.peer.as_uuid() {
                        None
                    } else {
                        region.observe_board(room, vec![change.clone()], now_ms)
                    }
                }
                Fed::OtherWork(_) | Fed::NotPerception(_) => None,
            }
        };
        if let Some(wake) = wake {
            match resident.wake_tx.try_send(wake) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(_)) => crate::probe!(
                    class = "mind.feed.wake_dropped",
                    persona = %persona,
                    "her wake channel is full; salience persists and re-raises on the next event"
                ),
                Err(mpsc::error::TrySendError::Closed(_)) => {}
            }
        }
    }
    count
}

/// Typed outcome for the admit path's board-fact rendering (keeps one mapping).
pub fn verdict_word(outcome: ObservedVerdict) -> &'static str {
    match outcome {
        ObservedVerdict::Passed => "PASSED",
        ObservedVerdict::Failed => "FAILED",
        ObservedVerdict::Unknown => "UNKNOWN",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::attention::AttentionDial;
    use crate::persona::salience::SalienceLevel;

    const ROOM_A: Uuid = Uuid::from_u128(0xa);
    const ROOM_B: Uuid = Uuid::from_u128(0xb);
    const KIMI: Uuid = Uuid::from_u128(0x1);
    const JOEL: Uuid = Uuid::from_u128(0x3);
    const CARD: Uuid = Uuid::from_u128(0x9);

    fn speech(room: Uuid, from: Uuid) -> TranscriptEvent {
        TranscriptEvent {
            event_id: airc_core::EventId::new(),
            room_id: airc_core::RoomId::from_uuid(room),
            peer_id: PeerId::from_uuid(from),
            client_id: airc_core::ClientId::new(),
            kind: airc_core::TranscriptKind::Message,
            occurred_at_ms: 5_000,
            lamport: 7,
            target: airc_core::MentionTarget::All,
            headers: Default::default(),
            body: Some(airc_core::Body::text("how is it going?")),
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        }
    }

    fn reviewed(room: Uuid, reviewer: Uuid) -> TranscriptEvent {
        let review = airc_work::WorkSubmissionReview {
            review_id: airc_work::WorkReviewId::new(),
            card_id: airc_work::WorkCardId::from_uuid(CARD),
            submission_id: airc_work::SubmissionId::new(),
            artifact: test_media(),
            review_card_id: airc_work::WorkCardId::new(),
            review_claim_id: airc_work::ClaimId::new(),
            reviewer: PeerId::from_uuid(reviewer),
            outcome: airc_work::WorkReviewOutcome::Passed,
            evidence: test_media(),
            reviewed_at_ms: 6_000,
        };
        let (headers, body) = airc_work::encode_work_event(&airc_work::WorkEvent::WorkSubmissionReviewed(review)).unwrap();
        let mut e = speech(room, reviewer);
        e.kind = airc_core::TranscriptKind::System;
        e.headers = headers;
        e.body = Some(body);
        e
    }

    fn test_media() -> airc_blobs::MediaRef {
        serde_json::from_value(serde_json::json!({"hash": "b".repeat(64), "size_bytes": 12})).unwrap()
    }

    fn resident(dial: AttentionDial) -> (Arc<Mutex<PerceptionRegion>>, mpsc::Receiver<Wake>) {
        let dir = tempfile::tempdir().unwrap();
        let (mut region, _rx) = PerceptionRegion::boot(PeerId::from_uuid(KIMI), dir.path(), 1_000, 0);
        std::mem::forget(dir);
        region.set_identity_facts(vec![JOEL], vec![CARD]);
        region.join(ROOM_A, "career-wrangler");
        region.set_dial(dial, 0);
        let _ = region.wake_for(0); // consume Resume (fresh: none)
        let region = Arc::new(Mutex::new(region));
        let (tx, rx) = mpsc::channel(4);
        register(KIMI, PeerId::from_uuid(KIMI), region.clone(), tx);
        (region, rx)
    }

    // what this catches: the one feed. A human's line in a room she is in wakes her
    // (Addressed under Normal); the same line in a room she is NOT in feeds nothing;
    // a typed PASS on her held card from another peer arrives as a board change and
    // wakes her; her own review echo does not. This is the 2026-10-04 defect (a PASS
    // dropped as non_chat_schema) as a gate, on the seam the fix lives at.
    #[test]
    fn the_feed_reaches_only_her_rooms_and_typed_verdicts_wake_her() {
        let (_region, mut rx) = resident(AttentionDial::default());
        assert_eq!(feed(&speech(ROOM_B, JOEL), 10), 0, "not her room");
        assert!(rx.try_recv().is_err());

        assert_eq!(feed(&speech(ROOM_A, JOEL), 11), 1);
        match rx.try_recv() {
            Ok(Wake::Perceive { activity, salience }) => {
                assert_eq!(activity, ROOM_A);
                assert_eq!(salience.level, SalienceLevel::Addressed);
            }
            other => panic!("a human in her room wakes her: {other:?}"),
        }

        let reviewer = Uuid::from_u128(0x2);
        assert_eq!(classify(&reviewed(ROOM_A, reviewer)), Fed::Board(BoardChange::Reviewed { card_id: CARD, outcome: ObservedVerdict::Passed, reviewer }));
        assert_eq!(feed(&reviewed(ROOM_A, reviewer), 12), 1);
        match rx.try_recv() {
            Ok(Wake::Perceive { activity, salience }) => {
                assert_eq!(activity, ROOM_A);
                assert!(salience.level >= SalienceLevel::Addressed, "{salience:?}");
            }
            other => panic!("a typed verdict on her card wakes her: {other:?}"),
        }

        assert_eq!(feed(&reviewed(ROOM_A, KIMI), 13), 1, "fed, but her own echo");
        assert!(rx.try_recv().is_err(), "her own review echo is not news");
        unregister(KIMI);
    }
}
