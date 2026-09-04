//! `tool-parsing/register-tools` — teach the shared [`ToolNameCodec`] the set of
//! canonical tool names so it can later decode the mangled variants models emit.
//!
//! Dep-holding: captures the module's shared `Arc<ToolNameCodec>`, so the names
//! registered here are visible to `decode-name` / `encode-name` calls that route
//! through the SAME codec.
//!
//! ## Gating
//!
//! `Privileged` — mutates shared codec state (the canonical-name table), so it is
//! a write, not a read.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::tool_parsing::ToolNameCodec;

/// The canonical tool names to register (e.g. `["code/write", "code/read"]`).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/tool_parsing/ToolRegisterParams.ts"
)]
pub struct ToolRegisterParams {
    /// Canonical, slash-namespaced tool names to add to the codec's table.
    pub tools: Vec<String>,
}

/// How many names were registered in this call, and the codec's running total.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/tool_parsing/ToolRegistrationReport.ts"
)]
pub struct ToolRegistrationReport {
    /// Number of names supplied in this call.
    pub registered: u32,
    /// Total distinct names the codec now knows.
    pub total: u32,
}

crate::action_command! {
    /// Register a batch of canonical tool names with the shared codec so model-
    /// produced variants (underscored, prefixed, double-underscored) can later be
    /// decoded back to them. Returns this batch's count and the codec's running
    /// total.
    pub struct ToolParsingRegisterTools {
        codec: Arc<ToolNameCodec>,
    }
    name: "tool-parsing/register-tools",
    access: Privileged,
    params: ToolRegisterParams,
    output: ToolRegistrationReport,
    run(this, _ctx, p) => {
        let registered = p.tools.len() as u32;
        this.codec.register_all(&p.tools);
        Ok(ToolRegistrationReport {
            registered,
            total: this.codec.count() as u32,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand, Ctx};

    // what this catches: name/access wiring — registering mutates shared codec
    // state, so it is Privileged (not AiSafe like the read verbs).
    #[test]
    fn name_and_access_wired() {
        assert_eq!(
            ToolParsingRegisterTools::NAME,
            "tool-parsing/register-tools"
        );
        assert!(matches!(
            ToolParsingRegisterTools::ACCESS,
            AccessLevel::Privileged
        ));
    }

    // what this catches: registered count + running total are reported, and the
    // names actually land in the shared codec (decode of an encoded variant
    // resolves back to the canonical name).
    #[tokio::test]
    async fn registers_into_shared_codec() {
        let codec = Arc::new(ToolNameCodec::new());
        let cmd = ToolParsingRegisterTools {
            codec: codec.clone(),
        };
        let out = cmd
            .run(
                &Ctx::default(),
                ToolRegisterParams {
                    tools: vec![
                        "code/write".to_string(),
                        "code/read".to_string(),
                        "collaboration/chat/send".to_string(),
                    ],
                },
            )
            .await
            .expect("register must succeed");
        assert_eq!(out.registered, 3);
        assert_eq!(out.total, 3);
        // the same codec now decodes the underscored variant
        assert_eq!(codec.decode("code_write"), "code/write");
    }
}
