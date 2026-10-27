//! The `ServiceModule` wiring of the persona instance manager
//! ([`crate::persona::instance_manager`]): the `persona/instances/*` command surface and
//! the module lifecycle. The manager itself (registry, birth, instance info) is the
//! persona's.

use continuum_base::time::EpochMs;
use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::persona::instance_manager::PersonaInstanceManagerModule;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};

/// The `ServiceModule` face of the mind's [`PersonaInstanceManagerModule`]: its command
/// surface and lifecycle are the surface's, the manager is the persona's. `as_any` is the
/// inner manager.
pub struct PersonaInstanceManagerService(pub Arc<PersonaInstanceManagerModule>);

#[async_trait]
impl ServiceModule for PersonaInstanceManagerService {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "persona_instance_manager",
            priority: ModulePriority::Normal,
            command_prefixes: &["persona/instances/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Capture the event bus so every completed birth (boot auto-seed OR the
        // `persona/spawn` command) can announce itself as `persona:born`. Shared with
        // `PersonaBirth` via the same `Arc<LateBound>`.
        self.0.bus.install(ctx.bus.clone());
        // Durable dormancy loads before the hosting reconciler is released (executor
        // release follows every module's initialize), so a dormant home is never drawn.
        // A ledger failure DEGRADES (resume blocked, spawn served, probed) and never fails
        // this module's init: the manager's verbs keep serving.
        let adapter = match ctx.storage("persona wakefulness") {
            Ok(storage) => storage.adapter("main").await,
            Err(e) => Err(e),
        };
        if let crate::persona::instance_manager::DormancyBoot::Loaded { dormant } = self.0.load_dormancy(adapter).await {
            continuum_base::probe!(
                class = "persona.dormancy.loaded",
                dormant = dormant as u64,
                "durable wakefulness loaded: dormant homes are not resumed until woken"
            );
        }
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            // RETIRED: `persona/instances/bootstrap` is superseded by the typed,
            // handle-based `persona/spawn` (#200), which births over the SAME
            // `PersonaBirth` core (no parallel spawn implementation). Fail loud with
            // the pointer rather than keep a redundant second birth surface.
            "persona/instances/bootstrap" => {
                let _ = params;
                Err(
                    "persona/instances/bootstrap is retired — use `persona/spawn` \
                     (typed, handle-based, params: name?/count?; births over the same core)"
                        .to_string(),
                )
            }

            // list + get migrated onto the typed DynCommand registry (#62):
            // `commands/persona/instances/{list,get}.rs`, contributed via
            // `commands()` below. They route through `route_object`, not here.
            // Reaching this arm for them means the typed path failed to register —
            // fail loud naming the cause rather than silently re-handling.
            "persona/instances/list" | "persona/instances/get" => Err(format!(
                "'{command}' is migrated to the typed registry \
                 (commands/persona/instances/) — it must route via route_object, \
                 not the legacy handle_command path"
            )),

            _ => Err(format!("unknown persona/instances command: {command}")),
        }
    }

    /// Contribute the dep-holding `persona/*` typed commands to the kernel's object
    /// map: the `instances/*` roster verbs (`list` + `get` + `despawn`, all sharing
    /// this module's live `PersonaAircRuntimeRegistry`), plus `persona/reassign-model`
    /// — which gets this module's `continuum_root` (to resolve persona homes) and a
    /// shared handle to the late-bound `executor` (to compose `serving/pin`).
    /// (bootstrap stays a legacy arm under task #62 until its full bootstrap
    /// capability — socket, room, executor — is threaded here.)
    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        crate::commands::persona::command_objects(
            self.0.registry.clone(),
            Arc::clone(&self.0.dormancy),
            self.0.continuum_root.clone(),
            Arc::clone(&self.0.executor),
            self.0.birth(),
        )
    }

    /// THE SEAM RENEWAL. A lease is renewed only by cognition, and a reboot's build window
    /// has none: every held card lapsed across each of the eight deploys on 2026-09-13 and
    /// idle peers took them at boot. Before the core stops, every resident's held leases
    /// are renewed once (board-derived, a full TTL), so the holds outlive the dark window.
    /// Bounded well inside the runtime's 2 s save budget; a slow daemon is probed, not waited on.
    async fn save_state(&self) -> Result<(), String> {
        let runtimes: Vec<_> = self.0.registry.iter().collect();
        let work = async {
            let mut renewed = 0usize;
            let now_ms = EpochMs::now().millis();
            for rt in &runtimes {
                // ONE walk of the boards she stands in: everything she OWNS, whole. The seam
                // renewal takes the live holds from it; the handoff record takes the rest
                // (her cards in Review, the reviews she owes) — card 49b5e806, Kimi's #4.
                let owned =
                    match crate::persona::airc_runtime::scoped_board_owned_by(rt.airc().as_ref())
                        .await
                    {
                        Ok(owned) => owned,
                        Err(_) => continue,
                    };
                crate::cognition::handoff::note_owned(rt.airc().peer_id().as_uuid(), &owned, now_ms);
                let held = crate::cognition::card_holder::held_of_owned(owned, now_ms);
                for (room, card) in held {
                    let Some(claim_id) = card.claim_id else {
                        continue;
                    };
                    if rt
                        .airc()
                        .heartbeat_work_claim_in(
                            &room,
                            airc_lib::HeartbeatWorkClaim {
                                card_id: card.card_id,
                                claim_id,
                                ttl_ms: crate::cognition::card_facts::DEFAULT_CLAIM_TTL_MS,
                            },
                        )
                        .await
                        .is_ok()
                    {
                        renewed += 1;
                    }
                }
            }
            renewed
        };
        match tokio::time::timeout(std::time::Duration::from_millis(1500), work).await {
            Ok(renewed) => continuum_base::probe!(
                class = "persona.claim.renewed_for_seam",
                residents = runtimes.len() as u64,
                renewed = renewed as u64,
                "held leases renewed before the core stops — the holds outlive the build window"
            ),
            Err(_) => continuum_base::probe!(
                class = "persona.claim.seam_renewal_timed_out",
                residents = runtimes.len() as u64,
                "seam renewal did not finish inside its bound — some holds may lapse across this stop"
            ),
        }
        Ok(())
    }

    fn install_executor(&self, executor: std::sync::Arc<crate::runtime::CommandExecutor>) {
        self.0.executor.install(executor);
    }

    fn as_any(&self) -> &dyn Any {
        self.0.as_ref()
    }
}

/// The authored bio for a role — the role_template `bio_template` verbatim
/// (`{name}` substituted by the caller). One source: the same templates the
/// spawner plans from, so the published airc bio and the role's self-concept
/// never drift. `Custom` has no authored template → a neutral one-liner (the
/// persona edits her own card from there; profile facets override upstream).

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::PersonaAircRuntimeRegistry;
    use std::path::PathBuf;

    #[test]
    fn module_config_routes_persona_instances() {
        let registry = PersonaAircRuntimeRegistry::new();
        let module = PersonaInstanceManagerService(Arc::new(PersonaInstanceManagerModule::new(
            registry,
            PathBuf::from("/nonexistent/socket"),
            PathBuf::from("/tmp/continuum-test"),
        )));
        let cfg = module.config();
        assert_eq!(cfg.name, "persona_instance_manager");
        assert_eq!(cfg.command_prefixes, &["persona/instances/"]);
    }

    // what this catches: list + get are migrated to the typed registry (#62), so
    // the legacy handle_command arms must FAIL LOUD naming the migration — never
    // silently re-handle. A regression that re-adds an inline handler (re-forking
    // the roster read away from the typed command) is caught here.
    #[tokio::test]
    async fn migrated_list_and_get_arms_fail_loud() {
        let module = PersonaInstanceManagerService(Arc::new(PersonaInstanceManagerModule::new(
            PersonaAircRuntimeRegistry::new(),
            PathBuf::from("/nonexistent/socket"),
            PathBuf::from("/tmp/continuum-test"),
        )));
        for command in ["persona/instances/list", "persona/instances/get"] {
            let err = module
                .handle_command(command, Value::Null)
                .await
                .expect_err("migrated arm must fail loud");
            assert!(err.contains("migrated"), "got {err}");
            assert!(err.contains(command), "got {err}");
        }
    }

    // what this catches: the module contributes the typed read/dealloc verbs
    // (list/get/despawn) to the kernel object map, all sharing its live registry.
    // A regression that drops the `commands()` override — leaving the persona
    // surface without the roster verbs — is caught.
    #[test]
    fn contributes_the_typed_instance_commands() {
        let module = PersonaInstanceManagerService(Arc::new(PersonaInstanceManagerModule::new(
            PersonaAircRuntimeRegistry::new(),
            PathBuf::from("/nonexistent/socket"),
            PathBuf::from("/tmp/continuum-test"),
        )));
        let names: Vec<&str> = module.commands().iter().map(|c| c.name()).collect();
        assert!(names.contains(&"persona/instances/list"), "got {names:?}");
        assert!(names.contains(&"persona/instances/get"), "got {names:?}");
        assert!(
            names.contains(&"persona/instances/despawn"),
            "got {names:?}"
        );
    }

    // what this catches: a wakefulness store that fails to open taking the whole manager
    // down (Cormac on #5509). Initialize succeeds, the ledger stays uninstalled (so the draw
    // resumes no home from disk), and spawn/despawn/wake are still contributed.
    #[tokio::test]
    async fn a_failed_ledger_degrades_and_the_manager_still_serves() {
        let module = PersonaInstanceManagerService(Arc::new(PersonaInstanceManagerModule::new(
            PersonaAircRuntimeRegistry::new(),
            PathBuf::from("/nonexistent/socket"),
            PathBuf::from("/tmp/continuum-test"),
        )));
        let boot = module.0.load_dormancy(Err("store down".to_string())).await;
        assert_eq!(
            boot,
            crate::persona::instance_manager::DormancyBoot::Unavailable { error: "store down".to_string() }
        );
        assert!(!module.0.dormancy.is_installed(), "no ledger: the draw fails closed");
        let names: Vec<&str> = module.commands().iter().map(|c| c.name()).collect();
        for verb in ["persona/spawn", "persona/instances/despawn", "persona/instances/wake"] {
            assert!(names.contains(&verb), "{verb} still served: {names:?}");
        }
    }

    #[tokio::test]
    async fn unknown_command_errors() {
        let registry = PersonaAircRuntimeRegistry::new();
        let module = PersonaInstanceManagerService(Arc::new(PersonaInstanceManagerModule::new(
            registry,
            PathBuf::from("/nonexistent/socket"),
            PathBuf::from("/tmp/continuum-test"),
        )));
        let res = module
            .handle_command("persona/instances/teleport", Value::Null)
            .await;
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("unknown"));
    }
}
