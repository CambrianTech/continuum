//! `events/get-class` — look up a single event class's resolved config. Returns
//! `null` when the class was never declared (caller falls back to the default
//! backward-compat behavior). Read-only introspection → `AiSafe`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::events::{lookup_event_class, ResolvedEventClassConfig};

/// Inputs to `events/get-class`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/events/GetEventClassParams.ts"
)]
pub struct GetEventClassParams {
    /// The event-class name to look up.
    pub name: String,
}

/// Result of `events/get-class` — the resolved config when the class was declared,
/// absent otherwise (preserving the legacy "no class → use default behavior"
/// contract). A named wrapper so the wire type is a struct, not a bare `T | null`.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/events/EventClassLookup.ts"
)]
pub struct EventClassLookup {
    /// The resolved config, or absent when the class was never declared.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub config: Option<ResolvedEventClassConfig>,
}

crate::action_command! {
    /// Look up the resolved config for a declared event class — its broadcast
    /// policy, channel strategy, and schema version. The `config` field is absent
    /// when the class was never declared.
    pub struct GetEventClass;
    name: "events/get-class",
    access: AiSafe,
    params: GetEventClassParams,
    output: EventClassLookup,
    run(_this, _ctx, p) => {
        Ok(EventClassLookup {
            config: lookup_event_class(&p.name),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: name/access wiring — a read-only lookup is on the AiSafe
    // surface so personas can introspect the event-class contract.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(GetEventClass::NAME, "events/get-class");
        assert!(matches!(
            GetEventClass::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: an undeclared class yields None (→ JSON null on the wire),
    // preserving the legacy "no class declared, use default behavior" contract
    // instead of erroring.
    #[tokio::test]
    async fn undeclared_class_is_none() {
        let out = GetEventClass
            .run(
                &Ctx::default(),
                GetEventClassParams {
                    name: "typed-test:never-declared".into(),
                },
            )
            .await
            .unwrap();
        assert!(out.config.is_none());
    }
}
