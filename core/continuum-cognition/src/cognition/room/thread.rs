//! Her thread of each room she is in: the SAGE part that keeps it, and how an airc event becomes one
//! of its lines. The thread itself is a value (`continuum_grounding::room_thread`), because a turn
//! hands the revision it pinned down to every source as data.

mod part;

pub use part::{Thread, Threaded, Threading, THREAD, THREADED};

use std::sync::Arc;

use airc_core::TranscriptEvent;
use continuum_grounding::room_thread::Line;

/// The room line `event` is, if it is one with something said or shown: the event kept whole by
/// handle, its text rendered by the one media-aware room renderer, so an attachment-only turn is a
/// line and a captioned one keeps its attachment notice.
pub fn line(event: &TranscriptEvent) -> Option<Line> {
    let (sender, text) = crate::cognition::conversation::room_event::room_turn_from_event(event).ok()?;
    if text.trim().is_empty() {
        return None;
    }
    // One copy at intake, out of the feed's or the page's event; shared by handle from here.
    let record = event.clone();
    Some(Line { record: Arc::new(record), sender, text })
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_grounding::room_thread::{said, RoomThread};
    use uuid::Uuid;

    fn attached(record: &TranscriptEvent, caption: Option<&str>) -> TranscriptEvent {
        let mut event = record.clone();
        event.kind = match caption {
            Some(_) => airc_core::TranscriptKind::Message,
            None => airc_core::TranscriptKind::Attachment,
        };
        event.body = caption.map(airc_core::Body::text);
        event.attachment = Some(airc_core::AttachmentManifest {
            file_id: airc_core::FileId::new(),
            name: "site.png".into(),
            media_type: Some("image/png".into()),
            size_bytes: 42,
            content_hash: airc_core::ContentHash(format!("sha256:{}", "a".repeat(64))),
            local_path: None,
            remote_ref: None,
        });
        event
    }

    // what this catches: Codex on #6246 f701c5be: the thread is her only history, and a line built
    // from the raw caption dropped an attachment-only turn and the notice on a captioned one, so
    // neither reached her prompt nor her read receipt on the turns after.
    #[test]
    fn an_attachment_is_a_line_and_stays_held_on_later_turns() {
        let (room, sender) = (Uuid::from_u128(7), Uuid::from_u128(8));
        let bare = attached(&said(room, 1, 1, sender, "x").record, None);
        let captioned = attached(&said(room, 2, 2, sender, "x").record, Some("the deploy page"));
        let lines: Vec<Line> = [&bare, &captioned].into_iter().filter_map(line).collect();
        assert_eq!(lines.len(), 2, "an attachment-only turn is a line");
        assert!(lines[0].text.contains("1 media attachment(s)"));
        assert!(lines[1].text.starts_with("the deploy page") && lines[1].text.contains("1 Image"));
        let thread = RoomThread::new(room).with(lines, 8).expect("first lines apply");
        let later = thread.with([said(room, 3, 3, sender, "next")], 8).expect("a later turn applies");
        for event in [&bare, &captioned] {
            let held = later.item(event.event_id.as_uuid()).expect("held on the later turn, so its receipt resolves");
            assert!(held.text.contains("media attachment"));
        }
    }
}
