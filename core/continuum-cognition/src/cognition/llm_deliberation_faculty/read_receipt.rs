//! Her read receipt: after a prompt is sent, the newest room event it actually rendered, per
//! room. Marking read belongs here, not at delivery: delivery is not reading, and only her
//! one view knows which turns she saw (FLOW-IMPLEMENTATION-PLAN §2d). Before this the airc
//! source advanced her cursor while packing, before any prompt existed, so a turn her view
//! then collapsed, or an act that was refused, still marked the room read.

use super::*;
use crate::cognition::workspace::TurnSource;

/// Where a sent prompt's read receipt goes (the persona's airc source, through its room cursor).
#[async_trait::async_trait]
pub trait ReadReceipt: Send + Sync {
    /// She read `room` through `event` (monotonic: a lower event never rewinds her).
    async fn read_through(&self, room: uuid::Uuid, event: uuid::Uuid);
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

    /// After a prompt was sent and answered: mark each room read through what it rendered.
    pub(super) async fn send_read_receipt(&self, ws: &Workspace) {
        let Some(receipt) = self.read_receipt.as_ref() else {
            return;
        };
        for source in self.rendered_through(ws) {
            receipt.read_through(source.room, source.event).await;
        }
    }
}
