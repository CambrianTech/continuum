//! tool_dialect — the ADAPTER between our command namespace and the tool-call
//! dialect models were actually trained on. [[joel-boundary-design-values]]:
//! "always adapters — meet the model ergonomically, never hardcode around it."
//!
//! ## Why (Joel, 2026-07-10: "our tools are a foreign language; theirs is the
//! model's native tongue")
//!
//! Two facts about every tool-trained model (Devstral/OpenHands, Qwen-Coder,
//! Hermes, the cloud models we cannot fine-tune):
//!
//! 1. **The OpenAI function-name convention is `[a-zA-Z0-9_-]{1,64}`.** Our
//!    command names carry slashes (`code/run`, `commands/help`) — a shape no
//!    model ever saw inside a `tools` array during training, and one the spec
//!    those models were trained against does not even allow.
//! 2. **The hot verbs have conventional names** — `bash`, `read_file`,
//!    `write_file`, `edit_file`, `grep` — burned in by OpenHands/SWE-agent-style
//!    scaffolds. A model reaches for `bash` by reflex; `code/shell` it must
//!    learn from a menu (the discovery-tool trap: 14/14 acts on `commands/help`,
//!    zero edits).
//!
//! So the WIRE speaks the model's dialect and the SUBSTRATE keeps its canonical
//! names: specs are renamed on OFFER ([`to_wire_spec`]), calls are mapped back
//! on RETURN ([`from_wire_name`]) before authorization/execution — the same
//! trivial-adapter shape the legacy Node personas used. Internal representation
//! stays OpenAI-shaped (`name/arguments/input_schema`) end to end.
//!
//! Canonical names remain first-class on the wire too: [`from_wire_name`] passes
//! unknown names through untouched, so `code/run` still resolves if a model says
//! it (MORE surface, never less). One table, one place — the compression rule.

use crate::ai::types::NativeToolSpec;

/// Our canonical command name ↔ the conventional tool-call alias models were
/// trained on. ONE table read by both directions; adding a hot verb is one row.
/// Aliases must match `[a-zA-Z0-9_-]+` (the OpenAI function-name charset).
const DIALECT: &[(&str, &str)] = &[
    ("code/shell", "bash"),
    ("code/read", "read_file"),
    ("code/write", "write_file"),
    ("code/edit", "edit_file"),
    ("code/search", "grep"),
    ("code/list", "list_files"),
    ("code/tree", "file_tree"),
    ("code/run", "run_code"),
    ("code/git/diff", "git_diff"),
    ("code/git/status", "git_status"),
    ("code/git/add", "git_add"),
    ("code/git/commit", "git_commit"),
    ("code/git/apply", "git_apply"),
    ("interface/screenshot", "screenshot"),
    ("commands/list", "list_commands"),
    ("commands/help", "help"),
    ("work/claim", "claim_task"),
];

/// Rename a spec to the wire dialect. Identity for commands with no alias —
/// the long tail keeps its canonical name (reachable, just not reflexive).
pub fn to_wire_spec(mut spec: NativeToolSpec) -> NativeToolSpec {
    if let Some((_, alias)) = DIALECT.iter().find(|(ours, _)| *ours == spec.name) {
        spec.name = (*alias).to_string();
    }
    spec
}

/// Map a wire tool-call name back to the canonical command. Pass-through for
/// names that aren't aliases (canonical names, the long tail) — the adapter
/// widens the surface, never narrows it.
pub fn from_wire_name(wire: &str) -> &str {
    DIALECT
        .iter()
        .find(|(_, alias)| *alias == wire)
        .map(|(ours, _)| *ours)
        .unwrap_or(wire)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the adapter contract — hot verbs offer under their
    // trained conventional names (charset-legal per the OpenAI function-name
    // spec), wire calls map back to canonical commands, and BOTH canonical and
    // unknown names pass through untouched (the surface only ever widens).
    #[test]
    fn dialect_round_trips_and_never_narrows() {
        let spec = |n: &str| NativeToolSpec {
            name: n.to_string(),
            description: String::new(),
            input_schema: crate::ai::types::ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: None,
                definitions: None,
            },
        };
        // Hot verb: renamed on offer, mapped back on return.
        assert_eq!(to_wire_spec(spec("code/shell")).name, "bash");
        assert_eq!(from_wire_name("bash"), "code/shell");
        // Long tail: identity both ways.
        assert_eq!(to_wire_spec(spec("cognition/eval")).name, "cognition/eval");
        assert_eq!(from_wire_name("cognition/eval"), "cognition/eval");
        // A model emitting the canonical name still resolves.
        assert_eq!(from_wire_name("code/shell"), "code/shell");
        // Every alias is charset-legal for the OpenAI function-name convention
        // — no slash ever reaches the wire from this table.
        for (_, alias) in DIALECT {
            assert!(
                alias.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
                "alias {alias} breaks the trained charset"
            );
        }
        // Full table round-trips.
        for (ours, alias) in DIALECT {
            assert_eq!(to_wire_spec(spec(ours)).name, *alias);
            assert_eq!(from_wire_name(alias), *ours);
        }
    }
}
