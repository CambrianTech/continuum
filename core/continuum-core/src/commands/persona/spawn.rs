//! `persona/spawn` — bring a new persona (an airc citizen) into being, on demand.
//!
//! ## The concern this owns
//!
//! "Make me a new persona" — issued by a human, another persona, a widget, or the
//! grid capacity-scaler. Birth is a first-class COMMAND any citizen can call, not a
//! private boot subroutine ([[persona-birth-is-a-first-class-handle-command]]).
//!
//! ## One birth path
//!
//! It does NOT re-implement spawning. It calls the SAME [`PersonaBirth::birth_one`]
//! the boot auto-seed supervisor calls — one birth core, never a parallel
//! implementation. Everything a persona needs is derived from her id (name, gender,
//! avatar, voice — the #199 `PersonaCard` genesis); the only inputs here are optional
//! overrides.
//!
//! ## Handle-based / non-blocking
//!
//! Birth is slow (the airc keypair ceremony, seed + card persist, avatar pin), so this
//! command does NOT block on it. It mints each identity (fast — a uuid + a pooled
//! name), returns the names IMMEDIATELY as a receipt, and completes the births in a
//! detached task. Each completed birth announces itself as `persona:born` (payload
//! [`PersonaInstanceInfo`](crate::modules::persona_instance_manager::PersonaInstanceInfo))
//! — the event-driven half of the handle shape. Subscribe to `persona:born`, or poll
//! `persona/instances/list`, to see them arrive.
//!
//! ## Gating
//!
//! `Privileged` — birth creates a durable citizen identity and consumes host/grid
//! capacity. Opening it to trusted personas (so a persona can spawn a teammate) is a
//! consent-gate decision (#136, [[consent-gates-on-actions-never-caps-on-cognition]]),
//! layered on top of this declared level — not a relaxation of it here.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::modules::persona_instance_manager::PersonaBirth;
use crate::persona::agent_name_from_identity;
use crate::persona::identity_provider::{PersonaIdentityIntent, PersonaIdentitySource};

/// Optional inputs to a spawn. All optional — the zero-arg call births ONE persona
/// with a random name and everything else derived from her id.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaSpawnParams.ts"
)]
pub struct PersonaSpawnParams {
    /// Explicit name for the (single) persona. Omit for a random name from the pool.
    /// Ignored when `count > 1` — a batch is all-random (one name can't name many).
    #[serde(default)]
    pub name: Option<String>,
    /// How many to spawn at once (grid capacity). Defaults to 1; clamped to
    /// `[1, MAX_SPAWN]` so a runaway value can't birth an unbounded population.
    #[serde(default)]
    pub count: Option<u32>,
}

/// The immediate receipt: who is being born. Births run in the background — each
/// completion fires `persona:born`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaSpawnReceipt.ts"
)]
pub struct PersonaSpawnReceipt {
    /// The names being birthed, in order.
    pub birthing: Vec<String>,
    /// How many births were started.
    pub count: u32,
    /// Human-readable summary.
    pub detail: String,
}

/// Max personas one call may spawn — a backstop against a runaway `count`.
pub const MAX_SPAWN: u32 = 32;

/// Plan the fresh-mint intents for a spawn: `count` (defaulted + clamped) identities,
/// each a fresh uuid + a name. A single spawn honors an explicit `name`; a batch is
/// all-random. Pure + fast (no I/O) so the command can return the names before the
/// slow births run — and so the count/name policy is unit-testable without deps.
fn plan_intents(name: Option<String>, count: Option<u32>) -> Vec<PersonaIdentityIntent> {
    let count = count.unwrap_or(1).clamp(1, MAX_SPAWN);
    (0..count)
        .map(|_| {
            let persona_id = Uuid::new_v4();
            let agent_name = if count == 1 {
                name.clone().unwrap_or_else(|| {
                    agent_name_from_identity(&persona_id.to_string()).to_string()
                })
            } else {
                agent_name_from_identity(&persona_id.to_string()).to_string()
            };
            PersonaIdentityIntent {
                persona_id,
                agent_name,
                source: PersonaIdentitySource::FreshlyMinted,
            }
        })
        .collect()
}

crate::action_command! {
    /// Bring one or more new personas (airc citizens) into being. Non-blocking: mints
    /// each identity, returns the names immediately, and completes the slow airc
    /// keypair ceremony + seed/card/avatar in the background — each birth then fires
    /// `persona:born`. The zero-arg call births ONE persona with a random name and
    /// everything derived from her id. Reuses the SAME birth core as boot auto-seed.
    pub struct PersonaSpawn {
        birth: Arc<PersonaBirth>,
    }
    name: "persona/spawn",
    access: Privileged,
    params: PersonaSpawnParams,
    output: PersonaSpawnReceipt,
    run(this, _ctx, p) => {
        let intents = plan_intents(p.name, p.count);
        let count = intents.len() as u32;
        let birthing: Vec<String> = intents.iter().map(|i| i.agent_name.clone()).collect();

        // Detach the slow births — return NOW with the names. Each birth announces
        // itself via `persona:born` on completion (birth_one emits it), so this is
        // event-driven, not a blocking wait.
        let birth = this.birth.clone();
        tokio::spawn(async move {
            for intent in &intents {
                match birth.birth_one(intent).await {
                    Ok(info) => tracing::info!(
                        agent_name = %info.agent_name,
                        peer_id = %info.peer_id,
                        "persona/spawn: birthed"
                    ),
                    Err(e) => tracing::error!(
                        agent_name = %intent.agent_name,
                        error = %e,
                        "persona/spawn: birth failed"
                    ),
                }
            }
        });

        let detail = if count == 1 {
            format!("birthing '{}' — watch persona:born for her peer_id", birthing[0])
        } else {
            format!("birthing {count} personas ({}) — watch persona:born", birthing.join(", "))
        };
        Ok(PersonaSpawnReceipt { birthing, count, detail })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: the wire name mirrors the file path and the verb is gated
    // Privileged — the routing + access contract for the typed registry.
    #[test]
    fn name_mirrors_path_and_is_privileged() {
        assert_eq!(PersonaSpawn::NAME, "persona/spawn");
        assert!(matches!(
            PersonaSpawn::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }

    // what this catches: the default is exactly ONE birth (the zero-arg "make me a
    // persona" call), and every minted intent is FreshlyMinted with a non-empty name.
    #[test]
    fn zero_arg_plans_one_fresh_mint() {
        let intents = plan_intents(None, None);
        assert_eq!(intents.len(), 1);
        assert_eq!(intents[0].source, PersonaIdentitySource::FreshlyMinted);
        assert!(!intents[0].agent_name.is_empty());
    }

    // what this catches: an explicit name is honored for a SINGLE spawn (a human/
    // persona can name the being they create).
    #[test]
    fn explicit_name_honored_for_single_spawn() {
        let intents = plan_intents(Some("Xena".to_string()), Some(1));
        assert_eq!(intents.len(), 1);
        assert_eq!(intents[0].agent_name, "Xena");
    }

    // what this catches: `count` is clamped to [1, MAX_SPAWN] — 0 becomes 1 (never a
    // no-op birth), and a runaway value is capped (never an unbounded population).
    #[test]
    fn count_is_clamped() {
        assert_eq!(plan_intents(None, Some(0)).len(), 1, "0 → 1, never a no-op");
        assert_eq!(plan_intents(None, Some(5)).len(), 5);
        assert_eq!(
            plan_intents(None, Some(9_999)).len() as u32,
            MAX_SPAWN,
            "runaway count is capped at MAX_SPAWN"
        );
    }

    // what this catches: a BATCH ignores the single-name override (one name can't name
    // many) — each of the batch gets its own derived name, and identities are unique.
    #[test]
    fn batch_is_all_random_with_unique_ids() {
        let intents = plan_intents(Some("Xena".to_string()), Some(4));
        assert_eq!(intents.len(), 4);
        // The explicit name is NOT stamped on the whole batch.
        assert!(
            intents.iter().filter(|i| i.agent_name == "Xena").count() < 4,
            "a batch must not stamp the single-name override on every member"
        );
        // Fresh uuids → distinct identities.
        let ids: std::collections::HashSet<_> = intents.iter().map(|i| i.persona_id).collect();
        assert_eq!(ids.len(), 4, "each batch member has a unique id");
    }
}
