//! `events/declare-class` — register a new event class with its transport-routing
//! metadata. Idempotent for identical re-declarations; fails loud on a conflicting
//! re-declaration (wire-contract integrity). Substrate bootstrap, invoked
//! in-process → `Internal`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::events::{declare_event_class, EventClassConfig};
use crate::sdk_codegen::CommandError;

/// Inputs to `events/declare-class`: the class name plus the (flattened) config
/// fields (`broadcast`, `channel`, `schemaVersion`, `onUnknownSchema`,
/// `description`) — the same wire shape the legacy IPC handler accepted.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/events/DeclareEventClassParams.ts"
)]
pub struct DeclareEventClassParams {
    /// The event-class name (registry key), e.g. `presence:update`.
    pub name: String,
    /// The declaration body, flattened onto the params alongside `name`.
    #[serde(flatten)]
    pub config: EventClassConfig,
}

crate::action_command! {
    /// Declare an event class — its broadcast policy, airc channel strategy, and
    /// schema version — so the transport can route it. Idempotent for an identical
    /// re-declaration; a conflicting re-declaration fails loud to protect the wire
    /// contract. Substrate bootstrap, not a persona tool.
    pub struct DeclareEventClass;
    name: "events/declare-class",
    access: Internal,
    params: DeclareEventClassParams,
    output: crate::events::ResolvedEventClassConfig,
    run(_this, _ctx, p) => {
        declare_event_class(&p.name, &p.config)
            .map_err(|e| CommandError::Invalid(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: name/access wiring — declaring a class mutates the
    // wire-contract registry, so it stays Internal substrate plumbing, never on the
    // AiSafe persona surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(DeclareEventClass::NAME, "events/declare-class");
        assert!(matches!(
            DeclareEventClass::ACCESS,
            crate::sdk_codegen::AccessLevel::Internal
        ));
    }

    // what this catches: a declare → identical re-declare is idempotent (the same
    // resolved config both times), so the typed path preserves the legacy
    // idempotency contract instead of erroring on a benign repeat.
    #[tokio::test]
    async fn declare_is_idempotent() {
        let cmd = DeclareEventClass;
        let params = || DeclareEventClassParams {
            name: "typed-test:declare-idempotent".into(),
            config: EventClassConfig {
                broadcast: false,
                channel: None,
                schema_version: "v1".into(),
                on_unknown_schema: None,
                description: None,
            },
        };
        let first = cmd.run(&Ctx::default(), params()).await.unwrap();
        let second = cmd.run(&Ctx::default(), params()).await.unwrap();
        assert_eq!(first, second);
    }
}
