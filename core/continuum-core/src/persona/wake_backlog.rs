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
//! (a ring), the trigger is the newest DIRECTED line (human/agent line or a
//! mention), and every directed line drained earns its heard receipt.

use std::collections::VecDeque;

use uuid::Uuid;

use super::service_loop::IncomingMessage;

/// Event-id ring for replay/redelivery dedupe at the loop head. Small: the
/// pump already dedupes the store catch-up; this catches re-open replays.
pub(crate) struct SeenIds {
    ring: VecDeque<Uuid>,
    cap: usize,
}

impl SeenIds {
    pub(crate) fn new(cap: usize) -> Self {
        Self { ring: VecDeque::with_capacity(cap), cap }
    }

    /// Note an id; `true` when it is NEW (first sight), `false` on a repeat.
    pub(crate) fn note(&mut self, id: Uuid) -> bool {
        if self.ring.contains(&id) {
            return false;
        }
        if self.ring.len() == self.cap {
            self.ring.pop_front();
        }
        self.ring.push_back(id);
        true
    }
}

/// Is this drained line stale? By event id when the source stamps one; by the
/// publisher clock only for id-less sources (scripted/test conversations).
pub(crate) fn is_stale(m: &IncomingMessage, seen: &mut SeenIds, high_water: u64) -> bool {
    if m.event_id.is_nil() {
        m.lamport <= high_water
    } else {
        !seen.note(m.event_id)
    }
}

/// The trigger for ONE turn over the drained backlog: the newest DIRECTED line
/// (a question put to her outranks newer ambient chatter — she answers it with
/// the newer context visible in the transcript), else the newest overall.
/// Returns the trigger and how many lines were coalesced into it.
pub(crate) fn pick_trigger(
    mut qualifying: Vec<IncomingMessage>,
    directed: impl Fn(&IncomingMessage) -> bool,
) -> Option<(IncomingMessage, usize)> {
    let coalesced = qualifying.len().saturating_sub(1);
    let picked = qualifying
        .iter()
        .rposition(|m| directed(m))
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
            crate::ipc::positron_source::CHAT_HEARD,
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

    fn line(peer: u8, lamport: u64, text: &str) -> IncomingMessage {
        IncomingMessage {
            event_id: Uuid::new_v4(),
            lamport,
            peer_id: Uuid::from_u128(peer as u128),
            room_id: Uuid::from_u128(7),
            text: text.to_string(),
        }
    }

    // what this catches: the per-publisher lamport read a human line as stale
    // against citizens' larger clocks (the 9/4 deaf-citizens regression).
    #[test]
    fn a_human_line_with_a_small_publisher_clock_is_not_stale() {
        let mut seen = SeenIds::new(8);
        let human = line(1, 3, "Joel here — which card do you hold?");
        assert!(!is_stale(&human, &mut seen, 157_000));
    }

    // what this catches: a re-open replay redelivering the same event id.
    #[test]
    fn a_replayed_event_id_is_stale_the_second_time() {
        let mut seen = SeenIds::new(8);
        let m = line(1, 3, "once");
        assert!(!is_stale(&m, &mut seen, 0));
        assert!(is_stale(&m, &mut seen, 0));
    }

    // what this catches: a human line without an @mention losing the turn to a
    // newer citizen work receipt.
    #[test]
    fn the_newest_directed_line_wins_over_newer_citizen_chatter() {
        let human = line(1, 3, "Joel here — which card do you hold?");
        let human_id = human.event_id;
        let receipt = line(2, 900, "💭 Let me get my bearings.");
        let (picked, coalesced) = pick_trigger(vec![human, receipt], |m| m.peer_id == Uuid::from_u128(1)).unwrap();
        assert_eq!(picked.event_id, human_id);
        assert_eq!(coalesced, 1);
    }
}
