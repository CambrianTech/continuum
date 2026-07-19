//! `persona/instances/despawn` — take a live persona off The Grid at runtime. The
//! deallocation counterpart of [`persona/instances/bootstrap`](crate::modules::persona_instance_manager):
//! anything that can be spawned can be despawned, no reboot.
//!
//! ## What it does
//!
//! Calls [`PersonaAircRuntimeRegistry::shutdown_slot`], the orderly teardown the
//! registry already owns: it takes the persona's service-loop `JoinHandle`,
//! `.abort()`s it and awaits the drain, then removes the slot. Dropping the slot
//! drops the last `Arc<PersonaAircRuntime>`, which cascades to the `Arc<Airc>` drop
//! → the wire-subscriber tasks abort → the persona leaves the room. The seat, the
//! tokio task, and the airc subscription are all reclaimed — the symmetric inverse
//! of bootstrap, which allocated them.
//!
//! ## What it does NOT do
//!
//! It does not delete the persona's home dir, seed.json, or engram store. Despawn
//! is the *runtime* dealloc (she goes offline); her durable self on disk is
//! untouched, so the next bootstrap resumes her as herself
//! ([[persona-persistence-self-determination]]). Erasing the durable identity would
//! be a different, destructive verb — not this one.
//!
//! ## Fail loud
//!
//! Unknown / un-parseable persona_id ⇒ [`CommandError::Invalid`]. A well-formed id
//! that is not in the live roster ⇒ [`CommandError::NotFound`] (despawning someone
//! who is already offline is a caller mistake worth surfacing, not a silent no-op).
//!
//! ## Gating
//!
//! `Privileged` — it removes a citizen from the running substrate and frees her
//! resources.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::persona::PersonaAircRuntimeRegistry;
use crate::sdk_codegen::CommandError;

#[cfg(test)]
use crate::sdk_codegen::Ctx;

/// Which live persona to take offline.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaDespawnParams.ts")]
pub struct PersonaDespawnParams {
    /// The persona's id as it appears in `persona/instances/list` (the airc
    /// peer_id Uuid). Fails loud if mal-formed or not currently online.
    pub persona_id: String,
}

/// What `persona/instances/despawn` did: who left, and the roster size after.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/persona/DespawnReport.ts")]
pub struct DespawnReport {
    /// The agent_name of the persona that was taken offline — echoed back so the
    /// caller can confirm they despawned who they meant to.
    pub agent_name: String,
    /// How many personas remain online after the despawn.
    pub roster_size: usize,
    /// Human-readable summary.
    pub detail: String,
}

crate::action_command! {
    /// Take a live persona off The Grid at runtime — no reboot. Orderly teardown:
    /// aborts her service loop, drains it, drops her airc subscription so she
    /// leaves the room, and frees the slot. The inverse of persona/instances/
    /// bootstrap. Her on-disk self (home, seed, engrams) is untouched — the next
    /// bootstrap resumes her. Fails loud on an unknown persona id.
    pub struct PersonaDespawn {
        registry: PersonaAircRuntimeRegistry,
    }
    name: "persona/instances/despawn",
    access: Privileged,
    params: PersonaDespawnParams,
    output: DespawnReport,
    run(this, _ctx, p) => {
        // 1. Resolve the id — a clean UUID passes through; the 8-char SHORT form a
        //    caller was shown (rosters DISPLAY short ids) or a one-char-mistyped
        //    UUID expands against who's online. The ONE shared id_resolve primitive
        //    (#164), candidates = the live runtime registry — same as instances/get.
        let persona_id = crate::id_resolve::resolve(
            &p.persona_id,
            &this.registry.ids(),
            "persona",
        )
        .map_err(|e| CommandError::Invalid(format!("{e} — call persona/instances/list")))?;

        // 2. Orderly shutdown: abort + drain the service loop, drop the slot
        //    (cascades to leaving the room). Returns None if she was not online.
        let runtime = this.registry.shutdown_slot(persona_id).await.ok_or_else(|| {
            CommandError::NotFound(format!(
                "no persona with id {persona_id} is currently online — call persona/instances/list"
            ))
        })?;

        let agent_name = runtime.agent_name().to_string();
        let roster_size = this.registry.len();
        Ok(DespawnReport {
            detail: format!(
                "{agent_name} left The Grid; {roster_size} persona(s) still online. Her on-disk \
                 self is preserved — bootstrap resumes her."
            ),
            agent_name,
            roster_size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    // what this catches: the wire name mirrors the file path — the routing
    // contract that lets the typed registry dispatch `persona/instances/despawn`
    // to this command (the deallocation half of the spawn↔despawn symmetry).
    #[test]
    fn name_mirrors_path() {
        use crate::sdk_codegen::ActionCommand;
        assert_eq!(PersonaDespawn::NAME, "persona/instances/despawn");
    }

    // what this catches: despawning a well-formed id that is not in the roster
    // fails loud as NotFound (never a silent no-op that hides an offline persona).
    #[tokio::test]
    async fn despawn_of_offline_persona_is_not_found() {
        use crate::sdk_codegen::ActionCommand;
        let registry = PersonaAircRuntimeRegistry::new();
        let cmd = PersonaDespawn { registry };
        let params = PersonaDespawnParams {
            persona_id: Uuid::new_v4().to_string(),
        };
        let err = cmd
            .run(&Ctx::default(), params)
            .await
            .expect_err("offline persona must fail loud");
        assert!(matches!(err, CommandError::NotFound(_)), "got {err:?}");
    }

    // what this catches: a mal-formed id is rejected as Invalid before any roster
    // lookup — a typo never silently does nothing.
    #[tokio::test]
    async fn despawn_of_malformed_id_is_invalid() {
        use crate::sdk_codegen::ActionCommand;
        let registry = PersonaAircRuntimeRegistry::new();
        let cmd = PersonaDespawn { registry };
        let params = PersonaDespawnParams {
            persona_id: "not-a-uuid".to_string(),
        };
        let err = cmd
            .run(&Ctx::default(), params)
            .await
            .expect_err("malformed id must fail loud");
        assert!(matches!(err, CommandError::Invalid(_)), "got {err:?}");
    }
}
