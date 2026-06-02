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
use crate::persona::hw_tier_descriptor::HwTierCategory;
use crate::persona::role_template::RoleId;
use crate::runtime::service_module::{
    CommandResult, CommandSchema, ModuleConfig, ModulePriority, ServiceModule,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;

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
