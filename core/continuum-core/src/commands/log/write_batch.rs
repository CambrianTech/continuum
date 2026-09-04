//! `log/write-batch` — queue multiple structured log entries in one call.
//!
//! Same dep-holding shape as [`super::write`]: captures the shared
//! [`LoggerCommandState`]. Best-effort enqueue per entry (`try_send`) so a full
//! queue drops the overflow rather than blocking the caller — batch logging must
//! never become back-pressure on the submitter.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::logger::{LoggerCommandState, WriteLogPayload};

/// Batch payload for `log/write-batch`: a list of entries to enqueue in one call.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/logger/WriteLogBatchPayload.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct WriteLogBatchPayload {
    /// The entries to queue. Each is the same shape as a single `log/write`.
    pub entries: Vec<WriteLogPayload>,
}

/// Result of `log/write-batch`: how many entries were accepted onto the queue.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/logger/WriteLogBatchResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct WriteLogBatchResult {
    #[ts(type = "number")]
    pub entries_queued: usize,
}

crate::action_command! {
    /// Queue multiple structured log entries onto the logger's background writer
    /// thread in a single call. Best-effort: a full queue drops overflow rather
    /// than blocking the submitter. Substrate-internal plumbing — the per-entry
    /// `clog_*` macros are the in-process equivalent.
    pub struct LogWriteBatch { state: Arc<LoggerCommandState> }
    name: "log/write-batch",
    access: Internal,
    params: WriteLogBatchPayload,
    output: WriteLogBatchResult,
    run(this, _ctx, p) => {
        let entries_queued = p.entries.len();
        for entry in p.entries {
            // Best-effort: drop on a full queue rather than block the submitter.
            let _ = this.state.log_tx.try_send(entry);
        }
        this.state.requests_processed.fetch_add(1, Ordering::Relaxed);
        Ok(WriteLogBatchResult { entries_queued })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::logger::LogLevel;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    fn entry(message: &str) -> WriteLogPayload {
        WriteLogPayload {
            category: "modules/test".into(),
            level: LogLevel::Info,
            component: "BatchTest".into(),
            message: message.into(),
            args: None,
        }
    }

    // what this catches: name/access wiring — batch writes are substrate-internal.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(LogWriteBatch::NAME, "log/write-batch");
        assert!(matches!(
            LogWriteBatch::ACCESS,
            crate::sdk_codegen::AccessLevel::Internal
        ));
    }

    // what this catches: every entry in the batch is enqueued (count reported and
    // all payloads land on the channel) and the processed counter ticks once per
    // batch, not once per entry.
    #[tokio::test]
    async fn batch_enqueues_every_entry() {
        let (state, rx) = LoggerCommandState::new_for_test();
        let cmd = LogWriteBatch {
            state: state.clone(),
        };
        let payload = WriteLogBatchPayload {
            entries: vec![entry("one"), entry("two"), entry("three")],
        };
        let result = cmd
            .run(&Ctx::default(), payload)
            .await
            .expect("batch must succeed");
        assert_eq!(result.entries_queued, 3);

        let drained: Vec<String> = std::iter::from_fn(|| rx.try_recv().ok())
            .map(|p| p.message)
            .collect();
        assert_eq!(drained, vec!["one", "two", "three"]);
        assert_eq!(state.requests_processed.load(Ordering::Relaxed), 1);
    }
}
