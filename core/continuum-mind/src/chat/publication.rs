//! A relayed line's proof of origin. When `chat/send` publishes a line for its caller through the
//! core's relay (the envelope leg), it records a durable PUBLICATION OBLIGATION in the same batch as
//! the message row and its arrival: the message id, room, sender (bound to the authenticated
//! caller), the relay it publishes as, and the digest of the exact line. (The fallback after a failed
//! sender publish records it alone, still before publishing: [`ChatModule::record_publication`].)
//!
//! The inbound seam attributes a line to a sender other than its transport author only when the line
//! matches an obligation exactly: the same id, from the same relay, in the same room, for the same
//! sender, with the same digest. A forged sender header, a malformed one, another peer replaying the
//! publication, another room, or a reused id with altered content matches nothing and is refused;
//! it is never relabeled (Codex on #5696).
//!
//! The digest is an integrity check of the publication, computed from its typed fields with no
//! serialization; routing still reads only the typed headers.

use airc_core::{PeerId, RoomId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::chat_data_handle::ChatModule;
use crate::routing::airc_realtime_wire::SenderClaim;

/// The digest of a published line, fed directly with its typed facts (no serialization): message id,
/// room, sender, text and every field of every media item a consumer sees (kind, MIME, both handles,
/// description, each with its presence), length-prefixed so no two lines collide by moving a boundary.
/// The publisher computes it from what it sends, the seam from what it received.
pub fn publication_digest(
    message: Uuid,
    room: RoomId,
    sender: PeerId,
    text: &str,
    media: &[crate::cognition::conversation::channel_items::MediaItemRequest],
) -> [u8; 32] {
    let mut hash = Sha256::new();
    let mut field = |bytes: &[u8]| {
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    };
    field(message.as_bytes());
    field(room.as_uuid().as_bytes());
    field(sender.as_uuid().as_bytes());
    field(text.as_bytes());
    field(&(media.len() as u64).to_le_bytes());
    for item in media {
        field(item.kind.as_bytes());
        // An absent field and an empty one hash differently, so neither can stand in for the other.
        for optional in [
            &item.mime_type,
            &item.blob_hash,
            &item.url,
            &item.description,
        ] {
            match optional {
                Some(value) => {
                    field(&[1]);
                    field(value.as_bytes());
                }
                None => field(&[0]),
            }
        }
    }
    hash.finalize().into()
}

/// Where a publication stands: published (or about to be) and not yet seen back, or seen back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationState {
    /// Recorded before publishing; a restart re-publishes it until its echo is imported.
    Pending,
    /// Its echo was imported. Kept, so an exact replay of it after a restart still attributes.
    Retired,
}

/// One line `chat/send` publishes for its caller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "chat_publications")]
pub struct PublicationObligation {
    /// The message id (the stored row's, and the envelope's `continuum.event_id` header).
    #[entity(primary_key)]
    pub id: Uuid,
    pub room: RoomId,
    pub sender: PeerId,
    /// The authenticated transport this core publishes relayed lines as: only it may deliver this line.
    pub relay: PeerId,
    pub digest: [u8; 32],
    pub state: PublicationState,
}

/// The transport identity this core publishes relayed lines as (the airc module's scope peer),
/// registered once at its construction.
static RELAY: std::sync::OnceLock<PeerId> = std::sync::OnceLock::new();

pub fn register_relay(peer: PeerId) {
    let _ = RELAY.set(peer); // first registration wins: the airc module constructs once per process
}

pub fn relay() -> Option<PeerId> {
    RELAY.get().copied()
}

/// Who a received line is from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attribution {
    /// The transport author speaks for itself (no claim, or a claim naming itself).
    Author(PeerId),
    /// The core relayed it for this sender, proven by its publication obligation.
    Delegated { sender: PeerId },
    /// A claim nothing proves: dropped, said, never relabeled. `claimed` is `None` for a malformed claim.
    Refused {
        claimed: Option<PeerId>,
        reason: DelegationRefusal,
    },
}

/// Why a claim to speak for someone else was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelegationRefusal {
    /// The sender header is present but not an id.
    MalformedClaim,
    /// No publication obligation for this message id.
    NoOrigin,
    /// It arrived from a transport other than the relay the obligation names (a replay by another
    /// peer, or a foreign relay: no scoped relay grant exists yet).
    ForeignRelay,
    /// The obligation is for another room.
    RoomMismatch,
    /// The obligation names another sender.
    SenderMismatch,
    /// The line is not the one published (a reused id with altered content).
    AlteredPublication,
}

/// Decide a received line's attribution from its transport author, its header's sender claim, the
/// obligation its message id names (if any, pending or retired: a retired one is an exact replay) and the digest of the line as received (`None` when it
/// could not be decoded, which matches no publication). Pure.
pub fn attribute(
    transport: PeerId,
    claim: SenderClaim,
    room: RoomId,
    obligation: Option<&PublicationObligation>,
    received: Option<[u8; 32]>,
) -> Attribution {
    let claimed = match claim {
        SenderClaim::Absent => return Attribution::Author(transport),
        SenderClaim::Named(claimed) if claimed == transport => {
            return Attribution::Author(transport)
        }
        SenderClaim::Named(claimed) => claimed,
        SenderClaim::Malformed => {
            return Attribution::Refused {
                claimed: None,
                reason: DelegationRefusal::MalformedClaim,
            }
        }
    };
    let refuse = |reason| Attribution::Refused {
        claimed: Some(claimed),
        reason,
    };
    let Some(obligation) = obligation else {
        return refuse(DelegationRefusal::NoOrigin);
    };
    if obligation.relay != transport {
        return refuse(DelegationRefusal::ForeignRelay);
    }
    if obligation.room != room {
        return refuse(DelegationRefusal::RoomMismatch);
    }
    if obligation.sender != claimed {
        return refuse(DelegationRefusal::SenderMismatch);
    }
    if Some(obligation.digest) != received {
        return refuse(DelegationRefusal::AlteredPublication);
    }
    Attribution::Delegated { sender: claimed }
}

impl ChatModule {
    /// The obligation for a line this core is about to publish for `params.sender_id` (bound to the
    /// authenticated caller by `chat/send`), as the batch operation that records it.
    pub(super) fn publication_op(
        message_id: Uuid,
        params: &super::types::ChatSendParams,
    ) -> Result<crate::orm::types::BatchOperation, String> {
        let relay = relay().ok_or(
            "chat/send: this core has no relay identity (the airc module is not attached), so it cannot publish for a caller",
        )?;
        let (room, sender) = (
            RoomId::from_uuid(params.room_id),
            PeerId::from_uuid(params.sender_id),
        );
        let obligation = PublicationObligation {
            id: message_id,
            room,
            sender,
            relay,
            digest: publication_digest(message_id, room, sender, &params.text, &params.media),
            state: PublicationState::Pending,
        };
        let data = serde_json::to_value(&obligation).map_err(|e| format!("publication: {e}"))?; // storage boundary: the adapter's row format
        Ok(crate::orm::types::BatchOperation {
            operation_type: crate::orm::types::BatchOperationType::Create,
            collection: <PublicationObligation as crate::orm::OrmEntity>::COLLECTION.to_string(),
            id: Some(message_id.to_string()),
            data: Some(data),
        })
    }

    /// The fallback path's custody: the line is already stored (its sender's own publish failed), so
    /// the obligation is recorded on its own, still BEFORE the envelope is published. A crash between
    /// leaves an unused obligation (retired by retention), never an unproven line.
    pub async fn record_publication(
        &self,
        message_id: Uuid,
        params: &super::types::ChatSendParams,
    ) -> Result<(), String> {
        let op = Self::publication_op(message_id, params)?;
        let params = serde_json::json!({ "dbPath": super::chat_data_handle::CHAT_DATA_HANDLE, "operations": [op] });
        let result = self
            .executor()?
            .execute_json("data/batch", params)
            .await
            .map_err(|e| format!("publication: data/batch failed: {e}"))?;
        match result.get("success").and_then(serde_json::Value::as_bool) {
            Some(true) => Ok(()),
            _ => Err(format!(
                "publication: the store refused the obligation: {result}"
            )),
        }
    }

    /// Restart recovery (Codex on #5696): an obligation still pending had its line stored but may
    /// never have been published (the core stopped between). Each is published again, rebuilt
    /// from its stored row; an echo that arrives twice is one line, because the import dedupes by
    /// message id and a duplicate is never perceived again. Returns how many were re-published.
    pub async fn republish_pending(&self) -> Result<usize, String> {
        let query = serde_json::json!({
            "dbPath": super::chat_data_handle::CHAT_DATA_HANDLE,
            "collection": <PublicationObligation as crate::orm::OrmEntity>::COLLECTION,
            "filter": { "state": { "$eq": "pending" } },
        });
        let result = self
            .executor()?
            .execute_json("data/query", query)
            .await
            .map_err(|e| format!("publication: pending query failed: {e}"))?;
        let mut republished = 0;
        for row in super::chat_module::extract_records_as_data(&result) {
            let parsed = serde_json::from_value::<PublicationObligation>(row); // storage boundary: the adapter's row format
            let obligation = parsed.map_err(|e| format!("publication: malformed row: {e}"))?;
            let Some((params, at_ms)) = self.sent_line(obligation.id).await? else {
                continue; // no stored line: nothing to publish (the obligation was written with it, so this is a pruned row)
            };
            self.broadcast_envelope(obligation.id, &params, at_ms)
                .await?;
            republished += 1;
        }
        Ok(republished)
    }

    /// The parameters `chat/send` stored a line with, read back from its row, and its time.
    async fn sent_line(
        &self,
        message: Uuid,
    ) -> Result<Option<(super::types::ChatSendParams, u64)>, String> {
        let params = serde_json::json!({
            "dbPath": super::chat_data_handle::CHAT_DATA_HANDLE,
            "collection": super::types::CHAT_MESSAGES_COLLECTION,
            "id": message.to_string(),
        });
        let result = self
            .executor()?
            .execute_json("data/read", params)
            .await
            .map_err(|e| format!("publication: line read failed: {e}"))?;
        let Some(row) = result
            .get("data")
            .and_then(|d| d.get("data"))
            .filter(|_| result.get("success").and_then(serde_json::Value::as_bool) == Some(true))
        else {
            return Ok(None);
        };
        let parsed = serde_json::from_value::<StoredLine>(row.clone()); // storage boundary: the adapter's row format
        let stored = parsed.map_err(|e| format!("publication: malformed line: {e}"))?;
        let at_ms = chrono::DateTime::parse_from_rfc3339(&stored.timestamp)
            .map_err(|e| format!("publication: line time: {e}"))?
            .timestamp_millis()
            .max(0) as u64;
        Ok(Some((
            super::types::ChatSendParams {
                room_id: stored.room_id,
                sender_id: stored.sender_id,
                text: stored.content.text,
                reply_to_id: stored.reply_to_id,
                media: stored.content.media,
            },
            at_ms,
        )))
    }

    /// The obligation a received line's message id names, if any.
    pub(super) async fn publication(
        &self,
        id: Uuid,
    ) -> Result<Option<PublicationObligation>, String> {
        let params = serde_json::json!({
            "dbPath": super::chat_data_handle::CHAT_DATA_HANDLE,
            "collection": <PublicationObligation as crate::orm::OrmEntity>::COLLECTION,
            "id": id.to_string(),
        });
        let result = self
            .executor()?
            .execute_json("data/read", params)
            .await
            .map_err(|e| format!("publication: data/read failed: {e}"))?;
        if result.get("success").and_then(serde_json::Value::as_bool) != Some(true) {
            return Ok(None);
        }
        let Some(data) = result.get("data").and_then(|d| d.get("data")).cloned() else {
            return Ok(None);
        };
        let row = serde_json::from_value(data); // storage boundary: the adapter's row format
        row.map(Some)
            .map_err(|e| format!("publication: malformed row: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::conversation::channel_items::MediaItemRequest;

    fn obligation(
        relay: PeerId,
        room: RoomId,
        sender: PeerId,
        digest: [u8; 32],
    ) -> PublicationObligation {
        PublicationObligation {
            id: Uuid::new_v4(),
            room,
            sender,
            relay,
            digest,
            state: PublicationState::Pending,
        }
    }

    // what this catches: attribution decided by anything but proof of origin (Codex on #5696): no
    // claim and a self claim are the author; a malformed claim, a forged claim to be Joel, a replay by
    // another peer, a room mismatch, another sender and altered content are refused; only an exact
    // obligation from its relay delegates.
    #[test]
    fn only_an_exact_publication_from_its_relay_delegates() {
        let (relay, other, joel, room) =
            (PeerId::new(), PeerId::new(), PeerId::new(), RoomId::new());
        let published = Some([7u8; 32]);
        let exact = obligation(relay, room, joel, [7u8; 32]);
        let named = SenderClaim::Named(joel);
        assert_eq!(
            attribute(relay, SenderClaim::Absent, room, None, published),
            Attribution::Author(relay),
            "no claim"
        );
        assert_eq!(
            attribute(relay, SenderClaim::Named(relay), room, None, published),
            Attribution::Author(relay),
            "self"
        );
        assert_eq!(
            attribute(relay, SenderClaim::Malformed, room, Some(&exact), published),
            Attribution::Refused {
                claimed: None,
                reason: DelegationRefusal::MalformedClaim
            },
            "a present but invalid header is refused, never read as absent"
        );
        let refused =
            |transport, claim, room, obligation: Option<&PublicationObligation>, received| {
                match attribute(transport, claim, room, obligation, received) {
                    Attribution::Refused { reason, .. } => Some(reason),
                    _ => None,
                }
            };
        assert_eq!(
            refused(relay, named, room, None, published),
            Some(DelegationRefusal::NoOrigin),
            "a forged Joel header"
        );
        assert_eq!(
            refused(other, named, room, Some(&exact), published),
            Some(DelegationRefusal::ForeignRelay),
            "another peer replaying the exact publication"
        );
        assert_eq!(
            refused(relay, named, RoomId::new(), Some(&exact), published),
            Some(DelegationRefusal::RoomMismatch)
        );
        assert_eq!(
            refused(
                relay,
                SenderClaim::Named(PeerId::new()),
                room,
                Some(&exact),
                published
            ),
            Some(DelegationRefusal::SenderMismatch)
        );
        assert_eq!(
            refused(relay, named, room, Some(&exact), Some([9u8; 32])),
            Some(DelegationRefusal::AlteredPublication)
        );
        assert_eq!(
            refused(relay, named, room, Some(&exact), None),
            Some(DelegationRefusal::AlteredPublication),
            "an undecodable line"
        );
        assert_eq!(
            attribute(relay, named, room, Some(&exact), published),
            Attribution::Delegated { sender: joel }
        );
        // what this catches: a genuine line replayed after its echo was imported (a restart before
        // the watermark advanced) refused as having no origin (Codex on #5696): a retired
        // obligation still proves its exact line, while altered content is still refused.
        let retired = PublicationObligation {
            state: PublicationState::Retired,
            ..exact.clone()
        };
        assert_eq!(
            attribute(relay, named, room, Some(&retired), published),
            Attribution::Delegated { sender: joel }
        );
        assert_eq!(
            refused(relay, named, room, Some(&retired), Some([9u8; 32])),
            Some(DelegationRefusal::AlteredPublication)
        );
    }

    // what this catches: a digest that ignores a field a consumer sees, so altered media still
    // matches the publication (Codex on #5696): every media field, and an absent field versus an empty
    // one, changes the digest.
    #[test]
    fn every_consumer_visible_media_field_binds_the_digest() {
        let (message, room, sender) = (Uuid::new_v4(), RoomId::new(), PeerId::new());
        let base = MediaItemRequest {
            kind: "image".into(),
            mime_type: Some("image/png".into()),
            blob_hash: Some("sha256:aa".into()),
            url: Some("https://a".into()),
            description: Some("a cat".into()),
        };
        let digest = |item: &MediaItemRequest| {
            publication_digest(message, room, sender, "look", std::slice::from_ref(item))
        };
        let original = digest(&base);
        let mutations: [fn(&mut MediaItemRequest); 6] = [
            |m| m.kind = "audio".into(),
            |m| m.mime_type = Some("image/jpeg".into()),
            |m| m.url = Some("https://b".into()),
            |m| m.description = Some("a dog".into()),
            |m| m.description = None,
            |m| m.url = Some(String::new()),
        ];
        for (i, mutate) in mutations.iter().enumerate() {
            let mut altered = base.clone();
            mutate(&mut altered);
            assert_ne!(
                digest(&altered),
                original,
                "mutation {i} must change the digest"
            );
        }
        assert_ne!(
            publication_digest(message, room, sender, "look", &[]),
            original,
            "dropping the media changes it"
        );
        assert_ne!(
            publication_digest(message, room, sender, "lookx", std::slice::from_ref(&base)),
            original
        );
    }
}

/// The stored chat row's fields a re-publish needs, typed.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredLine {
    room_id: Uuid,
    sender_id: Uuid,
    timestamp: String,
    content: StoredContent,
    #[serde(default)]
    reply_to_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct StoredContent {
    text: String,
    #[serde(default)]
    media: Vec<crate::cognition::conversation::channel_items::MediaItemRequest>,
}
