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
use crate::persona::profile_builder::ServingParams;
use crate::persona::role_template::RoleId;
use crate::persona::spawner::{derive_spawn_plan, RosterEntry};
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
    /// Continuous-batching lanes (`n_seq_max`) for this role's backend, from
    /// the serving daemon's ServingPlan. Defaults to 1 from `plan_for_tier`;
    /// `PersonaSpawnerModule::with_serving` overrides it from the live plan.
    pub lanes: u32,
    /// Host-fit served context window (tokens) the gateway llama-server was
    /// launched with — the planner's single source of truth, NOT a constant.
    /// Defaults to `MIN_SERVE_CTX` from `plan_for_tier` (runnable floor when no
    /// plan is published); `with_serving` overrides it from the live plan.
    pub served_context_window: u32,
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
    // Slice 13 ships SINGLE-PERSONA-per-plan. The Coder slot is
    // deferred to slice 14 because ResumeOrMintProvider's
    // scan_personas_dir sorts alphabetically (line 200 of
    // resume_or_mint_provider.rs), so on boot 2 the
    // position-pairing of [Helper, Coder] against the disk-yielded
    // alphabetic persona order flips role assignments — Bart
    // (random-derived from peer_id) becomes Helper when he was
    // bootstrapped as Coder, etc. PR #1510 review caught this; the
    // load-bearing fix is role-in-seed.json (slice 14).
    //
    // Re-enable [Helper, Coder] (and richer rosters for higher
    // tiers) when slice 14 lands. Until then the substrate hosts ONE
    // Helper persona per tier — the demo-binary level of coverage,
    // but through the substrate-managed path.
    let _ = hw_capability; // currently informational; future per-tier
                           // role_template selection consumes it
    let _ = tier_category; // all tiers produce the same single-Helper
                           // plan until slice 14 ships role-aware
                           // mapping + multi-role-per-tier rosters
    let plan = vec![DesiredRole {
        role: RoleId::Helper,
        // Fallback model when no ServingPlan is published yet (safe LCD). The
        // serving daemon's plan overrides this via with_serving — that's the
        // real, honest, GPU-residency-aware pick.
        model_id: "continuum-ai/qwen2.5-0.5b-instruct-GGUF".to_string(),
        // Default single lane; with_serving overrides from the live plan.
        lanes: 1,
        // Runnable floor until the serving daemon publishes a host-fit window.
        served_context_window: crate::cognition::serving_plan::MIN_SERVE_CTX,
    }];
    // P2 invariant tripwire (PR #1511 review advisory): if a future
    // refactor adds a second role here without atomically updating
    // ResumeOrMintProvider's role mapping, position-pairing will
    // silently mis-pair roles on boot 2 (alphabetic disk order vs
    // plan order). The debug_assert catches the regression at the
    // producer in debug + test builds. Slice 14 removes this when
    // role-in-seed.json makes multi-role plans safe.
    debug_assert!(
        plan.len() <= 1,
        "plan_for_tier returned {} roles before slice 14's role-in-seed.json \
         landed — position-pairing against ResumeOrMintProvider's alphabetic \
         disk order would flip role identity on boot 2. See #133 slice 14.",
        plan.len()
    );
    plan
    // TODO #133 slice 14: restore tier-shaped rosters after
    // RoleAwareProvider + role-in-seed.json land. Remove the
    // debug_assert above as part of that change.
}

/// Substrate ServiceModule that surfaces the spawner's roster plan.
/// Configurable at construction time with the detected tier; today
/// the config is static (set when the module is built at substrate
/// boot), future slices add a `tick()` that picks up tier changes
/// (laptop docked → external GPU available, etc.) and re-plans.
pub struct PersonaSpawnerModule {
    hw_capability: HwCapabilityTier,
    tier_category: HwTierCategory,
    /// Serving daemon's decision, applied as overrides on the tier roster:
    /// the base model every desired role runs (`None` → fall back to the
    /// tier's `plan_for_tier` pick) and the continuous-batching lane count.
    /// This is how the daemon's honest per-host ServingPlan drives what
    /// actually spawns — single source of truth, not a hardcode.
    serving_base_model: Option<String>,
    serving_lanes: u32,
    /// Host-fit served context window from the live ServingPlan. Defaults to
    /// `MIN_SERVE_CTX` (runnable floor); `with_serving` sets the real value.
    serving_context_window: u32,
    /// How many citizens of the tier's (homogeneous) role template to host.
    /// Default 1. Driven by `CONTINUUM_PERSONA_FLOOR` at boot — the SAME
    /// config that floors the identity provider's mint count, so plan-slots
    /// and minted identities stay 1:1. Replicating the same role is what
    /// makes a ≥2 population SAFE today: the boot-2 position-pairing hazard
    /// that defers heterogeneous multi-role plans (#133 slice 14) cannot
    /// mis-pair identical roles. Two-solver cooperation needs ≥2.
    population: usize,
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
            serving_base_model: None,
            serving_lanes: 1,
            serving_context_window: crate::cognition::serving_plan::MIN_SERVE_CTX,
            population: 1,
        }
    }

    /// Set how many citizens of the tier's role template to host. Clamped to
    /// ≥1. Wired from `CONTINUUM_PERSONA_FLOOR` at boot. See the `population`
    /// field doc for why replicating a homogeneous role is boot-2-safe.
    pub fn with_population(mut self, population: usize) -> Self {
        self.population = population.max(1);
        self
    }

    /// Apply the serving daemon's [`ServingPlan`](crate::cognition::serving_plan::ServingPlan):
    /// every desired role runs the plan's base model, lane count, and host-fit
    /// served context window. The daemon makes the honest per-host decision
    /// (budget + footprints, GPU-residency, served window); the spawner obeys
    /// it. Passing the whole plan by reference (per [[pass-the-model-struct-no-param-hell]])
    /// keeps adding a new serving knob a one-field change, not a signature
    /// re-engineer. `None` (or a plan that doesn't fit GPU) keeps the tier's
    /// `plan_for_tier` defaults.
    pub fn with_serving(
        mut self,
        plan: Option<&crate::cognition::serving_plan::ServingPlan>,
    ) -> Self {
        if let Some(p) = plan.filter(|p| p.fits_on_gpu) {
            self.serving_base_model = Some(p.base_model_id.clone());
            self.serving_lanes = p.lanes.max(1);
            self.serving_context_window = p
                .served_context_window
                .max(crate::cognition::serving_plan::MIN_SERVE_CTX);
        }
        self
    }

    /// Currently-planned desired roster. Pure over the module's configured
    /// tier + the serving overrides; no async, no lock — safe anywhere.
    pub fn plan(&self) -> Vec<DesiredRole> {
        let mut template = plan_for_tier(self.hw_capability, self.tier_category);
        for role in &mut template {
            if let Some(ref base) = self.serving_base_model {
                role.model_id = base.clone();
            }
            role.lanes = self.serving_lanes;
            role.served_context_window = self.serving_context_window;
        }
        // Replicate the (homogeneous) tier template to the configured
        // population. The role template stays single-source in `plan_for_tier`;
        // `population` only scales the COUNT of citizens, never introduces a
        // second DIFFERENT role — so the boot-2 position-pairing hazard (which
        // only bites heterogeneous rosters, #133 slice 14) stays sidestepped.
        // population==1 returns the template unchanged (the prior behavior).
        let mut roster = Vec::with_capacity(template.len() * self.population);
        for _ in 0..self.population {
            roster.extend(template.iter().cloned());
        }
        roster
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
//   was originally inlined per-persona at the call site; slice 9
//   factors that out so it's reusable from production boot and from
//   integration tests.

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
    registry: &crate::model_registry::Registry,
) -> Result<Vec<MaterializedPersonaPlan>, BootstrapPlannedError> {
    let plan = module.plan();
    let required = plan.len();
    let mut bootstrapped: Vec<(RoleId, PersonaInstanceInfo, String, ServingParams)> =
        Vec::with_capacity(required);

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

        bootstrapped.push((
            desired.role,
            info,
            desired.model_id.clone(),
            ServingParams {
                lanes: desired.lanes,
                served_context_window: desired.served_context_window,
            },
        ));
    }

    let roster: Vec<RosterEntry> = bootstrapped
        .iter()
        .map(|(role, info, model_id, serving)| RosterEntry {
            role: *role,
            persona_id: info.peer_id.as_uuid(),
            persona_name: info.agent_name.clone(),
            model_id: model_id.clone(),
            serving: *serving,
        })
        .collect();

    let profiles = derive_spawn_plan(&roster, tier_id, module.tier_category(), registry);

    Ok(bootstrapped
        .into_iter()
        .zip(profiles)
        .map(|((role, instance, _model_id, _serving), profile)| MaterializedPersonaPlan {
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
    ///
    /// Slice 13 update: temporarily single-Helper while ResumeOrMint-
    /// Provider's alphabetical sort + position-pairing hazard is
    /// resolved in slice 14. Coder will be re-added once
    /// role-in-seed.json lands.
    #[test]
    fn compat_tier_plans_single_helper_on_lcd() {
        let plan = plan_for_tier(
            HwCapabilityTier::MacIntelMetalDiscrete,
            HwTierCategory::Compat,
        );
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].role, RoleId::Helper);
        assert_eq!(
            plan[0].model_id,
            "continuum-ai/qwen2.5-0.5b-instruct-GGUF"
        );
    }

    /// Every tier currently plans exactly one Helper — until slice 14
    /// lands role-in-seed.json + RoleAwareProvider, multi-role plans
    /// would mis-pair roles on boot 2 (alphabetic-disk-order vs
    /// plan-order). Single role per tier is the safe floor for now.
    #[test]
    fn every_tier_plans_single_helper() {
        for (hw, cat) in [
            (HwCapabilityTier::CpuOnly, HwTierCategory::Compat),
            (HwCapabilityTier::M1Uma8Gb, HwTierCategory::MSeries),
            (HwCapabilityTier::M5UmaProMax, HwTierCategory::MSeriesPro),
            (HwCapabilityTier::Sm120, HwTierCategory::Cuda),
            (HwCapabilityTier::Cloud, HwTierCategory::Cloud),
        ] {
            let plan = plan_for_tier(hw, cat);
            assert_eq!(plan.len(), 1, "tier {cat:?} planned {} roles, want 1", plan.len());
            assert_eq!(
                plan[0].role,
                RoleId::Helper,
                "tier {cat:?} first slot must be Helper"
            );
        }
    }

    /// Regression test for the position-pairing hazard (PR #1510 review
    /// finding #2). Pinned `#[ignore]` until slice 14 lands role-in-
    /// seed.json + RoleAwareProvider. The body documents what slice
    /// 14 must restore: tier-shaped multi-role rosters that survive
    /// boot 2's alphabetic persona order without flipping roles.
    #[test]
    #[ignore = "tracks #133 slice 14 — role-in-seed.json + RoleAwareProvider"]
    fn slice_14_restores_helper_plus_coder_for_compat() {
        let plan = plan_for_tier(
            HwCapabilityTier::MacIntelMetalDiscrete,
            HwTierCategory::Compat,
        );
        // When slice 14 ships, this assertion becomes the live spec:
        assert_eq!(plan.len(), 2);
        assert!(plan.iter().any(|r| r.role == RoleId::Helper));
        assert!(plan.iter().any(|r| r.role == RoleId::Coder));
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
        // continuum_root/daemon_socket never get touched.
        let instance_manager = PersonaInstanceManagerModule::new(
            crate::persona::PersonaAircRuntimeRegistry::default(),
            PathBuf::from("/dev/null/unused"),
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
                // Slice 13 P2: single-Helper plan until slice 14 lands
                // role-in-seed.json. `required` reflects the current
                // plan size; updates to 2 when Coder returns.
                assert_eq!(required, 1);
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
            lanes: 1,
            served_context_window: 8192,
        };
        let json = serde_json::to_string(&role).expect("serialize");
        // RoleId already serializes as snake_case ("helper"); model_id
        // becomes modelId per the camelCase rename_all on this struct.
        assert!(json.contains("\"role\":\"helper\""));
        assert!(json.contains("\"modelId\":\"continuum-ai/qwen2.5-0.5b-instruct-GGUF\""));
        assert!(json.contains("\"servedContextWindow\":8192"));
        let back: DesiredRole = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, role);
    }
}
