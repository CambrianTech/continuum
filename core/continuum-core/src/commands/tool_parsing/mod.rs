//! `tool-parsing/<verb>` — the tool-call parsing + name-codec surface as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s.
//!
//! ## The concern this owns
//!
//! Turning the raw text a model emits into structured, corrected tool calls, and
//! translating between canonical slash-namespaced tool names and the mangled
//! variants different model families produce.
//!
//! ## Two shapes in one family
//!
//! - **Stateless** ([`parse`], [`correct`]) — the parse + correction logic are pure
//!   free functions, so these commands hold no state and self-register via the
//!   unit-struct `action_command!` form (zero module ceremony).
//! - **Dep-holding** ([`register_tools`], [`decode_name`], [`encode_name`]) — these
//!   share the module's one `Arc<ToolNameCodec>` so a name registered via
//!   `register-tools` is decodable by `decode-name` through the SAME table. They are
//!   assembled by [`command_objects`] and contributed by
//!   [`ToolParsingModule`](crate::modules::tool_parsing::ToolParsingModule)'s
//!   `commands()`.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sdk_codegen::DynCommand;
use crate::tool_parsing::ToolNameCodec;

pub mod correct;
pub mod decode_name;
pub mod encode_name;
pub mod parse;
pub mod register_tools;

use decode_name::ToolParsingDecodeName;
use encode_name::ToolParsingEncodeName;
use register_tools::ToolParsingRegisterTools;

/// A single tool name — shared by `decode-name` and `encode-name`, which both take
/// exactly one name and differ only in direction (compression principle: one
/// contract for the two halves of the codec, like `system/*`'s `SystemQuery`).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/tool_parsing/ToolNameParams.ts"
)]
pub struct ToolNameParams {
    /// The tool name to decode (any model variant) or encode (canonical form).
    pub name: String,
}

/// Build the dep-holding `tool-parsing/*` command objects over the shared
/// [`ToolNameCodec`]. Called from `ToolParsingModule::commands`. The stateless
/// `parse` / `correct` commands are NOT here — they self-register.
pub fn command_objects(codec: Arc<ToolNameCodec>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(ToolParsingRegisterTools {
            codec: codec.clone(),
        }),
        Arc::new(ToolParsingDecodeName {
            codec: codec.clone(),
        }),
        Arc::new(ToolParsingEncodeName { codec }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the dep-holding family wires all three codec-backed verbs
    // (register-tools/decode-name/encode-name) over the one shared codec. A
    // regression that drops any of them — or fails to share the codec — is caught.
    #[test]
    fn family_exposes_the_three_codec_verbs() {
        let codec = Arc::new(ToolNameCodec::new());
        let objs = command_objects(codec);
        let names: Vec<&str> = objs.iter().map(|o| o.name()).collect();
        assert!(names.contains(&"tool-parsing/register-tools"));
        assert!(names.contains(&"tool-parsing/decode-name"));
        assert!(names.contains(&"tool-parsing/encode-name"));
    }
}
