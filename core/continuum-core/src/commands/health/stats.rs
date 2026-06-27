//! `get-stats` — performance-stats probe (currently a stub).
//!
//! Stateless: self-registers onto the ONE registry via the unit-struct
//! `action_command!` form — no module ceremony, no `commands()` entry.
//!
//! ## Behavior parity
//!
//! Performance-stats tracking is not yet implemented; the legacy handler returned a
//! single `note` field saying so, and this migration preserves that shape verbatim
//! (key `note`) rather than inventing a richer contract before the data exists. When
//! real stats land, this output grows fields — the `note` stub is the honest current
//! state, not a fallback.
//!
//! ## Gating
//!
//! `AiSafe` — read-only inspection; spends no compute, exposes no credentials.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// `get-stats` input — none.
#[derive(Debug, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/health/GetStatsParams.ts"
)]
pub struct GetStatsParams {}

/// `get-stats` output. Stub until performance-stats tracking exists — carries a
/// single `note` explaining the current state (legacy key preserved verbatim).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/health/GetStatsReport.ts"
)]
pub struct GetStatsReport {
    /// Human-readable status of stats collection.
    pub note: String,
}

crate::action_command! {
    /// Report substrate performance statistics. Currently a stub — performance-stats
    /// tracking is not yet implemented, so it returns a `note` saying so. The shape
    /// will grow real fields when the collection layer lands.
    pub struct GetStats;
    name: "get-stats",
    access: AiSafe,
    params: GetStatsParams,
    output: GetStatsReport,
    run(_this, _ctx, _p) => {
        Ok(GetStatsReport {
            note: "Performance stats tracking not yet implemented".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand, Ctx};

    // what this catches: name/access wiring — get-stats is a read-only AiSafe probe,
    // and (stateless) it must self-register onto the registry with no module entry.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(GetStats::NAME, "get-stats");
        assert!(matches!(GetStats::ACCESS, AccessLevel::AiSafe));
    }

    // what this catches: the stub preserves the legacy `note` shape — a caller that
    // expects `note` (the only field the legacy handler emitted) keeps working.
    #[tokio::test]
    async fn returns_the_not_implemented_note() {
        let out = GetStats
            .run(&Ctx::default(), GetStatsParams {})
            .await
            .expect("get-stats never errors");
        assert!(out.note.contains("not yet implemented"));
        let json = serde_json::to_value(&out).unwrap();
        assert!(json["note"].is_string());
    }
}
