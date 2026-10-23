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
