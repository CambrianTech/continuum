use super::*;
use crate::modules::generator::types::PrioritySpec;

fn sample_params() -> GenerateModuleParams {
    GenerateModuleParams {
        name: "demo".into(),
        description: "Sample module for template tests".into(),
        commands: vec!["demo/echo".into(), "demo/ping".into()],
        events_subscribed: vec!["data:demo_items:created".into()],
        events_published: vec!["demo:event:emitted".into()],
        priority: PrioritySpec::Normal,
        force: false,
        stateful: false,
    }
}

// ── mod.rs template ──────────────────────────────────────────────

#[test]
fn mod_rs_contains_struct_definition_and_trait_impl() {
    let s = mod_rs_template(&sample_params());
    assert!(s.contains("pub struct DemoModule"));
    assert!(s.contains("impl ServiceModule for DemoModule"));
    assert!(s.contains("ModulePriority::Normal"));
}

#[test]
fn mod_rs_uses_typed_envelope_dispatch_for_each_command() {
    let s = mod_rs_template(&sample_params());
    // Each declared command's arm parses CommandRequest with the
    // typed Params + calls the typed handler + materializes
    // CommandResponse — the canonical envelope pattern.
    assert!(
        s.contains("CommandRequest::<EchoParams>::from_value(params)?"),
        "demo/echo must parse the typed envelope: {s}"
    );
    assert!(
        s.contains("self.handle_echo(req.params).await?"),
        "demo/echo must dispatch to handle_echo: {s}"
    );
    assert!(
        s.contains("CommandResponse::ok(result).into_command_result()"),
        "demo/echo must materialize the typed response: {s}"
    );
    assert!(s.contains("CommandRequest::<PingParams>::from_value(params)?"));
    assert!(s.contains("self.handle_ping(req.params).await?"));
}

#[test]
fn mod_rs_emits_typed_handler_methods_for_each_command() {
    let s = mod_rs_template(&sample_params());
    assert!(
        s.contains("pub async fn handle_echo(") && s.contains("params: EchoParams"),
        "handle_echo method must exist with typed param: {s}"
    );
    assert!(s.contains("Result<EchoResult, String>"));
    assert!(s.contains("pub async fn handle_ping("));
}

#[test]
fn mod_rs_imports_envelope_types_from_runtime() {
    let s = mod_rs_template(&sample_params());
    assert!(s.contains("CommandRequest"), "must import CommandRequest");
    assert!(s.contains("CommandResponse"), "must import CommandResponse");
    assert!(s.contains("CommandExecutor"), "must import CommandExecutor");
}

#[test]
fn mod_rs_includes_with_executor_constructor_for_tests() {
    let s = mod_rs_template(&sample_params());
    assert!(
        s.contains("#[cfg(test)]"),
        "must scope test-only constructor"
    );
    assert!(
        s.contains("pub fn with_executor(executor: Arc<CommandExecutor>) -> Self"),
        "with_executor must be available for test injection"
    );
    assert!(s.contains("executor_override"));
}

#[test]
fn mod_rs_emits_concurrency_stress_test_with_multi_thread_runtime() {
    let s = mod_rs_template(&sample_params());
    assert!(
        s.contains("flavor = \"multi_thread\", worker_threads = 4"),
        "concurrency test must use multi-thread tokio per field manual §4.2"
    );
    assert!(s.contains("fn handlers_under_concurrent_load"));
    assert!(
        s.contains("Arc::new(DemoModule::new())"),
        "test must use Arc<Module> per production multi-persona pattern"
    );
}

#[test]
fn mod_rs_for_stateless_module_omits_resource_lock_scaffold() {
    let s = mod_rs_template(&sample_params());
    assert!(
        !s.contains("resource_locks: DashMap"),
        "stateless module must NOT carry the resource lock field: {s}"
    );
    assert!(
        !s.contains("ResourceState"),
        "stateless module must NOT declare ResourceState"
    );
    assert!(
        !s.contains("use dashmap::DashMap"),
        "stateless module must NOT import dashmap"
    );
}

#[test]
fn mod_rs_for_stateful_module_emits_per_resource_lock_scaffold() {
    let mut p = sample_params();
    p.stateful = true;
    let s = mod_rs_template(&p);
    assert!(
        s.contains("use dashmap::DashMap"),
        "stateful module must import dashmap"
    );
    assert!(
        s.contains("struct ResourceState"),
        "stateful module must declare ResourceState"
    );
    assert!(
        s.contains("resource_locks: DashMap<String, Arc<tokio::sync::Mutex<ResourceState>>>"),
        "stateful module must carry the canonical per-resource lock map"
    );
    assert!(
        s.contains("fn resource_lock(&self, id: &str)"),
        "stateful module must expose the get-or-create helper"
    );
    assert!(
        s.contains("resource_locks_stay_parallel_across_distinct_ids"),
        "stateful module must include the per-resource concurrency test"
    );
}

#[test]
fn mod_rs_subscribes_to_declared_events() {
    let s = mod_rs_template(&sample_params());
    assert!(s.contains("\"data:demo_items:created\""));
}

#[test]
fn mod_rs_documents_published_events_in_module_docstring() {
    let s = mod_rs_template(&sample_params());
    assert!(s.contains("Documented published events"));
    assert!(s.contains("`demo:event:emitted`"));
}

#[test]
fn mod_rs_for_command_less_module_still_compiles_shape() {
    let mut p = sample_params();
    p.commands.clear();
    let s = mod_rs_template(&p);
    // Even with no commands the module-name prefix is in
    // command_prefixes so future commands route through it.
    assert!(s.contains("\"demo/\""));
    // The dispatch block is empty; the catch-all stays.
    assert!(s.contains("other => Err"));
    // No typed import line (no commands → no types).
    assert!(!s.contains("use types::{"));
}

// ── types.rs template ────────────────────────────────────────────

#[test]
fn types_rs_emits_params_and_result_for_each_command() {
    let s = types_rs_template(&sample_params());
    assert!(s.contains("pub struct EchoParams"));
    assert!(s.contains("pub struct EchoResult"));
    assert!(s.contains("pub struct PingParams"));
    assert!(s.contains("pub struct PingResult"));
}

#[test]
fn types_rs_annotates_for_ts_rs_export_with_camel_case() {
    let s = types_rs_template(&sample_params());
    assert!(s.contains("#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]"));
    assert!(s.contains("#[serde(rename_all = \"camelCase\")]"));
    assert!(
        s.contains("export_to = \"../../../protocol/typescript/demo/EchoParams.ts\""),
        "export_to path must match the canonical shared/generated layout"
    );
}

#[test]
fn types_rs_for_command_less_module_emits_no_params_structs() {
    let mut p = sample_params();
    p.commands.clear();
    let s = types_rs_template(&p);
    assert!(!s.contains("pub struct"));
    assert!(s.contains("No commands declared yet"));
}

// ── design.md template ───────────────────────────────────────────

#[test]
fn design_md_includes_all_required_sections() {
    let s = design_md_template(&sample_params());
    for header in [
        "## Role",
        "## Command surface",
        "## Cross-module dependencies",
        "## State model",
        "## Events emitted",
        "## Concurrency contract",
        "## Migration notes",
        "## Kinks found",
    ] {
        assert!(
            s.contains(header),
            "DESIGN.md must include header `{header}`: {s}"
        );
    }
}

#[test]
fn design_md_lists_each_command_in_the_surface_table() {
    let s = design_md_template(&sample_params());
    assert!(s.contains("| `demo/echo` | `EchoParams` | `EchoResult` |"));
    assert!(s.contains("| `demo/ping` | `PingParams` | `PingResult` |"));
}

#[test]
fn design_md_state_section_reflects_stateful_flag() {
    let stateless = design_md_template(&sample_params());
    assert!(
        stateless.contains("Stateless"),
        "stateless module's state section must say so"
    );

    let mut p = sample_params();
    p.stateful = true;
    let stateful = design_md_template(&p);
    assert!(
        stateful.contains("Per-resource state stored in `DashMap"),
        "stateful module's state section must describe the lock pattern"
    );
}

// ── README template ──────────────────────────────────────────────

#[test]
fn readme_lists_declared_contract_and_three_files() {
    let s = readme_template(&sample_params());
    assert!(s.contains("# `demo` module"));
    assert!(s.contains("- `demo/echo`"));
    assert!(s.contains("- `demo/ping`"));
    // The README must mention all three scaffolded files so the
    // author knows what's there.
    assert!(s.contains("mod.rs"));
    assert!(s.contains("types.rs"));
    assert!(s.contains("DESIGN.md"));
    assert!(s.contains("pub mod demo;"));
    assert!(s.contains("DemoModule::new()"));
}

#[test]
fn readme_for_stateful_module_announces_stateful_status() {
    let mut p = sample_params();
    p.stateful = true;
    let s = readme_template(&p);
    assert!(
        s.contains("**Stateful**"),
        "stateful module's README must announce it: {s}"
    );
}

#[test]
fn readme_handles_empty_lists_gracefully() {
    let mut p = sample_params();
    p.commands.clear();
    p.events_subscribed.clear();
    p.events_published.clear();
    let s = readme_template(&p);
    assert!(s.contains("_None declared._"));
}

// ── naming helpers ───────────────────────────────────────────────

#[test]
fn command_to_type_stem_strips_module_prefix_and_pascals() {
    assert_eq!(command_to_type_stem("chat", "chat/poll"), "Poll");
    assert_eq!(
        command_to_type_stem("chat", "chat/analyze/findings"),
        "AnalyzeFindings"
    );
    assert_eq!(
        command_to_type_stem("ai", "ai/inference/start"),
        "InferenceStart"
    );
    // Without the module prefix, pascal the whole thing.
    assert_eq!(
        command_to_type_stem("chat", "collaboration/chat/poll"),
        "CollaborationChatPoll"
    );
    // Dash-separated parts convert cleanly too.
    assert_eq!(
        command_to_type_stem("ai-provider", "ai-provider/route"),
        "Route"
    );
}

#[test]
fn command_to_handler_name_strips_module_prefix_and_snakes() {
    assert_eq!(command_to_handler_name("chat", "chat/poll"), "handle_poll");
    assert_eq!(
        command_to_handler_name("chat", "chat/analyze/findings"),
        "handle_analyze_findings"
    );
    assert_eq!(
        command_to_handler_name("ai", "ai/inference/start"),
        "handle_inference_start"
    );
}
