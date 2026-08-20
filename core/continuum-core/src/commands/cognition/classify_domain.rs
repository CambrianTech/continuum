//! `cognition/classify-domain` — score text into a skill domain via the persona's
//! adapter-aware keyword classifier (typed, dep-holding).
//!
//! Non-mutating read of the persona's [`DomainClassifier`](crate::persona::domain_classifier):
//! returns the best-matching domain, a confidence score, the covering adapter name (or
//! `None` when the domain is recognized but no adapter backs it — a coverage gap), and the
//! decision latency. Reuses the classifier's [`DomainClassification`] as the output.
//! Captures the owning module's [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;
use crate::persona::domain_classifier::DomainClassification;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ClassifyDomainParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ClassifyDomainParams {
    /// Persona whose domain classifier scores the text.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// The text to classify into a skill domain.
    pub text: String,
}

crate::action_command! {
    /// Classify text into the persona's best-matching skill domain (domain, confidence,
    /// covering adapter or gap, latency). Non-mutating. Host-invoked.
    pub struct ClassifyDomain { state: Arc<CognitionState> }
    name: "cognition/classify-domain",
    access: Internal,
    params: ClassifyDomainParams,
    output: DomainClassification,
    run(this, _ctx, p) => {
        let persona = this
            .state
            .personas
            .get(&p.persona_id)
            .ok_or_else(|| CommandError::NotFound(format!("No cognition for {}", p.persona_id)))?;
        let result = persona.domain_classifier.classify(&p.text);

        crate::log_info!(
            "module",
            "cognition",
            "classify-domain {}: '{}...' → domain={}, confidence={:.2}, adapter={:?} ({:.0}μs)",
            p.persona_id,
            crate::utils::str_truncate::truncate_at_char_boundary(&p.text, 40),
            result.domain,
            result.confidence,
            result.adapter_name,
            result.decision_time_us
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. classify-domain is host-driven
    // cognition IPC, so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(ClassifyDomain::NAME, "cognition/classify-domain");
        assert_eq!(ClassifyDomain::ACCESS, AccessLevel::Internal);
    }
}
