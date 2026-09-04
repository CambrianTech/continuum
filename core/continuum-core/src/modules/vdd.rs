//! `vdd/report` IPC module — Lane C PR-3 of the doc's
//! [Lane C VDD telemetry substrate] sequence.
//!
//! Consumes the pure read-side primitive from
//! `crate::vdd::reader` and emits a structured JSON report so
//! callers (CI dashboards, the chat-roundtrip post-mortem
//! command, sentinel attribution) stop scraping random console
//! text. Every claim "VDD: tokens/sec improved from X → Y" in a
//! PR body should be a query against this command, not a paste
//! from a terminal.
//!
//! Commands:
//! - `vdd/report` — read records from `~/.continuum/vdd/...`,
//!   apply optional git_sha / scenario filters, return list of
//!   matching records + a small aggregate summary.
//!
//! Failure modes (per Joel's never-swallow rule):
//! - Corrupt `record.jsonl` → typed Err, surface the parse error
//!   with the file path so the caller can `cat` the bad artifact.
//! - Missing artifact root → empty result (NOT error); fresh dev
//!   machine has nothing to report and that's a valid state.
//!
//! NOT in this slice:
//! - Cross-PR regression detection (compare two git_shas + flag
//!   tokens/sec regressions). That's a separate report mode that
//!   builds on this primitive — adds a `mode: "regression"` param.
//! - Subscribing to live `RuntimeMetric` events from inference
//!   paths (Lane C PR-1/PR-2 prereqs). This command reads what
//!   the harness has already written; the live-emit path lands
//!   when those PRs are bound.

use crate::commands::vdd::command_objects;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::sdk_codegen::DynCommand;
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::path::PathBuf;
use std::sync::Arc;

pub struct VddModule {
    /// Artifact root. In production this points at
    /// `~/.continuum/vdd`; in tests, the harness wires a temp
    /// dir so test data doesn't leak into the dev's real
    /// artifact store.
    artifact_root: PathBuf,
}

impl VddModule {
    pub fn new() -> Self {
        Self {
            artifact_root: default_artifact_root(),
        }
    }

    /// Constructor for tests + non-default deployments. Allows
    /// pointing the module at any artifact root.
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            artifact_root: root.into(),
        }
    }
}

impl Default for VddModule {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve `~/.continuum/vdd` as the canonical artifact root.
/// Matches `vdd::ArtifactWriter::continuum_default()` — that's the
/// writer's path; this is the reader's path; they must agree.
fn default_artifact_root() -> PathBuf {
    dirs::home_dir()
        .expect("home directory must exist for VDD artifact reads")
        .join(".continuum")
        .join("vdd")
}

#[async_trait]
impl ServiceModule for VddModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "vdd",
            priority: ModulePriority::Background,
            command_prefixes: &["vdd/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            // Pure-read + bounded fs scan; no need to cap fan-out.
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // Migrated to the typed registry (`commands/vdd/{report,score}.rs`). The
        // legacy string-match surface is retired; fail loud rather than silently
        // route a stale name (per Joel's never-swallow rule).
        Err(format!(
            "vdd command surface is migrated to the typed registry; '{command}' has no legacy handler"
        ))
    }

    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        // `vdd/report` is dep-holding (captures the artifact root). `vdd/score` is
        // stateless and self-registers via the inventory, so it is NOT listed here.
        command_objects(self.artifact_root.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    //! The module now owns only config + the artifact root + the dep-holding
    //! family. The command contracts themselves are pinned in
    //! `commands/vdd/{report,score}.rs`; these tests guard the module's wiring.
    use super::*;
    use crate::sdk_codegen::DynCommand;

    /// What this catches: config exposes the canonical `vdd/` prefix + module
    /// name. If either drifts, the registry routes the command elsewhere.
    #[test]
    fn config_reports_name_and_prefix() {
        let m = VddModule::new();
        let cfg = m.config();
        assert_eq!(cfg.name, "vdd");
        assert_eq!(cfg.command_prefixes, &["vdd/"]);
    }

    /// What this catches: the legacy string-match surface is retired — any
    /// `handle_command` call fails loud naming the command (never silently
    /// swallows or routes a stale name), per Joel's never-swallow rule.
    #[tokio::test]
    async fn legacy_handle_command_fails_loud() {
        let m = VddModule::new();
        let err = m
            .handle_command("vdd/report", serde_json::json!({}))
            .await
            .expect_err("migrated surface must fail loud");
        assert!(err.contains("migrated to the typed registry"));
        assert!(err.contains("vdd/report"));
    }

    /// What this catches: the module contributes exactly the dep-holding
    /// `vdd/report` to the typed registry (carrying its artifact root). `vdd/score`
    /// is stateless and self-registers, so it is NOT in this list — a regression
    /// that double-registers it would trip the duplicate-name panic.
    #[test]
    fn contributes_the_report_command() {
        let m = VddModule::with_root("/tmp/vdd-wiring-test");
        let names: Vec<&str> = m.commands().iter().map(|c| c.name()).collect();
        assert_eq!(names, vec!["vdd/report"]);
    }
}
