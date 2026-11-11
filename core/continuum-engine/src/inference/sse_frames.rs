//! The one SSE framing: raw bytes in, complete events out, and the `data:` payloads of an event.
//! Every SSE consumer in the core frames through it (a chat-completions stream, the learning
//! event stream), so an event boundary means the same thing everywhere.

/// Complete SSE events, assembled from raw bytes as they arrive.
#[derive(Debug, Default)]
pub struct SseFrames {
    buf: Vec<u8>,
}

impl SseFrames {
    /// Append `bytes`. CR (0x0D) is dropped so event boundaries normalize to `\n\n`; CR never
    /// appears inside a UTF-8 multibyte sequence, so raw bytes are buffered and only a COMPLETE
    /// event is decoded (no character split across reads).
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend(bytes.iter().copied().filter(|b| *b != b'\r'));
    }

    /// The next complete event, if one has arrived whole.
    pub fn next_event(&mut self) -> Option<String> {
        let end = self.buf.windows(2).position(|w| w == b"\n\n")? + 2;
        let event: Vec<u8> = self.buf.drain(..end).collect();
        Some(String::from_utf8_lossy(&event).into_owned())
    }
}

/// The `data:` payloads of one event, trimmed; empty ones (and comment or keepalive lines) skipped.
pub fn data_lines(event: &str) -> impl Iterator<Item = &str> {
    event.lines().filter_map(|line| line.trim_start().strip_prefix("data:")).map(str::trim).filter(|data| !data.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: an event decoded before it is whole (a character or a payload split across
    // reads), a CRLF stream never reaching a boundary, or a keepalive read as a payload.
    #[test]
    fn events_are_whole_and_only_data_lines_are_payloads() {
        let mut frames = SseFrames::default();
        frames.push(b"data: {\"a\":");
        assert_eq!(frames.next_event(), None, "half an event is not an event");
        frames.push("1}\r\n\r\n:\r\n\r\ndata: \u{e9}t\u{e9}\r\n\r\n".as_bytes());
        let first = frames.next_event().expect("the first event is whole");
        assert_eq!(data_lines(&first).collect::<Vec<_>>(), vec!["{\"a\":1}"]);
        let keepalive = frames.next_event().expect("the keepalive is an event");
        assert_eq!(data_lines(&keepalive).count(), 0, "a keepalive carries no payload");
        let third = frames.next_event().expect("a multibyte payload decodes whole");
        assert_eq!(data_lines(&third).collect::<Vec<_>>(), vec!["\u{e9}t\u{e9}"]);
        assert_eq!(frames.next_event(), None);
    }
}
