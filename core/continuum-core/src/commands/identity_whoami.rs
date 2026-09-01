//! `identity/whoami` — THE identity resolution point for a connecting client.
//!
//! Why (Joel, 2026-09-01: "there's two of me, one called 'you' and another
//! 'joel' — this is what happens when you break separation of concerns"): the
//! web client MINTED a per-browser uuid (`persistentIdentity()` in
//! localStorage) and used it as the human's identity — so every browser
//! profile, private window, and the eye-node's harness page each became a
//! phantom human in the directory and the call grid, beside the REAL durable
//! identity the core already holds (the operator self-peer, #27). Three
//! joel-shaped peers in one evening.
//!
//! The design ([[the-grid-identity-spine-durable-id-fluid-location]], and the
//! `userId → sessionId → contextId` hierarchy): identity is DURABLE and owned
//! by the substrate; a client session is a CONNECTION to it, never a mint.
//! Clients ask `identity/whoami` at boot and adopt the answer.
//!
//! Today's answer is the local-first one: a caller-less local session IS the
//! node's operator (one human per node); a persona toolbelt caller is herself.
//! Real multi-user auth replaces the resolution INSIDE this verb later — the
//! wire contract stays.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/identity/WhoamiParams.ts")]
pub struct WhoamiParams {}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/identity/WhoamiResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct WhoamiResult {
    /// The caller's DURABLE identity uuid — the one id this human/persona is
    /// everywhere (directory, calls, chat authorship).
    pub id: String,
    /// Display name (the operator's OS user, or the persona's agent name).
    pub name: String,
    /// `"human"` | `"persona"`.
    pub kind: String,
}

crate::action_command! {
    /// Who am I on this substrate? Returns the caller's DURABLE identity (uuid +
    /// name + kind). Clients call this at boot and adopt the answer — a client
    /// must never mint its own identity ([[one-logical-decision-one-place]]).
    pub struct IdentityWhoami;
    name: "identity/whoami",
    access: AiSafe,
    params: WhoamiParams,
    output: WhoamiResult,
    run(_this, ctx, _p) => {
        // A persona toolbelt caller is herself.
        if let Some(caller) = ctx.caller.as_ref() {
            let id = caller.peer_id.as_uuid();
            let name = crate::persona::PersonaAircRuntimeRegistry::try_global()
                .and_then(|r| r.get(id))
                .map(|rt| rt.agent_name().to_string())
                .unwrap_or_else(|| id.to_string());
            return Ok(WhoamiResult { id: id.to_string(), name, kind: "persona".into() });
        }
        // A caller-less local session is the node's OPERATOR (the durable human
        // identity, #27). Not online yet this boot = a loud retryable error —
        // never a minted stand-in.
        let rt = crate::persona::operator_peer::operator_runtime().ok_or_else(|| {
            CommandError::Internal(
                "the operator self-peer is not online yet this boot — retry shortly \
                 (it starts beside the citizens); never mint a local identity instead"
                    .into(),
            )
        })?;
        Ok(WhoamiResult {
            id: rt.airc().peer_id().as_uuid().to_string(),
            name: rt.agent_name().to_string(),
            kind: "human".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the identity spine's resolution verb — its wire name
    // (every client boots against it) and AiSafe access (asking who you are is
    // the first thing any session does).
    #[test]
    fn whoami_is_aisafe_under_its_wire_name() {
        assert_eq!(IdentityWhoami::NAME, "identity/whoami");
        assert_eq!(IdentityWhoami::ACCESS, AccessLevel::AiSafe);
    }
}
