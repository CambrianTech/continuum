//! `CommandToolExecutor` — the persona's HANDS.
//!
//! The deliberation faculty (the reasoner) can already decide to *act* — it
//! emits native `tool_use` calls in its agent loop. What it lacked was anything
//! to execute them: the only `ToolExecutor` was a test double, so the live
//! persona could talk but never touch the world. This is the production
//! executor that closes that gap.
//!
//! It routes each native tool call straight to the core's **command surface**
//! (`code/read`, `code/edit`, `cargo`, `data/*`, … — the same catalog the MCP
//! server exposes) via the [`CommandExecutor`]. No Node in the loop: the brain
//! is Rust, the tools are Rust commands. Tool name == command name; a model that
//! emits the underscore form (`code_read`) maps back to the slash form.
//!
//! Each call dispatches under the persona's own [`CallerIdentity`] (Local-
//! sourced, keyed by `persona_id`), so the SAME `AuthPolicy` gate that protects
//! every command (incl. [`crate::routing::GridTrustAuthPolicy`]) gates the
//! persona too — a persona can't reach a command its identity isn't allowed.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use super::types::{
    NativeBatchOutcome, ParsedToolBatch, ToolError, ToolExecutionContext, ToolOutcome,
};
use super::ToolExecutor;
use crate::ai::types::{ToolCall as NativeToolCall, ToolResult as NativeToolResult};
use crate::routing::CallerIdentity;
use crate::runtime::CommandExecutor;

/// Routes a persona's native tool calls to core commands. The persona's hands.
pub struct CommandToolExecutor {
    executor: Arc<CommandExecutor>,
}

impl CommandToolExecutor {
    pub fn new(executor: Arc<CommandExecutor>) -> Self {
        Self { executor }
    }
}

/// Truncate to at most `max` bytes on a UTF-8 char boundary. Tool output (a file
/// read, a cargo log) can be huge; the agent loop bounds it so the context
/// doesn't blow up. Appends a marker so the model knows it was cut.
fn truncate_on_boundary(mut s: String, max: usize) -> String {
    if s.len() <= max {
        return s;
    }
    let mut end = max.min(s.len());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
    s.push_str("\n…[truncated]");
    s
}

#[async_trait]
impl ToolExecutor for CommandToolExecutor {
    async fn execute_native_batch(
        &self,
        calls: &[NativeToolCall],
        ctx: &ToolExecutionContext,
        max_result_chars: usize,
    ) -> Result<NativeBatchOutcome, ToolError> {
        // The persona acts under its own (local-sourced) identity — the same
        // AuthPolicy gate that protects every command gates the persona.
        let caller = CallerIdentity::local(ctx.persona_id);

        let mut results = Vec::with_capacity(calls.len());
        for call in calls {
            // Tool name IS the command name. Map the underscore form some models
            // emit (`code_read`) back to the canonical slash form (`code/read`).
            let command = call.name.replace('_', "/");

            let outcome = self
                .executor
                .execute_with_caller(command.as_str(), call.input.clone(), Some(caller.clone()))
                .await
                .and_then(|r| r.to_json_value());

            let result = match outcome {
                Ok(value) => NativeToolResult {
                    tool_use_id: call.id.clone(),
                    content: truncate_on_boundary(value.to_string(), max_result_chars),
                    is_error: None,
                },
                // A failed tool call is NOT a batch failure — it's fed back to the
                // model as an error result so it can recover (retry, fix args,
                // pick another tool). Batch-level `Err` is reserved for the
                // executor itself being unavailable.
                Err(e) => NativeToolResult {
                    tool_use_id: call.id.clone(),
                    content: truncate_on_boundary(e, max_result_chars),
                    is_error: Some(true),
                },
            };
            results.push(result);
        }

        Ok(NativeBatchOutcome {
            results,
            media: Vec::new(),
            stored_ids: Vec::new(),
        })
    }

    async fn parse_response(
        &self,
        _response_text: &str,
        _model_family: Option<&str>,
    ) -> Result<ParsedToolBatch, ToolError> {
        // The deliberation loop consumes NATIVE tool_use blocks; it never asks
        // this executor to parse text. XML-fallback parsing for non-native
        // models is a separate concern, not this Rust executor's job.
        Err(ToolError::ParseFailed {
            raw_preview: String::new(),
            reason: "CommandToolExecutor is native-tool-use only; no XML parsing".to_string(),
        })
    }

    async fn store_outcome(
        &self,
        _outcome: &ToolOutcome,
        _context: &ToolExecutionContext,
    ) -> Result<Uuid, ToolError> {
        // The agent loop threads tool results inline (assistant tool_use → user
        // tool_result) and re-generates; it does not call store_outcome. A fresh
        // id satisfies the contract without a redundant working-memory write.
        Ok(Uuid::new_v4())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::tool_executor::types::PersonaMediaConfigLite;
    use crate::runtime::{
        CommandResult, ModuleConfig, ModuleContext, ModulePriority, ModuleRegistry, ServiceModule,
    };
    use serde_json::{json, Value};
    use std::any::Any;

    /// Minimal module that echoes its params back under `test/echo`.
    struct EchoModule;

    #[async_trait]
    impl ServiceModule for EchoModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: "echo",
                priority: ModulePriority::Normal,
                command_prefixes: &["test/"],
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }
        async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
            Ok(())
        }
        async fn handle_command(
            &self,
            command: &str,
            params: Value,
        ) -> Result<CommandResult, String> {
            match command {
                "test/echo" => Ok(CommandResult::Json(params)),
                other => Err(format!("unknown command: {other}")),
            }
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    fn ctx() -> ToolExecutionContext {
        ToolExecutionContext {
            persona_id: Uuid::new_v4(),
            persona_name: "Ivar".to_string(),
            session_id: Uuid::new_v4(),
            context_id: Uuid::new_v4(),
            caller_context: Value::Null,
            persona_config: PersonaMediaConfigLite {
                auto_load_media: false,
                supported_media_types: vec![],
            },
        }
    }

    fn executor_with_echo() -> CommandToolExecutor {
        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(EchoModule));
        CommandToolExecutor::new(Arc::new(CommandExecutor::new(registry)))
    }

    // what this catches: THE thing that turns "talks" into "acts" — a native tool
    // call routes to the real command and the command's result comes back,
    // correlated by tool_use_id, no error. If this regresses, the persona is back
    // to a chatbot that can't touch the world.
    #[tokio::test]
    async fn routes_native_tool_call_to_the_command() {
        let exec = executor_with_echo();
        let calls = vec![NativeToolCall {
            id: "t1".to_string(),
            name: "test/echo".to_string(),
            input: json!({ "path": "deploy.md" }),
        }];
        let out = exec.execute_native_batch(&calls, &ctx(), 8000).await.unwrap();
        assert_eq!(out.results.len(), 1);
        assert_eq!(out.results[0].tool_use_id, "t1");
        assert!(out.results[0].is_error.is_none(), "successful tool call");
        assert!(
            out.results[0].content.contains("deploy.md"),
            "command result fed back: {}",
            out.results[0].content
        );
    }

    // what this catches: the underscore→slash mapping for models that emit
    // `test_echo` instead of `test/echo`.
    #[tokio::test]
    async fn maps_underscore_tool_name_to_slash_command() {
        let exec = executor_with_echo();
        let calls = vec![NativeToolCall {
            id: "t1".to_string(),
            name: "test_echo".to_string(),
            input: json!({ "ok": true }),
        }];
        let out = exec.execute_native_batch(&calls, &ctx(), 8000).await.unwrap();
        assert!(out.results[0].is_error.is_none(), "test_echo → test/echo routed");
    }

    // what this catches: a failed tool call is fed back as an ERROR RESULT (so the
    // model can recover), NOT a batch-level failure that aborts the turn.
    #[tokio::test]
    async fn failed_call_becomes_error_result_not_batch_failure() {
        let exec = executor_with_echo();
        let calls = vec![NativeToolCall {
            id: "t1".to_string(),
            name: "test/nonexistent".to_string(),
            input: json!({}),
        }];
        let out = exec
            .execute_native_batch(&calls, &ctx(), 8000)
            .await
            .expect("batch itself succeeds");
        assert_eq!(out.results[0].is_error, Some(true), "per-call error, batch ok");
    }
}
