//! Pure tool exchange syntax and shared wire types. No runtime, model, or executor dependency.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
pub mod parser;

/// Native tool specification for providers with JSON tool support
/// (Anthropic, OpenAI, DeepSeek, etc.)
///
/// Field names match the Anthropic API wire format (snake_case):
/// - `input_schema` NOT `inputSchema`
///   This must NOT use rename_all = "camelCase" because the wire format
///   from TypeScript AND the Anthropic API both use snake_case for this struct.
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/NativeToolSpec.ts"
)]
pub struct NativeToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: ToolInputSchema,
}

/// JSON Schema for tool input parameters.
/// Matches Anthropic API wire format (snake_case field names).
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/ToolInputSchema.ts"
)]
pub struct ToolInputSchema {
    #[serde(rename = "type")]
    pub schema_type: String, // Always "object"
    #[ts(type = "Record<string, unknown>")]
    pub properties: Value, // JSON object describing properties
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub required: Option<Vec<String>>,
    /// Nested-type definitions — the `#/definitions/<Name>` targets schemars
    /// emits for any param with a nested struct/enum (`EditMode`, `OrderByClause`,
    /// the self-referential `RagSourceRequest`, …). They MUST travel with the
    /// schema: a backend's grammar/parser resolves each `$ref` against this
    /// sibling, and without it llama.cpp rejects the whole turn with a 400
    /// ("definitions not in {…}"). Carried verbatim under `definitions` (the key
    /// the refs name); harmless standard JSON Schema for OpenAI/Anthropic too.
    /// Inlining is NOT an option — recursive params (`sources: Vec<Self>`) express
    /// recursion AS a `$ref`, so the ref must resolve, not expand.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(type = "Record<string, unknown>")]
    pub definitions: Option<Value>,
}

/// Tool call from AI response (when AI wants to use a tool)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/ai/ToolCall.ts")]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,   // Unique ID for this tool use (e.g., "toolu_01A...")
    pub name: String, // Tool name
    #[ts(type = "Record<string, unknown>")]
    pub input: Value, // Tool parameters as JSON
}

impl ToolCall {
    /// Stable identity of THIS call for loop / repeat detection: `name|json(input)`.
    ///
    /// The random per-call `id` is deliberately excluded — two calls with the same name
    /// and arguments ARE the same action regardless of their generated ids. This is the
    /// SINGLE source of the fingerprint that both the settle loop's stuck-batch signature
    /// (`act_observe::settle::drive_to_settle`) and `apply_act`'s repeat guard key on;
    /// two hand-inlined copies of this format drifting apart would silently break loop
    /// detection, so they share this one method.
    pub fn loop_fingerprint(&self) -> String {
        format!(
            "{}|{}",
            self.name,
            serde_json::to_string(&self.input).unwrap_or_default()
        )
    }
}
