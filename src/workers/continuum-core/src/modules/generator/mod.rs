//! Generator module — manufactures new Continuum module scaffolds.
//!
//! Per [docs/architecture/MODULE-ARCHITECTURE.md §10](../../../../../docs/architecture/MODULE-ARCHITECTURE.md):
//! the recursive bootstrap. The generator IS a module; the things it
//! creates are modules; every operation it performs is a command. The
//! generator can generate itself (eventually). The system describes
//! itself in its own terms.
//!
//! # Why this exists
//!
//! Joel 2026-05-30 (after the foundation PRs landed): *"we developed a
//! generator so we could manufacture these patterns for new commands
//! modules etc, which itself was a command. Meta."*
//!
//! Right. Every architectural pattern we've codified — the
//! `ServiceModule` trait, `CommandRequest<P>` / `CommandResponse<T>`
//! envelopes, `HandleRef` for long-running state, the four cell return
//! shapes — would degrade fast if every new module's author had to
//! re-derive them from the docs. The generator is the boy-scout
//! amplifier: write the patterns once into a template, run
//! `Commands.execute("generate/module", ...)`, get a module skeleton
//! that already follows them.
//!
//! # Commands provided
//!
//! - **`generate/module`** — scaffolds a new module directory under
//!   `src/workers/continuum-core/src/modules/<name>/` containing a
//!   compilable `mod.rs` with a stub `ServiceModule` impl, plus a
//!   README documenting the module's declared commands + events. The
//!   caller wires the new module into the parent `modules/mod.rs`
//!   manually after generation (next-gen versions can do this too).
//!
//! Future commands (separate PRs as the pattern matures):
//!
//! - `generate/command` — add a new command handler to an existing
//!   module. Wires it into the daemon's `handle_command` dispatch
//!   + emits a typed `Params`/`Result` struct pair.
//! - `generate/refresh` — re-scan the modules tree and refresh
//!   manifests / generated bindings.
//!
//! # What the generated module looks like
//!
//! See `templates::mod_rs_template` for the canonical shape. Short
//! version: a `pub struct <Name>Module {}` with `ServiceModule`
//! implemented, the `ModuleConfig` declaring its commands and events
//! from the spec, and `handle_command` returning a typed
//! "not-yet-implemented" `CommandResponse::err` for each declared
//! command — so the scaffold compiles and registers cleanly, and the
//! author fills in real handlers afterwards.

use async_trait::async_trait;
use serde_json::Value;

use crate::runtime::{
    CommandRequest, CommandResponse, CommandResult, ModuleConfig, ModulePriority, ServiceModule,
};

pub mod templates;
pub mod types;

use types::{GenerateModuleParams, GenerateModuleResult};

/// Generator module — exposes `generate/module` (and future generator
/// commands) as kernel commands. See module docs for the contract.
pub struct GeneratorModule {
    /// Optional override for the workspace root when generating into a
    /// non-default location. Tests use this to write into a tempdir;
    /// production runs leave it `None` and the generator targets
    /// `src/workers/continuum-core/src/modules/<name>/` under the cwd.
    workspace_root: Option<std::path::PathBuf>,
}

impl GeneratorModule {
    pub fn new() -> Self {
        Self {
            workspace_root: None,
        }
    }

    /// Construct with a workspace root override. Tests use this to
    /// generate into a tempdir without touching the live source tree.
    pub fn with_workspace_root(root: std::path::PathBuf) -> Self {
        Self {
            workspace_root: Some(root),
        }
    }
}

impl Default for GeneratorModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for GeneratorModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "generator",
            priority: ModulePriority::Background,
            command_prefixes: &["generate/"],
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
        params: Value,
    ) -> Result<CommandResult, String> {
        match command {
            "generate/module" => self.handle_generate_module(params).await,
            other => Err(format!(
                "{other}: unknown generator command — supported: generate/module"
            )),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl GeneratorModule {
    /// Handle `generate/module` — typed envelope in, typed envelope
    /// out. The actual scaffold work is in
    /// [`generate_module_inner`] so tests can exercise it directly.
    async fn handle_generate_module(&self, params: Value) -> Result<CommandResult, String> {
        let req = CommandRequest::<GenerateModuleParams>::from_value(params)?;
        let result = self.generate_module_inner(&req.params)?;
        CommandResponse::ok(result).into_command_result()
    }

    /// The actual scaffolding. Pure synchronous filesystem work — no
    /// network, no IPC, no shared state. Easy to test.
    pub fn generate_module_inner(
        &self,
        params: &GenerateModuleParams,
    ) -> Result<GenerateModuleResult, String> {
        types::validate_module_name(&params.name)?;
        let target_dir = self.resolve_target_dir(&params.name);

        if target_dir.exists() && !params.force {
            return Err(format!(
                "Module directory already exists: {}. Pass `force: true` to overwrite.",
                target_dir.display()
            ));
        }

        std::fs::create_dir_all(&target_dir).map_err(|e| {
            format!("Failed to create module dir {}: {e}", target_dir.display())
        })?;

        let mut files_created = Vec::new();

        // mod.rs — the compilable ServiceModule stub.
        let mod_rs_path = target_dir.join("mod.rs");
        let mod_rs_content = templates::mod_rs_template(params);
        write_and_record(&mod_rs_path, &mod_rs_content, &mut files_created)?;

        // README.md — author-facing doc + wire-up reminder.
        let readme_path = target_dir.join("README.md");
        let readme_content = templates::readme_template(params);
        write_and_record(&readme_path, &readme_content, &mut files_created)?;

        Ok(GenerateModuleResult {
            module_path: target_dir,
            files_created,
            next_step: format!(
                "Add `pub mod {};` to src/workers/continuum-core/src/modules/mod.rs \
                 and register `Arc::new({}Module::new())` at runtime startup.",
                params.name,
                struct_name(&params.name)
            ),
        })
    }

    /// Compute the on-disk path where the new module will live.
    /// Production targets the continuum-core modules tree; tests
    /// override via `with_workspace_root` to write into a tempdir.
    fn resolve_target_dir(&self, name: &str) -> std::path::PathBuf {
        let root = self.workspace_root.clone().unwrap_or_else(|| {
            std::path::PathBuf::from("src/workers/continuum-core/src/modules")
        });
        root.join(name)
    }
}

/// Convert a module name like "chat" or "ai-provider" into a Rust
/// struct name prefix like "Chat" / "AiProvider". UpperCamelCase with
/// hyphens / underscores treated as word separators.
pub(crate) fn struct_name(module_name: &str) -> String {
    module_name
        .split(['-', '_'])
        .filter(|s| !s.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn write_and_record(
    path: &std::path::Path,
    contents: &str,
    files_created: &mut Vec<std::path::PathBuf>,
) -> Result<(), String> {
    std::fs::write(path, contents)
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    files_created.push(path.to_path_buf());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{ModuleConfig, ModulePriority};

    fn tempdir() -> std::path::PathBuf {
        // Build a unique tempdir per test so concurrent runs don't
        // collide. We don't use the `tempfile` crate here to avoid
        // adding a dev-dep just for this; manual cleanup is fine for
        // unit tests in the workspace.
        let base = std::env::temp_dir().join(format!(
            "continuum-generator-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&base).expect("tempdir create");
        base
    }

    #[test]
    fn struct_name_handles_hyphens_underscores_and_simple_names() {
        assert_eq!(struct_name("chat"), "Chat");
        assert_eq!(struct_name("ai-provider"), "AiProvider");
        assert_eq!(struct_name("ai_provider"), "AiProvider");
        assert_eq!(struct_name("airc-bridge-daemon"), "AircBridgeDaemon");
    }

    #[test]
    fn config_advertises_generate_prefix() {
        let m = GeneratorModule::new();
        let cfg: ModuleConfig = m.config();
        assert_eq!(cfg.name, "generator");
        assert_eq!(cfg.command_prefixes, &["generate/"]);
        assert!(matches!(cfg.priority, ModulePriority::Background));
    }

    #[test]
    fn generate_module_creates_dir_and_files() {
        let root = tempdir();
        let m = GeneratorModule::with_workspace_root(root.clone());
        let params = GenerateModuleParams {
            name: "demo".into(),
            description: "Demo module for generator tests".into(),
            commands: vec!["demo/echo".into()],
            events_subscribed: vec![],
            events_published: vec![],
            priority: types::PrioritySpec::Normal,
            force: false,
        };
        let result = m
            .generate_module_inner(&params)
            .expect("generation must succeed in an empty dir");

        assert_eq!(result.module_path, root.join("demo"));
        assert!(result.module_path.is_dir(), "module dir must exist");

        let mod_rs = result.module_path.join("mod.rs");
        let readme = result.module_path.join("README.md");
        assert!(mod_rs.is_file(), "mod.rs must be created");
        assert!(readme.is_file(), "README.md must be created");

        let mod_rs_content = std::fs::read_to_string(&mod_rs).unwrap();
        assert!(
            mod_rs_content.contains("pub struct DemoModule"),
            "generated struct name follows naming convention: {mod_rs_content}"
        );
        assert!(
            mod_rs_content.contains("\"demo/echo\""),
            "generated config lists the declared commands"
        );
        assert!(
            mod_rs_content.contains("ServiceModule"),
            "generated module implements the canonical trait"
        );
    }

    #[test]
    fn generate_module_refuses_existing_dir_without_force() {
        let root = tempdir();
        let m = GeneratorModule::with_workspace_root(root.clone());
        let params = GenerateModuleParams {
            name: "demo".into(),
            description: "first".into(),
            commands: vec![],
            events_subscribed: vec![],
            events_published: vec![],
            priority: types::PrioritySpec::Normal,
            force: false,
        };
        // First run succeeds.
        m.generate_module_inner(&params).expect("first generation");
        // Second run without force refuses.
        let err = m
            .generate_module_inner(&params)
            .expect_err("repeat generation without force must fail loud");
        assert!(
            err.contains("already exists"),
            "error must name the conflict: {err}"
        );
        assert!(
            err.contains("force"),
            "error must point at the escape hatch: {err}"
        );
    }

    #[test]
    fn generate_module_overwrites_with_force() {
        let root = tempdir();
        let m = GeneratorModule::with_workspace_root(root.clone());
        let mut params = GenerateModuleParams {
            name: "demo".into(),
            description: "first".into(),
            commands: vec![],
            events_subscribed: vec![],
            events_published: vec![],
            priority: types::PrioritySpec::Normal,
            force: false,
        };
        m.generate_module_inner(&params).expect("first generation");
        params.description = "second — overwritten".into();
        params.force = true;
        let result = m
            .generate_module_inner(&params)
            .expect("force-flagged regeneration must succeed");
        let mod_rs = std::fs::read_to_string(result.module_path.join("mod.rs")).unwrap();
        assert!(
            mod_rs.contains("second — overwritten"),
            "second generation must reflect the new description"
        );
    }

    #[test]
    fn generate_module_rejects_invalid_names() {
        let root = tempdir();
        let m = GeneratorModule::with_workspace_root(root);
        for bad in ["", "Has Space", "has/slash", "../escape", "9starts-with-digit"] {
            let params = GenerateModuleParams {
                name: bad.into(),
                description: "x".into(),
                commands: vec![],
                events_subscribed: vec![],
                events_published: vec![],
                priority: types::PrioritySpec::Normal,
                force: false,
            };
            let err = m
                .generate_module_inner(&params)
                .expect_err("invalid name must surface as error");
            assert!(
                err.contains("name") || err.contains("identifier"),
                "validation error must name the offending field: {err}"
            );
        }
    }

    #[tokio::test]
    async fn handle_command_returns_typed_envelope() {
        let root = tempdir();
        let m = GeneratorModule::with_workspace_root(root.clone());
        let params = serde_json::json!({
            "name": "envelope_demo",
            "description": "Verifies the full envelope round-trip",
            "commands": ["envelope_demo/ping"],
            "events_subscribed": [],
            "events_published": [],
            "priority": "normal",
            "force": false
        });
        let result = m
            .handle_command("generate/module", params)
            .await
            .expect("generate/module must succeed via the typed envelope");
        let value = match result {
            CommandResult::Json(v) => v,
            other => panic!("expected Json variant, got {other:?}"),
        };
        assert_eq!(value["success"], true);
        assert!(
            value["module_path"].is_string(),
            "envelope flattens the typed result fields: {value}"
        );
        assert!(
            value["files_created"].is_array(),
            "envelope carries the file list"
        );
        assert!(
            value["next_step"].as_str().unwrap().contains("pub mod"),
            "next_step prompts the caller to wire the new module"
        );
    }

    #[tokio::test]
    async fn handle_command_rejects_unknown_command_loud() {
        let m = GeneratorModule::new();
        let err = m
            .handle_command("generate/nonexistent", serde_json::json!({}))
            .await
            .expect_err("unknown sub-command must surface");
        assert!(
            err.contains("generate/nonexistent") && err.contains("unknown"),
            "error must name the bad command + what's supported: {err}"
        );
    }
}
