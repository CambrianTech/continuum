//! The perception region: her one view of all her activities, and what wakes her.
//!
//! EVENT-MIND.md §1b, §3, §6. This region owns no truth; it holds a handle onto
//! each activity's truth (the room's `ChannelDigest`, its board changes), her
//! cursor into it, the delta above the cursor and that delta's salience. From
//! those it folds the awareness strip (published as a `watch` snapshot) and
//! answers one question: *should she wake now, and for what?*
//!
//! Fed by the bus, never by polling: the inbound attach path calls [`observe_chat`]
//! when an activity's transcript advances and [`observe_board`] when its board
//! changes. Nothing here reads a room on a timer. Her acts, and only her acts,
//! call [`set_continuation`] and [`set_dial`]. After a turn, [`perceived`] moves
//! her cursor for what she actually looked at; nothing else moves it.
//!
//! Untimed events (`occurred_at_ms == 0`, seen in 110 of 126 lines of a live
//! prompt on 2026-10-04) are never treated as "now": they order by lamport and
//! do not feed recency or `Silent`.
//!
//! [`observe_chat`]: PerceptionRegion::observe_chat
//! [`observe_board`]: PerceptionRegion::observe_board
//! [`set_continuation`]: PerceptionRegion::set_continuation
//! [`set_dial`]: PerceptionRegion::set_dial
//! [`perceived`]: PerceptionRegion::perceived

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use airc_core::PeerId;
use tokio::sync::watch;
use uuid::Uuid;

use super::attention::{AttentionDial, Continuation};
use super::awareness::{self, ActivityLine, AwarenessSnapshot};
use super::mind_state::{ActivityCursor, MindState, MindStateError};
use super::salience::{self, ActivityDelta, BoardChange, Me, Salience, SalienceLevel};
use crate::cognition::channel_digest::ChannelDigest;

/// Why she is being woken. The activity rides on the wake; no room is implied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wake {
    /// An activity's delta is loud enough for her dial.
    Perceive { activity: Uuid, salience: Salience },
    /// The time she expected something by has passed.
    Continuation,
    /// First wake after a restart: continue what she was doing.
    Resume,
}

/// One activity as she perceives it (EVENT-MIND §6 `ActivityView`), borrowing the
/// truth; nothing copied.
#[derive(Clone)]
pub struct ActivityView {
    pub name: String,
    pub digest: Option<Arc<ChannelDigest>>,
    /// Board changes above her cursor, typed (the kanban view's delta).
    pub board: Vec<BoardChange>,
    /// Newest TIMED event above her cursor; untimed events do not set this.
    pub last_activity_ms: Option<u64>,
    pub salience: Salience,
}

impl std::fmt::Debug for ActivityView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActivityView")
            .field("name", &self.name)
            .field("unread", &self.unread().len())
            .field("board", &self.board)
            .field("last_activity_ms", &self.last_activity_ms)
            .field("salience", &self.salience)
            .finish()
    }
}

impl ActivityView {
    fn unread(&self) -> &[Arc<ChannelElementArc>] {
        self.digest.as_ref().map(|d| d.unread()).unwrap_or(&[])
    }
}
type ChannelElementArc = crate::cognition::channel_element::ChannelElement;

pub struct PerceptionRegion {
    me: PeerId,
    humans: Vec<Uuid>,
    held_cards: Vec<Uuid>,
    state: MindState,
    views: BTreeMap<Uuid, ActivityView>,
    strip: watch::Sender<AwarenessSnapshot>,
    turn_budget_tokens: u32,
    resumed: bool,
}

impl PerceptionRegion {
    /// Boot: load her durable state from `peer_dir`. If a continuation was saved,
    /// the first [`wake_for`](Self::wake_for) is `Wake::Resume`.
    pub fn boot(me: PeerId, peer_dir: &Path, turn_budget_tokens: u32, now_ms: u64) -> (Self, watch::Receiver<AwarenessSnapshot>) {
        let state = MindState::load(peer_dir);
        let resumed = state.continuation.is_none();
        if !resumed {
            crate::probe!(
                class = "mind.resume",
                persona = %me,
                activity = %state.continuation.as_ref().map(|c| c.activity.to_string()).unwrap_or_default(),
                "a saved continuation: her first wake continues it"
            );
        }
        let snapshot = awareness::fold(Vec::new(), state.continuation.clone(), state.dial, turn_budget_tokens, now_ms);
        let (strip, rx) = watch::channel(snapshot);
        (
            Self { me, humans: Vec::new(), held_cards: Vec::new(), state, views: BTreeMap::new(), strip, turn_budget_tokens, resumed },
            rx,
        )
    }

    /// Who the humans are and which cards she holds: inputs to salience, refreshed
    /// by the roster and board sources, never read from a room here.
    pub fn set_identity_facts(&mut self, humans: Vec<Uuid>, held_cards: Vec<Uuid>) {
        self.humans = humans;
        self.held_cards = held_cards;
    }

    /// A membership (an activity she is in). Idempotent; named so the strip can
    /// say "career-wrangler", not an id.
    pub fn join(&mut self, activity: Uuid, name: impl Into<String>) {
        self.views.entry(activity).or_insert_with(|| ActivityView {
            name: name.into(),
            digest: None,
            board: Vec::new(),
            last_activity_ms: None,
            salience: Salience::QUIET,
        });
    }

    /// She left an activity: its line leaves the strip; her cursor is kept.
    pub fn leave(&mut self, activity: Uuid) {
        self.views.remove(&activity);
    }

    /// The activity's transcript advanced (bus-fed). Recomputes its delta and
    /// salience and republishes the strip. Returns the wake it would cause, if any.
    pub fn observe_chat(&mut self, activity: Uuid, digest: Arc<ChannelDigest>, now_ms: u64) -> Option<Wake> {
        let view = self.views.entry(activity).or_insert_with(|| ActivityView {
            name: short8(activity),
            digest: None,
            board: Vec::new(),
            last_activity_ms: None,
            salience: Salience::QUIET,
        });
        view.last_activity_ms = digest
            .unread()
            .iter()
            .map(|e| e.event().occurred_at_ms)
            .filter(|ms| *ms > 0) // untimed is never "now"
            .max()
            .or(view.last_activity_ms);
        view.digest = Some(digest);
        self.recompute(activity, now_ms)
    }

    /// The activity's board changed above her cursor (bus-fed).
    pub fn observe_board(&mut self, activity: Uuid, changes: Vec<BoardChange>, now_ms: u64) -> Option<Wake> {
        let view = self.views.entry(activity).or_insert_with(|| ActivityView {
            name: short8(activity),
            digest: None,
            board: Vec::new(),
            last_activity_ms: None,
            salience: Salience::QUIET,
        });
        view.board.extend(changes);
        view.last_activity_ms = Some(now_ms);
        self.recompute(activity, now_ms)
    }

    fn recompute(&mut self, activity: Uuid, now_ms: u64) -> Option<Wake> {
        let me = Me { peer_id: self.me, humans: &self.humans, held_cards: &self.held_cards };
        let expectation = self
            .state
            .continuation
            .as_ref()
            .filter(|c| c.activity == activity)
            .and_then(|c| c.expectation.as_ref());
        let view = self.views.get_mut(&activity)?;
        let unread = view.unread();
        let delta = ActivityDelta { unread, board: &view.board };
        view.salience = salience::salience(&delta, &me, expectation, now_ms);
        let salience = view.salience.clone();
        self.publish(now_ms);
        let admitted = self.state.dial.admits(salience.level);
        crate::probe!(
            class = if admitted { "mind.perceive.wake" } else { "mind.perceive.quiet" },
            persona = %self.me,
            activity = %short8(activity),
            level = ?salience.level,
            reasons = salience.reasons.len(),
            dial = ?self.state.dial.depth,
            "an activity's delta against her dial"
        );
        admitted.then_some(Wake::Perceive { activity, salience })
    }

    /// What should wake her now, loudest first; `Resume` once after boot; her own
    /// continuation when its time has passed. `None` = nothing; idle is a slow
    /// clip at which she may muse by choice, decided by the loop, not here.
    pub fn wake_for(&mut self, now_ms: u64) -> Option<Wake> {
        if !self.resumed {
            self.resumed = true;
            return Some(Wake::Resume);
        }
        let loudest = self
            .views
            .iter()
            .filter(|(_, v)| self.state.dial.admits(v.salience.level) && v.salience.level > SalienceLevel::Quiet)
            .max_by(|a, b| a.1.salience.level.cmp(&b.1.salience.level).then(a.1.last_activity_ms.cmp(&b.1.last_activity_ms)));
        if let Some((activity, view)) = loudest {
            return Some(Wake::Perceive { activity: *activity, salience: view.salience.clone() });
        }
        if self.state.continuation.as_ref().is_some_and(|c| c.is_due(now_ms)) {
            return Some(Wake::Continuation);
        }
        None
    }

    /// After a turn: she perceived `activity` through `cursor`. Her cursor moves
    /// (never backwards), the delta clears, the strip updates. Only this moves a
    /// cursor; a turn that did not look at an activity leaves it unread.
    pub fn perceived(&mut self, activity: Uuid, cursor: &ActivityCursor, now_ms: u64) {
        self.state.perceived(activity, cursor);
        if let Some(view) = self.views.get_mut(&activity) {
            view.board.clear();
            view.salience = Salience::QUIET;
            // The digest handle stays (it is the truth); the next observe_chat brings
            // a digest whose unread starts above this cursor.
        }
        self.publish(now_ms);
    }

    /// Her act: what she is working on and what she expects next.
    pub fn set_continuation(&mut self, continuation: Option<Continuation>, now_ms: u64) {
        crate::probe!(class = "mind.continuation.written", persona = %self.me, present = continuation.is_some(), "written by her act");
        self.state.continuation = continuation;
        self.publish(now_ms);
    }

    /// Her act: how wide the door is.
    pub fn set_dial(&mut self, dial: AttentionDial, now_ms: u64) {
        crate::probe!(class = "mind.dial.set", persona = %self.me, depth = ?dial.depth, pass = ?dial.pass, "set by her act");
        self.state.dial = dial;
        self.publish(now_ms);
    }

    pub fn continuation(&self) -> Option<&Continuation> {
        self.state.continuation.as_ref()
    }

    pub fn dial(&self) -> AttentionDial {
        self.state.dial
    }

    pub fn cursor(&self, activity: Uuid) -> Option<&ActivityCursor> {
        self.state.cursors.get(&activity)
    }

    /// Save her durable state (the seam: deploy stop, periodic, after each turn).
    pub fn save(&self, peer_dir: &Path, now_ms: u64) -> Result<(), MindStateError> {
        self.state.save(peer_dir, now_ms)
    }

    fn publish(&self, now_ms: u64) {
        let lines = self
            .views
            .iter()
            .map(|(id, v)| ActivityLine {
                activity: *id,
                name: v.name.clone(),
                unread: (v.unread().len() + v.board.len()) as u32,
                salience: v.salience.clone(),
                last_activity_ms: v.last_activity_ms.unwrap_or(0),
                waiting_on_me: v
                    .board
                    .iter()
                    .filter_map(|c| match c {
                        BoardChange::WaitingOnMe { who, .. } => Some(*who),
                        _ => None,
                    })
                    .collect(),
            })
            .collect();
        let snapshot = awareness::fold(lines, self.state.continuation.clone(), self.state.dial, self.turn_budget_tokens, now_ms);
        crate::probe!(
            class = "mind.load",
            persona = %self.me,
            live = snapshot.load.live_activities,
            unread = snapshot.load.unread_total,
            context_share = snapshot.load.context_share,
            "shown to her, never a cap"
        );
        let _ = self.strip.send(snapshot); // no receiver = nobody rendering yet; not an error
    }
}

fn short8(id: Uuid) -> String {
    id.to_string()[..8].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::channel_digest::ChannelDigestBuilder;
    use crate::cognition::channel_element::ChannelElementCache;
    use crate::cognition::embedding::LexicalEmbedder;
    use crate::persona::salience::{Expectation, ExpectedVerdict, ObservedVerdict, SalienceReason};

    const ME: Uuid = Uuid::from_u128(0x1);
    const PEER: Uuid = Uuid::from_u128(0x2);
    const JOEL: Uuid = Uuid::from_u128(0x3);
    const A: Uuid = Uuid::from_u128(0xa);
    const B: Uuid = Uuid::from_u128(0xb);

    fn event(room: Uuid, from: Uuid, target: airc_core::MentionTarget, lamport: u64, text: &str) -> airc_core::TranscriptEvent {
        airc_core::TranscriptEvent {
            event_id: airc_core::EventId::new(),
            room_id: airc_core::RoomId::from_uuid(room),
            peer_id: PeerId::from_uuid(from),
            client_id: airc_core::ClientId::new(),
            kind: airc_core::TranscriptKind::Message,
            occurred_at_ms: 1_000 + lamport,
            lamport,
            target,
            headers: Default::default(),
            body: Some(airc_core::Body::text(text)),
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        }
    }

    /// A digest whose unread are exactly `events` (bookmark below them), built by
    /// the production builder from pre-fetched events.
    fn digest(room: Uuid, events: Vec<airc_core::TranscriptEvent>) -> Arc<ChannelDigest> {
        let cache = Arc::new(ChannelElementCache::new(Arc::new(LexicalEmbedder::default())));
        let builder = ChannelDigestBuilder::new(cache);
        Arc::new(builder.build_from_events(ME, room, events, 8, 0))
    }

    fn region() -> (PerceptionRegion, watch::Receiver<AwarenessSnapshot>) {
        region_with_saved(None)
    }

    /// Boot from a peer dir that holds `saved` (a prior sitting's continuation), so
    /// Resume means what it means in production: a continuation read from disk.
    fn region_with_saved(saved: Option<Continuation>) -> (PerceptionRegion, watch::Receiver<AwarenessSnapshot>) {
        let dir = tempfile::tempdir().unwrap();
        if let Some(c) = saved {
            let mut m = MindState::default();
            m.continuation = Some(c);
            m.save(dir.path(), 0).unwrap();
        }
        let (mut r, rx) = PerceptionRegion::boot(PeerId::from_uuid(ME), dir.path(), 1_000, 0);
        r.set_identity_facts(vec![JOEL], vec![]);
        r.join(A, "career-wrangler");
        r.join(B, "cambriantech");
        std::mem::forget(dir);
        (r, rx)
    }

    // what this catches: THE integrated-self acceptance (EVENT-MIND §4). While she
    // is deep in A, a peer's line in B is Notable and does not wake her, but a
    // human speaking in B is Addressed... and under Deep still does not pass; a
    // mention passes nothing but Urgent; and the strip knows both activities the
    // whole time. Under Normal, the human in B wakes her for B while A keeps its
    // line. A per-room turn cannot pass this test.
    #[test]
    fn she_knows_both_activities_and_wakes_only_past_her_dial() {
        let (mut r, rx) = region_with_saved(Some(Continuation { activity: A, note: "next: run the suite".into(), expectation: None, written_at_ms: 0 }));
        r.set_dial(AttentionDial::deep(), 0);
        assert_eq!(r.wake_for(1), Some(Wake::Resume), "first wake after boot continues her saved state");
        assert_eq!(r.wake_for(2), None, "Resume fires once");

        r.observe_chat(A, digest(A, vec![event(A, PEER, airc_core::MentionTarget::All, 1, "working")]), 10);
        let wake = r.observe_chat(B, digest(B, vec![event(B, PEER, airc_core::MentionTarget::All, 2, "hi all")]), 11);
        assert_eq!(wake, None, "a peer's line in B under Deep does not wake her");
        let snap = rx.borrow().clone();
        assert_eq!(snap.load.live_activities, 2);
        assert_eq!(snap.load.unread_total, 2, "awareness is total even when the door is shut");
        assert!(snap.render_lines()[0].contains("next: run the suite"));

        let wake = r.observe_chat(B, digest(B, vec![event(B, JOEL, airc_core::MentionTarget::All, 3, "how is it going?")]), 12);
        assert_eq!(wake, None, "Addressed does not pass Deep either");
        assert_eq!(r.wake_for(13), None);

        r.set_dial(AttentionDial::default(), 14);
        match r.wake_for(15) {
            Some(Wake::Perceive { activity, salience }) => {
                assert_eq!(activity, B);
                assert!(salience.reasons.iter().any(|x| matches!(x, SalienceReason::HumanSpoke { who } if *who == JOEL)));
            }
            other => panic!("Normal admits a human in B: {other:?}"),
        }
        // She looks at B; her cursor there moves; A is still unread and still hers.
        r.perceived(B, &ActivityCursor { chat_lamport: 3, chat_event_id: None, views: Default::default() }, 16);
        assert_eq!(r.cursor(B).unwrap().chat_lamport, 3);
        assert!(r.cursor(A).is_none(), "a turn that did not look at A leaves A unread");
        assert_eq!(rx.borrow().lines.iter().find(|l| l.activity == A).unwrap().unread, 1);
    }

    // what this catches: the three-job signal reaching the wake. A failed verdict on
    // her held card while her continuation expects a pass is Urgent and passes even
    // a Deep dial, with the typed Surprise on the wake; her own deadline passing
    // with nothing new wakes her as Continuation.
    #[test]
    fn surprise_passes_a_deep_dial_and_her_own_deadline_wakes_her() {
        let (mut r, _rx) = region();
        let card = Uuid::from_u128(0x9);
        r.set_identity_facts(vec![JOEL], vec![card]);
        r.set_dial(AttentionDial::deep(), 0);
        r.set_continuation(
            Some(Continuation {
                activity: A,
                note: "submitted; expect a pass".into(),
                expectation: Some(Expectation { text: "review passes".into(), by_ms: Some(1_000), verdict: Some(ExpectedVerdict::Passed) }),
                written_at_ms: 0,
            }),
            0,
        );
        assert_eq!(r.wake_for(2), None, "a fresh boot has nothing to resume; nothing yet, not due");
        assert_eq!(r.wake_for(1_001), Some(Wake::Continuation), "her deadline passed with nothing new");

        let wake = r.observe_board(A, vec![BoardChange::Reviewed { card_id: card, outcome: ObservedVerdict::Failed, reviewer: PEER }], 1_002);
        match wake {
            Some(Wake::Perceive { activity, salience }) => {
                assert_eq!(activity, A);
                assert_eq!(salience.level, SalienceLevel::Urgent);
                assert!(salience.reasons.iter().any(|x| matches!(x, SalienceReason::Surprise { .. })));
            }
            other => panic!("a contradicted expectation is Urgent and passes Deep: {other:?}"),
        }
    }

    // what this catches: untimed events (occurred_at_ms 0) never read as "now".
    // 110 of 126 lines in a live prompt were untimed on 2026-10-04; if they set
    // recency, every replayed room would look like it just spoke.
    #[test]
    fn an_untimed_event_does_not_set_recency() {
        let (mut r, rx) = region();
        let mut e = event(A, PEER, airc_core::MentionTarget::All, 1, "old");
        e.occurred_at_ms = 0;
        r.observe_chat(A, digest(A, vec![e]), 50);
        assert_eq!(rx.borrow().lines.iter().find(|l| l.activity == A).unwrap().last_activity_ms, 0);
        r.observe_chat(A, digest(A, vec![event(A, PEER, airc_core::MentionTarget::All, 2, "new")]), 60);
        assert_eq!(rx.borrow().lines.iter().find(|l| l.activity == A).unwrap().last_activity_ms, 1_002);
    }
}
