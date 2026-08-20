//! Typed params + result for the generator's commands.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Params for `generate/module`. The named, schema-able contract for the
/// `generate/module` typed command.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/generate/GenerateModuleParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GenerateModuleParams {
    /// Lowercase module name. Must be a valid Rust identifier
    /// (letters, digits, `_`, `-` allowed; can't start with a digit).
    /// Used to derive the struct name (`<Name>Module`) and the
    /// directory path.
    pub name: String,

    /// Human-readable description, embedded in the generated mod.rs
    /// docstring + the README.
    pub description: String,

    /// Commands this module will provide. Each becomes a stub entry
    /// in the generated `handle_command` dispatch and a line in the
    /// README's contract.
    #[serde(default)]
    pub commands: Vec<String>,

    /// Event globs the module subscribes to. Becomes
    /// `event_subscriptions` in the generated `ModuleConfig`.
    #[serde(default)]
    pub events_subscribed: Vec<String>,

    /// Event names this module emits. Documented in the README; not
    /// wired into the runtime (publishers emit at their own pace).
    #[serde(default)]
    pub events_published: Vec<String>,

    /// Priority class for the generated module. Mapped to
    /// [`crate::runtime::ModulePriority`] in the generated config.
    #[serde(default)]
    pub priority: PrioritySpec,

    /// Overwrite an existing module directory at the same path.
    /// Default is `false` — the generator fails loud if the target
    /// already exists, so a caller doesn't accidentally clobber work.
    #[serde(default)]
    pub force: bool,

    /// Opt in to the per-resource-lock scaffold when the module
    /// holds mutable state across an `.await` (or shared filesystem
    /// invariant). When `true`, the generator emits:
    ///
    /// - `DashMap<ResourceId, Arc<tokio::sync::Mutex<ResourceState>>>`
    ///   field on the module struct
    /// - A `ResourceState` placeholder struct authors fill in
    /// - A `resource_lock(&self, id)` get-or-create helper
    /// - A multi-thread concurrency stress test pinning the
    ///   "different resources stay parallel; same resource
    ///   serializes" invariant
    ///
    /// When `false` (default), the module is stateless and the
    /// concurrency test just verifies typed-envelope routing.
    ///
    /// See [`COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md`](../../../../../../docs/architecture/COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md)
    /// §4 (Concurrency doctrine) for when to set this.
    #[serde(default)]
    pub stateful: bool,
}

/// Wire-friendly enum mirroring [`crate::runtime::ModulePriority`]'s
/// public variants. Default is `Normal` to match the most common
/// module class.
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, TS, schemars::JsonSchema,
)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/generate/PrioritySpec.ts"
)]
#[serde(rename_all = "lowercase")]
pub enum PrioritySpec {
    Realtime,
    High,
    #[default]
    Normal,
    Background,
}

impl PrioritySpec {
    /// Render as the Rust enum variant name used in the generated
    /// module's `ModuleConfig::priority` field. e.g.
    /// `PrioritySpec::Realtime` → `"Realtime"`.
    pub fn as_variant_str(self) -> &'static str {
        match self {
            PrioritySpec::Realtime => "Realtime",
            PrioritySpec::High => "High",
            PrioritySpec::Normal => "Normal",
            PrioritySpec::Background => "Background",
        }
    }
}

/// Result of `generate/module` — the new module directory, the files written,
/// and the next manual wire-up step.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/generate/GenerateModuleResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GenerateModuleResult {
    /// Absolute path to the newly created module directory.
    pub module_path: std::path::PathBuf,

    /// Each file the generator wrote, in order. Lets the caller
    /// audit + maybe diff against expectations.
    pub files_created: Vec<std::path::PathBuf>,

    /// Plain-English next step for the human/AI caller. Today: a
    /// reminder to wire the new module into the parent `mod.rs`
    /// and register it at startup. Future versions of the generator
    /// can do this automatically; meanwhile this string surfaces the
    /// remaining manual step where the caller will see it.
    pub next_step: String,
}

/// Lightweight name validation. Generated module names become Rust
/// identifiers, directory names, and parts of command paths — so we
/// constrain to lowercase ASCII letters/digits with `_`/`-` allowed
/// as word separators, and refuse a leading digit.
pub fn validate_module_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Module name cannot be empty".to_string());
    }
    let first = name.chars().next().unwrap();
    if !first.is_ascii_lowercase() && first != '_' {
        return Err(format!(
            "Module name `{name}` must start with a lowercase ASCII letter or underscore \
             (got `{first}`) — names become Rust identifiers"
        ));
    }
    for c in name.chars() {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '_' && c != '-' {
            return Err(format!(
                "Module name `{name}` contains invalid character `{c}` — only \
                 lowercase ASCII letters, digits, `_`, and `-` are allowed"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_accepts_canonical_names() {
        for ok in ["chat", "ai_provider", "ai-provider", "_internal", "a1"] {
            validate_module_name(ok).unwrap_or_else(|e| panic!("expected `{ok}` to validate: {e}"));
        }
    }

    #[test]
    fn validate_rejects_empty_or_invalid() {
        for bad in ["", "Chat", "9chat", "has space", "with/slash"] {
            assert!(
                validate_module_name(bad).is_err(),
                "expected `{bad}` to fail validation"
            );
        }
    }

    #[test]
    fn priority_spec_round_trips_through_json() {
        for variant in [
            PrioritySpec::Realtime,
            PrioritySpec::High,
            PrioritySpec::Normal,
            PrioritySpec::Background,
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            let back: PrioritySpec = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, back, "JSON round-trip: {json}");
        }
    }

    #[test]
    fn priority_spec_default_is_normal() {
        assert_eq!(PrioritySpec::default(), PrioritySpec::Normal);
    }

    #[test]
    fn priority_spec_as_variant_str_matches_rust_enum() {
        assert_eq!(PrioritySpec::Realtime.as_variant_str(), "Realtime");
        assert_eq!(PrioritySpec::High.as_variant_str(), "High");
        assert_eq!(PrioritySpec::Normal.as_variant_str(), "Normal");
        assert_eq!(PrioritySpec::Background.as_variant_str(), "Background");
    }
}
