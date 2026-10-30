//! The ingress contract: when a peer's command frame may execute on this node.
//!
//! Incident 2026-10-30 00:28-00:38Z: peer `e9093c03` (tier `untrusted` in this node's
//! airc trust store; every node of ours is `own_account`) published
//! `persona/placement/reserve` and `ai/generate` requests into #general, and the M5
//! executed them (`airc.command.accepted caller=e9093c03`): the only gate in front of
//! the executor was "addressed to me", and the command ACL admits any airc stranger at
//! `Provisional` for exactly those two verbs.
//!
//! Every rule here is a HEADER fact, decided before the payload is decoded:
//! - the frame is a command request ([`ProtocolKind::CommandRequest`], from
//!   `continuum.body_hint`), never conversation or another protocol frame;
//! - (a) it arrived addressed to ONE peer ([`Addressing::Direct`]), never a room
//!   broadcast or a sibling-room reference, whatever its payload says;
//! - (c) the addressee is this node;
//! - it names its command in `continuum.command.path`, and the decoded request names
//!   the same command (a payload cannot re-route the decision made on the header);
//! - (b) the caller's tier in this node's airc trust store meets the command's
//!   required tier, or the caller presented an owner-signed grant verified to confer
//!   it. The required tier is the command ACL's own declaration
//!   ([`command_acl::required_trust`](crate::routing::command_acl::required_trust)),
//!   projected onto airc's tiers by [`required_tier`]: a `Provisional` command needs a
//!   `friend`, anything above needs `own_account`, and `untrusted` meets nothing.
//!
//! The executor's own trust gate (`GridTrustAuthPolicy`) still runs after this; the
//! contract is the floor the transport identity must clear first.

use std::path::PathBuf;
use std::sync::Arc;

use airc_core::{PeerId, TranscriptEvent};
use airc_lib::TrustTier;
use async_trait::async_trait;
use continuum_airc_protocol::{
    classify_inbound, Addressing, InboundEvent, ProtocolEnvelope, ProtocolKind,
};

use crate::routing::trust_level::TrustLevel;

/// Why a peer's command frame was refused before it executed. Each variant names the rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngressRefusal {
    /// The headers do not declare a command request (conversation, a reply, an event frame).
    NotACommandRequest,
    /// Rule (a): it arrived as a room broadcast or a sibling-room reference, not addressed
    /// to one peer. Never executed, whatever its payload says.
    NotDirect,
    /// Rule (c): addressed to another peer.
    AddressedElsewhere,
    /// No `continuum.command.path` header: the gate cannot decide without decoding the payload.
    MissingCommandPath,
    /// The decoded request names a different command than its header.
    PathMismatch,
    /// The command is local-only; no remote caller may run it.
    LocalOnlyCommand,
    /// Rule (b): the caller has no row in this node's airc trust store.
    UnenrolledCaller,
    /// Rule (b): this node's airc trust store could not be read; refused rather than guessed.
    TrustStoreUnreadable,
    /// Rule (b): the caller's tier is below the command's.
    TierBelowRequired {
        tier: TrustTier,
        required: TrustTier,
    },
}

impl IngressRefusal {
    /// The rule's stable name, for probes and the refusal reply.
    pub fn rule(&self) -> &'static str {
        match self {
            Self::NotACommandRequest => "not_a_command_request",
            Self::NotDirect => "not_direct",
            Self::AddressedElsewhere => "addressed_elsewhere",
            Self::MissingCommandPath => "missing_command_path",
            Self::PathMismatch => "path_mismatch",
            Self::LocalOnlyCommand => "local_only_command",
            Self::UnenrolledCaller => "unenrolled_caller",
            Self::TrustStoreUnreadable => "trust_store_unreadable",
            Self::TierBelowRequired { .. } => "tier_below_required",
        }
    }

    /// Whether the caller is owed a reply. A frame that was never a direct request to
    /// this node is nobody's to answer here (answering a broadcast would let one frame
    /// draw a reply from every node in the room).
    pub fn answered(&self) -> bool {
        !matches!(
            self,
            Self::NotACommandRequest | Self::NotDirect | Self::AddressedElsewhere
        )
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

/// A command request that passed the header rules: direct, addressed here, naming its command.
#[derive(Debug, Clone)]
pub struct AddressedCommand {
    pub envelope: ProtocolEnvelope,
    pub path: String,
}

/// Rules on the frame itself: a command request, direct, to `me`, naming its command.
/// Reads headers and the addressee only.
pub fn admit_frame(
    event: Arc<TranscriptEvent>,
    me: PeerId,
) -> Result<AddressedCommand, IngressRefusal> {
    let InboundEvent::Protocol(envelope) = classify_inbound(event) else {
        return Err(IngressRefusal::NotACommandRequest);
    };
    if envelope.kind() != &ProtocolKind::CommandRequest {
        return Err(IngressRefusal::NotACommandRequest);
    }
    match envelope.addressing() {
        Addressing::Direct(addressee) if addressee == me => {}
        Addressing::Direct(_) => return Err(IngressRefusal::AddressedElsewhere),
        Addressing::RoomBroadcast | Addressing::SiblingRoom(_) => {
            return Err(IngressRefusal::NotDirect)
        }
    }
    let path = envelope
        .command_path()
        .ok_or(IngressRefusal::MissingCommandPath)?
        .to_string();
    Ok(AddressedCommand { envelope, path })
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
    use airc_core::{Body, ClientId, EventId, MentionTarget, RoomId, TranscriptKind};
    use continuum_airc_protocol::{
        COMMAND_REQUEST_BODY_HINT, HEADER_COMMAND_PATH, HEADER_CONTINUUM_BODY_HINT,
    };

    fn frame(target: MentionTarget, path: Option<&str>) -> Arc<TranscriptEvent> {
        let mut headers = airc_core::Headers::new();
        headers.insert(
            HEADER_CONTINUUM_BODY_HINT.into(),
            COMMAND_REQUEST_BODY_HINT.into(),
        );
        if let Some(path) = path {
            headers.insert(HEADER_COMMAND_PATH.into(), path.into());
        }
        Arc::new(TranscriptEvent {
            event_id: EventId::new(),
            room_id: RoomId::new(),
            peer_id: PeerId::new(),
            client_id: ClientId::new(),
            kind: TranscriptKind::Message,
            occurred_at_ms: 1,
            lamport: 1,
            target,
            headers,
            body: Some(Body::Json(serde_json::json!({
                "kind": "peer", "path": path, "params": { "prompt": "a resident's whole prompt" }
            }))),
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        })
    }

    // what this catches: the incident's frame shape. A command request published to the
    // room (or a sibling room) is refused by rule (a) and owed no reply, whatever its
    // payload; one addressed to another peer is not ours; one with no command header
    // cannot be decided without decoding; a direct request here passes the frame rules.
    #[test]
    fn only_a_direct_request_to_this_node_naming_its_command_passes_the_frame_rules() {
        let me = PeerId::new();
        let path = Some("persona/placement/reserve");
        let broadcast = admit_frame(frame(MentionTarget::All, path), me).expect_err("refused");
        assert_eq!(broadcast, IngressRefusal::NotDirect);
        assert!(!broadcast.answered(), "a broadcast draws no reply");
        assert_eq!(
            admit_frame(frame(MentionTarget::Room(RoomId::new()), path), me).expect_err("refused"),
            IngressRefusal::NotDirect
        );
        assert_eq!(
            admit_frame(frame(MentionTarget::Peer(PeerId::new()), path), me).expect_err("refused"),
            IngressRefusal::AddressedElsewhere
        );
        assert_eq!(
            admit_frame(frame(MentionTarget::Peer(me), None), me).expect_err("refused"),
            IngressRefusal::MissingCommandPath
        );
        let admitted =
            admit_frame(frame(MentionTarget::Peer(me), path), me).expect("direct, here, named");
        assert_eq!(admitted.path, "persona/placement/reserve");
    }

    // what this catches: rule (b) on the incident's caller. An `untrusted` (or unenrolled,
    // or unreadable-store) caller on a direct channel is refused even the ACL's
    // Provisional verbs; `own_account` passes; a `friend` reaches the Provisional floor
    // but not an Owner-tier command; a verified grant confers its command at any tier.
    // The refusal reply names the rule and tiers and never carries params.
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
