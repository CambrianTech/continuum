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
            let req = required.contains(key.as_str());
            let doc = spec.get("description").and_then(Value::as_str).unwrap_or("");
            // Resolve the param's real shape — a scalar `type`, OR an enum (`oneOf`/
            // `anyOf`, possibly behind a `$ref`/`allOf`) whose variants we EXPAND into
            // a hint + a concrete example. Without this, a complex param (e.g. an
            // `EditMode` enum) collapsed to a useless `"any"`, so a model literally
            // could not tell what to pass — the invisible-contract bug.
            let (ty, placeholder) = param_shape(spec, schema);
            example.insert(key.clone(), placeholder);
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

/// Resolve a schema node that may be a `$ref` (or a single-element `allOf` wrapping one —
/// schemars' usual shape for a named type) against the root schema's `definitions`/`$defs`.
/// Returns the node unchanged if it isn't a ref or the target is missing.
fn resolve_ref<'a>(spec: &'a Value, root: &'a Value) -> &'a Value {
    if let Some(items) = spec.get("allOf").and_then(Value::as_array) {
        if items.len() == 1 {
            return resolve_ref(&items[0], root);
        }
    }
    if let Some(r) = spec.get("$ref").and_then(Value::as_str) {
        let name = r.rsplit('/').next().unwrap_or("");
        for defs in ["definitions", "$defs"] {
            if let Some(d) = root.get(defs).and_then(|d| d.get(name)) {
                return d;
            }
        }
    }
    spec
}

/// Describe a parameter's shape for help: a human type hint + a concrete placeholder
/// example. Scalars → `("string", "<string>")`. Externally-tagged enums (`oneOf`/`anyOf`,
/// possibly behind a `$ref`) → `("one of: A{fields} | B{fields}", <first-variant example>)`
/// so a model SEES the variants instead of a blind `"any"` (the invisible-contract bug that
/// left `code/edit`'s `edit_mode` uncallable). Graceful `any` fallback on any unknown shape.
fn param_shape(spec: &Value, root: &Value) -> (String, Value) {
    let spec = resolve_ref(spec, root);
    if let Some(ty) = spec.get("type").and_then(Value::as_str) {
        if let Some(en) = spec.get("enum").and_then(Value::as_array) {
            let opts: Vec<String> = en.iter().filter_map(Value::as_str).map(str::to_string).collect();
            if !opts.is_empty() {
                return (format!("one of: {}", opts.join(" | ")), Value::String(opts[0].clone()));
            }
        }
        return (ty.to_string(), json!(format!("<{ty}>")));
    }
    for tag in ["oneOf", "anyOf"] {
        if let Some(variants) = spec.get(tag).and_then(Value::as_array) {
            let mut names: Vec<String> = Vec::new();
            let mut example = json!("<any>");
            for (i, v) in variants.iter().enumerate() {
                let v = resolve_ref(v, root);
                let props = match v.get("properties").and_then(Value::as_object) {
                    Some(p) => p,
                    None => {
                        // a bare `const`/`enum` string variant (plain unit enum)
                        if let Some(c) = v.get("const").and_then(Value::as_str) {
                            names.push(format!("\"{c}\""));
                            if i == 0 {
                                example = json!(c);
                            }
                        }
                        continue;
                    }
                };
                // INTERNALLY-tagged (serde tag = "type"): a discriminator property whose
                // schema is a single-value const/enum string is the variant NAME; the other
                // properties are its fields, and the call is a FLAT object carrying the tag.
                let discr = props.iter().find_map(|(pk, pv)| {
                    let pv = resolve_ref(pv, root);
                    pv.get("const")
                        .and_then(Value::as_str)
                        .or_else(|| {
                            pv.get("enum")
                                .and_then(Value::as_array)
                                .filter(|a| a.len() == 1)
                                .and_then(|a| a[0].as_str())
                        })
                        .map(|val| (pk.clone(), val.to_string()))
                });
                if let Some((tag_field, vname)) = discr {
                    let fields: Vec<String> =
                        props.keys().filter(|k| **k != tag_field).cloned().collect();
                    names.push(if fields.is_empty() {
                        vname.clone()
                    } else {
                        format!("{vname}{{{}}}", fields.join(", "))
                    });
                    if i == 0 {
                        let mut inner = serde_json::Map::new();
                        inner.insert(tag_field.clone(), json!(vname.clone()));
                        for (fk, fv) in props.iter().filter(|(k, _)| **k != tag_field) {
                            let ft = resolve_ref(fv, root)
                                .get("type")
                                .and_then(Value::as_str)
                                .unwrap_or("any");
                            inner.insert(fk.clone(), json!(format!("<{ft}>")));
                        }
                        example = Value::Object(inner);
                    }
                } else if props.len() == 1 {
                    // EXTERNALLY-tagged: the single property key IS the variant name.
                    let (vname, vschema) = props.iter().next().unwrap();
                    let vs = resolve_ref(vschema, root);
                    let fields: Vec<String> = vs
                        .get("properties")
                        .and_then(Value::as_object)
                        .map(|o| o.keys().cloned().collect())
                        .unwrap_or_default();
                    names.push(if fields.is_empty() {
                        vname.clone()
                    } else {
                        format!("{vname}{{{}}}", fields.join(", "))
                    });
                    if i == 0 {
                        let mut inner = serde_json::Map::new();
                        if let Some(o) = vs.get("properties").and_then(Value::as_object) {
                            for (fk, fv) in o {
                                let ft = resolve_ref(fv, root)
                                    .get("type")
                                    .and_then(Value::as_str)
                                    .unwrap_or("any");
                                inner.insert(fk.clone(), json!(format!("<{ft}>")));
                            }
                        }
                        example = json!({ vname.clone(): Value::Object(inner) });
                    }
                }
            }
            if !names.is_empty() {
                return (format!("one of: {}", names.join(" | ")), example);
            }
        }
    }
    ("any".to_string(), json!("<any>"))
}

#[cfg(test)]
mod param_shape_tests {
    use super::*;
    use serde_json::json;

    // what this catches: an externally-tagged enum param (code/edit's EditMode) is EXPANDED
    // into its variants + a concrete example instead of collapsing to a useless "any" — the
    // invisible-contract bug I hit firsthand taking the SWE-bench test through cu (a model
    // literally could not tell what edit_mode wanted). Fixture mirrors schemars output
    // ($ref → definitions with a oneOf of {Variant:{fields}}).
    #[test]
    fn help_expands_enum_param_instead_of_any() {
        // EditMode is #[serde(tag = "type", rename_all = "snake_case")] — INTERNALLY tagged.
        // schemars renders each variant as an object with a single-enum `type` discriminator
        // plus the variant's fields; the real call is a FLAT `{"type":"search_replace", ...}`.
        let schema = json!({
            "type": "object",
            "required": ["file_path", "edit_mode"],
            "properties": {
                "file_path": {"type": "string", "description": "path"},
                "edit_mode": {"$ref": "#/definitions/EditMode"}
            },
            "definitions": { "EditMode": {"oneOf": [
                {"type":"object","required":["type","search","replace"],"properties":{"type":{"type":"string","enum":["search_replace"]},"search":{"type":"string"},"replace":{"type":"string"},"all":{"type":"boolean"}}},
                {"type":"object","required":["type","content"],"properties":{"type":{"type":"string","enum":["append"]},"content":{"type":"string"}}}
            ]}}
        });
        let out = render_ai_help("code/edit", "edit a file", &schema);
        assert!(out.contains("search_replace{"), "variant NAME (not field) + fields shown: {out}");
        assert!(out.contains("search") && out.contains("replace"), "variant fields shown: {out}");
        assert!(out.contains("append"), "second variant shown: {out}");
        assert!(!out.contains("edit_mode (any"), "no longer collapses to any: {out}");
        // the example is a FLAT object carrying the discriminator — the shape that actually works
        assert!(out.contains("\"type\"") && out.contains("\"search_replace\""), "flat tagged example: {out}");
    }

    // what this catches: a plain scalar param still renders as before (no regression).
    #[test]
    fn scalar_param_unchanged() {
        let schema = json!({"type":"object","required":["p"],"properties":{"p":{"type":"string"}}});
        let out = render_ai_help("x", "y", &schema);
        assert!(out.contains("p (string, required)"), "{out}");
    }
}

/// A persona's "how do I call this?" — returns the exact tool-call format for a
/// command it is authorized to run.
#[derive(Default)]
pub struct CommandsHelp;

#[async_trait]
impl ActionCommand for CommandsHelp {
    const NAME: &'static str = "commands/help";
    const NATIVE: bool = true; // discovery pair — the on-demand "how do I call this?" tool
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
