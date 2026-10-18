//! `benchmark/scoreboard`: aggregate scores per benchmark from recorded verdicts.

use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};
use crate::cognition::swe_bench;
use super::*;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkScoreboardParams.ts")]
pub struct BenchmarkScoreboardParams {}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkScoreRow.ts")]
pub struct BenchmarkScoreRow {
    /// Catalog benchmark this tallies (e.g. `swe-bench-verified`).
    pub benchmark: String,
    /// Distinct instances with a REAL verdict (error-free; absences never tally).
    #[ts(type = "number")]
    pub attempted: u32,
    /// Of those, resolved (all fail-to-pass passed, gate held).
    #[ts(type = "number")]
    pub resolved: u32,
    /// The dataset's full size — the leaderboard denominator this samples.
    #[ts(type = "number")]
    pub dataset_size: u32,
    /// Resolved instance ids — the receipt pointers.
    pub resolved_instances: Vec<String>,
    /// The harness build(s) that produced this row's verdicts, newest-first.
    ///
    /// MORE THAN ONE MEANS THE ROW BLENDS ERAS — and a rate averaged over a
    /// moving instrument is not a measurement. Measured 2026-08-28: 19 of 32
    /// verdicts here had been scored across ten days by three harness builds,
    /// and regrading one from its IDENTICAL banked patch moved pass-to-pass
    /// from 0/40 to 40/40. Publishing an improvement curve over that is
    /// publishing the harness's changes as if they were hers. `<unstamped>`
    /// marks verdicts written before provenance existed.
    #[serde(default)]
    pub harness_builds: Vec<String>,
    /// ENV failures (verdict carries `error`): the harness could not measure
    /// the model at all — clone/env/patch infrastructure, NEVER a model miss.
    /// These are absences owing retakes, and they must read that way.
    #[ts(type = "number")]
    pub env_absences: u32,
    /// The env-absent instance ids, so the failure is chaseable per instance.
    pub env_absent_instances: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkScoreboardResult.ts")]
pub struct BenchmarkScoreboardResult {
    pub rows: Vec<BenchmarkScoreRow>,
    /// The REGIME every published number must carry: model, window, build, host.
    pub regime: String,
    pub summary: String,
}

#[derive(Default)]
pub struct BenchmarkScoreboard;

#[async_trait::async_trait]
impl ActionCommand for BenchmarkScoreboard {
    const NAME: &'static str = "benchmark/scoreboard";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "The SCORE rollup: per SWE-class benchmark, attempted vs resolved from the durable \
         verdicts on disk, with the serving REGIME (model, window, build sha, host) every \
         published claim must carry. ONE read for the operator, the README chart, and a \
         citizen grounding on how the team is scoring — instead of tallying verdict files \
         by hand.";
    type Params = BenchmarkScoreboardParams;
    type Output = BenchmarkScoreboardResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        _p: BenchmarkScoreboardParams,
    ) -> Result<BenchmarkScoreboardResult, CommandError> {
        let verdicts = swe_bench::recorded_verdicts();
        let mut rows = Vec::new();
        for spec in known_benchmarks() {
            let Some(dataset) = spec.swe_dataset() else {
                continue;
            };
            // Membership by the SAME loader the grade path searches with — one
            // source of truth for "which dataset does this instance belong to".
            let Ok(instances) = crate::cognition::swe_bench::load_dataset(dataset).await else {
                continue; // not fetched yet — an un-run benchmark, not an error
            };
            let ids: std::collections::HashSet<&str> =
                instances.iter().map(|i| i.instance_id.as_str()).collect();
            let mut attempted = 0u32;
            let mut resolved_instances = Vec::new();
            let mut env_absent_instances = Vec::new();
            // Which harness build(s) actually produced the verdicts behind this
            // rate. Collected from the TALLIED verdicts only — the absences are
            // not part of the claim.
            let mut builds: std::collections::BTreeSet<String> = Default::default();
            for (id, v) in &verdicts {
                if !ids.contains(id.as_str()) {
                    continue;
                }
                if v.error.is_some() || !v.gate_ok {
                    // Absence, never a tallied attempt — but never invisible
                    // either: an env failure the user can't see reads as a
                    // model miss in every retelling. A failed GATE is the same
                    // class: the control was broken, nothing was measured
                    // (belt-and-suspenders — record_verdict already refuses
                    // gate-failed verdicts, so this arm should never fire).
                    env_absent_instances.push(id.clone());
                    continue;
                }
                attempted += 1;
                builds.insert(if v.harness_build.is_empty() {
                    "<unstamped>".to_string()
                } else {
                    v.harness_build.clone()
                });
                if v.resolved {
                    resolved_instances.push(id.clone());
                }
            }
            rows.push(BenchmarkScoreRow {
                benchmark: spec.name.to_string(),
                attempted,
                resolved: resolved_instances.len() as u32,
                dataset_size: instances.len() as u32,
                resolved_instances,
                harness_builds: {
                    let mut b: Vec<String> = builds.into_iter().collect();
                    b.sort();
                    b
                },
                env_absences: env_absent_instances.len() as u32,
                env_absent_instances,
            });
        }
        let serving = crate::inference::llama_server::current_serving();
        let regime = format!(
            "model={} served_window={} build={} sha={} host={}-{}",
            serving.active_model.as_deref().unwrap_or("none-serving"), // absence stated, never a fake model id
            serving.served_context_window,
            env!("CONTINUUM_BUILD_NUMBER"),
            env!("CONTINUUM_BUILD_GIT_SHA"),
            std::env::consts::OS,
            std::env::consts::ARCH,
        );
        let summary = rows
            .iter()
            .filter(|r| r.attempted > 0 || r.env_absences > 0)
            .map(|r| {
                let env = if r.env_absences > 0 {
                    format!(
                        " · {} ENV failure(s) — not model misses, they owe retakes: {}",
                        r.env_absences,
                        r.env_absent_instances.join(", ")
                    )
                } else {
                    String::new()
                };
                // SAY IT WHEN THE RATE BLENDS ERAS. A number averaged over a
                // moving instrument is not a measurement, and this string is
                // what gets quoted into charts and READMEs — the exact place a
                // silent blend becomes a published claim.
                let mixed = if r.harness_builds.len() > 1 {
                    format!(
                        " · ⚠ MIXED HARNESS ERAS ({}) — regrade to one build before \
                         publishing this as a curve",
                        r.harness_builds.join(", ")
                    )
                } else {
                    String::new()
                };
                format!(
                    "{}: {}/{} resolved (of {} in the set){env}{mixed}",
                    r.benchmark, r.resolved, r.attempted, r.dataset_size
                )
            })
            .collect::<Vec<_>>()
            .join(" · ");
        let summary = if summary.is_empty() {
            "no real verdicts on disk yet — dispatch a round first".to_string()
        } else {
            summary
        };
        Ok(BenchmarkScoreboardResult {
            rows,
            regime,
            summary,
        })
    }
}

crate::register_stateless_command!(BenchmarkScoreboard);
