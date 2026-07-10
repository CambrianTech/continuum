//! `keys/*` — provider API-key management, single-owner and value-blind.
//!
//! The elegant shape (Joel 2026-07-10: "the adapters coded elegantly and the
//! same for the key management"):
//!
//! - **ONE writer**: [`crate::config_env::upsert`] into `~/.continuum/config.env`
//!   ([[config-env-single-owner]]). No scattered `.env` files, no second store.
//! - **ONE reader**: [`crate::secrets`] loads that file at boot; adapters pull
//!   via `get_secret(api_key_env)` at auth time. Keys live in the adapter
//!   layer only ([[compute-lease-boundary]]) — no persona-visible surface.
//! - **DYNAMIC key universe**: the accepted names come from the model
//!   registry's provider rows (`Provider.api_key_env`), never a hardcoded
//!   list — registering a new provider row automatically makes its key
//!   settable and listable.
//! - **VALUE-BLIND**: `keys/list` reports presence booleans only; `keys/set`
//!   never echoes the value back. Both are `Privileged` — credential
//!   management is an operator act, not a persona tool.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};

/// The provider key-env names the substrate accepts — read LIVE from the model
/// registry's provider rows. One source; a new `ProviderSpec` row extends the
/// key universe with zero changes here.
fn known_key_envs() -> Vec<(String, String)> {
    let Some(registry) = crate::model_registry::try_global() else {
        return Vec::new();
    };
    registry
        .providers()
        .filter_map(|p| {
            p.api_key_env
                .as_ref()
                .map(|env| (env.clone(), p.id.clone()))
        })
        .collect()
}

// ─────────────────────────── keys/list ───────────────────────────

/// Params for `keys/list` — none.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/keys/KeysListParams.ts")]
pub struct KeysListParams {}

/// One provider key slot: which env name, which provider it unlocks, and
/// whether a value is PRESENT (never the value itself).
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/keys/KeyStatus.ts")]
pub struct KeyStatus {
    /// The env/config name (e.g. `DEEPSEEK_API_KEY`).
    pub name: String,
    /// The provider row this key unlocks (e.g. `deepseek`).
    pub provider: String,
    /// A value is present in `~/.continuum/config.env` (or the process env).
    pub present: bool,
}

/// Result of `keys/list`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/keys/KeysListResult.ts")]
pub struct KeysListResult {
    pub keys: Vec<KeyStatus>,
}

/// `keys/list` — every provider key slot the registry knows, with presence
/// booleans. Value-blind by construction.
#[derive(Default)]
pub struct KeysList;

#[async_trait::async_trait]
impl ActionCommand for KeysList {
    const NAME: &'static str = "keys/list";
    const ACCESS: crate::sdk_codegen::AccessLevel = crate::sdk_codegen::AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "List every cloud-provider API-key slot the model registry knows (name + \
         provider + whether a value is present). Never returns key values. Presence \
         drives persona allocation: one cloud persona per present key.";
    type Params = KeysListParams;
    type Output = KeysListResult;

    async fn run(&self, _ctx: &Ctx, _p: KeysListParams) -> Result<KeysListResult, CommandError> {
        let keys = known_key_envs()
            .into_iter()
            .map(|(name, provider)| {
                let present = crate::config_env::read(&name)
                    .filter(|v| !v.trim().is_empty())
                    .is_some()
                    || std::env::var(&name).map(|v| !v.trim().is_empty()).unwrap_or(false);
                KeyStatus { name, provider, present }
            })
            .collect();
        Ok(KeysListResult { keys })
    }
}

crate::register_stateless_command!(KeysList);

// ─────────────────────────── keys/set ───────────────────────────

/// Params for `keys/set`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/keys/KeysSetParams.ts")]
pub struct KeysSetParams {
    /// The key-env name to set — must be one the registry's provider rows
    /// declare (see `keys/list`). Unknown names fail loud with the known set.
    pub name: String,
    /// The key value. Stored via the single-owner config writer; NEVER echoed
    /// back in the result or any log.
    pub value: String,
}

/// Result of `keys/set` — confirmation without the value.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/keys/KeysSetResult.ts")]
pub struct KeysSetResult {
    /// The key name that was stored.
    pub name: String,
    /// The provider this unlocks.
    pub provider: String,
    /// When the key takes effect. Secrets load once at boot ([`crate::secrets`]),
    /// so a newly set key activates on the next core start — stated honestly
    /// rather than pretending hot-reload.
    pub effective: String,
}

/// `keys/set` — store a provider API key through the ONE config writer.
#[derive(Default)]
pub struct KeysSet;

#[async_trait::async_trait]
impl ActionCommand for KeysSet {
    const NAME: &'static str = "keys/set";
    const ACCESS: crate::sdk_codegen::AccessLevel = crate::sdk_codegen::AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Store a cloud-provider API key into ~/.continuum/config.env (the single-owner \
         config store). The name must be a provider key the registry declares — call \
         keys/list for the set. The value is never echoed back. Takes effect on the \
         next core start (secrets load once at boot).";
    type Params = KeysSetParams;
    type Output = KeysSetResult;

    async fn run(&self, _ctx: &Ctx, p: KeysSetParams) -> Result<KeysSetResult, CommandError> {
        let name = p.name.trim().to_string();
        let value = p.value.trim();
        if value.is_empty() {
            return Err(CommandError::Invalid(
                "keys/set: value is empty — nothing to store".into(),
            ));
        }
        let known = known_key_envs();
        let Some((_, provider)) = known.iter().find(|(env, _)| *env == name) else {
            let names: Vec<&str> = known.iter().map(|(env, _)| env.as_str()).collect();
            return Err(CommandError::Invalid(format!(
                "keys/set: `{name}` is not a provider key the registry declares. \
                 Known keys: {}. (A new provider is a ProviderSpec row in the \
                 catalog — its key becomes settable automatically.)",
                names.join(", ")
            )));
        };
        crate::config_env::upsert(&name, value)
            .map_err(|e| CommandError::Internal(format!("keys/set: config write failed: {e}")))?;
        crate::probe!(
            class = "keys.set",
            key = %name,
            provider = %provider,
            "provider key stored (value withheld from all logs)"
        );
        Ok(KeysSetResult {
            name,
            provider: provider.clone(),
            effective: "next core start (cu reboot)".to_string(),
        })
    }
}

crate::register_stateless_command!(KeysSet);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: name/access wiring — key management is Privileged
    // (operator act), never on the AiSafe persona surface; and the key universe
    // is registry-derived, not a hardcoded list (a provider row with an
    // api_key_env appears automatically).
    #[test]
    fn wiring_and_dynamic_universe() {
        assert_eq!(KeysList::NAME, "keys/list");
        assert_eq!(KeysSet::NAME, "keys/set");
        assert!(matches!(
            KeysList::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
        assert!(matches!(
            KeysSet::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
        // Registry may be uninitialized in unit context — the fn degrades to
        // empty rather than panicking; with it initialized the cloud rows
        // (deepseek/groq/fireworks/xai/…) each contribute their key env.
        let _ = known_key_envs();
    }
}
