//! ModuleRegistry — DashMap-based command routing + typed module discovery.
//!
//! Replaces the 55-arm match statement in ipc/mod.rs with dynamic routing.
//! `register(module)` auto-wires commands from the module's config.
//! Like CBAR's appendAnalyzer() — register once, everything routes automatically.
//!
//! Thread-safe: uses DashMap and RwLock for interior mutability.
//! Can be shared via Arc across threads.

use super::module_metrics::ModuleMetrics;
use super::service_module::{ModuleConfig, ModulePriority, ServiceModule};
use dashmap::DashMap;
use parking_lot::RwLock;
use std::any::TypeId;
use std::sync::Arc;

pub struct ModuleRegistry {
    /// Modules by name: "voice" -> Arc<dyn ServiceModule>
    modules: DashMap<&'static str, Arc<dyn ServiceModule>>,

    /// Module configs cached for quick access
    configs: DashMap<String, ModuleConfig>,

    /// Metrics per module
    metrics: DashMap<String, Arc<ModuleMetrics>>,

    /// Command prefix -> module name routing table.
    /// Sorted by prefix length descending for longest-match-first routing.
    /// RwLock because registration mutates (rare), routing reads (frequent).
    command_routes: RwLock<Vec<(&'static str, &'static str)>>,

    /// TypeId -> module name for typed discovery.
    type_routes: DashMap<TypeId, &'static str>,

    /// command name -> self-routing command object. The typed-path map: a
    /// migrated command routes DIRECTLY here (O(1), no prefix scan, no per-module
    /// match arm), and the executor consults this BEFORE the prefix table so the
    /// typed path wins. Populated from each module's
    /// [`ServiceModule::commands`](super::service_module::ServiceModule::commands)
    /// at `register()`. See docs/architecture/COMMAND-ORGANIZATION.md.
    command_objects: DashMap<&'static str, Arc<dyn crate::sdk_codegen::DynCommand>>,
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleRegistry {
    pub fn new() -> Self {
        let registry = Self {
            modules: DashMap::new(),
            configs: DashMap::new(),
            metrics: DashMap::new(),
            command_routes: RwLock::new(Vec::new()),
            type_routes: DashMap::new(),
            command_objects: DashMap::new(),
        };
        // Seed the typed object map with every self-registering STATELESS command
        // (zero host-module ceremony — see register_stateless_command!). Dep-holding
        // commands are added later via module.commands() in register().
        for cmd in crate::sdk_codegen::stateless_command_objects() {
            let name = cmd.name();
            if let Some(prev) = registry.command_objects.insert(name, cmd) {
                panic!(
                    "ModuleRegistry: duplicate stateless command object '{}' (prev '{}'). \
                     Command names must be unique across the whole registry.",
                    name,
                    prev.name()
                );
            }
        }
        registry
    }

    /// Register a module. Auto-wires command routing from its config.
    /// Like CBAR's appendAnalyzer() — one call, everything wired.
    /// Thread-safe via interior mutability.
    pub fn register(&self, module: Arc<dyn ServiceModule>) {
        let config = module.config();
        let name = config.name;

        // Register by name
        self.modules.insert(name, module.clone());

        // Cache config for quick access
        self.configs.insert(name.to_string(), config.clone());

        // Create metrics tracker for this module
        self.metrics
            .insert(name.to_string(), Arc::new(ModuleMetrics::new(name)));

        // Build command routing table from declared prefixes
        {
            let mut routes = self.command_routes.write();
            for prefix in config.command_prefixes {
                routes.push((prefix, name));
            }
            // Sort by prefix length descending (longest match first)
            routes.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        }

        // Register the module's self-routing command objects (the typed path).
        // Each captures the module's deps; the executor routes a command name to
        // the object DIRECTLY, ahead of the prefix table. A duplicate name across
        // modules is a hard error — the "no central list" design removes the human
        // backstop, so the map must catch the collision itself (mirrors
        // sdk_codegen::command_registry's duplicate-name panic).
        for cmd in module.commands() {
            let cmd_name = cmd.name();
            if let Some(existing) = self.command_objects.insert(cmd_name, cmd) {
                panic!(
                    "ModuleRegistry: duplicate command object '{}' — module '{}' \
                     claims a name already registered (prev descriptor: '{}'). \
                     Command names must be unique across the whole registry.",
                    cmd_name,
                    name,
                    existing.name()
                );
            }
        }

        // Register type for downcast discovery
        let type_id = (*module).as_any().type_id();
        self.type_routes.insert(type_id, name);
    }

    /// Dispatch-parity audit — the structural kill for the registered-but-
    /// undispatchable class (#309's final layer, found live 2026-08-03): every
    /// command DESCRIPTOR (what `commands/help`, did-you-mean suggestions, and the
    /// persona tool offer all advertise) must be DISPATCHABLE — either a typed
    /// command object, or a module prefix route. `register_command!` (descriptors)
    /// and `commands()` (dispatch) are two per-module lists a human must keep in
    /// sync; `work/list` shipped advertised-but-unroutable for a night because
    /// nothing checked. Benchy's first-ever live native tool call was refused with
    /// a did-you-mean listing the refused name ITSELF — the two registries
    /// disagreeing in one sentence. Returns the orphan names; the boot path logs
    /// them as ERRORs so the gap is loud on the very first startup that ships it.
    /// `Provided` commands are EXCLUDED, and that is not a loophole — it is what the
    /// shape means. [`WireShape::Provided`](crate::sdk_codegen::WireShape::Provided): "the
    /// substrate CANNOT execute it; it routes the call OUT to a client adapter... a
    /// different *server* (adapter, not ServiceModule)". Such a command has no module, so
    /// it can be in neither `command_objects` nor a prefix route, and the remedy this audit
    /// advertises — "add it to its module's `commands()` vec" — is IMPOSSIBLE to apply
    /// (`interface/mod.rs` has no `commands()` at all). Their routability is the
    /// [`ProviderRegistry`](crate::runtime::ProviderRegistry)'s concern, checked at call
    /// time by `Runtime::route_command`, whose no-provider path already fails LOUD naming
    /// the command AND the missing adapter (pinned by
    /// `provided_command_fails_loud_without_a_provider`).
    ///
    /// Counting them was a false positive by construction, and an expensive one: this ERROR
    /// fired on EVERY boot listing three "orphans" of which two — `perception/observe`,
    /// `interface/screenshot` — were healthy adapter-served commands. An alarm that mostly
    /// cries wolf trains its readers to skip it, which is how the ONE real orphan in that
    /// list went unnoticed. Worse, the stated hardening above ("promote to a boot refusal
    /// once the count holds at zero") could never ship while a structurally-unroutable
    /// class was counted — and promoting it anyway would have refused boot on a legitimate
    /// Provided command.
    pub fn dispatch_orphans(&self) -> Vec<&'static str> {
        crate::sdk_codegen::command_registry()
            .into_iter()
            .filter(|d| d.wire != crate::sdk_codegen::WireShape::Provided)
            .map(|d| d.name)
            .filter(|name| {
                !self.command_objects.contains_key(name) && self.route_command(name).is_none()
            })
            .collect()
    }

    /// Route a command to the correct module.
    /// Returns (module, full_command) — the module receives the full command string.
    /// Replaces the 55-arm match statement.
    pub fn route_command(&self, command: &str) -> Option<(Arc<dyn ServiceModule>, String)> {
        let routes = self.command_routes.read();
        for &(prefix, module_name) in routes.iter() {
            if command.starts_with(prefix) {
                return self
                    .modules
                    .get(module_name)
                    .map(|module| (module.clone(), command.to_string()));
            }
        }
        None
    }

    /// Route a command name to its self-routing [`DynCommand`] object, if one is
    /// registered (the typed path). O(1) lock-free read of an after-boot-immutable
    /// map — the executor consults this BEFORE the prefix table so a migrated
    /// command wins over its module's legacy `handle_command` arm.
    pub fn route_object(&self, command: &str) -> Option<Arc<dyn crate::sdk_codegen::DynCommand>> {
        self.command_objects.get(command).map(|e| e.value().clone())
    }

    /// List all registered command-object names (debugging / health-check /
    /// migration tracking — what's on the typed path vs still prefix-routed).
    pub fn list_command_objects(&self) -> Vec<&'static str> {
        self.command_objects.iter().map(|e| *e.key()).collect()
    }

    /// Get module by name.
    pub fn get_by_name(&self, name: &str) -> Option<Arc<dyn ServiceModule>> {
        self.modules.get(name).map(|m| m.clone())
    }

    /// Typed module discovery — like CBAR's getAnalyzerOfType<T>().
    ///
    /// Returns the module as a trait object. Caller can downcast via as_any():
    /// ```ignore
    /// if let Some(module) = registry.module_of_type::<VoiceModule>() {
    ///     let voice = module.as_any().downcast_ref::<VoiceModule>().unwrap();
    /// }
    /// ```
    pub fn module_of_type<T: ServiceModule + 'static>(&self) -> Option<Arc<dyn ServiceModule>> {
        let type_id = TypeId::of::<T>();
        self.type_routes
            .get(&type_id)
            .and_then(|name| self.modules.get(*name).map(|m| m.clone()))
    }

    /// List all registered module names.
    pub fn list_modules(&self) -> Vec<&'static str> {
        self.modules.iter().map(|e| *e.key()).collect()
    }

    /// List all registered command routes (for debugging/health-check).
    pub fn list_routes(&self) -> Vec<(&'static str, &'static str)> {
        self.command_routes.read().clone()
    }

    // ─── Helper methods for RuntimeControl ───────────────────────────────────────

    /// Check if a module exists by name.
    pub fn has_module(&self, name: &str) -> bool {
        self.modules.contains_key(name)
    }

    /// Get module priority by name.
    pub fn get_priority(&self, name: &str) -> Option<ModulePriority> {
        self.configs.get(name).map(|c| c.priority)
    }

    /// Get module config by name.
    pub fn get_config(&self, name: &str) -> Option<ModuleConfig> {
        self.configs.get(name).map(|c| c.clone())
    }

    /// Get module metrics by name.
    pub fn get_metrics(&self, name: &str) -> Option<Arc<ModuleMetrics>> {
        self.metrics.get(name).map(|m| m.clone())
    }

    /// List all module names (owned strings for cross-thread safety).
    pub fn module_names(&self) -> Vec<String> {
        self.modules.iter().map(|e| e.key().to_string()).collect()
    }

    /// Install the substrate-wide `CommandExecutor` into every registered
    /// module. Modules opt in by overriding `ServiceModule::install_executor`;
    /// the default impl is a no-op so this call is cheap for modules that
    /// don't dispatch commands.
    ///
    /// Called once by `start_server` after the executor is built. Replaces
    /// the deleted `GLOBAL_EXECUTOR` + `executor()` panic accessor pattern
    /// (task #224). Pure dependency injection — no global state, no boot-
    /// order racing.
    pub fn install_executor_on_all(&self, executor: Arc<super::command_executor::CommandExecutor>) {
        for entry in self.modules.iter() {
            entry.value().install_executor(Arc::clone(&executor));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::service_module::*;
    use super::super::ModuleContext;
    use super::*;
    use serde_json::Value;
    use std::any::Any;

    struct TestModule {
        name: &'static str,
        prefixes: &'static [&'static str],
    }

    #[async_trait::async_trait]
    impl ServiceModule for TestModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: self.name,
                priority: ModulePriority::Normal,
                command_prefixes: self.prefixes,
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }

        async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
            Ok(())
        }

        async fn handle_command(
            &self,
            command: &str,
            _params: Value,
        ) -> Result<CommandResult, String> {
            Ok(CommandResult::Json(serde_json::json!({
                "module": self.name,
                "command": command,
            })))
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[test]
    fn test_register_and_route() {
        let registry = ModuleRegistry::new();

        registry.register(Arc::new(TestModule {
            name: "voice",
            prefixes: &["voice/"],
        }));
        registry.register(Arc::new(TestModule {
            name: "code",
            prefixes: &["code/"],
        }));
        registry.register(Arc::new(TestModule {
            name: "health",
            prefixes: &["health-", "get-"],
        }));

        // Exact prefix matching
        assert!(registry.route_command("voice/synthesize").is_some());
        assert!(registry.route_command("code/read").is_some());
        assert!(registry.route_command("health-check").is_some());
        assert!(registry.route_command("get-stats").is_some());

        // No match
        assert!(registry.route_command("unknown/command").is_none());
        assert!(registry.route_command("").is_none());
    }

    #[test]
    fn test_longest_prefix_wins() {
        let registry = ModuleRegistry::new();

        registry.register(Arc::new(TestModule {
            name: "code",
            prefixes: &["code/"],
        }));
        registry.register(Arc::new(TestModule {
            name: "code-shell",
            prefixes: &["code/shell-"],
        }));

        // "code/shell-create" should route to code-shell (longer prefix)
        let (module, _) = registry.route_command("code/shell-create").unwrap();
        assert_eq!(module.config().name, "code-shell");

        // "code/read" should route to code (shorter prefix)
        let (module, _) = registry.route_command("code/read").unwrap();
        assert_eq!(module.config().name, "code");
    }

    #[test]
    fn test_list_modules() {
        let registry = ModuleRegistry::new();
        registry.register(Arc::new(TestModule {
            name: "voice",
            prefixes: &["voice/"],
        }));
        registry.register(Arc::new(TestModule {
            name: "code",
            prefixes: &["code/"],
        }));

        let mut modules = registry.list_modules();
        modules.sort();
        assert_eq!(modules, vec!["code", "voice"]);
    }

    #[test]
    fn test_typed_discovery() {
        let registry = ModuleRegistry::new();
        registry.register(Arc::new(TestModule {
            name: "voice",
            prefixes: &["voice/"],
        }));

        // Can find by type
        let found = registry.module_of_type::<TestModule>();
        assert!(found.is_some());
    }

    // what this catches: the registered-but-undispatchable class (#309 final layer —
    // work/list shipped advertised in help/suggestions/the tool offer while dispatch
    // refused it, live 2026-08-03). dispatch_orphans must read BOTH sides: with no
    // module registered, every dep-holding descriptor is an orphan (proves it reads
    // the descriptor registry, and that stateless self-registering commands are
    // exempt by construction); registering the work module clears ALL SEVEN work
    // verbs (proves objects clear orphans — and pins that commands() carries every
    // verb the descriptors advertise, the exact two-line gap that shipped).
    // what this catches: the audit counting a `Provided` command as undispatchable. Such a
    // command has NO ServiceModule by construction — an adapter is its server — so it can
    // never appear in `command_objects` or a prefix route, and no amount of module
    // registration will ever clear it. Counting it made the boot ERROR permanently noisy
    // (3 orphans, 2 of them healthy adapter-served commands) AND made the documented
    // promotion to a boot refusal unreachable, since the count could not reach zero.
    //
    // Asserted on a registry with NOTHING registered — the worst case for the audit, where
    // every module-served command IS an orphan. If a Provided name survives even there, the
    // filter is gone. Reverting the `wire != Provided` filter puts interface/screenshot back
    // in the list and this goes red.
    #[test]
    fn a_provided_command_is_never_an_orphan_because_it_has_no_module_to_be_missing_from() {
        let registry = ModuleRegistry::new();
        let orphans = registry.dispatch_orphans();

        // Every Provided descriptor in the registry, by shape — not a hand-kept name list,
        // so a newly-added adapter command inherits the guarantee.
        let provided: Vec<&'static str> = crate::sdk_codegen::command_registry()
            .into_iter()
            .filter(|d| d.wire == crate::sdk_codegen::WireShape::Provided)
            .map(|d| d.name)
            .collect();
        assert!(
            !provided.is_empty(),
            "positive control: the registry must contain at least one Provided command \
             (interface/screenshot), or this test proves nothing"
        );
        for name in provided {
            assert!(
                !orphans.contains(&name),
                "{name} is adapter-served (WireShape::Provided) — it has no module to be \
                 missing from, so the dispatch audit must not report it. orphans: {orphans:?}"
            );
        }
    }

    #[test]
    fn dispatch_orphans_reads_descriptors_and_clears_on_module_registration() {
        let registry = ModuleRegistry::new();
        let before = registry.dispatch_orphans();
        let work_names = [
            "work/list",
            "work/get",
            "work/claim",
            "work/create",
            "work/release",
            "work/state",
            "work/heartbeat",
        ];
        for name in work_names {
            assert!(
                before.contains(&name),
                "{name} must be an orphan before its module registers"
            );
        }

        registry.register(std::sync::Arc::new(crate::modules::work::WorkModule::new(
            crate::persona::PersonaAircRuntimeRegistry::new(),
        )));
        registry.register(std::sync::Arc::new(crate::modules::room::RoomModule::new(crate::persona::PersonaAircRuntimeRegistry::new(),)));
        // Event-driven SWE grade-on-done: subscribes to work.card.state_changed (emitted
        // by work/state) and grades a finished bench SWE card's workspace against the
        // held-out oracle. No commands, no tick — pure event subscriber.
        registry.register(std::sync::Arc::new(
            crate::modules::benchmark_grade::BenchmarkGradeModule::new(
                crate::persona::PersonaAircRuntimeRegistry::new(),
            ),
        ));
        let after = registry.dispatch_orphans();
        for name in work_names {
            assert!(
                !after.contains(&name),
                "{name} still orphaned after WorkModule registered — a verb is \
                 missing from commands() (the #309 two-line gap)"
            );
        }
    }

    // ─── Module-wiring audit (#344) ────────────────────────────────────
    //
    // `dispatch_orphans()` above audits DESCRIPTOR → dispatch. It cannot see
    // the layer under it: a module that ships NO descriptors at all. `impl
    // ServiceModule for X` registers NOTHING — the MODULE itself must be
    // handed to `register()` at a boot site (`runtime/registry.rs` +
    // `ipc/mod.rs`, cf. `RoomModule`). Nothing in the trait says so, and
    // `ProbeStreamModule` — 675 lines, a handle store, a broadcast
    // subscriber, its own green test mod, `command_prefixes:
    // &["debug/probes/"]` — has been unreachable since the day it landed
    // (#362). Compile, unit tests, and the task list all said done.
    //
    // The predicate here is deliberately NOT "is the name mentioned
    // anywhere". That version passes `ProbeStreamModule`, because the only
    // mention outside its own file is a DOC COMMENT in `probe_query.rs`.
    // Prose made a dead module look wired — the precise failure this audit
    // exists to catch. Comments are stripped before the search.

    /// A module that is intentionally not registered, with the reason. An
    /// entry here is a DECLARATION, not a mute: unregistered-and-undeclared
    /// fails the test, and a declared entry that becomes registered ALSO
    /// fails, so the list cannot rot into a graveyard.
    struct Unwired {
        module: &'static str,
        why: &'static str,
    }

    /// Categories, in the order they were derived from the live tree:
    ///
    /// - **fixture** — exists only to drive a test. Production binaries
    ///   never construct it.
    /// - **staging** — a real module deliberately landed before its wiring
    ///   slice. MUST name the task that wires it. This is the category the
    ///   audit was worth building for: staging is indistinguishable from a
    ///   defect unless it is *declared*.
    /// - **shadowed** — superseded by another module that serves the same
    ///   surface; dead code, not a live break.
    /// - **defect** — known-broken, with the task tracking the fix. Loud on
    ///   purpose: the guard refuses to let it be forgotten.
    const UNWIRED: &[Unwired] = &[
        Unwired { module: "BarrierModule", why: "fixture: barrier for the command-executor concurrency test" },
        Unwired { module: "DefaultsModule", why: "fixture: exercises ModuleConfig trait defaults" },
        Unwired { module: "FaultRecorder", why: "fixture: records genome page-faults in residency tests" },
        Unwired { module: "GreeterModule", why: "fixture: the module_harness worked example" },
        Unwired { module: "InferenceRecorder", why: "fixture: captures llm_module_service calls" },
        Unwired { module: "OptedInModule", why: "fixture: sibling of DefaultsModule, opts into every hook" },
        Unwired { module: "PageFaultOnly", why: "fixture: genome bus subscriber asserting fault-only delivery" },
        Unwired { module: "ReadyModule", why: "fixture: runtime readiness-gate test" },
        Unwired { module: "RecorderModule", why: "fixture: genome local_manager call recorder" },
        Unwired { module: "StubAircModule", why: "fixture: ChatModule's airc stand-in" },
        Unwired { module: "StubDataModule", why: "fixture: ChatModule's data stand-in" },
        Unwired { module: "TestModule", why: "fixture: this file's own routing tests" },
        Unwired {
            module: "HippocampusModule",
            why: "staging: BrainRegion skeleton, command_prefixes is empty by \
                  design until slice L0-3a.1b migrates memory/* over from \
                  MemoryModule. Registering it today would add a tick that \
                  does nothing.",
        },
        Unwired {
            module: "PersonaServiceModule",
            why: "shadowed: claims command_prefixes \"persona/\", but \
                  PersonaAllocatorModule claims the SAME prefix and IS \
                  registered (ipc/mod.rs). The surface is served; this is \
                  dead code, not a live break.",
        },
        // ProbeStreamModule's DEFECT entry (#362) lived here for exactly one
        // commit — the fix registers it in ipc/mod.rs against the router
        // installed by install_probe_tracing, and this audit's staleness
        // check is what forced the entry's removal in the same change.
    ];

    fn crate_src_files() -> Vec<(std::path::PathBuf, String)> {
        fn walk(dir: &std::path::Path, out: &mut Vec<(std::path::PathBuf, String)>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    if let Ok(text) = std::fs::read_to_string(&path) {
                        out.push((path, text));
                    }
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out = Vec::new();
        walk(&root, &mut out);
        out
    }

    /// Drop `//`-prefixed content so a doc comment can never stand in for
    /// real wiring. Crude on purpose — a `//` inside a string literal only
    /// ever costs us a false "wired" reading, never a false alarm.
    fn code_only(src: &str) -> String {
        src.lines()
            .map(|line| match line.find("//") {
                Some(idx) => &line[..idx],
                None => line,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn mentions_ident(haystack: &str, ident: &str) -> bool {
        let bytes = haystack.as_bytes();
        haystack.match_indices(ident).any(|(idx, _)| {
            let before_ok = idx == 0 || {
                let c = bytes[idx - 1] as char;
                !c.is_alphanumeric() && c != '_'
            };
            let end = idx + ident.len();
            let after_ok = end >= bytes.len() || {
                let c = bytes[end] as char;
                !c.is_alphanumeric() && c != '_'
            };
            before_ok && after_ok
        })
    }

    #[test]
    // what this catches: a ServiceModule that is implemented, tested and
    // green but handed to `register()` nowhere — so its handle_command,
    // commands() and tick never reach the runtime. Regression for #362
    // (ProbeStreamModule) and #325 (GeneratorModule, which shipped
    // unregistered and was found only by live dispatch).
    fn every_service_module_is_registered_or_declares_why_not() {
        let files = crate_src_files();
        assert!(
            files.len() > 100,
            "source scan found only {} files — the walk is broken, and a \
             guard that cannot see the tree would pass vacuously",
            files.len()
        );

        let code: Vec<(std::path::PathBuf, String)> = files
            .iter()
            .map(|(p, t)| (p.clone(), code_only(t)))
            .collect();

        // (module name, defining file) for every `impl ServiceModule for X`.
        const NEEDLE: &str = "impl ServiceModule for ";
        let mut impls: Vec<(String, std::path::PathBuf)> = Vec::new();
        for (path, text) in &code {
            for (idx, _) in text.match_indices(NEEDLE) {
                let rest = &text[idx + NEEDLE.len()..];
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    impls.push((name, path.clone()));
                }
            }
        }
        assert!(
            impls.len() > 50,
            "found only {} ServiceModule impls — the scan regressed",
            impls.len()
        );

        // THIS file is excluded from the haystack. The UNWIRED table below
        // names every declared module as a string literal, and a literal is
        // code, not a comment — so without this exclusion each declaration
        // would read as a reference, every entry would look wired, and the
        // staleness check would fire on the whole list. The guard would have
        // been its own first false positive.
        let this_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("runtime")
            .join("registry.rs");

        let unwired: Vec<String> = impls
            .iter()
            .filter(|(name, home)| {
                !code
                    .iter()
                    .any(|(p, c)| p != home && *p != this_file && mentions_ident(c, name))
            })
            .map(|(name, _)| name.clone())
            .collect();

        let declared: std::collections::HashSet<&str> =
            UNWIRED.iter().map(|u| u.module).collect();

        let undeclared: Vec<&String> =
            unwired.iter().filter(|n| !declared.contains(n.as_str())).collect();
        assert!(
            undeclared.is_empty(),
            "these ServiceModules are implemented but never registered, and \
             nothing declares why:\n  {}\n\nA module reaches dispatch ONLY if \
             the MODULE is handed to register() at a boot site (ipc/mod.rs, \
             cf. RoomModule) — `impl ServiceModule` registers nothing. Either \
             wire it, or add an UNWIRED entry in this file naming the \
             category and the task.",
            undeclared
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n  ")
        );

        let stale: Vec<&str> = UNWIRED
            .iter()
            .map(|u| u.module)
            .filter(|m| !unwired.iter().any(|n| n == m))
            .collect();
        assert!(
            stale.is_empty(),
            "these modules are declared unwired but ARE now referenced \
             outside their own file — delete their UNWIRED entry so the list \
             stays honest:\n  {}",
            stale.join("\n  ")
        );
    }

    #[test]
    // what this catches: an UNWIRED entry added as a silent mute. Every
    // exemption must carry a reason, and the two categories that are not
    // self-evidently harmless (staging, defect) must name the task that
    // resolves them — the requirement the #344 audit found most valuable.
    fn every_unwired_declaration_carries_a_reason() {
        for entry in UNWIRED {
            assert!(
                entry.why.len() > 20,
                "{}: UNWIRED entries need a real reason, not a placeholder",
                entry.module
            );
            let category = entry
                .why
                .split(':')
                .next()
                .expect("split always yields one element")
                .trim()
                .to_ascii_lowercase();
            assert!(
                ["fixture", "staging", "shadowed", "defect"]
                    .iter()
                    .any(|c| category.starts_with(c)),
                "{}: reason must start with one of fixture/staging/shadowed/\
                 defect — got {category:?}",
                entry.module
            );
            if category.starts_with("staging") || category.starts_with("defect") {
                assert!(
                    entry.why.contains('#') || entry.why.contains("slice"),
                    "{}: a {category} entry must name the task or slice that \
                     resolves it — otherwise it is indistinguishable from a \
                     forgotten defect",
                    entry.module
                );
            }
        }
    }
}
