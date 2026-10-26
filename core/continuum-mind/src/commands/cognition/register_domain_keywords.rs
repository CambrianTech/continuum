//! `cognition/register-domain-keywords` — add keywords to a domain in the persona's
//! classifier vocabulary (typed, dep-holding).
//!
//! Extends the [`DomainClassifier`](crate::persona::domain_classifier)'s keyword set for a
//! named domain so subsequent [`super::classify_domain`] calls can recognize it. Captures
//! the owning module's [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/RegisterDomainKeywordsParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct RegisterDomainKeywordsParams {
    /// Persona whose classifier vocabulary is extended.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// The domain to register keywords under (e.g. "code", "plumbing").
    pub domain: String,
    /// Keywords to associate with the domain.
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/RegisterDomainKeywordsResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct RegisterDomainKeywordsResult {
    pub registered: bool,
    pub domain: String,
    #[ts(type = "number")]
    pub keywords_added: usize,
}

crate::action_command! {
    /// Add keywords to a domain in the persona's classifier vocabulary so classify-domain
    /// can recognize it. Host-invoked.
    pub struct RegisterDomainKeywords { state: Arc<CognitionState> }
    name: "cognition/register-domain-keywords",
    access: Internal,
    params: RegisterDomainKeywordsParams,
    output: RegisterDomainKeywordsResult,
    run(this, _ctx, p) => {
        let keywords_added = p.keywords.len();
        let mut persona = this.state.get_or_create_persona(p.persona_id);
        persona
            .domain_classifier
            .register_domain_keywords(&p.domain, p.keywords);

        crate::log_info!(
            "module",
            "cognition",
            "register-domain-keywords {}: added {} keywords to domain '{}'",
            p.persona_id,
            keywords_added,
            p.domain
        );

        Ok(RegisterDomainKeywordsResult {
            registered: true,
            domain: p.domain,
            keywords_added,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. register-domain-keywords is
    // host-driven cognition IPC, so it is Internal — registered and grid-routable,
    // never a remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(
            RegisterDomainKeywords::NAME,
            "cognition/register-domain-keywords"
        );
        assert_eq!(RegisterDomainKeywords::ACCESS, AccessLevel::Internal);
    }
}
