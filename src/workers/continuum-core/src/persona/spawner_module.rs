//! [`PersonaSpawnerModule`] — substrate-level ServiceModule for
//! deciding which personas should be alive on this host.
//!
//! ## Doctrine
//!
//! Per #121: the substrate's "who lives here?" decision is a
//! background concern, not something user code drives by invoking a
//! command. The spawner exposes its plan as introspectable state
//! (`persona/spawner/plan` returns the resolved roster as JSON); the
//! actual airc bootstrap + chat loop spawn lands in slice 8 on top of
//! this planning surface.
//!
//! ## Slice 7 scope
//!
//! - `PersonaSpawnerModule` struct with `ServiceModule` impl
//! - One command: `persona/spawner/plan` — returns the desired roster
//!   for the configured hardware tier as JSON. Used by operators +
//!   tests + (eventually) the slice 8 bootstrap orchestrator to ask
//!   "what should be running here?" without firing async work.
//! - `plan_for_tier(hw_capability, tier_category)` pure function that
//!   produces a `Vec<DesiredRole>` — the LCD substrate's "Helper +
//!   Coder both on Qwen2.5-0.5B" default for Compat tier.
//!
//! ## Slice 8 scope (not in this commit)
//!
//! - `bootstrap_planned(spawn_plan, instance_manager)` — for each
//!   `DesiredRole`, calls
//!   `PersonaInstanceManagerModule::bootstrap_one` to get an airc
//!   identity, then `spawner::derive_spawn_plan` to materialize the
//!   inference profile, then `LlamaCppAdapter::for_persona` to
//!   construct the adapter.
//! - Per-persona subscribe-and-respond tokio task (the demo binary's
//!   main loop, factored as a reusable function).
//!
//! Splitting the planning from the async bootstrap chain keeps each
//! commit reviewable and testable without an airc fixture.

use crate::cognition::model_resolver::types::HwCapabilityTier;
use crate::modules::persona_instance_manager::{PersonaInstanceInfo, PersonaInstanceManagerModule};
use crate::persona::hw_tier_descriptor::HwTierCategory;
use crate::persona::identity_provider::PersonaIdentityIntent;
use crate::persona::inference_profile::{InferenceProfileError, PersonaInferenceProfile};
use crate::persona::role_template::RoleId;
use crate::persona::spawner::{derive_spawn_plan, RosterEntry};
use crate::runtime::service_module::{
    CommandResult, CommandSchema, ModuleConfig, ModulePriority, ServiceModule,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;

/// One row in the spawner's resolved plan: a desired persona slot for
/// the configured hardware tier, with its model already selected.
///
/// `DesiredRole` carries only the slow-changing facts — role + model
/// id. The fast-changing facts — peer_id, persona_name — come from
/// airc identity allocation at bootstrap time (slice 8).
///
/// Wire shape (`persona/spawner/plan` command result): camelCase
/// JSON. ts-rs export is deferred until `RoleId` itself derives `TS`
/// — landing that later is additive and doesn't change this struct's
/// JSON serialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesiredRole {
    /// Role identifier (helper / coder / sentinel / custom).
    pub role: RoleId,
    /// Model registry id picked by the substrate for this role at
    /// this tier. The slice 8 orchestrator resolves the model via the
    /// registry; this id is the substrate's *intent* — "Helper at
    /// Compat tier wants the LCD Qwen2.5-0.5B."
    pub model_id: String,
}

/// Compose the desired roster for a given hardware tier. Today this
/// is hardcoded LCD-first (Helper + Coder both on Qwen2.5-0.5B for
/// Compat). Future slices read from #123 ORM-stored role_templates.
///
/// `hw_capability` is the concrete tier id (e.g. `MacIntelMetalDiscrete`)
/// — used in case role_template's `model_per_tier` is consulted for a
/// model_id pick. `tier_category` is the 5-variant classifier from
/// slice 1 — used to gate the roster shape (Compat = LCD-only for
/// now; richer tiers add Sentinel + Researcher + etc. as they ship).
pub fn plan_for_tier(
    hw_capability: HwCapabilityTier,
    tier_category: HwTierCategory,
) -> Vec<DesiredRole> {
    // Slot the substrate's LCD as Helper + Coder on Compat. Per Joel
    // (#133): "no MacBooks left behind." Even the weakest hardware
    // gets multi-persona on day one.
    let _ = hw_capability; // currently informational; future per-tier
                           // role_template selection consumes it
    match tier_category {
        HwTierCategory::Compat => vec![
            DesiredRole {
                role: RoleId::Helper,
                model_id: "continuum-ai/qwen2.5-0.5b-instruct-GGUF".to_string(),
            },
            DesiredRole {
                role: RoleId::Coder,
                model_id: "continuum-ai/qwen2.5-0.5b-instruct-GGUF".to_string(),
            },
        ],
        // Other tiers: Helper + Coder for now, same model selection
        // pending tier-specific role_template wiring (#123). Slice 8+
        // will refine — MSeriesPro can fit Qwen2.5-7B; Cuda Sm120 can
        // fit Qwen2.5-14B + Sentinel + Researcher.
        HwTierCategory::MSeries
        | HwTierCategory::MSeriesPro
        | HwTierCategory::Cuda
        | HwTierCategory::Cloud => vec![
            DesiredRole {
                role: RoleId::Helper,
                model_id: "continuum-ai/qwen2.5-0.5b-instruct-GGUF".to_string(),
            },
            DesiredRole {
                role: RoleId::Coder,
                model_id: "continuum-ai/qwen2.5-0.5b-instruct-GGUF".to_string(),
            },
        ],
    }
}

/// Substrate ServiceModule that surfaces the spawner's roster plan.
/// Configurable at construction time with the detected tier; today
/// the config is static (set when the module is built at substrate
/// boot), future slices add a `tick()` that picks up tier changes
/// (laptop docked → external GPU available, etc.) and re-plans.
pub struct PersonaSpawnerModule {
    hw_capability: HwCapabilityTier,
    tier_category: HwTierCategory,
}

impl PersonaSpawnerModule {
    /// The detected hardware-tier classifier the module is configured
    /// against. Slice 8's `bootstrap_planned` reads this to forward
    /// the same tier_category into `derive_spawn_plan`.
    pub fn tier_category(&self) -> HwTierCategory {
        self.tier_category
    }

    /// The concrete hardware-tier id the module is configured against.
    /// Exposed for symmetry with `tier_category()` — substrate boot
    /// reads this when telemetry needs the precise host classification.
    pub fn hw_capability(&self) -> HwCapabilityTier {
        self.hw_capability
    }
}

impl PersonaSpawnerModule {
    /// Construct with the detected hardware tier. The slice 8
    /// substrate boot wiring calls `HostCapabilityProbe` to resolve
    /// the tier, then hands it here.
    pub fn new(hw_capability: HwCapabilityTier, tier_category: HwTierCategory) -> Self {
        Self {
            hw_capability,
            tier_category,
        }
    }

    /// Currently-planned desired roster. Pure function over the
    /// module's configured tier; doesn't touch async, doesn't hold a
    /// lock — safe to call from anywhere.
    pub fn plan(&self) -> Vec<DesiredRole> {
        plan_for_tier(self.hw_capability, self.tier_category)
    }
}

#[async_trait]
impl ServiceModule for PersonaSpawnerModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "persona_spawner",
            priority: ModulePriority::Normal,
            command_prefixes: &["persona/spawner/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        _params: Value,
    ) -> Result<CommandResult, String> {
        match command {
            "persona/spawner/plan" => {
                let plan = self.plan();
                CommandResult::json(&plan)
            }
            other => Err(format!(
                "persona_spawner: unknown command '{other}' — try 'persona/spawner/plan'"
            )),
        }
    }

    fn command_schemas(&self) -> Vec<CommandSchema> {
        vec![CommandSchema {
            name: "persona/spawner/plan",
            description:
                "Return the substrate's desired persona roster for the configured hardware tier",
            params: vec![],
        }]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ─────────────────────────────────────────────────────────────────────
// Slice 8 — `bootstrap_planned` async composition
// ─────────────────────────────────────────────────────────────────────
//
// Glue between slice 6 (`derive_spawn_plan`) + #87
// (`PersonaInstanceManagerModule::bootstrap_one`): for each
// `DesiredRole` in the planner's roster, pull an identity intent,
// bootstrap the airc identity, then materialize the inference profile
// against the airc-allocated (persona_id, agent_name).
//
// What this DOESN'T do (intentional, lands in slice 9):
//
// - Construct the `LlamaCppAdapter`. The adapter holds a loaded GGUF
//   (~500 MiB of weights) and is a hot-path resource; the substrate
//   supervisor that owns the adapter lifetimes (paging, eviction,
//   shared base across personas per #122) is the right owner. This
//   layer stops at the profile so it stays testable without llama.cpp
//   in the loop.
// - Run the per-persona subscribe-loop. The chat-attach + service-loop
//   is the demo binary's main today (`airc_chat_demo`); slice 9
//   factors that out so it's reusable from production boot.

/// Errors `bootstrap_planned` can surface, kept structured so callers
/// (e.g. a future supervisor module) can decide which failures are
/// fatal vs which to log-and-continue. Per [[no-fallbacks-ever]] the
/// substrate never substitutes a "default" persona when one fails.
#[derive(Debug, thiserror::Error)]
pub enum BootstrapPlannedError {
    /// The identity provider didn't have enough intents to satisfy
    /// the planner's roster. The slot is named so operators see which
    /// role couldn't get an identity.
    #[error("identity provider exhausted at slot {slot_index} (role {role:?}) — provider yielded {provided} intents, planner requires {required}")]
    IdentityProviderExhausted {
        slot_index: usize,
        role: RoleId,
        provided: usize,
        required: usize,
    },
    /// The identity provider's own error path — disk read failures,
    /// seed parse errors, etc. The substrate boot needs to see the
    /// full provider error chain to act on it.
    #[error("identity provider failed at slot {slot_index} (role {role:?}): {source}")]
    IdentityProvider {
        slot_index: usize,
        role: RoleId,
        #[source]
        source: crate::persona::identity_provider::PersonaIdentityError,
    },
    /// airc bootstrap failed for this persona — usually a daemon-
    /// unreachable / home-dir-permission / Ed25519 mint failure. Per
    /// [[no-stdio-piping-for-process-ipc]] this is a structured
    /// runtime error from airc-lib, not stderr-scraping.
    #[error("airc bootstrap failed at slot {slot_index} (role {role:?}): {source}")]
    AircBootstrap {
        slot_index: usize,
        role: RoleId,
        #[source]
        source: crate::persona::airc_runtime::PersonaAircRuntimeError,
    },
}

/// One row of slice 8's output: the substrate-resolved fact of "this
/// persona is alive on airc AND has its inference profile resolved".
/// Slice 9 takes a `Vec<MaterializedPersonaPlan>` and constructs the
/// per-persona inference + chat-loop runtime.
#[derive(Debug, Clone)]
pub struct MaterializedPersonaPlan {
    /// Role this slot fills (Helper / Coder / ...).
    pub role: RoleId,
    /// airc identity allocation result — peer_id, agent_name,
    /// home dir, default room, source (resumed vs minted).
    pub instance: PersonaInstanceInfo,
    /// Per-row profile or per-row error. Per the slice-6 contract:
    /// one bad row (e.g., a model id not yet in the registry) doesn't
    /// block the others. The supervisor decides whether to refuse
    /// boot or skip the bad personas with a diagnostic.
    pub profile: Result<PersonaInferenceProfile, InferenceProfileError>,
}

/// Compose a full bootstrap-and-plan for the configured roster.
///
/// For each `DesiredRole` in `module.plan()`:
///   1. Pull the next `PersonaIdentityIntent` from `provider`.
///   2. Call `instance_manager.bootstrap_one(&intent)` → airc identity
///      ceremony, seed.json write, registry register.
///   3. Build a `RosterEntry` from the airc-allocated
///      `(persona_id, agent_name)` + the planner's `model_id`.
///   4. Append the materialized row.
///
/// Once all roster entries are bootstrapped, the function calls
/// `derive_spawn_plan` ONCE to materialize all profiles in a single
/// pass against the same model registry.
///
/// Failures at the identity-provider or airc-bootstrap layers are
/// fatal — those affect every later slot, so the function early-
/// returns. Per-row profile errors stay per-row so the supervisor
/// keeps its policy choice.
pub async fn bootstrap_planned(
    module: &PersonaSpawnerModule,
    instance_manager: &PersonaInstanceManagerModule,
    provider: &mut dyn crate::persona::identity_provider::PersonaIdentityProvider,
    tier_id: &str,
    registry: &Arc<crate::model_registry::Registry>,
) -> Result<Vec<MaterializedPersonaPlan>, BootstrapPlannedError> {
    let plan = module.plan();
    let required = plan.len();
    let mut bootstrapped: Vec<(RoleId, PersonaInstanceInfo, String)> = Vec::with_capacity(required);

    for (slot_index, desired) in plan.iter().enumerate() {
        let intent: PersonaIdentityIntent = provider
            .next_persona()
            .await
            .map_err(|source| BootstrapPlannedError::IdentityProvider {
                slot_index,
                role: desired.role,
                source,
            })?
            .ok_or(BootstrapPlannedError::IdentityProviderExhausted {
                slot_index,
                role: desired.role,
                provided: slot_index,
                required,
            })?;

        let info = instance_manager
            .bootstrap_one(&intent)
            .await
            .map_err(|source| BootstrapPlannedError::AircBootstrap {
                slot_index,
                role: desired.role,
                source,
            })?;

        bootstrapped.push((desired.role, info, desired.model_id.clone()));
    }

    let roster: Vec<RosterEntry> = bootstrapped
        .iter()
        .map(|(role, info, model_id)| RosterEntry {
            role: *role,
            persona_id: info.persona_id,
            persona_name: info.agent_name.clone(),
            model_id: model_id.clone(),
        })
        .collect();

    let profiles = derive_spawn_plan(&roster, tier_id, module.tier_category(), registry);

    Ok(bootstrapped
        .into_iter()
        .zip(profiles)
        .map(|((role, instance, _model_id), profile)| MaterializedPersonaPlan {
            role,
            instance,
            profile,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compat tier produces the LCD roster: Helper + Coder both on
    /// Qwen2.5-0.5B. The canonical Intel-Mac startup state #133
    /// targets.
    #[test]
    fn compat_tier_plans_helper_and_coder_on_lcd() {
        let plan = plan_for_tier(
            HwCapabilityTier::MacIntelMetalDiscrete,
            HwTierCategory::Compat,
        );
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].role, RoleId::Helper);
        assert_eq!(plan[1].role, RoleId::Coder);
        assert_eq!(
            plan[0].model_id,
            "continuum-ai/qwen2.5-0.5b-instruct-GGUF"
        );
        assert_eq!(
            plan[1].model_id,
            "continuum-ai/qwen2.5-0.5b-instruct-GGUF"
        );
    }

    /// Every tier currently plans Helper + Coder — verifies the
    /// "no MacBooks (or anyone) left behind" floor that Joel set on
    /// 2026-06-01. Slice 8+ refines each tier's roster.
    #[test]
    fn every_tier_plans_at_least_helper_and_coder() {
        for (hw, cat) in [
            (HwCapabilityTier::CpuOnly, HwTierCategory::Compat),
            (HwCapabilityTier::M1Uma8Gb, HwTierCategory::MSeries),
            (HwCapabilityTier::M5UmaProMax, HwTierCategory::MSeriesPro),
            (HwCapabilityTier::Sm120, HwTierCategory::Cuda),
            (HwCapabilityTier::Cloud, HwTierCategory::Cloud),
        ] {
            let plan = plan_for_tier(hw, cat);
            assert!(plan.len() >= 2, "tier {cat:?} planned only {} roles", plan.len());
            assert!(
                plan.iter().any(|r| r.role == RoleId::Helper),
                "tier {cat:?} missing Helper"
            );
            assert!(
                plan.iter().any(|r| r.role == RoleId::Coder),
                "tier {cat:?} missing Coder"
            );
        }
    }

    /// ServiceModule.plan() is the same as the free function with the
    /// module's configured tier — proves the substrate-managed and
    /// pure-function paths agree.
    #[test]
    fn module_plan_matches_free_function() {
        let module = PersonaSpawnerModule::new(
            HwCapabilityTier::MacIntelMetalDiscrete,
            HwTierCategory::Compat,
        );
        assert_eq!(
            module.plan(),
            plan_for_tier(
                HwCapabilityTier::MacIntelMetalDiscrete,
                HwTierCategory::Compat,
            )
        );
    }

    /// Provider exhaustion is a clean structured error. Tests slice
    /// 8's wiring without needing airc — the provider returns None
    /// before any `bootstrap_one` would fire, so the function
    /// short-circuits with a named error.
    #[tokio::test]
    async fn bootstrap_planned_exhausted_provider_errors_with_slot_info() {
        use crate::persona::identity_provider::{
            PersonaIdentityError, PersonaIdentityIntent, PersonaIdentityProvider,
        };
        use async_trait::async_trait;
        use std::path::PathBuf;

        // Provider that returns None immediately — simulates "we
        // configured a roster of 2 but only have 0 saved identities
        // and refuse to mint" (or any other exhaustion).
        struct EmptyProvider;
        #[async_trait]
        impl PersonaIdentityProvider for EmptyProvider {
            fn name(&self) -> &'static str {
                "empty"
            }
            async fn next_persona(
                &mut self,
            ) -> Result<Option<PersonaIdentityIntent>, PersonaIdentityError> {
                Ok(None)
            }
        }

        let module = PersonaSpawnerModule::new(
            HwCapabilityTier::MacIntelMetalDiscrete,
            HwTierCategory::Compat,
        );

        // The bootstrapper is never reached because provider exhausts
        // first — its construction can be cheap-and-unreachable.
        // continuum_root/daemon_socket/default_room never get touched.
        let instance_manager = PersonaInstanceManagerModule::new(
            crate::persona::PersonaAircRuntimeRegistry::default(),
            PathBuf::from("/dev/null/unused"),
            airc_core::RoomId::from_uuid(uuid::Uuid::nil()),
            PathBuf::from("/dev/null/unused"),
        );

        // Registry contents don't matter — derive_spawn_plan is never
        // reached when the provider exhausts at slot 0.
        let registry = std::sync::Arc::new(
            crate::model_registry::Registry::from_catalog(vec![], vec![]).expect("empty registry"),
        );

        let mut provider = EmptyProvider;
        let err = bootstrap_planned(
            &module,
            &instance_manager,
            &mut provider,
            "mac_intel_metal_discrete",
            &registry,
        )
        .await
        .expect_err("must error when provider exhausts");
        match err {
            BootstrapPlannedError::IdentityProviderExhausted {
                slot_index,
                role,
                provided,
                required,
            } => {
                assert_eq!(slot_index, 0);
                assert_eq!(role, RoleId::Helper);
                assert_eq!(provided, 0);
                assert_eq!(required, 2);
            }
            other => panic!("expected IdentityProviderExhausted, got {other:?}"),
        }
    }

    /// Roundtrip the DesiredRole through serde — verifies the
    /// camelCase wire shape and that ts-rs export will produce a clean
    /// TS type.
    #[test]
    fn desired_role_serde_camel_case() {
        let role = DesiredRole {
            role: RoleId::Helper,
            model_id: "continuum-ai/qwen2.5-0.5b-instruct-GGUF".to_string(),
        };
        let json = serde_json::to_string(&role).expect("serialize");
        // RoleId already serializes as snake_case ("helper"); model_id
        // becomes modelId per the camelCase rename_all on this struct.
        assert!(json.contains("\"role\":\"helper\""));
        assert!(json.contains("\"modelId\":\"continuum-ai/qwen2.5-0.5b-instruct-GGUF\""));
        let back: DesiredRole = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, role);
    }
}
