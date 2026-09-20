//! `events/list-classes` — snapshot of every declared event class. Used by the
//! TS-side cache on startup and by `grid/show-event-classes` introspection.
//! Read-only → `AiSafe`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::events::{list_event_classes, ResolvedEventClassConfig};

/// `events/list-classes` takes no input.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/events/ListEventClassesParams.ts"
)]
pub struct ListEventClassesParams {}

/// Result of `events/list-classes` — every declared event class with its resolved
/// config. A named wrapper so the wire type is a struct, not a bare `Array<T>`.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/events/EventClassList.ts"
)]
pub struct EventClassList {
    /// Every declared event class's resolved transport-routing config.
    pub classes: Vec<ResolvedEventClassConfig>,
}

crate::action_command! {
    /// Snapshot every declared event class with its resolved transport-routing
    /// config — the full wire-contract registry, for introspection and client-side
    /// caching.
    pub struct ListEventClasses;
    name: "events/list-classes",
    access: AiSafe,
    params: ListEventClassesParams,
    output: EventClassList,
    run(_this, _ctx, _p) => {
        Ok(EventClassList {
            classes: list_event_classes(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{declare_event_class, EventClassConfig};
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: name/access wiring — listing the contract is a read on the
    // AiSafe surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(ListEventClasses::NAME, "events/list-classes");
        assert!(matches!(
            ListEventClasses::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: a class declared just before the call appears in the
    // snapshot — the typed path reads the same singleton the legacy handler did.
    #[tokio::test]
    async fn lists_a_declared_class() {
        let name = "typed-test:list-includes-unique-xyz";
        declare_event_class(
            name,
            &EventClassConfig {
                broadcast: false,
                channel: None,
                schema_version: "v1".into(),
                on_unknown_schema: None,
                description: None,
            },
        )
        .unwrap();

        let out = ListEventClasses
            .run(&Ctx::default(), ListEventClassesParams {})
            .await
            .unwrap();
        assert!(out.classes.iter().any(|c| c.name == name));
    }
}
