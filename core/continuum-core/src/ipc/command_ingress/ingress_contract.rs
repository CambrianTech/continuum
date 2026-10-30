//! The ingress contract: when a peer's command frame may execute on this node.
//!
//! Incident 2026-10-30 00:28-00:38Z: peer `e9093c03` (tier `untrusted` in this node's
//! airc trust store; every node of ours is `own_account`) published
//! `persona/placement/reserve` and `ai/generate` requests into #general, and the M5
//! executed them (`airc.command.accepted caller=e9093c03`): the only gate in front of
//! the executor was "addressed to me", and the command ACL admits any airc stranger at
//! `Provisional` for exactly those two verbs.
//!
//! A command reaches this node only through its identity's addressed inbox: airc has
//! verified the frame's sender, addressee and kind (`routing::addressed_inbox`). Room
//! subscriptions carry no command handling at all. The rules below read only those
//! verified fields and the typed headers, before the payload is decoded:
//! - the frame is airc's typed `Command` (a `CommandResult` is not a request);
//! - it is addressed to this node (`to() == me`);
//! - it names its command in `continuum.command.path`, and the decoded request names the
//!   same command (a payload cannot re-route the decision made on the header);
//! - the sender's tier in this node's airc trust store meets the command's required tier,
//!   or the sender presented an owner-signed grant verified to confer it. The required
//!   tier is the command ACL's own declaration
//!   ([`command_acl::required_trust`](crate::routing::command_acl::required_trust)),
//!   projected onto airc's tiers by [`required_tier`]: a `Provisional` command needs a
//!   `friend`, anything above needs `own_account`, and `untrusted` meets nothing.
//!
//! The executor's own trust gate (`GridTrustAuthPolicy`) still runs after this; the
//! contract is the floor the transport identity must clear first.

use std::path::PathBuf;
use std::sync::Arc;

use airc_core::PeerId;
use airc_lib::TrustTier;
use async_trait::async_trait;

use crate::routing::addressed_inbox::{AddressedFrame, AddressedKind};
use crate::routing::trust_level::TrustLevel;
use crate::routing::HEADER_COMMAND_PATH;

/// Why an addressed frame was refused before it executed. Each variant names the rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngressRefusal {
    /// The frame is not a `Command` (a `CommandResult` that no pending request claimed).
    NotACommand,
    /// Addressed to another peer.
    AddressedElsewhere,
    /// No `continuum.command.path` header: the gate cannot decide without decoding the payload.
    MissingCommandPath,
    /// The presented capability grant header does not decode.
    MalformedGrant,
    GrantAuthorizerUnavailable,
    PresentingKeyUnavailable,
    GrantRejected,
    MissingGenerationStamp,
    MalformedGenerationStamp,
    /// The payload is not an `AircCommandRequest`.
    MalformedRequest,
    /// The decoded request names a different command than its header.
    PathMismatch,
    /// The command is local-only; no remote caller may run it.
    LocalOnlyCommand,
    /// The sender has no row in this node's airc trust store.
    UnenrolledCaller,
    /// This node's airc trust store could not be read; refused rather than guessed.
    TrustStoreUnreadable,
    /// The sender's tier is below the command's.
    TierBelowRequired {
        tier: TrustTier,
        required: TrustTier,
    },
}

impl IngressRefusal {
    /// The rule's stable name, for probes and the refusal reply.
    pub fn rule(&self) -> &'static str {
        match self {
            Self::NotACommand => "not_a_command",
            Self::AddressedElsewhere => "addressed_elsewhere",
            Self::MissingCommandPath => "missing_command_path",
            Self::MalformedGrant => "malformed_grant",
            Self::GrantAuthorizerUnavailable => "grant_authorizer_unavailable",
            Self::PresentingKeyUnavailable => "presenting_key_unavailable",
            Self::GrantRejected => "grant_rejected",
            Self::MissingGenerationStamp => "missing_generation_stamp",
            Self::MalformedGenerationStamp => "malformed_generation_stamp",
            Self::MalformedRequest => "malformed_request",
            Self::PathMismatch => "path_mismatch",
            Self::LocalOnlyCommand => "local_only_command",
            Self::UnenrolledCaller => "unenrolled_caller",
            Self::TrustStoreUnreadable => "trust_store_unreadable",
            Self::TierBelowRequired { .. } => "tier_below_required",
        }
    }

    /// Whether the sender is owed a reply: a frame that was not a command to this node
    /// is nobody's to answer here.
    pub fn answered(&self) -> bool {
        !matches!(self, Self::NotACommand | Self::AddressedElsewhere)
    }

    /// The refusal reply's text: the rule and the tiers, never the request's params.
    pub fn reply_message(&self, path: &str) -> String {
        match self {
            Self::TierBelowRequired { tier, required } => format!(
                "ingress refused {path}: {} (caller tier {tier}, command needs {required})",
                self.rule()
            ),
            other => format!("ingress refused {path}: {}", other.rule()),
        }
    }
}

/// Rules on the verified frame itself: a `Command`, to `me`, naming its command. Returns
/// the command path from the typed header. Reads no payload.
pub fn admit_frame(frame: &dyn AddressedFrame, me: PeerId) -> Result<String, IngressRefusal> {
    if frame.kind() != AddressedKind::Command {
        return Err(IngressRefusal::NotACommand);
    }
    if frame.to() != me {
        return Err(IngressRefusal::AddressedElsewhere);
    }
    frame
        .headers()
        .get(HEADER_COMMAND_PATH)
        .cloned()
        .ok_or(IngressRefusal::MissingCommandPath)
}

/// The airc tier a remote caller needs for `path`: the command ACL's declared tier,
/// projected onto airc's trust gradient. `Provisional` (the ACL's cross-grid floor, e.g.
/// `ai/generate`) needs a `friend` the operator enrolled; `Trusted` and `Owner` need
/// `own_account`. `untrusted` meets no command.
pub fn required_tier(path: &str) -> Result<TrustTier, IngressRefusal> {
    match crate::routing::command_acl::required_trust(path) {
        None => Err(IngressRefusal::LocalOnlyCommand),
        Some(TrustLevel::Provisional) => Ok(TrustTier::Friend),
        Some(TrustLevel::Blocked | TrustLevel::Trusted | TrustLevel::Owner) => {
            Ok(TrustTier::OwnAccount)
        }
    }
}

/// Position on airc's gradient, highest first in `TrustTier::ALL_VARIANTS`.
fn rank(tier: TrustTier) -> u8 {
    match tier {
        TrustTier::OwnMachine => 3,
        TrustTier::OwnAccount => 2,
        TrustTier::Friend => 1,
        TrustTier::Untrusted => 0,
    }
}

/// Rule (b). `tier` is the caller's row in this node's trust store (`Ok(None)` = not
/// enrolled); `conferred_by_grant` is true only when a presented owner-signed grant was
/// verified against the authenticated sender key to confer this command.
pub fn admit_caller(
    tier: Result<Option<TrustTier>, String>,
    path: &str,
    conferred_by_grant: bool,
) -> Result<(), IngressRefusal> {
    let required = required_tier(path)?;
    if conferred_by_grant {
        return Ok(());
    }
    let tier = tier
        .map_err(|_| IngressRefusal::TrustStoreUnreadable)?
        .ok_or(IngressRefusal::UnenrolledCaller)?;
    if rank(tier) < rank(required) {
        return Err(IngressRefusal::TierBelowRequired { tier, required });
    }
    Ok(())
}

/// Where a caller's airc trust tier is read: the one seam the ingress depends on.
#[async_trait]
pub trait PeerTierSource: Send + Sync {
    /// The caller's tier in this node's trust store; `Ok(None)` = not enrolled.
    async fn tier_of(&self, peer: PeerId) -> Result<Option<TrustTier>, String>;
}

/// This node's airc trust store (scope rows first, then the machine account's), the
/// same records `airc peers` and the account registry's tier detection write.
pub struct AircTrustStore {
    home: PathBuf,
}

impl AircTrustStore {
    pub fn new(home: PathBuf) -> Arc<Self> {
        Arc::new(Self { home })
    }
}

#[async_trait]
impl PeerTierSource for AircTrustStore {
    async fn tier_of(&self, peer: PeerId) -> Result<Option<TrustTier>, String> {
        let peers = airc_lib::peer_trust_snapshot(&self.home)
            .await
            .map_err(|e| format!("read airc trust store at {}: {e}", self.home.display()))?;
        Ok(peers
            .into_iter()
            .find(|row| row.peer_id == peer)
            .map(|row| row.tier))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::addressed_inbox::fixtures::FakeFrame;

    // what this catches: the frame rules on airc's verified fields. A command for
    // another peer is not ours and owed no reply; a CommandResult is not a request; a
    // command with no path header cannot be decided without decoding; a Command to this
    // node naming its command passes.
    #[test]
    fn only_a_command_to_this_node_naming_its_command_passes_the_frame_rules() {
        let (me, them) = (PeerId::new(), PeerId::new());
        let reserve = Some("persona/placement/reserve");
        let params = serde_json::json!({ "prompt": "a resident's whole prompt" });
        let elsewhere = FakeFrame::command(
            them,
            PeerId::new(),
            reserve,
            "persona/placement/reserve",
            params.clone(),
        );
        let refused = admit_frame(&elsewhere, me).expect_err("refused");
        assert_eq!(refused, IngressRefusal::AddressedElsewhere);
        assert!(!refused.answered(), "not ours to answer");
        let mut result = FakeFrame::command(
            them,
            me,
            reserve,
            "persona/placement/reserve",
            params.clone(),
        );
        for kind in [AddressedKind::CommandResult, AddressedKind::CommandProgress] {
            result.kind = kind;
            assert_eq!(
                admit_frame(&result, me).expect_err("only a request may execute"),
                IngressRefusal::NotACommand
            );
        }
        let unnamed =
            FakeFrame::command(them, me, None, "persona/placement/reserve", params.clone());
        assert_eq!(
            admit_frame(&unnamed, me).expect_err("refused"),
            IngressRefusal::MissingCommandPath
        );
        let here = FakeFrame::command(them, me, reserve, "persona/placement/reserve", params);
        assert_eq!(
            admit_frame(&here, me).expect("direct, here, named"),
            "persona/placement/reserve"
        );
    }

    // what this catches: the tier rule on the incident's caller. An `untrusted` (or
    // unenrolled, or unreadable-store) sender is refused even the ACL's Provisional
    // verbs; `own_account` passes; a `friend` reaches the Provisional floor but not an
    // Owner-tier command; a verified grant confers its command at any tier. The refusal
    // reply names the rule and tiers and never carries params.
    #[test]
    fn the_caller_tier_must_meet_the_commands_declared_tier() {
        let reserve = "persona/placement/reserve";
        let refused =
            admit_caller(Ok(Some(TrustTier::Untrusted)), reserve, false).expect_err("refused");
        assert_eq!(
            refused,
            IngressRefusal::TierBelowRequired {
                tier: TrustTier::Untrusted,
                required: TrustTier::Friend
            }
        );
        assert!(refused.answered());
        assert!(!refused.reply_message(reserve).contains("prompt"));
        assert_eq!(
            admit_caller(Ok(None), "ai/generate", false),
            Err(IngressRefusal::UnenrolledCaller)
        );
        assert_eq!(
            admit_caller(Err("locked".into()), "ai/generate", false),
            Err(IngressRefusal::TrustStoreUnreadable)
        );
        assert_eq!(
            admit_caller(Ok(Some(TrustTier::OwnAccount)), reserve, false),
            Ok(())
        );
        assert_eq!(
            admit_caller(Ok(Some(TrustTier::OwnMachine)), "data/delete", false),
            Ok(())
        );
        assert_eq!(
            admit_caller(Ok(Some(TrustTier::Friend)), "ai/generate", false),
            Ok(())
        );
        assert_eq!(
            admit_caller(Ok(Some(TrustTier::Friend)), "data/delete", false),
            Err(IngressRefusal::TierBelowRequired {
                tier: TrustTier::Friend,
                required: TrustTier::OwnAccount
            })
        );
        assert_eq!(
            admit_caller(Ok(Some(TrustTier::Untrusted)), "compute/echo", true),
            Ok(())
        );
    }
}
