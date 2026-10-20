//! Read-only explanation of the current local inbound AIRC tier policy.
//! This is not enrollment, reachability, remote permission, or a command verdict:
//! verified capability grants and command ACLs still apply at the executor gate.
use super::{AccessDecision, AccessPolicy, TrustLevel, Uuid};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LocalInboundAircAuthorization {
    Uncorrelated,
    Unrecorded { effective_level: TrustLevel },
    Recorded { effective_level: TrustLevel, decision: AccessDecision },
    PolicyUnreadable { effective_level: TrustLevel },
}

impl AccessPolicy {
    /// Explain one snapshot using the same inputs as the production AIRC gate.
    /// Legacy address trust and enrollment do not confer authenticated permissions.
    pub fn local_inbound_airc_authorization(&self, peer: Option<Uuid>) -> LocalInboundAircAuthorization {
        let Some(peer) = peer else {
            return LocalInboundAircAuthorization::Uncorrelated;
        };
        let effective_level = self.citizen_trust(peer, false, None, None);
        if self.unreadable {
            LocalInboundAircAuthorization::PolicyUnreadable { effective_level }
        } else if let Some(decision) = self.decisions.iter().find(|decision| decision.peer == peer) {
            LocalInboundAircAuthorization::Recorded { effective_level, decision: decision.clone() }
        } else {
            LocalInboundAircAuthorization::Unrecorded { effective_level }
        }
    }
}
