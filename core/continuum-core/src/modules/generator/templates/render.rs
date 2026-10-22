//! Private render helpers for the `generate/module` templates: list/array
//! renderers, stateful-scaffold fragments, and command-name conversions.

/// Render the `command_prefixes` array literal. Strategy: emit each
/// declared command exactly (e.g., `"chat/send"`) AND the module's
/// `name/` prefix so future commands under the same prefix route
/// through this module without re-running the generator. If the
/// caller declares no commands, the prefix alone is enough.
pub(super) fn render_command_prefixes(name: &str, commands: &[String]) -> String {
    let mut entries: Vec<String> = commands.iter().map(|c| format!("\"{c}\"")).collect();
    let prefix = format!("\"{name}/\"");
    if !entries.contains(&prefix) {
        entries.push(prefix);
    }
    entries.join(", ")
}

pub(super) fn render_string_array(items: &[String]) -> String {
    items
        .iter()
        .map(|s| format!("\"{s}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Generate the `use types::{...}` line at the top of mod.rs,
/// importing each declared command's typed Params + Result.
pub(super) fn render_typed_imports(name: &str, commands: &[String]) -> String {
    if commands.is_empty() {
        return String::new();
    }
    let mut types: Vec<String> = Vec::with_capacity(commands.len() * 2);
    for command in commands {
        let stem = command_to_type_stem(name, command);
        types.push(format!("{stem}Params"));
        types.push(format!("{stem}Result"));
    }
    format!("use types::{{{}}};\n", types.join(", "))
}

/// `ResourceState` struct declaration. Only emitted when stateful.
pub(super) fn render_resource_state_decl(stateful: bool) -> String {
    if !stateful {
        return String::new();
    }
    String::from(
        r#"
/// Per-resource state managed by this module. Authors add the fields
/// each handler reads/mutates. Wrapped in `tokio::sync::Mutex` inside
/// the module's per-resource lock map so concurrent access on the
/// same resource serializes (different resources stay parallel).
#[derive(Debug, Default)]
struct ResourceState {
    // TODO(author): add per-resource fields here.
}
"#,
    )
}

/// Per-resource lock map field on the module struct. Only emitted
/// when stateful.
pub(super) fn render_stateful_field(stateful: bool) -> String {
    if !stateful {
        return String::new();
    }
    String::from(
        "\n    /// Per-resource locks. Different ids stay parallel (DashMap\n    /// shards); same id serializes via `tokio::sync::Mutex` held\n    /// across the read-then-async-then-write window in handlers.\n    /// See [field manual §4.1](../../../../../../docs/architecture/COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md).\n    resource_locks: DashMap<String, Arc<tokio::sync::Mutex<ResourceState>>>,\n",
    )
}

/// Per-resource lock map initializer in the `new` / `with_executor`
/// bodies. Only emitted when stateful.
pub(super) fn render_stateful_init(stateful: bool) -> String {
    if !stateful {
        return String::new();
    }
    String::from("            resource_locks: DashMap::new(),\n")
}

/// `resource_lock(&self, id)` get-or-create helper. Only emitted
/// when stateful.
pub(super) fn render_stateful_helper(stateful: bool, _struct_prefix: &str) -> String {
    if !stateful {
        return String::new();
    }
    String::from(
        r#"
    /// Get-or-create the per-resource lock for `id`. `DashMap::entry`
    /// is atomic within a shard, so concurrent callers either find
    /// the same `Arc` (one wins the slot, others clone) or both
    /// create distinct `Arc`s for distinct ids (different shards
    /// stay parallel). See field manual §4.1.
    #[allow(dead_code)]
    fn resource_lock(&self, id: &str) -> Arc<tokio::sync::Mutex<ResourceState>> {
        self.resource_locks
            .entry(id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(ResourceState::default())))
            .clone()
    }
"#,
    )
}

/// One typed `&self` handler method per declared command. Each body
/// is a stub that errors with `not yet implemented` so the scaffold
/// compiles + the author fills in real logic afterwards.
pub(super) fn render_handler_methods(name: &str, commands: &[String]) -> String {
    if commands.is_empty() {
        return String::new();
    }
    let mut s = String::new();
    for command in commands {
        let stem = command_to_type_stem(name, command);
        let handler = command_to_handler_name(name, command);
        s.push_str(&format!(
            r#"
    /// Typed handler for `{command}`. Replace the `Err` body with
    /// real logic; the typed envelope wiring in `handle_command` is
    /// already in place.
    #[allow(dead_code, unused_variables)]
    pub async fn {handler}(
        &self,
        params: {stem}Params,
    ) -> Result<{stem}Result, String> {{
        Err("{command}: not yet implemented in this scaffolded module".to_string())
    }}
"#
        ));
    }
    s
}

/// Dispatch arms for `handle_command`. Each arm parses the typed
/// envelope, calls the typed handler method, materializes the
/// typed response.
pub(super) fn render_command_dispatch_arms(name: &str, commands: &[String]) -> String {
    if commands.is_empty() {
        return String::new();
    }
    commands
        .iter()
        .map(|command| {
            let stem = command_to_type_stem(name, command);
            let handler = command_to_handler_name(name, command);
            format!(
                "            \"{command}\" => {{\n                \
                 let req = CommandRequest::<{stem}Params>::from_value(params)?;\n                \
                 let result = self.{handler}(req.params).await?;\n                \
                 CommandResponse::ok(result).into_command_result()\n            \
                 }}"
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn render_published_events_doc(events: &[String]) -> String {
    if events.is_empty() {
        return String::new();
    }
    let mut s = String::from("//!\n//! Documented published events (no runtime wiring; \n//! publishers emit at their own pace):\n");
    for e in events {
        s.push_str(&format!("//!   - `{e}`\n"));
    }
    s
}

/// Multi-thread concurrency stress test scaffold per field manual §4.2.
/// When stateful, also includes a per-resource serialization sanity
/// check.
pub(super) fn render_concurrency_test(name: &str, struct_prefix: &str, stateful: bool) -> String {
    let extra_stateful_test = if stateful {
        format!(
            r#"
    /// Concurrent handlers on DIFFERENT resource ids must stay
    /// parallel; on the SAME id must serialize via the per-resource
    /// mutex. This test pins the parallel-different-ids half — the
    /// serializes-same-id half should be added once a real handler
    /// is in place.
    /// See field manual §4.1.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn resource_locks_stay_parallel_across_distinct_ids() {{
        let module = Arc::new({struct_prefix}Module::new());
        let mut tasks = Vec::new();
        for i in 0..16 {{
            let module = module.clone();
            tasks.push(tokio::spawn(async move {{
                // Acquiring distinct ids must not block each other.
                let lock = module.resource_lock(&format!("resource-{{i}}"));
                let _guard = lock.lock().await;
                // TODO(author): exercise the real handler under the lock.
            }}));
        }}
        for t in tasks {{
            t.await.expect("task must not panic");
        }}
        // 16 distinct ids ⇒ 16 distinct lock entries.
        assert_eq!(module.resource_locks.len(), 16);
    }}
"#
        )
    } else {
        String::new()
    };

    format!(
        r#"
// ════════════════════════════════════════════════════════════════
// Tests
// ════════════════════════════════════════════════════════════════
//
// The concurrency stress test below is mandatory per
// [field manual §4.2](../../../../../../docs/architecture/COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md).
// Single-threaded `#[tokio::test]` would silently serialize even
// genuinely racy code and pass; `flavor = "multi_thread",
// worker_threads = 4` actually preempts across OS threads so race
// windows open.
//
// Extend the test as you fill in real handler bodies — assert no
// losses, distinct ids, per-call ordering invariants, etc.

#[cfg(test)]
mod tests {{
    use super::*;

    #[tokio::test]
    async fn config_advertises_module_name_and_prefix() {{
        let m = {struct_prefix}Module::new();
        let cfg = m.config();
        assert_eq!(cfg.name, "{name}");
        assert!(
            cfg.command_prefixes.iter().any(|p| p == &"{name}/"),
            "module's own `name/` prefix must appear in command_prefixes"
        );
    }}

    /// Multi-thread concurrency smoke test. Scaffolded per field
    /// manual §4.2 — extend as real handlers land.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn handlers_under_concurrent_load() {{
        const PARALLEL: usize = 16;
        let module = Arc::new({struct_prefix}Module::new());
        let mut tasks = Vec::with_capacity(PARALLEL);
        for _ in 0..PARALLEL {{
            let module = module.clone();
            tasks.push(tokio::spawn(async move {{
                // TODO(author): replace with a real handler call once
                // the scaffold is filled in. The Arc<Module> here is
                // the production multi-persona usage pattern.
                module.config()
            }}));
        }}
        for t in tasks {{
            t.await.expect("task must not panic");
        }}
    }}
{extra_stateful_test}}}
"#
    )
}

pub(super) fn render_md_list(title: &str, items: &[String]) -> String {
    if items.is_empty() {
        format!("## {title}\n\n_None declared._")
    } else {
        let mut s = format!("## {title}\n\n");
        for it in items {
            s.push_str(&format!("- `{it}`\n"));
        }
        s
    }
}

pub(super) fn csv_or_none(items: &[String]) -> String {
    if items.is_empty() {
        "(none)".to_string()
    } else {
        items.join(", ")
    }
}

/// Convert a command like `chat/poll` (with module name `chat`) into
/// the canonical PascalCase type stem `Poll`. For nested commands
/// like `chat/analyze/findings`, produces `AnalyzeFindings`.
///
/// Strategy: strip the leading `<module>/` if present, then convert
/// the remainder to PascalCase (splitting on `/`, `-`, `_`).
pub(super) fn command_to_type_stem(module_name: &str, command: &str) -> String {
    let stripped = command
        .strip_prefix(&format!("{module_name}/"))
        .unwrap_or(command);
    pascal_case(stripped)
}

/// Convert a command like `chat/poll` into the canonical snake_case
/// handler name `handle_poll`. For nested commands like
/// `chat/analyze/findings`, produces `handle_analyze_findings`.
pub(super) fn command_to_handler_name(module_name: &str, command: &str) -> String {
    let stripped = command
        .strip_prefix(&format!("{module_name}/"))
        .unwrap_or(command);
    let snake = stripped.replace(['/', '-'], "_");
    format!("handle_{snake}")
}

pub(super) fn pascal_case(s: &str) -> String {
    s.split(['/', '-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<String>()
}
