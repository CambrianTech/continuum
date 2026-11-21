//! Her senses as a worker: what reaches her, what wakes her, and that a wake she took stops
//! calling. Driven through the worker's own input, so no test shares the core's one feed.

use std::sync::Arc;

use airc_core::{PeerId, TranscriptEvent};
use continuum_airc_protocol::ConversationEvent;
use continuum_sage::dataflow::{Event, InputRevision, Reader, TopicName};

use tokio::sync::{oneshot, watch};
use uuid::Uuid;

use super::feed::{classify, Arrival, Fed};
use super::worker::Sensed;
use crate::cognition::attention::{AttentionDial, Continuation};
use crate::cognition::conversation::salience::{BoardChange, Expectation, ObservedVerdict, SalienceLevel, SalienceReason};
use super::Calling;
use crate::cognition::conversation::Wake;
use crate::persona::awareness::AwarenessSnapshot;
use crate::persona::perception_region::PerceptionRegion;

const ROOM_A: Uuid = Uuid::from_u128(0xa);
const ROOM_B: Uuid = Uuid::from_u128(0xb);
const KIMI: Uuid = Uuid::from_u128(0x1);
const PEER: Uuid = Uuid::from_u128(0x2);
const HUMAN: Uuid = Uuid::from_u128(0x3);
const CARD: Uuid = Uuid::from_u128(0x9);

fn speech(room: Uuid, from: Uuid, text: &str) -> TranscriptEvent {
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
        body: Some(airc_core::Body::text(text)),
        attachment: None,
        receipt: None,
        metadata: serde_json::Value::Null,
    }
}

fn media() -> airc_blobs::MediaRef {
    airc_blobs::MediaRef { hash: airc_blobs::ContentHash::from_hex(&"b".repeat(64)).expect("64 hex digits"), size_bytes: 12, mime: None } // JUSTIFIED: a fixed 64-hex-digit hash always parses
}

fn reviewed(room: Uuid, reviewer: Uuid) -> TranscriptEvent {
    let review = airc_work::WorkSubmissionReview {
        review_id: airc_work::WorkReviewId::new(),
        card_id: airc_work::WorkCardId::from_uuid(CARD),
        submission_id: airc_work::SubmissionId::new(),
        artifact: media(),
        review_card_id: airc_work::WorkCardId::new(),
        review_claim_id: airc_work::ClaimId::new(),
        reviewer: PeerId::from_uuid(reviewer),
        outcome: airc_work::WorkReviewOutcome::Passed,
        evidence: media(),
        evidence_text: None,
        reviewed_at_ms: 6_000,
    };
    let (headers, body) = airc_work::encode_work_event(&airc_work::WorkEvent::WorkSubmissionReviewed(review)).expect("a review encodes"); // JUSTIFIED: a well-formed review always encodes; a failure is the fixture's own bug
    let mut event = speech(room, reviewer, "");
    event.kind = airc_core::TranscriptKind::System;
    event.headers = headers;
    event.body = Some(body);
    event
}

/// A room event as the one feed hands it to her senses: classified once.
fn heard(event: TranscriptEvent, through: u64) -> Sensed {
    let event: ConversationEvent = match continuum_airc_protocol::classify_inbound(Arc::new(event)) {
        continuum_airc_protocol::InboundEvent::Conversation(conversation) => conversation,
        other => panic!("a room line is conversation: {other:?}"),
    };
    let fed = classify(event.event());
    Sensed::Heard(Event { value: Arc::new(Arrival { event, fed }), revision: InputRevision { topic: TopicName("mind/feed"), through } })
}

struct Kimi {
    resident: super::Resident,
    calling: Reader<Calling>,
    strip: watch::Receiver<AwarenessSnapshot>,
    /// Her mind's home, where her acts save.
    home: std::path::PathBuf,
}

impl Kimi {
    fn start(me: Uuid) -> Self {
        let dir = tempfile::tempdir().expect("a temp dir"); // JUSTIFIED: a test without a temp dir has nothing to run on
        let (mut region, strip) = PerceptionRegion::boot(PeerId::from_uuid(me), dir.path(), 1_000, 0);
        let home = dir.path().to_path_buf();
        std::mem::forget(dir);
        region.set_identity_facts(vec![HUMAN], vec![CARD]);
        region.join(ROOM_A, "career-wrangler");
        region.set_dial(AttentionDial::default(), 0);
        let resident = super::start(me, "Kimi", region, strip.clone()).expect("a fresh mind's calling topic is unclaimed"); // JUSTIFIED: her bus is new here, so nothing has claimed it
        let calling = resident.handle().calling();
        Self { resident, calling, strip, home }
    }

    async fn tell(&self, act: Sensed) {
        assert!(self.resident.handle().tell(act).await, "her senses are running");
    }

    /// Everything told before this has been handled: her act's answer comes back in order.
    async fn settled(&self) {
        let (saved, answer) = oneshot::channel();
        self.tell(Sensed::Continuation { continuation: None, dir: self.home.clone(), saved }).await;
        assert!(answer.await.is_ok(), "her senses answer her act");
    }

    async fn next_wake(&mut self) -> Option<Wake> {
        self.calling.changed().await.map(|snapshot| snapshot.value.wake.clone()).ok().flatten()
    }

    /// What calls her as her senses last published it.
    fn calling_now(&self) -> Option<Wake> {
        self.calling.latest().and_then(|snapshot| snapshot.value.wake.clone())
    }

    fn unread(&self, room: Uuid) -> Option<u32> {
        self.strip.borrow().lines.iter().find(|line| line.activity == room).map(|line| line.unread)
    }
}

// what this catches: the one feed's contract, now on her senses worker. A line in a room she is
// not in reaches nothing; a human's line in her room calls her (Addressed); a typed PASS on her
// held card from another peer is a board change that calls her (the 2026-10-04 defect: a PASS
// dropped as non_chat_schema); her own review echo is not news; a peer's "@Kimi" in the text is
// Addressed by her name. Once she perceives the room it stops calling.
#[tokio::test]
async fn her_senses_take_only_her_rooms_and_typed_verdicts_call_her() {
    let mut kimi = Kimi::start(KIMI);
    kimi.tell(heard(speech(ROOM_B, HUMAN, "how is it going?"), 1)).await;
    kimi.settled().await;
    assert_eq!(kimi.unread(ROOM_B), None, "not her room");

    kimi.tell(heard(speech(ROOM_A, HUMAN, "how is it going?"), 2)).await;
    match kimi.next_wake().await {
        Some(Wake::Perceive { activity, salience }) => {
            assert_eq!(activity, ROOM_A);
            assert_eq!(salience.level, SalienceLevel::Addressed);
        }
        other => panic!("a human in her room calls her: {other:?}"),
    }

    assert!(matches!(
        classify(match &heard(reviewed(ROOM_A, PEER), 0) { Sensed::Heard(e) => e.value.event.event(), _ => unreachable!() }),
        Fed::Board(BoardChange::Reviewed { card_id: CARD, outcome: ObservedVerdict::Passed, reviewer, .. }) if reviewer == PEER
    ));
    kimi.tell(heard(reviewed(ROOM_A, PEER), 3)).await;
    kimi.settled().await;
    assert_eq!(kimi.unread(ROOM_A), Some(2), "the verdict on her card is perceived");
    kimi.tell(heard(reviewed(ROOM_A, KIMI), 4)).await;
    kimi.settled().await;
    assert_eq!(kimi.unread(ROOM_A), Some(2), "her own review echo is not news");

    kimi.tell(Sensed::Perceived { room: ROOM_A, cursor: Default::default() }).await;
    kimi.settled().await;
    assert_eq!(kimi.calling_now(), None, "perceived: the room stops calling");

    kimi.tell(heard(speech(ROOM_A, PEER, "@Kimi your slice has a typed PASS on record"), 5)).await;
    kimi.settled().await;
    match kimi.calling_now() {
        Some(Wake::Perceive { salience, .. }) => {
            assert_eq!(salience.level, SalienceLevel::Addressed);
            assert!(salience.reasons.iter().any(|r| matches!(r, SalienceReason::MentionedMe { by } if *by == PEER)));
        }
        other => panic!("her name in the text is addressed to her: {other:?}"),
    }
}

// what this catches: a clock deciding for her. Her continuation's deadline is the one timer her
// senses keep: when it passes it calls her, with no poll; once she takes it, it stops calling
// until she sets a new one (it used to re-fire on every floor check).
#[tokio::test]
async fn her_deadline_calls_her_once_and_stops_when_she_takes_it() {
    let mut kimi = Kimi::start(Uuid::from_u128(0x4));
    let by_ms = continuum_base::time::EpochMs::now().millis() + 30;
    let (saved, answer) = oneshot::channel();
    let continuation = Continuation {
        activity: ROOM_A,
        note: "submitted; expect a pass".into(),
        expectation: Some(Expectation { text: "review passes".into(), by_ms: Some(by_ms), verdict: None }),
        written_at_ms: 0,
    };
    kimi.tell(Sensed::Continuation { continuation: Some(continuation), dir: kimi.home.clone(), saved }).await;
    assert!(answer.await.is_ok());
    assert_eq!(kimi.next_wake().await, Some(Wake::Continuation), "her deadline passed");
    kimi.tell(Sensed::Took(Wake::Continuation)).await;
    assert_eq!(kimi.next_wake().await, None, "taken: it stops calling");
}
