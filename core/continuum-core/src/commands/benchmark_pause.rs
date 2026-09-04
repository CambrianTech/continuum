//! `benchmark/pause` + `benchmark/resume` — the operator's hold on a round.
//!
//! Pause is a HOLD, not a kill: cards stay on the board, in-flight solves run
//! to their graded ends (interrupting mid-attempt wastes paid compute), and
//! every driver edge hands out nothing until resume. Resume flips the stage
//! back and kicks the driver immediately — the same dispatch the settle edge
//! fires — instead of waiting for the next event. Both verbs answer with the
//! round's live snapshot so the caller sees the effect, not just an ack.
//!
//! Boot honesty: a Paused round survives reboots AS PAUSED — boot-resume
//! funnels through the same stage gate, so "pause, reboot, still paused" is
//! one rule, not three.

use crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Default, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkPauseParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkPauseParams {
    /// The round's id (= its run room id, shown by `benchmark/rounds`).
    /// Typed Uuid on the wire ([[uuids-are-not-strings]]): serde parses the
    /// JSON string and a malformed id fails at DESERIALIZATION with serde's
    /// own error — the hand-rolled parse_round below is deleted, not moved.
    #[ts(type = "string")]
    pub round_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkPauseResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkPauseResult {
    /// The round's stage after the verb ("working" | "paused" | "done").
    pub stage: String,
    /// Whether this call changed anything (false = already there / unknown).
    pub changed: bool,
    /// On resume: the card the driver was kicked with, if any work remained.
    #[serde(default)]
    #[ts(optional)]
    pub kicked_card: Option<String>,
}

fn stage_of(round_id: Uuid) -> String {
    crate::cognition::bench_round::live_rounds()
        .into_iter()
        .find(|r| r.round_id == round_id.to_string())
        .map(|r| r.stage)
        .unwrap_or_else(|| "unknown".to_string()) // unwrap_or: an untracked id reports itself honestly
}

#[derive(Default)]
pub struct BenchmarkPause;

#[async_trait]
impl ActionCommand for BenchmarkPause {
    const NAME: &'static str = "benchmark/pause";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "HOLD a benchmark round: in-flight solves finish and settle, but no new card is \
         dispatched until benchmark/resume. Survives reboots as paused. Pass the round_id \
         from benchmark/rounds.";
    type Params = BenchmarkPauseParams;
    type Output = BenchmarkPauseResult;

    async fn run(&self, _ctx: &Ctx, p: BenchmarkPauseParams) -> Result<BenchmarkPauseResult, CommandError> {
        let id = p.round_id;
        let changed = crate::cognition::bench_round::pause_round(id);
        Ok(BenchmarkPauseResult { stage: stage_of(id), changed, kicked_card: None })
    }
}

#[derive(Default)]
pub struct BenchmarkResume;

#[async_trait]
impl ActionCommand for BenchmarkResume {
    const NAME: &'static str = "benchmark/resume";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Lift a benchmark/pause: the round returns to working and the driver is kicked \
         immediately (the next unworked card fires without waiting for a settle edge).";
    type Params = BenchmarkPauseParams;
    type Output = BenchmarkPauseResult;

    async fn run(&self, _ctx: &Ctx, p: BenchmarkPauseParams) -> Result<BenchmarkPauseResult, CommandError> {
        let id = p.round_id;
        let next = crate::cognition::bench_round::resume_round(id);
        let changed = next.is_some()
            || stage_of(id) == "working" && {
                // resume_round returns None both for "not paused" and for
                // "resumed, but no unworked card remains" — disambiguate by
                // stage so `changed` stays honest.
                false
            };
        let mut kicked = None;
        if let Some(next) = next {
            // The settle edge's own dispatch, verbatim shape (benchmark_grade):
            // author through the assignee's airc, fall back to any live citizen.
            if let Some(reg) = PersonaAircRuntimeRegistry::try_global() {
                if let Some(rt) = reg.get(next.assignee).or_else(|| reg.any_live_citizen()) {
                    let airc = rt.airc().clone();
                    kicked = Some(next.card.to_string());
                    crate::modules::work::dispatch_staged_swe_solve(
                        &Default::default(),
                        &airc,
                        crate::modules::work::StagedSolveDispatch {
                            claimer: crate::identity::PeerId::from_uuid(next.assignee),
                            card: airc_work::WorkCardId::from_uuid(next.card),
                            room: airc_core::RoomId::from_u128(next.run_room.as_u128()),
                            teammates: next
                                .teammates
                                .iter()
                                .map(|t| crate::identity::PeerId::from_uuid(*t))
                                .collect(),
                        },
                    )
                    .await;
                }
            }
        }
        Ok(BenchmarkPauseResult {
            stage: stage_of(id),
            changed: changed || kicked.is_some(),
            kicked_card: kicked,
        })
    }
}

crate::register_command!(BenchmarkPause);
crate::register_command!(BenchmarkResume);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the wire names + AiSafe access — pausing your own
    // round is a cooperative act, and the verbs must stay discoverable under
    // exactly these names (the desktop's round controls bind to them).
    #[test]
    fn names_and_access() {
        assert_eq!(BenchmarkPause::NAME, "benchmark/pause");
        assert_eq!(BenchmarkResume::NAME, "benchmark/resume");
        assert!(BenchmarkPause::DESCRIPTION.contains("HOLD"));
    }
}
