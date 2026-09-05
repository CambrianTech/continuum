//! Reply speaker — a hosted persona's room line becomes her VOICE while the
//! room has a live call.
//!
//! Before this, a call had twelve animated heads and hold music: citizens
//! answered the human in TEXT (the room line, the token rail lit the tile)
//! and nothing ever reached `voice/speak-in-call`, so nobody was audible
//! (Joel, 2026-09-05: "they say they're talking … no output"). The room is
//! the call (`call_id == room_id`, #193), so the seam is the bus: every
//! `chat:posted` in a room with an active voice session, from a persona this
//! node hosts, is spoken through ONE path — [`VoiceState::speak_in_room`].
//!
//! Shape (CONCURRENCY-STYLE-GUIDE): own task on the bus receiver, sequential
//! speech (voices never overlap — one utterance at a time across the node),
//! lag on the broadcast channel is skipped and counted, never a crash.

use std::sync::Arc;

use tokio::sync::broadcast::error::RecvError;

use crate::ipc::positron_source::parse_chat_posted;
use crate::modules::live::VoiceState;
use crate::runtime::message_bus::MessageBus;

/// Longest utterance spoken from one room line. A 500-token reply is minutes
/// of TTS on a laptop; the first sentences carry the answer, the rest stays
/// on screen as text.
pub(crate) const MAX_SPOKEN_CHARS: usize = 600;

pub(crate) fn spawn(bus: Arc<MessageBus>, state: Arc<VoiceState>) {
    let mut rx = bus.receiver();
    tokio::spawn(async move {
        loop {
            let event = match rx.recv().await {
                Ok(e) => e,
                Err(RecvError::Lagged(n)) => {
                    crate::probe!(
                        class = "live.tts.bus_lagged",
                        module = "live",
                        skipped = n,
                        "reply speaker fell behind the bus — those lines stay text-only"
                    );
                    continue;
                }
                Err(RecvError::Closed) => return,
            };
            let Some(posted) = parse_chat_posted(&event.name, &event.payload) else {
                continue;
            };
            let room = posted.room_id.to_string();
            if !state.has_active_session(&room) {
                continue;
            }
            // Only a persona THIS node hosts speaks here — the human's line is
            // her own voice already, a remote peer's line is spoken on its host.
            let Some(name) = crate::persona::PersonaAircRuntimeRegistry::try_global()
                .and_then(|r| r.get(posted.sender_id))
                .map(|rt| rt.agent_name().to_string())
            else {
                continue;
            };
            let Some((text, truncated)) = speakable_text(&posted.content) else {
                continue;
            };
            let persona = posted.sender_id.to_string();
            let started = std::time::Instant::now();
            match state
                .speak_in_room(&room, &persona, &text, None, None, Some(&name))
                .await
            {
                Ok((_, duration_ms, _)) => crate::probe!(
                    class = "live.tts.spoken",
                    module = "live",
                    room = room.as_str(),
                    persona = name.as_str(),
                    chars = text.chars().count() as u64,
                    truncated = truncated,
                    audio_ms = duration_ms,
                    synth_ms = started.elapsed().as_millis() as u64,
                    "a room line became her voice in the call"
                ),
                Err(e) => crate::probe!(
                    class = "live.tts.failed",
                    module = "live",
                    room = room.as_str(),
                    persona = name.as_str(),
                    error = e.as_str(),
                    "speak_in_room refused — the line stays text-only"
                ),
            }
        }
    });
}

/// What of a room line is worth SAYING: thought receipts (`💭 …`) are the
/// glass box, not speech; code fences are read on screen, not aloud; and
/// the utterance is capped at [`MAX_SPOKEN_CHARS`] on a sentence boundary.
/// Returns `(text, truncated)`, or `None` when nothing speakable remains.
pub(crate) fn speakable_text(content: &str) -> Option<(String, bool)> {
    let trimmed = content.trim();
    if trimmed.is_empty() || trimmed.starts_with("💭") {
        return None;
    }
    // Drop fenced code blocks wholesale.
    let mut out = String::with_capacity(trimmed.len());
    let mut in_fence = false;
    for line in trimmed.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            out.push_str(line);
            out.push(' ');
        }
    }
    let spoken: String = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if spoken.is_empty() {
        return None;
    }
    if spoken.chars().count() <= MAX_SPOKEN_CHARS {
        return Some((spoken, false));
    }
    let head: String = spoken.chars().take(MAX_SPOKEN_CHARS).collect();
    let cut = head
        .rfind(['.', '!', '?'])
        .map(|i| i + 1)
        .filter(|&i| i > MAX_SPOKEN_CHARS / 3)
        .unwrap_or(head.len());
    Some((head[..cut].trim_end().to_string(), true))
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the speakable filter's three rules — a thought
    // receipt is never spoken, code fences are dropped while the prose around
    // them survives, and a long reply is cut on a sentence boundary under the
    // cap with `truncated = true`.
    #[test]
    fn receipts_are_silent_fences_drop_and_long_replies_cut_on_a_sentence() {
        assert_eq!(speakable_text("💭 Let me look at the repo"), None);
        assert_eq!(speakable_text("   "), None);
        let (t, cut) = speakable_text("Try this:\n```rust\nlet x = 1;\n```\nThen run it.").unwrap();
        assert_eq!((t.as_str(), cut), ("Try this: Then run it.", false));
        let long = "One sentence here. ".repeat(60);
        let (t, cut) = speakable_text(&long).unwrap();
        assert!(cut);
        assert!(t.chars().count() <= MAX_SPOKEN_CHARS);
        assert!(t.ends_with('.'), "cut on a sentence boundary, got: {t:?}");
    }
}
