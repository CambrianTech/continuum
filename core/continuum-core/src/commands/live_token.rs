//! `live/token` — mint a LiveKit access token so a CLIENT joins the media
//! plane as a REAL WebRTC participant.
//!
//! Why (Joel, 2026-08-31: "hope to god you are using webrtc streaming over udp
//! and not json… boundaries are the killers"): the web's live face consumed the
//! call server's raw-RGBA-over-WebSocket tee — uncompressed pixels over TCP,
//! decoded with main-thread `putImageData`, ~1MB per 640×360 frame per citizen.
//! The REAL plane already exists (livekit-server on the boot rail; the bridge
//! publishes每 citizen's Bevy feed as hardware-encoded UDP tracks) — the only
//! missing piece was a token for a human client to join it. With this verb the
//! browser connects via livekit-client, subscribes citizen tracks into <video>
//! elements (GPU-decoded, congestion-controlled, zero JS pixel work) and
//! publishes camera/mic with native WebRTC capture. The WS tee demotes back to
//! the glass-box harness it was born as. (Native node↔node media rides airc's
//! UDP transport; LiveKit is the BROWSER edge — browsers cannot speak raw UDP.)
//!
//! Identity: the token is minted for the CALLER (persona toolbelt caller or the
//! operator self-peer) — same resolution chain as `chat/send`; a client may
//! request for an explicit `identity` uuid only matching itself later when
//! delegation is designed. Grants are room-scoped.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/live/LiveTokenParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct LiveTokenParams {
    /// The call's room id (the activity room uuid — call_id == room_id).
    pub room: String,
    /// Identity the token is minted for. Omit = the caller (persona, or the
    /// operator self-peer for a caller-less CLI/web session — the same chain
    /// as chat/send).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub identity: Option<String>,
    /// Display name on the track (defaults to the identity).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/live/LiveTokenResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct LiveTokenResult {
    /// The SFU websocket URL the client connects to (signalling; media is UDP).
    pub url: String,
    /// The signed access token (room-scoped publish+subscribe grants).
    pub token: String,
    /// The identity the token was minted for (uuid string).
    pub identity: String,
}

crate::action_command! {
    /// Mint a LiveKit access token for the caller to join a call's media plane as a
    /// native WebRTC participant (UDP, hardware codecs). Room-scoped grants:
    /// publish + subscribe. Omit `identity` to act as yourself.
    pub struct LiveToken;
    name: "live/token",
    access: AiSafe,
    params: LiveTokenParams,
    output: LiveTokenResult,
    run(_this, ctx, p) => {
        #[cfg(not(feature = "livekit-webrtc"))]
        {
            let _ = (ctx, p);
            return Err(CommandError::Internal(
                "this build carries no livekit-webrtc feature — the media plane is \
                 unavailable on this node".into(),
            ));
        }
        #[cfg(feature = "livekit-webrtc")]
        {
            let identity = p
                .identity
                .or_else(|| ctx.caller.as_ref().map(|c| c.peer_id.as_uuid().to_string()))
                .or_else(|| {
                    crate::persona::operator_peer::operator_runtime()
                        .map(|rt| rt.airc().peer_id().as_uuid().to_string())
                })
                .ok_or_else(|| CommandError::Invalid(
                    "no identity: pass one, or wait for the operator self-peer this boot".into(),
                ))?;
            let name = p.name.unwrap_or_else(|| identity.clone()); // JUSTIFIED unwrap_or_else: declared default — display name falls back to the identity itself
            // Same key resolution as the bridge (dev keys as the local default —
            // the boot rail starts livekit-server with them).
            let api_key = std::env::var("LIVEKIT_API_KEY").unwrap_or_else(|_| "devkey".into()); // JUSTIFIED unwrap_or_else: the boot rail starts livekit-server with these dev creds — same default as the bridge (agent.rs DEV_API_KEY)
            let api_secret =
                std::env::var("LIVEKIT_API_SECRET").unwrap_or_else(|_| "secret".into()); // JUSTIFIED unwrap_or_else: dev-cred default paired with the boot rail (see api_key above)
            let url = std::env::var("LIVEKIT_URL").unwrap_or_else(|_| "ws://localhost:7880".into()); // JUSTIFIED unwrap_or_else: the boot rail binds the SFU here; env overrides for remote SFUs
            let token = livekit_api::access_token::AccessToken::with_api_key(&api_key, &api_secret)
                .with_identity(&identity)
                .with_name(&name)
                .with_grants(livekit_api::access_token::VideoGrants {
                    room_join: true,
                    room: p.room.clone(),
                    can_publish: true,
                    can_subscribe: true,
                    can_publish_data: true,
                    ..Default::default()
                })
                .to_jwt()
                .map_err(|e| CommandError::Internal(format!("token mint failed: {e}")))?;
            Ok(LiveTokenResult { url, token, identity })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the media plane's entry verb — its wire name (the web
    // client binds to it) and AiSafe access (a citizen or the operator joining
    // their own call's media is the ordinary case; grants are room-scoped).
    #[test]
    fn token_verb_is_aisafe_under_its_wire_name() {
        assert_eq!(LiveToken::NAME, "live/token");
        assert_eq!(LiveToken::ACCESS, AccessLevel::AiSafe);
    }
}
