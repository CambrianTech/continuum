//! `events/resolve-channel` — resolve the airc channel a broadcast event maps to,
//! given its class and payload. Channel strategies that depend on payload fields
//! (`ByRoomId`, `ByPeerId`) extract them from the payload. Used by the
//! AircEventTransport at emit time. Read-only resolution → `AiSafe`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::events::resolve_event_class_channel;
use crate::sdk_codegen::CommandError;

/// Inputs to `events/resolve-channel`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/events/ResolveEventChannelParams.ts"
)]
pub struct ResolveEventChannelParams {
    /// The event-class name whose channel to resolve.
    pub name: String,
    /// The event payload. `ByRoomId` / `ByPeerId` strategies read `roomId` /
    /// `peerId` from this; other strategies ignore it.
    #[serde(default)]
    #[ts(type = "Record<string, unknown>")]
    pub payload: Value,
}

/// Result of `events/resolve-channel`: the resolved airc channel name.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/events/ResolveEventChannelResult.ts"
)]
pub struct ResolveEventChannelResult {
    /// The airc channel the event routes to.
    pub channel: String,
}

crate::action_command! {
    /// Resolve the airc channel a broadcast event of the given class routes to,
    /// given its payload. Fails loud when the class is undeclared or its strategy
    /// needs a payload field the payload doesn't carry.
    pub struct ResolveEventChannel;
    name: "events/resolve-channel",
    access: AiSafe,
    params: ResolveEventChannelParams,
    output: ResolveEventChannelResult,
    run(_this, _ctx, p) => {
        let channel = resolve_event_class_channel(&p.name, &p.payload)
            .map_err(|e| CommandError::Invalid(e.to_string()))?;
        Ok(ResolveEventChannelResult { channel })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{declare_event_class, EventClassChannelStrategy, EventClassConfig};
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: name/access wiring — channel resolution is a read on the
    // AiSafe surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(ResolveEventChannel::NAME, "events/resolve-channel");
        assert!(matches!(
            ResolveEventChannel::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: a Global-strategy class resolves to its fixed channel —
    // the typed path computes the same routing the legacy handler did.
    #[tokio::test]
    async fn resolves_global_channel() {
        let name = "typed-test:resolve-global";
        declare_event_class(
            name,
            &EventClassConfig {
                broadcast: true,
                channel: Some(EventClassChannelStrategy::Global),
                schema_version: "v1".into(),
                on_unknown_schema: None,
                description: None,
            },
        )
        .unwrap();

        let out = ResolveEventChannel
            .run(
                &Ctx::default(),
                ResolveEventChannelParams {
                    name: name.into(),
                    payload: serde_json::json!({}),
                },
            )
            .await
            .unwrap();
        assert_eq!(out.channel, "global");
    }
}
