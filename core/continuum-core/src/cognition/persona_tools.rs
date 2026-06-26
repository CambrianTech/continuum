//! Dynamic persona tool surface — discovered, never hardcoded.
//!
//! A persona's hands are the registry's `AiSafe` commands. The tool list is a
//! PURE FUNCTION of `command_registry() × access_level`: every command that
//! declares [`AccessLevel::AiSafe`] becomes a tool the persona can call, and a
//! new ai-safe command appears → the persona can use it with zero code change.
//! There is NO hardcoded tool list and NO parallel allow-table — the command's
//! own `access_level` (its "destiny") is the single, consistent source of truth,
//! surfaced through the one central listing [`command_registry`].
//!
//! ## The bug this shape refuses to ship (Joel 2026-06-21)
//! "The tool's gone and no one knows why — RAG says it has it, some smell
//! somewhere restricted it." Because the surface is exactly
//! `command_registry().filter(AiSafe)`, "why isn't tool X here?" has ONE answer:
//! its `access_level` isn't `AiSafe`. [`tool_surface_report`] makes that a
//! one-look diagnosis (included / excluded + the reason), never a hunt.
//!
//! ## Metadata maturity — mechanism in place, richness later
//! A [`CommandDescriptor`] today carries `name` + `access_level` + the param
//! TYPE ref — enough to know WHICH commands are tools and to name them, but not
//! a human description or a JSON param schema. So a tool's description is
//! best-effort (command + param type) and its `input_schema` is an open object
//! (the command validates its own typed params). When commands declare richer
//! tool-facing metadata — a description and a param schema, the next mechanism,
//! living in the command's own destiny — [`descriptor_to_tool_spec`] consumes it
//! with no caller change. Open-by-default now; advanced description/validation
//! later, exactly where it belongs.

use crate::ai::types::{NativeToolSpec, ToolInputSchema};
use crate::modules::grid::acl::is_command_authorized;
use crate::modules::grid::node::TrustLevel;
use crate::sdk_codegen::{command_registry, AccessLevel, CommandDescriptor};
use serde_json::json;

/// The persona's tool surface: every command it is AUTHORIZED to run at `trust`,
/// projected to a tool spec. **Offer == authorized, by construction** — a persona
/// is NEVER shown a tool the gate would refuse (no "offer ping then deny it"). The
/// SAME [`is_command_authorized`] the executor enforces decides what's offered, so
/// the two can't drift, and opening a command to a trust level auto-adds it here.
/// Dynamic from the live [`command_registry`]; nothing hardcoded.
pub fn authorized_tool_specs(trust: TrustLevel) -> Vec<NativeToolSpec> {
    command_registry()
        .iter()
        .filter(|d| is_command_authorized(d.name, trust))
        .map(descriptor_to_tool_spec)
        .collect()
}

/// The raw `AiSafe`-by-declaration surface, IGNORING caller identity and the grid
/// ACL overrides. This is NOT a persona tool surface and must never become one:
/// it can over-list (a command declared `AiSafe` but bumped to `Owner` by an
/// explicit ACL rule would appear here yet be denied at the gate) — exactly the
/// "listed a tool I can't call" violation Joel forbids. Gated `#[cfg(test)]` so no
/// production path can reach it: the ONLY way to get a persona's tools is the
/// identity-gated [`authorized_tool_specs`]. Kept solely to assert the projection
/// (descriptor → spec) against the registry in tests.
#[cfg(test)]
pub fn ai_safe_tool_specs() -> Vec<NativeToolSpec> {
    command_registry()
        .iter()
        .filter(|d| d.access_level == AccessLevel::AiSafe)
        .map(descriptor_to_tool_spec)
        .collect()
}

/// Project one command descriptor to an LLM tool spec. The tool NAME is the
/// command name (the executor maps it straight back to a command). Description +
/// schema are best-effort until the descriptor carries richer metadata (see
/// module docs); this projection is structured so that richness lands here
/// without changing any caller.
pub fn descriptor_to_tool_spec(d: &CommandDescriptor) -> NativeToolSpec {
    NativeToolSpec {
        name: d.name.to_string(),
        // The command's own declared DESCRIPTION (headless, compartmentalized) when
        // present; otherwise fall back to a name + param-type handle so the model
        // still has something. A command becomes a GOOD tool simply by declaring
        // `const DESCRIPTION` in its own file — no change here.
        description: if d.description.is_empty() {
            format!("Command `{}` (params: {}).", d.name, d.params.name)
        } else {
            d.description.to_string()
        },
        // The command's REAL param schema (derived automatically from its Rust
        // type by the base traits) becomes the tool's `input_schema` — so the
        // reasoner sees exactly what fields a tool takes, same schema every other
        // SDK adapts from. Commands not yet on a base trait carry a `Null` schema;
        // those fall back to an open object (the command still validates its typed
        // params). One source, every interface ([[command-organization]]).
        input_schema: tool_input_schema_from(&d.params_schema),
    }
}

/// Project a command's params JSON Schema into the LLM [`ToolInputSchema`]. A
/// `Null` schema (command not yet on a base trait) → an open object. Otherwise
/// lift `type`/`properties`/`required` — AND the `definitions`/`$defs` map — from
/// the derived schema.
///
/// Nested-param commands (`code/edit` → `EditMode`, `data/list` → `OrderByClause`,
/// `rag/load` → the self-referential `RagSourceRequest`, …) make schemars emit
/// `$ref: "#/definitions/<Name>"` in `properties` plus a sibling `definitions`
/// map. Both MUST ship: the backend resolves each ref against the carried map.
/// Drop the map and llama.cpp rejects the whole turn with a 400 ("definitions not
/// in {…}") — the bug that kept every tool-enabled persona turn silent until the
/// command-registry migration exposed the first nested-param tool.
fn tool_input_schema_from(schema: &serde_json::Value) -> ToolInputSchema {
    if schema.is_null() {
        return ToolInputSchema {
            schema_type: "object".to_string(),
            properties: json!({}),
            required: None,
            definitions: None,
        };
    }
    ToolInputSchema {
        schema_type: schema
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("object")
            .to_string(),
        properties: schema
            .get("properties")
            .cloned()
            .unwrap_or_else(|| json!({})),
        required: schema.get("required").and_then(|v| v.as_array()).map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        }),
        // Carry the nested-type definitions under the key the refs name. schemars
        // (draft-07) emits `definitions`; tolerate `$defs` (2020-12) too.
        definitions: schema
            .get("definitions")
            .or_else(|| schema.get("$defs"))
            .cloned(),
    }
}

/// What's in the persona's tool surface and WHY — the anti-"tool vanished"
/// diagnostic. One-look answer to "what can the persona call, and why is X in or
/// out," derived from the same single source of truth as the surface itself.
pub fn tool_surface_report() -> ToolSurfaceReport {
    let mut included = Vec::new();
    let mut excluded = Vec::new();
    for d in command_registry() {
        match d.access_level {
            AccessLevel::AiSafe => included.push(d.name.to_string()),
            other => excluded.push((d.name.to_string(), other)),
        }
    }
    included.sort();
    excluded.sort_by(|a, b| a.0.cmp(&b.0));
    ToolSurfaceReport { included, excluded }
}

/// Inspectable tool-surface snapshot.
#[derive(Debug, Clone)]
pub struct ToolSurfaceReport {
    /// Command names the persona CAN call (`AccessLevel::AiSafe`).
    pub included: Vec<String>,
    /// Commands EXCLUDED, each with the access level that excluded it
    /// (`Privileged` / `Internal`) — the single reason, no hunting.
    pub excluded: Vec<(String, AccessLevel)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the tool surface is DYNAMIC and consistent — it is
    // exactly the AiSafe slice of command_registry, nothing hardcoded. If a
    // command's access_level changes, its tool presence changes with it, no code
    // edit. Regression here = the surface drifting from the single source of
    // truth (the exact "tool vanished / appeared and no one knows why" bug).
    #[test]
    fn tool_surface_is_exactly_the_ai_safe_slice_of_the_registry() {
        let specs = ai_safe_tool_specs();
        let report = tool_surface_report();

        // Every spec corresponds to an AiSafe command; counts agree with the
        // registry's own AiSafe count — no hidden inclusion/exclusion.
        let registry_ai_safe = command_registry()
            .iter()
            .filter(|d| d.access_level == AccessLevel::AiSafe)
            .count();
        assert_eq!(specs.len(), registry_ai_safe, "surface == registry AiSafe count");
        assert_eq!(report.included.len(), registry_ai_safe);

        // Included and excluded partition the WHOLE registry — every command is
        // accounted for, so "why isn't X here" always has an answer.
        let total = command_registry().len();
        assert_eq!(
            report.included.len() + report.excluded.len(),
            total,
            "every command is either included or excluded — no silent drops"
        );

        // The spec names ARE command names (the executor maps them straight back).
        for spec in &specs {
            assert!(
                command_registry().iter().any(|d| d.name == spec.name),
                "tool {} must be a real command",
                spec.name
            );
        }
    }

    // what this catches: descriptor → tool-spec projection keeps the command
    // name verbatim (the executor dispatches on it) and emits a usable open
    // schema. When richer metadata lands, this is the one place it flows in.
    #[test]
    fn projection_preserves_command_name_and_emits_open_schema() {
        // Pick any real AiSafe descriptor from the registry (don't hardcode one).
        let registry = command_registry();
        let Some(d) = registry
            .iter()
            .find(|d| d.access_level == AccessLevel::AiSafe)
        else {
            // No AiSafe commands compiled into this build — nothing to assert.
            return;
        };
        let spec = descriptor_to_tool_spec(d);
        assert_eq!(spec.name, d.name, "tool name is the command name verbatim");
        assert_eq!(spec.input_schema.schema_type, "object");
        assert!(!spec.description.is_empty(), "tool carries a description handle");
    }

    // what this catches: a nested-param command (schemars emits `$ref:
    // "#/definitions/<Name>"` + a sibling `definitions` map) MUST ship that map on
    // the projected tool schema. Dropping it leaves dangling refs and llama.cpp
    // 400s the whole turn ("definitions not in {…}") — the bug that kept every
    // tool-enabled persona turn silent. If a property references definitions, the
    // map travels; the two are never split.
    #[test]
    fn nested_param_schema_carries_its_definitions() {
        // Find any registered command whose derived params schema has a
        // `definitions`/`$defs` map (code/edit, data/list, rag/load, …). If none
        // is compiled into this build, there is nothing to assert.
        let registry = command_registry();
        let Some(d) = registry.iter().find(|d| {
            d.params_schema.get("definitions").is_some() || d.params_schema.get("$defs").is_some()
        }) else {
            return;
        };
        let spec = descriptor_to_tool_spec(d);
        let defs = spec
            .input_schema
            .definitions
            .as_ref()
            .unwrap_or_else(|| panic!("command {} has nested definitions that were dropped", d.name))
            .as_object()
            .expect("definitions is a JSON object map");
        assert!(!defs.is_empty(), "definitions map for {} must not be empty", d.name);

        // Every `#/definitions/<Name>` referenced in the serialized properties has
        // a matching key in the carried map — no dangling ref a backend can't
        // resolve (the exact shape that 400'd the turn).
        let props = spec.input_schema.properties.to_string();
        for fragment in props.split("#/definitions/").skip(1) {
            let name: String = fragment
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            assert!(
                defs.contains_key(&name),
                "command {} references #/definitions/{} but the carried map lacks it",
                d.name,
                name
            );
        }
    }
}
