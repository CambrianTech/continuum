//! Streams one served generation's tokens to its requester under its stream id, as
//! they are produced, through the library's stream bound to the request.

use std::sync::Arc;

use airc_core::Body;
use tracing::warn;
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
    async fn publish(&self, seq: u64, kind: &str, text: String, is_final: bool) -> bool {
        let mut headers = airc_core::Headers::new();
        headers.insert(airc_lib::HEADER_STREAM_ID.into(), self.stream_id.to_string());
        headers.insert(airc_lib::HEADER_STREAM_SEQ.into(), seq.to_string());
        headers.insert(airc_lib::HEADER_STREAM_KIND.into(), kind.to_string());
        if is_final {
            headers.insert(airc_lib::HEADER_STREAM_FINAL.into(), "true".to_string());
        }
        match self.port.stream(self.request.as_ref(), headers, Body::text(text)).await {
            Ok(_) => true,
            Err(e) => {
                warn!(correlation = %self.stream_id, seq, error = %e, "stream chunk publish failed");
                false
            }
        }
    }

    /// Drain the command's sink onto the wire until it closes; returns chunks published.
    pub(super) async fn drain(
        self,
        mut rx: tokio::sync::mpsc::UnboundedReceiver<crate::ai::adapter::GenerationChunk>,
    ) -> u64 {
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
                    GenerationChunk::Prefill { processed, total, cached } => {
                        prefill = Some(
                            serde_json::json!({ "processed": processed, "total": total, "cached": cached })
                                .to_string(),
                        );
                    }
                }
            }
            if closed || last_flush.elapsed() >= STREAM_FLUSH_EVERY {
                if !first_progress_recorded && (prefill.is_some() || !token.is_empty() || !reasoning.is_empty()) {
                    first_progress_recorded = true;
                    crate::inference::resource_admission::note_leased_in_first_progress_ms(
                        self.received_at.elapsed().as_millis().min(u64::MAX as u128) as u64,
                    );
                }
                if let Some(p) = prefill.take() {
                    published += self.publish(seq, crate::routing::STREAM_KIND_PREFILL, p, false).await as u64;
                    seq += 1;
                }
                if !reasoning.is_empty() {
                    let flushed = std::mem::take(&mut reasoning);
                    published += self
                        .publish(seq, airc_lib::STREAM_KIND_TEXT_REASONING, flushed, false)
                        .await as u64;
                    seq += 1;
                }
                if !token.is_empty() {
                    let flushed = std::mem::take(&mut token);
                    published += self
                        .publish(seq, airc_lib::STREAM_KIND_TEXT_TOKEN, flushed, false)
                        .await as u64;
                    seq += 1;
                }
                last_flush = std::time::Instant::now();
            }
            if closed {
                break;
            }
        }
        published += self
            .publish(seq, airc_lib::STREAM_KIND_TEXT_TOKEN, String::new(), true)
            .await as u64;
        published
    }
}
