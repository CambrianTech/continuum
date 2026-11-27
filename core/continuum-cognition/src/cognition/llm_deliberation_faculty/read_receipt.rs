//! Her read receipt: after a prompt is sent, the newest room event it actually rendered, per
//! room. Marking read belongs here, not at delivery: delivery is not reading, and only her
//! one view knows which turns she saw (THE-LANE-ON-SAGE.md). Before this the airc
//! source advanced her cursor while packing, before any prompt existed, so a turn her view
//! then collapsed, or an act that was refused, still marked the room read.

use super::*;
use crate::cognition::workspace::TurnSource;

/// Where a sent prompt's read receipt goes (the persona's airc source, through its room cursor).
#[async_trait::async_trait]
pub trait ReadReceipt: Send + Sync {
    /// She read `room` through `record`, the airc event itself (monotonic: a lower event never
    /// rewinds her).
    async fn read_through(&self, room: uuid::Uuid, record: &airc_core::TranscriptEvent);
}

impl LlmDeliberationFaculty {
    /// Install the read receipt. Off by default: an eval or replay faculty marks nothing read.
    pub fn with_read_receipt(mut self, receipt: Arc<dyn ReadReceipt>) -> Self {
        self.read_receipt = Some(receipt);
        self
    }

    /// The newest event per room that the prompt just sent rendered: her stimulus and every
    /// conversation turn her view shows (room updates are conversation turns, keyed by event).
    pub(super) fn rendered_through(&self, ws: &Workspace) -> Vec<TurnSource> {
        let stimulus = ws.turns.iter().rposition(|t| !t.is_self && t.voice == crate::cognition::workspace::TurnVoice::Speech);
        let view = self.context_view.lock();
        let shown = |index: usize, source: &TurnSource| {
            Some(index) == stimulus || view.as_ref().is_some_and(|v| v.renders("conversation", &source.event.to_string()))
        };
        let turns = ws.turns.iter().enumerate().filter_map(|(i, t)| t.source.filter(|s| shown(i, s)));
        let updates = ws
            .room_updates
            .iter()
            .map(|m| TurnSource { room: m.room_id, event: m.event_id, lamport: m.lamport })
            .filter(|source| view.as_ref().is_some_and(|v| v.renders("conversation", &source.event.to_string())));
        let mut newest: std::collections::HashMap<uuid::Uuid, TurnSource> = std::collections::HashMap::new();
        for source in turns.chain(updates) {
            let entry = newest.entry(source.room).or_insert(source);
            if source.lamport > entry.lamport {
                *entry = source;
            }
        }
        newest.into_values().collect()
    }

    /// After a prompt was sent and answered: mark each room read through what it rendered, the
    /// event resolved against the thread revision this prompt carried (never a revision a later
    /// compose pinned since: Codex on the thread history design). An event that revision does not
    /// hold (another room's, or the stimulus pushed out) moves nothing, and says so.
    pub(super) async fn send_read_receipt(&self, ws: &Workspace) {
        let Some(receipt) = self.read_receipt.as_ref() else {
            return;
        };
        let thread = ws.said.thread();
        for (room, record) in read_records(thread, self.rendered_through(ws)) {
            receipt.read_through(room, &record).await;
        }
    }
}

/// The airc event each rendered newest line is, from the one revision the prompt carried. A line
/// that revision does not hold (another room's, or one a newer arrival pushed out) is named and
/// moves nothing.
fn read_records(thread: &continuum_grounding::room_thread::RoomThread, rendered: Vec<TurnSource>) -> Vec<(uuid::Uuid, Arc<airc_core::TranscriptEvent>)> {
    rendered
        .into_iter()
        .filter_map(|source| match thread.item(source.event).filter(|_| thread.room == source.room) {
            Some(item) => Some((source.room, Arc::clone(&item.record))),
            None => {
                continuum_base::probe!(
                    class = "perception.mark_read.unknown_event",
                    room = %source.room,
                    event = %source.event,
                    revision = thread.revision,
                    "a read receipt named an event the prompt's thread revision does not hold: the cursor stays"
                );
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_grounding::room_thread::{said, RoomThread};

    // what this catches: Codex on the thread history design: a receipt resolved against a revision
    // a later compose pinned. The prompt carried revision A; B, pinned after, pushed A's newest
    // line out. A's receipt still resolves to that line's airc event, and B (which no longer holds
    // it) resolves nothing rather than a neighbour.
    #[test]
    fn a_receipt_resolves_against_the_revision_its_prompt_carried() {
        let (room, peer) = (uuid::Uuid::from_u128(9), uuid::Uuid::from_u128(2));
        let a = RoomThread::new(room).with((1..=2).map(|n| said(room, n, n as u64, peer, "line")), 2).unwrap_or_default();
        let b = a.with((3..=4).map(|n| said(room, n, n as u64, peer, "line")), 2).unwrap_or_default();
        let newest_of_a = TurnSource { room, event: uuid::Uuid::from_u128(2), lamport: 2 };
        let from_a = read_records(&a, vec![newest_of_a]);
        assert_eq!(from_a.iter().map(|(r, e)| (*r, e.lamport)).collect::<Vec<_>>(), [(room, 2)]);
        assert!(read_records(&b, vec![newest_of_a]).is_empty(), "the newer revision no longer holds it");
        let elsewhere = TurnSource { room: uuid::Uuid::from_u128(10), ..newest_of_a };
        assert!(read_records(&a, vec![elsewhere]).is_empty(), "another room's line is not this thread's");
    }
}
