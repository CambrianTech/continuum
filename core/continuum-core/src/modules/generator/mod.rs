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
//!   `core/continuum-core/src/modules/<name>/` containing a
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

use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;

use crate::runtime::{CommandResult, ModuleConfig, ModulePriority, ServiceModule};

pub mod templates;
pub mod types;

use types::{GenerateModuleParams, GenerateModuleResult};

/// The stateful generator engine — the workspace root + per-name locks + the
/// scaffolding logic. `Arc`-shared between [`GeneratorModule`] (which exposes it
/// as the `generate/module` command) and that command object, so concurrent
/// callers serialize on the SAME `name_locks`. Constructing fresh state per call
/// would silently break the same-name serialization guarantee, so the engine is
/// shared, not rebuilt.
pub struct GeneratorEngine {
    /// Optional override for the workspace root when generating into a
    /// non-default location. Tests use this to write into a tempdir;
    /// production runs leave it `None` and the generator targets
    /// `core/continuum-core/src/modules/<name>/` under the cwd.
    workspace_root: Option<std::path::PathBuf>,

    /// Per-module-name locks. Concurrent `generate/module` calls
    /// targeting DIFFERENT names stay fully parallel (DashMap's
    /// lock-free read path); calls targeting the SAME name serialize
    /// so the exists()-check / mkdir / write sequence is atomic.
    ///
    /// Without this, two concurrent generators with the same name
    /// and different params would race the dir-exists check, both
    /// pass, both call create_dir_all, both write — and the on-disk
    /// state ends with mod.rs from one caller's template + README
    /// from the other's (silent torn-state corruption). With it, the
    /// loser sees the canonical "already exists" error (without
    /// force) or the writes serialize cleanly so the final state
    /// belongs to ONE generation round (with force).
    ///
    /// `std::sync::Mutex` (not `tokio::sync`) because the protected
    /// critical section is purely synchronous filesystem I/O — no
    /// `.await` inside the lock — so blocking the tokio worker for
    /// the brief mkdir + 2 writes is correct and avoids cascading the
    /// API into async.
    ///
    /// Per Joel 2026-05-30: "Each persona exists in its own threads."
    /// The kernel registers ONE generator module; multiple personas
    /// (or scripts) firing `generate/module` concurrently is the
    /// production scenario, not a rare path.
    name_locks: DashMap<String, Arc<std::sync::Mutex<()>>>,
}

impl GeneratorEngine {
    pub fn new() -> Self {
        Self {
            workspace_root: None,
            name_locks: DashMap::new(),
        }
    }

    /// Construct with a workspace root override. Tests use this to
    /// generate into a tempdir without touching the live source tree.
    pub fn with_workspace_root(root: std::path::PathBuf) -> Self {
        Self {
            workspace_root: Some(root),
            name_locks: DashMap::new(),
        }
    }

    /// Get-or-create the per-name lock for `name`. `DashMap::entry`
    /// is atomic within a shard, so concurrent callers either find
    /// the same Arc (one wins the slot, others clone) or both create
    /// distinct Arcs for distinct names (different shards stay
    /// parallel).
    ///
    /// Lock entries are never evicted — module names are bounded
    /// (no unbounded production stream of unique names) and each
    /// entry is small (~50 bytes). If memory ever matters, a TTL
    /// scan can be added without changing the protocol.
    fn name_lock(&self, name: &str) -> Arc<std::sync::Mutex<()>> {
        self.name_locks
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(std::sync::Mutex::new(())))
            .clone()
    }
}

impl Default for GeneratorEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Generator module — exposes `generate/module` (and future generator
/// commands) as typed kernel commands. A thin holder over the `Arc`-shared
/// [`GeneratorEngine`]; `commands()` hands the engine to the command object.
pub struct GeneratorModule {
    engine: Arc<GeneratorEngine>,
}

impl GeneratorModule {
    pub fn new() -> Self {
        Self {
            engine: Arc::new(GeneratorEngine::new()),
        }
    }

    /// Construct with a workspace root override. Tests use this to
    /// generate into a tempdir without touching the live source tree.
    pub fn with_workspace_root(root: std::path::PathBuf) -> Self {
        Self {
            engine: Arc::new(GeneratorEngine::with_workspace_root(root)),
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

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // `generate/module` is migrated to the typed registry
        // (`commands/generator/module.rs`, exposed via `commands()` below).
        // Fail loud — no silent legacy fallback.
        Err(format!(
            "generator command surface is migrated to the typed registry; \
             '{command}' has no legacy handler"
        ))
    }

    fn commands(&self) -> Vec<std::sync::Arc<dyn crate::sdk_codegen::DynCommand>> {
        vec![std::sync::Arc::new(
            crate::commands::generator::module::GenerateModule {
                engine: self.engine.clone(),
            },
        )]
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl GeneratorEngine {
    /// The actual scaffolding. Pure synchronous filesystem work — no
    /// network, no IPC, no `.await`. Easy to test.
    ///
    /// # Concurrency contract
    ///
    /// Two concurrent callers targeting the SAME `params.name`
    /// serialize via a per-name `std::sync::Mutex` held across the
    /// entire exists() / mkdir / write sequence — so the substrate's
    /// promises hold under load:
    ///
    /// - Without `force`: the loser of the race sees the canonical
    ///   "already exists" error (not a silent overwrite).
    /// - With `force`: both succeed, but the FINAL on-disk state
    ///   belongs to ONE generation round — never torn (mod.rs from
    ///   caller A + README from caller B).
    ///
    /// Different names stay fully parallel (different DashMap shards).
    pub fn generate_module_inner(
        &self,
        params: &GenerateModuleParams,
    ) -> Result<GenerateModuleResult, String> {
        types::validate_module_name(&params.name)?;
        let target_dir = self.resolve_target_dir(&params.name);

        // Serialize same-name concurrent generation. Mutex is held
        // for the entire exists() / mkdir / write sequence so the
        // race window between "I checked, dir doesn't exist" and "I
        // created the dir + wrote files" is closed.
        let name_lock = self.name_lock(&params.name);
        let _guard = name_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if target_dir.exists() && !params.force {
            return Err(format!(
                "Module directory already exists: {}. Pass `force: true` to overwrite.",
                target_dir.display()
            ));
        }

        std::fs::create_dir_all(&target_dir)
            .map_err(|e| format!("Failed to create module dir {}: {e}", target_dir.display()))?;

        let mut files_created = Vec::new();

        // ── mod.rs — the compilable ServiceModule with envelope dispatch
        let mod_rs_path = target_dir.join("mod.rs");
        let mod_rs_content = templates::mod_rs_template(params);
        write_and_record(&mod_rs_path, &mod_rs_content, &mut files_created)?;

        // ── types.rs — typed Params/Result pairs with ts-rs exports
        let types_rs_path = target_dir.join("types.rs");
        let types_rs_content = templates::types_rs_template(params);
        write_and_record(&types_rs_path, &types_rs_content, &mut files_created)?;

        // ── DESIGN.md — per-module design skeleton
        let design_md_path = target_dir.join("DESIGN.md");
        let design_md_content = templates::design_md_template(params);
        write_and_record(&design_md_path, &design_md_content, &mut files_created)?;

        // ── README.md — author-facing summary + wire-up reminder
        let readme_path = target_dir.join("README.md");
        let readme_content = templates::readme_template(params);
        write_and_record(&readme_path, &readme_content, &mut files_created)?;

        Ok(GenerateModuleResult {
            module_path: target_dir,
            files_created,
            next_step: format!(
                "Add `pub mod {};` to core/continuum-core/src/modules/mod.rs \
                 and register `Arc::new({}Module::new())` at runtime startup. \
                 Then fill in handler bodies + Params/Result fields per DESIGN.md.",
                params.name,
                struct_name(&params.name)
            ),
        })
    }

    /// Compute the on-disk path where the new module will live.
    /// Production targets the continuum-core modules tree; tests
    /// override via `with_workspace_root` to write into a tempdir.
    fn resolve_target_dir(&self, name: &str) -> std::path::PathBuf {
        let root = self
            .workspace_root
            .clone()
            .unwrap_or_else(|| std::path::PathBuf::from("core/continuum-core/src/modules"));
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
        // collide. PID is constant across cargo's in-process test
        // threads, so PID+nanos can collide when two tempdir() calls
        // land in the same SystemTime::now() granularity — and four
        // tests in this suite use `name: "demo"`, so a tempdir
        // collision would race them on <base>/demo/mod.rs. UUID v4
        // makes the suffix collision-free regardless of clock
        // granularity (uuid is already a workspace dep).
        let base = std::env::temp_dir().join(format!(
            "continuum-generator-test-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
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
        let m = GeneratorEngine::with_workspace_root(root.clone());
        let params = GenerateModuleParams {
            name: "demo".into(),
            description: "Demo module for generator tests".into(),
            commands: vec!["demo/echo".into()],
            events_subscribed: vec![],
            events_published: vec![],
            priority: types::PrioritySpec::Normal,
            force: false,
            stateful: false,
        };
        let result = m
            .generate_module_inner(&params)
            .expect("generation must succeed in an empty dir");

        assert_eq!(result.module_path, root.join("demo"));
        assert!(result.module_path.is_dir(), "module dir must exist");

        let mod_rs = result.module_path.join("mod.rs");
        let types_rs = result.module_path.join("types.rs");
        let design_md = result.module_path.join("DESIGN.md");
        let readme = result.module_path.join("README.md");
        assert!(mod_rs.is_file(), "mod.rs must be created");
        assert!(types_rs.is_file(), "types.rs must be created");
        assert!(design_md.is_file(), "DESIGN.md must be created");
        assert!(readme.is_file(), "README.md must be created");
        assert_eq!(
            result.files_created.len(),
            4,
            "v2 scaffolding writes mod.rs + types.rs + DESIGN.md + README.md"
        );

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
        assert!(
            mod_rs_content.contains("CommandRequest::<EchoParams>::from_value(params)?"),
            "v2 scaffold dispatches via typed envelope"
        );

        let types_rs_content = std::fs::read_to_string(&types_rs).unwrap();
        assert!(
            types_rs_content.contains("pub struct EchoParams"),
            "types.rs carries the typed Params for the declared command"
        );
        assert!(
            types_rs_content.contains("pub struct EchoResult"),
            "types.rs carries the typed Result for the declared command"
        );

        let design_md_content = std::fs::read_to_string(&design_md).unwrap();
        assert!(
            design_md_content.contains("## Concurrency contract"),
            "DESIGN.md scaffolds the canonical sections"
        );
    }

    /// Dogfood: scaffold a STATEFUL multi-command module and verify
    /// the generated source has consistent cross-references between
    /// mod.rs (envelope dispatch, handler methods, lock helper) and
    /// types.rs (typed Params/Result for each command). This is the
    /// closest unit-level proof that a real consumer (e.g., the next
    /// chat-analyze migration) can `cargo check` the scaffold without
    /// touching it.
    #[test]
    fn stateful_multi_command_scaffold_has_consistent_cross_references() {
        let root = tempdir();
        let m = GeneratorEngine::with_workspace_root(root.clone());
        let params = GenerateModuleParams {
            name: "stateful_demo".into(),
            description: "Stateful module dogfood test".into(),
            commands: vec![
                "stateful_demo/open".into(),
                "stateful_demo/poll".into(),
                "stateful_demo/close".into(),
            ],
            events_subscribed: vec![],
            events_published: vec!["stateful_demo:opened".into()],
            priority: types::PrioritySpec::Normal,
            force: false,
            stateful: true,
        };
        let result = m
            .generate_module_inner(&params)
            .expect("stateful scaffold must succeed");
        assert_eq!(result.files_created.len(), 4);

        let mod_rs = std::fs::read_to_string(result.module_path.join("mod.rs")).unwrap();
        let types_rs = std::fs::read_to_string(result.module_path.join("types.rs")).unwrap();

        // Cross-reference: every command in the dispatch must have a
        // matching typed handler method, which must reference a typed
        // Params + Result that types.rs declares.
        for (command, type_stem, handler) in [
            ("stateful_demo/open", "Open", "handle_open"),
            ("stateful_demo/poll", "Poll", "handle_poll"),
            ("stateful_demo/close", "Close", "handle_close"),
        ] {
            assert!(
                mod_rs.contains(&format!("\"{command}\" =>")),
                "mod.rs missing dispatch arm for {command}"
            );
            assert!(
                mod_rs.contains(&format!(
                    "CommandRequest::<{type_stem}Params>::from_value(params)?"
                )),
                "mod.rs missing typed envelope parse for {command}"
            );
            assert!(
                mod_rs.contains(&format!("self.{handler}(req.params)")),
                "mod.rs missing dispatch to {handler}"
            );
            assert!(
                mod_rs.contains(&format!("pub async fn {handler}(")),
                "mod.rs missing typed handler method {handler}"
            );
            assert!(
                types_rs.contains(&format!("pub struct {type_stem}Params")),
                "types.rs missing {type_stem}Params"
            );
            assert!(
                types_rs.contains(&format!("pub struct {type_stem}Result")),
                "types.rs missing {type_stem}Result"
            );
        }

        // Stateful-specific scaffold: lock map field + helper + struct.
        assert!(
            mod_rs.contains(
                "resource_locks: DashMap<String, Arc<tokio::sync::Mutex<ResourceState>>>"
            ),
            "stateful mod.rs must carry the lock map field"
        );
        assert!(
            mod_rs.contains("fn resource_lock(&self, id: &str)"),
            "stateful mod.rs must expose the lock helper"
        );
        assert!(
            mod_rs.contains("struct ResourceState"),
            "stateful mod.rs must declare ResourceState"
        );
        assert!(
            mod_rs.contains("resource_locks_stay_parallel_across_distinct_ids"),
            "stateful scaffold must include the per-resource concurrency test"
        );
    }

    #[test]
    fn generate_module_refuses_existing_dir_without_force() {
        let root = tempdir();
        let m = GeneratorEngine::with_workspace_root(root.clone());
        let params = GenerateModuleParams {
            name: "demo".into(),
            description: "first".into(),
            commands: vec![],
            events_subscribed: vec![],
            events_published: vec![],
            priority: types::PrioritySpec::Normal,
            force: false,
            stateful: false,
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
        let m = GeneratorEngine::with_workspace_root(root.clone());
        let mut params = GenerateModuleParams {
            name: "demo".into(),
            description: "first".into(),
            commands: vec![],
            events_subscribed: vec![],
            events_published: vec![],
            priority: types::PrioritySpec::Normal,
            force: false,
            stateful: false,
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
        let m = GeneratorEngine::with_workspace_root(root);
        for bad in [
            "",
            "Has Space",
            "has/slash",
            "../escape",
            "9starts-with-digit",
        ] {
            let params = GenerateModuleParams {
                name: bad.into(),
                description: "x".into(),
                commands: vec![],
                events_subscribed: vec![],
                events_published: vec![],
                priority: types::PrioritySpec::Normal,
                force: false,
                stateful: false,
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

    // what this catches: generate/module is migrated to the typed registry
    // (commands/generator/module.rs), so the module's legacy handle_command must
    // fail loud for any command name (no silent fallback). The typed-envelope
    // round-trip is covered in the command file's own tests.
    #[tokio::test]
    async fn legacy_handle_command_fails_loud() {
        let m = GeneratorModule::new();
        let err = m
            .handle_command("generate/module", serde_json::json!({}))
            .await
            .expect_err("legacy handler must fail loud after migration");
        assert!(
            err.contains("migrated to the typed registry"),
            "error must name the migration: {err}"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Multi-persona concurrency stress tests — gated behind the
    // `stress-tests` cargo feature. Default `cargo test` skips
    // compilation; periodic CI runs them via
    //     cargo test -p continuum-core --features stress-tests
    // See continuum-core/Cargo.toml § "stress-tests" for the doctrine.
    // ════════════════════════════════════════════════════════════════
    #[cfg(feature = "stress-tests")]
    mod stress {
        use super::*;
        //
        // Per Joel 2026-05-30: "Each persona exists in its own threads."
        //
        // The kernel registers ONE GeneratorModule; multiple personas (or
        // scripts) may call `generate/module` concurrently. The per-name
        // mutex on the module guarantees:
        //
        // - same-name calls serialize (one wins without force; consistent
        //   final state with force)
        // - different-name calls stay fully parallel (different DashMap
        //   shards, no contention)
        //
        // Every test uses `flavor = "multi_thread", worker_threads = 4`
        // so spawned tasks actually preempt on distinct OS threads, not
        // cooperatively interleave on one. The protected work is purely
        // synchronous filesystem I/O (`std::sync::Mutex`), so blocking
        // worker threads briefly for mkdir + 2 writes is correct.

        /// N concurrent generators race the same name without force.
        /// EXACTLY ONE must succeed; the rest must surface the canonical
        /// "already exists" error. Without the per-name mutex, ALL of
        /// them would pass the exists() check, ALL would write, and the
        /// friendly error would be silenced — silent data corruption.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn same_name_concurrent_generation_without_force_yields_one_winner() {
            const PARALLEL: usize = 8;

            let root = tempdir();
            let module = Arc::new(GeneratorEngine::with_workspace_root(root.clone()));

            let mut tasks = Vec::with_capacity(PARALLEL);
            for i in 0..PARALLEL {
                let module = module.clone();
                tasks.push(tokio::spawn(async move {
                    module.generate_module_inner(&GenerateModuleParams {
                        name: "racy".into(),
                        description: format!("attempt {i}"),
                        commands: vec![],
                        events_subscribed: vec![],
                        events_published: vec![],
                        priority: types::PrioritySpec::Normal,
                        force: false,
                        stateful: false,
                    })
                }));
            }
            let results: Vec<Result<GenerateModuleResult, String>> =
                futures::future::join_all(tasks)
                    .await
                    .into_iter()
                    .map(|r| r.expect("task must not panic"))
                    .collect();

            let winners = results.iter().filter(|r| r.is_ok()).count();
            let losers = results.iter().filter(|r| r.is_err()).count();

            assert_eq!(
            winners, 1,
            "exactly ONE concurrent generation must succeed without force; got {winners} winners"
        );
            assert_eq!(
                losers,
                PARALLEL - 1,
                "the remaining {} must Err; got {losers}",
                PARALLEL - 1
            );
            for r in &results {
                if let Err(e) = r {
                    assert!(
                        e.contains("already exists"),
                        "losers must surface the canonical error: {e}"
                    );
                    assert!(
                        e.contains("force"),
                        "loser error must mention the `force` escape hatch: {e}"
                    );
                }
            }

            // Filesystem state: the dir exists once, both files present.
            assert!(root.join("racy").join("mod.rs").exists());
            assert!(root.join("racy").join("README.md").exists());
        }

        /// N concurrent generators race the same name WITH force. All
        /// should succeed (force allows overwrite). Critical: the final
        /// on-disk state must NOT be torn — mod.rs and README must come
        /// from the SAME caller's params, not a mix of different
        /// callers' templates.
        ///
        /// We tag each caller with a unique `description` (embedded in
        /// both templates); reading the final files must show the SAME
        /// description in both. Without the per-name lock, the writes
        /// would interleave per file → mismatch.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn same_name_concurrent_generation_with_force_produces_consistent_final_state() {
            const PARALLEL: usize = 8;

            let root = tempdir();
            let module = Arc::new(GeneratorEngine::with_workspace_root(root.clone()));

            let mut tasks = Vec::with_capacity(PARALLEL);
            for i in 0..PARALLEL {
                let module = module.clone();
                tasks.push(tokio::spawn(async move {
                    module.generate_module_inner(&GenerateModuleParams {
                        name: "forcy".into(),
                        description: format!("MARKER-{i:02}"),
                        commands: vec![],
                        events_subscribed: vec![],
                        events_published: vec![],
                        priority: types::PrioritySpec::Normal,
                        force: true,
                        stateful: false,
                    })
                }));
            }
            let results: Vec<Result<GenerateModuleResult, String>> =
                futures::future::join_all(tasks)
                    .await
                    .into_iter()
                    .map(|r| r.expect("task must not panic"))
                    .collect();

            for r in &results {
                assert!(
                    r.is_ok(),
                    "every force=true concurrent generation must succeed: {r:?}"
                );
            }

            // Read both files. They must contain the SAME marker.
            let mod_rs = std::fs::read_to_string(root.join("forcy").join("mod.rs"))
                .expect("mod.rs must exist");
            let readme = std::fs::read_to_string(root.join("forcy").join("README.md"))
                .expect("README.md must exist");

            // Pull MARKER-XX out of each file (both templates embed the
            // description). The two markers MUST match.
            let mod_marker = extract_marker(&mod_rs).expect("mod.rs must carry a marker");
            let readme_marker = extract_marker(&readme).expect("README.md must carry a marker");
            assert_eq!(
            mod_marker, readme_marker,
            "mod.rs ({mod_marker}) and README.md ({readme_marker}) must come from the SAME generation round — torn state from interleaved writes would surface here"
        );
        }

        /// Helper for the torn-state test: pull `MARKER-XX` out of a
        /// file's content. Looks for the pattern emitted by the
        /// description field which both templates embed.
        fn extract_marker(content: &str) -> Option<String> {
            for line in content.lines() {
                if let Some(idx) = line.find("MARKER-") {
                    let rest = &line[idx..];
                    // Take "MARKER-" + 2 digits.
                    let end = "MARKER-".len() + 2;
                    if rest.len() >= end {
                        return Some(rest[..end].to_string());
                    }
                }
            }
            None
        }

        /// N concurrent generators with DISTINCT names. All must succeed,
        /// each producing its own files. This is the "stay parallel"
        /// half of the per-name lock's promise — different shards in the
        /// DashMap, no cross-name contention.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn different_names_concurrent_generation_runs_fully_parallel() {
            const PARALLEL: usize = 12;

            let root = tempdir();
            let module = Arc::new(GeneratorEngine::with_workspace_root(root.clone()));

            let mut tasks = Vec::with_capacity(PARALLEL);
            for i in 0..PARALLEL {
                let module = module.clone();
                let name = format!("parallel_{i:02}");
                tasks.push(tokio::spawn(async move {
                    let result = module.generate_module_inner(&GenerateModuleParams {
                        name: name.clone(),
                        description: format!("module {i}"),
                        commands: vec![],
                        events_subscribed: vec![],
                        events_published: vec![],
                        priority: types::PrioritySpec::Normal,
                        force: false,
                        stateful: false,
                    });
                    (name, result)
                }));
            }
            let results: Vec<(String, Result<GenerateModuleResult, String>)> =
                futures::future::join_all(tasks)
                    .await
                    .into_iter()
                    .map(|r| r.expect("task must not panic"))
                    .collect();

            // Every distinct-name task must succeed.
            for (name, result) in &results {
                let r = result
                    .as_ref()
                    .unwrap_or_else(|e| panic!("distinct-name {name} must succeed: {e}"));
                assert_eq!(
                r.files_created.len(),
                4,
                "{name}: every successful generation writes mod.rs + types.rs + DESIGN.md + README.md"
            );
            }

            // Every module's directory + files exist and are distinct on
            // disk (no cross-contamination).
            for (name, _) in &results {
                let dir = root.join(name);
                assert!(dir.join("mod.rs").exists(), "{name}: mod.rs must exist");
                assert!(
                    dir.join("README.md").exists(),
                    "{name}: README.md must exist"
                );
            }

            // The per-name lock map carries one entry per distinct name.
            assert_eq!(
                module.name_locks.len(),
                PARALLEL,
                "each distinct name gets its own lock entry"
            );
        }
    } // end mod stress
}
