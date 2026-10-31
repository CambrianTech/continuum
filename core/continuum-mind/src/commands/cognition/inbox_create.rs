//! `inbox/create` — ensure a persona's inbox exists (typed, dep-holding).
//!
//! The persona inbox is part of [`PersonaCognition`](crate::persona::PersonaCognition),
//! so "creating" an inbox is just ensuring the persona's cognition state exists — a
//! lazy get-or-create through the module's one lazy-create policy. Captures the owning
//! module's [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! `access: Internal` — host-driven persona lifecycle IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/InboxCreateParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct InboxCreateParams {
    /// Persona whose inbox is ensured.
    #[ts(type = "string")]
    pub persona_id: Uuid,
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/InboxCreateResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct InboxCreateResult {
    pub created: bool,
}

crate::action_command! {
    /// Ensure the persona's inbox (part of its cognition state) exists. Host-invoked
    /// persona lifecycle.
    pub struct InboxCreate { state: Arc<CognitionState> }
    name: "inbox/create",
    access: Internal,
    params: InboxCreateParams,
    output: InboxCreateResult,
    run(this, _ctx, p) => {
        // Ensure persona exists with all state (inbox is part of PersonaCognition).
        this.state.get_or_create_persona(p.persona_id);

        crate::log_info!("module", "cognition", "Ensured inbox for {}", p.persona_id);

        Ok(InboxCreateResult { created: true })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. inbox/create is host-driven
    // persona lifecycle IPC, so it is Internal — registered and grid-routable,
    // never a remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(InboxCreate::NAME, "inbox/create");
        assert_eq!(InboxCreate::ACCESS, AccessLevel::Internal);
    }
}
