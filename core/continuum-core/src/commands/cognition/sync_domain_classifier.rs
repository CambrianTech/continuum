//! `cognition/sync-domain-classifier` — reconcile the persona's domain classifier to its
//! current genome adapter set (typed, dep-holding).
//!
//! Rebuilds the [`DomainClassifier`](crate::persona::domain_classifier)'s domain→adapter
//! map from the genome engine's active + available adapters, then reports how many domains
//! are known and how many are backed by an adapter (the rest are coverage gaps). Captures
//! the owning module's [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/SyncDomainClassifierParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct SyncDomainClassifierParams {
    /// Persona whose domain classifier is reconciled.
    #[ts(type = "string")]
    pub persona_id: Uuid,
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/SyncDomainClassifierResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct SyncDomainClassifierResult {
    pub synced: bool,
    #[ts(type = "number")]
    pub total_domains: usize,
    #[ts(type = "number")]
    pub covered_domains: usize,
}

crate::action_command! {
    /// Reconcile the persona's domain classifier to its genome adapter set and report
    /// domain coverage (total known, how many backed by an adapter). Host-invoked.
    pub struct SyncDomainClassifier { state: Arc<CognitionState> }
    name: "cognition/sync-domain-classifier",
    access: Internal,
    params: SyncDomainClassifierParams,
    output: SyncDomainClassifierResult,
    run(this, _ctx, p) => {
        let mut persona = this.state.get_or_create_persona(p.persona_id);

        // Build adapter list from genome engine state.
        let state = persona.genome_engine.state();
        let all_adapters: Vec<_> = state
            .active_adapters
            .iter()
            .chain(state.available_adapters.iter())
            .cloned()
            .collect();

        persona.domain_classifier.sync_from_adapters(&all_adapters);

        let summary = persona.domain_classifier.domain_summary();
        let covered_domains = summary.iter().filter(|(_, has)| *has).count();
        let total_domains = summary.len();

        crate::log_info!(
            "module",
            "cognition",
            "sync-domain-classifier {}: {} domains ({} with adapters)",
            p.persona_id,
            total_domains,
            covered_domains
        );

        Ok(SyncDomainClassifierResult {
            synced: true,
            total_domains,
            covered_domains,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. sync-domain-classifier is
    // host-driven cognition IPC, so it is Internal — registered and grid-routable,
    // never a remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(
            SyncDomainClassifier::NAME,
            "cognition/sync-domain-classifier"
        );
        assert_eq!(SyncDomainClassifier::ACCESS, AccessLevel::Internal);
    }
}
