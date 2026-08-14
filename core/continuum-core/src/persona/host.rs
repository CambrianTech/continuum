//! Persona host orchestration — slice 12 + slice 13.5.
//!
//! This module is the substrate's "spawn / host" surface for
//! personas. It owns the composition seam that turns an identity
//! provider into a roster of hosted personas talking on the grid.
//!
//! ## What lives here
//!
//! - [`spawn_persona_service`] — slice 12: the per-persona compose
//!   point. Takes a fully-assembled [`PersonaContext`] and starts
//!   her service loop on a tokio handle. Used by the supervisor
//!   below and by integration tests that need a single persona on
//!   the grid without the full boot pipeline.
//! - [`PersonaSpawnSupervisor`] — slice 13.5: the boot-level
//!   orchestrator. Wraps the slice 7-12 pipeline into one named
//!   class. Construct it once at substrate boot, call
//!   [`PersonaSpawnSupervisor::spawn_all`] with an identity
//!   provider, and it produces a [`BootSummary`] reporting what
//!   shipped vs what failed.
//! - [`BootSummary`] / [`BootSlotFailure`] — typed boot-result
//!   structs. ts-rs-exported so the substrate's observability +
//!   admin surfaces (web client, jtag CLI, future
//!   `persona:boot:summary` event consumers) all see the same
//!   shape per [[clients-are-rust-too-thin-node-web-shell]].
//!
//! ## Why the extract-class refactor
//!
//! Pre-13.5, the boot pipeline was ~170 lines of inline code in
//! `ipc/mod.rs::start_server`. That mixed "boot the IPC server"
//! with "spawn personas" — two concerns with different lifetimes
//! and different test needs. Per Joel's "obsessive elegance"
//! direction (2026-06-02), this module names the persona-spawn
//! concern: one struct, one entry point, one typed result. IPC boot
//! shrinks to ~10 lines that construct + call the supervisor.

use crate::persona::airc_persona_conversation::AircPersonaConversation;
use crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry;
use crate::persona::airc_source::AircTranscriptReader;
use crate::persona::identity_provider::PersonaIdentityProvider;
use crate::persona::role_template::RoleId;
use crate::persona::service_loop::{
    serve_persona_loop, PersonaConversation, ServeOptions, ServeOutcome,
};
use crate::persona::spawner_module::{bootstrap_planned, PersonaSpawnerModule};
use crate::persona::supervisor::{
    materialize_adapters, HostedPersona, PersonaAdapterFactory, PersonaContext, SupervisorError,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::task::JoinHandle;
use uuid::Uuid;

/// Spawn one tokio task that hosts a single persona on the airc grid:
/// subscribes to her room, runs `serve_persona_loop` against the
/// cognition path, posts replies through `ctx.runtime.say`.
///
/// Per [[init-once-handle-then-lease-zero-copy-refs]]: the airc
/// subscribe round-trip happens HERE, BEFORE the JoinHandle is
/// returned. When this future resolves, the persona is genuinely
/// ready to converse — her stream is open, her registry slot, when
/// later attached, advertises a substrate-correct "hosted = ready"
/// invariant (slice-13.6 reviewer fix to PR #1514).
///
/// On Err: the daemon round-trip (prime) failed and the task is
/// NEVER spawned. Caller (supervisor) records the failure in
/// `BootSummary::failures` and continues with sibling slots per
/// [[no-fallbacks-ever]] — no half-spawned persona, no orphaned
/// service loop, no degraded path.
///
/// The returned `JoinHandle` runs until:
/// - the airc subscribe stream ends (daemon disconnect) — resolves
///   with `Ok(ServeOutcome { ... })`.
/// - `serve_persona_loop` returns `Err` from a non-recoverable error
///   (e.g., `high_water_mark` failed) — resolves with `Err`.
/// - `.abort()`'d by the caller — the supervisor's shutdown path.
pub async fn spawn_persona_service(
    ctx: HostedPersona,
    opts: ServeOptions,
    rt_handle: tokio::runtime::Handle,
) -> Result<JoinHandle<Result<ServeOutcome, String>>, String> {
    // `ctx.runtime: Arc<dyn AircCitizen>` — slice 13.5 trait
    // extraction. The reader for the RAG layer upcoerces from
    // `AircCitizen` to its `AircTranscriptReader` supertrait via
    // Rust 1.86+ trait_upcasting; no manual conversion, no Option,
    // no `.expect("None is test-only")` per [[no-fallbacks-ever]].
    let citizen = ctx.runtime.clone();
    let reader: Arc<dyn AircTranscriptReader> = citizen.clone();
    let mut conversation = AircPersonaConversation::new(citizen);

    // Eager priming BEFORE the spawn (slice 13.6 reviewer fix).
    // The substrate's "hosted = ready" invariant requires that the
    // daemon subscribe complete BEFORE this function returns —
    // otherwise the supervisor's `summary.hosted += 1` accountancy
    // is racing N concurrent in-flight subscribes against itself.
    // Per [[init-once-handle-then-lease-zero-copy-refs]]: the init
    // pays at boot, not on hot path; the contract is that the
    // persona is genuinely warm when "hosted" ticks.
    conversation
        .prime()
        .await
        .map_err(|e| format!("conversation.prime() failed before spawn: {e}"))?;

    // Wrap the per-persona service loop in `catch_unwind` so a panic
    // inside any single turn (malformed input, adapter bug, RAG store
    // corruption) doesn't silently kill THIS persona's task — same
    // RTOS-safe shape `start_tick_loops` uses for every ServiceModule.
    // Per docs/architecture/CONCURRENCY-STYLE-GUIDE.md: every owned
    // task wraps its body in `AssertUnwindSafe(...).catch_unwind()`
    // and surfaces panics through `probe!` + per-module logger.
    let persona_name = ctx.identity.agent_name.to_string();
    let persona_id = ctx.identity.peer_id.as_uuid();
    Ok(rt_handle.spawn(async move {
        use futures::FutureExt;
        let outcome =
            std::panic::AssertUnwindSafe(serve_persona_loop(&ctx, &mut conversation, reader, opts))
                .catch_unwind()
                .await;
        match outcome {
            Ok(r) => r,
            Err(panic) => {
                let panic_msg = if let Some(s) = panic.downcast_ref::<&'static str>() {
                    (*s).to_string()
                } else if let Some(s) = panic.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "<non-string panic payload>".to_string()
                };
                tracing::error!(
                    persona = %persona_name,
                    persona_id = %persona_id,
                    "Persona service_loop aborted with panic: {}",
                    panic_msg
                );
                crate::probe!(
                    class = "persona.service_loop.aborted",
                    persona = %persona_name,
                    persona_id = %persona_id,
                    reason = %panic_msg
                );
                Err(format!(
                    "persona '{persona_name}' service loop panicked: {panic_msg}"
                ))
            }
        }
    }))
}

/// Typed summary of one boot composition run. Replaces the inline
/// `hosted_count` / `failed_count` counters in `ipc/mod.rs` with a
/// proper struct that downstream consumers (web client, jtag CLI,
/// `persona:boot:summary` event subscribers per the design doc's
/// deferred Q5) can read as one shape.
///
/// ts-rs export is deferred — `RoleId` doesn't derive `TS` yet
/// (slice 14's role-in-seed.json work touches it). Once RoleId is
/// TS-exportable this struct gets the same treatment, and the web
/// client + jtag CLI read identical types via [[clients-are-rust-too-thin-node-web-shell]].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootSummary {
    /// Personas that spawned + attached cleanly.
    pub hosted: usize,
    /// Per-slot failure rows.
    pub failures: Vec<BootSlotFailure>,
}

impl BootSummary {
    /// Total slots attempted.
    pub fn attempted(&self) -> usize {
        self.hosted + self.failures.len()
    }

    /// Convenience getter for the failed count.
    pub fn failed(&self) -> usize {
        self.failures.len()
    }
}

/// One slot's failure facts. Identity is best-effort: if the
/// failure was at the materialization stage (before the adapter was
/// built), we still know the role and slot_index; if it was at the
/// attach stage (after spawn), we additionally know the persona_id.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootSlotFailure {
    pub slot_index: usize,
    /// `None` if the failure happened before role assignment.
    pub role: Option<RoleId>,
    /// `None` if the failure happened before airc-bootstrap.
    pub persona_id: Option<Uuid>,
    /// Human-readable reason. Operators read this to diagnose; the
    /// substrate's [[no-fallbacks-ever]] doctrine means each failure
    /// gets a named cause, not a silent skip.
    pub reason: String,
}

/// The boot-level orchestrator that composes slices 7-12 into one
/// named class.
///
/// Construct once at substrate boot with the configured pipeline
/// inputs; call [`Self::spawn_all`] with an identity provider to
/// produce a roster of hosted personas. The supervisor is the
/// canonical site for the "what does it take to bring N personas
/// online?" question — every layer of the pipeline lives in one
/// place, every per-slot failure path lands in [`BootSummary`].
pub struct PersonaSpawnSupervisor {
    spawner: PersonaSpawnerModule,
    instance_manager: Arc<crate::modules::persona_instance_manager::PersonaInstanceManagerModule>,
    registry: PersonaAircRuntimeRegistry,
    factory: Arc<dyn PersonaAdapterFactory>,
    tier_id: String,
    model_registry: &'static crate::model_registry::Registry,
    rt_handle: tokio::runtime::Handle,
}

impl PersonaSpawnSupervisor {
    /// Construct with the substrate-resolved boot inputs. None of
    /// these arguments are looked up at construction — the
    /// supervisor is a value-type aggregator; the work happens in
    /// [`Self::spawn_all`].
    pub fn new(
        spawner: PersonaSpawnerModule,
        instance_manager: Arc<
            crate::modules::persona_instance_manager::PersonaInstanceManagerModule,
        >,
        factory: Arc<dyn PersonaAdapterFactory>,
        tier_id: impl Into<String>,
        model_registry: &'static crate::model_registry::Registry,
        rt_handle: tokio::runtime::Handle,
    ) -> Self {
        let registry = instance_manager.registry().clone();
        Self {
            spawner,
            instance_manager,
            registry,
            factory,
            tier_id: tier_id.into(),
            model_registry,
            rt_handle,
        }
    }

    /// Run the full boot pipeline:
    ///
    /// 1. `bootstrap_planned`: provider intents → airc-bootstrapped
    ///    [`MaterializedPersonaPlan`](crate::persona::spawner_module::MaterializedPersonaPlan)s.
    /// 2. `materialize_adapters`: plans → hosted [`PersonaContext`]s
    ///    (with the runtime looked up from the registry).
    /// 3. Per slot: `spawn_persona_service` + `attach_service_loop`.
    ///    Failures drain the spawned task and record into
    ///    [`BootSummary::failures`] per [[no-fallbacks-ever]].
    ///
    /// If `bootstrap_planned` fails (slot-fatal — affects every
    /// later slot), the supervisor orderly-drains any partial
    /// registration via `shutdown_slot` and returns a summary with
    /// `hosted=0` and one synthetic failure row noting the cause.
    pub async fn spawn_all(
        &self,
        provider: &mut dyn PersonaIdentityProvider,
        // The substrate's wired `CommandExecutor` (GridTrustAuthPolicy +
        // interceptors), delivered through the executor-ready oneshot. Each
        // persona's HANDS are built over it (identity-scoped), so the ACL gates
        // what they may do. `None` → personas spawn speak-only (no hands).
        tool_command_executor: Option<Arc<crate::runtime::CommandExecutor>>,
    ) -> BootSummary {
        let plans = match bootstrap_planned(
            &self.spawner,
            &self.instance_manager,
            provider,
            &self.tier_id,
            self.model_registry,
        )
        .await
        {
            Ok(p) => p,
            Err(err) => {
                let already_registered = self.registry.ids();
                let orphans = already_registered.len();
                for orphan_id in already_registered {
                    let _ = self.registry.shutdown_slot(orphan_id).await;
                }
                tracing::error!(
                    error = %err,
                    orphans_drained = orphans,
                    "PersonaSpawnSupervisor: bootstrap_planned failed; \
                     {} partially-registered personas drained",
                    orphans,
                );
                return BootSummary {
                    hosted: 0,
                    failures: vec![BootSlotFailure {
                        slot_index: 0,
                        role: None,
                        persona_id: None,
                        reason: format!("bootstrap_planned failed: {err}"),
                    }],
                };
            }
        };

        let mut summary = BootSummary::default();
        self.host_plans(plans, tool_command_executor, &mut summary)
            .await;

        tracing::info!(
            hosted = summary.hosted,
            failed = summary.failed(),
            "🌐 PersonaSpawnSupervisor: boot composition complete — \
             {} citizen(s) hosted, {} failed",
            summary.hosted,
            summary.failed(),
        );

        summary
    }

    /// Host every REGISTERED citizen whose slot has never had a
    /// service loop attached (#429 — the mute-citizen re-entry).
    ///
    /// `persona/spawn` births share `birth_one` with boot, but birth
    /// ends at `registry.register` — hosting (adapter + cognition
    /// loop) only ran from boot's [`Self::spawn_all`]. A command-born
    /// citizen was on airc, in the commons, carded — and MUTE. This
    /// is the standing reconciler's verb: scan the registry, derive
    /// the SAME single-slot plans boot uses (the spawner's roster row
    /// is the config authority until #430 makes it recipe data), and
    /// run the shared hosting tail.
    ///
    /// Attached slots — running OR finished — are skipped: a finished
    /// loop is respawn-on-death territory (slice-14), deliberately
    /// not this verb's concern. Idempotent: calling with nothing
    /// unattended returns an empty summary.
    pub async fn host_unattended(
        &self,
        tool_command_executor: Option<Arc<crate::runtime::CommandExecutor>>,
    ) -> BootSummary {
        let mut summary = BootSummary::default();
        let mut unattended = Vec::new();
        for persona_id in self.registry.ids() {
            // None = no service loop was ever attached. Some(_) — running
            // (false) or finished (true) — means a host decision was already
            // made for this slot; not ours to redo here.
            if self
                .registry
                .is_service_loop_finished(persona_id)
                .await
                .is_none()
            {
                if let Some(rt) = self.registry.get(persona_id) {
                    unattended.push(rt);
                }
            }
        }
        if unattended.is_empty() {
            return summary;
        }

        let plan_rows = self.spawner.plan();
        let Some(desired) = plan_rows.first() else {
            // An empty roster plan with live unhosted citizens is a
            // configuration hole, not a skippable state — say so loudly
            // per [[no-fallbacks-ever]]; no synthetic role is invented.
            tracing::error!(
                unattended = unattended.len(),
                "host_unattended: spawner plan is EMPTY — {} registered \
                 citizen(s) cannot be hosted (no role/model row to derive \
                 an inference profile from)",
                unattended.len()
            );
            return summary;
        };

        let roster: Vec<crate::persona::spawner::RosterEntry> = unattended
            .iter()
            .map(|rt| crate::persona::spawner::RosterEntry {
                role: desired.role,
                persona_id: rt.persona_id(),
                persona_name: rt.agent_name().to_string(),
                model_id: desired.model_id.clone(),
                serving: crate::persona::profile_builder::ServingParams {
                    lanes: desired.lanes,
                    served_context_window: desired.served_context_window,
                },
            })
            .collect();
        let profiles = crate::persona::spawner::derive_spawn_plan(
            &roster,
            &self.tier_id,
            self.spawner.tier_category(),
            self.model_registry,
        );
        let plans: Vec<crate::persona::spawner_module::MaterializedPersonaPlan> = unattended
            .iter()
            .zip(profiles)
            .map(|(rt, profile)| crate::persona::spawner_module::MaterializedPersonaPlan {
                role: desired.role,
                instance:
                    crate::modules::persona_instance_manager::PersonaInstanceInfo::from_runtime(rt),
                profile,
            })
            .collect();

        self.host_plans(plans, tool_command_executor, &mut summary)
            .await;
        summary
    }

    /// Materialize adapters for a batch of plans and attach each
    /// resulting context's service loop — the shared hosting tail of
    /// [`Self::spawn_all`] (boot) and [`Self::host_unattended`]
    /// (post-boot reconcile). One definition: "hosting" IS this
    /// sequence, whichever verb asks for it.
    async fn host_plans(
        &self,
        plans: Vec<crate::persona::spawner_module::MaterializedPersonaPlan>,
        tool_command_executor: Option<Arc<crate::runtime::CommandExecutor>>,
        summary: &mut BootSummary,
    ) {
        let registry_for_lookup = self.registry.clone();
        // `registry.get` returns `Option<Arc<PersonaAircRuntime>>` —
        // the closure upcoerces to `Option<Arc<dyn AircCitizen>>` so
        // `PersonaContext.runtime` stays trait-shaped. Per
        // [[personas-are-citizens-airc-is-identity-provider]] the
        // citizen type is what the substrate carries; the concrete
        // runtime is one impl among future BaseUser variants.
        // Per-persona HANDS: a CommandToolExecutor over the wired executor,
        // scoped to the persona's identity (so the ACL gates it). Cheap — each is
        // an Arc bump on the shared executor + connection. `None` executor →
        // every persona is speak-only.
        let tool_exec_source = tool_command_executor;
        let hosted_results = materialize_adapters(
            plans,
            &*self.factory,
            move |pid| {
                registry_for_lookup
                    .get(pid)
                    .map(|r| r as Arc<dyn crate::persona::airc_citizen::AircCitizen>)
            },
            move |pid| {
                tool_exec_source.clone().map(|ex| {
                    Arc::new(
                        crate::cognition::tool_executor::CommandToolExecutor::for_persona(ex, pid),
                    ) as Arc<dyn crate::cognition::tool_executor::ToolExecutor>
                })
            },
        )
        .await;

        for (slot_idx, result) in hosted_results.into_iter().enumerate() {
            match result {
                Ok(ctx) => self.spawn_and_attach(slot_idx, ctx, summary).await,
                Err(err) => {
                    let (slot_index, role) = supervisor_error_facts(&err);
                    summary.failures.push(BootSlotFailure {
                        slot_index: slot_index.unwrap_or(slot_idx),
                        role: Some(role),
                        persona_id: None,
                        reason: format!("{err}"),
                    });
                    tracing::warn!(
                        slot = slot_idx,
                        error = ?err,
                        "PersonaSpawnSupervisor: slot materialization failed; \
                         sibling slots still attempted"
                    );
                }
            }
        }
    }

    /// Spawn one persona's service loop and attach to the registry.
    ///
    /// Steps:
    /// 1. Call `spawn_persona_service` — this AWAITS the daemon
    ///    subscribe round-trip (prime) before returning. If prime
    ///    fails, no task is spawned; the slot fails cleanly with no
    ///    leaked resources per [[no-fallbacks-ever]].
    /// 2. Attach the returned handle to the registry.
    /// 3. On `attach_service_loop` failure: orderly-drain the spawned
    ///    handle per [[organization-purity-as-we-migrate]] — no
    ///    leaked tokio tasks.
    ///
    /// `summary.hosted += 1` only ticks when BOTH prime succeeded AND
    /// attach succeeded — substrate's "hosted = ready" invariant holds.
    async fn spawn_and_attach(
        &self,
        slot_idx: usize,
        ctx: PersonaContext,
        summary: &mut BootSummary,
    ) {
        let persona_id = ctx.identity.peer_id.as_uuid();
        let agent_name = ctx.identity.agent_name.clone();
        let role = ctx.role;
        // Hand the loop the SAME quiesce atomic the registry slot holds so an
        // eval-preemption lease can suspend her autonomic self-tick. Register precedes
        // spawn, so the flag is present; if it somehow isn't, that's a registration
        // bug, not a serving one — degrade to a private never-set flag (she's simply
        // never quiescable) rather than fail her whole boot.
        let quiesced = self
            .registry
            .quiesced_flag(persona_id)
            .unwrap_or_else(|| std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)));
        let handle = match spawn_persona_service(
            ctx,
            ServeOptions {
                quiesced,
                ..ServeOptions::default()
            },
            self.rt_handle.clone(),
        )
        .await
        {
            Ok(h) => h,
            Err(reason) => {
                tracing::error!(
                    slot = slot_idx,
                    persona_id = %persona_id,
                    reason = %reason,
                    "PersonaSpawnSupervisor: spawn_persona_service failed \
                     before task spawn (prime round-trip). No service loop \
                     started; persona's registry entry is unattended."
                );
                summary.failures.push(BootSlotFailure {
                    slot_index: slot_idx,
                    role: Some(role),
                    persona_id: Some(persona_id),
                    reason: format!("spawn_persona_service failed: {reason}"),
                });
                return;
            }
        };
        match self.registry.attach_service_loop(persona_id, handle).await {
            Ok(()) => {
                summary.hosted += 1;
                tracing::info!(
                    persona_id = %persona_id,
                    agent_name = %agent_name,
                    role = ?role,
                    slot = slot_idx,
                    "🌐 The Grid hosts citizen {} (slot {}, role {:?}) — substrate \
                     service-loop attached",
                    agent_name,
                    slot_idx,
                    role,
                );
            }
            Err((returned_handle, reason)) => {
                returned_handle.abort();
                let _ = returned_handle.await;
                tracing::error!(
                    slot = slot_idx,
                    persona_id = %persona_id,
                    reason = reason,
                    "PersonaSpawnSupervisor: attach_service_loop failed; \
                     spawned task drained. Persona registered but unattended — \
                     fire `persona/spawn` to retry."
                );
                summary.failures.push(BootSlotFailure {
                    slot_index: slot_idx,
                    role: Some(role),
                    persona_id: Some(persona_id),
                    reason: format!("attach_service_loop failed: {reason}"),
                });
            }
        }
    }
}

/// Pull (slot_index, role) out of a [`SupervisorError`] in one
/// place — the error enum's variants both carry these fields but
/// behind different names. Centralizing the extraction keeps the
/// summary-construction site clean.
fn supervisor_error_facts(err: &SupervisorError) -> (Option<usize>, RoleId) {
    match err {
        SupervisorError::Profile {
            slot_index, role, ..
        }
        | SupervisorError::AdapterFactory {
            slot_index, role, ..
        }
        | SupervisorError::AdapterWarmup {
            slot_index, role, ..
        }
        | SupervisorError::RuntimeMissing {
            slot_index, role, ..
        } => (Some(*slot_index), *role),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the hosting reconciler (#429) runs host_unattended on
    // EVERY serving-plan edge for the life of the process. With nothing
    // unattended it must be a QUIET no-op — empty summary, zero failure rows
    // (a spurious row here would warn-log every ~5s forever), and the adapter
    // factory must never be consulted (a factory call on an empty scan would
    // probe /v1 each tick for nothing). Hosting an actual newborn end-to-end
    // needs a live airc daemon (same limit the registry's own tests document
    // in clone_shares_roster); the scan + early-return contract is the
    // unit-pinnable half.
    #[tokio::test]
    async fn host_unattended_with_nothing_unattended_is_a_quiet_noop() {
        struct NeverConsultedFactory;
        #[async_trait::async_trait]
        impl PersonaAdapterFactory for NeverConsultedFactory {
            async fn build_adapter(
                &self,
                _profile: &crate::persona::inference_profile::PersonaInferenceProfile,
            ) -> Result<Arc<dyn crate::ai::adapter::AIProviderAdapter>, String> {
                panic!("factory must not be consulted when the registry has no unattended citizens");
            }
        }

        let registry = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::new();
        let tmp = std::env::temp_dir().join("host-unattended-noop-test");
        let instance_manager = Arc::new(
            crate::modules::persona_instance_manager::PersonaInstanceManagerModule::new(
                registry,
                tmp.join("daemon.sock"),
                tmp,
            ),
        );
        // Idempotent — safe under full-suite parallelism (the singleton's own
        // doc: "subsequent init_global calls are no-ops").
        let model_registry =
            crate::model_registry::init_global().expect("model registry init for test");
        let supervisor = PersonaSpawnSupervisor::new(
            PersonaSpawnerModule::new(
                crate::cognition::model_resolver::types::HwCapabilityTier::CpuOnly,
                crate::persona::hw_tier_descriptor::HwTierCategory::Compat,
            ),
            instance_manager,
            Arc::new(NeverConsultedFactory),
            "test-tier",
            model_registry,
            tokio::runtime::Handle::current(),
        );

        let summary = supervisor.host_unattended(None).await;
        assert_eq!(summary.hosted, 0, "nothing to host → nothing hosted");
        assert!(
            summary.failures.is_empty(),
            "an empty scan must not synthesize failure rows: {:?}",
            summary.failures
        );
    }

    #[test]
    fn boot_summary_attempted_sums_hosted_and_failures() {
        let mut s = BootSummary::default();
        s.hosted = 3;
        s.failures.push(BootSlotFailure {
            slot_index: 1,
            role: Some(RoleId::Helper),
            persona_id: None,
            reason: "test".into(),
        });
        s.failures.push(BootSlotFailure {
            slot_index: 2,
            role: Some(RoleId::Coder),
            persona_id: None,
            reason: "test".into(),
        });
        assert_eq!(s.attempted(), 5);
        assert_eq!(s.failed(), 2);
    }

    #[test]
    fn boot_summary_serde_camel_case() {
        let s = BootSummary {
            hosted: 1,
            failures: vec![BootSlotFailure {
                slot_index: 0,
                role: Some(RoleId::Helper),
                persona_id: None,
                reason: "demo".into(),
            }],
        };
        let json = serde_json::to_string(&s).expect("serialize");
        assert!(json.contains("\"hosted\":1"));
        assert!(json.contains("\"slotIndex\":0"));
    }
}
