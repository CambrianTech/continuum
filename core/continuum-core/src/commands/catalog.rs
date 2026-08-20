//! `commands/list` — the live command catalog, from the ONE registry.
//!
//! Discovery is dynamic and single-source: this returns a snapshot of
//! `command_registry()` (the inventory-assembled descriptor list every other
//! surface reads). No client, tray, or CLI hardcodes a command list — they call
//! `commands/list` and adapt. Removing/renaming a command updates its one file and
//! this output follows automatically.
//!
//! It is itself a zero-ceremony STATELESS command (`register_stateless_command!`),
//! so it dogfoods the very thing it lists: declared in one file, live on the typed
//! path with no host module.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::grid::acl::is_command_authorized;
use crate::routing::caller_trust;
use crate::sdk_codegen::{command_registry, ActionCommand, CommandError, Ctx, WireShape};

/// Params for `commands/list` — an optional case-insensitive name substring to
/// filter by (so a tray can ask "what `data/*` commands exist?"). Empty ⇒ all.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/commands/CommandsListParams.ts"
)]
pub struct CommandsListParams {
    /// Optional substring filter on the command name (case-insensitive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub filter: Option<String>,
}

/// One command's discovery record — enough for any interface to surface and
/// adapt to it (name to call, what it does, what it needs, how it's gated).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/commands/CommandInfo.ts"
)]
pub struct CommandInfo {
    /// The command name — the routing key you call (`uu <name>`).
    pub name: String,
    /// Model/human-facing description (the command's own `DESCRIPTION`).
    pub description: String,
    /// Declared access level (`ai-safe` / `privileged` / `internal`).
    pub access_level: String,
    /// Wire shape (`bare` / `enveloped` / `provided`).
    pub wire: String,
    /// The params type name — the canonical schema every interface adapts to.
    pub params_type: String,
    /// The params' JSON Schema (derived from the Rust type), or `null` if the
    /// command hasn't declared one yet. THE single source every SDK/interface
    /// adapts from — CLI flags, web forms, mobile pickers, AI tool `input_schema`,
    /// and `uu <cmd> --help`.
    #[ts(type = "unknown")]
    pub params_schema: serde_json::Value,
}

/// Result of `commands/list` — the live catalog.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/commands/CommandsListResult.ts"
)]
pub struct CommandsListResult {
    /// How many commands matched — declared FIRST so it serializes at the head of
    /// the JSON (`{"total":N,...}`). A broad `commands/list` result is large and the
    /// act→observe fold truncates it to `RESULT_FOLD_MAX_CHARS`; putting the count
    /// first means "how many commands are available?" is answerable straight from the
    /// result head even when the `commands` array is clipped. Compute-once, present
    /// legibly (the compression principle) — never make the reader tally the array,
    /// which a smaller persona model cannot do reliably from a truncated dump.
    #[ts(type = "number")]
    pub total: usize,
    /// Every command in the registry (optionally filtered), sorted by name.
    pub commands: Vec<CommandInfo>,
}

/// `commands/list` — dynamic, single-source command discovery.
#[derive(Default)]
pub struct CommandsList;

#[async_trait]
impl ActionCommand for CommandsList {
    const NAME: &'static str = "commands/list";
    const ALIASES: &'static [&'static str] = &["list_commands"];
    const NATIVE: bool = true; // discovery pair — the persona reaches the long tail through this
    const DESCRIPTION: &'static str =
        "List the available commands (name, description, access, params type) from the live \
         registry. Optional `filter` is a case-insensitive name substring.";
    type Params = CommandsListParams;
    type Output = CommandsListResult;

    async fn run(
        &self,
        ctx: &Ctx,
        params: CommandsListParams,
    ) -> Result<CommandsListResult, CommandError> {
        let needle = params.filter.as_deref().map(|s| s.to_lowercase());
        // Listed == callable, BY IDENTITY: show only what THIS caller could
        // actually run at its effective trust (local owner sees all; a persona /
        // cross-grid peer at Provisional sees only its authorized surface). Same
        // trust rule the executor's gate uses (`caller_trust`), so list and call
        // can't drift — no separate allow-table.
        let trust = caller_trust(ctx.caller.as_ref());
        let commands = command_registry()
            .iter()
            .filter(|d| is_command_authorized(d.name, trust))
            .filter(|d| match &needle {
                Some(n) => d.name.to_lowercase().contains(n),
                None => true,
            })
            .map(|d| CommandInfo {
                name: d.name.to_string(),
                description: d.description.to_string(),
                access_level: d.access_level.as_str().to_string(),
                wire: match d.wire {
                    WireShape::Bare => "bare",
                    WireShape::Enveloped => "enveloped",
                    WireShape::Provided => "provided",
                }
                .to_string(),
                params_type: d.params.name.clone(),
                params_schema: d.params_schema.clone(),
            })
            .collect::<Vec<_>>();
        Ok(CommandsListResult {
            total: commands.len(),
            commands,
        })
    }
}
crate::register_stateless_command!(CommandsList);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: commands/list is itself in the registry it reports (it
    // self-registered with zero ceremony), AND the filter narrows by name. This is
    // the single-source discovery proof — the catalog isn't a hand-maintained list.
    #[tokio::test]
    async fn lists_from_the_registry_and_filters() {
        let out = CommandsList
            .run(&Ctx::default(), CommandsListParams { filter: None })
            .await
            .expect("ok");
        assert!(
            out.commands.iter().any(|c| c.name == "commands/list"),
            "the catalog includes itself (self-registered, single source)"
        );
        assert!(
            out.commands.iter().any(|c| c.name == "ping"),
            "and other real commands"
        );

        let filtered = CommandsList
            .run(
                &Ctx::default(),
                CommandsListParams {
                    filter: Some("commands/".to_string()),
                },
            )
            .await
            .expect("ok");
        assert!(
            filtered
                .commands
                .iter()
                .all(|c| c.name.contains("commands/")),
            "filter narrows by name substring"
        );
        assert!(!filtered.commands.is_empty());

        // what this catches: `total` is the head-of-result count that answers "how
        // many commands are available?" without tallying a (possibly fold-truncated)
        // array — it must equal the returned set for both the broad and filtered
        // views, so a persona reasons from it instead of guessing. Regression guard
        // for the live dogfood quirk where a 14B persona could not count commands
        // from the raw JSON dump and confabulated "I can't execute tools".
        assert_eq!(out.total, out.commands.len(), "total == broad set size");
        assert_eq!(
            filtered.total,
            filtered.commands.len(),
            "total == filtered set size"
        );
    }

    // what this catches: LISTED == CALLABLE, BY IDENTITY. A local owner (caller
    // None) sees the full surface; an airc/Provisional caller sees only what it
    // could actually run — and every command it's shown IS authorized at its trust
    // (same `caller_trust` + `is_command_authorized` the executor's gate uses, so
    // discovery and dispatch can't drift). This is the persona/cross-grid guarantee
    // the user asked for at the discovery surface, not just the call surface.
    #[tokio::test]
    async fn list_is_gated_by_caller_identity() {
        use crate::modules::grid::node::TrustLevel;
        use crate::routing::CallerIdentity;
        use std::collections::HashSet;
        use uuid::Uuid;

        let owner = CommandsList
            .run(&Ctx::default(), CommandsListParams { filter: None })
            .await
            .unwrap();
        let airc_ctx = Ctx {
            caller: Some(CallerIdentity::airc(crate::identity::PeerId::new())),
            ..Default::default()
        };
        let provisional = CommandsList
            .run(&airc_ctx, CommandsListParams { filter: None })
            .await
            .unwrap();

        // AiSafe ping is visible to both.
        assert!(owner.commands.iter().any(|c| c.name == "ping"));
        assert!(
            provisional.commands.iter().any(|c| c.name == "ping"),
            "AiSafe command visible at Provisional"
        );

        // Non-vacuous: the Provisional surface is non-empty (so the subset loop
        // below actually checks something) and never larger than the owner surface.
        assert!(
            !provisional.commands.is_empty(),
            "Provisional surface must be non-empty (else the subset check is vacuous)"
        );
        assert!(provisional.commands.len() <= owner.commands.len());

        // Provisional ⊆ Owner, and everything shown to the Provisional caller is
        // actually authorized at Provisional (listed == callable).
        let owner_names: HashSet<&str> = owner.commands.iter().map(|c| c.name.as_str()).collect();
        for c in &provisional.commands {
            assert!(
                owner_names.contains(c.name.as_str()),
                "provisional surface ⊆ owner surface ({})",
                c.name
            );
            assert!(
                is_command_authorized(&c.name, TrustLevel::Provisional),
                "listed must be callable at Provisional: {}",
                c.name
            );
        }
    }
}
