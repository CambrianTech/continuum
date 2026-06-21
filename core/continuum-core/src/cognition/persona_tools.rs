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
use crate::sdk_codegen::{command_registry, AccessLevel, CommandDescriptor};
use serde_json::json;

/// The persona's tool surface: every `AiSafe` command, projected to a tool spec.
/// Dynamic — derived from the live [`command_registry`], nothing hardcoded.
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
        // Open object — the command validates its own typed params. A declared
        // param JSON schema replaces this when the metadata mechanism lands.
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: json!({}),
            required: None,
        },
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
}
