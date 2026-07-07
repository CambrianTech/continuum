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
    /// The command to explain, e.g. `code/read`. OMIT it to get an INDEX of every
    /// command you can call (name + one-line description) — your starting point when
    /// you don't yet know which tool to use.
    #[serde(default)]
    #[ts(optional)]
    pub name: Option<String>,
}

/// Closest authorized command names to a miss — same category prefix (before `/`),
/// or sharing a path segment. Cheap, dependency-free, and enough to unstick an agent
/// that guessed a plausible-but-wrong name (e.g. `commands/describe` → `commands/help`).
pub(crate) fn did_you_mean<'a>(query: &str, authorized: &[&'a str]) -> Vec<&'a str> {
    let q = query.to_lowercase();
    let q_prefix = q.split('/').next().unwrap_or(&q);
    let q_segs: HashSet<&str> = q.split('/').filter(|s| !s.is_empty()).collect();
    let mut scored: Vec<(u8, &str)> = authorized
        .iter()
        .filter_map(|name| {
            let n = name.to_lowercase();
            let n_prefix = n.split('/').next().unwrap_or(&n);
            let shares_seg = n.split('/').any(|s| !s.is_empty() && q_segs.contains(s));
            let score = if n_prefix == q_prefix {
                2 // same category — strongest signal
            } else if shares_seg {
                1
            } else {
                return None;
            };
            Some((score, *name))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
    scored.into_iter().take(6).map(|(_, n)| n).collect()
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
pub(crate) fn render_ai_help(name: &str, description: &str, schema: &Value) -> String {
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
        // Everything THIS caller could actually run — the universe for both the index
        // and did-you-mean (never leak commands above the caller's access).
        let authorized: Vec<_> = command_registry()
            .into_iter()
            .filter(|d| is_command_authorized(d.name, trust))
            .collect();

        // No name → the INDEX: every command you can call, one line each. This is the
        // orientation an agent needs FIRST; erroring here (the old "missing field name")
        // dead-ends the very first discovery move.
        let Some(name) = p.name.as_deref().filter(|s| !s.trim().is_empty()) else {
            let mut lines: Vec<String> = authorized
                .iter()
                .map(|d| format!("- {} — {}", d.name, d.description))
                .collect();
            lines.sort();
            let manual = format!(
                "{} commands available to you. Call `commands/help` with a `name` for the \
                 exact tool-call format of any one.\n\n{}",
                lines.len(),
                lines.join("\n"),
            );
            return Ok(CommandsHelpResult {
                name: String::new(),
                description: "index of all callable commands".to_string(),
                access_level: "ai-safe".to_string(),
                manual,
            });
        };

        let Some(d) = authorized.iter().find(|d| d.name == name) else {
            // Unknown/unauthorized name → don't dead-end; suggest the nearest callable
            // commands so a plausible wrong guess still moves the agent forward.
            let names: Vec<&str> = authorized.iter().map(|d| d.name).collect();
            let suggestions = did_you_mean(name, &names);
            let hint = if suggestions.is_empty() {
                "Call `commands/help` with no name for the full index.".to_string()
            } else {
                format!("Did you mean: {}?", suggestions.join(", "))
            };
            return Err(CommandError::NotFound(format!(
                "no command '{name}' available to you (unknown, or above your access). {hint}"
            )));
        };

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
