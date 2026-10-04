//! `access/get` and `access/set`: a citizen's access is COMMAND-BASED like everything else
//! (Joel, 2026-09-28). The policy (the capability threshold and per-citizen decisions) is read
//! and changed only through these verbs; see `routing::access_decision` for how it decides.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::modules::grid::node::TrustLevel;
use crate::routing::access_decision::{self, AccessDecision, AccessPolicy};
use crate::sdk_codegen::CommandError;

/// Wire shape for `access/get`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/access/AccessGetParams.ts")]
pub struct AccessGetParams {}

/// What `access/get` reports: the policy, and what it means on this node right now.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/access/AccessGetResult.ts")]
pub struct AccessGetResult {
    pub policy: AccessPolicy,
    /// The capability rank of the model this node serves now (a local citizen's level).
    #[ts(optional)]
    pub local_cognitive_rank: Option<u8>,
    /// The level a local citizen gets right now under this policy.
    pub local_citizen_level: TrustLevel,
}

/// The level `access/set` records for one citizen, or `clear` to remove her decision so the
/// capability default applies again. A closed set on the wire: serde refuses anything else,
/// so no caller string is ever interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/access/AccessLevelChange.ts")]
#[serde(rename_all = "lowercase")]
pub enum AccessLevelChange {
    Blocked,
    Provisional,
    Trusted,
    Owner,
    Clear,
}

impl From<AccessLevelChange> for Option<TrustLevel> {
    fn from(change: AccessLevelChange) -> Self {
        match change {
            AccessLevelChange::Blocked => Some(TrustLevel::Blocked),
            AccessLevelChange::Provisional => Some(TrustLevel::Provisional),
            AccessLevelChange::Trusted => Some(TrustLevel::Trusted),
            AccessLevelChange::Owner => Some(TrustLevel::Owner),
            AccessLevelChange::Clear => None,
        }
    }
}

/// Wire shape for `access/set`. Every field is optional; what is given changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/access/AccessSetParams.ts")]
pub struct AccessSetParams {
    /// The capability rank at or above which a citizen gets the full working surface.
    #[ts(optional)]
    pub full_access_min_rank: Option<u8>,
    /// The citizen (peer id) a decision is about.
    #[ts(optional, type = "string")]
    pub peer: Option<Uuid>,
    /// The level to grant that citizen, or `clear` to remove her decision.
    #[ts(optional)]
    pub level: Option<AccessLevelChange>,
    /// Why: reputation, a review, an incident. Required with a level.
    #[ts(optional)]
    pub reason: Option<String>,
}

crate::action_command! {
    /// Read the access policy: the capability threshold for the full working surface, every
    /// recorded per-citizen decision, and the level a local citizen gets right now.
    pub struct AccessGet;
    name: "access/get",
    access: AiSafe,
    params: AccessGetParams,
    output: AccessGetResult,
    run(_this, _ctx, _p) => {
        let policy = access_decision::policy();
        let rank = access_decision::local_cognitive_rank();
        let level = policy.resident_trust(Uuid::nil());
        Ok(AccessGetResult { policy, local_cognitive_rank: rank, local_citizen_level: level })
    }
}

crate::action_command! {
    /// Change the access policy (owner only): set the capability threshold for the full
    /// working surface, and/or record a decision for one citizen (a level and why), or clear
    /// it. A recorded decision is how earned access arrives, and it wins over the default.
    pub struct AccessSet;
    name: "access/set",
    access: Privileged,
    params: AccessSetParams,
    output: AccessGetResult,
    run(_this, ctx, p) => {
        let mut policy = access_decision::policy();
        if let Some(rank) = p.full_access_min_rank {
            policy.full_access_min_rank = rank;
        }
        match (p.peer, p.level) {
            (Some(peer), Some(change)) => {
                policy.decisions.retain(|d| d.peer != peer);
                if let Some(level) = Option::<TrustLevel>::from(change) {
                    let reason = p.reason.clone().filter(|r| !r.trim().is_empty()).ok_or_else(|| {
                        CommandError::Invalid("access/set: a decision needs a reason (it is shown wherever it is read)".into())
                    })?;
                    policy.decisions.push(AccessDecision {
                        peer,
                        level,
                        reason,
                        decided_by: ctx.caller.as_ref().map(|c| c.peer_id.as_uuid()),
                        decided_ms: chrono::Utc::now().timestamp_millis().max(0) as u64,
                    });
                }
            }
            (None, None) => {}
            (Some(_), None) | (None, Some(_)) => return Err(CommandError::Invalid("access/set: a decision needs both peer and level".into())),
        }
        access_decision::store(policy.clone()).map_err(CommandError::Internal)?;
        let rank = access_decision::local_cognitive_rank();
        let level = policy.resident_trust(Uuid::nil());
        Ok(AccessGetResult { policy, local_cognitive_rank: rank, local_citizen_level: level })
    }
}
