//! Streams one served generation's tokens to its requester under its stream id, as
//! they are produced, through the library's stream bound to the request.

use std::sync::Arc;

use airc_core::Body;
use uuid::Uuid;

use crate::routing::addressed_inbox::{AddressedFrame, AddressedPort};

/// Tokens are coalesced and flushed at most this often — the same cadence the local
/// persona turn uses into a room; one frame per token would be 35 frames/s on a 27B.
const STREAM_FLUSH_EVERY: std::time::Duration = std::time::Duration::from_millis(250);

/// Streams one generation's chunks to the request it answers, under its stream id.
pub(super) struct StreamPublisher {
    pub(super) port: Arc<dyn AddressedPort>,
    pub(super) request: Arc<dyn AddressedFrame>,
    pub(super) stream_id: Uuid,
    /// When this seat RECEIVED the request. The first prefill frame or token published
    /// after it is the seat's own receipt-to-first-progress wait — the queue as the seat
    /// experiences it, which is what it may honestly advertise on its beacon.
    pub(super) received_at: std::time::Instant,
}

impl StreamPublisher {
    async fn publish(
        &self,
        seq: u64,
        kind: &str,
        text: String,
        is_final: bool,
    ) -> Result<(), String> {
        let mut headers = airc_core::Headers::new();
        headers.insert(
            airc_lib::HEADER_STREAM_ID.into(),
            self.stream_id.to_string(),
        );
        headers.insert(airc_lib::HEADER_STREAM_SEQ.into(), seq.to_string());
        headers.insert(airc_lib::HEADER_STREAM_KIND.into(), kind.to_string());
        if is_final {
            headers.insert(airc_lib::HEADER_STREAM_FINAL.into(), "true".to_string());
        }
        self.port
            .stream(self.request.as_ref(), headers, Body::text(text))
            .await
            .map_err(|error| format!("stream {} sequence {seq}: {error}", self.stream_id))
    }

    /// Drain the command's sink onto the wire until it closes; returns chunks published.
    pub(super) async fn drain(
        self,
        mut rx: tokio::sync::mpsc::UnboundedReceiver<crate::ai::adapter::GenerationChunk>,
    ) -> Result<u64, String> {
        use crate::ai::adapter::GenerationChunk;
        let mut seq = 0u64;
        let mut published = 0u64;
        let mut token = String::new();
        let mut reasoning = String::new();
        let mut prefill: Option<String> = None;
        let mut last_flush = std::time::Instant::now();
        // Recorded once, at the FIRST progress the requester can see (prefill frame or
        // token) — receipt-to-first-progress is the seat's queue, and only the first
        // publish is that measurement; every later flush is generation.
        let mut first_progress_recorded = false;
        loop {
            let next = tokio::time::timeout(STREAM_FLUSH_EVERY, rx.recv()).await;
            let closed = matches!(next, Ok(None));
            if let Ok(Some(chunk)) = next {
                match chunk {
                    GenerationChunk::Token(t) => token.push_str(&t),
                    GenerationChunk::Reasoning(r) => reasoning.push_str(&r),
                    GenerationChunk::Prefill {
                        processed,
                        total,
                        cached,
                    } => {
                        prefill = Some(
                            serde_json::json!({ "processed": processed, "total": total, "cached": cached })
                                .to_string(),
                        );
                    }
                }
            }
            if closed || last_flush.elapsed() >= STREAM_FLUSH_EVERY {
                if !first_progress_recorded
                    && (prefill.is_some() || !token.is_empty() || !reasoning.is_empty())
                {
                    first_progress_recorded = true;
                    crate::inference::resource_admission::note_leased_in_first_progress_ms(
                        self.received_at.elapsed().as_millis().min(u64::MAX as u128) as u64,
                    );
                }
                if let Some(p) = prefill.take() {
                    self.publish(seq, crate::routing::STREAM_KIND_PREFILL, p, false)
                        .await?;
                    published += 1;
                    seq += 1;
                }
                if !reasoning.is_empty() {
                    let flushed = std::mem::take(&mut reasoning);
                    self.publish(seq, airc_lib::STREAM_KIND_TEXT_REASONING, flushed, false)
                        .await?;
                    published += 1;
                    seq += 1;
                }
                if !token.is_empty() {
                    let flushed = std::mem::take(&mut token);
                    self.publish(seq, airc_lib::STREAM_KIND_TEXT_TOKEN, flushed, false)
                        .await?;
                    published += 1;
                    seq += 1;
                }
                last_flush = std::time::Instant::now();
            }
            if closed {
                break;
            }
        }
        self.publish(seq, airc_lib::STREAM_KIND_TEXT_TOKEN, String::new(), true)
            .await?;
        published += 1;
        Ok(published)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::adapter::GenerationChunk;
    use crate::routing::addressed_inbox::fixtures::{FakeFrame, FakePort};

    #[tokio::test]
    async fn failed_progress_is_an_error_and_never_reported_as_drained() {
        let peer = airc_core::PeerId::new();
        let (port, sent) = FakePort::new(peer);
        drop(sent);
        let request = Arc::new(FakeFrame::command(
            airc_core::PeerId::new(),
            peer,
            Some("ai/generate"),
            "ai/generate",
            serde_json::json!({}),
        ));
        let publisher = StreamPublisher {
            stream_id: request.correlation,
            request,
            port,
            received_at: std::time::Instant::now(),
        };
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tx.send(GenerationChunk::Token("answer".into()))
            .expect("local sink open");
        drop(tx);
        assert!(
            publisher.drain(rx).await.is_err(),
            "a failed send cannot become a successful result"
        );
    }

    #[tokio::test]
    async fn successful_drain_orders_final_after_content() {
        let peer = airc_core::PeerId::new();
        let (port, mut sent) = FakePort::new(peer);
        let request = Arc::new(FakeFrame::command(
            airc_core::PeerId::new(),
            peer,
            Some("ai/generate"),
            "ai/generate",
            serde_json::json!({}),
        ));
        let publisher = StreamPublisher {
            stream_id: request.correlation,
            request,
            port,
            received_at: std::time::Instant::now(),
        };
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tx.send(GenerationChunk::Token("answer".into()))
            .expect("local sink open");
        drop(tx);
        assert_eq!(publisher.drain(rx).await.expect("published"), 2);
        let first = sent.recv().await.expect("content");
        let last = sent.recv().await.expect("final");
        assert_eq!(first.headers[airc_lib::HEADER_STREAM_SEQ], "0");
        assert!(!first.headers.contains_key(airc_lib::HEADER_STREAM_FINAL));
        assert_eq!(last.headers[airc_lib::HEADER_STREAM_SEQ], "1");
        assert_eq!(last.headers[airc_lib::HEADER_STREAM_FINAL], "true");
        assert!(first.stream && last.stream);
    }
}
