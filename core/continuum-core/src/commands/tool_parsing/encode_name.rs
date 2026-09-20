//! `tool-parsing/encode-name` — encode a canonical tool name for API transmission
//! (slashes → underscores), the inverse of `decode-name`.
//!
//! Dep-holding: captures the module's shared `Arc<ToolNameCodec>` for symmetry
//! with the rest of the family (encoding itself is a pure slash→underscore
//! transform, but it lives on the codec so the family shares one state handle).
//!
//! ## Gating
//!
//! `AiSafe` — pure name transform, no side effects.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ToolNameParams;
use crate::tool_parsing::ToolNameCodec;

/// The encoded, API-safe tool name.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/tool_parsing/EncodedToolName.ts"
)]
pub struct EncodedToolName {
    /// The tool name with slashes replaced by underscores (API transmission form).
    pub encoded: String,
}

crate::action_command! {
    /// Encode a canonical slash-namespaced tool name into its API-safe transmission
    /// form (slashes become underscores) — the inverse of `decode-name`.
    pub struct ToolParsingEncodeName {
        codec: Arc<ToolNameCodec>,
    }
    name: "tool-parsing/encode-name",
    // Internal: the tool-name codec is substrate machinery for the wire form of a
    // model's tool names — never a citizen-facing task tool.
    access: Internal,
    params: ToolNameParams,
    output: EncodedToolName,
    run(this, _ctx, p) => {
        Ok(EncodedToolName {
            encoded: this.codec.encode(&p.name),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand, Ctx};

    // what this catches: name/access wiring — the name codec is substrate machinery,
    // gated Internal so it never reaches the persona AiSafe tool surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(ToolParsingEncodeName::NAME, "tool-parsing/encode-name");
        assert!(matches!(
            ToolParsingEncodeName::ACCESS,
            AccessLevel::Internal
        ));
    }

    // what this catches: slashes become underscores (the transmission form).
    #[tokio::test]
    async fn encodes_slashes_to_underscores() {
        let cmd = ToolParsingEncodeName {
            codec: Arc::new(ToolNameCodec::new()),
        };
        let out = cmd
            .run(
                &Ctx::default(),
                ToolNameParams {
                    name: "collaboration/chat/send".to_string(),
                },
            )
            .await
            .expect("encode must succeed");
        assert_eq!(out.encoded, "collaboration_chat_send");
    }
}
