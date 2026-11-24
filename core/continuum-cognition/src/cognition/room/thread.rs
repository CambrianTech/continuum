//! Her thread of each room she is in: the SAGE part that keeps it, and how an airc event becomes one
//! of its lines. The thread itself is a value (`continuum_grounding::room_thread`), because a turn
//! hands the revision it pinned down to every source as data.

mod part;

pub use part::{Thread, Threaded, Threading, THREAD, THREADED};

use std::sync::Arc;

use airc_core::TranscriptEvent;
use continuum_grounding::room_thread::Line;

/// The room line `event` is, if it is one with something said: the event kept whole by handle.
pub fn line(event: &TranscriptEvent) -> Option<Line> {
    let turn = crate::cognition::conversation::room_event::room_content_from_event(event).ok()?;
    if turn.text.trim().is_empty() {
        return None;
    }
    // One copy at intake, out of the feed's or the page's event; shared by handle from here.
    let record = event.clone();
    Some(Line { record: Arc::new(record), sender: turn.sender, text: turn.text })
}
