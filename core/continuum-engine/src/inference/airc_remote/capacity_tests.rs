//! Remote capacity notifications are fanout, not generation requests.
#![cfg(test)]
use super::adapter::AircRemoteInferenceAdapter;
use super::transport::StubInferenceTransport;
use crate::ai::adapter::AIProviderAdapter;
use crate::capacity::gossip::{global_ledger, CapacityOffer};

#[tokio::test]
async fn independent_remote_waiters_ignore_clock_refresh_and_observe_growth() {
    let peer = uuid::Uuid::new_v4();
    let now = continuum_base::time::EpochMs::now().millis();
    let mut offer: CapacityOffer = serde_json::from_value(serde_json::json!({
        "gpuTotalBytes": 1, "gpuFreeBytesLive": 0, "systemRamFreeBytes": 0,
        "atMs": now, "servedModel": "capacity-witness-test",
        "servedContextWindow": 4096, "capabilities": ["served_window_stamp"]
    }))
    .unwrap();
    let ledger = global_ledger();
    ledger.hear(peer, offer.clone(), now);
    let make = || {
        AircRemoteInferenceAdapter::new(StubInferenceTransport::new(|_| {
            panic!("capacity wake must not dispatch inference")
        }))
        .with_target_peer(peer.to_string())
        .with_model("capacity-witness-test")
    };
    let one = make();
    let two = make();
    assert_eq!(one.live_served_window(), Some(4096));
    // Same payload, newer clock: no new capacity, even across independent adapters.
    let first = one.capacity_changed(Some(4096));
    let second = two.capacity_changed(Some(4096));
    tokio::pin!(first, second);
    offer.at_ms += 1;
    ledger.hear(peer, offer.clone(), now);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), &mut first)
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), &mut second)
            .await
            .is_err()
    );
    offer.served_context_window = 8192;
    ledger.hear(peer, offer, now);
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        tokio::join!(first, second);
    })
    .await
    .expect("both adapters observe the same published growth");
    assert_eq!(one.live_served_window(), Some(8192));
    assert_eq!(two.live_served_window(), Some(8192));
}

// what this catches (card 03660a77, Codex review of 634f6d338): a mind the seat refused as
// overloaded, with no home lane to fall to, must not ask the full seat again. Her later
// acts make zero transport calls until a beacon heard AFTER the refusal offers a free
// slot; a beacon with no free slot does not reopen it.
#[tokio::test]
async fn a_lane_refused_as_overloaded_does_not_ask_again_until_the_seat_offers_a_slot() {
    use super::protocol::RemoteInferenceError;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let peer = uuid::Uuid::new_v4();
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let counted = std::sync::Arc::clone(&calls);
    let lane = AircRemoteInferenceAdapter::new(StubInferenceTransport::new(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        Err(RemoteInferenceError::SeatOverloaded {
            message: "2 generations outstanding for 2 served lanes".to_string(),
        })
    }))
    .with_target_peer(peer.to_string());
    let ask = || lane.generate_text(crate::ai::types::TextGenerationRequest::default());
    assert!(ask().await.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1, "the seat refused once");
    assert!(ask().await.is_err());
    assert!(ask().await.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1, "held: no round trip to a full seat");

    let later = continuum_base::time::EpochMs::now().millis() + 1_000;
    let beacon = |free: u32, at_ms: u64| -> CapacityOffer {
        serde_json::from_value(serde_json::json!({
            "gpuTotalBytes": 1, "gpuFreeBytesLive": 0, "systemRamFreeBytes": 0,
            "atMs": at_ms, "freeSlotsLive": free,
        }))
        .expect("decodes") // boundary: the wire shape of a seat's beacon
    };
    global_ledger().hear(peer, beacon(0, later), later);
    assert!(ask().await.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1, "a beacon with no free slot does not reopen it");
    global_ledger().hear(peer, beacon(1, later + 1), later + 1);
    assert!(ask().await.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 2, "the seat offered a slot: asked again");
}
