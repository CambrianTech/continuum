//! Tool discovery — the seam that lets a persona's cognition cycle SEE
//! every command the substrate hosts. Per `[[commands-are-kernel-level-
//! and-compose]]`, commands are universal: every citizen
//! (persona / human / Claude / OpenClaw / sentinel) reaches them through
//! the same `Commands.execute()` primitive. The substrate does NOT gate
//! — there is no role-based authorization layer between a persona and
//! the registry. This module surfaces the full registry of typed
//! command schemas as `NativeToolSpec` so the cognition cycle's
//! `evaluate_response` can hand them to the model and let the model
//! decide what to call.
//!
//! # The role of role templates (priming, not authorization)
//!
//! When `RoleTemplate` grows a tool-related field, it is a **system-
//! prompt hint** — "you're a Coder, you typically work with code/* and
//! cargo/*" — not a permission gate. The model's cognition uses this to
//! bias its decision space. Any persona can still invoke any command:
//! if a Coder benefits from posting to `collaboration/chat/send`, it
//! does. The substrate runs every invocation through the same
//! audit-trailed path; outcomes are the record, not authorizations.
//!
//! # What lives here vs in `tool_executor`
//!
//! - `tool_discovery` (this file) — **describe** the surface. Pure walk
//!   of `ModuleRegistry`, returns a flat `Vec<NativeToolSpec>` shaped
//!   for `TextGenerationRequest.tools`. No execution, no IPC.
//! - `tool_executor` — **run** invocations. Receives `ContentPart::ToolUse`
//!   from the model's response, dispatches them (via the TS-IPC bridge
//!   today; future rust-native impl slots in behind the same trait).
//!
//! Discovery is read-only over the registry; execution is the
//! mutating, fan-out side. The two compose: discovery sets the model's
//! decision space, execution carries out what the model chose.
//!
//! # Why the conversion is mechanical
//!
//! `CommandSchema` and `NativeToolSpec` are isomorphic for the fields
//! that matter to a model: `name` + `description` + a JSON schema of
//! parameters. The translation is one-to-one with no judgment calls:
//! - `CommandSchema.name` → `NativeToolSpec.name`
//! - `CommandSchema.description` → `NativeToolSpec.description`
//! - `CommandSchema.params` → `ToolInputSchema` with each `ParamSchema`
//!   turned into a JSON Schema property entry.
//!
//! Kept pure (no async, no global state) so unit tests pin the
//! conversion without spinning up the runtime. The wiring caller
//! (in `cognition/generate_response.rs` and downstream) is responsible
//! for handing the registry in.

use std::sync::Arc;

use crate::ai::types::{NativeToolSpec, ToolInputSchema};
use crate::runtime::service_module::{CommandSchema, ParamSchema};
use crate::runtime::{ModuleRegistry, ServiceModule};

/// Walk every registered `ServiceModule` and emit a `NativeToolSpec`
/// for each command it declares. The substrate's universal command
/// surface — every citizen sees the same set.
///
/// Result is deterministic (modules listed in `ModuleRegistry`'s
/// `list_modules()` order; commands within a module in the order
/// `command_schemas()` returns them). Callers can stable-sort if they
/// need a different presentation; the cognition cycle handles
/// `Vec<NativeToolSpec>` as an unordered set so the natural ordering
/// is fine for production.
///
/// Cost: O(M × C) where M = number of modules, C = average commands
/// per module. Called once per cognition turn (or rarer, if the cycle
/// caches the result against registry version) — the registry walk is
/// cheap relative to a single inference.
pub fn discoverable_tools(registry: &ModuleRegistry) -> Vec<NativeToolSpec> {
    let mut specs = Vec::new();
    for module_name in registry.list_modules() {
        let Some(module) = registry.get_by_name(module_name) else {
            // Modules are inserted under their names; a None here is a
            // logic error in the registry itself (race between list +
            // get). Skip rather than panic — discovery is non-critical
            // path; the missing tool simply doesn't appear in the
            // turn's surface.
            continue;
        };
        collect_module_specs(&module, &mut specs);
    }
    specs
}

/// Pull a single module's command schemas into the running spec list.
/// Split from the public walker so a future caller wanting just one
/// module (e.g. an introspection probe targeting `code/*`) can reuse
/// the conversion without rebuilding the registry traversal.
fn collect_module_specs(module: &Arc<dyn ServiceModule>, out: &mut Vec<NativeToolSpec>) {
    for schema in module.command_schemas() {
        out.push(command_schema_to_native_tool(schema));
    }
}

/// Pure conversion: one `CommandSchema` → one `NativeToolSpec`. Kept
/// public so a generator / template / replay fixture can produce the
/// same shape the cognition cycle does, without dragging the registry
/// in.
///
/// JSON Schema shape:
/// - `"type": "object"` always (the Anthropic wire format requires
///   the top-level schema be an object descriptor).
/// - `"properties": { <name>: { "type": <param_type>, "description":
///   <description> } }` per `ParamSchema`.
/// - `"required": [<names>]` when any param's `required` is true,
///   omitted (kept `None`) otherwise so the wire format stays minimal.
pub fn command_schema_to_native_tool(schema: CommandSchema) -> NativeToolSpec {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for param in &schema.params {
        properties.insert(
            param.name.to_string(),
            serde_json::json!({
                "type": param.param_type,
                "description": param.description,
            }),
        );
        if param.required {
            required.push(param.name.to_string());
        }
    }
    NativeToolSpec {
        name: schema.name.to_string(),
        description: schema.description.to_string(),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::Value::Object(properties),
            required: if required.is_empty() {
                None
            } else {
                Some(required)
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A param with no required flag set must NOT appear in the
    /// `required` array — the JSON Schema is then minimal (no
    /// `required` field at all, kept `None` so wire serialization
    /// drops it via `skip_serializing_if = "Option::is_none"`).
    #[test]
    fn optional_only_params_emit_no_required_array() {
        let schema = CommandSchema {
            name: "data/list",
            description: "List entities in a collection.",
            params: vec![ParamSchema {
                name: "limit",
                param_type: "number",
                required: false,
                description: "Maximum number of entries to return.",
            }],
        };
        let spec = command_schema_to_native_tool(schema);
        assert_eq!(spec.name, "data/list");
        assert!(
            spec.input_schema.required.is_none(),
            "no required params → required field stays None: {:?}",
            spec.input_schema.required
        );
    }

    /// A mix of required + optional params must produce `required:
    /// Some([<required names>])` listing exactly the required ones in
    /// declaration order. Order matters for replay determinism; if a
    /// future refactor moves to a HashSet here, this test fails.
    #[test]
    fn required_params_listed_in_declaration_order() {
        let schema = CommandSchema {
            name: "code/edit",
            description: "Edit a file at the given path.",
            params: vec![
                ParamSchema {
                    name: "path",
                    param_type: "string",
                    required: true,
                    description: "Absolute path to the file.",
                },
                ParamSchema {
                    name: "encoding",
                    param_type: "string",
                    required: false,
                    description: "File encoding (default utf-8).",
                },
                ParamSchema {
                    name: "old_string",
                    param_type: "string",
                    required: true,
                    description: "The exact text to replace.",
                },
            ],
        };
        let spec = command_schema_to_native_tool(schema);
        assert_eq!(
            spec.input_schema.required,
            Some(vec!["path".to_string(), "old_string".to_string()]),
            "required array must list required params in declaration \
             order — replay determinism depends on it"
        );
    }

    /// Each `ParamSchema` appears as a `properties[<name>]` entry with
    /// `type` and `description`. The JSON Schema is the model's
    /// disposition surface; missing description hurts model decisions,
    /// missing type fails Anthropic API validation.
    #[test]
    fn properties_carry_type_and_description() {
        let schema = CommandSchema {
            name: "cargo/test",
            description: "Run cargo test, parse the JSON message stream.",
            params: vec![ParamSchema {
                name: "package",
                param_type: "string",
                required: false,
                description: "Package name to scope the test run.",
            }],
        };
        let spec = command_schema_to_native_tool(schema);
        let expected_props = json!({
            "package": {
                "type": "string",
                "description": "Package name to scope the test run.",
            }
        });
        assert_eq!(
            spec.input_schema.properties, expected_props,
            "properties must carry type + description per JSON Schema \
             contract Anthropic / OpenAI / DeepSeek consume"
        );
        assert_eq!(spec.input_schema.schema_type, "object");
    }

    /// Top-level shape must be `{type: "object", ...}` — every
    /// supported provider expects the root schema to declare itself
    /// an object descriptor, regardless of whether any params exist.
    #[test]
    fn empty_params_still_emit_object_schema() {
        let schema = CommandSchema {
            name: "health/ping",
            description: "Ping the runtime for liveness.",
            params: vec![],
        };
        let spec = command_schema_to_native_tool(schema);
        assert_eq!(spec.input_schema.schema_type, "object");
        assert_eq!(spec.input_schema.properties, json!({}));
        assert!(spec.input_schema.required.is_none());
    }

    /// The `name` and `description` strings on `NativeToolSpec` come
    /// directly from the schema with no munging — a model that has
    /// learned "code/edit" as a name keeps recognising it, and the
    /// substrate's audit trail can grep the same literal across the
    /// registry, the cognition turn, the tool invocation, and the
    /// outcome.
    #[test]
    fn name_and_description_pass_through_verbatim() {
        let schema = CommandSchema {
            name: "collaboration/chat/send",
            description: "Send a chat message to the named room.",
            params: vec![],
        };
        let spec = command_schema_to_native_tool(schema);
        assert_eq!(spec.name, "collaboration/chat/send");
        assert_eq!(spec.description, "Send a chat message to the named room.");
    }
}
