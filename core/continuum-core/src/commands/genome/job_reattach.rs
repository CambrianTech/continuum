//! `genome/job-reattach` — after a core restart, ask the job's adapter whether a run a previous
//! core started is still going, and watch it again under the same id if so
//! (SHARED-RESIDENT-LIFECYCLE.md step 3, card 7bb4e5a2).
//!
//! The training trigger calls this for every job the boot replay found open, BEFORE resuming it
//! from its input: an in-engine run survives a core-only restart, and resuming it would POST a
//! second run into the engine the first is still training in. The answer's
//! [`ReattachOutcome::permits_resume`] is the one rule for whether the job may be started again.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::genome::fine_tuning::{FineTuningRegistry, ReattachOutcome};
use crate::sdk_codegen::CommandError;

/// Wire shape for `genome/job-reattach`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/JobReattachParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct JobReattachParams {
    /// The adapter that created the job (the ledger's `provider_id`). Absent when the caller
    /// knows the job's id but not which trainer took it: a training dispatch interrupted
    /// before job-create answered. Then every registered trainer is asked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub provider_id: Option<String>,
    /// The job's substrate-side id (`JobHandle::local_id`).
    #[ts(type = "string")]
    pub local_id: Uuid,
}

crate::action_command! {
    /// After a core restart: re-attach to a training job a previous core started, when its run
    /// is still going (an in-engine run survives a core-only restart), under the same id and
    /// without starting it again. Answers attached, not resident, engine gone, or uncertain;
    /// only not resident and engine gone permit resuming the job from its input.
    pub struct GenomeJobReattach { registry: Arc<FineTuningRegistry> }
    name: "genome/job-reattach",
    access: Privileged,
    params: JobReattachParams,
    output: ReattachOutcome,
    run(this, _ctx, p) => {
        let Some(provider) = p.provider_id else {
            return Ok(across_trainers(&this.registry, p.local_id).await);
        };
        let adapter = this.registry.get(&provider).ok_or_else(|| {
            CommandError::Invalid(format!(
                "genome/job-reattach: no adapter registered for provider {provider:?}"
            ))
        })?;
        adapter
            .reattach(p.local_id)
            .await
            .map_err(|e| CommandError::Internal(format!("genome/job-reattach: {e}")))
    }
}

/// Every registered trainer asked about one job id, as one answer. Only positive evidence
/// counts, as for one trainer: a trainer still running it wins (Attached); any trainer that
/// cannot rule it out holds it (Uncertain, and a trainer that errors cannot rule it out);
/// otherwise a dead engine that held it permits a resume (EngineGone); and no trainer
/// holding it is NotResident.
async fn across_trainers(registry: &FineTuningRegistry, local_id: Uuid) -> ReattachOutcome {
    let (mut uncertain, mut gone) = (Vec::new(), None);
    for provider in registry.list() {
        let Some(adapter) = registry.get(&provider) else { continue };
        match adapter.reattach(local_id).await {
            Ok(ReattachOutcome::Attached { handle }) => return ReattachOutcome::Attached { handle },
            Ok(ReattachOutcome::Uncertain { reason }) => uncertain.push(format!("{provider}: {reason}")),
            Ok(ReattachOutcome::EngineGone { reason }) => gone = Some(format!("{provider}: {reason}")),
            Ok(ReattachOutcome::NotResident) => {}
            Err(e) => uncertain.push(format!("{provider}: {e}")),
        }
    }
    if !uncertain.is_empty() {
        return ReattachOutcome::Uncertain { reason: uncertain.join("; ") };
    }
    match gone {
        Some(reason) => ReattachOutcome::EngineGone { reason },
        None => ReattachOutcome::NotResident,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::genome::test_support::registry_with;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: an adapter whose runs die with the core (every one but the engine's)
    // answering anything but NotResident, which would block the resume that is the only way
    // its orphaned job continues; and the answer's resume rule for that case.
    #[tokio::test]
    async fn an_adapter_whose_runs_die_with_the_core_is_not_resident_and_permits_resume() {
        let cmd = GenomeJobReattach { registry: registry_with(&["openai"]) };
        let out = cmd
            .run(&Ctx::default(), JobReattachParams { provider_id: Some("openai".into()), local_id: Uuid::nil() })
            .await
            .expect("test: a registered provider answers");
        assert!(matches!(out, ReattachOutcome::NotResident));
        assert!(out.permits_resume());
    }
}
