//! `log/ping` — logger health check: uptime, request count, open-category count,
//! and pending (un-flushed) writes.
//!
//! Read-only introspection over the logger's live counters + open-file cache.
//! Gated `Privileged` — parallel to `runtime/*` introspection: trusted citizens
//! inspect host logging health; untrusted callers do not enumerate host internals.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::logger::LoggerCommandState;

/// Empty params for `log/ping` — it reads live counters and takes no input.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/logger/LoggerPingParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct LoggerPingParams {}

/// Result of `log/ping` — a snapshot of the logger's health counters.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/logger/LoggerPingResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct LoggerPingResult {
    /// Milliseconds since the logger started.
    #[ts(type = "number")]
    pub uptime_ms: u64,
    /// Total `log/*` requests handled over the logger's lifetime.
    #[ts(type = "number")]
    pub requests_processed: u64,
    /// Number of distinct categories with an open log file right now.
    #[ts(type = "number")]
    pub active_categories: usize,
    /// Entries queued but not yet flushed to disk by the writer thread.
    #[ts(type = "number")]
    pub pending_writes: usize,
}

crate::action_command! {
    /// Logger health check: returns uptime, lifetime request count, the number of
    /// categories with an open log file, and the count of queued-but-not-flushed
    /// writes. Read-only introspection over the logger's live state.
    pub struct LogPing { state: Arc<LoggerCommandState> }
    name: "log/ping",
    access: Privileged,
    params: LoggerPingParams,
    output: LoggerPingResult,
    run(this, _ctx, _p) => {
        let active_categories = this
            .state
            .file_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len();
        Ok(LoggerPingResult {
            uptime_ms: this.state.started_at.elapsed().as_millis() as u64,
            requests_processed: this.state.requests_processed.load(Ordering::Relaxed),
            active_categories,
            pending_writes: this.state.pending_writes.load(Ordering::Relaxed) as usize,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: name/access wiring — host-internal introspection is
    // Privileged, parallel to runtime/* (untrusted callers don't enumerate host
    // internals).
    #[test]
    fn name_and_access_wired() {
        assert_eq!(LogPing::NAME, "log/ping");
        assert!(matches!(
            LogPing::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }

    // what this catches: ping reports the live counters — a fresh state has zero
    // requests, zero open categories, zero pending writes, and a non-panicking
    // uptime read.
    #[tokio::test]
    async fn ping_reports_live_counters() {
        let (state, _rx) = LoggerCommandState::new_for_test();
        let cmd = LogPing { state };
        let result = cmd
            .run(&Ctx::default(), LoggerPingParams {})
            .await
            .expect("ping must succeed");
        assert_eq!(result.requests_processed, 0);
        assert_eq!(result.active_categories, 0);
        assert_eq!(result.pending_writes, 0);
    }
}
