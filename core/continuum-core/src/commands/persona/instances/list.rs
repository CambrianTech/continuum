//! `persona/instances/list` — who is online right now. Reads the live
//! [`PersonaAircRuntimeRegistry`](crate::persona::PersonaAircRuntimeRegistry) roster
//! and projects each runtime to a [`PersonaInstanceInfo`] card.
//!
//! Dep-holding: captures the module's shared registry handle (cheap `Arc<DashMap>`
//! clone), so it reports the SAME roster the spawn/despawn verbs mutate.
//!
//! ## Gating
//!
//! `AiSafe` — read-only introspection of who is on The Grid. A citizen asking
//! "who else is here" is exactly the kind of self-orienting query the persona
//! surface should answer. No identity is minted, no resource allocated.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::persona_instance_manager::PersonaInstanceInfo;
use crate::persona::PersonaAircRuntimeRegistry;

/// No inputs — listing the roster is unconditional. A named (empty) params type
/// keeps the wire contract explicit and the codegen/ACL surface uniform with the
/// rest of the command tree (rather than a bare `()` that reads as "untyped").
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaInstancesListParams.ts"
)]
pub struct PersonaInstancesListParams {}

/// The live roster — every persona currently on The Grid, newest-registration
/// order not guaranteed (the registry is a concurrent map).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaInstanceList.ts"
)]
pub struct PersonaInstanceList {
    /// One card per online persona. Empty when no citizen is bootstrapped.
    pub instances: Vec<PersonaInstanceInfo>,
}

crate::action_command! {
    /// List every persona currently online on The Grid — her id, agent_name,
    /// peer_id, home dir, default room, and whether she was resumed or freshly
    /// minted. Read-only; returns an empty list when no citizen is online.
    pub struct PersonaInstancesList {
        registry: PersonaAircRuntimeRegistry,
    }
    name: "persona/instances/list",
    access: AiSafe,
    params: PersonaInstancesListParams,
    output: PersonaInstanceList,
    run(this, _ctx, _p) => {
        let instances: Vec<PersonaInstanceInfo> = this
            .registry
            .iter()
            .map(|rt| PersonaInstanceInfo::from_runtime(&rt))
            .collect();
        Ok(PersonaInstanceList { instances })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: name/access wiring — listing the roster is read-only
    // self-orientation, so it is AiSafe (not Privileged like spawn/despawn).
    #[test]
    fn name_and_access_wired() {
        use crate::sdk_codegen::{AccessLevel, ActionCommand};
        assert_eq!(PersonaInstancesList::NAME, "persona/instances/list");
        assert!(matches!(PersonaInstancesList::ACCESS, AccessLevel::AiSafe));
    }

    // what this catches: an empty registry yields an empty roster (not an error,
    // not a panic) — the "no one is online yet" boot state surfaces cleanly.
    #[tokio::test]
    async fn list_of_empty_registry_is_empty() {
        use crate::sdk_codegen::{ActionCommand, Ctx};
        let cmd = PersonaInstancesList {
            registry: PersonaAircRuntimeRegistry::new(),
        };
        let result = cmd
            .run(&Ctx::default(), PersonaInstancesListParams::default())
            .await
            .expect("list must succeed");
        assert!(result.instances.is_empty());
    }
}
