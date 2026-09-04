//! `log/write` — queue a single structured log entry for the logger's
//! background writer thread.
//!
//! Dep-holding: captures the shared [`LoggerCommandState`] (the queue sender +
//! lifetime counters) the [`LoggerModule`](crate::modules::logger::LoggerModule)
//! owns. The command only *enqueues*; the disk write happens asynchronously on
//! the writer thread, so `bytes_written` is always 0 — the queue is the contract,
//! not the byte count.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::logger::{LoggerCommandState, WriteLogPayload};

/// Result of `log/write`. `bytesWritten` is always 0 — the real write happens on
/// the logger's background thread; this command confirms only that the entry was
/// queued.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/logger/WriteLogResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct WriteLogResult {
    #[ts(type = "number")]
    pub bytes_written: usize,
}

crate::action_command! {
    /// Queue a single structured log entry (category, level, component, message,
    /// optional args) onto the logger's background writer thread. Returns once the
    /// entry is enqueued; the disk write happens asynchronously. Substrate-internal
    /// plumbing — the in-process `clog_*` macros are the zero-IPC equivalent.
    pub struct LogWrite { state: Arc<LoggerCommandState> }
    name: "log/write",
    access: Internal,
    params: WriteLogPayload,
    output: WriteLogResult,
    run(this, _ctx, p) => {
        this.state
            .log_tx
            .send(p)
            .map_err(|e| format!("Queue send failed: {e}"))?;
        this.state.requests_processed.fetch_add(1, Ordering::Relaxed);
        Ok(WriteLogResult { bytes_written: 0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::logger::LogLevel;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: name/access wiring — log writes are substrate-internal
    // plumbing, not a persona-facing tool, so the access level is Internal.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(LogWrite::NAME, "log/write");
        assert!(matches!(
            LogWrite::ACCESS,
            crate::sdk_codegen::AccessLevel::Internal
        ));
    }

    // what this catches: a write enqueues the payload onto the channel (the
    // receiver sees exactly what was submitted) and bumps the processed counter.
    // A regression that dropped the entry or skipped the counter surfaces here.
    #[tokio::test]
    async fn write_enqueues_payload_and_counts() {
        let (state, rx) = LoggerCommandState::new_for_test();
        let cmd = LogWrite {
            state: state.clone(),
        };
        let payload = WriteLogPayload {
            category: "modules/test".into(),
            level: LogLevel::Info,
            component: "WriteTest".into(),
            message: "hello".into(),
            args: None,
        };
        let result = cmd
            .run(&Ctx::default(), payload)
            .await
            .expect("write must succeed");
        assert_eq!(result.bytes_written, 0);

        let received = rx.try_recv().expect("payload must be queued");
        assert_eq!(received.category, "modules/test");
        assert_eq!(received.message, "hello");
        assert_eq!(state.requests_processed.load(Ordering::Relaxed), 1);
    }
}
