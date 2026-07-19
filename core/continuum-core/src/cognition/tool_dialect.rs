//! tool_dialect — the ADAPTER between our command namespace and the tool-call
//! dialect models were trained on. [[joel-boundary-design-values]]: "always
//! adapters — meet the model ergonomically, never hardcode around it."
//!
//! ## The design (Joel, 2026-07-19): the command owns its own aliases
//!
//! A model reaches for the names it was trained on (`read_file`, `bash`, `grep`)
//! and for former names of tools that have since moved. Rather than a central
//! table that drifts, EACH command declares what it answers to — its
//! [`ALIASES`](crate::sdk_codegen::command::ActionCommand::ALIASES), right in its
//! own file. A command is then fully portable: rename or move it and its aliases
//! travel with it; no second source of truth to keep in sync.
//!
//! This module just AGGREGATES those per-command declarations into two generated
//! indices (built once, cached), and is the surface-agnostic core every entry
//! point shares — the persona tool-call path, the `cu` CLI, MCP. The mapping is
//! one thing; each surface renders it in its own native form.
//!
//! - [`from_wire_name`] — a wire tool-call name → the canonical command. Trained
//!   reflex / former name resolves; a canonical or unknown name passes through
//!   untouched (the adapter only ever WIDENS the surface).
//! - [`to_wire_spec`] — rename a spec to the model's primary reflex on OFFER
//!   (`code/read` → `read_file`), the name it acts on without learning a menu.
//!
//! A tool-call name two commands both claim is a build-time panic — the same
//! fail-loud the registry uses for duplicate command NAMES.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::ai::types::NativeToolSpec;
use crate::cognition::tool_usage::{record, Outcome};
use crate::sdk_codegen::command_registry;

/// alias → canonical command name. Built ONCE from every command's declared
/// `ALIASES`. A name claimed by two commands panics at init (fail-loud, like the
/// registry's duplicate-NAME guard) — an ambiguous alias is a bug, not a
/// silent last-writer-wins.
fn alias_to_command() -> &'static HashMap<&'static str, &'static str> {
    static IDX: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    IDX.get_or_init(|| {
        let mut m: HashMap<&'static str, &'static str> = HashMap::new();
        for d in command_registry() {
            for &alias in d.aliases {
                if let Some(prev) = m.insert(alias, d.name) {
                    panic!(
                        "tool_dialect: tool-call name '{alias}' is claimed by both '{prev}' \
                         and '{}' — a command's ALIASES must be unique across the registry.",
                        d.name
                    );
                }
            }
        }
        m
    })
}

/// canonical command name → its PRIMARY offered alias (the FIRST declared) — the
/// trained reflex we rename to on the wire. Commands with no alias are absent
/// (offered under their canonical name).
fn command_to_primary_alias() -> &'static HashMap<&'static str, &'static str> {
    static IDX: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    IDX.get_or_init(|| {
        command_registry()
            .iter()
            .filter_map(|d| d.aliases.first().map(|&alias| (d.name, alias)))
            .collect()
    })
}

/// Rename a spec to the wire dialect: the command's PRIMARY declared alias (the
/// model's trained reflex), so it acts by instinct instead of learning our name
/// from a menu (the discovery-tool trap: 14/14 acts on `commands/help`, zero
/// edits). Identity for a command with no alias — the long tail keeps its
/// canonical name (reachable, just not reflexive).
pub fn to_wire_spec(mut spec: NativeToolSpec) -> NativeToolSpec {
    if let Some(&alias) = command_to_primary_alias().get(spec.name.as_str()) {
        spec.name = alias.to_string();
    }
    spec
}

/// The set of canonical command names — for classifying a wire name as CANONICAL
/// (a real command) vs a MISS. Built once from the live registry.
fn command_names() -> &'static HashSet<&'static str> {
    static NAMES: OnceLock<HashSet<&'static str>> = OnceLock::new();
    NAMES.get_or_init(|| command_registry().iter().map(|d| d.name).collect())
}

/// Classify a wire tool-call name into (canonical command, how it resolved) —
/// the PURE core, no side effects. Resolves, in order: a declared reflex/former
/// alias; our canonical name as-is; our charset-legal name (`code_read` →
/// `code/read`). An unknown name passes through untouched (classified a MISS) —
/// the adapter widens the surface, never narrows it. Idempotent:
/// `classify(canonical) == (canonical, Canonical)`, so it's safe to apply at
/// every dispatch seam without changing an already-resolved name.
fn classify(wire: &str) -> (String, Outcome) {
    if let Some(&cmd) = alias_to_command().get(wire) {
        return (cmd.to_string(), Outcome::Alias);
    }
    if command_names().contains(wire) {
        return (wire.to_string(), Outcome::Canonical);
    }
    // Our charset-legal name (`code_read` → `code/read`) — the underscore form of a
    // real command. Resolving it HERE (not just in the executor's naive replace)
    // classifies it CANONICAL instead of a false miss.
    if wire.contains('_') {
        let slashed = wire.replace('_', "/");
        if command_names().contains(slashed.as_str()) {
            return (slashed, Outcome::Canonical);
        }
    }
    (wire.to_string(), Outcome::Miss)
}

/// Resolve a wire tool-call name to the canonical command WITHOUT tallying — the
/// shared resolver every dispatch seam (the `cu` CLI, the IPC/MCP socket route)
/// funnels through so a trained reflex / former name / charset-legal form resolves
/// the SAME way it does on the persona path. No recording here: these surfaces
/// carry infra traffic (`commands/list`, health pings) that would drown the
/// tool-call signal, and the persona path already tallies at [`from_wire_name`].
pub fn resolve_wire_name(wire: &str) -> String {
    classify(wire).0
}

/// Map a wire tool-call name back to the canonical command, and TALLY the outcome
/// (`cognition::tool_usage`) so a benchmark run surfaces exactly what each model
/// reached for. This is the RECORDING resolver — the persona tool-call path uses
/// it (every miss/alias/canonical is a training + ergonomics signal). Other
/// surfaces use [`resolve_wire_name`] (same mapping, no tally).
pub fn from_wire_name(wire: &str) -> String {
    let (name, outcome) = classify(wire);
    record(wire, outcome);
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the per-command declarations aggregate into a working
    // round-trip over the LIVE registry — a hot verb offers under its trained
    // reflex and maps back to the canonical command. This is the decentralized
    // replacement for the old global DIALECT table; if a command's ALIASES stops
    // being read, these break.
    #[test]
    fn per_command_aliases_round_trip_through_the_registry() {
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
        // The hot verbs declared on their commands: offer under the reflex, map back.
        for (canonical, reflex) in [
            ("code/shell", "bash"),
            ("code/read", "read_file"),
            ("code/write", "write_file"),
            ("code/run", "run_code"),
            ("code/list", "list_files"),
            ("work/claim", "claim_task"),
        ] {
            assert_eq!(to_wire_spec(spec(canonical)).name, reflex, "offer {canonical} as {reflex}");
            assert_eq!(from_wire_name(reflex), canonical, "map {reflex} back to {canonical}");
        }
        // A canonical name a model emits directly still resolves (never narrows).
        assert_eq!(from_wire_name("code/read"), "code/read");
        // An unknown name passes through — the executor fails it loud with a
        // did-you-mean; the adapter never invents a route.
        assert_eq!(from_wire_name("frobnicate"), "frobnicate");
    }

    // what this catches: the socket/CLI resolver (used at Runtime::route_command and
    // the `cu` entry) maps IDENTICALLY to the persona path — alias, canonical
    // (idempotent), charset-legal, unknown-passthrough — so `cu` / IPC / MCP accept
    // the same vocabulary a persona does. Non-recording is a structural guarantee
    // (resolve_wire_name == classify().0, and classify never calls record), so it's
    // asserted by construction, not by racy global-tally inspection.
    #[test]
    fn resolve_wire_name_maps_like_the_persona_path() {
        assert_eq!(resolve_wire_name("read_file"), "code/read"); // trained alias
        assert_eq!(resolve_wire_name("code/read"), "code/read"); // canonical, idempotent
        assert_eq!(resolve_wire_name("code_read"), "code/read"); // charset-legal
        assert_eq!(resolve_wire_name("frobnicate"), "frobnicate"); // unknown passes through
    }

    // what this catches: the index is NON-VACUOUS — the migration actually landed
    // aliases on commands. A regression that dropped the ALIASES consts (or the
    // descriptor plumbing) would empty this and silently stop all reflex mapping.
    #[test]
    fn the_alias_index_is_populated() {
        assert!(
            alias_to_command().len() >= 10,
            "expected the hot-verb aliases from the migrated commands, got {}",
            alias_to_command().len()
        );
        // Spot-check the highest-frequency live reflexes (mined 2026-07-19).
        assert_eq!(from_wire_name("file_tree"), "code/tree");
        assert_eq!(from_wire_name("read_file"), "code/read");
    }
}
