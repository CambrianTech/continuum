//! `cognition/genome-coverage-report` — which skill domains the persona covers vs. gaps
//! (typed, dep-holding).
//!
//! Non-mutating read of the [`GenomePagingEngine`](crate::persona::genome_paging)'s
//! domain-activity ledger: the covered domains, the gaps (domains seen but not yet
//! backed by an adapter), and the coverage ratio — the signal a gap-driven sentinel
//! reads to decide what to train next. Reuses the engine's [`CoverageReport`] as the
//! output. Captures the owning module's
//! [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;
use crate::persona::genome_paging::CoverageReport;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GenomeCoverageReportParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GenomeCoverageReportParams {
    /// Persona whose coverage ledger is read.
    #[ts(type = "string")]
    pub persona_id: Uuid,
}

crate::action_command! {
    /// Report the persona's skill-domain coverage (covered domains, gaps, ratio) —
    /// the signal for gap-driven training. Non-mutating. Host-invoked.
    pub struct GenomeCoverageReport { state: Arc<CognitionState> }
    name: "cognition/genome-coverage-report",
    access: Internal,
    params: GenomeCoverageReportParams,
    output: CoverageReport,
    run(this, _ctx, p) => {
        let persona = this
            .state
            .personas
            .get(&p.persona_id)
            .ok_or_else(|| CommandError::NotFound(format!("No cognition for {}", p.persona_id)))?;
        let report = persona.genome_engine.coverage_report();

        crate::log_info!(
            "module",
            "cognition",
            "genome-coverage-report {}: {} covered, {} gaps, ratio={:.2}",
            p.persona_id,
            report.covered.len(),
            report.gaps.len(),
            report.coverage_ratio
        );

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. genome-coverage-report is
    // host-driven cognition IPC, so it is Internal — registered and grid-routable,
    // never a remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(
            GenomeCoverageReport::NAME,
            "cognition/genome-coverage-report"
        );
        assert_eq!(GenomeCoverageReport::ACCESS, AccessLevel::Internal);
    }
}
