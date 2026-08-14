//! `tool-parsing/decode-name` — resolve a model-produced tool name (underscored,
//! prefixed, double-underscored, etc.) back to its canonical slash-namespaced
//! form, using the shared [`ToolNameCodec`]'s registered table.
//!
//! Dep-holding: captures the module's shared `Arc<ToolNameCodec>`, so it resolves
//! against names previously taught via `register-tools`.
//!
//! ## Gating
//!
//! `AiSafe` — read-only lookup against the codec table.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ToolNameParams;
use crate::tool_parsing::ToolNameCodec;

/// The decoded canonical name and whether decoding changed the input.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/tool_parsing/DecodedToolName.ts"
)]
pub struct DecodedToolName {
    /// The canonical, slash-namespaced tool name.
    pub decoded: String,
    /// True when `decoded` differs from the supplied `name` (i.e. a fixup applied).
    pub changed: bool,
}

crate::action_command! {
    /// Decode a model-produced tool name back to its canonical slash-namespaced
    /// form via the shared codec (5-step resolution: exact, strip prefix,
    /// normalise, double-underscore, single-underscore). Reports whether the
    /// resolution changed the input.
    pub struct ToolParsingDecodeName {
        codec: Arc<ToolNameCodec>,
    }
    name: "tool-parsing/decode-name",
    // Internal: the tool-name codec is substrate machinery for interpreting a
    // model's emitted tool names — never a citizen-facing task tool.
    access: Internal,
    params: ToolNameParams,
    output: DecodedToolName,
    run(this, _ctx, p) => {
        let decoded = this.codec.decode(&p.name);
        let changed = decoded != p.name;
        Ok(DecodedToolName { decoded, changed })
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
        assert_eq!(ToolParsingDecodeName::NAME, "tool-parsing/decode-name");
        assert!(matches!(
            ToolParsingDecodeName::ACCESS,
            AccessLevel::Internal
        ));
    }

    // what this catches: an underscored variant of a registered name decodes back
    // to the canonical form and reports changed=true; a prefixed variant resolves
    // too.
    #[tokio::test]
    async fn decodes_registered_variants() {
        let codec = Arc::new(ToolNameCodec::new());
        codec.register_all(&["code/write".to_string()]);
        let cmd = ToolParsingDecodeName {
            codec: codec.clone(),
        };
        let out = cmd
            .run(
                &Ctx::default(),
                ToolNameParams {
                    name: "code_write".to_string(),
                },
            )
            .await
            .expect("decode must succeed");
        assert_eq!(out.decoded, "code/write");
        assert!(out.changed);

        let prefixed = cmd
            .run(
                &Ctx::default(),
                ToolNameParams {
                    name: "$FUNCTIONS.code_write".to_string(),
                },
            )
            .await
            .expect("decode must succeed");
        assert_eq!(prefixed.decoded, "code/write");
    }
}
