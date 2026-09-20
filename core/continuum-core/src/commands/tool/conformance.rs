//! `tool/conformance` — run the tool AI-usability audit on demand.
//!
//! The static half of the #163 conformance harness, surfaced as a command so a
//! persona (debugging why a hand didn't work) or an operator (a pre-merge check)
//! can ask "is every tool actually usable?" and get the SAME computation the CI
//! gate runs — never a hand-authored list. It reads `command_registry()` (the ONE
//! source) and reports every AiSafe tool that fails the floor: unnameable,
//! undescribed, or with an unlearnable params schema.
//!
//! Zero-ceremony STATELESS command — it dogfoods the discovery surface it audits.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sdk_codegen::conformance::{audit_tool_conformance, audited_tool_count};
use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};

/// Params for `tool/conformance` — an optional case-insensitive substring to
/// filter violations by tool name (so "why can't I use `data/*`?" is answerable).
/// Empty ⇒ the whole surface.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/tool/ToolConformanceParams.ts"
)]
pub struct ToolConformanceParams {
    /// Optional substring filter on the offending tool name (case-insensitive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub filter: Option<String>,
}

/// One tool that fails the AI-usability floor — the tool, which rule it broke, and
/// the fix (the detail doubles as the how-to-fix).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/tool/ToolConformanceViolationInfo.ts"
)]
pub struct ToolConformanceViolationInfo {
    /// The offending tool's name.
    pub tool: String,
    /// The rule it broke (`name-emittable` / `description-present` / `params-learnable`).
    pub rule: String,
    /// What's wrong and how to fix it, in the persona's paradigm.
    pub detail: String,
}

/// Result of `tool/conformance` — the audit outcome.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/tool/ToolConformanceReport.ts"
)]
pub struct ToolConformanceReport {
    /// True when NO tool (matching the filter) fails the floor — declared first so
    /// the verdict serializes at the head of the JSON even if the array is folded.
    pub conformant: bool,
    /// How many AiSafe tools the audit examined (the whole reachable surface).
    #[ts(type = "number")]
    pub examined: usize,
    /// How many violations matched (post-filter).
    #[ts(type = "number")]
    pub violation_count: usize,
    /// Every matching violation, so a fix pass sees the whole surface at once.
    pub violations: Vec<ToolConformanceViolationInfo>,
}

/// `tool/conformance` — the AI-usability audit, on demand.
#[derive(Default)]
pub struct ToolConformance;

#[async_trait]
impl ActionCommand for ToolConformance {
    const NAME: &'static str = "tool/conformance";
    const DESCRIPTION: &'static str =
        "Audit whether every tool a persona can reach is actually usable — nameable, described, \
         and with a learnable input schema. Reports each failing tool + how to fix it. Optional \
         `filter` narrows to tool names containing a substring.";
    type Params = ToolConformanceParams;
    type Output = ToolConformanceReport;

    async fn run(
        &self,
        _ctx: &Ctx,
        params: ToolConformanceParams,
    ) -> Result<ToolConformanceReport, CommandError> {
        let needle = params.filter.as_deref().map(|s| s.to_lowercase());
        let violations: Vec<ToolConformanceViolationInfo> = audit_tool_conformance()
            .into_iter()
            .filter(|v| match &needle {
                Some(n) => v.tool.to_lowercase().contains(n),
                None => true,
            })
            .map(|v| ToolConformanceViolationInfo {
                tool: v.tool,
                rule: v.rule.to_string(),
                detail: v.detail,
            })
            .collect();
        Ok(ToolConformanceReport {
            conformant: violations.is_empty(),
            examined: audited_tool_count(),
            violation_count: violations.len(),
            violations,
        })
    }
}
crate::register_stateless_command!(ToolConformance);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the audit is reachable as a command AND agrees with the
    // gate — the live surface is conformant (examined a real number of tools, zero
    // violations). If this ever goes non-conformant, this command names the tool +
    // fix, exactly like the CI gate; they read the SAME computation, so they can't
    // disagree.
    #[tokio::test]
    async fn reports_a_conformant_non_vacuous_surface() {
        let out = ToolConformance
            .run(&Ctx::default(), ToolConformanceParams::default())
            .await
            .expect("ok");
        assert!(
            out.examined >= 40,
            "must examine the real surface, not nothing: {} tools",
            out.examined
        );
        assert!(
            out.conformant && out.violation_count == 0,
            "live surface must clear the floor — offenders:\n{}",
            out.violations
                .iter()
                .map(|v| format!("  [{}] {} — {}", v.rule, v.tool, v.detail))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    // what this catches: the filter narrows to the named tool family (so "why can't
    // I use data/*?" is answerable), and a filter that matches nothing is still a
    // clean conformant=true empty report, never an error.
    #[tokio::test]
    async fn filter_narrows_and_empty_match_is_clean() {
        let none = ToolConformance
            .run(
                &Ctx::default(),
                ToolConformanceParams {
                    filter: Some("this-tool-name-cannot-exist".to_string()),
                },
            )
            .await
            .expect("ok");
        assert_eq!(none.violation_count, 0);
        assert!(none.conformant, "no matches ⇒ conformant, not an error");
        // examined is the WHOLE surface regardless of filter (the filter is on the
        // violation set, not the audited set) — so the count stays meaningful.
        assert!(none.examined >= 40);
    }
}
