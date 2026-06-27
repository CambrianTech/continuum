//! ToolParsingModule — tool-call parsing + name-codec.
//!
//! All five verbs are migrated to the typed [`DynCommand`](crate::sdk_codegen::DynCommand)
//! registry under `commands/tool_parsing/` (task #62):
//! - `tool-parsing/parse` / `tool-parsing/correct` — stateless (self-register).
//! - `tool-parsing/register-tools` / `tool-parsing/decode-name` /
//!   `tool-parsing/encode-name` — dep-holding over this module's shared
//!   [`ToolNameCodec`], contributed via [`commands()`](ToolParsingModule::commands).
//!
//! The module retains only the shared codec state; its legacy `handle_command`
//! arms now fail loud, directing callers to the typed `route_object` path.

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::sdk_codegen::DynCommand;
use crate::tool_parsing::ToolNameCodec;
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;

pub struct ToolParsingModule {
    codec: Arc<ToolNameCodec>,
}

impl Default for ToolParsingModule {
    fn default() -> Self {
        Self {
            codec: Arc::new(ToolNameCodec::new()),
        }
    }
}

impl ToolParsingModule {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ServiceModule for ToolParsingModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "tool-parsing",
            priority: ModulePriority::Normal,
            command_prefixes: &["tool-parsing/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    /// Expose the dep-holding codec verbs (register-tools / decode-name /
    /// encode-name) over this module's shared [`ToolNameCodec`]. The stateless
    /// `parse` / `correct` verbs are NOT here — they self-register.
    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        crate::commands::tool_parsing::command_objects(self.codec.clone())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // All five verbs are migrated to the typed registry (commands/tool_parsing/).
        // The codec verbs route via `route_object` against THIS module's shared codec
        // (contributed by `commands()`); parse/correct are stateless and self-register.
        match command {
            "tool-parsing/parse"
            | "tool-parsing/correct"
            | "tool-parsing/register-tools"
            | "tool-parsing/decode-name"
            | "tool-parsing/encode-name" => Err(format!(
                "'{command}' is migrated to the typed registry \
                 (commands/tool_parsing/) — it must route via route_object, \
                 not the legacy handle_command path"
            )),
            _ => Err(format!("unknown tool-parsing command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: every migrated verb now fails loud through the legacy
    // path, naming itself + pointing at the typed registry (no silent success that
    // would mask a routing regression). The behavioral tests live in the command
    // files (commands/tool_parsing/*).
    #[tokio::test]
    async fn migrated_arms_fail_loud() {
        let module = ToolParsingModule::new();
        for command in [
            "tool-parsing/parse",
            "tool-parsing/correct",
            "tool-parsing/register-tools",
            "tool-parsing/decode-name",
            "tool-parsing/encode-name",
        ] {
            let err = module
                .handle_command(command, Value::Null)
                .await
                .expect_err("migrated arm must fail loud");
            assert!(err.contains("migrated"), "for {command}: {err}");
            assert!(err.contains(command), "for {command}: {err}");
        }
    }

    // what this catches: the module contributes the three dep-holding codec verbs
    // to the typed object map (sharing its one codec). A regression that drops the
    // `commands()` override — leaving them unroutable — is caught.
    #[test]
    fn contributes_the_typed_codec_commands() {
        let module = ToolParsingModule::new();
        let names: Vec<&str> = module.commands().iter().map(|c| c.name()).collect();
        assert!(names.contains(&"tool-parsing/register-tools"));
        assert!(names.contains(&"tool-parsing/decode-name"));
        assert!(names.contains(&"tool-parsing/encode-name"));
    }

    // what this catches: an unmigrated/unknown verb still errors (not a panic, not
    // a silent ok).
    #[tokio::test]
    async fn unknown_command_errors() {
        let module = ToolParsingModule::new();
        let result = module
            .handle_command("tool-parsing/nope", Value::Null)
            .await;
        assert!(result.is_err());
    }
}
