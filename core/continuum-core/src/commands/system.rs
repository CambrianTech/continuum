//! `system/info` — substrate build + process identity, for compatibility checks.
//!
//! A second worked example of the zero-ceremony stateless command (after
//! `commands/list`): one file, one `run` body, `register_stateless_command!`, and
//! it's instantly callable via `cu system/info`, the persona's tools, and every
//! SDK — with a derived param schema and ACL gating, no wiring anywhere else. This
//! is the "minimal code per command" the ported catalog will look like.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};

/// Params for `system/info` — none today (a placeholder struct so the command has
/// a canonical, schema-able params type like every other command).
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/system/SystemInfoParams.ts")]
pub struct SystemInfoParams {}

/// Result of `system/info` — what a client needs to confirm it's talking to a
/// compatible, live substrate.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/system/SystemInfoResult.ts")]
pub struct SystemInfoResult {
    /// The continuum-core crate version (`CARGO_PKG_VERSION`).
    pub version: String,
    /// OS process id of the running core — handy for ops (`cu stop` targets it).
    pub pid: u32,
}

/// `system/info` — build + process identity. Stateless, AiSafe.
#[derive(Default)]
pub struct SystemInfo;

#[async_trait]
impl ActionCommand for SystemInfo {
    const NAME: &'static str = "system/info";
    const DESCRIPTION: &'static str =
        "Report the running substrate's version and process id — for client \
         compatibility checks and ops.";
    type Params = SystemInfoParams;
    type Output = SystemInfoResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        _params: SystemInfoParams,
    ) -> Result<SystemInfoResult, CommandError> {
        Ok(SystemInfoResult {
            version: env!("CARGO_PKG_VERSION").to_string(),
            pid: std::process::id(),
        })
    }
}
crate::register_stateless_command!(SystemInfo);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: system/info is a complete, routable command from one
    // file — it reports a non-empty version and the live pid, and (via the stateless
    // registration proven elsewhere) is on the typed object map with no wiring. The
    // "minimal code per command" guarantee, exercised.
    #[tokio::test]
    async fn reports_version_and_pid() {
        let out = SystemInfo
            .run(&Ctx::default(), SystemInfoParams {})
            .await
            .expect("ok");
        assert!(!out.version.is_empty(), "version present");
        assert_eq!(out.pid, std::process::id(), "live process id");
    }
}
