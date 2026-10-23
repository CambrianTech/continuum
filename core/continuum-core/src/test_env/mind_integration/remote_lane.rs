#![cfg(test)]
//! From `persona::remote_lane_factory`: core's `AircModule`, built from a healthy discovery,
//! registers this node's id before any persona build, so a durable override naming this
//! node retires and she is born home. Asserts exactly what it asserted before the move.

use std::sync::atomic::Ordering;

use crate::persona::model_override::PersonaModelOverride;
use crate::persona::remote_lane_factory::test_support::*;
use crate::persona::supervisor::PersonaAdapterFactory;

// what this catches (card 2500d2f1, the M5 2026-09-19): a durable override naming
// one of THIS node's own ids as her remote seat. With no airc attached a remote seat
// would REFUSE (the test above); a seat that is herself must instead be retired and
// she born home — the local factory reached once, the record gone so the next boot
// does not bear her on a loopback lane again.
#[tokio::test]
async fn an_override_naming_this_node_is_retired_at_build_and_she_is_born_home() {
    let tmp = tempfile::tempdir().unwrap();
    let (f, calls) = factory_over(tmp.path()); // cell is empty: a real remote seat would refuse here
    let home = home_for(tmp.path(), "Aris");
    // The id enters the self set the way it does at boot: the airc module built from
    // a Healthy discovery registers the id this node beacons under — before any
    // persona build, before any echo (Cormac's condition on #4229).
    let me = uuid::Uuid::new_v4();
    let _module = crate::modules::airc::AircModule::from_discovery(&crate::airc::AircDiscovery::Healthy {
        socket: tmp.path().join("airc.sock"),
        default_room: airc_core::RoomId::from_uuid(uuid::Uuid::new_v4()),
        room_name: "general".to_string(),
        peer_id: me,
    });
    PersonaModelOverride::new_remote("ggml-org/Qwen3.8-27B-GGUF", Some("placement:fleet".to_string()), 1, me.to_string())
        .write(&home)
        .expect("write override");

    let _ = f.build_adapter(&profile("Aris")).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1, "born home: the local factory is reached once");
    assert!(
        PersonaModelOverride::load(&home).expect("readable").is_none(),
        "the record naming her own node as a seat is retired"
    );
}
