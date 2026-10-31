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
//!   inference profile, then `LlamaCppAdapter::for_launch` to
//!   construct the adapter.
//! - Per-persona subscribe-and-respond tokio task (the demo binary's
//!   main loop, factored as a reusable function).
//!
//! Splitting the planning from the async bootstrap chain keeps each
//! commit reviewable and testable without an airc fixture.

use crate::cognition::model_resolver::types::HwCapabilityTier;
use crate::persona::instance_manager::{PersonaInstanceInfo, PersonaInstanceManagerModule};
use crate::persona::hw_tier_descriptor::HwTierCategory;
use crate::persona::identity_provider::PersonaIdentityIntent;
use crate::persona::inference_profile::{InferenceProfileError, PersonaInferenceProfile};
use crate::persona::profile_builder::ServingParams;
use crate::experience::role::RoleId;
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
    // The data-absent floor: one Helper, the same roster the embedded chat
    // recipe declares. Production does NOT take this path with a hardcoded
    // roster anymore — boot reads `RecipeExperienceSource::resident_roles`
    // (the DEFAULT experience's `citizens`, #430) and injects it via
    // `with_citizens`; this wrapper serves tests/fixtures that construct a
    // spawner without recipe data.
    plan_for_roles(&[RoleId::Helper], hw_capability, tier_category)
}

/// Compose the desired roster from RECIPE-DECLARED roles (#430). The roles
/// come from authored data (the default experience's `citizens`); this
/// function maps each onto the serving scaffold — LCD fallback model until
/// the serving daemon's plan overrides it via `with_serving`, single lane,
/// runnable-floor window.
///
/// SLICE-14 GUARD, now enforced on DATA: heterogeneous multi-role plans are
/// unsafe until role-in-seed.json lands — ResumeOrMintProvider's
/// alphabetic-disk-order position-pairing would flip role identity on boot 2
/// (PR #1510/#1511). A recipe may legally AUTHOR several citizens; until
/// slice 14, everything after the first role is DEFERRED with a loud error
/// naming each dropped role — never a silent cap, never a debug-only assert
/// (an author can hit this in release).
pub fn plan_for_roles(
    roles: &[RoleId],
    hw_capability: HwCapabilityTier,
    tier_category: HwTierCategory,
) -> Vec<DesiredRole> {
    let _ = hw_capability; // informational; future per-tier role_template
                           // selection consumes it
    let _ = tier_category; // roster shape now comes from recipe data, not
                           // the tier; the tier drives MODEL selection via
                           // the serving plan
    if roles.len() > 1 {
        tracing::error!(
            hosted = ?roles[0],
            deferred = ?&roles[1..],
            "recipe declares a multi-role roster before role-in-seed.json \
             (#133 slice 14) — hosting the FIRST role only; the rest are \
             deferred until role identity survives a reboot"
        );
    }
    roles
        .iter()
        .take(1)
        .map(|role| DesiredRole {
            role: role.clone(),
            // Fallback model when no ServingPlan is published yet: the FLOOR, which
            // is the 1.5B coder — there is no 0.5B row in the catalog any more. The
            // serving daemon's plan overrides this via with_serving — that's the
            // real, honest, GPU-residency-aware pick.
            model_id: "continuum-ai/qwen2.5-coder-1.5b-instruct-GGUF".to_string(),
            // Default single lane; with_serving overrides from the live plan.
            lanes: 1,
            // Runnable floor until the serving daemon publishes a host-fit
            // window.
            served_context_window: crate::inference::planning::MIN_SERVE_CTX,
        })
        .collect()
    // TODO #133 slice 14: host every declared role after RoleAwareProvider +
    // role-in-seed.json land. Remove the take(1) + error above as part of
    // that change.
}

/// The operator's roster state at one instant: the hold (an allow-list that bounds the
/// seats) and how many seats rest under this build. It lives in the operator's home, so
/// it is an INPUT to the seat arithmetic, never a read buried inside it: production
/// reads it from this host ([`OperatorRoster::on_this_host`]); a test hands a fixed one,
/// so the seat count never depends on whichever hold the machine running it carries
/// (card c4d84be3: the M5's four-name hold made four spawner tests seat 4).
#[derive(Clone, Debug, Default)]
pub struct OperatorRoster {
    pub hold: Option<crate::persona::roster_hold::RosterHold>,
    pub resting: Vec<crate::persona::resting_seat::RestingSeat>,
    /// Durably dormant minds (`persona::dormancy`): never drawn, never counted as seats.
    /// `Unavailable` (the ledger has not loaded) blocks every home resumed from disk.
    pub dormant: crate::persona::dormancy::DormantSet,
}

impl OperatorRoster {
    /// The hold and the resting seats as this host's operator left them.
    pub fn on_this_host() -> Self {
        Self {
            hold: crate::persona::roster_hold::active(),
            resting: crate::persona::resting_seat::resting(),
            dormant: crate::persona::dormancy::dormant_set(),
        }
    }

    fn is_resting(&self, agent_name: &str) -> bool {
        self.resting.iter().any(|s| s.agent_name.eq_ignore_ascii_case(agent_name))
    }

    fn is_dormant(&self, agent_name: &str) -> bool {
        self.dormant.contains(agent_name)
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
    /// The lane count of the last SETTLED geometry this host served, handed in at boot
    /// (`served_window_store`) — the warm-slot bound's source until the first fitting
    /// live plan sets `serving_lanes`. `None` = this host has never served: no bound.
    remembered_lanes: Option<u32>,
    /// The RECIPE-DECLARED resident roles (#430) — production injects the
    /// default experience's `citizens` via [`Self::with_citizens`]; the
    /// constructor default (single Helper, matching the embedded chat
    /// recipe) serves tests/fixtures built without recipe data.
    citizens: Vec<RoleId>,
    /// THE GRID ALLOCATION'S ROSTER FOR THIS NODE (card 10bba591): the seats the
    /// allocator counts here — seated + open for this owner. ONE OWNER for the lane-side
    /// bound: once the allocator daemon has published, this is it, and the warm-slot
    /// bound (`bounded_by_warm_slots`) is only the prior the boot draws on until then.
    /// The identity-side bound (what the provider can yield: the population under the
    /// hold, minus resting seats) stays the spawner's — the allocation never seats a
    /// name the provider cannot fill. Told by the reconciler each pass
    /// (`set_grid_roster`), never a global read on the draw path.
    grid_roster: Option<crate::cognition::grid_allocation::GridRoster>,
    /// Where the operator's roster state comes from (see [`OperatorRoster`]): read
    /// live on every `seats()` and every draw, because `set`/`clear`/`rest`/`wake`
    /// change it at runtime. Production: [`OperatorRoster::on_this_host`].
    operator_roster: fn() -> OperatorRoster,
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
            remembered_lanes: None,
            serving_context_window: crate::inference::planning::MIN_SERVE_CTX,
            population: 1,
            citizens: vec![RoleId::Helper],
            grid_roster: None,
            operator_roster: OperatorRoster::on_this_host,
        }
    }

    /// Inject where the operator's roster state is read from (default: this host's).
    pub fn with_operator_roster(mut self, source: fn() -> OperatorRoster) -> Self {
        self.operator_roster = source;
        self
    }

    /// The operator's roster state as this module's source reads it right now.
    pub fn operator_roster(&self) -> OperatorRoster {
        (self.operator_roster)()
    }

    /// The grid allocation's roster for this node, as the reconciler last read it
    /// (see the `grid_roster` field). `None` = nothing published: the warm-slot prior.
    pub(crate) fn set_grid_roster(&mut self, roster: Option<crate::cognition::grid_allocation::GridRoster>) {
        self.grid_roster = roster;
    }

    /// Inject the RECIPE-DECLARED resident roles (#430) — the default
    /// experience's `citizens`, read by boot from
    /// `RecipeExperienceSource::resident_roles`. An EMPTY list is a
    /// legitimate authored state (a headless serving node with no resident
    /// personas) and produces an empty plan; `plan_for_roles` guards the
    /// slice-14 multi-role hazard.
    pub fn with_citizens(mut self, citizens: Vec<RoleId>) -> Self {
        self.citizens = citizens;
        self
    }

    /// Set how many citizens of the tier's role template to host. Clamped to
    /// ≥1. Wired from `CONTINUUM_PERSONA_FLOOR` at boot. See the `population`
    /// field doc for why replicating a homogeneous role is boot-2-safe.
    pub fn with_population(mut self, population: usize) -> Self {
        self.population = population;
        self
    }
    /// The last settled lane count this host served (see `remembered_lanes`), for the
    /// boot draw's bound before any live plan is known.
    pub fn with_remembered_lanes(mut self, lanes: Option<u32>) -> Self {
        self.remembered_lanes = lanes.filter(|l| *l > 0);
        self
    }
    /// The warm slots this node is known to serve, from what it was TOLD (never a global
    /// read on the draw path): the live fitting plan's lanes, else the remembered settled
    /// geometry, else none.
    fn warm_lanes(&self) -> Option<u32> {
        if self.serving_base_model.is_some() && self.serving_lanes > 0 {
            Some(self.serving_lanes)
        } else {
            self.remembered_lanes
        }
    }

    /// Mutating sibling of [`Self::with_population`], for the boot task that
    /// only learns the REAL population after constructing the identity
    /// provider (#432: the plan must be sized to what the provider will
    /// yield — every resumed citizen — not to the mint floor alone). Same
    /// ≥1 clamp; same homogeneous-replication safety as the builder form.
    pub fn set_population(&mut self, population: usize) {
        self.population = population;
    }

    /// Apply the serving daemon's [`ServingPlan`](crate::inference::planning::serving_plan::ServingPlan):
    /// every desired role runs the plan's base model, lane count, and host-fit
    /// served context window. The daemon makes the honest per-host decision
    /// (budget + footprints, GPU-residency, served window); the spawner obeys
    /// it. Passing the whole plan by reference (per [[pass-the-model-struct-no-param-hell]])
    /// keeps adding a new serving knob a one-field change, not a signature
    /// re-engineer. `None` (or a plan that doesn't fit GPU) keeps the tier's
    /// `plan_for_tier` defaults.
    pub fn with_serving(
        mut self,
        plan: Option<&crate::inference::planning::serving_plan::ServingPlan>,
    ) -> Self {
        if let Some(p) = plan.filter(|p| p.fits_on_gpu) {
            self.serving_base_model = Some(p.base_model.model_id.clone());
            self.serving_lanes = p.lanes.max(1);
            self.serving_context_window = p
                .served_context_window
                .max(crate::inference::planning::MIN_SERVE_CTX);
        }
        self
    }

    /// Mutating twin of [`Self::with_serving`] for the resident reconciler: the
    /// serving plan changes at runtime (a pin, a tier swap, a lane relaunch) and the
    /// roster's seat count follows the LIVE lane count, never the boot-time one.
    pub fn set_serving(&mut self, plan: Option<&crate::inference::planning::serving_plan::ServingPlan>) {
        match plan {
            Some(p) if p.fits_on_gpu => {
                self.serving_base_model = Some(p.base_model.model_id.clone());
                self.serving_lanes = p.lanes.max(1);
                self.serving_context_window = p
                    .served_context_window
                    .max(crate::inference::planning::MIN_SERVE_CTX);
            }
            // A live plan that does NOT fit the GPU (a CPU-tier node, or a runtime
            // downgrade) must never leave the seat count on a stale GPU plan's lanes
            // (IntelMac's review of #3798): the marker and the lane count move together —
            // both cleared, so `seats()` falls back to the configured population and the
            // runnable-floor template.
            _ => {
                self.serving_base_model = None;
                self.serving_lanes = 1;
            }
        }
    }

    /// Currently-planned desired roster. Pure over the module's configured
    /// tier + the serving overrides; no async, no lock — safe anywhere.
    pub fn plan(&self) -> Vec<DesiredRole> {
        let mut template =
            plan_for_roles(&self.citizens, self.hw_capability, self.tier_category);
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
        // THE ROSTER NEVER EXCEEDS THE WARM LANES (Joel 2026-09-06: "your machine should
        // be serving the qwen3.8 for these coder personas"; measured the night before:
        // a switch to the 27B left 12 minds on 3 lanes at ACT 0 / QUE 12, one act per
        // 30 min). A mind without a lane is a cold restore every turn, not a citizen.
        // When the daemon has published a real plan, the population is capped at its
        // lane count; the runnable-floor template (no plan yet) keeps the configured
        // population so boot never under-hosts on a stale guess.
        let seats = self.seats();
        let mut roster = Vec::with_capacity(template.len() * seats);
        for _ in 0..seats {
            roster.extend(template.iter().cloned());
        }
        roster
    }

    /// How many citizens this node seats: the configured population minus dormant homes,
    /// under any hold, minus resting seats, BOUNDED by the warm slots (`bounded_by_warm_slots`).
    ///
    /// THE REVERSAL (Joel, 2026-09-20, card 7c38ff6f). On 2026-09-13 the rule was "the
    /// lane count is NOT a cap: N minds over M slots is the design, KV pages to disk and
    /// restores on return". Measured a week later on the M5: sixteen resident minds cost
    /// the plan its second lane (their residency is memory the 27B needed), every
    /// unslotted mind re-prefilled cold, and the 22:22Z hour read acts 5, writes 0, 2,174
    /// of 2,177 pulls lane-deferred; the boot of 23d33019d drew sixteen onto a 20 GB
    /// budget and served ONE lane at 2,048 tokens. "The nursery is over-saturated": the
    /// active roster is the warm slots × ~2, the rest dormant with identity and memory
    /// intact, waking as the lanes grow. The KV page store stays the restore path for the
    /// minds that ARE seated.
    pub fn seats(&self) -> usize {
        let operator = self.operator_roster();
        // A dormant home is not part of the population that may sit: out BEFORE the hold
        // bounds it (a four-name hold over twelve dormant homes still seats four).
        let awake = self.population.saturating_sub(operator.dormant.len());
        let unbounded = seats_under(awake, operator.hold.as_ref()).saturating_sub(operator.resting.len());
        let lanes = self.warm_lanes();
        // The allocation's seat count for this node owns the lane-side bound once
        // published (card 10bba591); the warm-slot bound is the prior before it.
        let seats = match self.grid_roster {
            Some(r) => unbounded.min(r.seats() as usize),
            None => bounded_by_warm_slots(unbounded, lanes),
        };
        // One row when the bound CHANGES what is drawn, not one per reconcile pass.
        static LAST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);
        let shape = ((unbounded as u64) << 32) | seats as u64;
        if seats < unbounded && LAST.swap(shape, std::sync::atomic::Ordering::Relaxed) != shape {
            continuum_base::probe!(
                class = "persona.roster.bounded",
                population = unbounded as u64,
                lanes = lanes.unwrap_or(0) as u64, // unwrap_or: 0 = no lane known (then nothing is bounded; unreachable here)
                seats = seats as u64,
                minds_per_lane = crate::cognition::health_ledger::MINDS_PER_LANE_STARVED_ABOVE,
                "the draw is bounded by the warm slots — the rest of the roster stays dormant until the lanes grow"
            );
        }
        seats
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

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
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
/// PHASE 1 of [`bootstrap_planned`]: draw one identity per planned slot from the
/// provider, in its yield order — SKIPPING identities a standing roster hold does
/// not allow. The hold says WHO may be hosted; the plan says HOW MANY seats there
/// are; the seats go to the first `required` ALLOWED identities. Measured
/// 2026-09-06 17:3xZ: with a 5-lane plan (#3798) and a hold naming five coders,
/// the old draw took the first five identities alphabetically, the hold then
/// dropped three of them, and the three named coders further down the yield were
/// never reached — two hosted, three seats empty, the round dead. Under a hold,
/// provider exhaustion fills fewer seats loudly (a probe per held-out identity)
/// instead of failing the boot: the operator asked for those names, and only those.
pub(crate) async fn draw_intents(
    provider: &mut dyn crate::persona::identity_provider::PersonaIdentityProvider,
    plan: &[DesiredRole],
    operator: &OperatorRoster,
    already_hosted: usize,
) -> Result<Vec<PersonaIdentityIntent>, BootstrapPlannedError> {
    let hold = operator.hold.as_ref();
    let required = plan.len();
    let mut intents: Vec<PersonaIdentityIntent> = Vec::with_capacity(required);
    let mut drawn = 0usize;
    while intents.len() < required {
        let slot_index = intents.len();
        let desired = &plan[slot_index];
        let next = provider
            .next_persona()
            .await
            .map_err(|source| BootstrapPlannedError::IdentityProvider {
                slot_index,
                role: desired.role,
                source,
            })?;
        let Some(intent) = next else {
            // A SHORTFALL IS A PARTIAL ROSTER, NEVER A DRAIN (2026-09-13): with the
            // roster no longer capped at lanes, a plan can ask for more seats than the
            // provider holds identities (or than a hold allows). Returning Err here made
            // the host drain the already-registered minds as "partially registered" and
            // retry every second — zero residents until an operator intervened. The
            // identities the provider yielded ARE the roster; the plan's surplus seats are
            // the plan's business (a probe names the shortfall), not a boot failure.
            // A TOP-UP THAT DRAWS NOTHING IS ALSO A PARTIAL ROSTER (IntelMac 2026-09-21
            // 13:00:49Z, Fable's read): the guard above only knew this pass's own draw.
            // On a reconciler pass the residents from earlier passes are the roster —
            // `already_hosted` of them — and a plan that opened two more seats than the
            // provider has names left drew zero at slot 0, so this returned the honest
            // boot failure for a node that was fully staffed, and the host drained six
            // live citizens as "partial". The shortfall is the plan's business either way.
            if !intents.is_empty() || already_hosted > 0 {
                continuum_base::probe!(
                    class = "persona.host.provider_filled_fewer_seats",
                    seats = required,
                    filled = intents.len(),
                    already_hosted,
                    drawn,
                    held = hold.is_some(),
                    "the provider ran out of identities (or the hold allows no more) — seating fewer, not failing"
                );
                break;
            }
            return Err(BootstrapPlannedError::IdentityProviderExhausted {
                slot_index,
                role: desired.role,
                provided: slot_index,
                required,
            });
        };
        drawn += 1;
        // A RESTING seat (the substrate paged her out: card aed15611) is not re-drawn —
        // she returns on a change, never on the next reconcile. The seat count already
        // excludes her (`plan()`), so the loop never asks the provider for a seat it
        // cannot fill.
        if operator.is_resting(&intent.agent_name) {
            continuum_base::probe!(
                class = "persona.host.resting",
                agent = %intent.agent_name,
                "resting seat: identity drawn is paged out until a change — seat goes to the next one"
            );
            continue;
        }
        // A DORMANT mind (`persona::dormancy`) is never resumed by the reconciler: she
        // returns on `persona/instances/wake`, never on a clock or a restart.
        // FAIL CLOSED: with the ledger unavailable nobody knows who sleeps, so no home is
        // resumed from disk (a fresh mint is still a birth, never a dormant mind).
        if operator.dormant == crate::persona::dormancy::DormantSet::Unavailable
            && intent.source == crate::persona::identity_provider::PersonaIdentitySource::ResumedFromDisk
        {
            continuum_base::probe!(
                class = "persona.dormancy.ledger_unavailable",
                agent = %intent.agent_name,
                "the wakefulness ledger has not loaded: no home is resumed from disk until it does (persona/spawn still births)"
            );
            continue;
        }
        if operator.is_dormant(&intent.agent_name) {
            continuum_base::probe!(
                class = "persona.host.dormant",
                agent = %intent.agent_name,
                "dormant home: the reconciler does not resume her until she is woken — seat goes to the next one"
            );
            continue;
        }
        // Only an EXCLUSIVE (operator-file) hold holds anyone out. A derived team hold
        // orders the roster — team first — but seats the whole population (below).
        if let Some(h) = hold.filter(|h| h.exclusive) {
            if !h.allows(&intent.agent_name) {
                continuum_base::probe!(
                    class = "persona.host.held_out",
                    agent = %intent.agent_name,
                    reason = %h.reason,
                    "roster hold: identity drawn at boot is not on the allow-list — seat goes to the next allowed one"
                );
                continue;
            }
        }
        crate::persona::resting_seat::note_seated(
            &intent.agent_name,
            continuum_base::time::EpochMs::now().millis(),
        );
        intents.push(intent);
    }
    // A DERIVED hold (a working round's team) seats its names FIRST and everyone else
    // after — never fewer minds. Stable: the provider's order is kept within each half.
    if let Some(h) = hold.filter(|h| !h.exclusive) {
        let (team, rest): (Vec<_>, Vec<_>) = intents.into_iter().partition(|i| h.allows(&i.agent_name));
        intents = team.into_iter().chain(rest).collect();
    }
    Ok(intents)
}

/// The slots still to fill: the module's plan cut to `seats - already_hosted`. The
/// roster follows the LIVE lane count in both directions — at boot the first fitting
/// plan is often the small under-memory one (one or two lanes, card 40f53419), so a
/// cap computed there seats one citizen; when the pin brings five lanes the
/// reconciler must draw the four missing seats on that edge, never wait for a
/// reboot (M5 2026-09-06 18:0xZ: one citizen on a five-lane node).
/// The seat count under an operator hold: a hold DEFINES the roster — only its
/// names may sit — so it also bounds how many seats there are to fill. Without
/// this (2026-09-13, first boot after the lane cap left the roster): population 12,
/// hold of 5 → draw_intents filled 5 and returned; the reconciler then asked for
/// the 7 "missing" every second, the exhausted provider yielded none, the Err path
/// drained the five as partially registered, and the node sat at ZERO residents
/// until the operator moved the hold file aside. Pure.
pub fn seats_under(population: usize, hold: Option<&crate::persona::roster_hold::RosterHold>) -> usize {
    // ZERO IS DELIBERATE (2026-09-15, the one-roster rule): the population is what the
    // identity provider WILL yield, and an empty node that hears a team seated elsewhere
    // on the grid yields none — it offers lanes. A ≥1 clamp here would draw a seat the
    // provider cannot fill, the "identity provider exhausted at slot 0" retry every
    // second (the 2026-09-12 shape).
    let seats = population;
    match hold {
        Some(h) if h.exclusive && !h.only.is_empty() => seats.min(h.only.len()),
        _ => seats,
    }
}

/// THE DRAW IS BOUNDED BY THE WARM SLOTS (card 7c38ff6f; Joel, 2026-09-20: "23 minds … the
/// nursery is over-saturated" — the active roster is the warm slots × ~2, the rest dormant).
/// Measured the boot of 23d33019d on the M5, 01:12Z: the old lane was dead at boot, sixteen
/// minds loaded, their residency left the 27B a 20 GB Eco budget, the plan launched ONE
/// lane at 2,048 tokens, and the hourly rest was fifty minutes away. The bound belongs at
/// the draw: `seats` never exceeds `lanes × MINDS_PER_LANE_STARVED_ABOVE` (never below the
/// resident floor), so the memory the roster would have taken is the plan's for lanes. No
/// lane known (`None`, a node that has never served) bounds nothing — an absence is not a
/// number. The mirror is free: the reconciler re-reads `seats()` every pass, so lanes that
/// grow draw more; lanes that shrink are the hourly rest's (`citizen_health`). Pure.
pub fn bounded_by_warm_slots(seats: usize, lanes: Option<u32>) -> usize {
    match lanes {
        Some(l) if l > 0 => {
            let edge = (l as usize).saturating_mul(crate::cognition::health_ledger::MINDS_PER_LANE_STARVED_ABOVE as usize);
            seats.min(edge.max(crate::cognition::health_ledger::MINDLESS_RESIDENT_FLOOR as usize))
        }
        _ => seats,
    }
}

pub fn missing_plan(module: &PersonaSpawnerModule, already_hosted: usize) -> Vec<DesiredRole> {
    let mut plan = module.plan();
    let missing = plan.len().saturating_sub(already_hosted);
    plan.truncate(missing);
    plan
}

pub async fn bootstrap_planned(
    module: &PersonaSpawnerModule,
    instance_manager: &PersonaInstanceManagerModule,
    provider: &mut dyn crate::persona::identity_provider::PersonaIdentityProvider,
    tier_id: &str,
    registry: &crate::model_registry::Registry,
    already_hosted: usize,
) -> Result<Vec<MaterializedPersonaPlan>, BootstrapPlannedError> {
    let plan = missing_plan(module, already_hosted);
    if plan.is_empty() {
        return Ok(Vec::new());
    }
    let required = plan.len();
    let mut bootstrapped: Vec<(RoleId, PersonaInstanceInfo, String, ServingParams)> =
        Vec::with_capacity(required);

    // PHASE 1 — draw every identity from the provider. Sequential because the
    // provider hands out one identity at a time (`&mut`), but this is CHEAP: the
    // cost is `bootstrap_one` below, not `next_persona`.
    let intents = draw_intents(provider, &plan, &module.operator_roster(), already_hosted).await?;

    // PHASE 2 — bootstrap ALL personas CONCURRENTLY (fork/join). The airc keypair
    // ceremony + room join + seed are INDEPENDENT per persona, so a serial loop
    // paid ~minutes PER citizen: measured 2026-09-03, ~7-minute gaps between
    // successive citizens on a reboot, so the last of N idled 30+ minutes with
    // `resident=0` and no turns — the slow-restart Joel called out ("reboot needs
    // work… always think of parallel", the CBAR pthreads/lambdas fork-join shape).
    // Now the restart's bootstrap cost is the SLOWEST SINGLE persona, not their
    // sum. `bootstrap_one` takes `&self`, so concurrent calls share no mutable
    // state; each writes its own per-persona identity dir + seed.
    let infos = futures::future::join_all(
        intents
            .iter()
            .map(|intent| instance_manager.bootstrap_one(intent)),
    )
    .await;

    for (slot_index, (desired, info_res)) in plan.iter().zip(infos).enumerate() {
        let info = info_res.map_err(|source| BootstrapPlannedError::AircBootstrap {
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
        .map(
            |((role, instance, _model_id, _serving), profile)| MaterializedPersonaPlan {
                role,
                instance,
                profile,
            },
        )
        .collect())
}

#[cfg(test)]
mod tests;
