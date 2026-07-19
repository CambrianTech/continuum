//! `tool/usage` — the miss-tracking report. What did models actually reach for,
//! and what MISSED?
//!
//! The flywheel (Joel 2026-07-19): kick off a benchmark across a variety of
//! models, then come HERE. Every name they called is tallied by outcome (hit a
//! declared alias / was our canonical name / missed). For each MISS, the report
//! does the did-you-mean for you: a near-match means "add this string as an alias
//! on that command" (a one-minute fix — the command owns its aliases); no match
//! means "a tool we don't have yet." Add the aliases, restart, re-benchmark. It's
//! also the training signal (which model reached wrong → drill it) and works for
//! a first-class citizen (Claude Code over MCP) checking its own fumbles.
//!
//! Reads the in-memory tally ([`crate::cognition::tool_usage`]) which resets each
//! deploy — so after you add aliases and reboot, this reflects the NEW surface,
//! never last session's ghosts. Zero-ceremony STATELESS command.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cognition::tool_usage::{snapshot, Stat};
use crate::commands::help::did_you_mean;
use crate::sdk_codegen::{command_registry, ActionCommand, AccessLevel, CommandError, Ctx};

/// Params for `tool/usage` — no inputs; the report is the whole tally since the
/// last deploy.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/tool/ToolUsageParams.ts")]
pub struct ToolUsageParams {}

/// A tool name that resolved — a declared alias hit, or our canonical name.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/tool/ToolUsageResolvedRow.ts")]
pub struct ToolUsageResolvedRow {
    pub name: String,
    #[ts(type = "number")]
    pub alias_hits: u64,
    #[ts(type = "number")]
    pub canonical: u64,
}

/// A tool name that MISSED — no command answers to it — with the fix suggestion.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/tool/ToolUsageMissRow.ts")]
pub struct ToolUsageMissRow {
    /// The name the model reached for that didn't resolve.
    pub name: String,
    #[ts(type = "number")]
    pub count: u64,
    /// The remedy: closest command(s) to add this as an ALIAS on, or a note that
    /// there is no close match (a tool we likely lack).
    pub suggestion: String,
}

/// Result of `tool/usage` — resolved calls + the actionable miss list.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/tool/ToolUsageReport.ts")]
pub struct ToolUsageReport {
    #[ts(type = "number")]
    pub total_calls: u64,
    #[ts(type = "number")]
    pub miss_count: u64,
    /// Names that didn't resolve, most-called first — each with its remedy.
    pub misses: Vec<ToolUsageMissRow>,
    /// Names that resolved (alias hit or canonical), most-called first.
    pub resolved: Vec<ToolUsageResolvedRow>,
}

/// `tool/usage` — the miss-tracking report.
#[derive(Default)]
pub struct ToolUsage;

#[async_trait]
impl ActionCommand for ToolUsage {
    const NAME: &'static str = "tool/usage";
    const DESCRIPTION: &'static str =
        "Report which tool names models actually called since the last deploy, and — crucially — \
         which MISSED (no command answers to them). Each miss carries the fix: the closest \
         command to add it to as an alias, or a note that we may lack that tool. The first place \
         to look when a benchmark scores low on tool use.";
    type Params = ToolUsageParams;
    type Output = ToolUsageReport;

    async fn run(&self, _ctx: &Ctx, _p: ToolUsageParams) -> Result<ToolUsageReport, CommandError> {
        // AiSafe command names — the candidates a miss could/should map to.
        let ai_names: Vec<&'static str> = command_registry()
            .into_iter()
            .filter(|d| d.access_level == AccessLevel::AiSafe)
            .map(|d| d.name)
            .collect();
        Ok(build_report(snapshot(), &ai_names))
    }
}

/// Fold a usage tally into the report — the pure core (no globals), so it's
/// deterministic to test. `ai_names` are the commands a miss's did-you-mean can
/// point at.
fn build_report(usage: Vec<(String, Stat)>, ai_names: &[&str]) -> ToolUsageReport {
    let mut resolved: Vec<ToolUsageResolvedRow> = Vec::new();
    let mut misses: Vec<ToolUsageMissRow> = Vec::new();
    let mut total_calls = 0u64;

    for (name, stat) in usage {
        total_calls += stat.total();
        if stat.alias_hits + stat.canonical > 0 {
            resolved.push(ToolUsageResolvedRow {
                name: name.clone(),
                alias_hits: stat.alias_hits,
                canonical: stat.canonical,
            });
        }
        if stat.misses > 0 {
            let hits = did_you_mean(&name, ai_names);
            let suggestion = if hits.is_empty() {
                "no close command — likely a tool we don't have yet; add the command, or \
                 rename an existing one to this industry-standard form"
                    .to_string()
            } else {
                format!(
                    "closest: {} — add `{}` as an alias on it (the command owns its aliases), \
                     or rename to this form if it's the industry standard",
                    hits.iter().map(|h| format!("`{h}`")).collect::<Vec<_>>().join(", "),
                    name
                )
            };
            misses.push(ToolUsageMissRow {
                name,
                count: stat.misses,
                suggestion,
            });
        }
    }
    // Most-called first — the biggest wins at the top.
    resolved.sort_by(|a, b| (b.alias_hits + b.canonical).cmp(&(a.alias_hits + a.canonical)));
    misses.sort_by(|a, b| b.count.cmp(&a.count));

    let miss_count = misses.iter().map(|m| m.count).sum();
    ToolUsageReport {
        total_calls,
        miss_count,
        misses,
        resolved,
    }
}
crate::register_stateless_command!(ToolUsage);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the report folds a usage tally into resolved + an
    // ACTIONABLE miss list — a near-miss carries a did-you-mean fix, a real gap
    // says so. This is the benchmark-session gift: run, read, add the string.
    // Tested on the pure `build_report` with a synthetic tally so it's
    // deterministic — no shared-global-state race with the recording seam.
    #[test]
    fn report_folds_tally_into_resolved_and_actionable_misses() {
        let tally = vec![
            ("read_file".to_string(), Stat { alias_hits: 3, canonical: 0, misses: 0 }),
            ("code/read".to_string(), Stat { alias_hits: 0, canonical: 5, misses: 0 }),
            // a name no command answers to, but close to `code/read` — should get
            // a did-you-mean remedy.
            ("read_fil".to_string(), Stat { alias_hits: 0, canonical: 0, misses: 2 }),
            // a name nothing is close to — should get the "tool we lack" remedy.
            ("frobnicate_widget".to_string(), Stat { alias_hits: 0, canonical: 0, misses: 1 }),
        ];
        let ai_names = ["code/read", "code/write", "code/list"];
        let out = build_report(tally, &ai_names);

        assert_eq!(out.total_calls, 3 + 5 + 2 + 1);
        assert_eq!(out.miss_count, 2 + 1);
        // Resolved rows are most-called first: code/read (5) before read_file (3).
        assert_eq!(out.resolved.first().unwrap().name, "code/read");
        assert!(out
            .resolved
            .iter()
            .any(|r| r.name == "read_file" && r.alias_hits == 3));
        // Misses are most-called first, and EVERY miss is actionable.
        assert_eq!(out.misses.first().unwrap().name, "read_fil");
        for miss in &out.misses {
            assert!(
                !miss.suggestion.trim().is_empty(),
                "every miss carries a remedy: {}",
                miss.suggestion
            );
        }
    }
}
