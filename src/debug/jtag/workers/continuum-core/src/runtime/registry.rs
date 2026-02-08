/// ModuleRegistry — DashMap-based command routing + typed module discovery.
///
/// Replaces the 55-arm match statement in ipc/mod.rs with dynamic routing.
/// `register(module)` auto-wires commands from the module's config.
/// Like CBAR's appendAnalyzer() — register once, everything routes automatically.
///
/// Thread-safe: uses DashMap and RwLock for interior mutability.
/// Can be shared via Arc across threads.

use super::service_module::{ModuleConfig, ModulePriority, ServiceModule};
use super::module_metrics::ModuleMetrics;
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
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self {
            modules: DashMap::new(),
            configs: DashMap::new(),
            metrics: DashMap::new(),
            command_routes: RwLock::new(Vec::new()),
            type_routes: DashMap::new(),
        }
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
        self.metrics.insert(name.to_string(), Arc::new(ModuleMetrics::new(name)));

        // Build command routing table from declared prefixes
        {
            let mut routes = self.command_routes.write();
            for prefix in config.command_prefixes {
                routes.push((prefix, name));
            }
            // Sort by prefix length descending (longest match first)
            routes.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        }

        // Register type for downcast discovery
        let type_id = (*module).as_any().type_id();
        self.type_routes.insert(type_id, name);
    }

    /// Route a command to the correct module.
    /// Returns (module, full_command) — the module receives the full command string.
    /// Replaces the 55-arm match statement.
    pub fn route_command(&self, command: &str) -> Option<(Arc<dyn ServiceModule>, String)> {
        let routes = self.command_routes.read();
        for &(prefix, module_name) in routes.iter() {
            if command.starts_with(prefix) {
                return self.modules.get(module_name).map(|module| {
                    (module.clone(), command.to_string())
                });
            }
        }
        None
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
        self.type_routes.get(&type_id).and_then(|name| {
            self.modules.get(*name).map(|m| m.clone())
        })
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::service_module::*;
    use super::super::ModuleContext;
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
            }
        }

        async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
            Ok(())
        }

        async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
            Ok(CommandResult::Json(serde_json::json!({
                "module": self.name,
                "command": command,
            })))
        }

        fn as_any(&self) -> &dyn Any { self }
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
