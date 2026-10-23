#![cfg(test)]
//! From `cognition::tool_executor::command_executor`: a persona's hands reaching core's
//! `HealthModule` (a surface module) through the typed object path. Asserts exactly what it
//! asserted before the move.

use std::sync::Arc;

use serde_json::{json, Value};
use uuid::Uuid;

use crate::cognition::tool_executor::types::PersonaMediaConfigLite;
use crate::ai::types::ToolCall as NativeToolCall;
use crate::cognition::tool_executor::{CommandToolExecutor, ToolExecutionContext, ToolExecutor};
use crate::runtime::{CommandExecutor, ModuleRegistry};

// what this catches: a WORKING PERSONA on the clean command infra — a persona's
// hands route a tool call to `ping`, which is migrated to the self-routing
// DynCommand object path (ActionCommand ⟹ DynCommand, OFF the prefix table). The
// call goes persona → uniform Connection → InProcessTransport → CommandExecutor
// → execute_inner → route_object → the ping object, and the bare result comes
// back, no error. Fully deterministic: no inference, no airc, no models — just
// the command substrate. This is the regression guard that the cleanup didn't
// break the persona's ability to ACT, and that the typed path serves personas.
#[tokio::test]
async fn persona_executes_ping_via_typed_object_path() {
    let registry = Arc::new(ModuleRegistry::new());
    registry.register(Arc::new(crate::modules::health::HealthModule::new()));
    // The SAME production factory the spawn path uses, over an executor built from this registry.
    let exec = CommandToolExecutor::for_persona(Arc::new(CommandExecutor::new(registry)), Uuid::new_v4());

    let ctx = ToolExecutionContext {
        persona_id: Uuid::new_v4(),
        persona_name: "Ivar".to_string(),
        session_id: Uuid::new_v4(),
        context_id: Uuid::new_v4(),
        caller_context: Value::Null,
        persona_config: PersonaMediaConfigLite { auto_load_media: false, supported_media_types: vec![] },
    };
    let calls = vec![NativeToolCall {
        id: "p1".to_string(),
        name: "ping".to_string(),
        input: json!({}),
    }];
    let out = exec
        .execute_native_batch(&calls, &ctx, 8000)
        .await
        .expect("batch ok");

    assert_eq!(out.results.len(), 1);
    assert_eq!(out.results[0].tool_use_id, "p1");
    assert!(
        out.results[0].is_error.is_none(),
        "ping must succeed for the persona via the typed path: {}",
        out.results[0].content
    );
    assert!(
        out.results[0].content.contains("\"ok\":true"),
        "the bare PingResult is fed back to the persona: {}",
        out.results[0].content
    );
}
