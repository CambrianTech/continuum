//! A relayed line's proof of origin. When `chat/send` publishes a line for its caller through the
//! core's relay (the envelope leg), it first records a durable PUBLICATION OBLIGATION in the same
//! batch as the message row: the message id, room, sender (bound to the authenticated caller) and the
//! digest of the exact envelope it publishes. The inbound seam attributes a line to a sender other
//! than its transport author only when the line it received matches an obligation exactly: same id,
//! room, claimed sender and envelope digest. A forged sender header, a different room, or a reused id
//! with altered text matches nothing and is refused; it is never relabeled (Codex on #5696).
//!
//! The digest is an integrity check of the publication, computed from its typed fields with no
//! serialization; routing still reads only the typed headers.

use airc_core::{PeerId, RoomId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// The digest of a published line, fed directly with its typed facts (no serialization): message id,
/// room, sender, text and each media item's content handle, each length-prefixed so no two lines
/// can collide by moving a boundary. The publisher computes it from what it sends, the seam from what
/// it received; any altered field changes it.
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
    for item in media {
        field(
            item.blob_hash
                .as_deref()
                .or(item.url.as_deref())
                .unwrap_or_default()
                .as_bytes(),
        ); // unwrap_or_default: an item with neither handle still counts, as an empty field
    }
    hash.finalize().into()
}

/// One line `chat/send` published for its caller and has not yet seen come back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "chat_publications")]
pub struct PublicationObligation {
    /// The message id (the stored row's, and the envelope's `continuum.event_id` header).
    #[entity(primary_key)]
    pub id: Uuid,
    pub room: RoomId,
    pub sender: PeerId,
    pub digest: [u8; 32],
}

/// Who a received line is from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attribution {
    /// The transport author speaks for itself (no claim, or a claim naming itself).
    Author(PeerId),
    /// The core relayed it for this sender, proven by its publication obligation.
    Delegated { sender: PeerId },
    /// A claim to speak for someone else that nothing proves: dropped, said, never relabeled.
    Refused {
        claimed: PeerId,
        reason: DelegationRefusal,
    },
}

/// Why a claim to speak for someone else was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelegationRefusal {
    /// No publication obligation for this message id (a forged claim, or a foreign relay: no
    /// scoped relay grant exists yet).
    NoOrigin,
    /// The obligation is for another room.
    RoomMismatch,
    /// The obligation names another sender.
    SenderMismatch,
    /// The envelope is not the one published (a reused id with altered content).
    AlteredPublication,
}

/// Decide a received line's attribution from its transport author, the sender its header claims, the
/// obligation its message id names (if any) and the digest of the envelope it arrived as (`None` when
/// it could not be decoded, which matches no publication). Pure.
pub fn attribute(
    transport: PeerId,
    claimed: Option<PeerId>,
    room: RoomId,
    obligation: Option<&PublicationObligation>,
    received: Option<[u8; 32]>,
) -> Attribution {
    let claimed = match claimed {
        None => return Attribution::Author(transport),
        Some(claimed) if claimed == transport => return Attribution::Author(transport),
        Some(claimed) => claimed,
    };
    let refuse = |reason| Attribution::Refused { claimed, reason };
    let Some(obligation) = obligation else {
        return refuse(DelegationRefusal::NoOrigin);
    };
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

#[cfg(test)]
mod tests {
    use super::*;

    fn obligation(room: RoomId, sender: PeerId, digest: [u8; 32]) -> PublicationObligation {
        PublicationObligation {
            id: Uuid::new_v4(),
            room,
            sender,
            digest,
        }
    }

    // what this catches: attribution decided by anything but proof of origin (Codex on #5696): no
    // claim and a self claim are the author; a forged claim to be Joel, a room mismatch, another
    // sender, and a reused id with altered content are refused; only an exact obligation delegates.
    #[test]
    fn only_an_exact_publication_delegates() {
        let (relay, joel, room) = (PeerId::new(), PeerId::new(), RoomId::new());
        let published = [7u8; 32];
        let exact = obligation(room, joel, published);
        assert_eq!(
            attribute(relay, None, room, None, Some(published)),
            Attribution::Author(relay),
            "no claim"
        );
        assert_eq!(
            attribute(relay, Some(relay), room, None, Some(published)),
            Attribution::Author(relay),
            "self"
        );
        assert_eq!(
            attribute(relay, Some(joel), room, None, Some(published)),
            Attribution::Refused {
                claimed: joel,
                reason: DelegationRefusal::NoOrigin
            },
            "a forged Joel header proves nothing"
        );
        assert_eq!(
            attribute(
                relay,
                Some(joel),
                RoomId::new(),
                Some(&exact),
                Some(published)
            ),
            Attribution::Refused {
                claimed: joel,
                reason: DelegationRefusal::RoomMismatch
            }
        );
        assert!(matches!(
            attribute(
                relay,
                Some(PeerId::new()),
                room,
                Some(&exact),
                Some(published)
            ),
            Attribution::Refused {
                reason: DelegationRefusal::SenderMismatch,
                ..
            }
        ));
        assert_eq!(
            attribute(relay, Some(joel), room, Some(&exact), Some([9u8; 32])),
            Attribution::Refused {
                claimed: joel,
                reason: DelegationRefusal::AlteredPublication
            },
            "a reused id with changed content"
        );
        assert_eq!(
            attribute(relay, Some(joel), room, Some(&exact), Some(published)),
            Attribution::Delegated { sender: joel }
        );
    }
}

impl super::chat_data_handle::ChatModule {
    /// Record that this core is publishing this line for `params.sender_id` (bound to the
    /// authenticated caller by `chat/send`), before it is published.
    pub(super) async fn record_publication(
        &self,
        message_id: Uuid,
        params: &super::types::ChatSendParams,
    ) -> Result<(), String> {
        let (room, sender) = (
            RoomId::from_uuid(params.room_id),
            PeerId::from_uuid(params.sender_id),
        );
        let obligation = PublicationObligation {
            id: message_id,
            room,
            sender,
            digest: publication_digest(message_id, room, sender, &params.text, &params.media),
        };
        let data = serde_json::to_value(&obligation).map_err(|e| format!("publication: {e}"))?; // storage boundary: the adapter's row format
        let params = serde_json::json!({
            "dbPath": super::chat_data_handle::CHAT_DATA_HANDLE,
            "collection": <PublicationObligation as crate::orm::OrmEntity>::COLLECTION,
            "id": message_id.to_string(),
            "data": data,
        });
        let result = self
            .executor()?
            .execute_json("data/create", params)
            .await
            .map_err(|e| format!("publication: data/create failed: {e}"))?;
        match result.get("success").and_then(serde_json::Value::as_bool) {
            Some(true) => Ok(()),
            _ => Err(format!(
                "publication: the store refused the obligation: {result}"
            )),
        }
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
        row.map(Some).map_err(|e| format!("publication: malformed row: {e}"))
    }
}
