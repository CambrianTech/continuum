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
    /// Background acceptance, execution and terminal persistence share custody
    /// with this runtime's module registry, including across executor instances.
    pub(crate) task_admission: Arc<super::admission_gate::AdmissionGate>,
    /// Modules by name: "voice" -> Arc<dyn ServiceModule>
    modules: DashMap<&'static str, Arc<dyn ServiceModule>>,

    /// Module configs cached for quick access
    configs: DashMap<String, ModuleConfig>,

    /// Metrics per module
    metrics: DashMap<String, Arc<ModuleMetrics>>,

    /// When the previous module was registered. Boot registers modules SERIALLY
    /// right after constructing each, so the gap between two registrations is
    /// the later module's construction cost — which is how a 14 s
    /// `construct_modules` phase (M5, 2026-09-05, boot.phase) gets attributed to
    /// the constructors that own it without touching every `X::new()` site.
    last_registered_at: parking_lot::Mutex<Option<std::time::Instant>>,

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
            task_admission: Arc::new(super::admission_gate::AdmissionGate::new()),
            modules: DashMap::new(),
            configs: DashMap::new(),
            metrics: DashMap::new(),
            last_registered_at: parking_lot::Mutex::new(None),
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

        // Attribute construction time: the gap since the previous registration is
        // this module's constructor (boot constructs and registers serially). The
        // first registration has no predecessor and reports 0.
        {
            let now = std::time::Instant::now();
            let mut last = self.last_registered_at.lock();
            let since_prev_ms = last
                .map(|t| now.duration_since(t).as_millis() as u64)
                .unwrap_or(0); // unwrap_or: no predecessor = nothing to attribute, 0 by definition
            *last = Some(now);
            continuum_base::probe!(
                class = "boot.construct",
                module = %name,
                since_prev_ms = since_prev_ms,
                "module constructed and registered"
            );
        }

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
            // A command is ONE thing declared in ONE file as a (descriptor, constructor)
            // pair. `register_stateless_command!` emits both; a dep-holding command splits
            // them — `register_command!(X)` at the declaration site for the descriptor
            // (type-only, `CommandDescriptor::of::<Spec>()`, so deps are irrelevant to it)
            // and this `commands()` vec for the constructor (which needs the module's
            // deps). Splitting is fine. Shipping only ONE HALF is not, and nothing used to
            // catch it: a constructor with no descriptor is routable but INVISIBLE — absent
            // from `commands/list`, from the persona tool surface, from the ACL, from
            // codegen. That is how `activity/spawn` — the verb that mints every room —
            // became unfindable while `commands/list` was promising "Listed == callable".
            // Six commands had shipped this way.
            //
            // `dispatch_orphans` audits the OTHER direction (descriptor, no route). This is
            // its mirror, and together they make the pair total. It panics rather than logs
            // for the same reason the duplicate-name check below does, and it is the same
            // class of defect: the "no central list" design removes the human backstop, so
            // the registry must catch its own wiring gaps. Boot is the right time — the
            // fact is knowable the instant the module registers, and the alternative is a
            // command that silently does not exist for anyone who has to discover it.
            if !crate::sdk_codegen::descriptor_exists(cmd_name) {
                panic!(
                    "ModuleRegistry: command object '{cmd_name}' (module '{name}') has NO \
                     DESCRIPTOR — it would route but stay invisible to commands/list, the \
                     persona tool surface, the ACL and codegen. Add `crate::register_command!\
                     (<Type>);` at its declaration site (descriptor is type-only, so it needs \
                     none of the module's deps — see BenchmarkDispatch for the pattern)."
                );
            }
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
    /// Every registered module's declared edges, for the dataflow-graph audit
    /// (`runtime::event_graph`). Consumption is the LIVE-delivered artifact
    /// subscriptions only; bus `event_subscriptions` ride a dispatch path with no
    /// production caller and are reported separately by the runtime.
    pub fn event_edges(&self) -> Vec<super::event_graph::ModuleEdges> {
        let mut edges: Vec<_> = self
            .modules
            .iter()
            .map(|entry| super::event_graph::ModuleEdges {
                module: *entry.key(),
                emits: entry.value().emissions(),
                consumes: entry.value().artifact_subscriptions(),
            })
            .collect();
        edges.sort_by_key(|e| e.module);
        edges
    }

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

    /// A module contributing a command CONSTRUCTOR whose name was never declared as
    /// a descriptor — the exact half-declared shape the boot guard exists to catch.
    struct UndeclaredCommandModule;

    struct UndeclaredCommand;

    /// A local command contract with named TS types: the descriptor below only has to be a
    /// real one, so the kernel's own fixture stands in for an upper module's command.
    #[derive(serde::Serialize, serde::Deserialize, ts_rs::TS)]
    struct StubParams {}
    #[derive(serde::Serialize, serde::Deserialize, ts_rs::TS)]
    struct StubResult {}
    struct StubSpec;
    impl crate::sdk_codegen::CommandSpec for StubSpec {
        const NAME: &'static str = "test/stub-descriptor";
        const ACCESS_LEVEL: crate::sdk_codegen::AccessLevel =
            crate::sdk_codegen::AccessLevel::AiSafe;
        const WIRE: crate::sdk_codegen::WireShape = crate::sdk_codegen::WireShape::Bare;
        type Params = StubParams;
        type Result = StubResult;
    }

    #[async_trait::async_trait]
    impl crate::sdk_codegen::DynCommand for UndeclaredCommand {
        fn name(&self) -> &'static str {
            // Deliberately a name no `register_command!` anywhere claims. The guard
            // keys on this, so the descriptor below can be any real one.
            "test/never-declared-verb"
        }
        fn descriptor(&self) -> crate::sdk_codegen::CommandDescriptor {
            crate::sdk_codegen::CommandDescriptor::of::<StubSpec>()
        }
        async fn invoke(
            &self,
            _params: Value,
            _caller: Option<crate::routing::CallerIdentity>,
        ) -> Result<CommandResult, String> {
            Ok(CommandResult::Json(serde_json::json!({})))
        }
    }

    #[async_trait::async_trait]
    impl ServiceModule for UndeclaredCommandModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: "undeclared",
                priority: ModulePriority::Normal,
                command_prefixes: &[],
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }
        async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
            Ok(())
        }
        async fn handle_command(&self, _c: &str, _p: Value) -> Result<CommandResult, String> {
            Ok(CommandResult::Json(serde_json::json!({})))
        }
        fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
            vec![Arc::new(UndeclaredCommand)]
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    // what this catches: a command shipping only HALF its declaration — a constructor
    // in a module's `commands()` with no `register_command!` descriptor. Such a command
    // ROUTES, so every smoke test passes, while being invisible to `commands/list`, the
    // persona tool surface, the ACL and codegen. Three real verbs shipped that way
    // (`activity/spawn|archive|protect`), which is why the room-minting verb could not
    // be found by anyone — human or citizen — reading the catalog. `dispatch_orphans`
    // covers the mirror case (descriptor, no route); this covers the one that had no
    // check at all. It must PANIC at register(), not warn: the fact is knowable the
    // instant the module registers, and a warning about an invisible command is read
    // by nobody, for the same reason the command is invisible.
    #[test]
    #[should_panic(expected = "has NO DESCRIPTOR")]
    fn a_command_constructor_without_a_descriptor_refuses_to_register() {
        ModuleRegistry::new().register(Arc::new(UndeclaredCommandModule));
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
}
