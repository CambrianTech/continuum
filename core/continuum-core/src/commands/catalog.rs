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

use crate::sdk_codegen::{command_registry, ActionCommand, CommandError, Ctx, WireShape};

/// Params for `commands/list` — an optional case-insensitive name substring to
/// filter by (so a tray can ask "what `data/*` commands exist?"). Empty ⇒ all.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/commands/CommandsListParams.ts")]
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
#[ts(export, export_to = "../../../protocol/typescript/commands/CommandInfo.ts")]
pub struct CommandInfo {
    /// The command name — the routing key you call (`cu <name>`).
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
    /// and `cu <cmd> --help`.
    #[ts(type = "unknown")]
    pub params_schema: serde_json::Value,
}

/// Result of `commands/list` — the live catalog.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/commands/CommandsListResult.ts")]
pub struct CommandsListResult {
    /// Every command in the registry (optionally filtered), sorted by name.
    pub commands: Vec<CommandInfo>,
}

/// `commands/list` — dynamic, single-source command discovery.
#[derive(Default)]
pub struct CommandsList;

#[async_trait]
impl ActionCommand for CommandsList {
    const NAME: &'static str = "commands/list";
    const DESCRIPTION: &'static str =
        "List the available commands (name, description, access, params type) from the live \
         registry. Optional `filter` is a case-insensitive name substring.";
    type Params = CommandsListParams;
    type Output = CommandsListResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        params: CommandsListParams,
    ) -> Result<CommandsListResult, CommandError> {
        let needle = params.filter.as_deref().map(|s| s.to_lowercase());
        let commands = command_registry()
            .iter()
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
            .collect();
        Ok(CommandsListResult { commands })
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
            filtered.commands.iter().all(|c| c.name.contains("commands/")),
            "filter narrows by name substring"
        );
        assert!(!filtered.commands.is_empty());
    }
}
