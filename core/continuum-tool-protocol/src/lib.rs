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
    /// The call's arguments as the tool saw them, in one canonical encoding: compact JSON with
    /// every object's keys sorted, so two calls that ask the same thing encode to the same bytes
    /// whatever order the model wrote their keys in. The random per-call `id` is not part of it.
    /// This is the single source repeat identity is built from (MIND-CONTEXT invariant 4,
    /// `continuum_context::cause::Fingerprint`). `Value`'s `Display` cannot fail, so no call falls
    /// back to an empty encoding that would collide with every other; its keys come out sorted
    /// because serde_json's map is ordered (without `preserve_order`, which the test below pins).
    pub fn canonical_input(&self) -> Vec<u8> {
        self.input.to_string().into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(input: Value) -> ToolCall {
        ToolCall { id: uuid::Uuid::new_v4().to_string(), name: "code/read".into(), input }
    }

    // what this catches: one request encoding two ways (key order the model chose, a fresh call
    // id), which would let a repeated read pass as new; and two different requests colliding.
    #[test]
    fn equal_requests_encode_equal_whatever_their_key_order_or_id() {
        let a = call(serde_json::json!({"path": "a.rs", "range": {"start": 1, "end": 9}, "tags": ["x", 2, null, true]}));
        // Built key by key in reverse order, so an insertion-ordered map (serde_json's
        // `preserve_order`, if any dependency ever enables it) fails here instead of silently
        // giving one request two identities.
        let mut range = serde_json::Map::new();
        range.insert("end".into(), 9.into());
        range.insert("start".into(), 1.into());
        let mut reversed = serde_json::Map::new();
        reversed.insert("tags".into(), serde_json::json!(["x", 2, null, true]));
        reversed.insert("range".into(), Value::Object(range));
        reversed.insert("path".into(), "a.rs".into());
        let b = call(Value::Object(reversed));
        assert_eq!(a.canonical_input(), b.canonical_input());
        assert_eq!(
            a.canonical_input(),
            br#"{"path":"a.rs","range":{"end":9,"start":1},"tags":["x",2,null,true]}"#.to_vec()
        );
        assert_ne!(a.canonical_input(), call(serde_json::json!({"path": "b.rs"})).canonical_input());
        assert_ne!(call(serde_json::json!({"p": "a\"b"})).canonical_input(), call(serde_json::json!({"p": "a", "b": ""})).canonical_input());
    }
}
