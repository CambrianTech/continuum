//! The wake backlog: which drained inbound lines qualify for a turn, which one
//! triggers it, and the delivery receipt each directed line earns.
//!
//! Two defects this file replaces (both found live 2026-09-04, the deaf-citizens
//! night): (1) staleness was `lamport <= high_water`, but a lamport is the
//! PUBLISHER's clock, not a room order — the operator self-peer's young clock
//! read as stale against citizens' large ones, so a human line was skipped
//! before it could trigger anything; (2) the trigger preferred the newest
//! line that @mentioned her, else the newest overall — a human line without
//! a mention lost to any newer citizen work receipt, and the "heard by N"
//! receipt fired only for the chosen line. Now: staleness is by event id
//! (a ring), the trigger is the newest priority line (human opportunity or a
//! mention), and every priority line drained earns its heard receipt.

use std::collections::{HashMap, VecDeque};

use uuid::Uuid;

use crate::cognition::conversation::IncomingMessage;
use crate::cognition::workspace::TurnAttention;

/// Loop-local custody of admitted inputs. Capacity refusals wait on their bound
/// serving facts, not the beat, while the ordinary inbox remains reachable.
#[derive(Default)]
pub(crate) struct PendingInput {
    persona: Option<Uuid>,
    inputs: VecDeque<RetainedInput>,
    self_refusals: HashMap<Uuid, Option<crate::cognition::workspace::CapacityRefusal>>,
    work_witnesses: VecDeque<(Uuid, crate::cognition::conversation::room_event::RoomWork)>,
    /// Interrupted turns that recorded an effect (stop-drain S3): never retried, owed one
    /// completed turn on the cycle that carries their notice, then closed. `(row, room)`.
    uncertain: Vec<(Uuid, Uuid)>,
    /// Uncertain rows a completed turn has answered: the loop closes them.
    answered: Vec<Uuid>,
    /// When she last passed her initiative turn; engagement forgets it (card d0cf9473).
    initiative_passed_at_ms: Option<u64>,
}
mod recovered;
struct RetainedInput {
    message: IncomingMessage,
    engram: Option<Uuid>,
    /// Its durable row (`persona::obligation`, card 920619c5), kept open while retained.
    obligation: Option<Uuid>,
    attention: TurnAttention,
    capacity: Option<crate::cognition::workspace::CapacityRefusal>,
}

impl PendingInput {
    pub(crate) fn for_persona(persona: Uuid) -> Self {
        Self { persona: Some(persona), ..Self::default() }
    }
    pub(crate) fn stop(&mut self) {
        for input in &mut self.inputs {
            if let Some(refusal) = input.capacity.take() {
                refusal.note_transition(self.persona, input.message.room_id, "stopped", "stop");
            }
        }
        for (room, refusal) in &mut self.self_refusals {
            if let Some(refusal) = refusal.take() {
                refusal.note_transition(self.persona, *room, "stopped", "stop");
            }
        }
    }
    pub(crate) fn resume_input(&mut self, message: &IncomingMessage) {
        if let Some(input) = self.inputs.iter_mut().find(|input| input.message == *message) {
            if let Some(reason) = input.capacity.as_ref().and_then(|r| r.readiness.as_ref())
                .and_then(|r| r.change_reason()) {
                if let Some(refusal) = input.capacity.take() {
                    refusal.note_transition(self.persona, message.room_id, "resumed", reason);
                }
            }
        }
    }
    fn park_self(&mut self, room: Uuid, refusal: &crate::cognition::workspace::CapacityRefusal) {
        refusal.note_transition(self.persona, room, "parked", "capacity");
        self.self_refusals.insert(room, Some(refusal.clone()));
    }

    #[cfg(test)]
    pub(crate) fn message(&self) -> Option<&IncomingMessage> {
        self.inputs.front().map(|input| &input.message)
    }
    pub(crate) fn permits_self_tick(&self) -> bool {
        self.inputs.is_empty() && self.self_refusals.is_empty()
    }
    pub(crate) fn wake_engram_for(&self, message: &IncomingMessage) -> Option<Uuid> {
        self.inputs
            .iter()
            .find(|input| input.message == *message)
            .and_then(|input| input.engram)
    }
    /// The durable row of a retained input, so its retry continues the same obligation.
    pub(crate) fn obligation_for(&self, message: &IncomingMessage) -> Option<Uuid> {
        self.inputs
            .iter()
            .find(|input| input.message == *message)
            .and_then(|input| input.obligation)
    }
    /// After a turn that settled without finishing: when the input is still retained (a
    /// capacity refusal or a deferral), it keeps `obligation` and the row stays open; returns
    /// whether it did. A `false` means the input is done and its row may close.
    pub(crate) fn keep_obligation(&mut self, message: &IncomingMessage, obligation: Option<Uuid>) -> bool {
        match self.inputs.iter_mut().find(|input| input.message == *message) {
            Some(input) => {
                input.obligation = obligation.or(input.obligation);
                true
            }
            None => false,
        }
    }
    pub(crate) fn attention_for(&self, message: &IncomingMessage) -> Option<TurnAttention> {
        self.inputs
            .iter()
            .find(|input| input.message == *message)
            .map(|input| input.attention)
    }
    /// New admitted payload is a semantic change in that room. It never replaces
    /// the old input or its engram; the old obligation gets a fresh assembly later.
    pub(crate) fn payload_changed(&mut self, message: &IncomingMessage) {
        for input in &mut self.inputs {
            if input.message.room_id == message.room_id && input.message != *message {
                if let Some(refusal) = input.capacity.take() {
                    refusal.note_transition(self.persona, input.message.room_id, "resumed", "input");
                }
            }
        }
        if let Some(refusal) = self.self_refusals.get_mut(&message.room_id) {
            if let Some(refusal) = refusal.take() {
                refusal.note_transition(self.persona, message.room_id, "resumed", "input");
            }
        }
    }
    /// Typed board facts already pass through the retained inbox. Observe them
    /// before trigger filtering; a board change need not masquerade as chat.
    /// Lease/presence traffic is excluded by the shared decoder. Clock-only and
    /// repeated semantic facts cannot re-arm an unchanged parked request.
    pub(crate) fn observe_work(&mut self, message: &IncomingMessage) {
        let Some(work) = &message.work else { return };
        if matches!(work, crate::cognition::conversation::room_event::RoomWork::Other { .. }) { return; }
        let key = |work: &crate::cognition::conversation::room_event::RoomWork| {
            use crate::cognition::conversation::room_event::RoomWork;
            match work {
                RoomWork::Reviewed { card_id, .. } | RoomWork::StateChanged { card_id, .. }
                | RoomWork::Submitted { card_id, .. } | RoomWork::Claimed { card_id, .. } => Some(*card_id),
                RoomWork::Other { .. } => None,
            }
        };
        let previous = self.work_witnesses.iter().position(|(room, prior)| {
            *room == message.room_id && key(prior) == key(work)
                && std::mem::discriminant(prior) == std::mem::discriminant(work)
        });
        if let Some(index) = previous {
            if self.work_witnesses[index].1 == *work { return; }
            self.work_witnesses.remove(index);
        }
        self.work_witnesses.push_back((message.room_id, work.clone()));
        if self.work_witnesses.len() > super::perception_region::UNPERCEIVED_CAP {
            self.work_witnesses.pop_front();
        }
        self.payload_changed(message);
    }
    pub(crate) fn take_ready_self(&mut self, room: Option<Uuid>) -> bool {
        let Some(room) = room else { return false };
        let ready = self.self_refusals.get(&room).is_some_and(|r| {
            r.as_ref().map_or(true, |r| r.readiness.as_ref().is_some_and(|r| r.changed()))
        });
        if ready {
            if let Some(Some(refusal)) = self.self_refusals.remove(&room) {
                if let Some(reason) = refusal.readiness.as_ref().and_then(|r| r.change_reason()) {
                    refusal.note_transition(self.persona, room, "resumed", reason);
                }
            }
        }
        ready
    }
    pub(crate) async fn ready_self_room(&self) -> Uuid {
        use futures_util::stream::{FuturesUnordered, StreamExt};
        let mut waits = self
            .self_refusals
            .iter()
            .map(|(room, refusal)| async move {
                if let Some(refusal) = refusal { wait_capacity(refusal).await; }
                *room
            })
            .collect::<FuturesUnordered<_>>();
        match waits.next().await {
            Some(room) => room,
            None => std::future::pending().await,
        }
    }
    pub(crate) fn settle_work(&mut self, work: &crate::persona::act_question::WorkQuestionOutcome) {
        if let Some(room) = work.room {
            match &work.capacity {
                Some(refusal) => {
                    self.park_self(room, refusal);
                }
                None => {
                    self.self_refusals.remove(&room);
                }
            }
        }
    }
    /// Her initiative turn's outcome on the self-cycle that began at `cycle_ms`: a pass is
    /// remembered so the next beat waits for her perception to move
    /// ([`Initiative::on_beat`](crate::persona::initiative_turn::Initiative::on_beat));
    /// engagement forgets it. The cycle's start, not its end: a line that arrived while
    /// she deliberated may not be in what she passed on, so it still moves her.
    pub(crate) fn note_initiative(&mut self, work: &crate::persona::act_question::WorkQuestionOutcome, cycle_ms: u64) {
        if work.engaged {
            self.initiative_passed_at_ms = None;
        } else if work.drove {
            self.initiative_passed_at_ms = Some(cycle_ms);
        }
    }
    pub(crate) fn initiative_passed_at_ms(&self) -> Option<u64> {
        self.initiative_passed_at_ms
    }
    pub(crate) fn settle_self(
        &mut self,
        room: Uuid,
        step: &crate::cognition::act_observe::SettleStep,
    ) {
        match step {
            crate::cognition::act_observe::SettleStep::CapacityRefused { error, .. } => {
                self.park_self(room, error);
            }
            _ => {
                self.self_refusals.remove(&room);
            }
        }
    }
    /// Cancellation never takes an input out of custody. Multiple parked inputs
    /// wait independently; a small new request does not evict a larger old one.
    pub(crate) async fn at_next_beat(&self, beat: std::time::Duration) -> Option<IncomingMessage> {
        use futures_util::stream::{FuturesUnordered, StreamExt};
        if self.inputs.is_empty() {
            if self.self_refusals.is_empty() {
        tokio::time::sleep(beat).await;
            } else {
                std::future::pending::<()>().await;
    }
            return None;
    }
        let mut waits = self
            .inputs
            .iter()
            .map(|input| async move {
                match &input.capacity {
                    Some(refusal) => wait_capacity(refusal).await,
                    None => tokio::time::sleep(beat).await,
    }
                input.message.clone()
            })
            .collect::<FuturesUnordered<_>>();
        waits.next().await
    }
    pub(crate) fn settle(
        &mut self,
        message: &IncomingMessage,
        wake_engram: Option<Uuid>,
        attention: TurnAttention,
        step: &crate::cognition::act_observe::SettleStep,
    ) {
        use crate::cognition::act_observe::SettleStep;
        // A re-retained input keeps its durable row: a second refusal is the same obligation.
        let obligation = self.obligation_for(message);
        self.inputs.retain(|input| input.message != *message);
        let capacity = match step {
            SettleStep::CapacityRefused {
                error,
                replay_input: true,
            } => Some(error.clone()),
            SettleStep::CapacityRefused {
                error,
                replay_input: false,
            } => {
                // Earlier work happened: do not replay the original message.
                // Its continuation still waits for capacity rather than each beat.
                self.park_self(message.room_id, error);
                return;
            }
            SettleStep::DeferredBeforeDispatch if attention.requires_priority() => None,
            _ => return,
        };
        if let Some(refusal) = &capacity {
            refusal.note_transition(self.persona, message.room_id, "parked", "capacity");
        }
        self.inputs.push_back(RetainedInput {
            message: message.clone(),
            engram: wake_engram,
            obligation,
            attention,
            capacity,
        });
        }
}
async fn wait_capacity(refusal: &crate::cognition::workspace::CapacityRefusal) {
    match &refusal.readiness {
        Some(readiness) => readiness.wait().await,
        // A producer without a readiness owner must not generate a retry storm.
        None => std::future::pending::<()>().await,
    }
}

/// Can this drained line be the TRIGGER of a turn at all? A priority line
/// (human, or anyone naming her) always; an undirected line from a fellow
/// CITIZEN yes — that is conversation among citizens; an undirected line from
/// an AGENT (neither human nor citizen) NEVER — it is perceived (transcript,
/// digest) and takes no lane. Measured 2026-09-05: 34 of Joaquin's last 60
/// turns were message turns on BigMama's and IntelMac's walls in the project
/// room; her held card had zero edits in eight hours (card ae4bb4fd).
pub(crate) fn triggers_a_turn(priority: bool, sender_is_citizen: bool, is_receipt: bool) -> bool {
    priority || (sender_is_citizen && !is_receipt)
}

/// A RECEIPT is a citizen's own trace — her 💭 thought line or her ⚙ act line — written
/// for the human and the HUD, not addressed to anyone. It reaches the digest (perception)
/// and never wakes a turn. Measured 2026-09-13 10:00–14:00Z: every holder in the seed-4
/// room took a lane every ~4 min on a teammate's receipt (57–86 admitted inputs each,
/// from_peer = a citizen), re-orienting ("let me take stock") instead of working; the room
/// read "spinning" to the citizens themselves. A plain citizen line (a question, one line
/// of help) still wakes; a human's or a mention always does (priority).
pub(crate) fn is_receipt(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with(crate::cognition::presence_glyph::THOUGHT)
        || t.starts_with(crate::cognition::presence_glyph::ACT)
}

/// The trigger for ONE turn over the drained backlog: the newest priority line
/// (a question put to her outranks newer ambient chatter — she answers it with
/// the newer context visible in the transcript), else the newest overall.
/// Returns the trigger and how many lines were coalesced into it.
pub(crate) fn pick_trigger(
    mut qualifying: Vec<IncomingMessage>,
    priority: impl Fn(&IncomingMessage) -> bool,
) -> Option<(IncomingMessage, usize)> {
    let coalesced = qualifying.len().saturating_sub(1);
    let picked = qualifying
        .iter()
        .rposition(|m| priority(m))
        .map(|i| qualifying.swap_remove(i))
        .or_else(|| qualifying.pop())?;
    Some((picked, coalesced))
}

/// FEEDBACK FOR THE INTERFACE: this citizen HEARD the line — the delivery
/// receipt the human sees as "heard by N" on the row (Joel 2026-09-04: "more
/// feedback events for the interface"). Fired for every directed line drained,
/// not only the one that triggers the turn: hearing is delivery, not reply.
pub(crate) fn publish_heard(persona: Uuid, msg: &IncomingMessage) {
    if msg.event_id.is_nil() {
        return;
    }
    if let Some(bus) = crate::runtime::MessageBus::global() {
        bus.publish_async_only(
            crate::cognition::mind_events::CHAT_HEARD,
            serde_json::json!({
                "message_id": msg.event_id,
                "room_id": msg.room_id,
                "persona_id": persona,
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A capacity-refused original remains owned while a newer small request is
    // answered. Prior-work refusals park continuation without replaying the input.
    #[tokio::test(start_paused = true)]
    async fn capacity_custody_is_quiet_and_new_input_cannot_evict_it() {
        use crate::cognition::act_observe::SettleStep;
        use crate::cognition::workspace::PromptCapacityError;
        use std::time::Duration;
        let original = line(1, 1, "large original");
        let newer = line(2, 2, "small new input");
        let engram = Some(Uuid::new_v4());
        let refusal = SettleStep::CapacityRefused {
            error: PromptCapacityError {
                required_tokens: 8192,
                budget_tokens: 4096,
            }
            .into(),
            replay_input: true,
        };
        let mut pending = PendingInput::default();
        pending.settle(&original, engram, TurnAttention::HumanOpportunity, &refusal);
        assert!(!pending.permits_self_tick());
        assert!(tokio::time::timeout(
            Duration::from_secs(30),
            pending.at_next_beat(Duration::from_secs(1))
        )
        .await
        .is_err());
        pending.settle(
            &newer,
            None,
            TurnAttention::HumanOpportunity,
            &SettleStep::Passed { reason: None },
        );
        assert_eq!(pending.message(), Some(&original));
        assert_eq!(pending.wake_engram_for(&original), engram);
        pending.payload_changed(&newer);
        assert_eq!(
            pending.at_next_beat(Duration::from_secs(1)).await,
            Some(original.clone())
        );
        let SettleStep::CapacityRefused { error, .. } = refusal else {
            unreachable!()
        };
        pending.settle(
            &original,
            engram,
            TurnAttention::HumanOpportunity,
            &SettleStep::CapacityRefused {
                error,
                replay_input: false,
            },
        );
        assert!(pending.message().is_none());
        assert!(!pending.permits_self_tick());
        let mut elsewhere = newer.clone();
        elsewhere.room_id = Uuid::new_v4();
        pending.payload_changed(&elsewhere);
        pending.settle_self(elsewhere.room_id, &SettleStep::Passed { reason: None });
        pending.settle_work(&crate::persona::act_question::WorkQuestionOutcome {
            drove: true,
            room: Some(elsewhere.room_id),
            capacity: None,
            engaged: true,
        });
        assert!(
            pending.self_refusals.contains_key(&original.room_id),
            "unrelated room input and completed work cannot unpark the original continuation"
        );
        assert!(tokio::time::timeout(
            Duration::from_secs(30),
            pending.at_next_beat(Duration::from_secs(1))
        )
        .await
        .is_err());
    }

    // Regression 2e3f0eab: the actual wake owner waits the existing cadence,
    // retains input across cancellation/new arrivals, and clears only on handling.
    #[tokio::test(start_paused = true)]
    async fn deferred_attention_waits_without_consuming_the_trigger_or_new_input() {
        use crate::cognition::act_observe::SettleStep;
        use crate::cognition::conversation::scripted_conversation::ScriptedConversation;
        use crate::cognition::conversation::PersonaConversation;
        use std::time::Duration;
        let original = line(1, 1, "original directed request");
        let newer = line(2, 2, "newer directed request");
        let cause = Some(Uuid::new_v4());
        let mut pending = PendingInput::default();
        pending.settle(
            &original,
            cause,
            TurnAttention::HumanOpportunity,
            &SettleStep::DeferredBeforeDispatch,
        );
        let mut conversation =
            ScriptedConversation::new().with_events(vec![Ok(Some(newer.clone()))]);
        conversation.prime().await.unwrap();
        let beat = Duration::from_secs(3);
        assert!(tokio::time::timeout(beat / 2, pending.at_next_beat(beat)).await.is_err());
        assert_eq!(pending.message(), Some(&original), "cancelled wake retains custody");
        let retry = pending.at_next_beat(beat).await.unwrap();
        assert_eq!(retry, original);
        assert_eq!(pending.wake_engram_for(&original), cause);
        // Presence may disappear while waiting; the admitted human opportunity
        // remains priority without becoming a literal address.
        assert_eq!(
            TurnAttention::for_message(false, false),
            TurnAttention::Ambient
        );
        assert_eq!(
            pending.attention_for(&original),
            Some(TurnAttention::HumanOpportunity)
        );
        assert!(pending
            .attention_for(&original)
            .unwrap()
            .requires_priority());
        assert!(!pending.attention_for(&original).unwrap().is_addressed());
        assert_eq!(
            conversation
                .perceive_ready()
                .await
                .unwrap()
                .front()
                .map(|m| m.as_ref()),
            Some(&newer)
        );
        pending.settle(
            &retry,
            cause,
            TurnAttention::HumanOpportunity,
            &SettleStep::DeferredBeforeDispatch,
        );
        assert_eq!(
            pending.message(),
            Some(&original),
            "another refusal is not completion"
        );
        pending.settle(
            &retry,
            cause,
            TurnAttention::HumanOpportunity,
            &SettleStep::Passed {
                reason: Some("chosen silence".into()),
            },
        );
        assert!(
            pending.at_next_beat(beat).await.is_none(),
            "a genuine Pass is terminal"
        );
        assert!(pending.attention_for(&original).is_none());
        assert_eq!(conversation.next_message().await.unwrap(), Some(newer));
        assert!(conversation.next_message().await.unwrap().is_none());
        pending.settle(
            &original,
            cause,
            TurnAttention::Ambient,
            &SettleStep::DeferredBeforeDispatch,
        );
        assert!(
            pending.message().is_none(),
            "ambient policy does not hold off directed arrivals"
        );
    }

    // what this catches (Codex on #5480): a deferred input's durable obligation closing while
    // the input is still owed. The retained input keeps the row across another refusal, its
    // retry reads the same row back, and only a terminal settle releases it.
    #[test]
    fn a_retained_input_keeps_its_obligation_until_a_terminal_settle() {
        use crate::cognition::act_observe::SettleStep;
        let original = line(1, 1, "original directed request");
        let row = Some(Uuid::new_v4());
        let mut pending = PendingInput::default();
        assert!(!pending.keep_obligation(&original, row), "nothing retained: the row may close");
        let deferred = SettleStep::DeferredBeforeDispatch;
        pending.settle(&original, None, TurnAttention::HumanOpportunity, &deferred);
        assert!(pending.keep_obligation(&original, row), "retained: the row stays open");
        assert_eq!(pending.obligation_for(&original), row, "its retry continues the same row");
        pending.settle(&original, None, TurnAttention::HumanOpportunity, &deferred);
        assert!(pending.keep_obligation(&original, None), "another refusal is not completion");
        assert_eq!(pending.obligation_for(&original), row, "a refusal never forgets the row");
        let pass = SettleStep::Passed { reason: Some("chosen silence".into()) };
        pending.settle(&original, None, TurnAttention::HumanOpportunity, &pass);
        assert!(!pending.keep_obligation(&original, row), "a terminal settle releases it to close");
    }

    // what this catches (2026-09-13): a teammate's 💭/⚙ receipt admitting a turn — every
    // holder in a five-coder room took a lane per teammate trace and re-oriented instead
    // of working. A citizen's plain line still wakes; a human's line always does.
    #[test]
    fn a_teammates_receipt_is_perceived_never_served() {
        assert!(is_receipt("💭 Let me take stock honestly. I'm Kira…"));
        assert!(is_receipt("  ⚙ code/read  ✓"));
        assert!(!is_receipt("Kira, the fix is in sinks.py line 40 — try the mtime path"));
        assert!(!triggers_a_turn(false, true, true), "a citizen's receipt never wakes a turn");
        assert!(triggers_a_turn(false, true, false), "a citizen's plain line still wakes");
        assert!(triggers_a_turn(true, false, true), "a priority line (human, mention) always wakes");
        assert!(!triggers_a_turn(false, false, false), "an undirected agent line never wakes");
    }

    fn line(peer: u8, lamport: u64, text: &str) -> IncomingMessage {
        IncomingMessage {
            work: None,
            media: Vec::new(),
            event_id: Uuid::new_v4(),
            lamport,
            peer_id: Uuid::from_u128(peer as u128),
            room_id: Uuid::from_u128(7),
            text: text.to_string(),
        }
    }

    // what this catches: an undirected AGENT line is perceived but never a
    // trigger; a citizen's ambient line and any directed line still are.
    #[test]
    fn an_undirected_agent_line_never_triggers_a_turn() {
        assert!(
            !triggers_a_turn(false, false, false),
            "agent wall, no mention: perceived only"
        );
        assert!(triggers_a_turn(true, false, false), "agent naming her: a turn");
        assert!(
            triggers_a_turn(false, true, false),
            "citizen chatter: conversation, a turn"
        );
        assert!(triggers_a_turn(true, true, false));
    }

    // what this catches: a human line without an @mention losing the turn to a
    // newer citizen work receipt.
    #[test]
    fn a_human_opportunity_wins_over_newer_citizen_chatter_without_a_mention() {
        use crate::cognition::workspace::TurnAttention;
        let identity =
            crate::cognition::conversation::persona_identity::PersonaIdentity::new(Uuid::new_v4(), "Kimi");
        let human = line(1, 3, "Joel here — which card do you hold?");
        let human_id = human.event_id;
        let receipt = line(2, 900, "💭 Let me get my bearings.");
        let (picked, coalesced) = pick_trigger(vec![human, receipt], |m| {
            TurnAttention::for_message(identity.mentions(&m.text), m.peer_id == Uuid::from_u128(1))
                .requires_priority()
        })
        .unwrap();
        assert_eq!(picked.event_id, human_id);
        assert_eq!(coalesced, 1);
        assert!(
            !identity.mentions(&picked.text),
            "human priority does not require a name"
        );
    }

    // what this catches (moved from cognition::act_observe with mind split step 7: custody is
    // persona's): regression 2e3f0eab: admission refusal must retain the original directed
    // input, even when a newer directed input arrives before the admitted retry.
    #[tokio::test]
    async fn starved_priority_input_survives_until_dispatch_without_replaying_actions() {
        use crate::cognition::act_observe::test_support::{admission, body_with_wm, ActThenSpeak, RecordingExecutor};
        use crate::cognition::act_observe::{drive_to_settle_with_input, SettleStep};
        use crate::cognition::working_memory::{WorkingMemory, WorkingMemoryFaculty};
        use crate::cognition::workspace::{Burst, Faculty, SalienceArbiter, TurnAttention, TurnFraming, WorkspaceCycle};
        use std::sync::{Arc, Mutex};
        use crate::cognition::conversation::scripted_conversation::ScriptedConversation;
        use crate::cognition::conversation::{IncomingMessage, PersonaConversation};
        use std::sync::atomic::{AtomicBool, Ordering};

        let gate = Arc::new(AtomicBool::new(false));
        let mut faculty = ActThenSpeak::new();
        faculty.admission_open = Some(Arc::clone(&gate));
        let exec = Arc::new(RecordingExecutor {
            seen_context: Arc::new(Mutex::new(None)),
            result_content: "4".into(),
        });
        let wm = Arc::new(WorkingMemory::new(8));
        let cycle = WorkspaceCycle::new(
            vec![
                Arc::new(WorkingMemoryFaculty::new(Arc::clone(&wm))) as Arc<dyn Faculty>,
                Arc::new(faculty),
            ],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec.clone(), admission(), wm));
        let original = IncomingMessage {
            event_id: Uuid::new_v4(), lamport: 10, peer_id: Uuid::new_v4(),
            room_id: Uuid::new_v4(), text: "Asha, inspect this exact input".into(), work: None,
            media: vec![crate::cognition::conversation::channel_items::MediaItemRequest {
                kind: "image".into(), mime_type: Some("image/png".into()),
                blob_hash: Some("sha256:retained-input".into()), url: None,
                description: Some("original attachment".into()),
            }],
        };
        let mut newer = original.clone();
        newer.event_id = Uuid::new_v4();
        newer.room_id = Uuid::new_v4();
        newer.lamport += 1;
        newer.text = "Asha, a newer directed request".into();
        let mut conversation = ScriptedConversation::new();
        conversation.prime().await.unwrap();
        let mut pending = PendingInput::default();
        let cause = Some(Uuid::new_v4());
        let framing = TurnFraming::message(TurnAttention::for_message(true, false));
        let burst = |message: &IncomingMessage| {
            Burst::raw_in(
            crate::identity::ActivityRoom::from_uuid(message.room_id).unwrap(),
            message.render_content(),
            )
        };
        let refused =
            drive_to_settle_with_input(&cycle, burst(&original), 8, framing, &mut conversation).await;
        assert!(refused.deferred_before_dispatch);
        assert_eq!(refused.acts, 0);
        assert!(exec.seen_context.lock().unwrap().is_none());
        let (step, _, _) = SettleStep::from_settled(refused);
        pending.settle(&original, cause, framing.attention, &step);
        conversation.event_feed().push(Ok(Some(newer.clone())));
        assert_eq!(pending.message(), Some(&original));
        assert_eq!(pending.wake_engram_for(&original), cause);

        // A second failed admission neither consumes nor replaces the original.
        let retried = pending.message().unwrap().clone();
        let refused_again = drive_to_settle_with_input(&cycle, burst(&retried), 8, framing, &mut conversation).await;
        let (step, _, _) = SettleStep::from_settled(refused_again);
        pending.settle(&retried, cause, framing.attention, &step);
        assert_eq!(pending.message(), Some(&original));
        assert!(conversation.perceive_ready().await.unwrap().iter().any(|m| **m == newer));

        gate.store(true, Ordering::SeqCst);
        let completed = drive_to_settle_with_input(
            &cycle,
            burst(pending.message().unwrap()),
            8,
            framing,
            &mut conversation,
        )
        .await;
        assert!(!completed.deferred_before_dispatch);
        assert_eq!(completed.acts, 1, "only the admitted retry executes an action");
        assert!(completed.room_updates.iter().any(|m| **m == newer));
        assert_eq!(completed.room, original.room_id);
        let (step, _, _) = SettleStep::from_settled(completed);
        assert!(matches!(&step, SettleStep::Spoke(_)));
        pending.settle(&original, cause, framing.attention, &step);
        assert!(pending.message().is_none(), "completed input is not replayed");
        assert_eq!(conversation.next_message().await.unwrap(), Some(newer));
        assert!(conversation.next_message().await.unwrap().is_none());
    }
}
