//! `commands/help` — the AI-paradigm manual: how to CALL a command, in the exact
//! tool-call format the caller is expected to emit.
//!
//! Symmetry with the CLI: `cu <command> --help` renders the SAME single schema as
//! bash flags ("the manual matches the paradigm"); this renders it as the canonical
//! tool-call envelope a persona emits. One source (`command_registry()` + the
//! command's `params_schema`), two paradigms. So when a persona is unsure HOW to
//! call a tool, it asks `commands/help` and gets back a fill-in-the-blanks example —
//! and because that example IS the canonical format, it teaches the model toward the
//! shape the adapter prefers (the flexible parser accepts any format; help nudges
//! toward the best one). [[command-infra-self-routing-schema-adapters]]
//!
//! Gated to the caller's own trust: you only get help for commands you could
//! actually run (no teaching the format of an Owner-only command to a persona).

use std::collections::HashSet;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;

use crate::modules::grid::acl::is_command_authorized;
use crate::routing::grid_trust_policy::caller_trust;
use crate::sdk_codegen::{command_registry, AccessLevel, ActionCommand, CommandError, Ctx};

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CommandsHelpParams {
    /// The command to explain, e.g. `code/read`.
    pub name: String,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CommandsHelpResult {
    pub name: String,
    pub description: String,
    pub access_level: String,
    /// The how-to-call manual rendered in the caller's paradigm: the exact tool-call
    /// envelope to emit (fill in the values) plus per-argument docs.
    pub manual: String,
}

/// Render a command's manual in the AI paradigm: the canonical `{"tool_call": …}`
/// envelope (with placeholder values typed from the schema) + an argument list.
/// Pure function of (name, description, params JSON Schema) — same schema every
/// other interface adapts from.
fn render_ai_help(name: &str, description: &str, schema: &Value) -> String {
    let props = schema.get("properties").and_then(Value::as_object);
    let required: HashSet<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let mut example = serde_json::Map::new();
    let mut arg_lines: Vec<String> = Vec::new();
    if let Some(props) = props {
        for (key, spec) in props {
            let ty = spec.get("type").and_then(Value::as_str).unwrap_or("any");
            let req = required.contains(key.as_str());
            let doc = spec.get("description").and_then(Value::as_str).unwrap_or("");
            // Placeholder typed by the schema; a trailing `?` marks optional.
            example.insert(
                key.clone(),
                json!(format!("<{ty}{}>", if req { "" } else { "?" })),
            );
            arg_lines.push(format!(
                "- {key} ({ty}, {}){}",
                if req { "required" } else { "optional" },
                if doc.is_empty() { String::new() } else { format!(" — {doc}") },
            ));
        }
    }

    let envelope = json!({ "tool_call": { "name": name, "arguments": Value::Object(example) } });
    let envelope_str = serde_json::to_string_pretty(&envelope)
        .unwrap_or_else(|_| "{\"tool_call\": {\"name\": \"…\", \"arguments\": {}}}".to_string());

    let args_block = if arg_lines.is_empty() {
        "(no arguments)".to_string()
    } else {
        arg_lines.join("\n")
    };

    format!(
        "{name} — {description}\n\n\
         To call it, emit exactly this (fill in the values):\n{envelope_str}\n\n\
         Arguments:\n{args_block}"
    )
}

/// A persona's "how do I call this?" — returns the exact tool-call format for a
/// command it is authorized to run.
#[derive(Default)]
pub struct CommandsHelp;

#[async_trait]
impl ActionCommand for CommandsHelp {
    const NAME: &'static str = "commands/help";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Show how to CALL a command: the exact tool-call format to emit + its arguments. \
         Pass the command name (e.g. code/read). Use it when unsure how to invoke a tool.";
    type Params = CommandsHelpParams;
    type Output = CommandsHelpResult;

    async fn run(&self, ctx: &Ctx, p: CommandsHelpParams) -> Result<CommandsHelpResult, CommandError> {
        let trust = caller_trust(ctx.caller.as_ref());
        let d = command_registry()
            .into_iter()
            // Only help for what THIS caller could actually run (don't teach the
            // format of a command it isn't authorized to invoke).
            .find(|d| d.name == p.name && is_command_authorized(d.name, trust))
            .ok_or_else(|| {
                CommandError::NotFound(format!(
                    "no command '{}' available to you (unknown, or above your access)",
                    p.name
                ))
            })?;

        Ok(CommandsHelpResult {
            name: d.name.to_string(),
            description: d.description.to_string(),
            access_level: d.access_level.as_str().to_string(),
            manual: render_ai_help(d.name, d.description, &d.params_schema),
        })
    }
}

crate::register_stateless_command!(CommandsHelp);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the manual renders the CANONICAL tool-call envelope with
    // the command name + schema-typed argument placeholders — i.e. a persona gets a
    // fill-in-the-blanks example in the exact format the adapter prefers.
    #[test]
    fn renders_canonical_envelope_with_typed_args() {
        let schema = json!({
            "type": "object",
            "properties": {
                "file_path": {"type": "string", "description": "Path to read"},
                "start_line": {"type": "integer"}
            },
            "required": ["file_path"]
        });
        let manual = render_ai_help("code/read", "Read a file.", &schema);
        assert!(manual.contains("\"tool_call\""), "shows the envelope: {manual}");
        assert!(manual.contains("\"name\": \"code/read\""));
        assert!(manual.contains("file_path"));
        assert!(manual.contains("(string, required)"));
        assert!(manual.contains("(integer, optional)"));
    }

    // what this catches: a no-arg command renders cleanly (no panic, clear note).
    #[test]
    fn no_args_command_renders_cleanly() {
        let manual = render_ai_help("ping", "Health check.", &Value::Null);
        assert!(manual.contains("(no arguments)"), "{manual}");
        assert!(manual.contains("\"name\": \"ping\""));
    }
}
