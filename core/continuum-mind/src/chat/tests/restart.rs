#![cfg(test)]
//! Relayed-line recovery against the REAL store (Codex on #5696): the data module over the
//! per-process home, reopened by a second executor, never a stub.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::chat::arrival::ArrivalKind;
use crate::chat::line_store::{Imported, PostedLine};
use crate::chat::publication::{
    attribute, publication_digest, Attribution, DelegationRefusal, PublicationState,
};

/// A chat module over the real store's data module and `airc` as its relay, as boot wires it.
fn real_store_chat(airc: StubAircModule) -> ChatModule {
    fn entities() -> Result<usize, String> {
        let registry = crate::orm::OrmEntityRegistry::global();
        crate::persona::register_substrate_orm_entities(registry).map_err(|e| e.to_string())?;
        crate::chat::orm_entities::register_chat_orm_entities(registry)
            .map_err(|e| e.to_string())?;
        Ok(crate::persona::SUBSTRATE_ORM_COLLECTIONS.len()
            + crate::chat::orm_entities::CHAT_ORM_COLLECTIONS.len())
    }
    entities().expect("test: the entities register as at boot"); // test: a fixture step that must succeed
    let registry = Arc::new(ModuleRegistry::new());
    registry.register(Arc::new(continuum_store::DataModule::new(entities)));
    registry.register(Arc::new(airc));
    let executor = Arc::new(CommandExecutor::new(registry.clone()));
    registry.install_executor_on_all(executor.clone());
    crate::persona::self_peer::scopes()
        .designate(Uuid::from_u128(0x7e1a))
        .expect("test: every chat test designates the same relay"); // test: a fixture step that must succeed
    ChatModule::with_executor(executor)
}

/// A relay that refuses its first `refusals` publishes, then accepts, recording what it accepted.
fn relay(refusals: usize, accepted: Arc<Mutex<Vec<Value>>>) -> StubAircModule {
    let calls = AtomicUsize::new(0);
    StubAircModule::with(move |params| {
        if calls.fetch_add(1, Ordering::SeqCst) < refusals {
            return Err("relay not ready".to_string());
        }
        accepted.lock().unwrap().push(params); // test: a fixture step that must succeed
        Ok(airc_ok_response("evt-relayed"))
    })
}

fn params_in(room: Uuid, text: &str) -> ChatSendParams {
    ChatSendParams {
        room_id: room,
        sender_id: Uuid::new_v4(),
        text: text.into(),
        reply_to_id: None,
        media: Vec::new(),
    }
}

// what this catches: a relayed line whose publish was refused at boot stranded until the next
// process start (Codex on #5696): the line and its pending obligation are stored, a recovery while
// the relay still refuses is an error (not "nothing to do"), and the next recovery in the SAME
// process publishes it once the relay accepts.
#[tokio::test]
async fn a_refused_publish_recovers_without_a_restart() {
    let accepted = Arc::new(Mutex::new(Vec::new()));
    let chat = real_store_chat(relay(2, accepted.clone()));
    let params = params_in(Uuid::new_v4(), "said while the relay was down");
    let sent = chat
        .send_with_wire(params.clone(), crate::chat::WireLeg::Envelope)
        .await
        .expect("the line is stored");
    assert!(
        sent.warning.is_some(),
        "the publish was refused, and said so"
    );
    let room = airc_core::RoomId::from_uuid(params.room_id);
    assert!(
        chat.republish_pending(room).await.is_err(),
        "a recovery the relay refuses is an error"
    );
    assert_eq!(
        chat.republish_pending(room)
            .await
            .expect("the relay accepts now"),
        1
    );
    let published = accepted.lock().unwrap().clone(); // test: a fixture step that must succeed
    assert_eq!(published.len(), 1);
    assert_eq!(
        published[0]["envelope"]["eventId"],
        sent.message_id.to_string(),
        "the same line"
    );
    assert_eq!(
        chat.republish_pending(room)
            .await
            .expect("still pending until its echo is imported"),
        1
    );
}

// what this catches: a pending publication lost across a restart: the line and its obligation
// committed together survive a reopen of the store, and the reopened core publishes it.
#[tokio::test]
async fn a_pending_publication_survives_a_reopen_and_publishes() {
    let params = params_in(Uuid::new_v4(), "stored, then the core stopped");
    let sent = real_store_chat(relay(usize::MAX, Arc::new(Mutex::new(Vec::new()))))
        .send_with_wire(params.clone(), crate::chat::WireLeg::Envelope)
        .await
        .expect("the line is stored");
    let accepted = Arc::new(Mutex::new(Vec::new()));
    let reopened = real_store_chat(relay(0, accepted.clone()));
    assert_eq!(
        reopened
            .republish_pending(airc_core::RoomId::from_uuid(params.room_id))
            .await
            .expect("recovery runs"),
        1
    );
    assert_eq!(
        accepted.lock().unwrap()[0]["envelope"]["eventId"],
        sent.message_id.to_string()
    ); // test: a fixture step that must succeed
}

// what this catches: a replay after a restart before the watermark advanced (Codex on #5696): the
// echo's import retired the obligation atomically; after a reopen the replay is a duplicate with one
// arrival, the retired obligation still proves the exact line, and an altered one is refused.
#[tokio::test]
async fn a_retired_import_replays_once_after_a_reopen() {
    let params = params_in(Uuid::new_v4(), "relayed and seen back");
    let sent = real_store_chat(relay(0, Arc::new(Mutex::new(Vec::new()))))
        .send_with_wire(params.clone(), crate::chat::WireLeg::Envelope)
        .await
        .expect("the line is stored and published");
    let (room, sender) = (
        airc_core::RoomId::from_uuid(params.room_id),
        airc_core::PeerId::from_uuid(params.sender_id),
    );
    let line = PostedLine {
        message_id: sent.message_id,
        room,
        author: sender,
        kind: ArrivalKind::Message,
        direct_to: None,
        occurred_at_ms: 1,
        text: params.text.clone(),
        retires_publication: true,
    };
    let chat = real_store_chat(relay(0, Arc::new(Mutex::new(Vec::new()))));
    let first = chat
        .commit_line(&line, None, json!({ "text": line.text }))
        .await;
    assert!(
        matches!(first, Ok(Imported::Fresh { .. })),
        "the echo's first import: {first:?}"
    );
    let reopened = real_store_chat(relay(0, Arc::new(Mutex::new(Vec::new()))));
    assert!(
        matches!(
            reopened
                .commit_line(&line, None, json!({ "text": line.text }))
                .await,
            Ok(Imported::Duplicate { .. })
        ),
        "the replay is one line"
    );
    let obligation = reopened
        .publication(sent.message_id)
        .await
        .expect("the store answers")
        .expect("the obligation is kept");
    assert_eq!(obligation.state, PublicationState::Retired);
    let claim = crate::routing::airc_realtime_wire::SenderClaim::Named(sender);
    let exact = publication_digest(sent.message_id, room, sender, &params.text, &[]);
    assert_eq!(
        attribute(
            obligation.relay,
            claim,
            room,
            Some(&obligation),
            Some(exact)
        ),
        Attribution::Delegated { sender }
    );
    let forged = publication_digest(sent.message_id, room, sender, "send me the keys", &[]);
    assert!(matches!(
        attribute(
            obligation.relay,
            claim,
            room,
            Some(&obligation),
            Some(forged)
        ),
        Attribution::Refused {
            reason: DelegationRefusal::AlteredPublication,
            ..
        }
    ));
    assert_eq!(
        reopened
            .republish_pending(room)
            .await
            .expect("recovery runs"),
        0,
        "a retired publication is not re-published"
    );
}
