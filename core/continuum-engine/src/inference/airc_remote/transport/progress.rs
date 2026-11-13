//! Decode only progress admitted by the sealed command exchange. Sequence and final
//! markers describe stream completeness; only CommandEvent::Completed settles work.
use super::RemoteInferenceError;
use crate::ai::adapter::GenerationChunk;
use airc_core::{Body, Headers};

#[derive(Default)]
pub(super) struct Progress {
    next: u64,
    finished: bool,
}

fn invalid(message: impl Into<String>) -> RemoteInferenceError {
    RemoteInferenceError::Transport {
        message: message.into(),
    }
}

impl Progress {
    pub(super) fn forward(
        &mut self,
        headers: &Headers,
        body: &Body,
        stream_id: &str,
        sink: Option<&tokio::sync::mpsc::UnboundedSender<GenerationChunk>>,
    ) -> Result<bool, RemoteInferenceError> {
        if self.finished {
            return Err(invalid("progress after final stream marker"));
        }
        if headers.get(airc_lib::HEADER_STREAM_ID).map(String::as_str) != Some(stream_id) {
            return Err(invalid(
                "progress stream id differs from command correlation",
            ));
        }
        let seq = headers
            .get(airc_lib::HEADER_STREAM_SEQ)
            .ok_or_else(|| invalid("progress missing sequence"))?
            .parse::<u64>()
            .map_err(|_| invalid("invalid progress sequence"))?;
        if seq != self.next {
            return Err(invalid("progress sequence gap or duplicate"));
        }
        let is_final = match headers
            .get(airc_lib::HEADER_STREAM_FINAL)
            .map(String::as_str)
        {
            None | Some("false") => false,
            Some("true") => true,
            Some(_) => return Err(invalid("invalid progress final marker")),
        };
        let kind = headers
            .get(airc_lib::HEADER_STREAM_KIND)
            .ok_or_else(|| invalid("progress missing kind"))?;
        let text = body
            .as_text()
            .ok_or_else(|| invalid("progress requires text body"))?;
        let chunk = match kind.as_str() {
            airc_lib::STREAM_KIND_TEXT_TOKEN => GenerationChunk::Token(text.to_owned()),
            airc_lib::STREAM_KIND_TEXT_REASONING => GenerationChunk::Reasoning(text.to_owned()),
            crate::routing::STREAM_KIND_PREFILL => {
                #[derive(serde::Deserialize)]
                struct Prefill {
                    processed: u64,
                    total: u64,
                    cached: u64,
                }
                let value: Prefill = serde_json::from_str(text)
                    .map_err(|error| invalid(format!("invalid prefill progress: {error}")))?;
                GenerationChunk::Prefill {
                    processed: value.processed,
                    total: value.total,
                    cached: value.cached,
                }
            }
            _ => return Err(invalid("unknown progress kind")),
        };
        if let Some(sink) = sink {
            if !text.is_empty() {
                sink.send(chunk)
                    .map_err(|_| invalid("progress sink closed during delivery"))?;
            }
        }
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| invalid("progress sequence exhausted"))?;
        self.finished = is_final;
        Ok(is_final)
    }

    pub(super) fn complete(&self) -> Result<(), RemoteInferenceError> {
        if !self.finished {
            return Err(invalid("command completed before its stream final marker"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn headers(seq: u64, kind: &str, final_chunk: bool) -> Headers {
        let mut h = Headers::from([
            (airc_lib::HEADER_STREAM_ID.into(), "exchange".into()),
            (airc_lib::HEADER_STREAM_SEQ.into(), seq.to_string()),
            (airc_lib::HEADER_STREAM_KIND.into(), kind.into()),
        ]);
        if final_chunk {
            h.insert(airc_lib::HEADER_STREAM_FINAL.into(), "true".into());
        }
        h
    }
    #[test]
    fn ordered_typed_progress_requires_final_before_completion() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut progress = Progress::default();
        assert!(
            progress.complete().is_err(),
            "success requires final even without tokens"
        );
        for (seq, kind, text) in [
            (0, airc_lib::STREAM_KIND_TEXT_REASONING, "thinking"),
            (1, airc_lib::STREAM_KIND_TEXT_TOKEN, "answer"),
            (
                2,
                crate::routing::STREAM_KIND_PREFILL,
                r#"{"processed":1,"total":2,"cached":0}"#,
            ),
        ] {
            assert!(!progress
                .forward(
                    &headers(seq, kind, false),
                    &Body::text(text),
                    "exchange",
                    Some(&tx)
                )
                .expect("valid chunk"));
        }
        assert!(progress.complete().is_err());
        assert!(matches!(rx.try_recv(), Ok(GenerationChunk::Reasoning(_))));
        assert!(matches!(rx.try_recv(), Ok(GenerationChunk::Token(_))));
        assert!(matches!(
            rx.try_recv(),
            Ok(GenerationChunk::Prefill {
                processed: 1,
                total: 2,
                cached: 0
            })
        ));
        assert!(progress
            .forward(
                &headers(3, airc_lib::STREAM_KIND_TEXT_TOKEN, true),
                &Body::text(""),
                "exchange",
                Some(&tx)
            )
            .expect("final"));
        assert!(progress.complete().is_ok());
        assert!(progress
            .forward(
                &headers(4, airc_lib::STREAM_KIND_TEXT_TOKEN, false),
                &Body::text("late"),
                "exchange",
                Some(&tx)
            )
            .is_err());
    }
    #[test]
    fn malformed_or_missing_progress_never_becomes_a_token() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        for (seq, kind, text, stream) in [
            (1, airc_lib::STREAM_KIND_TEXT_TOKEN, "gap", "exchange"),
            (0, "unknown", "not a token", "exchange"),
            (0, crate::routing::STREAM_KIND_PREFILL, "{}", "exchange"),
            (
                0,
                airc_lib::STREAM_KIND_TEXT_TOKEN,
                "wrong stream",
                "another",
            ),
        ] {
            assert!(Progress::default()
                .forward(
                    &headers(seq, kind, false),
                    &Body::text(text),
                    stream,
                    Some(&tx)
                )
                .is_err());
        }
    }
    #[test]
    fn closed_consumer_fails_but_explicit_nonstreaming_still_validates() {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        drop(rx);
        let h = headers(0, airc_lib::STREAM_KIND_TEXT_TOKEN, false);
        assert!(Progress::default()
            .forward(&h, &Body::text("lost"), "exchange", Some(&tx))
            .is_err());
        assert!(Progress::default()
            .forward(&h, &Body::text("discarded by contract"), "exchange", None)
            .is_ok());
        assert!(Progress::default()
            .forward(
                &headers(1, airc_lib::STREAM_KIND_TEXT_TOKEN, false),
                &Body::text("gap"),
                "exchange",
                None
            )
            .is_err());
    }
}
