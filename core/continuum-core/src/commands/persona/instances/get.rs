//! `persona/instances/get` — look up one online persona by her id. Reads the live
//! [`PersonaAircRuntimeRegistry`](crate::persona::PersonaAircRuntimeRegistry) and
//! projects the matching runtime to a [`PersonaInstanceInfo`] card.
//!
//! Dep-holding: captures the module's shared registry handle, so it resolves
//! against the SAME roster `list`/spawn/despawn act on.
//!
//! ## Fail loud
//!
//! Mal-formed id ⇒ [`CommandError::Invalid`] (a typo never silently misses). A
//! well-formed id that is not online ⇒ [`CommandError::NotFound`] (asking about an
//! offline persona is surfaced, not answered with a null).
//!
//! ## Gating
//!
//! `AiSafe` — read-only lookup of a single roster entry.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::persona_instance_manager::PersonaInstanceInfo;
use crate::persona::PersonaAircRuntimeRegistry;
use crate::sdk_codegen::CommandError;

/// Which online persona to fetch.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaInstancesGetParams.ts")]
pub struct PersonaInstancesGetParams {
    /// The persona's id as it appears in `persona/instances/list` (the airc
    /// peer_id Uuid). Fails loud if mal-formed or not currently online.
    pub persona_id: crate::identity::PersonaRef,
}

crate::action_command! {
    /// Fetch one online persona's card by her id — agent_name, peer_id, home dir,
    /// default room, and resumed-vs-minted source. Read-only. Fails loud on a
    /// mal-formed id (Invalid) or an id that is not currently online (NotFound).
    pub struct PersonaInstancesGet {
        registry: PersonaAircRuntimeRegistry,
    }
    name: "persona/instances/get",
    access: AiSafe,
    params: PersonaInstancesGetParams,
    output: PersonaInstanceInfo,
    run(this, _ctx, p) => {
        // Short-form persona ids resolve too (#164): the roster/personas surfaces
        // DISPLAY 8-char short ids, so accept the id a caller was shown. A clean
        // UUID passes straight through; a short/mistyped form expands against the
        // live registry — the ONE shared id_resolve primitive, candidates = who's
        // online.
        let persona_id = crate::id_resolve::resolve(
            p.persona_id.as_str(),
            &this.registry.ids(),
            "persona",
        )
        .map_err(|e| CommandError::Invalid(format!("{e} — call persona/instances/list")))?;
        let runtime = this.registry.get(persona_id).ok_or_else(|| {
            CommandError::NotFound(format!(
                "no persona with id {persona_id} is currently online — call persona/instances/list"
            ))
        })?;
        Ok(PersonaInstanceInfo::from_runtime(&runtime))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    // what this catches: name/access wiring — a single-entry roster lookup is
    // read-only, so it is AiSafe.
    #[test]
    fn name_and_access_wired() {
        use crate::sdk_codegen::{AccessLevel, ActionCommand};
        assert_eq!(PersonaInstancesGet::NAME, "persona/instances/get");
        assert!(matches!(PersonaInstancesGet::ACCESS, AccessLevel::AiSafe));
    }

    // what this catches: a well-formed id that is not in the roster fails loud as
    // NotFound (never a null/empty success that hides an offline persona).
    #[tokio::test]
    async fn get_of_offline_persona_is_not_found() {
        use crate::sdk_codegen::{ActionCommand, Ctx};
        let cmd = PersonaInstancesGet {
            registry: PersonaAircRuntimeRegistry::new(),
        };
        let err = cmd
            .run(
                &Ctx::default(),
                PersonaInstancesGetParams {
                    persona_id: Uuid::new_v4().to_string().into(),
                },
            )
            .await
            .expect_err("offline persona must fail loud");
        assert!(matches!(err, CommandError::NotFound(_)), "got {err:?}");
    }

    // what this catches: a mal-formed id is rejected as Invalid before any roster
    // lookup — a typo never silently does nothing.
    #[tokio::test]
    async fn get_of_malformed_id_is_invalid() {
        use crate::sdk_codegen::{ActionCommand, Ctx};
        let cmd = PersonaInstancesGet {
            registry: PersonaAircRuntimeRegistry::new(),
        };
        let err = cmd
            .run(
                &Ctx::default(),
                PersonaInstancesGetParams {
                    persona_id: "not-a-uuid".to_string().into(),
                },
            )
            .await
            .expect_err("malformed id must fail loud");
        assert!(matches!(err, CommandError::Invalid(_)), "got {err:?}");
    }
}
